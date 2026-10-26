#![allow(dead_code)]

//! Runs module initialisers (`DT_PREINIT_ARRAY`, `DT_INIT`, `DT_INIT_ARRAY`) and finalisers
//! (`DT_FINI_ARRAY`, `DT_FINI`) on the guest CPU.
//!
//! This is opt-in. On Horizon the SDK's own startup code normally runs constructors, and `rtld`
//! does so when it is present, so running them here as well would run them twice.

use super::nro::NroLoader;
use crate::cpu::{Cpu, StepResult};
use crate::mmu::Memory;

/// Return address planted in `X30`. A guest call ends when the PC reaches it. It is never mapped.
pub const GUEST_RETURN_SENTINEL: u64 = 0xFFFF_0000_DEAD_0000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GuestCallError {
    #[error("guest made SVC #0x{svc:X} at PC 0x{pc:016X}")]
    Svc { svc: u16, pc: u64 },
    #[error("guest halted: {0}")]
    Halted(String),
    #[error("CPU fault: {0}")]
    Fault(String),
    #[error("did not return within {0} instructions")]
    StepLimit(usize),
}

/// Calls the guest function at `entry` with the current argument registers, on a stack at `stack_top`.
/// Returns `X0`. All registers are restored afterwards; memory writes are kept.
/// SVCs are not serviced: a function that makes one stops the call with `GuestCallError::Svc`.
pub fn call_guest_function(
    cpu: &mut Cpu,
    mmu: &mut Memory,
    entry: u64,
    stack_top: u64,
    max_steps: usize,
) -> Result<u64, GuestCallError> {
    let saved = cpu.regs.clone();
    cpu.regs.pc = entry;
    cpu.regs.sp = stack_top & !0xF;
    cpu.regs.write_x(30, GUEST_RETURN_SENTINEL);

    let mut result = Err(GuestCallError::StepLimit(max_steps));
    for _ in 0..max_steps {
        if cpu.regs.pc == GUEST_RETURN_SENTINEL {
            result = Ok(cpu.regs.read_x(0));
            break;
        }
        let pc = cpu.regs.pc;
        match cpu.step(mmu) {
            Ok(StepResult::Continue) => {}
            Ok(StepResult::Svc(svc)) => {
                result = Err(GuestCallError::Svc { svc, pc });
                break;
            }
            Ok(StepResult::Halt(reason)) => {
                result = Err(GuestCallError::Halted(reason));
                break;
            }
            Err(e) => {
                result = Err(GuestCallError::Fault(e.to_string()));
                break;
            }
        }
    }
    if result.is_err() && cpu.regs.pc == GUEST_RETURN_SENTINEL {
        result = Ok(cpu.regs.read_x(0));
    }
    cpu.regs = saved;
    result
}

/// Outcome of running a set of initialisers or finalisers.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InitReport {
    /// Functions that returned normally.
    pub completed: usize,
    /// Functions that did not, with the reason. Later functions still run.
    pub failed: Vec<(u64, GuestCallError)>,
}

fn run_all(cpu: &mut Cpu, mmu: &mut Memory, functions: &[u64], stack_top: u64, max_steps: usize) -> InitReport {
    let mut report = InitReport::default();
    for &f in functions {
        match call_guest_function(cpu, mmu, f, stack_top, max_steps) {
            Ok(_) => report.completed += 1,
            Err(e) => report.failed.push((f, e)),
        }
    }
    report
}

/// Runs initialisers for `modules` (as `(name, base, size)`, `main` first, as in `LooseBootInfo`).
/// Companion modules run first in their listed order, then `main`, so libraries initialise before
/// the executable. Each function gets `max_steps` instructions.
pub fn run_initializers(
    cpu: &mut Cpu,
    mmu: &mut Memory,
    modules: &[(String, u64, usize)],
    stack_top: u64,
    max_steps: usize,
) -> InitReport {
    let mut functions = Vec::new();
    for (_, base, _) in modules.iter().filter(|(n, _, _)| n != "main") {
        functions.extend(NroLoader::init_functions(*base, mmu, false));
    }
    for (_, base, _) in modules.iter().filter(|(n, _, _)| n == "main") {
        functions.extend(NroLoader::init_functions(*base, mmu, true));
    }
    run_all(cpu, mmu, &functions, stack_top, max_steps)
}

/// Runs finalisers in the reverse of `run_initializers` module order: `main` first, then companions
/// from last to first.
pub fn run_finalizers(
    cpu: &mut Cpu,
    mmu: &mut Memory,
    modules: &[(String, u64, usize)],
    stack_top: u64,
    max_steps: usize,
) -> InitReport {
    let mut functions = Vec::new();
    for (_, base, _) in modules.iter().filter(|(n, _, _)| n == "main") {
        functions.extend(NroLoader::fini_functions(*base, mmu));
    }
    for (_, base, _) in modules.iter().rev().filter(|(n, _, _)| n != "main") {
        functions.extend(NroLoader::fini_functions(*base, mmu));
    }
    run_all(cpu, mmu, &functions, stack_top, max_steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::nro::{write_test_module_with, DT_FINI, DT_FINI_ARRAY, DT_FINI_ARRAYSZ, DT_INIT, DT_INIT_ARRAY, DT_INIT_ARRAYSZ};
    use crate::mmu::Permission;

    const STACK_TOP: u64 = 0x9000_0000;
    const LOG: u64 = 0x8800_0000;

    /// Guest function: `[x19] = [x19] * 10 + digit`, so the log records call order as decimal digits.
    fn log_digit(mmu: &mut Memory, addr: u64, digit: u32) {
        let code = [
            0xF940_0269,                // ldr  x9, [x19]
            0xD280_014A,                // movz x10, #10
            0x9B0A_7D29,                // mul  x9, x9, x10
            0x9100_0129 | (digit << 10), // add  x9, x9, #digit
            0xF900_0269,                // str  x9, [x19]
            0xD65F_03C0,                // ret
        ];
        for (i, insn) in code.iter().enumerate() {
            mmu.write_u32(addr + i as u64 * 4, *insn).unwrap();
        }
    }

    fn setup() -> (Cpu, Memory) {
        let mut mmu = Memory::new();
        mmu.map_range(STACK_TOP - 0x1000, 0x1000, Permission::READ_WRITE);
        mmu.map_range(LOG, 0x1000, Permission::READ_WRITE);
        let mut cpu = Cpu::new();
        cpu.regs.write_x(19, LOG);
        (cpu, mmu)
    }

    #[test]
    fn test_initializers_and_finalizers_run_in_order() {
        let (mut cpu, mut mmu) = setup();
        let main = 0x8000_0000u64;
        let sdk = 0x8400_0000u64;

        // main: DT_INIT=1, INIT_ARRAY=[2, 3], FINI_ARRAY=[4, 5], DT_FINI=6.
        write_test_module_with(
            &mut mmu,
            main,
            &[],
            &[],
            &[],
            &[
                (DT_INIT, 0x1000),
                (DT_INIT_ARRAY, 0xC00),
                (DT_INIT_ARRAYSZ, 16),
                (DT_FINI_ARRAY, 0xC80),
                (DT_FINI_ARRAYSZ, 16),
                (DT_FINI, 0x10A0),
            ],
            false,
        );
        for (i, digit) in (1..=6).enumerate() {
            log_digit(&mut mmu, main + 0x1000 + i as u64 * 0x20, digit);
        }
        mmu.write_u64(main + 0xC00, main + 0x1020).unwrap();
        mmu.write_u64(main + 0xC08, main + 0x1040).unwrap();
        mmu.write_u64(main + 0xC80, main + 0x1060).unwrap();
        mmu.write_u64(main + 0xC88, main + 0x1080).unwrap();

        // sdk: DT_INIT=7, DT_FINI=8.
        write_test_module_with(&mut mmu, sdk, &[], &[], &[], &[(DT_INIT, 0x1000), (DT_FINI, 0x1020)], false);
        log_digit(&mut mmu, sdk + 0x1000, 7);
        log_digit(&mut mmu, sdk + 0x1020, 8);

        let modules = vec![("main".to_string(), main, 0), ("sdk".to_string(), sdk, 0)];
        let saved_pc = cpu.regs.pc;

        let report = run_initializers(&mut cpu, &mut mmu, &modules, STACK_TOP, 100);
        assert_eq!(report, InitReport { completed: 4, failed: vec![] });
        assert_eq!(mmu.read_u64(LOG).unwrap(), 7123);
        assert_eq!(cpu.regs.pc, saved_pc);

        mmu.write_u64(LOG, 0).unwrap();
        let report = run_finalizers(&mut cpu, &mut mmu, &modules, STACK_TOP, 100);
        assert_eq!(report.completed, 4);
        assert_eq!(mmu.read_u64(LOG).unwrap(), 5468);
    }

    #[test]
    fn test_guest_call_stops_on_svc_and_step_limit() {
        let (mut cpu, mut mmu) = setup();
        let code = 0x8000_0000u64;
        mmu.map_range(code, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.write_u32(code, 0xD400_00E1).unwrap(); // svc #0x7
        mmu.write_u32(code + 0x10, 0x1400_0000).unwrap(); // b . (spins)

        assert_eq!(
            call_guest_function(&mut cpu, &mut mmu, code, STACK_TOP, 10),
            Err(GuestCallError::Svc { svc: 7, pc: code })
        );
        assert_eq!(
            call_guest_function(&mut cpu, &mut mmu, code + 0x10, STACK_TOP, 10),
            Err(GuestCallError::StepLimit(10))
        );
        assert_eq!(cpu.regs.read_x(30), 0);
    }
}
