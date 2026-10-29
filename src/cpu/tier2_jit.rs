#![allow(dead_code)]

use super::decoder::decode;
use super::instructions::{BranchRegOp, Condition, Instruction, MoveWideOp, ShiftType};
use super::interpreter::Cpu;
use super::registers::Pstate;
use crate::mmu::Memory;
use std::collections::HashMap;
use std::sync::Arc;

#[inline(always)]
fn set_nzcv_64(pstate: &mut Pstate, res: u64, c: bool, v: bool) {
    pstate.n = (res as i64) < 0;
    pstate.z = res == 0;
    pstate.c = c;
    pstate.v = v;
}

#[inline(always)]
fn set_nzcv_32(pstate: &mut Pstate, res: u32, c: bool, v: bool) {
    pstate.n = (res as i32) < 0;
    pstate.z = res == 0;
    pstate.c = c;
    pstate.v = v;
}

#[inline(always)]
fn eval_cond(pstate: &Pstate, cond: Condition) -> bool {
    match cond {
        Condition::Eq => pstate.z,
        Condition::Ne => !pstate.z,
        Condition::Cs => pstate.c,
        Condition::Cc => !pstate.c,
        Condition::Mi => pstate.n,
        Condition::Pl => !pstate.n,
        Condition::Vs => pstate.v,
        Condition::Vc => !pstate.v,
        Condition::Hi => pstate.c && !pstate.z,
        Condition::Ls => !pstate.c || pstate.z,
        Condition::Ge => pstate.n == pstate.v,
        Condition::Lt => pstate.n != pstate.v,
        Condition::Gt => !pstate.z && (pstate.n == pstate.v),
        Condition::Le => pstate.z || (pstate.n != pstate.v),
        Condition::Al | Condition::Nv => true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockExit {
    /// Direct branch or unconditional jump to `target`
    Branch { target: u64 },
    /// Conditional branch taken to `target`
    BranchCondTaken { target: u64 },
    /// Conditional branch fell through to `next_pc`
    BranchFallthrough { next_pc: u64 },
    /// Supervisor call requested (`SVC #imm`)
    Svc { svc_id: u16 },
    /// Subroutine return (`RET` to `target`)
    Return { target: u64 },
    /// Process completed instruction slice budget
    BudgetExceeded,
    /// Unimplemented instruction or fault
    Fault { pc: u64 },
}

/// A contiguous, pre-compiled basic block of guest AArch64 instructions executed in an
/// unrolled fast loop without per-instruction hash lookups or dispatch overhead.
#[derive(Debug, Clone)]
pub struct Tier2Block {
    pub start_pc: u64,
    pub end_pc: u64,
    pub insns: Vec<(u64, Instruction)>,
    pub static_target_pc: Option<u64>,
}

impl Tier2Block {
    /// Compiles a basic block starting at `pc` from guest memory up to the first block terminator.
    pub fn compile(start_pc: u64, mmu: &Memory) -> Option<Self> {
        let mut cur_pc = start_pc;
        let mut insns = Vec::with_capacity(32);
        let mut static_target = None;

        for _ in 0..128 {
            let Ok(raw) = mmu.read_u32(cur_pc) else {
                break;
            };
            let inst = decode(raw);
            insns.push((cur_pc, inst));

            match inst {
                Instruction::BranchImm { offset, link } => {
                    let target = if link {
                        cur_pc.wrapping_add(4)
                    } else {
                        (cur_pc as i64).wrapping_add(offset) as u64
                    };
                    static_target = Some(target);
                    cur_pc = cur_pc.wrapping_add(4);
                    break;
                }
                Instruction::BranchCond { .. }
                | Instruction::CompareBranch { .. }
                | Instruction::TestBranch { .. }
                | Instruction::BranchReg { .. }
                | Instruction::BranchRegAuth { .. }
                | Instruction::Svc { .. }
                | Instruction::Unimplemented { .. } => {
                    cur_pc = cur_pc.wrapping_add(4);
                    break;
                }
                _ => {
                    cur_pc = cur_pc.wrapping_add(4);
                }
            }
        }

        if insns.is_empty() {
            None
        } else {
            Some(Self {
                start_pc,
                end_pc: cur_pc,
                insns,
                static_target_pc: static_target,
            })
        }
    }

    /// Fast execution path: executes all instructions within this block sequentially.
    #[inline(always)]
    pub fn execute(&self, cpu: &mut Cpu, mmu: &mut Memory) -> (usize, BlockExit) {
        let mut executed = 0usize;
        let code_gen = mmu.code_write_gen();

        // An indirect branch into a guarded page left a BTI check for this block's first insn.
        if cpu.regs.pstate.btype != 0 {
            cpu.regs.pc = self.start_pc;
            if cpu.check_landing_pad(mmu).is_err() {
                return (0, BlockExit::Fault { pc: self.start_pc });
            }
        }

        for &(pc, inst) in &self.insns {
            cpu.regs.pc = pc;
            executed += 1;

            match inst {
                // ADD/SUB immediate treats register 31 as SP (except the flags destination),
                // unlike read_x/write_x, which model XZR. Keep those forms on the reference path.
                Instruction::AddSubImm { rn: 31, .. } | Instruction::AddSubImm { rd: 31, .. } => {
                    if cpu.step(mmu).is_err() {
                        return (executed, BlockExit::Fault { pc });
                    }
                }

                Instruction::AddSubImm { is_64, is_sub, set_flags, rd, rn, imm } => {
                    if is_64 {
                        let op1 = cpu.regs.read_x(rn);
                        let (res, c, v) = if is_sub {
                            let (res, borrow) = op1.overflowing_sub(imm);
                            let v = ((op1 ^ imm) & (op1 ^ res) & (1 << 63)) != 0;
                            (res, !borrow, v)
                        } else {
                            let (res, carry) = op1.overflowing_add(imm);
                            let v = (!((op1 ^ imm)) & (op1 ^ res) & (1 << 63)) != 0;
                            (res, carry, v)
                        };
                        if rd != 31 || set_flags {
                            cpu.regs.write_x(rd, res);
                        }
                        if set_flags {
                            set_nzcv_64(&mut cpu.regs.pstate, res, c, v);
                        }
                    } else {
                        let op1_32 = cpu.regs.read_w(rn);
                        let imm_32 = imm as u32;
                        let (res, c, v) = if is_sub {
                            let (res, borrow) = op1_32.overflowing_sub(imm_32);
                            let v = ((op1_32 ^ imm_32) & (op1_32 ^ res) & (1 << 31)) != 0;
                            (res, !borrow, v)
                        } else {
                            let (res, carry) = op1_32.overflowing_add(imm_32);
                            let v = (!((op1_32 ^ imm_32)) & (op1_32 ^ res) & (1 << 31)) != 0;
                            (res, carry, v)
                        };
                        if rd != 31 || set_flags {
                            cpu.regs.write_w(rd, res);
                        }
                        if set_flags {
                            set_nzcv_32(&mut cpu.regs.pstate, res, c, v);
                        }
                    }
                }

                Instruction::AddSubReg { is_extended: true, .. } => {
                    // Fall back to interpreter for extended-register variants
                    let _ = cpu.step(mmu);
                    if mmu.code_write_gen() != code_gen {
                        return (executed, BlockExit::BranchFallthrough { next_pc: cpu.regs.pc });
                    }
                }

                Instruction::AddSubReg {
                    is_64,
                    is_sub,
                    set_flags,
                    rd,
                    rn,
                    rm,
                    shift_type,
                    shift_amount,
                    is_extended: false,
                    ..
                } => {
                    let raw_rm = cpu.regs.read_x(rm);
                    let op2 = if shift_amount == 0 {
                        if is_64 { raw_rm } else { (raw_rm as u32) as u64 }
                    } else if is_64 {
                        let sh = (shift_amount & 63) as u32;
                        match shift_type {
                            ShiftType::Lsl => raw_rm.wrapping_shl(sh),
                            ShiftType::Lsr => raw_rm.wrapping_shr(sh),
                            ShiftType::Asr => ((raw_rm as i64).wrapping_shr(sh)) as u64,
                            ShiftType::Ror => raw_rm.rotate_right(sh),
                        }
                    } else {
                        let sh = (shift_amount & 31) as u32;
                        let rm32 = raw_rm as u32;
                        let shifted32 = match shift_type {
                            ShiftType::Lsl => rm32.wrapping_shl(sh),
                            ShiftType::Lsr => rm32.wrapping_shr(sh),
                            ShiftType::Asr => ((rm32 as i32).wrapping_shr(sh)) as u32,
                            ShiftType::Ror => rm32.rotate_right(sh),
                        };
                        shifted32 as u64
                    };

                    if is_64 {
                        let op1 = cpu.regs.read_x(rn);
                        let (res, c, v) = if is_sub {
                            let (res, borrow) = op1.overflowing_sub(op2);
                            let v = ((op1 ^ op2) & (op1 ^ res) & (1 << 63)) != 0;
                            (res, !borrow, v)
                        } else {
                            let (res, carry) = op1.overflowing_add(op2);
                            let v = (!((op1 ^ op2)) & (op1 ^ res) & (1 << 63)) != 0;
                            (res, carry, v)
                        };
                        if rd != 31 || set_flags {
                            cpu.regs.write_x(rd, res);
                        }
                        if set_flags {
                            set_nzcv_64(&mut cpu.regs.pstate, res, c, v);
                        }
                    } else {
                        let op1_32 = cpu.regs.read_w(rn);
                        let op2_32 = op2 as u32;
                        let (res, c, v) = if is_sub {
                            let (res, borrow) = op1_32.overflowing_sub(op2_32);
                            let v = ((op1_32 ^ op2_32) & (op1_32 ^ res) & (1 << 31)) != 0;
                            (res, !borrow, v)
                        } else {
                            let (res, carry) = op1_32.overflowing_add(op2_32);
                            let v = (!((op1_32 ^ op2_32)) & (op1_32 ^ res) & (1 << 31)) != 0;
                            (res, carry, v)
                        };
                        if rd != 31 || set_flags {
                            cpu.regs.write_w(rd, res);
                        }
                        if set_flags {
                            set_nzcv_32(&mut cpu.regs.pstate, res, c, v);
                        }
                    }
                }

                Instruction::MoveWide { is_64, rd, imm, shift, op } => {
                    let val = (imm as u64) << shift;
                    match op {
                        MoveWideOp::Movz => {
                            if is_64 { cpu.regs.write_x(rd, val); } else { cpu.regs.write_w(rd, val as u32); }
                        }
                        MoveWideOp::Movk => {
                            let cur = if is_64 { cpu.regs.read_x(rd) } else { cpu.regs.read_w(rd) as u64 };
                            let mask = !((0xFFFFu64) << shift);
                            let res = (cur & mask) | val;
                            if is_64 { cpu.regs.write_x(rd, res); } else { cpu.regs.write_w(rd, res as u32); }
                        }
                        MoveWideOp::Movn => {
                            let not_val = !val;
                            if is_64 { cpu.regs.write_x(rd, not_val); } else { cpu.regs.write_w(rd, not_val as u32); }
                        }
                    }
                }

                Instruction::BranchImm { offset, link } => {
                    if link {
                        cpu.regs.write_x(30, pc.wrapping_add(4));
                    }
                    let target = (pc as i64).wrapping_add(offset) as u64;
                    cpu.regs.pc = target;
                    return (executed, BlockExit::Branch { target });
                }

                Instruction::BranchCond { cond, offset } => {
                    if eval_cond(&cpu.regs.pstate, cond) {
                        let target = (pc as i64).wrapping_add(offset) as u64;
                        cpu.regs.pc = target;
                        return (executed, BlockExit::BranchCondTaken { target });
                    } else {
                        let next = pc.wrapping_add(4);
                        cpu.regs.pc = next;
                        return (executed, BlockExit::BranchFallthrough { next_pc: next });
                    }
                }

                Instruction::CompareBranch { is_64, is_cbnz, rt, offset } => {
                    let val = if is_64 { cpu.regs.read_x(rt) } else { cpu.regs.read_w(rt) as u64 };
                    let condition_met = if is_cbnz { val != 0 } else { val == 0 };
                    if condition_met {
                        let target = (pc as i64).wrapping_add(offset) as u64;
                        cpu.regs.pc = target;
                        return (executed, BlockExit::BranchCondTaken { target });
                    } else {
                        let next = pc.wrapping_add(4);
                        cpu.regs.pc = next;
                        return (executed, BlockExit::BranchFallthrough { next_pc: next });
                    }
                }

                Instruction::TestBranch { .. } => {
                    // The reference step owns TBZ/TBNZ register-width and bit semantics.
                    // Yield immediately: the branch target must not be overwritten by
                    // the next instruction tuple in this compiled block.
                    if cpu.step(mmu).is_err() {
                        return (executed, BlockExit::Fault { pc });
                    }
                    let target = cpu.regs.pc;
                    return (executed, if target == pc.wrapping_add(4) {
                        BlockExit::BranchFallthrough { next_pc: target }
                    } else {
                        BlockExit::BranchCondTaken { target }
                    });
                }

                Instruction::BranchReg { op, .. } | Instruction::BranchRegAuth { op, .. } => {
                    // The interpreter sets X30 for BLR, authenticates, and sets PSTATE.BTYPE.
                    if cpu.step(mmu).is_err() {
                        return (executed, BlockExit::Fault { pc });
                    }
                    let target = cpu.regs.pc;
                    if op == BranchRegOp::Ret {
                        return (executed, BlockExit::Return { target });
                    } else {
                        return (executed, BlockExit::Branch { target });
                    }
                }

                Instruction::Svc { imm } => {
                    cpu.regs.pc = pc.wrapping_add(4);
                    return (executed, BlockExit::Svc { svc_id: imm });
                }

                _ => {
                    // Fall back to interpreter for complex/uncommon instructions
                    let _ = cpu.step(mmu);
                    // A store into cached code may have changed the rest of this block: stop here
                    // so the engine invalidates and recompiles before running more of it.
                    if mmu.code_write_gen() != code_gen {
                        return (executed, BlockExit::BranchFallthrough { next_pc: cpu.regs.pc });
                    }
                }
            }
        }

        (executed, BlockExit::BranchFallthrough { next_pc: self.end_pc })
    }
}

/// Tier-2 Dynamic Binary JIT Engine with Direct Block Chaining.
#[derive(Debug, Default)]
pub struct Tier2JitEngine {
    pub blocks: HashMap<u64, Arc<Tier2Block>>,
    pub total_block_entries: u64,
    pub total_instructions_executed: u64,
    pub chained_jumps: u64,
    pub blocks_invalidated: u64,
    /// `Memory::code_write_gen` as of the last invalidation sweep.
    seen_code_gen: u64,
}

impl Tier2JitEngine {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::with_capacity(1024),
            total_block_entries: 0,
            total_instructions_executed: 0,
            chained_jumps: 0,
            blocks_invalidated: 0,
            seen_code_gen: 0,
        }
    }

    /// Drops every cached block whose guest code pages were written since the last sweep.
    pub fn sync_code_writes(&mut self, mmu: &Memory) {
        let code_gen = mmu.code_write_gen();
        if code_gen == self.seen_code_gen {
            return;
        }
        let since = self.seen_code_gen;
        let before = self.blocks.len();
        self.blocks.retain(|_, b| !mmu.code_written_since(b.start_pc, b.end_pc, since));
        self.blocks_invalidated += (before - self.blocks.len()) as u64;
        self.seen_code_gen = code_gen;
    }

    /// Fetches an existing compiled block or JIT-compiles it on the fly.
    pub fn get_or_compile_block(&mut self, pc: u64, mmu: &mut Memory) -> Option<Arc<Tier2Block>> {
        self.sync_code_writes(mmu);
        if let Some(block) = self.blocks.get(&pc) {
            return Some(Arc::clone(block));
        }

        if let Some(new_block) = Tier2Block::compile(pc, mmu) {
            mmu.mark_code_range(new_block.start_pc, new_block.end_pc);
            let arc_block = Arc::new(new_block);
            self.blocks.insert(pc, Arc::clone(&arc_block));
            Some(arc_block)
        } else {
            None
        }
    }

    /// Executes compiled blocks in a fast direct-chained loop until `max_instructions` is reached
    /// or a block yields on `Svc`, `Return`, or `Fault`.
    pub fn run_slice(&mut self, cpu: &mut Cpu, mmu: &mut Memory, max_instructions: usize) -> (usize, BlockExit) {
        let mut executed = 0usize;

        while executed < max_instructions {
            let cur_pc = cpu.regs.pc;
            let Some(block) = self.get_or_compile_block(cur_pc, mmu) else {
                return (executed, BlockExit::Fault { pc: cur_pc });
            };

            self.total_block_entries += 1;
            let (insn_count, exit) = block.execute(cpu, mmu);
            executed += insn_count;
            self.total_instructions_executed += insn_count as u64;

            match exit {
                BlockExit::Branch { target } => {
                    self.chained_jumps += 1;
                    cpu.regs.pc = target;
                }
                BlockExit::BranchCondTaken { target } => {
                    self.chained_jumps += 1;
                    cpu.regs.pc = target;
                }
                BlockExit::BranchFallthrough { next_pc } => {
                    cpu.regs.pc = next_pc;
                }
                BlockExit::Svc { .. } | BlockExit::Return { .. } | BlockExit::Fault { .. } => {
                    return (executed, exit);
                }
                BlockExit::BudgetExceeded => break,
            }
        }

        (executed, BlockExit::BudgetExceeded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    #[test]
    fn test_tier2_jit_fibonacci_block_execution_and_chaining() {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        let code_va = 0x8000_0000u64;
        mmu.map_range(code_va, 4096, Permission::READ_WRITE_EXECUTE);

        // Fibonacci(10) basic blocks:
        // entry:
        //   movz x1, #10
        //   movz x2, #0
        //   movz x3, #1
        // loop:
        //   cbz  x1, done (offset +6 insns = 24 bytes)
        //   add  x4, x2, x3
        //   add  x2, x3, #0
        //   add  x3, x4, #0
        //   subs x1, x1, #1
        //   b    loop (-5 insns = -20 bytes)
        // done:
        //   add  x0, x2, #0
        //   svc  #0x07
        #[rustfmt::skip]
        let program: [u32; 11] = [
            0xD2800141, // 00: movz x1, #10
            0xD2800002, // 04: movz x2, #0
            0xD2800023, // 08: movz x3, #1
            0xB40000C1, // 0C: cbz  x1, done (+24 bytes -> offset 0x24)
            0x8B030044, // 10: add  x4, x2, x3
            0x91000062, // 14: add  x2, x3, #0
            0x91000083, // 18: add  x3, x4, #0
            0xF1000421, // 1C: subs x1, x1, #1
            0x17FFFFFB, // 20: b    loop (-20 bytes -> offset 0x0C)
            0x91000040, // 24: add  x0, x2, #0
            0xD40000E1, // 28: svc  #0x07
        ];

        for (i, &w) in program.iter().enumerate() {
            mmu.write_u32(code_va + (i * 4) as u64, w).unwrap();
        }

        let mut jit = Tier2JitEngine::new();
        cpu.regs.pc = code_va;

        let (total_insns, exit) = jit.run_slice(&mut cpu, &mut mmu, 10_000);
        assert_eq!(exit, BlockExit::Svc { svc_id: 7 });
        assert_eq!(cpu.regs.read_x(0), 55); // Fib(10) = 55!
        assert!(total_insns >= 60);
        assert!(jit.chained_jumps >= 9); // Loop chained 9 times!
    }

    fn load(mmu: &mut Memory, base: u64, words: &[(u64, u32)]) {
        mmu.map_range(base, 4096, Permission::READ_WRITE_EXECUTE);
        for &(off, w) in words {
            mmu.write_u32(base + off, w).unwrap();
        }
    }

    #[test]
    fn add_sub_immediate_stack_operands_match_interpreter() {
        let base = 0x8000_0000;
        let stack = 0x2000u64;
        let mut mmu = Memory::new();
        load(&mut mmu, base, &[
            (0x00, 0xD10083FF), // sub sp, sp, #32
            (0x04, 0x910023E1), // add x1, sp, #8
            (0x08, 0xD2800542), // mov x2, #42
            (0x0C, 0xF9000422), // str x2, [x1, #8]
            (0x10, 0xF9400420), // ldr x0, [x1, #8]
            (0x14, 0x910083FF), // add sp, sp, #32
            (0x18, 0xD40000E1), // svc #7
        ]);
        mmu.map_range(stack - 4096, 4096, Permission::READ_WRITE);
        let mut reference_mmu = Memory::new();
        reference_mmu.map_range(base, 4096, Permission::READ_WRITE_EXECUTE);
        reference_mmu.map_range(stack - 4096, 4096, Permission::READ_WRITE);
        for offset in (0..0x1Cu64).step_by(4) {
            reference_mmu.write_u32(base + offset, mmu.read_u32(base + offset).unwrap()).unwrap();
        }
        let mut reference = Cpu::new();
        reference.regs.pc = base;
        reference.regs.sp = stack;
        loop {
            if matches!(reference.step(&mut reference_mmu).unwrap(), super::super::interpreter::StepResult::Svc(7)) { break; }
        }
        let mut cpu = Cpu::new();
        cpu.regs.pc = base;
        cpu.regs.sp = stack;
        let block = Tier2Block::compile(base, &mmu).unwrap();
        let (_, exit) = block.execute(&mut cpu, &mut mmu);
        assert!(matches!(exit, BlockExit::Svc { svc_id: 7 }));
        assert_eq!(cpu.regs.sp, reference.regs.sp);
        assert_eq!(cpu.regs.read_x(0), reference.regs.read_x(0));
        assert_eq!(cpu.regs.read_x(0), 42);
        assert_eq!(cpu.regs.read_x(1), reference.regs.read_x(1));
        assert_eq!(mmu.read_u64(stack - 16).unwrap(), 42);
    }

    #[test]
    fn test_bit_branches_end_compiled_blocks_and_preserve_taken_targets() {
        let base = 0x8000_0000;
        for (word, value, taken) in [
            (0x36000040, 0, true), (0x36000040, 1, false), // TBZ bit 0
            (0x37000040, 1, true), (0x37000040, 0, false), // TBNZ bit 0
            (0xB6F80040, 0, true), (0xB6F80040, 1u64 << 63, false), // TBZ bit 63
            (0xB7F80040, 1u64 << 63, true), (0xB7F80040, 0, false),
        ] {
            let mut mmu = Memory::new();
            load(&mut mmu, base, &[(0, word), (4, 0xD2800021), (8, 0xD40000E1)]);
            let block = Tier2Block::compile(base, &mmu).unwrap();
            assert_eq!(block.insns.len(), 1);
            let mut cpu = Cpu::new();
            cpu.regs.pc = base;
            cpu.regs.write_x(0, value);
            cpu.regs.write_x(1, 42);
            let (_, exit) = block.execute(&mut cpu, &mut mmu);
            if taken {
                assert!(matches!(exit, BlockExit::BranchCondTaken { target } if target == base + 8));
                assert_eq!(cpu.regs.read_x(1), 42);
            } else {
                assert!(matches!(exit, BlockExit::BranchFallthrough { next_pc } if next_pc == base + 4));
            }
            let mut engine = super::super::tier3_jit::Tier3JitEngine::new();
            engine.hot_pc_counters.insert(base, super::super::tier3_jit::Tier3JitEngine::HOT_THRESHOLD);
            cpu.regs.pc = base;
            let (_, exit) = engine.run_slice(&mut cpu, &mut mmu, 100);
            assert!(matches!(exit, BlockExit::Svc { svc_id: 7 }));
            assert_eq!(cpu.regs.read_x(1), if taken { 42 } else { 1 });
            assert!(engine.total_super_block_executions > 0, "must exercise a hot compiled path");
        }
    }

    /// Runs Tier-2 slices until the guest exits with `SVC`, stepping over `RET` yields.
    fn run_to_svc(jit: &mut Tier2JitEngine, cpu: &mut Cpu, mmu: &mut Memory) -> BlockExit {
        for _ in 0..16 {
            let (_, exit) = jit.run_slice(cpu, mmu, 1_000);
            if !matches!(exit, BlockExit::Return { .. }) {
                return exit;
            }
        }
        panic!("guest never reached SVC");
    }

    #[test]
    fn test_tier2_smc_guest_patches_called_function() {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        let base = 0x8000_0000u64;
        #[rustfmt::skip]
        load(&mut mmu, base, &[
            (0x00, 0x94000010), // bl   f (0x40)
            (0x04, 0xB9000041), // str  w1, [x2]      ; patch f
            (0x08, 0x9400000E), // bl   f (0x40)
            (0x0C, 0xD4000001), // svc  #0
            (0x40, 0xD2800020), // f: movz x0, #1
            (0x44, 0xD65F03C0), //    ret
        ]);
        cpu.regs.write_x(1, 0xD280_0040); // movz x0, #2
        cpu.regs.write_x(2, base + 0x40);
        cpu.regs.pc = base;

        let mut jit = Tier2JitEngine::new();
        assert_eq!(run_to_svc(&mut jit, &mut cpu, &mut mmu), BlockExit::Svc { svc_id: 0 });
        assert_eq!(cpu.regs.read_x(0), 2, "second call ran the stale cached block");
        assert!(jit.blocks_invalidated >= 1);
    }

    #[test]
    fn test_tier2_smc_store_ends_block_before_patched_insn() {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        let base = 0x8000_0000u64;
        #[rustfmt::skip]
        load(&mut mmu, base, &[
            (0x00, 0xB9000041), // str  w1, [x2]      ; patch 0x08 in this same block
            (0x04, 0xD503201F), // nop
            (0x08, 0xD2800020), // movz x0, #1
            (0x0C, 0xD4000001), // svc  #0
        ]);
        cpu.regs.write_x(1, 0xD280_0040); // movz x0, #2
        cpu.regs.write_x(2, base + 0x08);
        cpu.regs.pc = base;

        let mut jit = Tier2JitEngine::new();
        let (_, exit) = jit.run_slice(&mut cpu, &mut mmu, 1_000);
        assert_eq!(exit, BlockExit::Svc { svc_id: 0 });
        assert_eq!(cpu.regs.read_x(0), 2);
    }

    #[test]
    fn test_tier2_writes_to_data_pages_keep_blocks() {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        let base = 0x8000_0000u64;
        load(&mut mmu, base, &[(0x00, 0xD2800020), (0x04, 0xD4000001)]);
        mmu.map_range(0x9000_0000, 4096, Permission::READ_WRITE);

        let mut jit = Tier2JitEngine::new();
        cpu.regs.pc = base;
        jit.run_slice(&mut cpu, &mut mmu, 100);
        mmu.write_u64(0x9000_0000, 0xDEAD).unwrap();
        cpu.regs.pc = base;
        jit.run_slice(&mut cpu, &mut mmu, 100);
        assert_eq!(mmu.code_write_gen(), 0);
        assert_eq!(jit.blocks_invalidated, 0);
        assert_eq!(jit.blocks.len(), 1);
    }

    #[test]
    fn test_tier2_addsubreg_shifts_and_flags() {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        let base = 0x8000_0000u64;

        #[rustfmt::skip]
        load(&mut mmu, base, &[
            (0x00, 0x8B020420), // add  x0, x1, x2, lsl #1
            (0x04, 0xCB420423), // sub  x3, x1, x2, lsr #1
            (0x08, 0x8B820824), // add  x4, x1, x2, asr #2
            (0x0C, 0x0B020825), // add  w5, w1, w2, lsl #2
            (0x10, 0x8B224428), // add  x8, x1, w2, uxtw #1 (extended fallback)
            (0x14, 0xD4000001), // svc  #0
        ]);

        cpu.regs.write_x(1, 3);
        cpu.regs.write_x(2, 4);
        cpu.regs.pc = base;

        let mut jit = Tier2JitEngine::new();
        let (_, exit) = jit.run_slice(&mut cpu, &mut mmu, 100);
        assert_eq!(exit, BlockExit::Svc { svc_id: 0 });

        // 1. ADD X0, X1, X2, LSL #1: 3 + (4 << 1) = 11 (fixes Issue #25)
        assert_eq!(cpu.regs.read_x(0), 11);
        // 2. SUB X3, X1, X2, LSR #1: 3 - (4 >> 1) = 1
        assert_eq!(cpu.regs.read_x(3), 1);
        // 3. ADD X4, X1, X2, ASR #2: 3 + (4 >> 2) = 4
        assert_eq!(cpu.regs.read_x(4), 4);
        // 4. ADD W5, W1, W2, LSL #2: 3 + (4 << 2) = 19
        assert_eq!(cpu.regs.read_x(5), 19);
        // 5. Extended register fallback: ADD X8, X1, W2, UXTW #1: 3 + (4 << 1) = 11
        assert_eq!(cpu.regs.read_x(8), 11);
    }

    #[test]
    fn test_tier2_addsubreg_signed_overflow_and_flags() {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        let base = 0x8000_0000u64;

        #[rustfmt::skip]
        load(&mut mmu, base, &[
            (0x00, 0xEB020026), // subs x6, x1, x2, lsl #0
            (0x04, 0x2B020027), // adds w7, w1, w2, lsl #0
            (0x08, 0xD4000001), // svc  #0
        ]);

        // SUBS X6, X1, X2: 5 - 5 = 0 -> Z=1, C=1, N=0, V=0
        cpu.regs.write_x(1, 5);
        cpu.regs.write_x(2, 5);
        cpu.regs.pc = base;

        let mut jit = Tier2JitEngine::new();
        let (_, exit) = jit.run_slice(&mut cpu, &mut mmu, 100);
        assert_eq!(exit, BlockExit::Svc { svc_id: 0 });
        assert_eq!(cpu.regs.read_x(6), 0);

        // Test 32-bit signed overflow: 0x7FFF_FFFF + 1 = 0x8000_0000 -> N=1, V=1, Z=0, C=0
        cpu.regs.write_w(1, 0x7FFF_FFFF);
        cpu.regs.write_w(2, 1);
        cpu.regs.pc = base + 4;
        let (_, exit) = jit.run_slice(&mut cpu, &mut mmu, 100);
        assert_eq!(exit, BlockExit::Svc { svc_id: 0 });
        assert_eq!(cpu.regs.read_w(7), 0x8000_0000);
        assert!(cpu.regs.pstate.n, "N flag should be set for negative 32-bit result");
        assert!(cpu.regs.pstate.v, "V flag should be set for signed overflow");
        assert!(!cpu.regs.pstate.z, "Z flag should be clear");
        assert!(!cpu.regs.pstate.c, "C flag should be clear");
    }
}
