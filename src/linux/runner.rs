//! Run loop: ELF load, engine selection, SVC dispatch and failure diagnosis.

use super::elf::{self, ElfError};
use super::syscalls::{Outcome, Output, Process};
use crate::cpu::tier2_jit::BlockExit;
use crate::cpu::{Cpu, CpuEngine, CpuTier, StepResult};
use crate::mmu::Memory;

/// Exit status when the guest hit a syscall the layer does not implement.
pub const EXIT_UNIMPLEMENTED_SYSCALL: i32 = 70;
/// Exit status when the guest hit an instruction or memory fault.
pub const EXIT_FAULT: i32 = 71;
/// Exit status when the ELF cannot be loaded or is unsupported.
pub const EXIT_LOAD_ERROR: i32 = 72;
/// Exit status when the instruction budget ran out.
pub const EXIT_BUDGET: i32 = 73;

#[derive(Debug, Clone)]
pub struct Options {
    pub tier: CpuTier,
    pub max_instructions: u64,
    pub trace_syscalls: bool,
    pub deterministic: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { tier: CpuTier::Tier3, max_instructions: 2_000_000_000, trace_syscalls: false, deterministic: false }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    Exited(i32),
    /// Diagnosis line explaining why the program cannot continue, and the exit status to use.
    Failed { diagnosis: String, status: i32 },
}

#[derive(Debug)]
pub struct Report {
    pub stop: Stop,
    pub instructions: u64,
    pub syscalls: u64,
    pub engine: &'static str,
}

impl Report {
    pub fn exit_status(&self) -> i32 {
        match &self.stop {
            Stop::Exited(c) => *c & 0xff,
            Stop::Failed { status, .. } => *status,
        }
    }
}

fn fail(diagnosis: String, status: i32, engine: &'static str, instructions: u64, syscalls: u64) -> Report {
    Report { stop: Stop::Failed { diagnosis, status }, instructions, syscalls, engine }
}

pub fn run(elf_bytes: &[u8], argv: &[String], opts: &Options, out: &mut dyn Output, log: &mut dyn FnMut(&str)) -> Report {
    let mut mmu = Memory::new();
    let image = match elf::load(elf_bytes, &mut mmu) {
        Ok(i) => i,
        Err(e @ (ElfError::NotElf | ElfError::Unsupported(_) | ElfError::Malformed(_))) => {
            return fail(format!("cannot load ELF: {e}"), EXIT_LOAD_ERROR, "none", 0, 0);
        }
    };
    let execfn = argv.first().map(String::as_str).unwrap_or("a.out");
    let sp = match elf::build_stack(&mut mmu, &image, argv, &[], execfn) {
        Ok(sp) => sp,
        Err(e) => return fail(format!("cannot build process stack: {e}"), EXIT_LOAD_ERROR, "none", 0, 0),
    };
    if opts.tier == CpuTier::Native {
        return fail("--cpu=native is not supported by run-elf (use tier3 or tier4)".into(), EXIT_LOAD_ERROR, "none", 0, 0);
    }
    let mut engine = CpuEngine::new(opts.tier);
    let engine_name = engine.name();
    let mut cpu = Cpu::new();
    cpu.regs.pc = image.entry;
    cpu.regs.sp = sp;
    let mut proc = if opts.deterministic { Process::new_deterministic(image.brk_start) } else { Process::new(image.brk_start) };
    let (mut insns, mut syscalls) = (0u64, 0u64);
    loop {
        if insns >= opts.max_instructions {
            return fail(
                format!("instruction budget of {} exhausted at pc={:#x} (raise with --max-insns=N)", opts.max_instructions, cpu.regs.pc),
                EXIT_BUDGET, engine_name, insns, syscalls,
            );
        }
        let slice = (opts.max_instructions - insns).min(200_000) as usize;
        let (n, exit) = engine.run_slice(&mut cpu, &mut mmu, slice);
        insns += n as u64;
        match exit {
            BlockExit::Svc { svc_id } => {
                let svc_pc = cpu.regs.pc.wrapping_sub(4);
                if svc_id != 0 {
                    return fail(
                        format!("SVC #{svc_id:#x} at pc={svc_pc:#x} is not a Linux syscall (Linux AArch64 uses SVC #0)"),
                        EXIT_UNIMPLEMENTED_SYSCALL, engine_name, insns, syscalls,
                    );
                }
                let nr = cpu.regs.read_x(8);
                let args: [u64; 6] = std::array::from_fn(|i| cpu.regs.read_x(i as u8));
                syscalls += 1;
                let outcome = proc.dispatch(nr, args, &mut mmu, out);
                if opts.trace_syscalls {
                    log(&format!("syscall {nr} {args:#x?} -> {outcome:?}"));
                }
                match outcome {
                    Outcome::Return(v) => cpu.regs.write_x(0, v as u64),
                    Outcome::Exit(code) => {
                        return Report { stop: Stop::Exited(code), instructions: insns, syscalls, engine: engine_name };
                    }
                    Outcome::Unimplemented(why) => {
                        return fail(format!("{why} at pc={svc_pc:#x}"), EXIT_UNIMPLEMENTED_SYSCALL, engine_name, insns, syscalls);
                    }
                }
            }
            BlockExit::Fault { pc } => {
                return fail(diagnose_fault(&mut cpu, &mut mmu, pc), EXIT_FAULT, engine_name, insns, syscalls);
            }
            _ => {}
        }
    }
}

/// Re-steps the faulting instruction on the interpreter to learn why it failed. The program is
/// being stopped anyway, so the register state this disturbs does not matter.
fn diagnose_fault(cpu: &mut Cpu, mmu: &mut Memory, pc: u64) -> String {
    let word = mmu.read_u32(pc).ok();
    let word_text = match word {
        Some(w) => format!("instruction word {w:#010x}"),
        None => "unreadable instruction word".to_string(),
    };
    cpu.regs.pc = pc;
    let reason = match cpu.step(mmu) {
        Err(e) => e.to_string(),
        Ok(StepResult::Halt(m)) => format!("CPU halted: {m}"),
        Ok(_) => "engine faulted but the interpreter executes it (engine disagreement; try --cpu=tier3)".to_string(),
    };
    format!("fault at pc={pc:#x}, {word_text}: {reason}")
}

/// Writes guest fd 1 and 2 to the host's stdout and stderr.
pub struct HostOutput;

impl Output for HostOutput {
    fn write(&mut self, fd: u64, data: &[u8]) {
        use std::io::Write;
        if fd == 2 {
            let _ = std::io::stderr().write_all(data);
        } else {
            let mut so = std::io::stdout();
            let _ = so.write_all(data);
            let _ = so.flush();
        }
    }
}

/// CLI entry: `args` are everything after `run-elf`. Returns the process exit status.
pub fn cli(args: &[String]) -> i32 {
    let mut opts = Options { tier: CpuTier::from_env(), ..Options::default() };
    let mut path: Option<&String> = None;
    let mut guest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            guest.extend(args[i + 1..].iter().cloned());
            break;
        } else if let Some(v) = a.strip_prefix("--max-insns=") {
            match v.replace('_', "").parse() {
                Ok(n) => opts.max_instructions = n,
                Err(_) => {
                    eprintln!("run-elf: bad --max-insns value '{v}'");
                    return 2;
                }
            }
        } else if a == "--deterministic" {
            opts.deterministic = true;
        } else if a == "--trace-syscalls" {
            opts.trace_syscalls = true;
        } else if a.starts_with("--") && path.is_none() {
            eprintln!("run-elf: unknown option '{a}'");
            return 2;
        } else if path.is_none() {
            path = Some(a);
        } else {
            eprintln!("run-elf: unexpected argument '{a}' (guest arguments go after `--`)");
            return 2;
        }
        i += 1;
    }
    let Some(path) = path else {
        eprintln!("usage: ounce-core run-elf <static-aarch64-linux-ELF> [--cpu=interpreter|tier3|tier4] [--max-insns=N] [--trace-syscalls] [--deterministic] [-- args...]");
        return 2;
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("run-elf: cannot read {path}: {e}");
            return EXIT_LOAD_ERROR;
        }
    };
    let mut argv = vec![path.clone()];
    argv.extend(guest);
    let report = run(&bytes, &argv, &opts, &mut HostOutput, &mut |l| eprintln!("[run-elf] {l}"));
    eprintln!("[run-elf] engine={} instructions={} syscalls={}", report.engine, report.instructions, report.syscalls);
    if let Stop::Failed { diagnosis, .. } = &report.stop {
        eprintln!("run-elf: DIAGNOSIS: {diagnosis}");
    }
    report.exit_status()
}
