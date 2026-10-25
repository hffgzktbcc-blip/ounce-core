#![allow(dead_code)]

use super::decoder::decode;
use super::instructions::{BranchRegOp, Condition, Instruction, MoveWideOp};
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
                | Instruction::BranchReg { .. }
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

        for &(pc, inst) in &self.insns {
            cpu.regs.pc = pc;
            executed += 1;

            match inst {
                Instruction::AddSubImm { is_64, is_sub, set_flags, rd, rn, imm } => {
                    let op1 = if is_64 { cpu.regs.read_x(rn) } else { cpu.regs.read_w(rn) as u64 };
                    let (res, c, v) = if is_sub {
                        let (res, c) = op1.overflowing_sub(imm);
                        let v = ((op1 as i64) ^ (imm as i64)) < 0 && ((op1 as i64) ^ (res as i64)) < 0;
                        (res, !c, v)
                    } else {
                        let (res, c) = op1.overflowing_add(imm);
                        let v = !(((op1 as i64) ^ (imm as i64)) < 0) && ((op1 as i64) ^ (res as i64)) < 0;
                        (res, c, v)
                    };
                    if rd != 31 || set_flags {
                        if is_64 { cpu.regs.write_x(rd, res); } else { cpu.regs.write_w(rd, res as u32); }
                    }
                    if set_flags {
                        if is_64 { set_nzcv_64(&mut cpu.regs.pstate, res, c, v); } else { set_nzcv_32(&mut cpu.regs.pstate, res as u32, c, v); }
                    }
                }

                Instruction::AddSubReg { is_64, is_sub, set_flags, rd, rn, rm, .. } => {
                    let op1 = if is_64 { cpu.regs.read_x(rn) } else { cpu.regs.read_w(rn) as u64 };
                    let op2 = if is_64 { cpu.regs.read_x(rm) } else { cpu.regs.read_w(rm) as u64 };
                    let (res, c, v) = if is_sub {
                        let (res, c) = op1.overflowing_sub(op2);
                        let v = ((op1 as i64) ^ (op2 as i64)) < 0 && ((op1 as i64) ^ (res as i64)) < 0;
                        (res, !c, v)
                    } else {
                        let (res, c) = op1.overflowing_add(op2);
                        let v = !(((op1 as i64) ^ (op2 as i64)) < 0) && ((op1 as i64) ^ (res as i64)) < 0;
                        (res, c, v)
                    };
                    if rd != 31 || set_flags {
                        if is_64 { cpu.regs.write_x(rd, res); } else { cpu.regs.write_w(rd, res as u32); }
                    }
                    if set_flags {
                        if is_64 { set_nzcv_64(&mut cpu.regs.pstate, res, c, v); } else { set_nzcv_32(&mut cpu.regs.pstate, res as u32, c, v); }
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

                Instruction::BranchReg { op, rn } => {
                    let target = cpu.regs.read_x(rn);
                    cpu.regs.pc = target;
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
}

impl Tier2JitEngine {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::with_capacity(1024),
            total_block_entries: 0,
            total_instructions_executed: 0,
            chained_jumps: 0,
        }
    }

    /// Fetches an existing compiled block or JIT-compiles it on the fly.
    pub fn get_or_compile_block(&mut self, pc: u64, mmu: &Memory) -> Option<Arc<Tier2Block>> {
        if let Some(block) = self.blocks.get(&pc) {
            return Some(Arc::clone(block));
        }

        if let Some(new_block) = Tier2Block::compile(pc, mmu) {
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
}
