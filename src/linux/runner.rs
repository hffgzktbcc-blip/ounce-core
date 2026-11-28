//! Run loop: ELF load, engine selection, SVC dispatch and failure diagnosis.

use super::elf::{self, ElfError};
use super::signals::SyncFault;
use super::syscalls::{Outcome, Output, Process};
use crate::cpu::tier2_jit::BlockExit;
use crate::cpu::{Cpu, CpuEngine, CpuError, CpuTier, StepResult};
use crate::mmu::Memory;
use crate::run_report::{self, RunReport};

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
    /// `--root=DIR`: a host directory the guest sees read-only as its file system outside the scratch area
    pub root: Option<std::path::PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            tier: CpuTier::Tier3,
            max_instructions: 2_000_000_000,
            trace_syscalls: false,
            deterministic: false,
            root: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    Exited(i32),
    /// Diagnosis line explaining why the program cannot continue, and the exit status to use.
    Failed {
        diagnosis: String,
        status: i32,
    },
}

/// Registers when the program stopped, for `--dump-regs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopRegs {
    pub pc: u64,
    /// Link register (`x30`) and stack pointer.
    pub lr: u64,
    pub sp: u64,
    pub x0: u64,
    /// Raw bits of D0 (the low 64 bits of V0).
    pub d0: u64,
    /// N, Z, C, V as bits 3..0.
    pub nzcv: u32,
}

impl StopRegs {
    fn capture(cpu: &Cpu, pc: u64) -> Self {
        Self {
            pc,
            lr: cpu.regs.read_x(30),
            sp: cpu.regs.sp,
            x0: cpu.regs.read_x(0),
            d0: cpu.regs.read_q(0) as u64,
            nzcv: cpu.regs.pstate.to_bits() >> 28,
        }
    }
}

/// Structured facts about how the program stopped: the `ounce.run.v1` fields and `--dump-regs`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StopDetail {
    /// `ounce.run.v1` `stop_kind` (docs/OUNCE_RUN_V1_SCHEMA.md).
    pub stop_kind: &'static str,
    pub pc: Option<u64>,
    pub opcode: Option<u32>,
    pub fault_addr: Option<u64>,
    /// Resolved ELF `.symtab` symbol for `pc` on fault stops (#475).
    pub fault_symbol: Option<String>,
    /// Bounded AArch64 `X29`/`LR` frame-pointer chain on fault stops (#475).
    pub backtrace: Option<String>,
    /// The Linux syscall number (`x8`) of an unimplemented syscall, or the immediate of a non-zero `SVC #imm`.
    pub svc_id: Option<u32>,
    /// `None` when the program never started.
    pub regs: Option<StopRegs>,
}

impl StopDetail {
    fn at(stop_kind: &'static str, cpu: &Cpu, pc: u64) -> Self {
        Self {
            stop_kind,
            pc: Some(pc),
            regs: Some(StopRegs::capture(cpu, pc)),
            ..Self::default()
        }
    }

    fn not_started() -> Self {
        Self {
            stop_kind: "loader_error",
            ..Self::default()
        }
    }
}

#[derive(Debug)]
pub struct Report {
    pub stop: Stop,
    pub instructions: u64,
    pub syscalls: u64,
    pub engine: &'static str,
    pub detail: StopDetail,
}

impl Report {
    pub fn exit_status(&self) -> i32 {
        match &self.stop {
            Stop::Exited(c) => *c & 0xff,
            Stop::Failed { status, .. } => *status,
        }
    }

    /// The `ounce.run.v1` report. `exit_code` says how the engine stopped (0 for any clean exit);
    /// `guest_exit_status` carries the program's own status, which is what a test harness reports through.
    pub fn to_run_report(&self, requested: CpuTier) -> RunReport {
        let tier = match self.engine {
            "Interpreter" => "interpreter",
            "Tier-3" => "tier3",
            "Tier-4" => "tier4",
            _ => requested.report_label(),
        };
        RunReport {
            tier,
            silent_fallback: tier != requested.report_label(),
            stop_kind: self.detail.stop_kind,
            exit_code: run_report::exit_code_for(self.detail.stop_kind),
            pc: self.detail.pc,
            lr: self.detail.regs.map(|r| r.lr),
            sp: self.detail.regs.map(|r| r.sp),
            opcode: self.detail.opcode,
            fault_addr: self.detail.fault_addr,
            fault_symbol: self.detail.fault_symbol.clone(),
            backtrace: self.detail.backtrace.clone(),
            svc_id: self.detail.svc_id,
            sm_service: None,
            cmif_cmd_id: None,
            break_result: None,
            insns_executed: self.instructions,
            frames_presented: 0,
            framebuffer_hash: None,
            audio_frames_mixed: 0,
            audio_hash: None,
            guest_exit_status: match self.stop {
                Stop::Exited(code) => Some(code & 0xff),
                Stop::Failed { .. } => None,
            },
            message: match &self.stop {
                Stop::Failed { diagnosis, .. } => Some(diagnosis.clone()),
                Stop::Exited(_) => None,
            },
        }
    }
}

/// `--dump-regs` text: one line, stable field order.
pub fn describe_regs(regs: Option<StopRegs>) -> String {
    match regs {
        Some(r) => format!(
            "registers at stop: pc={:#018x} x0={:#018x} d0={:#018x} ({}) nzcv={:04b}",
            r.pc,
            r.x0,
            r.d0,
            f64::from_bits(r.d0),
            r.nzcv
        ),
        None => "registers at stop: unavailable (the program did not start)".to_string(),
    }
}

fn fail(
    diagnosis: String,
    status: i32,
    detail: StopDetail,
    engine: &'static str,
    instructions: u64,
    syscalls: u64,
) -> Report {
    Report {
        stop: Stop::Failed { diagnosis, status },
        instructions,
        syscalls,
        engine,
        detail,
    }
}

pub fn run(
    elf_bytes: &[u8],
    argv: &[String],
    opts: &Options,
    out: &mut dyn Output,
    log: &mut dyn FnMut(&str),
) -> Report {
    run_with_stdin(elf_bytes, argv, opts, &[], out, log)
}

pub fn run_with_stdin(
    elf_bytes: &[u8],
    argv: &[String],
    opts: &Options,
    stdin: &[u8],
    out: &mut dyn Output,
    log: &mut dyn FnMut(&str),
) -> Report {
    let mut mmu = Memory::new();
    let mut image = match elf::load(elf_bytes, &mut mmu) {
        Ok(i) => i,
        Err(e @ (ElfError::NotElf | ElfError::Unsupported(_) | ElfError::Malformed(_))) => {
            return fail(
                format!("cannot load ELF: {e}"),
                EXIT_LOAD_ERROR,
                StopDetail::not_started(),
                "none",
                0,
                0,
            );
        }
    };
    // PT_INTERP: the program interpreter (ld.so) comes from --root, runs first at its own base and starts the program
    let mut start = image.entry;
    if let Some(path) = image.interp.clone() {
        let not_started = |why: String| fail(why, EXIT_LOAD_ERROR, StopDetail::not_started(), "none", 0, 0);
        let Some(root) = &opts.root else {
            return not_started(format!(
                "dynamically linked: its program interpreter {path} must come from --root=DIR, a folder that contains {path}"
            ));
        };
        let bytes = match super::syscalls::read_under_root(root, &path) {
            Ok(b) => b,
            Err(e) => {
                return not_started(format!(
                    "cannot read the program interpreter {path} under --root={}: {e}",
                    root.display()
                ));
            }
        };
        match elf::load_at(&bytes, &mut mmu, elf::INTERP_BASE) {
            Ok(ld) => {
                image.at_base = ld.base;
                start = ld.entry;
            }
            Err(e) => return not_started(format!("cannot load the program interpreter {path}: {e}")),
        }
    }
    let execfn = argv.first().map(String::as_str).unwrap_or("a.out");
    let (sp, auxv_bytes) = match elf::build_stack_with_auxv(&mut mmu, &image, argv, &[], execfn) {
        Ok(res) => res,
        Err(e) => {
            return fail(
                format!("cannot build process stack: {e}"),
                EXIT_LOAD_ERROR,
                StopDetail::not_started(),
                "none",
                0,
                0,
            );
        }
    };
    if opts.tier == CpuTier::Native {
        let why = "--cpu=native is not supported by run-elf (valid for run-elf: interpreter (alias interp, tier1), tier3 (alias 3), tier4 (alias 4))".into();
        return fail(why, EXIT_LOAD_ERROR, StopDetail::not_started(), "none", 0, 0);
    }
    let mut engine = CpuEngine::new(opts.tier);
    let engine_name = engine.name();
    let mut cpu = Cpu::new();
    cpu.set_linux_counter(opts.deterministic);
    cpu.regs.pc = start;
    cpu.regs.sp = sp;
    let mut proc = if opts.deterministic {
        Process::new_deterministic(image.brk_start)
    } else {
        Process::new(image.brk_start)
    };
    proc.set_auxv(auxv_bytes);
    proc.set_stdin(stdin.to_vec());
    if let Some(dir) = &opts.root
        && let Err(e) = proc.set_host_root(dir)
    {
        let why = format!("--root={}: {e}", dir.display());
        return fail(why, EXIT_LOAD_ERROR, StopDetail::not_started(), "none", 0, 0);
    }
    let (mut insns, mut syscalls) = (0u64, 0u64);
    let mut threads = super::threads::Threads::new();
    loop {
        if insns >= opts.max_instructions {
            return fail(
                format!(
                    "instruction budget of {} exhausted at pc={:#x} (raise with --max-insns=N)",
                    opts.max_instructions, cpu.regs.pc
                ),
                EXIT_BUDGET,
                StopDetail::at("budget_exhausted", &cpu, cpu.regs.pc),
                engine_name,
                insns,
                syscalls,
            );
        }
        if opts.deterministic {
            cpu.counter_ticks = cpu.counter_ticks.max(insns);
        }
        let slice = threads.slice(opts.max_instructions - insns);
        let (n, exit) = engine.run_slice(&mut cpu, &mut mmu, slice);
        insns += n as u64;
        match exit {
            BlockExit::Svc { svc_id } => {
                let svc_pc = cpu.regs.pc.wrapping_sub(4);
                if svc_id != 0 {
                    let detail = StopDetail {
                        svc_id: Some(svc_id as u32),
                        ..StopDetail::at("unhandled_svc", &cpu, svc_pc)
                    };
                    return fail(
                        format!(
                            "SVC #{svc_id:#x} at pc={svc_pc:#x} is not a Linux syscall (Linux AArch64 uses SVC #0)"
                        ),
                        EXIT_UNIMPLEMENTED_SYSCALL,
                        detail,
                        engine_name,
                        insns,
                        syscalls,
                    );
                }
                let nr = cpu.regs.read_x(8);
                let mut args: [u64; 6] = std::array::from_fn(|i| cpu.regs.read_x(i as u8));
                syscalls += 1;
                proc.set_retired_instructions(insns);
                if nr == 139 {
                    if proc.signal_return(&mut cpu, &mmu).is_err() {
                        let detail = StopDetail::at("cpu_error", &cpu, svc_pc);
                        return fail(
                            "invalid rt_sigreturn frame".into(),
                            EXIT_FAULT,
                            detail,
                            engine_name,
                            insns,
                            syscalls,
                        );
                    }
                    continue;
                }
                // signals are the process's: a tkill/tgkill aimed at any live thread goes to the process
                use super::threads::{SYS_TGKILL, SYS_TKILL, Step};
                if nr == SYS_TKILL && threads.is_live(args[0]) {
                    args[0] = super::syscalls::PID;
                } else if nr == SYS_TGKILL && threads.is_live(args[1]) {
                    args[1] = super::syscalls::PID;
                }
                match threads.syscall(nr, args, &mut cpu, &mut mmu) {
                    Step::NotMine => {}
                    step => {
                        if opts.trace_syscalls {
                            log(&format!("syscall {nr} {args:#x?} -> {step:?}"));
                        }
                        match step {
                            Step::Return(v) => cpu.regs.write_x(0, v as u64),
                            Step::ProcessExit(code) => {
                                return Report {
                                    stop: Stop::Exited(code),
                                    instructions: insns,
                                    syscalls,
                                    engine: engine_name,
                                    detail: StopDetail::at("clean_exit", &cpu, svc_pc),
                                };
                            }
                            Step::Deadlock(why) => {
                                // a deadlocked guest would run forever: report it as an exhausted budget
                                let detail = StopDetail::at("budget_exhausted", &cpu, svc_pc);
                                return fail(why, EXIT_BUDGET, detail, engine_name, insns, syscalls);
                            }
                            Step::Switched | Step::NotMine => {}
                        }
                        continue;
                    }
                }
                let outcome = proc.dispatch(nr, args, &mut mmu, out);
                if opts.trace_syscalls {
                    log(&format!("syscall {nr} {args:#x?} -> {outcome:?}"));
                }
                match outcome {
                    Outcome::Return(v) => cpu.regs.write_x(0, v as u64),
                    Outcome::Exit(code) => {
                        let detail = StopDetail::at("clean_exit", &cpu, svc_pc);
                        return Report {
                            stop: Stop::Exited(code),
                            instructions: insns,
                            syscalls,
                            engine: engine_name,
                            detail,
                        };
                    }
                    Outcome::Unimplemented(why) => {
                        let detail = StopDetail {
                            svc_id: u32::try_from(nr).ok(),
                            ..StopDetail::at("unhandled_svc", &cpu, svc_pc)
                        };
                        return fail(
                            format!("{why} at pc={svc_pc:#x}"),
                            EXIT_UNIMPLEMENTED_SYSCALL,
                            detail,
                            engine_name,
                            insns,
                            syscalls,
                        );
                    }
                }
            }
            BlockExit::Fault { pc } => {
                let delivery = proc.deliver_fault(&mut cpu, &mut mmu, pc);
                if matches!(delivery, SyncFault::Delivered) {
                    continue;
                }
                let (diagnosis, detail) = diagnose_fault(&mut cpu, &mut mmu, pc, &image.symbols);
                if let SyncFault::Fatal(sig) = delivery {
                    // Default action of a forced SIGILL/SIGBUS: the process dies of the signal, which a shell
                    // reports as 128 + signal. The diagnosis still names the instruction or register.
                    let name = if sig == 4 {
                        "SIGILL (illegal instruction)"
                    } else {
                        "SIGBUS (bus error)"
                    };
                    return fail(
                        format!("guest terminated by {name}: {diagnosis}"),
                        128 + sig,
                        detail,
                        engine_name,
                        insns,
                        syscalls,
                    );
                }
                return fail(diagnosis, EXIT_FAULT, detail, engine_name, insns, syscalls);
            }
            // the slice ran out: the next runnable thread gets the CPU
            _ => threads.preempt(&mut cpu),
        }
    }
}

/// Re-steps the faulting instruction on the interpreter to learn why it failed. Registers are captured
/// first; the program is being stopped anyway, so the state the re-step disturbs does not matter.
fn diagnose_fault(cpu: &mut Cpu, mmu: &mut Memory, pc: u64, symbols: &elf::SymbolTable) -> (String, StopDetail) {
    let mut detail = StopDetail::at("cpu_error", cpu, pc);
    let lr = cpu.regs.read_x(30);
    let fp = cpu.regs.read_x(29);
    detail.fault_symbol = symbols.resolve(pc);
    detail.backtrace = run_report::format_backtrace(pc, lr, fp, mmu, |addr| symbols.resolve(addr));
    let word = mmu.read_u32(pc).ok();
    let word_text = match word {
        Some(w) => format!("instruction word {w:#010x}"),
        None => "unreadable instruction word".to_string(),
    };
    cpu.regs.pc = pc;
    let reason = match cpu.step(mmu) {
        Err(e) => {
            match &e {
                CpuError::UnimplementedInstruction { raw, .. } => {
                    detail.stop_kind = "unimplemented_insn";
                    detail.opcode = Some(*raw);
                }
                CpuError::Memory(m) => {
                    detail.stop_kind = "memory_fault";
                    detail.fault_addr = Some(m.addr());
                }
                CpuError::Breakpoint { .. }
                | CpuError::Undefined { .. }
                | CpuError::Halted(_)
                | CpuError::BranchTarget { .. }
                | CpuError::IllegalSystemRegister { .. } => {}
            }
            e.to_string()
        }
        Ok(StepResult::Halt(m)) => {
            detail.stop_kind = "cpu_halted";
            format!("CPU halted: {m}")
        }
        Ok(_) => "engine faulted but the interpreter executes it (engine disagreement; try --cpu=tier3)".to_string(),
    };
    let loc = match &detail.fault_symbol {
        Some(sym) => format!("pc={pc:#x} ({sym})"),
        None => format!("pc={pc:#x}"),
    };
    (format!("fault at {loc}, {word_text}: {reason}"), detail)
}

/// Writes guest fd 1 and 2 to the host's stdout and stderr. While `--report-json=-` owns stdout, guest fd 1 goes
/// to stderr too, so stdout is only the JSON report.
pub struct HostOutput;

impl Output for HostOutput {
    fn write(&mut self, fd: u64, data: &[u8]) {
        use std::io::Write;
        if fd == 2 || crate::run_report::is_stdout_report() {
            let _ = std::io::stderr().write_all(data);
        } else {
            let mut so = std::io::stdout();
            let _ = so.write_all(data);
            let _ = so.flush();
        }
    }
}

const RUN_ELF_USAGE: &str = "usage: ounce-core run-elf <static-aarch64-linux-ELF> [--cpu=interpreter|tier3|tier4] [--max-insns=N] [--trace-syscalls] [--deterministic] [--dump-regs] [--report-json=<path>] [--stdin[=<path>]] [--root=<dir>] [-- args...]";

/// CLI entry: `args` are everything after `run-elf`. Returns the process exit status.
pub fn cli(args: &[String]) -> i32 {
    let mut opts = Options {
        tier: CpuTier::from_env(),
        ..Options::default()
    };
    let mut dump_regs = false;
    let mut stdin_bytes = Vec::new();
    let mut path: Option<&String> = None;
    let mut guest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            if path.is_none() {
                if i + 1 < args.len() {
                    path = Some(&args[i + 1]);
                    guest.extend(args[i + 2..].iter().cloned());
                }
                break;
            } else {
                guest.extend(args[i + 1..].iter().cloned());
                break;
            }
        } else if let Some(v) = a.strip_prefix("--max-insns=") {
            match v.replace('_', "").parse() {
                Ok(n) => opts.max_instructions = n,
                Err(_) => {
                    eprintln!("run-elf: bad --max-insns value '{v}'");
                    return 2;
                }
            }
        } else if a == "--help" || a == "-h" {
            println!("{RUN_ELF_USAGE}");
            return 0;
        } else if let Some(dir) = a.strip_prefix("--root=") {
            opts.root = Some(std::path::PathBuf::from(dir));
        } else if a == "--deterministic" {
            opts.deterministic = true;
        } else if a == "--trace-syscalls" {
            opts.trace_syscalls = true;
        } else if a == "--dump-regs" {
            dump_regs = true;
        } else if a == "--stdin" || a == "--stdin=-" {
            use std::io::Read;
            let mut buf = Vec::new();
            if let Err(e) = std::io::stdin().take(64 << 20).read_to_end(&mut buf) {
                eprintln!("run-elf: failed to read stdin: {e}");
                return 2;
            }
            stdin_bytes = buf;
        } else if let Some(p) = a.strip_prefix("--stdin=") {
            use std::io::Read;
            let f = match std::fs::File::open(p) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("run-elf: failed to open stdin file '{p}': {e}");
                    return 2;
                }
            };
            let mut buf = Vec::new();
            if let Err(e) = f.take(64 << 20).read_to_end(&mut buf) {
                eprintln!("run-elf: failed to read stdin file '{p}': {e}");
                return 2;
            }
            stdin_bytes = buf;
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
        eprintln!("{RUN_ELF_USAGE}");
        return 2;
    };
    let report = match std::fs::read(path) {
        Ok(bytes) => {
            let mut argv = vec![path.clone()];
            argv.extend(guest);
            run_with_stdin(&bytes, &argv, &opts, &stdin_bytes, &mut HostOutput, &mut |l| {
                eprintln!("[run-elf] {l}")
            })
        }
        Err(e) => fail(
            format!("cannot read {path}: {e}"),
            EXIT_LOAD_ERROR,
            StopDetail::not_started(),
            "none",
            0,
            0,
        ),
    };
    eprintln!(
        "[run-elf] engine={} instructions={} syscalls={}",
        report.engine, report.instructions, report.syscalls
    );
    if let Stop::Failed { diagnosis, .. } = &report.stop {
        eprintln!("run-elf: DIAGNOSIS: {diagnosis}");
    }
    if dump_regs {
        eprintln!("[run-elf] {}", describe_regs(report.detail.regs));
    }
    run_report::write_if_configured(&report.to_run_report(opts.tier));
    report.exit_status()
}
