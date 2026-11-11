//! CPU tier selection: `--cpu=interpreter|tier3|tier4|native` or `OUNCE_CPU` (default `tier3`). Tier-4 needs the
//! `native-jit` feature; without it, or on a host the recompiler does not support, Tier-3 runs.
//! `native` (native execution, `--features nce`, AArch64 hosts) runs whole homebrew programs on the
//! host CPU (`run` and the corpus oracles); block-engine callers get Tier-3 for it.

use super::interpreter::Cpu;
use super::tier2_jit::BlockExit;
use super::tier3_jit::Tier3JitEngine;
#[cfg(feature = "native-jit")]
use super::tier4::Tier4JitEngine;
use crate::mmu::Memory;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuTier {
    Interpreter,
    Tier3,
    Tier4,
    Native,
}

static CLI_TIER: OnceLock<CpuTier> = OnceLock::new();

impl CpuTier {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "interpreter" => Some(Self::Interpreter),
            "tier3" | "3" => Some(Self::Tier3),
            "tier4" | "4" => Some(Self::Tier4),
            "native" | "nce" => Some(Self::Native),
            _ => None,
        }
    }

    /// The `--cpu=` choice if one was given, else `OUNCE_CPU`, else [`CpuTier::Tier3`].
    pub fn from_env() -> Self {
        if let Some(tier) = CLI_TIER.get() {
            return *tier;
        }
        std::env::var("OUNCE_CPU").ok().and_then(|v| Self::parse(&v)).unwrap_or(Self::Tier3)
    }

    /// The tier the user asked for with `--cpu=` or `OUNCE_CPU`, or `None` when neither was given
    /// (callers then keep their default execution path).
    pub fn explicit() -> Option<Self> {
        CLI_TIER.get().copied().or_else(|| std::env::var("OUNCE_CPU").ok().and_then(|v| Self::parse(&v)))
    }

    pub fn set_cli_override(tier: Self) {
        let _ = CLI_TIER.set(tier);
    }
}

/// The selected block engine. Same `run_slice` contract for every tier.
pub enum CpuEngine {
    Interpreter,
    Tier3(Tier3JitEngine),
    #[cfg(feature = "native-jit")]
    Tier4(Box<Tier4JitEngine>),
}

impl CpuEngine {
    /// Builds the engine for `tier`, printing why if Tier-4 cannot run here.
    pub fn new(tier: CpuTier) -> Self {
        match tier {
            CpuTier::Interpreter => CpuEngine::Interpreter,
            CpuTier::Tier3 => CpuEngine::Tier3(Tier3JitEngine::new()),
            #[cfg(feature = "native-jit")]
            CpuTier::Tier4 => {
                let engine = Tier4JitEngine::new();
                match &engine.unavailable {
                    None => CpuEngine::Tier4(Box::new(engine)),
                    Some(why) => {
                        eprintln!("[Tier-4] Not available on this host ({why}); running Tier-3.");
                        CpuEngine::Tier3(Tier3JitEngine::new())
                    }
                }
            }
            CpuTier::Native => {
                eprintln!("[CPU] Native execution runs whole programs (`run`), not single blocks; running Tier-3.");
                CpuEngine::Tier3(Tier3JitEngine::new())
            }
            #[cfg(not(feature = "native-jit"))]
            CpuTier::Tier4 => {
                eprintln!("[Tier-4] Not available: rebuild with `--features native-jit`; running Tier-3.");
                CpuEngine::Tier3(Tier3JitEngine::new())
            }
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            CpuEngine::Interpreter => "Interpreter",
            CpuEngine::Tier3(_) => "Tier-3",
            #[cfg(feature = "native-jit")]
            CpuEngine::Tier4(_) => "Tier-4",
        }
    }

    pub fn run_slice(&mut self, cpu: &mut Cpu, mmu: &mut Memory, max_instructions: usize) -> (usize, BlockExit) {
        // A pending BTI check (indirect branch into a guarded page) runs before any native block;
        // Tier-4 does not check landing pads itself yet.
        if cpu.check_landing_pad(mmu).is_err() {
            return (0, BlockExit::Fault { pc: cpu.regs.pc });
        }
        match self {
            CpuEngine::Interpreter => {
                for executed in 0..max_instructions {
                    let pc = cpu.regs.pc;
                    match cpu.step(mmu) {
                        Ok(super::interpreter::StepResult::Continue) => {}
                        Ok(super::interpreter::StepResult::Svc(svc_id)) =>
                            return (executed + 1, BlockExit::Svc { svc_id }),
                        _ => return (executed, BlockExit::Fault { pc }),
                    }
                }
                (max_instructions, BlockExit::BudgetExceeded)
            }
            CpuEngine::Tier3(e) => e.run_slice(cpu, mmu, max_instructions),
            #[cfg(feature = "native-jit")]
            CpuEngine::Tier4(e) => e.run_slice(cpu, mmu, max_instructions),
        }
    }

    pub fn mips_rate(&self) -> f64 {
        match self {
            CpuEngine::Interpreter => 0.0,
            CpuEngine::Tier3(e) => e.mips_rate,
            #[cfg(feature = "native-jit")]
            CpuEngine::Tier4(e) => e.mips_rate,
        }
    }

    /// One line of engine counters for logs.
    pub fn stats(&self) -> String {
        match self {
            CpuEngine::Interpreter => "interpreter (no block cache)".into(),
            CpuEngine::Tier3(e) => format!("{} Super-Blocks fused", e.total_super_block_executions),
            #[cfg(feature = "native-jit")]
            CpuEngine::Tier4(e) => format!(
                "{} native blocks compiled, {} native runs, {} native / {} Tier-3 insns",
                e.blocks_compiled, e.native_block_runs, e.native_insns, e.fallback_insns
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tier_names() {
        assert_eq!(CpuTier::parse("interpreter"), Some(CpuTier::Interpreter));
        assert_eq!(CpuTier::parse("tier3"), Some(CpuTier::Tier3));
        assert_eq!(CpuTier::parse(" Tier4 "), Some(CpuTier::Tier4));
        assert_eq!(CpuTier::parse("native"), Some(CpuTier::Native));
        assert_eq!(CpuTier::parse("tier5"), None);
    }
}
