//! Opt-in test adapter for unchanged external NROs. No downloads or host device backends.
//! The host deadline is cooperative between slices/SVCs; callers must also impose a process
//! timeout because a blocking kernel call cannot be interrupted here.

use crate::cpu::{Cpu, StepResult};
use crate::cpu::tier2_jit::BlockExit;
use crate::cpu::tier3_jit::Tier3JitEngine;
use crate::kernel::{HorizonKernel, SvcResult};
use crate::kernel::hid::NpadInput;
use crate::mmu::{Memory, Permission};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engine { Interpreter, Tier3 }

#[derive(Debug, Clone, PartialEq, Eq)]
enum Stop {
    ExitProcess,
    Break { reason: u64, info1: u64, info2: u64, result: Option<u32> },
    UnhandledSvc(u16),
    CpuHalt(String),
    Fault { pc: u64, instruction: Option<u32>, error: String },
    InstructionBudget,
    HostTimeout,
    FrameLimit,
    LoadError(String),
    MissingInput(String),
}

#[derive(Clone, Copy)]
struct InputCheckpoint { at_instructions: usize, input: NpadInput }

struct Limits {
    instructions: usize,
    host_time: Duration,
    frames: usize,
    trace_entries: usize,
    debug_bytes: usize,
    /// Stop after this many VI `QUEUE_BUFFER`s (0 = no limit). Every queued frame is captured.
    queued_frames: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { instructions: crate::NRO_INSTRUCTION_BUDGET, host_time: Duration::from_secs(10),
            frames: 16, trace_entries: 256, debug_bytes: 8192, queued_frames: 0 }
    }
}

#[derive(Debug)]
struct Report {
    #[allow(dead_code)]
    requested_engine: Engine,
    actual_engine: Engine,
    stop: Stop,
    instructions: usize,
    pc: u64,
    x0: u64,
    registers: [u64; 31],
    frames: usize,
    preemptions: usize,
    svcs: Vec<(u64, u16)>,
    /// The last 32 SVCs (pc, id), oldest first.
    last_svcs: std::collections::VecDeque<(u64, u16)>,
    debug_output: String,
    input_applied_at: Vec<usize>,
    raw_frame: Option<RawFrameEvidence>,
    /// One entry per VI `QUEUE_BUFFER`, captured right after the queue.
    queued: Vec<QueuedFrame>,
    first_present_diagnostic: Option<String>,
    post_queue_diagnostic: Option<String>,
    // A report is an observation. A normal guest exit alone is not a compatibility verdict.
    compatibility: &'static str,
}

/// The submitted VI buffer right after the `index`-th queue (1-based).
#[derive(Debug, Clone, PartialEq, Eq)]
struct QueuedFrame {
    index: u64,
    width: usize,
    height: usize,
    /// `Frame::content_hash` (alpha forced to 0xFF by `frame_from_surface`).
    rgba_fnv1a64: u64,
    /// The pixel every pixel equals, if the frame is uniform.
    uniform: Option<[u8; 4]>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawFrameEvidence {
    guest_addr: u64,
    width: usize,
    height: usize,
    rgba_fnv1a64: u64,
    non_black_pixels: usize,
    center_rgba: Option<(u8, u8, u8, u8)>,
}

fn report(engine: Engine, stop: Stop, cpu: &Cpu) -> Report {
    Report { requested_engine: engine, actual_engine: engine, stop,
        instructions: 0, pc: cpu.regs.pc, x0: cpu.regs.read_x(0), registers: std::array::from_fn(|i| cpu.regs.read_x(i as u8)), frames: 0,
        preemptions: 0, svcs: Vec::new(), last_svcs: Default::default(), debug_output: String::new(),
        input_applied_at: Vec::new(), raw_frame: None, queued: Vec::new(), first_present_diagnostic: None, post_queue_diagnostic: None, compatibility: "not-assessed" }
}

fn capture_queued(kernel: &HorizonKernel, mmu: &Memory, index: u64) -> QueuedFrame {
    let mut out = QueuedFrame { index, width: 0, height: 0, rgba_fnv1a64: 0, uniform: None, error: None };
    let Some(submitted) = kernel.ipc.submitted_vi_surface() else {
        out.error = Some("no resolvable VI submission".into());
        return out;
    };
    let Some(surface) = submitted.tiling_surface() else {
        out.error = Some(format!("unsupported descriptor: {submitted:?}"));
        return out;
    };
    match crate::gpu::present::surface::frame_from_surface(mmu, submitted.guest_addr, &surface) {
        Ok(frame) => {
            let mut px = frame.rgba.chunks_exact(4);
            let first: Option<[u8; 4]> = px.next().map(|p| p.try_into().unwrap());
            out.uniform = first.filter(|f| px.all(|p| p == f));
            out.width = frame.width;
            out.height = frame.height;
            out.rgba_fnv1a64 = frame.content_hash();
        }
        Err(error) => out.error = Some(format!("{error:?}")),
    }
    out
}

/// Run an already configured VM, retaining the real kernel dispatcher and scheduler.
/// Tier-3 can overshoot a slice target by one super-block; report the actual instruction total.
fn execute(mut cpu: Cpu, mut mmu: Memory, mut kernel: HorizonKernel, engine: Engine,
    limits: &Limits, inputs: &[InputCheckpoint]) -> (Report, HorizonKernel) {
    let mut result = report(engine, Stop::InstructionBudget, &cpu);
    let started = Instant::now();
    let mut tier3 = match engine { Engine::Tier3 => Some(Tier3JitEngine::new()), _ => None };
    let quantum = crate::kernel::scheduler::PREEMPT_INTERVAL;
    let mut next_preemption = quantum;
    let mut next_input = 0;
    'run: loop {
        if started.elapsed() >= limits.host_time { result.stop = Stop::HostTimeout; break; }
        if result.instructions >= limits.instructions { break; }
        while next_input < inputs.len() && inputs[next_input].at_instructions <= result.instructions {
            kernel.ipc.hid.input = inputs[next_input].input;
            kernel.ipc.hid.sync(&mut mmu);
            result.input_applied_at.push(result.instructions);
            next_input += 1;
        }
        let remaining = limits.instructions - result.instructions;
        let until_preemption = next_preemption.saturating_sub(result.instructions).max(1);
        let until_input = inputs.get(next_input).map(|i| i.at_instructions.saturating_sub(result.instructions).max(1)).unwrap_or(usize::MAX);
        let target = remaining.min(until_preemption).min(until_input).min(256);
        let mut svc = None;
        match tier3.as_mut() {
            Some(jit) => {
                let (count, exit) = jit.run_slice(&mut cpu, &mut mmu, target);
                result.instructions = result.instructions.saturating_add(count);
                match exit {
                    BlockExit::Svc { svc_id } => svc = Some(svc_id),
                    BlockExit::Fault { pc } => {
                        result.stop = Stop::Fault { pc, instruction: mmu.read_u32(pc).ok(), error: "Tier-3 instruction or memory fault".into() };
                        break;
                    }
                    BlockExit::Return { .. } | BlockExit::Branch { .. } |
                    BlockExit::BranchCondTaken { .. } | BlockExit::BranchFallthrough { .. } |
                    BlockExit::BudgetExceeded => {}
                }
            }
            None => {
                let pc = cpu.regs.pc;
                match cpu.step(&mut mmu) {
                    Ok(step) => {
                        result.instructions += 1;
                        match step {
                            StepResult::Continue => {}
                            StepResult::Svc(id) => svc = Some(id),
                            StepResult::Halt(message) => { result.stop = Stop::CpuHalt(message); break; }
                        }
                    }
                    Err(error) => {
                        result.stop = Stop::Fault { pc, instruction: mmu.read_u32(pc).ok(), error: error.to_string() };
                        break;
                    }
                }
            }
        }
        if let Some(id) = svc {
            if result.svcs.len() < limits.trace_entries {
                result.svcs.push((cpu.regs.pc.saturating_sub(4), id));
            }
            if result.last_svcs.len() == 32 {
                result.last_svcs.pop_front();
            }
            result.last_svcs.push_back((cpu.regs.pc.saturating_sub(4), id));
            if id == 0x27 {
                let available = limits.debug_bytes.saturating_sub(result.debug_output.len());
                let mut bytes = vec![0; (cpu.regs.read_x(1).min(available as u64)) as usize];
                if mmu.read_bytes(cpu.regs.read_x(0), &mut bytes).is_ok() {
                    // Keep ASCII/UTF-8 text bounded even when replacement characters expand.
                    for ch in String::from_utf8_lossy(&bytes).chars() {
                        if result.debug_output.len() + ch.len_utf8() > limits.debug_bytes { break; }
                        result.debug_output.push(ch);
                    }
                }
            }
            match kernel.dispatch_svc(id, &mut cpu, &mut mmu) {
                SvcResult::Success => {}
                SvcResult::ExitProcess => { result.stop = Stop::ExitProcess; break; }
                SvcResult::Break { reason, info1, info2 } => {
                    result.stop = Stop::Break { reason, info1, info2,
                        result: if info2 == 4 { mmu.read_u32(info1).ok() } else { None } };
                    break;
                }
                SvcResult::Unhandled(id) => { result.stop = Stop::UnhandledSvc(id); break; }
                SvcResult::PresentFrame => {
                    result.frames += 1;
                    if result.first_present_diagnostic.is_none() {
                        result.first_present_diagnostic = Some(match kernel.ipc.submitted_vi_surface() {
                            None => format!("svc={id:#x}: no resolvable VI submission; {}",
                                kernel.ipc.vi_submission_diagnostic()),
                            Some(submitted) => match submitted.tiling_surface() {
                                None => format!("svc={id:#x}: unsupported or out-of-bounds descriptor: {submitted:?}"),
                                Some(surface) => match crate::gpu::present::surface::frame_from_surface(
                                    &mmu, submitted.guest_addr, &surface) {
                                    Ok(_) => format!("svc={id:#x}: extracted {submitted:?}"),
                                    Err(error) => format!("svc={id:#x}: extraction failed: {error:?}; {submitted:?}"),
                                },
                            },
                        });
                    }
                    if let Some(submitted) = kernel.ipc.submitted_vi_surface()
                        && let Some(surface) = submitted.tiling_surface()
                        && let Ok(frame) = crate::gpu::present::surface::frame_from_surface(
                            &mmu, submitted.guest_addr, &surface)
                    {
                        result.raw_frame = Some(RawFrameEvidence {
                            guest_addr: submitted.guest_addr,
                            width: frame.width,
                            height: frame.height,
                            rgba_fnv1a64: frame.content_hash(),
                            non_black_pixels: frame.rgba.chunks_exact(4)
                                .filter(|pixel| pixel[..3] != [0, 0, 0]).count(),
                            center_rgba: frame.pixel(frame.width / 2, frame.height / 2),
                        });
                    }
                    if result.frames >= limits.frames { result.stop = Stop::FrameLimit; break 'run; }
                }
            }
        }
        let queued = kernel.ipc.vi_frames_queued();
        if queued > result.queued.len() as u64 {
            result.queued.push(capture_queued(&kernel, &mmu, queued));
            if limits.queued_frames != 0 && queued >= limits.queued_frames {
                result.stop = Stop::FrameLimit;
                break;
            }
        }
        if result.post_queue_diagnostic.is_none()
            && let Some(submitted) = kernel.ipc.submitted_vi_surface()
        {
            result.post_queue_diagnostic = Some(match submitted.tiling_surface() {
                None => format!("unsupported or out-of-bounds descriptor: {submitted:?}"),
                Some(surface) => match crate::gpu::present::surface::frame_from_surface(
                    &mmu, submitted.guest_addr, &surface) {
                    Ok(frame) => {
                        result.raw_frame = Some(RawFrameEvidence {
                            guest_addr: submitted.guest_addr,
                            width: frame.width,
                            height: frame.height,
                            rgba_fnv1a64: frame.content_hash(),
                            non_black_pixels: frame.rgba.chunks_exact(4)
                                .filter(|pixel| pixel[..3] != [0, 0, 0]).count(),
                            center_rgba: frame.pixel(frame.width / 2, frame.height / 2),
                        });
                        format!("post-queue extraction succeeded: {}x{} hash={:#x}; {submitted:?}",
                            frame.width, frame.height, frame.content_hash())
                    },
                    Err(error) => format!("post-queue extraction failed: {error:?}; {submitted:?}"),
                },
            });
        }
        // Dispatch may switch the guest thread: resume from the post-dispatch registers.
        // A block can cross a quantum without landing exactly on a modulo boundary.
        if result.instructions >= next_preemption {
            kernel.scheduler.preempt(&mut cpu);
            result.preemptions += 1;
            next_preemption = result.instructions.saturating_add(quantum);
        }
    }
    result.pc = cpu.regs.pc;
    result.x0 = cpu.regs.read_x(0);
    result.registers = std::array::from_fn(|i| cpu.regs.read_x(i as u8));
    (result, kernel)
}

fn run_nro_state(nro: &[u8], path: &str, args: &[&str], engine: Engine,
    limits: &Limits, inputs: &[InputCheckpoint]) -> (Report, HorizonKernel) {
    let (mut cpu, mut mmu, mut kernel) = crate::setup_vm();
    let loaded = super::super::BootLoader::load_any_package(nro, crate::DEFAULT_BASE_ADDR, &[0; 16], &mut mmu);
    let error = match loaded {
        Ok(info) if info.format == "NRO0" => None,
        Ok(_) => Some("external runner accepts NRO0 only".into()),
        Err(error) => Some(error.to_string()),
    };
    if let Some(error) = error { return (report(engine, Stop::LoadError(error), &cpu), kernel); }
    if let Err(error) = crate::setup_homebrew_nro(&mut cpu, &mut mmu, &mut kernel, path, nro, args) {
        return (report(engine, Stop::LoadError(error.to_string()), &cpu), kernel);
    }
    cpu.regs.pc = crate::DEFAULT_BASE_ADDR;
    execute(cpu, mmu, kernel, engine, limits, inputs)
}

fn run_nro(nro: &[u8], path: &str, args: &[&str], engine: Engine,
    limits: &Limits, inputs: &[InputCheckpoint]) -> Report {
    run_nro_state(nro, path, args, engine, limits, inputs).0
}

fn run_path(path: &Path, engine: Engine, limits: &Limits) -> Report {
    match std::fs::read(path) {
        Ok(nro) => run_nro(&nro, &path.to_string_lossy(), &[], engine, limits, &[]),
        Err(error) => report(engine, Stop::MissingInput(format!("{}: {error}", path.display())), &Cpu::new()),
    }
}

/// Flight recorder: one small JSON summary of a run, so a failure can be diagnosed without logs.
/// `first_failure` is the first of: CPU fault, unhandled SVC, IPC command answered "not
/// implemented", unhandled nvdrv ioctl, GPU diagnostic, guest break, no VI frame queued.
fn flight_record(report: &Report, kernel: &HorizonKernel) -> serde_json::Value {
    use serde_json::json;
    let ipc = &kernel.ipc;
    let hex = |v: u64| format!("{v:#x}");
    let stop = format!("{:?}", report.stop);
    let first_failure = match &report.stop {
        Stop::Fault { pc, instruction, error } => json!({"kind": "cpu_fault", "pc": hex(*pc),
            "opcode": instruction.map(|i| format!("{i:#010x}")), "error": error}),
        Stop::UnhandledSvc(id) => json!({"kind": "unhandled_svc", "svc": hex(*id as u64), "pc": hex(report.pc)}),
        _ => match (ipc.unimplemented.first(), ipc.nvdrv.unhandled.first(), ipc.nvdrv.gpu_diagnostics.first()) {
            (Some((service, cmd)), _, _) => json!({"kind": "ipc_unimplemented", "service": service, "command": cmd}),
            (_, Some((device, ioctl)), _) => json!({"kind": "nvdrv_ioctl", "device": device, "ioctl": format!("{ioctl:#010x}")}),
            (_, _, Some(d)) => json!({"kind": "gpu", "diagnostic": format!("{d:?}")}),
            _ => match &report.stop {
                Stop::Break { reason, info1, info2, result } => json!({"kind": "guest_break", "reason": hex(*reason),
                    "info1": hex(*info1), "info2": hex(*info2), "result": result.map(|r| format!("{r:#x}")), "pc": hex(report.pc)}),
                _ if report.queued.is_empty() && report.raw_frame.is_none() => json!({"kind": "no_vi_frame",
                    "vi": ipc.vi_submission_diagnostic(), "first_present": report.first_present_diagnostic}),
                _ => serde_json::Value::Null,
            },
        },
    };
    json!({
        "engine": format!("{:?}", report.actual_engine),
        "stop": stop,
        "instructions": report.instructions,
        "pc": hex(report.pc),
        "first_failure": first_failure,
        "last_svcs": report.last_svcs.iter().map(|(pc, id)| format!("{pc:#x}:{id:#x}")).collect::<Vec<_>>().join(" "),
        "frames_presented": report.frames,
        "frames_queued": ipc.vi_frames_queued(),
        "ipc_unimplemented": ipc.unimplemented,
        "nvdrv_unhandled": ipc.nvdrv.unhandled.iter().map(|(d, i)| format!("{d} {i:#010x}")).collect::<Vec<_>>(),
        "gpu_diagnostics": ipc.nvdrv.gpu_diagnostics.iter().take(16).map(|d| format!("{d:?}")).collect::<Vec<_>>(),
        "debug_output": report.debug_output.chars().take(2000).collect::<String>(),
        "raw_frame": report.raw_frame.as_ref().map(|f| json!({"width": f.width, "height": f.height,
            "hash": hex(f.rgba_fnv1a64), "center_rgba": f.center_rgba})),
    })
}

/// Writes `{commit, nro, nro_sha256, runs: [...]}` to `out`.
fn write_flight_record(out: &Path, nro_path: &Path, runs: &[serde_json::Value]) {
    use sha2::Digest;
    let nro = std::fs::read(nro_path).unwrap_or_default();
    let commit = std::env::var("GITHUB_SHA").ok().or_else(|| {
        std::process::Command::new("git").args(["rev-parse", "HEAD"]).output().ok()
            .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    });
    let doc = serde_json::json!({
        "commit": commit,
        "nro": nro_path.display().to_string(),
        "nro_sha256": format!("{:x}", sha2::Sha256::digest(&nro)),
        "runs": runs,
    });
    std::fs::write(out, serde_json::to_string_pretty(&doc).unwrap() + "\n").expect("write flight record");
    println!("[flight recorder] wrote {}", out.display());
}

fn synthetic(words: &[u32], engine: Engine, limits: &Limits) -> Report {
    let (mut cpu, mut mmu, kernel) = crate::setup_vm();
    let base = crate::DEFAULT_BASE_ADDR;
    mmu.map_range(base, 4096, Permission::READ_WRITE_EXECUTE);
    for (index, word) in words.iter().enumerate() { mmu.write_u32(base + index as u64 * 4, *word).unwrap(); }
    cpu.regs.pc = base;
    execute(cpu, mmu, kernel, engine, limits, &[]).0
}

#[test]
fn corpus_runner_returns_from_functions_but_exits_only_on_svc() {
    // BL callee; ExitProcess; MOV X0, #42; RET.
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let result = synthetic(&[0x94000002, 0xD40000E1, 0xD2800540, 0xD65F03C0], engine, &Limits::default());
        assert_eq!(result.stop, Stop::ExitProcess, "{result:?}");
        assert_eq!(result.x0, 42);
        assert_eq!(result.actual_engine, engine);
        assert_eq!(result.instructions, 4);
        assert_eq!(result.svcs.last(), Some(&(crate::DEFAULT_BASE_ADDR + 4, 7)));
    }
}

#[test]
fn corpus_runner_bounds_faults_and_cooperative_deadline() {
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let budget = Limits { instructions: 1000, ..Limits::default() };
        let looping = synthetic(&[0x14000000], engine, &budget);
        assert_eq!(looping.stop, Stop::InstructionBudget);
        assert!(looping.instructions >= budget.instructions);
        assert!(looping.instructions < budget.instructions + 1024);
        let deadline = Limits { host_time: Duration::ZERO, ..Limits::default() };
        let timed = synthetic(&[0x14000000], engine, &deadline);
        assert_eq!(timed.stop, Stop::HostTimeout);
        assert_eq!(timed.instructions, 0);
        let fault = synthetic(&[0xFFFFFFFF], engine, &Limits::default());
        assert!(matches!(fault.stop, Stop::Fault { .. } | Stop::CpuHalt(_)), "{fault:?}");
    }
}

#[test]
fn corpus_runner_records_breaks_and_missing_input_without_success() {
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let broken = synthetic(&[0xD40004C1], engine, &Limits::default());
        assert!(matches!(broken.stop, Stop::Break { .. }));
        assert_eq!(broken.compatibility, "not-assessed");
        let missing = run_path(Path::new("/ounce-deliberately-missing-corpus-input.nro"), engine, &Limits::default());
        assert!(matches!(missing.stop, Stop::MissingInput(_)));
    }
}

#[test]
fn corpus_runner_existing_libnx_fixtures_match_both_engines() {
    for (relative, args, expected) in [
        ("hello/hello.nro", vec!["fixture"], "ounce-fixture result=0x00000EEE"),
        ("services/services.nro", vec![], "ounce-services result=0x00000EEE\n"),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nro").join(relative);
        let nro = std::fs::read(&path).unwrap();
        for engine in [Engine::Interpreter, Engine::Tier3] {
            let result = run_nro(&nro, "fixture.nro", &args, engine, &Limits::default(), &[]);
            assert_eq!(result.stop, Stop::ExitProcess, "{result:?}");
            assert!(result.debug_output.ends_with(expected), "{result:?}");
        }
    }
}


#[test]
fn corpus_runner_preempts_spinning_threads_and_preserves_kernel_switches() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nro/threads/threads.nro");
    let nro = std::fs::read(path).unwrap();
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let limits = Limits { host_time: Duration::from_secs(20), trace_entries: 8192, ..Limits::default() };
        let (result, kernel) = run_nro_state(&nro, "fixture.nro", &[], engine, &limits, &[]);
        assert_eq!(result.stop, Stop::ExitProcess, "{result:?}");
        assert!(result.debug_output.contains("ounce-threads total=0x00000384"), "{result:?}");
        assert!(result.debug_output.ends_with("ounce-threads result=0x00000EEE\n"), "{result:?}");
        assert!(result.preemptions > 0, "spinner requires time-slice preemption: {result:?}");
        assert!(kernel.scheduler.switches > 10);
        assert_eq!(kernel.scheduler.total_threads_completed, 3);
        assert!(result.svcs.iter().any(|s| s.1 == 0x1A), "workers contend on mutex");
        assert!(result.svcs.iter().any(|s| s.1 == 0x1C), "condvar wait");
        assert!(result.svcs.iter().any(|s| s.1 == 0x1D), "condvar signal");
    }
}

#[test]
fn corpus_runner_scripted_input_reaches_libnx_shared_memory() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nro/input/input.nro");
    let nro = std::fs::read(path).unwrap();
    let checkpoints = [
        InputCheckpoint { at_instructions: 0, input: NpadInput::default() },
        InputCheckpoint { at_instructions: 8, input: NpadInput {
            buttons: 1 | 1 << 14, stick_l: (0x1234, -0x100), stick_r: (0, 0) } },
    ];
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let result = run_nro(&nro, "fixture.nro", &[], engine, &Limits::default(), &checkpoints);
        assert_eq!(result.stop, Stop::ExitProcess, "{result:?}");
        assert_eq!(result.input_applied_at.len(), 2);
        assert_eq!(result.input_applied_at[0], 0);
        assert!(result.input_applied_at[1] >= 8, "Tier-3 may cross a slice target");
        assert!(result.debug_output.contains("ounce-input buttons=0x00004001\n"), "{result:?}");
        assert!(result.debug_output.contains("ounce-input stick_l=0x1234FF00\n"), "{result:?}");
        assert!(result.debug_output.ends_with("ounce-input result=0x00000EEE\n"), "{result:?}");
    }
}


#[test]
fn corpus_runner_hello_differential_slices() {
    let nro = super::fixture();
    let initialize = || {
        let (mut cpu, mut mmu, mut kernel) = crate::setup_vm();
        crate::loader::BootLoader::load_any_package(&nro, crate::DEFAULT_BASE_ADDR, &[0; 16], &mut mmu).unwrap();
        crate::setup_homebrew_nro(&mut cpu, &mut mmu, &mut kernel, "fixture.nro", &nro, &["fixture"]).unwrap();
        cpu.regs.pc = crate::DEFAULT_BASE_ADDR;
        (cpu, mmu, kernel)
    };
    let (mut cpu, mut mmu, mut kernel) = initialize();
    let (mut reference, mut reference_mmu, mut reference_kernel) = initialize();
    let mut engine = Tier3JitEngine::new();
    for _ in 0..crate::NRO_INSTRUCTION_BUDGET {
        let entry_pc = cpu.regs.pc;
        let (count, exit) = engine.run_slice(&mut cpu, &mut mmu, 1);
        let mut trail = Vec::new();
        let mut reference_svc = None;
        for _ in 0..count {
            let pc = reference.regs.pc;
            trail.push((pc, reference_mmu.read_u32(pc).ok()));
            match reference.step(&mut reference_mmu).unwrap() {
                StepResult::Continue => {}
                StepResult::Svc(id) => reference_svc = Some(id),
                other => panic!("reference stopped {other:?} at {pc:X}"),
            }
        }
        for reg in 0..31 {
            assert_eq!(cpu.regs.read_x(reg), reference.regs.read_x(reg),
                "X{reg} diverged entering {entry_pc:X}, count={count}, exit={exit:?}, trail={trail:X?}");
        }
        assert_eq!(cpu.regs.sp, reference.regs.sp, "SP entry={entry_pc:X}, trail={trail:X?}");
        assert_eq!(cpu.regs.pc, reference.regs.pc, "PC entry={entry_pc:X}, trail={trail:X?}");
        assert_eq!(format!("{:?}", cpu.regs.pstate), format!("{:?}", reference.regs.pstate), "flags entry={entry_pc:X}");
        if let BlockExit::Svc { svc_id } = exit {
            assert_eq!(reference_svc, Some(svc_id));
            let ref_out = reference_kernel.dispatch_svc(svc_id, &mut reference, &mut reference_mmu);
            let out = kernel.dispatch_svc(svc_id, &mut cpu, &mut mmu);
            assert_eq!(std::mem::discriminant(&out), std::mem::discriminant(&ref_out));
            // GetInfo entropy and clock results may differ between kernels. Use identical
            // syscall outputs so this test isolates CPU execution, not host timing.
            cpu.regs = reference.regs.clone();
            if out == SvcResult::ExitProcess { return; }
        } else {
            assert!(reference_svc.is_none());
        }
    }
    panic!("differential run did not terminate");
}

/// Explicit opt-in diagnostic. It does not assert compatibility. Run the enclosing test process
/// under an outer timeout as a cooperative deadline cannot interrupt a blocking kernel SVC.
#[test]
#[ignore = "requires OUNCE_CORPUS_NRO pointing at an unchanged external sample"]
fn external_corpus_nro_probe_both_engines() {
    let path = std::env::var_os("OUNCE_CORPUS_NRO").map(std::path::PathBuf::from);
    let mut reference_frame = None;
    let record = std::env::var_os("OUNCE_REPORT_JSON").map(std::path::PathBuf::from);
    let mut runs = Vec::new();
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let result = match path.as_ref().map(|p| (p, std::fs::read(p))) {
            Some((p, Ok(nro))) => {
                let (result, kernel) = run_nro_state(&nro, &p.to_string_lossy(), &[], engine, &Limits::default(), &[]);
                runs.push(flight_record(&result, &kernel));
                result
            }
            Some((p, Err(error))) => report(engine, Stop::MissingInput(format!("{}: {error}", p.display())), &Cpu::new()),
            None => report(engine, Stop::MissingInput("OUNCE_CORPUS_NRO is unset".into()), &Cpu::new()),
        };
        if let (Some(out), Some(p)) = (&record, &path) {
            write_flight_record(out, p, &runs);
        }
        println!("{result:#?}");
        assert!(!matches!(result.stop, Stop::MissingInput(_) | Stop::LoadError(_)), "{result:?}");
        if engine == Engine::Interpreter {
            reference_frame = result.raw_frame.clone();
        } else {
            assert_eq!(result.raw_frame, reference_frame,
                "raw VI frame presence or content differs between engines");
        }
    }
}

/// The simplegfx first-frame oracle (`docs/first-frame-oracle.md`), criteria 3-6, on an NRO built
/// unchanged from the pinned source. Opt-in: `OUNCE_SIMPLEGFX_NRO=<path> cargo test --release
/// simplegfx_first_frame_oracle -- --ignored --nocapture`. `OUNCE_SIMPLEGFX_FRAMES` (default 61)
/// sets how many queued frames to run.
#[test]
#[ignore = "requires OUNCE_SIMPLEGFX_NRO pointing at simplegfx.nro built from the pinned source"]
fn simplegfx_first_frame_oracle() {
    let path = std::env::var_os("OUNCE_SIMPLEGFX_NRO").expect("OUNCE_SIMPLEGFX_NRO is unset");
    let frames: u64 = std::env::var("OUNCE_SIMPLEGFX_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(61);
    // Hash with alpha forced to 0xFF, from the oracle table.
    let expected_hash = |i: u64| match i {
        1 => Some(0xd95ddf22e46ba325u64),
        2 => Some(0x1dd22fb233d7a325),
        60 => Some(0xa50b597b02e4a325),
        61 => Some(0x52bda05e66a4a325),
        _ => None,
    };
    let limits = Limits { instructions: 4_000_000_000, host_time: Duration::from_secs(900), frames: usize::MAX,
        queued_frames: frames, ..Limits::default() };
    let mut reference: Option<Vec<QueuedFrame>> = None;
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let result = run_path(Path::new(&path), engine, &limits);
        println!("{engine:?}: stop={:?} instructions={} queued={} pc={:#x}", result.stop, result.instructions, result.queued.len(), result.pc);
        // Criterion 6: stopped by the runner's frame limit, nothing else.
        assert!(matches!(result.stop, Stop::FrameLimit), "{engine:?} stopped with {:?}", result.stop);
        assert_eq!(result.queued.len() as u64, frames);
        for f in &result.queued {
            // Criteria 3-4: 1280x720, every pixel 4 * (i mod 61) in R, G, B (alpha forced to FF).
            let v = (4 * (f.index % 61)) as u8;
            assert_eq!(f.error, None, "frame {}", f.index);
            assert_eq!((f.width, f.height), (1280, 720), "frame {}", f.index);
            assert_eq!(f.uniform, Some([v, v, v, 0xff]), "frame {}", f.index);
            if let Some(h) = expected_hash(f.index) {
                assert_eq!(f.rgba_fnv1a64, h, "frame {} hash", f.index);
            }
        }
        // Criterion 5: both engines give identical frames.
        match &reference {
            None => reference = Some(result.queued),
            Some(r) => assert_eq!(&result.queued, r, "interpreter and Tier-3 frames differ"),
        }
    }
}

/// The deko_basic first-frame oracle (`docs/deko-basic-oracle.md`), criteria 3-6, on an NRO built
/// unchanged from the pinned source. Opt-in: `OUNCE_DEKO_BASIC_NRO=<path> cargo test --release
/// deko_basic_first_frame_oracle -- --ignored --nocapture`. `OUNCE_DEKO_BASIC_FRAMES` (default 5)
/// sets how many queued frames to run.
#[test]
#[ignore = "requires OUNCE_DEKO_BASIC_NRO pointing at deko_basic.nro built from the pinned source"]
fn deko_basic_first_frame_oracle() {
    let path = std::env::var_os("OUNCE_DEKO_BASIC_NRO").expect("OUNCE_DEKO_BASIC_NRO is unset");
    let nro_bytes = std::fs::read(&path).expect("read deko_basic NRO binary");
    use sha2::Digest;
    let nro_sha256 = format!("{:x}", sha2::Sha256::digest(&nro_bytes));
    // Canonical CI toolchain build (GCC 15.2.0, binutils 2.45.1) vs local build variant (GCC 16.1.0).
    const KNOWN_VALID_SHA256: &[&str] = &[
        "4f2ea875311acc46e5cef0632b2031091c72787aca64e676d91eaadf6cee5dfa", // devkitpro/devkita64:latest build A
        "5cec608c6358d5aa5d110c5f5d98674e9ba46685752c497d567e2d286f8766e9", // devkitpro/devkita64:latest build B (romfs inode timestamp variation)
        "5d108784619f6830952f7a7342ab5079c678d3d09521659204a83ac45a52b696", // local devkitA64 GCC 16.1.0 build
    ];
    assert!(
        KNOWN_VALID_SHA256.contains(&nro_sha256.as_str()),
        "unexpected deko_basic NRO SHA-256: {nro_sha256} (expected one of {KNOWN_VALID_SHA256:?})"
    );
    println!("deko_basic NRO SHA-256 verified: {nro_sha256}");
    let frames: u64 = std::env::var("OUNCE_DEKO_BASIC_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    let expected_hash = |i: u64| match i {
        1 => 0x7c9a9a3fa3856325u64,
        _ => 0x3a00ccf6b6f28ea1u64,
    };
    let limits = Limits { instructions: 4_000_000_000, host_time: Duration::from_secs(900), frames: usize::MAX,
        queued_frames: frames, ..Limits::default() };
    let mut reference: Option<Vec<QueuedFrame>> = None;
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let result = run_path(Path::new(&path), engine, &limits);
        println!("{engine:?}: stop={:?} instructions={} queued={} pc={:#x}", result.stop, result.instructions, result.queued.len(), result.pc);
        // Criterion 7: stopped by the runner's frame limit.
        assert!(matches!(result.stop, Stop::FrameLimit), "{engine:?} stopped with {:?}", result.stop);
        assert_eq!(result.queued.len() as u64, frames);
        for f in &result.queued {
            assert_eq!(f.error, None, "frame {}", f.index);
            assert_eq!((f.width, f.height), (1280, 720), "frame {}", f.index);
            assert_eq!(f.rgba_fnv1a64, expected_hash(f.index), "frame {} hash mismatch", f.index);
            if f.index == 1 {
                assert_eq!(f.uniform, Some([32, 75, 122, 255]), "frame 1 must be clear color");
            } else {
                assert_eq!(f.uniform, None, "rendered frames must contain non-uniform pixels");
            }
        }
        // Criterion 6: both engines give identical frames.
        match &reference {
            None => reference = Some(result.queued),
            Some(r) => assert_eq!(&result.queued, r, "interpreter and Tier-3 frames differ"),
        }
    }
}
