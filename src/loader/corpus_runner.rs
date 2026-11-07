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
enum Engine { Interpreter, Tier3, Native }

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

#[derive(Debug, Clone, Copy)]
struct FrameInput { after_queued_frame: u64, buttons: u64 }

fn parse_frame_inputs(value: &serde_json::Value, frames: u64) -> Result<Vec<FrameInput>, String> {
    let entries = value.as_array().filter(|a| !a.is_empty()).ok_or("inputs must be non-empty")?;
    let mut inputs = Vec::new();
    let mut previous = None;
    for entry in entries {
        let object = entry.as_object().ok_or("input must be an object")?;
        if object.len() != 2 { return Err("input requires exactly after_queued_frame and buttons".into()); }
        let frame = entry["after_queued_frame"].as_u64().filter(|n| *n < frames)
            .ok_or("input must precede the final queued frame")?;
        let buttons = entry["buttons"].as_u64().ok_or("buttons must be an unsigned integer")?;
        if previous.is_some_and(|p| frame <= p) { return Err("inputs must strictly increase".into()); }
        if previous.is_none() && (frame != 0 || buttons != 0) { return Err("input must start neutral at frame zero".into()); }
        inputs.push(FrameInput { after_queued_frame: frame, buttons });
        previous = Some(frame);
    }
    if inputs.last().unwrap().buttons != 0 { return Err("input must end with release".into()); }
    Ok(inputs)
}

#[test]
fn frame_input_script_rejects_ambiguous_or_unreachable_transitions() {
    use serde_json::json;
    let script = json!([
        {"after_queued_frame": 0, "buttons": 0},
        {"after_queued_frame": 2, "buttons": 32768},
        {"after_queued_frame": 3, "buttons": 0},
        {"after_queued_frame": 4, "buttons": 1},
        {"after_queued_frame": 5, "buttons": 0}
    ]);
    assert!(parse_frame_inputs(&script, 8).is_ok());
    for invalid in [json!([]), json!([{"after_queued_frame": 1, "buttons": 0}]),
        json!([{"after_queued_frame": 0, "buttons": 0}, {"after_queued_frame": 0, "buttons": 1}]),
        json!([{"after_queued_frame": 0, "buttons": 0}, {"after_queued_frame": 8, "buttons": 0}]),
        json!([{"after_queued_frame": 0, "buttons": 0}, {"after_queued_frame": 2, "buttons": 1}]),
        json!([{"after_queued_frame": 0, "buttons": -1}]),
        json!([{"after_queued_frame": 0, "buttons": 0, "butons": 1}])] {
        assert!(parse_frame_inputs(&invalid, 8).is_err(), "accepted {invalid}");
    }
}

#[test]
fn registered_interactions_have_reachable_inputs_and_pixel_checks() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../../tests/corpus/interactions.json")).unwrap();
    for oracle in manifest["oracles"].as_array().unwrap() {
        validate_frame_oracle(oracle).unwrap();
        parse_frame_inputs(&oracle["inputs"], oracle["queued_frames"].as_u64().unwrap()).unwrap();
    }
}

struct Limits {
    instructions: usize,
    host_time: Duration,
    frames: usize,
    trace_entries: usize,
    debug_bytes: usize,
    /// Stop after this many VI `QUEUE_BUFFER`s (0 = no limit). Every queued frame is captured.
    queued_frames: u64,
    /// Pixels (x, y) sampled from every queued frame.
    probes: Vec<(usize, usize)>,
    frame_inputs: Vec<FrameInput>,
}

impl Default for Limits {
    fn default() -> Self {
        Self { instructions: crate::NRO_INSTRUCTION_BUDGET, host_time: Duration::from_secs(10),
            frames: 16, trace_entries: 256, debug_bytes: 8192, queued_frames: 0, probes: Vec::new(), frame_inputs: Vec::new() }
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
    /// SVCs taken (native execution counts these instead of instructions).
    svc_count: usize,
    svcs: Vec<(u64, u16)>,
    /// The last 32 SVCs (pc, id), oldest first.
    last_svcs: std::collections::VecDeque<(u64, u16)>,
    debug_output: String,
    input_applied_at: Vec<usize>,
    /// Actual (queued-frame count, buttons, instruction count) for each transition.
    frame_inputs_applied: Vec<(u64, u64, usize)>,
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
    /// RGBA at each `Limits::probes` coordinate (alpha forced to 0xFF), `None` if out of range.
    probes: Vec<Option<[u8; 4]>>,
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
        preemptions: 0, svc_count: 0, svcs: Vec::new(), last_svcs: Default::default(), debug_output: String::new(),
        input_applied_at: Vec::new(), frame_inputs_applied: Vec::new(), raw_frame: None, queued: Vec::new(), first_present_diagnostic: None, post_queue_diagnostic: None, compatibility: "not-assessed" }
}

fn capture_queued(kernel: &HorizonKernel, mmu: &Memory, index: u64, probes: &[(usize, usize)]) -> QueuedFrame {
    let mut out = QueuedFrame { index, width: 0, height: 0, rgba_fnv1a64: 0, uniform: None, probes: Vec::new(), error: None };
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
            let frame = match kernel.ipc.submitted_vi_crop() {
                Some((l, t, r, b)) => frame.cropped((l as usize, t as usize, r as usize, b as usize)),
                None => frame,
            };
            let mut px = frame.rgba.chunks_exact(4);
            let first: Option<[u8; 4]> = px.next().map(|p| p.try_into().unwrap());
            out.uniform = first.filter(|f| px.all(|p| p == f));
            out.width = frame.width;
            out.height = frame.height;
            out.rgba_fnv1a64 = frame.content_hash();
            out.probes = probes.iter().map(|&(x, y)| frame.pixel(x, y).map(|(r, g, b, a)| [r, g, b, a])).collect();
        }
        Err(error) => out.error = Some(format!("{error:?}")),
    }
    out
}

#[cfg(feature = "nce")]
type NativeEngine = crate::cpu::nce::NceEngine;
#[cfg(not(feature = "nce"))]
enum NativeEngine {}

/// The native engine for a VM set up by `run_nro_state` (layout at `kernel.addr_base`).
#[cfg(feature = "nce")]
fn native_engine(mmu: &mut Memory, kernel: &HorizonKernel) -> Result<NativeEngine, String> {
    let code = kernel.addr_base + crate::DEFAULT_BASE_ADDR;
    // The NRO header (after the entry branch) is data.
    NativeEngine::new(mmu, kernel.addr_base, vec![(code + 4, code + 0x80)])
}

#[cfg(not(feature = "nce"))]
fn native_engine(_mmu: &mut Memory, _kernel: &HorizonKernel) -> Result<NativeEngine, String> {
    Err("native execution needs a build with `--features nce`".into())
}

/// Runs the guest natively up to its next SVC.
#[cfg(feature = "nce")]
fn native_run(nce: &mut NativeEngine, cpu: &mut Cpu, mmu: &mut Memory, kernel: &HorizonKernel) -> Result<Option<u16>, Stop> {
    use crate::cpu::nce::NceExit;
    match nce.run(cpu, mmu, &|| kernel.system_tick()) {
        NceExit::Svc(id) => Ok(Some(id)),
        NceExit::Preempted => Ok(None),
        NceExit::Fault { pc, signal, addr } => Err(Stop::Fault { pc, instruction: mmu.read_u32(pc).ok(),
            error: format!("native signal {signal} (fault address {addr:#x})") }),
        NceExit::Error(error) => Err(Stop::Fault { pc: cpu.regs.pc, instruction: None, error: format!("native engine: {error}") }),
    }
}

#[cfg(not(feature = "nce"))]
fn native_run(nce: &mut NativeEngine, _cpu: &mut Cpu, _mmu: &mut Memory, _kernel: &HorizonKernel) -> Result<Option<u16>, Stop> {
    match *nce {}
}

/// Guest memory, layout base and alias region size for `engine`: native execution needs its window.
fn guest_memory(engine: Engine) -> Result<(Memory, u64, u64), String> {
    match engine {
        #[cfg(feature = "nce")]
        Engine::Native => {
            let arena = crate::cpu::nce::reserve_window()?;
            let base = arena.base();
            Ok((Memory::with_arena(arena), base, crate::cpu::nce::ALIAS_SIZE))
        }
        #[cfg(not(feature = "nce"))]
        Engine::Native => Err("native execution needs a build with `--features nce`".into()),
        _ => Ok((Memory::new(), 0, 0x10_0000_0000)),
    }
}

/// Run an already configured VM, retaining the real kernel dispatcher and scheduler.
/// Tier-3 can overshoot a slice target by one super-block; report the actual instruction total.
fn execute(mut cpu: Cpu, mut mmu: Memory, mut kernel: HorizonKernel, engine: Engine,
    limits: &Limits, inputs: &[InputCheckpoint]) -> (Report, HorizonKernel) {
    let mut result = report(engine, Stop::InstructionBudget, &cpu);
    let started = Instant::now();
    let mut tier3 = match engine { Engine::Tier3 => Some(Tier3JitEngine::new()), _ => None };
    let mut native = match engine {
        Engine::Native => match native_engine(&mut mmu, &kernel) {
            Ok(e) => Some(e),
            Err(error) => { result.stop = Stop::LoadError(error); return (result, kernel); }
        },
        _ => None,
    };
    let quantum = crate::kernel::scheduler::PREEMPT_INTERVAL;
    let mut next_preemption = quantum;
    let mut next_input = 0;
    let mut next_frame_input = 0;
    'run: loop {
        if started.elapsed() >= limits.host_time { result.stop = Stop::HostTimeout; break; }
        if result.instructions >= limits.instructions { break; }
        if let Some(input) = limits.frame_inputs.get(next_frame_input)
            && input.after_queued_frame <= kernel.ipc.vi_frames_queued()
        {
            kernel.ipc.hid.input = NpadInput { buttons: input.buttons, ..Default::default() };
            kernel.ipc.hid.sync(&mut mmu);
            result.frame_inputs_applied.push((kernel.ipc.vi_frames_queued(), input.buttons, result.instructions));
            next_frame_input += 1;
        }
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
        if let Some(nce) = native.as_mut() {
            match native_run(nce, &mut cpu, &mut mmu, &kernel) {
                Ok(Some(id)) => { svc = Some(id); result.svc_count += 1; }
                // Time slice over: same as the instruction quantum of the other engines.
                Ok(None) => { kernel.scheduler.preempt(&mut cpu); result.preemptions += 1; }
                Err(stop) => { result.stop = stop; break; }
            }
        } else {
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
            result.queued.push(capture_queued(&kernel, &mmu, queued, &limits.probes));
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

/// Wall-clock date the corpus runner gives guests (2024-01-01 00:00:00 UTC).
const DETERMINISTIC_UNIX_SECS: u64 = 1_704_067_200;

fn run_nro_state(nro: &[u8], path: &str, args: &[&str], engine: Engine,
    limits: &Limits, inputs: &[InputCheckpoint]) -> (Report, HorizonKernel) {
    let (memory, base, alias_size) = match guest_memory(engine) {
        Ok(m) => m,
        Err(error) => { let (cpu, _, kernel) = crate::setup_vm(); return (report(engine, Stop::LoadError(error), &cpu), kernel); }
    };
    let (mut cpu, mut mmu, mut kernel) = crate::setup_vm_in(memory, base, alias_size);
    kernel.set_deterministic_clock(DETERMINISTIC_UNIX_SECS);
    let loaded = super::super::BootLoader::load_any_package(nro, base + crate::DEFAULT_BASE_ADDR, &[0; 16], &mut mmu);
    let error = match loaded {
        Ok(info) if info.format == "NRO0" => None,
        Ok(_) => Some("external runner accepts NRO0 only".into()),
        Err(error) => Some(error.to_string()),
    };
    if let Some(error) = error { return (report(engine, Stop::LoadError(error), &cpu), kernel); }
    if let Err(error) = crate::setup_homebrew_nro(&mut cpu, &mut mmu, &mut kernel, path, nro, args) {
        return (report(engine, Stop::LoadError(error.to_string()), &cpu), kernel);
    }
    cpu.regs.pc = base + crate::DEFAULT_BASE_ADDR;
    execute(cpu, mmu, kernel, engine, limits, inputs)
}

fn run_nro(nro: &[u8], path: &str, args: &[&str], engine: Engine,
    limits: &Limits, inputs: &[InputCheckpoint]) -> Report {
    run_nro_state(nro, path, args, engine, limits, inputs).0
}

fn run_path(path: &Path, engine: Engine, limits: &Limits) -> Report {
    match std::fs::read(path) {
        Ok(nro) => run_nro(&nro, &path.to_string_lossy(), &[], engine, limits, &[]),
        Err(error) => report(
            engine,
            Stop::MissingInput(format!("{}: {error}", path.display())),
            &Cpu::new(),
        ),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
    let nro = std::fs::read(nro_path).unwrap_or_default();
    let commit = std::env::var("GITHUB_SHA").ok().or_else(|| {
        std::process::Command::new("git").args(["rev-parse", "HEAD"]).output().ok()
            .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    });
    let doc = serde_json::json!({
        "commit": commit,
        "nro": nro_path.display().to_string(),
        "nro_sha256": sha256_hex(&nro),
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

/// The same fixtures run natively on an AArch64 host (`--features nce`): rewritten SVCs, TLS
/// reads and x18 uses reach the same kernel and print the same result.
#[cfg(all(feature = "nce", target_arch = "aarch64", any(target_os = "macos", target_os = "linux")))]
#[test]
fn corpus_runner_native_runs_libnx_fixtures() {
    for (relative, args, expected) in [
        ("hello/hello.nro", vec!["fixture"], "ounce-fixture result=0x00000EEE"),
        ("services/services.nro", vec![], "ounce-services result=0x00000EEE\n"),
        ("threads/threads.nro", vec![], ""),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nro").join(relative);
        let nro = std::fs::read(&path).unwrap();
        let result = run_nro(&nro, "fixture.nro", &args, Engine::Native, &Limits::default(), &[]);
        println!("{relative}: stop={:?} svcs={} out={:?}", result.stop, result.svc_count, result.debug_output);
        assert_eq!(result.stop, Stop::ExitProcess, "{relative}: {result:?}");
        assert!(result.debug_output.ends_with(expected), "{relative}: {result:?}");
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
fn frame_input_runner_applies_startup_once_and_does_not_apply_future_frames() {
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let (mut cpu, mut mmu, kernel) = crate::setup_vm();
        let base = crate::DEFAULT_BASE_ADDR;
        mmu.map_range(base, 4096, Permission::READ_WRITE_EXECUTE);
        // Several slices before ExitProcess; none queues a VI frame.
        for i in 0..1024 { mmu.write_u32(base + i * 4, 0xD503201F).unwrap(); }
        mmu.write_u32(base + 4092, 0xD40000E1).unwrap();
        cpu.regs.pc = base;
        let limits = Limits { frame_inputs: vec![
            FrameInput { after_queued_frame: 0, buttons: 1 },
            FrameInput { after_queued_frame: 1, buttons: 0 },
        ], ..Limits::default() };
        let (result, kernel) = execute(cpu, mmu, kernel, engine, &limits, &[]);
        assert_eq!(result.stop, Stop::ExitProcess);
        assert_eq!(result.frame_inputs_applied, vec![(0, 1, 0)]);
        assert_eq!(kernel.ipc.hid.input.buttons, 1);
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

/// Engines an oracle runs: the interpreter and Tier-3, or with `OUNCE_CPU=native` (`--cpu=native`)
/// Tier-3, the reference and speed baseline, then native execution.
fn oracle_engines() -> Vec<Engine> {
    if crate::cpu::CpuTier::from_env() == crate::cpu::CpuTier::Native {
        vec![Engine::Tier3, Engine::Native]
    } else {
        vec![Engine::Interpreter, Engine::Tier3]
    }
}

/// Prints each engine's wall time for the whole run (load, guest code, kernel, frame capture) and
/// native execution's speedup over Tier-3 when both ran.
fn report_speed(times: &[(Engine, Duration)]) {
    for (engine, time) in times {
        println!("[speed] {engine:?}: {:.3} s", time.as_secs_f64());
    }
    let of = |e: Engine| times.iter().find(|(x, _)| *x == e).map(|(_, t)| t.as_secs_f64());
    if let (Some(tier3), Some(native)) = (of(Engine::Tier3), of(Engine::Native)) {
        println!("[speed] native speedup over Tier-3: {:.1}x ({tier3:.3} s -> {native:.3} s)", tier3 / native);
    }
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
    // The ordinary GUI runner must present the same guest pixels, not its legacy demo VRAM.
    // Headless is the presentation backend here so this route is exercised in CI too.
    {
        let nro = std::fs::read(&path).unwrap();
        let (mut cpu, mut memory, mut kernel) = crate::setup_vm();
        super::super::BootLoader::load_any_package(&nro, crate::DEFAULT_BASE_ADDR, &[0; 16], &mut memory).unwrap();
        crate::setup_homebrew_nro(&mut cpu, &mut memory, &mut kernel, "simplegfx.nro", &nro, &[]).unwrap();
        cpu.regs.pc = crate::DEFAULT_BASE_ADDR;
        let mut display = crate::gpu::DisplayServer::new(crate::gpu::display::DEFAULT_FRAMEBUFFER_ADDR,
            crate::gpu::DisplayConfig { backend: crate::gpu::BackendKind::Headless, ..Default::default() });
        let mut hid = crate::hid::HidServer::new(crate::HID_SHARED_MEM_ADDR);
        let result = crate::run_vm_internal(cpu, memory, kernel, Some(&mut display), Some(&mut hid), 100_000_000, 2);
        assert_eq!(result.stop, crate::VmStop::FrameLimit, "ordinary GUI runner did not present the queued frames");
        assert_eq!(display.swapchain.vsync_count, 2);
        assert_eq!(display.swapchain.front_buffer.content_hash(), 0x1dd22fb233d7a325,
            "GUI runner must show the oracle's second guest frame without a telemetry overlay");
    }
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
    let mut times = Vec::new();
    for engine in oracle_engines() {
        let started = Instant::now();
        let result = run_path(Path::new(&path), engine, &limits);
        times.push((engine, started.elapsed()));
        println!("{engine:?}: stop={:?} instructions={} svcs={} preemptions={} queued={} pc={:#x}", result.stop, result.instructions, result.svc_count, result.preemptions, result.queued.len(), result.pc);
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
            Some(r) => assert_eq!(&result.queued, r, "{engine:?} frames differ from {:?}", times[0].0),
        }
    }
    report_speed(&times);
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
    let nro_sha256 = sha256_hex(&nro_bytes);
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
    // Every frame draws the triangle (frame 1 corrected in PLAN step 3, see docs/deko-basic-oracle.md).
    let expected_hash = |_: u64| 0x3a00ccf6b6f28ea1u64;
    let limits = Limits { instructions: 4_000_000_000, host_time: Duration::from_secs(900), frames: usize::MAX,
        queued_frames: frames, ..Limits::default() };
    let mut reference: Option<Vec<QueuedFrame>> = None;
    let mut times = Vec::new();
    for engine in oracle_engines() {
        let started = Instant::now();
        let result = run_path(Path::new(&path), engine, &limits);
        times.push((engine, started.elapsed()));
        println!("{engine:?}: stop={:?} instructions={} svcs={} preemptions={} queued={} pc={:#x}", result.stop, result.instructions, result.svc_count, result.preemptions, result.queued.len(), result.pc);
        // Criterion 7: stopped by the runner's frame limit.
        assert!(matches!(result.stop, Stop::FrameLimit), "{engine:?} stopped with {:?}", result.stop);
        assert_eq!(result.queued.len() as u64, frames);
        for f in &result.queued {
            assert_eq!(f.error, None, "frame {}", f.index);
            assert_eq!((f.width, f.height), (1280, 720), "frame {}", f.index);
            assert_eq!(f.rgba_fnv1a64, expected_hash(f.index), "frame {} hash mismatch", f.index);
            assert_eq!(f.uniform, None, "rendered frames must contain non-uniform pixels");
        }
        // Criterion 6: both engines give identical frames.
        match &reference {
            None => reference = Some(result.queued),
            Some(r) => assert_eq!(&result.queued, r, "{engine:?} frames differ from {:?}", times[0].0),
        }
    }
    report_speed(&times);
}

/// The romfs first-frame oracle (`docs/romfs-oracle.md`) on an NRO built unchanged from the pinned source.
/// Opt-in: `OUNCE_ROMFS_NRO=<path> cargo test --release romfs_first_frame_oracle -- --ignored --nocapture`.
#[test]
#[ignore = "requires OUNCE_ROMFS_NRO pointing at romfs.nro built from the pinned source"]
fn romfs_first_frame_oracle() {
    let path = std::env::var_os("OUNCE_ROMFS_NRO").expect("OUNCE_ROMFS_NRO is unset");
    let frames: u64 = std::env::var("OUNCE_ROMFS_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    let limits = Limits {
        instructions: 4_000_000_000,
        host_time: Duration::from_secs(900),
        frames: usize::MAX,
        queued_frames: frames,
        ..Limits::default()
    };
    let mut reference: Option<Vec<QueuedFrame>> = None;
    let mut times = Vec::new();
    for engine in oracle_engines() {
        let started = Instant::now();
        let result = run_path(Path::new(&path), engine, &limits);
        times.push((engine, started.elapsed()));
        println!("{engine:?}: stop={:?} instructions={} svcs={} preemptions={} queued={} pc={:#x}", result.stop, result.instructions, result.svc_count, result.preemptions, result.queued.len(), result.pc);
        assert!(matches!(result.stop, Stop::FrameLimit), "{engine:?} stopped with {:?}", result.stop);
        assert_eq!(result.queued.len() as u64, frames);
        for f in &result.queued {
            assert_eq!(f.error, None, "frame {}", f.index);
            assert_eq!((f.width, f.height), (1280, 720), "frame {}", f.index);
            assert_ne!(f.rgba_fnv1a64, 0, "frame {} must have non-zero hash", f.index);
        }
        match &reference {
            None => reference = Some(result.queued),
            Some(r) => assert_eq!(&result.queued, r, "{engine:?} frames differ from {:?}", times[0].0),
        }
    }
    report_speed(&times);
}

/// The startup (templates/application) first-frame oracle (`docs/startup-oracle.md`) on an NRO built unchanged from the pinned source.
/// Opt-in: `OUNCE_STARTUP_NRO=<path> cargo test --release startup_first_frame_oracle -- --ignored --nocapture`.
#[test]
#[ignore = "requires OUNCE_STARTUP_NRO pointing at application.nro built from the pinned source"]
fn startup_first_frame_oracle() {
    let path = std::env::var_os("OUNCE_STARTUP_NRO").expect("OUNCE_STARTUP_NRO is unset");
    let frames: u64 = std::env::var("OUNCE_STARTUP_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    let limits = Limits {
        instructions: 4_000_000_000,
        host_time: Duration::from_secs(900),
        frames: usize::MAX,
        queued_frames: frames,
        ..Limits::default()
    };
    let mut reference: Option<Vec<QueuedFrame>> = None;
    let mut times = Vec::new();
    for engine in oracle_engines() {
        let started = Instant::now();
        let result = run_path(Path::new(&path), engine, &limits);
        times.push((engine, started.elapsed()));
        println!("{engine:?}: stop={:?} instructions={} svcs={} preemptions={} queued={} pc={:#x}", result.stop, result.instructions, result.svc_count, result.preemptions, result.queued.len(), result.pc);
        assert!(matches!(result.stop, Stop::FrameLimit), "{engine:?} stopped with {:?}", result.stop);
        assert_eq!(result.queued.len() as u64, frames);
        for f in &result.queued {
            assert_eq!(f.error, None, "frame {}", f.index);
            assert_eq!((f.width, f.height), (1280, 720), "frame {}", f.index);
            assert_ne!(f.rgba_fnv1a64, 0, "frame {} must have non-zero hash", f.index);
        }
        match &reference {
            None => reference = Some(result.queued),
            Some(r) => assert_eq!(&result.queued, r, "{engine:?} frames differ from {:?}", times[0].0),
        }
    }
    report_speed(&times);
}

/// Reject vacuous and unreachable pixel assertions before executing a workload.
fn validate_frame_oracle(oracle: &serde_json::Value) -> Result<(), String> {
    let frames = oracle["queued_frames"].as_u64().filter(|n| *n > 0)
        .ok_or("queued_frames must be positive")?;
    let checks = oracle["checks"].as_array().filter(|v| !v.is_empty())
        .ok_or("checks must be non-empty")?;
    for (number, check) in checks.iter().enumerate() {
        let validate = || -> Result<(), String> {
            let indices = check["frames"].as_array().filter(|v| !v.is_empty())
                .ok_or("frames must be non-empty")?;
            let mut seen = std::collections::HashSet::new();
            for index in indices {
                let index = index.as_u64().filter(|n| *n > 0 && *n <= frames)
                    .ok_or("checked frame must be within 1..=queued_frames")?;
                if !seen.insert(index) { return Err("duplicate checked frame".into()); }
            }
            let expect = &check["expect"];
            let width = expect["width"].as_u64().or(check["width"].as_u64())
                .filter(|n| *n > 0).ok_or("width must be positive")?;
            let height = expect["height"].as_u64().or(check["height"].as_u64())
                .filter(|n| *n > 0).ok_or("height must be positive")?;
            let hash = expect.get("rgba_fnv1a64");
            if let Some(hash) = hash {
                let hash = hash.as_str().ok_or("frame hash must be a string")?;
                if hash.len() != 18 || !hash.starts_with("0x")
                    || !hash[2..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                    return Err("frame hash must be 0x plus 16 lowercase hex digits".into());
                }
            }
            let probes = match check.get("probes") {
                Some(value) => value.as_array().ok_or("probes must be an array")?.as_slice(),
                None => &[],
            };
            if hash.is_none() && probes.is_empty() {
                return Err("check needs a frame hash or pixel probes".into());
            }
            for probe in probes {
                if probe["x"].as_u64().filter(|x| *x < width).is_none()
                    || probe["y"].as_u64().filter(|y| *y < height).is_none() {
                    return Err("probe must lie inside the expected frame".into());
                }
                let rgb = probe["rgb"].as_array().ok_or("probe rgb must be an array")?;
                if rgb.len() != 3 || rgb.iter().any(|v| v.as_u64().filter(|n| *n <= 255).is_none()) {
                    return Err("probe rgb needs three integer channels in 0..=255".into());
                }
            }
            Ok(())
        };
        validate().map_err(|error| format!("check {number}: {error}"))?;
    }
    Ok(())
}

#[test]
fn registered_frame_oracles_have_reachable_pixel_assertions() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../../tests/corpus/oracles.json")).unwrap();
    for oracle in manifest["oracles"].as_array().unwrap() {
        validate_frame_oracle(oracle).unwrap_or_else(|error| panic!("oracle {}: {error}", oracle["id"]));
    }
}

#[test]
fn frame_oracles_reject_vacuous_and_unreachable_checks() {
    use serde_json::json;
    let valid = json!({"queued_frames": 1, "checks": [{"frames": [1],
        "expect": {"width": 2, "height": 1}, "probes": [{"x": 1, "y": 0, "rgb": [0, 128, 255]}]}]});
    assert!(validate_frame_oracle(&valid).is_ok());
    let mutations = [
        ("/queued_frames", json!(0)), ("/checks", json!([])),
        ("/checks/0/frames", json!([])), ("/checks/0/frames", json!([0])),
        ("/checks/0/frames", json!([2])), ("/checks/0/frames", json!([1, 1])),
        ("/checks/0/probes", json!([])), ("/checks/0/probes/0/x", json!(2)),
        ("/checks/0/probes/0/rgb", json!([0, 0, 256])),
        ("/checks/0/expect/width", json!(0)),
    ];
    for (path, value) in mutations {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(validate_frame_oracle(&invalid).is_err(), "accepted {invalid}");
    }
    let mut invalid = valid;
    invalid["checks"][0]["expect"]["rgba_fnv1a64"] = json!("bad-hash");
    assert!(validate_frame_oracle(&invalid).is_err());
}

/// One pre-registered first-frame oracle from `tests/corpus/oracles.json` (PLAN step 2,
/// `docs/corpus-oracles.md`). Its expected values were computed from the program's source by
/// `tools/corpus_oracle.py` before any run. Opt-in: `OUNCE_CORPUS_ROOT=<builds> OUNCE_ORACLE_ID=<id>
/// cargo test --release corpus_first_frame_oracle -- --ignored --nocapture`. The NRO is read from
/// `<builds>/<build>/build/source/<nro>`. `OUNCE_ORACLE_RESULT=<file>` receives the verdict as JSON,
/// and `OUNCE_ORACLE_SECONDS` (default 300) bounds each engine's host time.
#[test]
#[ignore = "requires OUNCE_CORPUS_ROOT with built corpus programs and OUNCE_ORACLE_ID"]
fn corpus_first_frame_oracle() {
    registered_frame_oracle(include_str!("../../tests/corpus/oracles.json"), false);
}

#[test]
#[ignore = "requires built external corpus and OUNCE_ORACLE_ID"]
fn corpus_interaction_oracle() {
    registered_frame_oracle(include_str!("../../tests/corpus/interactions.json"), true);
}

fn registered_frame_oracle(manifest: &str, interaction: bool) {
    use serde_json::{json, Value};
    let oracles: Value = serde_json::from_str(manifest).unwrap();
    let id = std::env::var("OUNCE_ORACLE_ID").expect("OUNCE_ORACLE_ID is unset");
    let root = std::env::var_os("OUNCE_CORPUS_ROOT").expect("OUNCE_CORPUS_ROOT is unset");
    let oracle = oracles["oracles"].as_array().unwrap().iter().find(|o| o["id"] == id.as_str())
        .unwrap_or_else(|| panic!("no oracle {id}"));
    validate_frame_oracle(oracle).unwrap_or_else(|error| panic!("invalid oracle {id}: {error}"));
    let path = Path::new(&root).join(oracle["build"].as_str().unwrap()).join("build/source")
        .join(oracle["nro"].as_str().unwrap());
    let nro = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let nro_sha256 = sha256_hex(&nro);
    let frames = oracle["queued_frames"].as_u64().unwrap();
    let checks = oracle["checks"].as_array().unwrap();
    let probes: Vec<(usize, usize)> = checks.iter().flat_map(|c| c["probes"].as_array().cloned().unwrap_or_default())
        .map(|p| (p["x"].as_u64().unwrap() as usize, p["y"].as_u64().unwrap() as usize)).collect();
    let seconds = std::env::var("OUNCE_ORACLE_SECONDS").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let limits = Limits { instructions: 4_000_000_000, host_time: Duration::from_secs(seconds), frames: usize::MAX,
        queued_frames: frames, probes,
        frame_inputs: if interaction { parse_frame_inputs(&oracle["inputs"], frames).expect("invalid interaction input") } else { Vec::new() },
        ..Limits::default() };
    let mut failures = Vec::new();
    let mut runs = Vec::new();
    let mut reference: Option<Vec<QueuedFrame>> = None;
    let mut times = Vec::new();
    // Interaction qualification always uses the exact engine set, even if OUNCE_CPU is set.
    let engines = if interaction { vec![Engine::Interpreter, Engine::Tier3] } else { oracle_engines() };
    for engine in engines {
        let started = Instant::now();
        let (result, kernel) = run_nro_state(&nro, &path.to_string_lossy(), &[], engine, &limits, &[]);
        let seconds_taken = started.elapsed();
        times.push((engine, seconds_taken));
        println!("{engine:?}: stop={:?} instructions={} svcs={} queued={} pc={:#x}", result.stop, result.instructions, result.svc_count, result.queued.len(), result.pc);
        let mut engine_failures = Vec::new();
        let mut fail = |message: String| engine_failures.push(message);
        if !matches!(result.stop, Stop::FrameLimit) || result.queued.len() as u64 != frames {
            fail(format!("stopped with {:?} after {} of {frames} queued frames", result.stop, result.queued.len()));
        }
        if interaction {
            if result.actual_engine != engine { fail("requested CPU engine was not used".into()); }
            if result.frame_inputs_applied.len() != limits.frame_inputs.len()
                || result.frame_inputs_applied.iter().zip(&limits.frame_inputs)
                    .any(|(&(frame, buttons, _), expected)| frame != expected.after_queued_frame || buttons != expected.buttons)
            {
                fail("input script did not complete at its registered frame boundaries".into());
            }
        }
        let stats: Vec<_> = kernel.ipc.nvdrv.engines.values().map(|g| g.render_stats()).collect();
        let host_draws: u64 = stats.iter().map(|s| s.host_draws as u64).sum();
        let hardware_draws: u64 = stats.iter().map(|s| s.hardware_draws as u64).sum();
        let readback_bytes: u64 = stats.iter().map(|s| s.host_readback_bytes).sum();
        let host_required = interaction && std::env::var("OUNCE_GPU_BACKEND").as_deref() == Ok("host");
        if host_required {
            if host_draws < 3 || readback_bytes < 3 * 1280 * 720 * 4 {
                fail("missing completed host draws/readbacks for Triangle".into());
            }
            if std::env::var("OUNCE_GPU_REQUIRE_HARDWARE").as_deref() == Ok("1") && hardware_draws < 3 {
                fail("missing physical GPU Triangle draws".into());
            }
            if !kernel.ipc.nvdrv.gpu_diagnostics.is_empty() {
                fail(format!("GPU diagnostics: {:?}", kernel.ipc.nvdrv.gpu_diagnostics));
            }
        }
        let mut probe_index = 0;
        for check in checks {
            let expect = &check["expect"];
            let check_probes = check["probes"].as_array().cloned().unwrap_or_default();
            let size = (expect["width"].as_u64().or(check["width"].as_u64()).unwrap() as usize,
                expect["height"].as_u64().or(check["height"].as_u64()).unwrap() as usize);
            for index in check["frames"].as_array().unwrap().iter().map(|f| f.as_u64().unwrap()) {
                let Some(f) = result.queued.get(index as usize - 1) else {
                    fail(format!("frame {index}: required frame was not captured"));
                    continue;
                };
                if let Some(error) = &f.error {
                    fail(format!("frame {index}: {error}"));
                    continue;
                }
                if (f.width, f.height) != size {
                    fail(format!("frame {index}: {}x{}, expected {}x{}", f.width, f.height, size.0, size.1));
                    continue;
                }
                if let Some(hash) = expect["rgba_fnv1a64"].as_str() {
                    let actual = format!("{:#018x}", f.rgba_fnv1a64);
                    if actual != hash { fail(format!("frame {index}: hash {actual}, expected {hash}")); }
                }
                for (i, probe) in check_probes.iter().enumerate() {
                    let want: Vec<u8> = probe["rgb"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
                    let got = f.probes.get(probe_index + i).copied().flatten();
                    if got.map(|p| p[..3].to_vec()) != Some(want.clone()) {
                        fail(format!("frame {index}: pixel ({}, {}) = {got:?}, expected {want:?}", probe["x"], probe["y"]));
                    }
                }
            }
            probe_index += check_probes.len();
        }
        failures.extend(engine_failures.iter().map(|m| format!("{engine:?}: {m}")));
        runs.push(json!({
            "engine": format!("{engine:?}"), "stop": format!("{:?}", result.stop), "instructions": result.instructions,
            "pc": format!("{:#018x}", result.pc), "x0": format!("{:#018x}", result.x0), "failures": engine_failures,
            "host_timeout_seconds": seconds, "wall_seconds": seconds_taken.as_secs_f64(),
            "frame_inputs_applied": result.frame_inputs_applied.iter().map(|&(frame, buttons, instructions)|
                json!({"after_queued_frame": frame, "buttons": buttons, "instructions": instructions})).collect::<Vec<_>>(),
            "gpu": {"host_required": host_required, "host_draws": host_draws, "hardware_draws": hardware_draws,
                "readback_bytes": readback_bytes, "diagnostics": kernel.ipc.nvdrv.gpu_diagnostics.iter().map(|d| format!("{d:?}")).collect::<Vec<_>>()},
            "max_guest_instructions": limits.instructions, "queued": result.queued.iter().map(|f| json!({
                "index": f.index, "width": f.width, "height": f.height,
                "rgba_fnv1a64": format!("{:#018x}", f.rgba_fnv1a64), "error": f.error,
                "probes": limits.probes.iter().zip(&f.probes).map(|(&(x, y), rgba)|
                    json!({"x": x, "y": y, "rgba": rgba})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        }));
        match &reference {
            None => reference = Some(result.queued),
            Some(r) if *r != result.queued => failures.push(format!("{engine:?} frames differ from {:?}", times[0].0)),
            Some(_) => {}
        }
    }
    report_speed(&times);
    let verdict = json!({"id": id, "pass": failures.is_empty(), "nro_sha256": nro_sha256,
        "scope": if interaction { "interaction" } else { "first-frame" },
        "manifest_sha256": sha256_hex(manifest.as_bytes()),
        "failures": failures, "runs": runs});
    println!("{}", serde_json::to_string_pretty(&verdict).unwrap());
    if let Some(out) = std::env::var_os("OUNCE_ORACLE_RESULT") {
        std::fs::write(out, serde_json::to_string_pretty(&verdict).unwrap()).expect("write oracle result");
    }
    assert!(failures.is_empty(), "oracle {id} failed: {failures:#?}");
}

/// Host-execution qualification reuses the owner's unchanged deko-basic expectations.
/// Set OUNCE_GPU_REQUIRE_HARDWARE=1 to require physical hardware as well as readbacks.
#[cfg(feature = "gpu-present")]
#[test]
#[ignore = "requires pinned outside deko_basic NRO and OUNCE_GPU_BACKEND=host"]
fn guest_gpu_deko_existing_oracle() {
    assert_eq!(std::env::var("OUNCE_GPU_BACKEND").as_deref(), Ok("host"));
    let path = std::env::var_os("OUNCE_DEKO_BASIC_NRO").expect("OUNCE_DEKO_BASIC_NRO is unset");
    let bytes = std::fs::read(&path).unwrap();
    println!("guest GPU NRO SHA-256: {}", sha256_hex(&bytes));
    let oracles: serde_json::Value = serde_json::from_str(include_str!("../../tests/corpus/oracles.json")).unwrap();
    let oracle = oracles["oracles"].as_array().unwrap().iter().find(|o| o["id"] == "deko-basic").unwrap();
    validate_frame_oracle(oracle).unwrap();
    let frames = oracle["queued_frames"].as_u64().unwrap();
    let limits = Limits { instructions: 4_000_000_000, host_time: Duration::from_secs(300),
        frames: usize::MAX, queued_frames: frames, ..Limits::default() };
    let mut failures = Vec::new();
    // Qualification always covers the exact required engine set, regardless of OUNCE_CPU.
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let (result,kernel) = run_nro_state(&bytes,&Path::new(&path).to_string_lossy(),&[],engine,&limits,&[]);
        if result.actual_engine != engine { failures.push(format!("{engine:?}: actual engine {:?}",result.actual_engine)); }
        let stats: Vec<_> = kernel.ipc.nvdrv.engines.values().map(|g| g.render_stats()).collect();
        let host_draws: u64 = stats.iter().map(|s| s.host_draws as u64).sum();
        let hardware_draws: u64 = stats.iter().map(|s| s.hardware_draws as u64).sum();
        let readback_bytes: u64 = stats.iter().map(|s| s.host_readback_bytes).sum();
        println!("{engine:?}: stop={:?} queued={} host_draws={host_draws} hardware_draws={hardware_draws} readback_bytes={readback_bytes}",result.stop,result.queued.len());
        if !matches!(result.stop, Stop::FrameLimit) || result.queued.len() as u64 != frames {
            failures.push(format!("{engine:?}: incomplete guest execution: {:?}",result.stop));
        }
        if host_draws < frames || readback_bytes < frames*1280*720*4 {
            failures.push(format!("{engine:?}: missing completed host draws/readbacks"));
        }
        if std::env::var("OUNCE_GPU_REQUIRE_HARDWARE").as_deref() == Ok("1") && hardware_draws < frames {
            failures.push(format!("{engine:?}: physical GPU coverage incomplete"));
        }
        if !kernel.ipc.nvdrv.gpu_diagnostics.is_empty() {
            failures.push(format!("{engine:?}: GPU diagnostics: {:?}",kernel.ipc.nvdrv.gpu_diagnostics));
        }
        for check in oracle["checks"].as_array().unwrap() {
            for index in check["frames"].as_array().unwrap() {
                let index=index.as_u64().unwrap();
                let Some(frame)=result.queued.iter().find(|f| f.index == index) else { continue };
                let expected=&check["expect"];
                let hash=format!("{:#018x}",frame.rgba_fnv1a64);
                println!("{engine:?}: frame {index} hash={hash}");
                if frame.error.is_some() || frame.width as u64 != expected["width"].as_u64().unwrap()
                    || frame.height as u64 != expected["height"].as_u64().unwrap()
                    || hash != expected["rgba_fnv1a64"].as_str().unwrap() {
                    failures.push(format!("{engine:?}: frame {index}: hash {hash}, expected {}",expected["rgba_fnv1a64"]));
                }
            }
        }
    }
    assert!(failures.is_empty(), "host GPU qualification failed (oracle unchanged): {failures:#?}");
}
