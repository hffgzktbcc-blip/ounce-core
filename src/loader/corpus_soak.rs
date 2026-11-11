//! Long-run soak of registered corpus programs on the interpreter and Tier-3 (opt-in, headless).
//!
//! The first-frame oracles stop after a handful of frames. This keeps a program running for
//! thousands of frames or a host time budget and records, per engine: the stop reason, one frame
//! hash per queued frame, periodic samples of host RSS / guest pages / Tier-3 cache size, and the
//! first interpreter-vs-Tier-3 divergence. It asserts nothing about what the frames should show;
//! it detects crashes, hangs, unbounded growth and cross-engine disagreement.
//!
//! `OUNCE_CORPUS_ROOT=<builds> OUNCE_ORACLE_ID=<id> cargo test --release corpus_soak -- --ignored --nocapture`
//! Knobs: `OUNCE_SOAK_FRAMES` (2000), `OUNCE_SOAK_SECONDS` host time per engine (600),
//! `OUNCE_SOAK_SAMPLE` frames between samples (100), `OUNCE_SOAK_INPUT` non-zero seed for scripted
//! pseudo-random button input every 30 frames (0 = no input), `OUNCE_SOAK_RESULT` verdict JSON path.
//! Run it under an outer process timeout (scripts/soak_corpus.py does): a blocking SVC cannot be
//! interrupted from inside the runner.

use super::*;
use serde_json::{json, Value};

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Seeded button script that never presses PLUS or MINUS (the usual exit buttons).
fn soak_inputs(seed: u64, frames: u64) -> Vec<FrameInput> {
    const BUTTONS: [u32; 14] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15];
    let mut state = seed | 1;
    let mut next = move || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
    let mut inputs = vec![FrameInput { after_queued_frame: 0, buttons: 0 }];
    let mut frame = 30;
    while frame < frames {
        let mut buttons = 0u64;
        for _ in 0..(next() % 3) { buttons |= 1 << BUTTONS[(next() % BUTTONS.len() as u64) as usize]; }
        inputs.push(FrameInput { after_queued_frame: frame, buttons });
        frame += 30;
    }
    inputs
}

#[test]
fn soak_inputs_never_press_exit_buttons_and_strictly_increase() {
    let inputs = soak_inputs(7, 600);
    assert_eq!(inputs[0].buttons, 0);
    assert!(inputs.windows(2).all(|w| w[0].after_queued_frame < w[1].after_queued_frame));
    assert!(inputs.iter().all(|i| i.buttons & (3 << 10) == 0));
    assert!(inputs.iter().any(|i| i.buttons != 0));
    assert_eq!(soak_inputs(7, 600).len(), inputs.len());
}

/// Growth after a warm-up of the first quarter of samples: the highest later reading minus the highest
/// warm-up reading. Peaks rather than the last value, because `ps` on macOS leaves compressed pages out
/// of RSS, so a series can fall and rise again without the program allocating anything.
fn growth(values: &[u64]) -> i64 {
    if values.len() < 4 { return 0; }
    let warmup = values.len() / 4 + 1;
    let peak = |v: &[u64]| v.iter().copied().max().unwrap_or(0) as i64;
    (peak(&values[warmup..]) - peak(&values[..warmup])).max(0)
}

#[test]
fn growth_ignores_warmup_dips_and_short_series() {
    assert_eq!(growth(&[1, 100]), 0);
    assert_eq!(growth(&[10, 500, 500, 500, 500, 500, 500, 500]), 0);
    assert_eq!(growth(&[10, 20, 30, 40, 50, 60, 70, 80]), 50);
    // Falls to a compressed-page reading, then comes back to the old peak: no growth.
    assert_eq!(growth(&[300, 300, 40, 40, 300, 300, 40, 300]), 0);
    // A real climb past the warm-up peak is reported.
    assert_eq!(growth(&[300, 300, 40, 40, 300, 400, 500, 700]), 400);
}

fn engine_verdict(engine: Engine, frames: u64, seconds: u64, result: &Report, wall: Duration) -> (Value, Vec<String>) {
    let mut findings = Vec::new();
    let reached = result.queued.len() as u64;
    let outcome = match &result.stop {
        Stop::FrameLimit => "soaked",
        Stop::ExitProcess => "exited",
        Stop::HostTimeout => {
            // Frames kept arriving until near the deadline: slow, not hung.
            let stalled_ms = (wall.as_millis() as u64).saturating_sub(result.last_queue_ms);
            if reached == 0 || stalled_ms > (seconds * 1000 / 4).max(5_000) { "hang" } else { "slow" }
        }
        Stop::InstructionBudget => "budget",
        _ => "crash",
    };
    if matches!(outcome, "crash" | "hang") {
        findings.push(format!("{engine:?}: {outcome} ({:?}) after {reached} of {frames} frames, pc={:#x}", result.stop, result.pc));
    }
    if let Some(bad) = result.queued.iter().find(|f| f.error.is_some()) {
        findings.push(format!("{engine:?}: frame {} unreadable: {}", bad.index, bad.error.as_deref().unwrap()));
    }
    let rss: Vec<u64> = result.samples.iter().filter_map(|s| s.host_rss_kib).collect();
    let pages: Vec<u64> = result.samples.iter().map(|s| s.guest_pages as u64).collect();
    let jit: Vec<u64> = result.samples.iter().map(|s| s.jit_blocks as u64).collect();
    // Thresholds: 128 MiB host RSS, 8 MiB of guest pages or 2048 Tier-3 blocks after warm-up.
    if growth(&rss) > 128 * 1024 { findings.push(format!("{engine:?}: host RSS grew {} KiB after warm-up", growth(&rss))); }
    if growth(&pages) > 2048 { findings.push(format!("{engine:?}: guest pages grew {} after warm-up", growth(&pages))); }
    if growth(&jit) > 2048 { findings.push(format!("{engine:?}: Tier-3 blocks grew {} after warm-up", growth(&jit))); }
    let distinct: std::collections::HashSet<u64> = result.queued.iter().map(|f| f.rgba_fnv1a64).collect();
    let verdict = json!({
        "engine": format!("{engine:?}"), "outcome": outcome, "stop": format!("{:?}", result.stop),
        "frames_reached": reached, "frames_requested": frames, "instructions": result.instructions,
        "wall_seconds": wall.as_secs_f64(), "frames_per_second": reached as f64 / wall.as_secs_f64().max(1e-9),
        "last_queue_ms": result.last_queue_ms, "pc": format!("{:#018x}", result.pc),
        "distinct_frames": distinct.len(), "svc_count": result.svc_count, "preemptions": result.preemptions,
        "rss_growth_kib": growth(&rss), "guest_page_growth": growth(&pages), "jit_block_growth": growth(&jit),
        "samples": result.samples.iter().map(|s| json!({
            "queued": s.queued, "instructions": s.instructions, "regs_fnv1a64": format!("{:#018x}", s.regs_fnv1a64),
            "guest_pages": s.guest_pages, "jit_blocks": s.jit_blocks, "host_rss_kib": s.host_rss_kib})).collect::<Vec<_>>(),
        "frame_hashes": result.queued.iter().map(|f| format!("{:016x}", f.rgba_fnv1a64)).collect::<Vec<_>>(),
        "last_svcs": result.last_svcs.iter().map(|&(pc, id)| format!("{pc:#x}:svc{id:#x}")).collect::<Vec<_>>(),
        "registers": result.registers.iter().map(|r| format!("{r:#x}")).collect::<Vec<_>>(),
        "debug_output_head": result.debug_output.chars().take(200).collect::<String>(),
        "findings": findings,
    });
    (verdict, findings)
}

#[test]
fn verdict_classifies_a_spinning_guest_as_hang_and_a_faulting_guest_as_crash() {
    let limits = Limits { instructions: usize::MAX / 2, host_time: Duration::from_secs(1), sample_every: 1, ..Limits::default() };
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let started = Instant::now();
        let spin = synthetic(&[0x1400_0000], engine, &limits); // B .
        let (verdict, findings) = engine_verdict(engine, 10, 1, &spin, started.elapsed());
        assert_eq!(verdict["outcome"], "hang", "{engine:?}: {verdict}");
        assert!(findings[0].contains("hang"), "{findings:?}");

        let crash = synthetic(&[0x0000_0000], engine, &limits); // UDF #0
        let (verdict, findings) = engine_verdict(engine, 10, 1, &crash, Duration::from_millis(5));
        assert_eq!(verdict["outcome"], "crash", "{engine:?}: {verdict}");
        assert!(findings[0].contains("crash"), "{findings:?}");

        // BL callee; ExitProcess: a clean early exit is reported but is not a finding by itself.
        let exit = synthetic(&[0x9400_0002, 0xD400_00E1, 0xD280_0540, 0xD65F_03C0], engine, &limits);
        let (verdict, findings) = engine_verdict(engine, 10, 1, &exit, Duration::from_millis(5));
        assert_eq!(verdict["outcome"], "exited");
        assert!(findings.is_empty());
    }
}

#[test]
#[ignore = "requires OUNCE_CORPUS_ROOT with built corpus programs and OUNCE_ORACLE_ID"]
fn corpus_soak() {
    let id = std::env::var("OUNCE_ORACLE_ID").expect("OUNCE_ORACLE_ID is unset");
    let root = std::env::var_os("OUNCE_CORPUS_ROOT").expect("OUNCE_CORPUS_ROOT is unset");
    let manifest: Value = serde_json::from_str(include_str!("../../tests/corpus/oracles.json")).unwrap();
    let oracle = manifest["oracles"].as_array().unwrap().iter().find(|o| o["id"] == id.as_str())
        .unwrap_or_else(|| panic!("no registered oracle {id}"));
    let path = Path::new(&root).join(oracle["build"].as_str().unwrap()).join("build/source")
        .join(oracle["nro"].as_str().unwrap());
    let nro = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    use sha2::Digest;
    let nro_sha256 = format!("{:x}", sha2::Sha256::digest(&nro));
    let frames = env_u64("OUNCE_SOAK_FRAMES", 2000);
    let seconds = env_u64("OUNCE_SOAK_SECONDS", 600);
    let seed = env_u64("OUNCE_SOAK_INPUT", 0);
    let limits = Limits { instructions: usize::MAX / 2, host_time: Duration::from_secs(seconds), frames: usize::MAX,
        queued_frames: frames, sample_every: env_u64("OUNCE_SOAK_SAMPLE", 100).max(1),
        frame_inputs: if seed != 0 { soak_inputs(seed, frames) } else { Vec::new() }, ..Limits::default() };
    let mut findings = Vec::new();
    let mut runs = Vec::new();
    let mut reports: Vec<(Engine, Report)> = Vec::new();
    for engine in [Engine::Interpreter, Engine::Tier3] {
        let started = Instant::now();
        let (result, _kernel) = run_nro_state(&nro, &path.to_string_lossy(), &[], engine, &limits, &[]);
        let wall = started.elapsed();
        println!("{engine:?}: stop={:?} queued={} instructions={} wall={:.1}s", result.stop, result.queued.len(), result.instructions, wall.as_secs_f64());
        let (verdict, engine_findings) = engine_verdict(engine, frames, seconds, &result, wall);
        findings.extend(engine_findings);
        runs.push(verdict);
        reports.push((engine, result));
    }
    // Divergence: compare frame hashes over the frames both engines reached, and the register
    // hash at each common sample. A frame divergence is a finding; a register-only divergence
    // is reported separately because scheduler quanta differ by engine for threaded guests.
    let (a, b) = (&reports[0].1, &reports[1].1);
    let common = a.queued.len().min(b.queued.len());
    let mut divergence = Value::Null;
    if let Some(i) = (0..common).find(|&i| a.queued[i].rgba_fnv1a64 != b.queued[i].rgba_fnv1a64) {
        divergence = json!({"first_frame": i + 1, "interpreter": format!("{:016x}", a.queued[i].rgba_fnv1a64),
            "tier3": format!("{:016x}", b.queued[i].rgba_fnv1a64)});
        findings.push(format!("interpreter and Tier-3 first differ at frame {}", i + 1));
    }
    let diverged_frames = (0..common).filter(|&i| a.queued[i].rgba_fnv1a64 != b.queued[i].rgba_fnv1a64).count();
    let state_diffs: Vec<u64> = a.samples.iter().zip(&b.samples)
        .filter(|(x, y)| x.queued == y.queued && x.regs_fnv1a64 != y.regs_fnv1a64).map(|(x, _)| x.queued).collect();
    // Both engines stopping for a legitimate reason at different frames is also a disagreement.
    if matches!((&a.stop, &b.stop), (Stop::ExitProcess, Stop::ExitProcess)) && a.queued.len() != b.queued.len() {
        findings.push(format!("engines exited after different frame counts: {} vs {}", a.queued.len(), b.queued.len()));
    }
    let verdict = json!({"id": id, "pass": findings.is_empty(), "nro_sha256": nro_sha256,
        "frames_requested": frames, "seconds_per_engine": seconds, "input_seed": seed,
        "compared_frames": common, "diverged_frames": diverged_frames, "first_divergence": divergence,
        "register_divergent_samples": state_diffs, "findings": findings, "runs": runs});
    println!("{}", serde_json::to_string_pretty(&verdict).unwrap());
    if let Some(out) = std::env::var_os("OUNCE_SOAK_RESULT") {
        std::fs::write(out, serde_json::to_string_pretty(&verdict).unwrap()).expect("write soak result");
    }
    assert!(findings.is_empty(), "soak {id} found problems: {findings:#?}");
}
