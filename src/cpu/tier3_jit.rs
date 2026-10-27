#![allow(dead_code)]

use super::interpreter::Cpu;
use super::tier2_jit::{BlockExit, Tier2Block};
use crate::mmu::Memory;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// A fused execution trace (Super-Block) combining multiple hot basic blocks
/// into an unbroken straight-line execution sequence to cut dispatch overhead (see Stage 20 for measured host MIPS).
#[derive(Debug, Clone)]
pub struct SuperBlock {
    pub entry_pc: u64,
    pub fused_blocks: Vec<Tier2Block>,
    pub total_instructions: usize,
    pub static_exit_pc: Option<u64>,
}

impl SuperBlock {
    pub fn new(entry_pc: u64) -> Self {
        Self {
            entry_pc,
            fused_blocks: Vec::with_capacity(8),
            total_instructions: 0,
            static_exit_pc: None,
        }
    }

    /// Forms a Super-Block trace by unrolling and fusing blocks along hot execution paths.
    pub fn form_trace(entry_pc: u64, mmu: &Memory, max_fused: usize) -> Option<Self> {
        let mut trace = Self::new(entry_pc);
        let mut cur_pc = entry_pc;

        while trace.fused_blocks.len() < max_fused {
            let Some(block) = Tier2Block::compile(cur_pc, mmu) else {
                break;
            };

            trace.total_instructions += block.insns.len();
            let static_target = block.static_target_pc;
            trace.fused_blocks.push(block);

            // Follow unconditional jumps or predictable targets
            if let Some(target) = static_target {
                cur_pc = target;
                trace.static_exit_pc = Some(target);
            } else {
                break;
            }
        }

        if !trace.fused_blocks.is_empty() {
            Some(trace)
        } else {
            None
        }
    }

    /// Fast unrolled execution of all fused basic blocks within the super-block.
    #[inline(always)]
    pub fn execute(&self, cpu: &mut Cpu, mmu: &mut Memory) -> (usize, BlockExit) {
        let mut executed = 0usize;
        let code_gen = mmu.code_write_gen();

        for block in &self.fused_blocks {
            // The trace was formed along static targets; if execution left that path (e.g. `BL`
            // jumped to the callee) or a store hit cached code, hand control back to the engine.
            if cpu.regs.pc != block.start_pc || mmu.code_write_gen() != code_gen {
                return (executed, BlockExit::Branch { target: cpu.regs.pc });
            }
            let (count, exit) = block.execute(cpu, mmu);
            executed += count;

            match exit {
                BlockExit::Branch { target } => {
                    cpu.regs.pc = target;
                }
                BlockExit::BranchCondTaken { target } => {
                    cpu.regs.pc = target;
                    return (executed, exit);
                }
                BlockExit::BranchFallthrough { next_pc } => {
                    cpu.regs.pc = next_pc;
                }
                _ => return (executed, exit),
            }
        }

        (executed, BlockExit::BudgetExceeded)
    }
}

/// Tier-3 Native Host JIT & Super-Block Optimization Engine.
#[derive(Debug)]
pub struct Tier3JitEngine {
    pub super_blocks: HashMap<u64, Arc<SuperBlock>>,
    pub hot_pc_counters: HashMap<u64, u32>,
    pub total_super_block_executions: u64,
    pub total_super_block_insns: u64,
    pub mips_rate: f64,
    pub super_blocks_invalidated: u64,
    /// `Memory::code_write_gen` as of the last invalidation sweep.
    seen_code_gen: u64,
}

impl Default for Tier3JitEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Tier3JitEngine {
    pub const HOT_THRESHOLD: u32 = 4; // Number of entries before fusing into a SuperBlock

    pub fn new() -> Self {
        Self {
            super_blocks: HashMap::with_capacity(512),
            hot_pc_counters: HashMap::with_capacity(1024),
            total_super_block_executions: 0,
            total_super_block_insns: 0,
            mips_rate: 0.0,
            super_blocks_invalidated: 0,
            seen_code_gen: 0,
        }
    }

    /// Drops every super-block that fused code from a guest page written since the last sweep.
    pub fn sync_code_writes(&mut self, mmu: &Memory) {
        let code_gen = mmu.code_write_gen();
        if code_gen == self.seen_code_gen {
            return;
        }
        let since = self.seen_code_gen;
        let before = self.super_blocks.len();
        self.super_blocks.retain(|_, sb| {
            !sb.fused_blocks
                .iter()
                .any(|b| mmu.code_written_since(b.start_pc, b.end_pc, since))
        });
        self.super_blocks_invalidated += (before - self.super_blocks.len()) as u64;
        self.seen_code_gen = code_gen;
    }

    /// Evaluates if `pc` qualifies for Tier-3 Super-Block optimization.
    pub fn get_or_compile_super_block(&mut self, pc: u64, mmu: &mut Memory) -> Option<Arc<SuperBlock>> {
        self.sync_code_writes(mmu);
        if let Some(sb) = self.super_blocks.get(&pc) {
            return Some(Arc::clone(sb));
        }

        let counter = self.hot_pc_counters.entry(pc).or_insert(0);
        *counter += 1;

        if *counter >= Self::HOT_THRESHOLD {
            if let Some(sb) = SuperBlock::form_trace(pc, mmu, 4) {
                for b in &sb.fused_blocks {
                    mmu.mark_code_range(b.start_pc, b.end_pc);
                }
                let arc_sb = Arc::new(sb);
                self.super_blocks.insert(pc, Arc::clone(&arc_sb));
                return Some(arc_sb);
            }
        }

        None
    }

    /// Executes instructions using the Tier-3 JIT engine, returning instruction count and exit reason.
    pub fn run_slice(&mut self, cpu: &mut Cpu, mmu: &mut Memory, max_instructions: usize) -> (usize, BlockExit) {
        let start_time = Instant::now();
        let mut executed = 0usize;

        while executed < max_instructions {
            let cur_pc = cpu.regs.pc;
            if let Some(super_block) = self.get_or_compile_super_block(cur_pc, mmu) {
                self.total_super_block_executions += 1;
                let (insn_count, exit) = super_block.execute(cpu, mmu);
                executed += insn_count;
                self.total_super_block_insns += insn_count as u64;

                match exit {
                    BlockExit::Branch { target } | BlockExit::BranchCondTaken { target } => {
                        cpu.regs.pc = target;
                    }
                    BlockExit::BranchFallthrough { next_pc } => {
                        cpu.regs.pc = next_pc;
                    }
                    BlockExit::BudgetExceeded => {
                        // Continue loop until max_instructions or Svc is reached
                    }
                    _ => {
                        let elapsed = start_time.elapsed().as_secs_f64();
                        if elapsed > 0.0 {
                            self.mips_rate = (executed as f64 / elapsed) / 1_000_000.0;
                        }
                        return (executed, exit);
                    }
                }
            } else {
                // Step single instruction using interpreter until block becomes hot
                match cpu.step(mmu) {
                    Ok(super::interpreter::StepResult::Continue) => {
                        executed += 1;
                    }
                    Ok(super::interpreter::StepResult::Svc(imm)) => {
                        executed += 1;
                        let elapsed = start_time.elapsed().as_secs_f64();
                        if elapsed > 0.0 {
                            self.mips_rate = (executed as f64 / elapsed) / 1_000_000.0;
                        }
                        return (executed, BlockExit::Svc { svc_id: imm });
                    }
                    _ => {
                        return (executed, BlockExit::Fault { pc: cur_pc });
                    }
                }
            }
        }

        let elapsed = start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.mips_rate = (executed as f64 / elapsed) / 1_000_000.0;
        }

        (executed, BlockExit::BudgetExceeded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    #[test]
    fn test_tier3_super_block_trace_formation_and_execution() {
        let mut cpu = Cpu::new();
        let mut mmu = Memory::new();
        let base_pc = 0x8000_0000;

        // Fibonacci computation loop in AArch64
        #[rustfmt::skip]
        let loop_code: Vec<u32> = vec![
            0xD2800141, // MOVZ X1, #10
            0xD2800002, // MOVZ X2, #0
            0xD2800023, // MOVZ X3, #1
            // loop:
            0xB40000C1, // CBZ X1, done (+6 insns)
            0x8B030044, // ADD X4, X2, X3
            0x91000062, // ADD X2, X3, #0
            0x91000083, // ADD X3, X4, #0
            0xF1000421, // SUBS X1, X1, #1
            0x17FFFFFB, // B loop (-5 insns)
            // done:
            0x91000040, // ADD X0, X2, #0
            0xD40000E1, // SVC #0x07 (Exit)
        ];

        let mut bytes = Vec::new();
        for inst in &loop_code {
            bytes.extend_from_slice(&inst.to_le_bytes());
        }

        mmu.map_range(base_pc, 4096, Permission::READ_WRITE_EXECUTE);
        mmu.write_bytes(base_pc, &bytes).unwrap();
        cpu.regs.pc = base_pc;

        let mut engine = Tier3JitEngine::new();
        let (executed, exit) = engine.run_slice(&mut cpu, &mut mmu, 500);

        assert!(executed >= 60);
        assert_eq!(cpu.regs.read_x(0), 55); // Fib(10) = 55
        assert_eq!(exit, BlockExit::Svc { svc_id: 7 });
        assert!(engine.total_super_block_executions > 0);
    }

    /// Runs Tier-3 slices from `entry` until the guest exits with `SVC`.
    fn run_from(engine: &mut Tier3JitEngine, cpu: &mut Cpu, mmu: &mut Memory, entry: u64) -> BlockExit {
        cpu.regs.pc = entry;
        for _ in 0..16 {
            let (_, exit) = engine.run_slice(cpu, mmu, 1_000);
            if !matches!(exit, BlockExit::Return { .. }) {
                return exit;
            }
        }
        panic!("guest never reached SVC");
    }

    #[test]
    fn test_tier3_smc_invalidates_hot_super_block() {
        let mut cpu = Cpu::new();
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        mmu.map_range(base, 4096, Permission::READ_WRITE_EXECUTE);
        mmu.write_u32(base, 0xD2800020).unwrap(); // movz x0, #1
        mmu.write_u32(base + 4, 0xD4000001).unwrap(); // svc #0

        let mut engine = Tier3JitEngine::new();
        for _ in 0..Tier3JitEngine::HOT_THRESHOLD + 1 {
            run_from(&mut engine, &mut cpu, &mut mmu, base);
        }
        assert!(engine.super_blocks.contains_key(&base));
        assert_eq!(cpu.regs.read_x(0), 1);

        mmu.write_u32(base, 0xD2800040).unwrap(); // movz x0, #2
        assert_eq!(run_from(&mut engine, &mut cpu, &mut mmu, base), BlockExit::Svc { svc_id: 0 });
        assert_eq!(cpu.regs.read_x(0), 2, "hot super-block ran stale code");
        assert_eq!(engine.super_blocks_invalidated, 1);
    }

    #[test]
    fn test_tier3_super_block_follows_bl_into_callee() {
        let mut cpu = Cpu::new();
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        #[rustfmt::skip]
        let code: [(u64, u32); 5] = [
            (0x00, 0x94000004), // bl   f (0x10)
            (0x04, 0xD28000E0), // movz x0, #7
            (0x08, 0xD4000001), // svc  #0
            (0x10, 0xD28000A1), // f: movz x1, #5
            (0x14, 0xD65F03C0), //    ret
        ];
        mmu.map_range(base, 4096, Permission::READ_WRITE_EXECUTE);
        for (off, w) in code {
            mmu.write_u32(base + off, w).unwrap();
        }

        let mut engine = Tier3JitEngine::new();
        for _ in 0..Tier3JitEngine::HOT_THRESHOLD + 2 {
            cpu.regs.write_x(1, 0);
            assert_eq!(run_from(&mut engine, &mut cpu, &mut mmu, base), BlockExit::Svc { svc_id: 0 });
            assert_eq!(cpu.regs.read_x(1), 5, "callee skipped");
            assert_eq!(cpu.regs.read_x(0), 7);
        }
        assert!(engine.total_super_block_executions > 0);
    }
}
