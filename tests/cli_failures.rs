// Without the `nro` feature the NRO tests are compiled out, which leaves some of their helpers unused.
#![cfg_attr(not(feature = "nro"), allow(dead_code, unused_imports))]
use std::process::Command;

#[cfg(feature = "nro")]
#[test]
fn missing_input_is_a_nonzero_cli_failure() {
    let path = std::env::temp_dir().join(format!("ounce_missing_{}_not_created.nro", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run", path.to_str().unwrap(), "--present=headless"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to read file"));
}

#[cfg(feature = "nro")]
#[test]
fn malformed_image_is_a_nonzero_cli_failure() {
    let path = std::env::temp_dir().join(format!("ounce_bad_{}.nro", std::process::id()));
    let mut bytes = [0u8; 128];
    bytes[0x10..0x14].copy_from_slice(b"NRO0");
    bytes[0x18..0x1c].copy_from_slice(&128u32.to_le_bytes());
    bytes[0x24..0x28].copy_from_slice(&4096u32.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run", path.to_str().unwrap(), "--present=headless"])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[Loader Error]"));
}

#[cfg(feature = "nro")]
#[test]
fn nro_only_command_rejects_raw_input_before_execution() {
    let path = std::env::temp_dir().join(format!("ounce_raw_{}.nro", std::process::id()));
    std::fs::write(&path, [0u8; 128]).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run-nro", path.to_str().unwrap(), "--present=headless"])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[Loader Error]"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Starting execution"));
}

/// A minimal NRO whose header branches to 0x80, where `text` (code, then any data it uses) starts.
fn nro_with_text(text: &[u8]) -> Vec<u8> {
    let total = 0x80 + text.len();
    let mut b = vec![0u8; total];
    b[0..4].copy_from_slice(&0x1400_0020u32.to_le_bytes());
    b[0x10..0x14].copy_from_slice(b"NRO0");
    b[0x18..0x1c].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x24..0x28].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x80..].copy_from_slice(text);
    b
}

/// The issue's minimal NRO: `mov w0, #0; svc #7` (svcExitProcess), 128 bytes.
#[cfg(not(windows))]
fn exit_nro_bytes() -> Vec<u8> {
    let text: Vec<u8> = [0x5280_0000u32, 0xD400_00E1]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    nro_with_text(&text)
}

/// #476: guest debug output reaches stdout as it is written, not when the run ends, so a hung program's last words
/// survive an outer timeout. The guest prints a partial line (no newline) and then spins; stdout is a pipe, as it is
/// under `ounce-hb`. The run is killed as soon as the text arrives, or after 20 seconds.
#[cfg(feature = "nro")]
#[test]
fn guest_output_is_visible_while_a_hung_program_still_runs() {
    use std::io::Read;
    let message = b"guest says hello, then hangs";
    // ADR X0, message; MOVZ X1, #len; SVC #0x27 (svcOutputDebugString); B . (spin)
    let code = [
        0x1000_0080u32,
        0xD280_0001 | ((message.len() as u32) << 5),
        0xD400_04E1,
        0x1400_0000,
    ];
    let mut text: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
    text.extend_from_slice(message);
    let path = std::env::temp_dir().join(format!("ounce_hang_{}.nro", std::process::id()));
    std::fs::write(&path, nro_with_text(&text)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run-nro", path.to_str().unwrap(), "--present=headless"])
        .env("OUNCE_NRO_BUDGET", "1000000000000")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut seen, mut chunk) = (Vec::new(), [0u8; 4096]);
        while let Ok(n @ 1..) = stdout.read(&mut chunk) {
            seen.extend_from_slice(&chunk[..n]);
            if seen.windows(message.len()).any(|w| w == message) {
                let _ = seen_tx.send(());
                return;
            }
        }
    });
    let visible = seen_rx.recv_timeout(std::time::Duration::from_secs(20)).is_ok();
    let still_running = child.try_wait().unwrap().is_none();
    let _ = child.kill();
    let _ = child.wait();
    std::fs::remove_file(path).unwrap();
    assert!(visible, "the guest's text did not reach stdout while the program ran");
    assert!(
        still_running,
        "the program had already stopped, so this did not test a hung run"
    );
}

/// #479: `--deterministic` on `run-nro` runs the kernel on a fixed clock, so a guest that waits on time executes
/// the same instruction count every run and on every host. The guest polls svcGetSystemTick until 100 ms of guest
/// time have passed, then exits. Each SVC advances the deterministic clock by 20 us, so the wait takes exactly 5000
/// polls: the header's branch to the code, 2 setup instructions, 3 per poll, 2 to exit. On the host clock the
/// number of polls follows host speed.
#[cfg(feature = "nro")]
#[test]
fn deterministic_runs_execute_the_same_instructions_while_waiting_on_time() {
    // MOVZ X1, #0x1D, LSL #16; MOVK X1, #0x4C00 (X1 = 1,920,000 ticks = 100 ms at 19.2 MHz)
    // loop: SVC #0x1E (svcGetSystemTick -> X0); CMP X0, X1; B.LO loop
    // MOVZ W0, #0; SVC #0x07 (svcExitProcess)
    let code = [
        0xD2A0_03A1u32,
        0xF289_8001,
        0xD400_03C1,
        0xEB01_001F,
        0x54FF_FFC3,
        0x5280_0000,
        0xD400_00E1,
    ];
    let text: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
    let dir = std::env::temp_dir().join(format!("ounce_deterministic_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let nro = dir.join("wait.nro");
    std::fs::write(&nro, nro_with_text(&text)).unwrap();
    let counts: Vec<u64> = ["interpreter", "tier3", "interpreter"]
        .iter()
        .map(|tier| {
            let report = dir.join(format!("{tier}.json"));
            let _ = std::fs::remove_file(&report);
            let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
                .args([
                    "run-nro",
                    nro.to_str().unwrap(),
                    "--present=headless",
                    "--deterministic",
                ])
                .arg(format!("--cpu={tier}"))
                .arg(format!("--report-json={}", report.display()))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{tier}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
            assert_eq!(json["stop_kind"], "clean_exit", "{tier}: {json}");
            json["insns_executed"].as_u64().unwrap()
        })
        .collect();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        counts,
        [1 + 2 + 3 * 5000 + 2; 3],
        "interpreter, Tier-3, interpreter again"
    );
}

/// Runs `run-nro <nro> --present=headless --deterministic` on `tier` and returns its `ounce.run.v1` report.
fn deterministic_report(nro: &str, tier: &str, tag: &str, guest_args: &[&str]) -> serde_json::Value {
    let report = std::env::temp_dir().join(format!("ounce_audio_{tag}_{tier}_{}.json", std::process::id()));
    let _ = std::fs::remove_file(&report);
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run-nro", nro, "--present=headless", "--deterministic"])
        .arg(format!("--cpu={tier}"))
        .arg(format!("--report-json={}", report.display()))
        .arg("--")
        .args(guest_args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{tier}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    std::fs::remove_file(&report).unwrap();
    json
}

/// FNV-1a 64 over the little-endian bytes of stereo frames, written out here so the expected hashes below come from
/// the fixture's documented waveform and not from the engine under test.
fn expected_audio_hash(frames: impl Iterator<Item = i16>) -> String {
    let hash = frames
        .flat_map(|sample| [sample, sample]) // the fixture routes one voice to both channels
        .flat_map(i16::to_le_bytes)
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        });
    format!("fnv1a64:{hash:016x}")
}

/// #478: a headless run reports how much audio the guest mixed and a hash of it, so audout / audren output can be
/// verified without a sound device. `tests/fixtures/nro/audren` plays a square wave (period 48 frames, starting high)
/// and mixes in blocks of 240 frames. Its default mode plays 2400 frames of +/-8000, then one block of silence; "long"
/// loops at 20% volume for 400 blocks. The report must match those streams exactly, on every run and both tiers. A
/// program that never opens an audio service reports none.
#[cfg(feature = "nro")]
#[test]
fn headless_audio_is_reported_as_a_frame_count_and_a_deterministic_hash() {
    let audren = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/audren/audren.nro");
    let hello = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/hello/hello.nro");
    let square = |amplitude: i16, frames: usize| {
        (0..frames).map(move |i| if (i / 24) % 2 == 1 { -amplitude } else { amplitude })
    };
    let tone_then_silence = expected_audio_hash(square(8000, 2400).chain(std::iter::repeat_n(0, 240)));
    let long = expected_audio_hash(square(1600, 400 * 240));
    for (guest_args, frames, expected) in [
        (&[][..], 2400 + 240, tone_then_silence),
        (&["long"][..], 400 * 240, long),
    ] {
        for tier in ["interpreter", "tier3", "interpreter"] {
            let run = deterministic_report(audren, tier, "audren", guest_args);
            assert_eq!(run["audio_frames_mixed"], frames, "{tier} {guest_args:?}: {run}");
            assert_eq!(run["audio_hash"], expected.as_str(), "{tier} {guest_args:?}: {run}");
        }
    }
    let silent = deterministic_report(hello, "interpreter", "hello", &[]);
    assert_eq!(silent["audio_frames_mixed"], 0, "{silent}");
    assert!(silent["audio_hash"].is_null(), "{silent}");
}

/// Runs a minimal NRO that calls a function (`BL`), which loads from unmapped memory, on `tier` and returns its report.
/// 0x80: BL +8 (to 0x88)   0x84: MOVZ W0,#0 (never reached)   0x88: MOVZ X1,#0x10   0x8C: LDR X0,[X1] (unmapped)
fn faulting_nro_report(tier: &str) -> serde_json::Value {
    let code = [0x9400_0002u32, 0x5280_0000, 0xD280_0201, 0xF940_0020];
    let text: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
    // Tests run in parallel inside one process, so each call gets its own directory.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ounce_fault_{tier}_{}_{call}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let nro = dir.join("fault.nro");
    std::fs::write(&nro, nro_with_text(&text)).unwrap();
    let report = dir.join("report.json");
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args([
            "run-nro",
            nro.to_str().unwrap(),
            "--present=headless",
            "--deterministic",
        ])
        .arg(format!("--cpu={tier}"))
        .arg(format!("--report-json={}", report.display()))
        .output()
        .unwrap();
    assert!(!output.status.success(), "{tier}: a fault is a failed run");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        (json["stop_kind"].as_str(), json["exit_code"].as_i64()),
        (Some("memory_fault"), Some(11)),
        "{tier}: {json}"
    );
    json
}

/// #475: a stop that has a `pc` also reports the link register and stack pointer, so a failure can be located without
/// a debugger. `lr` is the instruction after the `BL`, exactly 8 bytes before the faulting `pc`, and `sp` is the
/// guest's 16-byte-aligned stack. A clean exit has no `pc` on `run-nro`, so it reports neither.
#[cfg(feature = "nro")]
#[test]
fn a_fault_reports_the_link_register_and_stack_pointer() {
    for tier in ["interpreter", "tier3"] {
        let json = faulting_nro_report(tier);
        let (pc, lr, sp) = (json["pc"].as_u64(), json["lr"].as_u64(), json["sp"].as_u64());
        assert!(
            pc.is_some() && lr.is_some() && sp.is_some(),
            "{tier}: pc, lr and sp are numbers: {json}"
        );
        assert_eq!(
            pc.unwrap() - lr.unwrap(),
            8,
            "{tier}: lr is the BL's return address: {json}"
        );
        assert!(
            sp.unwrap() != 0 && sp.unwrap() % 16 == 0,
            "{tier}: sp is the aligned guest stack: {json}"
        );
    }
    let hello = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/hello/hello.nro");
    let exit = deterministic_report(hello, "interpreter", "lr", &[]);
    assert!(
        exit["pc"].is_null() && exit["lr"].is_null() && exit["sp"].is_null(),
        "{exit}"
    );
}

/// A run that stops on a fault reports the instructions that completed before it: the header's branch, the `BL` and
/// the `MOVZ`, three, on every tier. The faulting load did not complete. A report of 0 would hide how far the program
/// got, and make the two tiers impossible to compare.
#[cfg(feature = "nro")]
#[test]
fn a_fault_still_counts_the_instructions_that_ran_before_it() {
    let counts: Vec<u64> = ["interpreter", "tier3", "interpreter"]
        .iter()
        .map(|tier| faulting_nro_report(tier)["insns_executed"].as_u64().unwrap())
        .collect();
    assert_eq!(counts, [3; 3], "interpreter, Tier-3, interpreter again");
}

/// Runs the exit NRO with one guest argument of `arg_len` bytes. Returns the output and the file name, which sets
/// the program path that run-nro puts in front of the guest argument (`sdmc:/<file>`).
#[cfg(not(windows))]
fn run_exit_nro_with_argument(tag: &str, arg_len: usize) -> std::process::Output {
    let path = std::env::temp_dir().join(format!("ounce_argv_{tag}_{}.nro", std::process::id()));
    std::fs::write(&path, exit_nro_bytes()).unwrap();
    let argument = "a".repeat(arg_len);
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run-nro", path.to_str().unwrap(), "--present=headless", &argument])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    output
}

// Windows caps a command line at 32,767 characters, so these arguments cannot be passed there; the
// loader unit tests cover the same limit on every platform.
#[cfg(not(windows))]
#[cfg(feature = "nro")]
#[test]
fn guest_argument_at_the_documented_limit_runs() {
    // The argument string is "sdmc:/<file> " plus the guest argument; 62,855 bytes is the limit (`MAX_ARGV_LEN`).
    let file_name_len = format!("ounce_argv_at_{}.nro", std::process::id()).len();
    let prefix_len = "sdmc:/".len() + file_name_len + " ".len();
    let output = run_exit_nro_with_argument("at", 62_855 - prefix_len);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Process exited gracefully"));
}

// Windows caps a command line at 32,767 characters, so these arguments cannot be passed there; the
// loader unit tests cover the same limit on every platform.
#[cfg(not(windows))]
#[cfg(feature = "nro")]
#[test]
fn guest_argument_one_byte_over_the_limit_is_refused_before_the_program_starts() {
    let file_name_len = format!("ounce_argv_over_{}.nro", std::process::id()).len();
    let prefix_len = "sdmc:/".len() + file_name_len + " ".len();
    let output = run_exit_nro_with_argument("over", 62_855 - prefix_len + 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("[Loader Error]"), "stderr: {stderr}");
    assert!(stderr.contains("over the 62855-byte limit"), "stderr: {stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Starting execution"));
}

#[cfg(feature = "nro")]
#[test]
fn headless_run_prints_no_host_audio_errors() {
    // Headless opens no audio device, so no ALSA or HostAudioSink lines appear on stderr, whatever the machine has.
    // The issue's minimal NRO: `mov w0, #0; svc #7` (svcExitProcess).
    let mut nro = vec![0u8; 0x88];
    nro[0..4].copy_from_slice(&0x1400_0020u32.to_le_bytes());
    nro[0x10..0x14].copy_from_slice(b"NRO0");
    nro[0x18..0x1c].copy_from_slice(&0x88u32.to_le_bytes());
    nro[0x24..0x28].copy_from_slice(&0x88u32.to_le_bytes());
    nro[0x80..0x88].copy_from_slice(&[0x00, 0x00, 0x80, 0x52, 0xE1, 0x00, 0x00, 0xD4]);
    let path = std::env::temp_dir().join(format!("ounce_headless_audio_{}.nro", std::process::id()));
    std::fs::write(&path, nro).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run-nro", path.to_str().unwrap(), "--present=headless"])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(!stderr.contains("ALSA"), "stderr: {stderr}");
    assert!(!stderr.contains("HostAudioSink"), "stderr: {stderr}");
}

#[test]
fn linux_deterministic_clock_cli_repeats_the_same_output() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/linux-elf/bin/clock.elf");
    for _ in 0..2 {
        let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
            .args(["run-elf", fixture, "--deterministic", "--cpu=tier3"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(
            output.stdout,
            b"realtime=1700000000:0\nmonotonic=0:1000000\nraw=0:2000000\nboottime=0:3000000\n"
        );
    }
}

#[test]
fn linux_guest_arguments_after_separator_are_not_host_options() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/linux-elf/bin/args.elf");
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args([
            "run-elf",
            fixture,
            "--cpu=tier3",
            "--",
            "--cpu=tier4",
            "--present=headless",
            "--sdmc=/guest",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(4)); // fixture exits with argc
    let expected = format!(
        "argc=4\nargv: {fixture}\nargv: --cpu=tier4\nargv: --present=headless\nargv: --sdmc=/guest\npagesz=4096\n"
    );
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(String::from_utf8_lossy(&output.stderr).contains("engine=Tier-3"));
}

#[test]
fn fuzzer_rejects_empty_trace_instead_of_reporting_zero_mismatches_as_success() {
    let path = std::env::temp_dir().join(format!("ounce_empty_trace_{}.txt", std::process::id()));
    std::fs::write(&path, b"").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["fuzz-replay", path.to_str().unwrap()])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no cases"));
}

#[test]
fn fuzzer_does_not_ignore_trace_read_errors() {
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["fuzz-replay", std::env::temp_dir().to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn fuzzer_rejects_zero_generated_cases() {
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["fuzz-replay", "--engine-vs-engine", "--count", "0"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn fuzzer_fails_when_the_engine_accepts_a_host_undefined_word() {
    let path = std::env::temp_dir().join(format!("ounce_undefined_trace_{}.txt", std::process::id()));
    // A deliberately contradictory reference: MOV X0,#1 is implemented, not UNDEFINED.
    std::fs::write(&path, b"U d2800020\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["fuzz-replay", path.to_str().unwrap()])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("emulator runs a word the host calls UNDEFINED: 1"));
}

fn mov_trace(expected_x0: u64) -> String {
    let input = vec!["0".to_string(); 63].join(" ");
    let mut output = vec!["0".to_string(); 63];
    output[0] = format!("{expected_x0:x}");
    format!("T d2800020 0 {input} => 0 0 {}\n", output.join(" "))
}

#[test]
fn first_instruction_difference_retains_the_actual_execution_and_replay_input() {
    let root = std::env::temp_dir().join(format!("ounce_dossier_{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let trace = root.join("input.trace");
    let bundle = root.join("first-difference");
    // First case agrees; the second and third deliberately contradict MOV X0,#1.
    std::fs::write(&trace, mov_trace(1) + &mov_trace(2) + &mov_trace(3)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args([
            "fuzz-replay",
            "--engine",
            "tier3",
            "--first-difference",
            bundle.to_str().unwrap(),
            trace.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        bundle.join("diagnosis.json").exists(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("diagnosis.json")).unwrap()).unwrap();
    assert_eq!(report["line_number"], 2);
    assert_eq!(report["word"], "0xd2800020");
    assert_eq!(report["input"]["x"][0], "0x0000000000000000");
    assert_eq!(report["reference"]["x"][0], "0x0000000000000002");
    assert_eq!(report["actual"]["x"][0], "0x0000000000000001");
    assert_eq!(
        std::fs::read_to_string(bundle.join("case.trace")).unwrap(),
        mov_trace(2)
    );
    // The bundle is immutable; rerunning never overwrites the original diagnosis.
    let before = std::fs::read(bundle.join("diagnosis.json")).unwrap();
    let again = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args([
            "fuzz-replay",
            "--first-difference",
            bundle.to_str().unwrap(),
            trace.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(again.status.code(), Some(2));
    assert_eq!(std::fs::read(bundle.join("diagnosis.json")).unwrap(), before);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn instruction_dossier_replay_checks_bytes_and_distinguishes_reproduction_from_correctness() {
    let root = std::env::temp_dir().join(format!("ounce_replay_{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let trace = root.join("input.trace");
    let bundle = root.join("bundle");
    std::fs::write(&trace, mov_trace(2)).unwrap();
    let saved = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args([
            "fuzz-replay",
            "--engine",
            "tier3",
            "--first-difference",
            bundle.to_str().unwrap(),
            trace.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(saved.status.code(), Some(1));
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_ounce-core"))
            .args(["replay-instruction", bundle.to_str().unwrap()])
            .output()
            .unwrap()
    };
    let result = replay();
    assert_eq!(result.status.code(), Some(0));
    assert!(
        result.stdout.starts_with(b"{"),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["replay_verdict"], "reproduced");
    assert_eq!(report["reference_verdict"], "mismatch");
    // Corrupting retained input must fail before execution, even if it is valid ARM64.
    std::fs::write(bundle.join("case.trace"), mov_trace(3)).unwrap();
    let corrupt = replay();
    assert_eq!(corrupt.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&corrupt.stderr).contains("hash mismatch"));
    std::fs::write(bundle.join("case.trace"), mov_trace(2)).unwrap();
    let file = bundle.join("diagnosis.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    manifest["reference"]["x"][0] = "0x0000000000000063".into();
    std::fs::write(&file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(replay().status.code(), Some(2));
    manifest["reference"]["x"][0] = "0x0000000000000002".into();
    manifest["actual"]["x"][0] = "0x0000000000000009".into();
    std::fs::write(&file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let changed = replay();
    assert_eq!(changed.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&changed.stdout).unwrap();
    assert_eq!(report["replay_verdict"], "changed");
    // Model a valid historical record whose word the current decoder cannot execute.
    // The record is synthetic; changed runtime behavior is distinct from corrupt bytes.
    use sha2::{Digest, Sha256};
    let unsupported = mov_trace(2).replace("d2800020", "00000000");
    std::fs::write(bundle.join("case.trace"), unsupported.as_bytes()).unwrap();
    manifest["word"] = "0x00000000".into();
    manifest["case_sha256"] = format!("{:x}", Sha256::digest(unsupported.as_bytes())).into();
    std::fs::write(&file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let stopped = replay();
    assert_eq!(stopped.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&stopped.stdout).unwrap();
    assert_eq!(report["replay_verdict"], "changed");
    assert_eq!(report["reference_verdict"], "not-compared");
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(feature = "nro")]
#[test]
fn truncated_nso_is_a_loader_error_before_guest_execution() {
    let path = std::env::temp_dir().join(format!("ounce_truncated_{}.nso", std::process::id()));
    // Self-authored NSO0: a four-byte text segment begins exactly at EOF.
    let mut bytes = [0u8; 0x100];
    bytes[..4].copy_from_slice(b"NSO0");
    bytes[0x10..0x14].copy_from_slice(&0x100u32.to_le_bytes());
    bytes[0x18..0x1c].copy_from_slice(&4u32.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run", path.to_str().unwrap(), "--present=headless"])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[Loader Error]") && stderr.contains("file"), "{stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Successfully loaded package"));
}

fn run_cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn bare_command_prints_a_start_screen_and_runs_nothing() {
    let output = run_cli(&[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("ounce-core ui") && stdout.contains("ounce-core help"));
    assert!(!stdout.contains("Stage 1/"), "the demo must not start");
    assert!(stdout.lines().count() < 14);
}

#[test]
fn unknown_command_is_a_usage_error_with_a_suggestion() {
    let output = run_cli(&["run-nr"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown command 'run-nr'") && stderr.contains("Did you mean 'run-nro'?"));
}

#[cfg(feature = "nro")]
#[test]
fn commands_without_a_path_are_usage_errors() {
    for command in ["run", "run-nro", "inspect"] {
        let output = run_cli(&[command]);
        assert_eq!(output.status.code(), Some(2), "{command}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(&format!("'{command}' needs")));
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("OUNCE-CORE:"),
            "no banner before the error"
        );
    }
}

#[cfg(feature = "nro")]
#[test]
fn an_early_stop_ends_with_the_next_step_and_the_report_form() {
    // One undecodable instruction word at 0x80, behind a branch in the header slot.
    let path = std::env::temp_dir().join(format!("ounce_halt_{}.nro", std::process::id()));
    let mut image = vec![0u8; 0x1000];
    image[0..4].copy_from_slice(&0x1400_0020u32.to_le_bytes());
    image[0x10..0x14].copy_from_slice(b"NRO0");
    image[0x18..0x1c].copy_from_slice(&0x1000u32.to_le_bytes());
    image[0x24..0x28].copy_from_slice(&0x1000u32.to_le_bytes());
    image[0x28..0x2c].copy_from_slice(&0x1000u32.to_le_bytes());
    image[0x30..0x34].copy_from_slice(&0x1000u32.to_le_bytes());
    std::fs::write(&path, image).unwrap();
    let output = run_cli(&["run-nro", path.to_str().unwrap(), "--present=headless"]);
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Run stopped early."), "{stderr}");
    assert!(
        stderr.contains("issues/new?template=emulation_bug.yml") && stderr.contains("--cpu=tier3"),
        "{stderr}"
    );
}

/// `hello.nro` reports `0x00000EEE` only when its argv is exactly `[path, "fixture"]`; bit 3 (`0x0BAD0008`) means
/// `argc != 2 || argv[1] != "fixture"`. That makes it a probe for what the guest actually received (#487).
fn run_hello(args: &[&str]) -> std::process::Output {
    let nro = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/hello/hello.nro");
    Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run-nro", nro, "--present=headless"])
        .args(args)
        .output()
        .unwrap()
}

#[cfg(feature = "nro")]
#[test]
fn guest_arguments_after_separator_reach_an_nro_unchanged() {
    let exact = run_hello(&["--", "fixture"]);
    assert!(
        String::from_utf8_lossy(&exact.stdout).contains("result=0x00000EEE"),
        "{}",
        String::from_utf8_lossy(&exact.stdout)
    );
    // A dashed word after `--` used to be dropped silently, so the guest still saw argc == 2 and passed.
    for extra in ["--verbose", "-x", "--gui"] {
        let output = run_hello(&["--", "fixture", extra]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("result=0x0BAD0008"),
            "guest did not receive {extra}: {stdout}"
        );
    }
}

/// libnx builds argv by splitting one string on spaces, and drops an empty token even inside quotes. A guest argument
/// that is the empty string therefore cannot reach the guest: the program would see a smaller `argc` than it was run
/// with. Refusing it before the program starts is better than that silent change (same rule as #487).
#[cfg(feature = "nro")]
#[test]
fn an_empty_guest_argument_is_refused_instead_of_dropped() {
    let output = run_hello(&["--", "fixture", ""]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("empty guest argument"), "stderr: {stderr}");
    assert!(
        output.stdout.is_empty(),
        "the program must not start: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[cfg(feature = "nro")]
#[test]
fn unknown_dashed_nro_argument_before_separator_is_refused_with_the_fix() {
    let output = run_hello(&["fixture", "-x"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("unknown option '-x'"), "stderr: {stderr}");
    assert!(stderr.contains("-- -x"), "stderr: {stderr}");
    assert!(
        output.stdout.is_empty(),
        "the program must not start: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[cfg(feature = "nro")]
#[test]
fn run_nro_help_documents_the_separator() {
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run-nro", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("usage: ounce-core run-nro <path>"), "{stdout}");
    assert!(
        stdout.contains("Everything after `--` is passed to the program unchanged"),
        "{stdout}"
    );
}

/// `run-elf` writes the same `ounce.run.v1` file as `run-nro`; its field set must match the schema document (#473).
#[test]
fn run_elf_writes_an_ounce_run_v1_report_and_dumps_registers() {
    let elf = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/linux-elf/bin/exitcode.elf");
    let report = std::env::temp_dir().join(format!("ounce_run_elf_report_{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run-elf", elf, "--cpu=tier3", "--dump-regs"])
        .arg(format!("--report-json={}", report.display()))
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(42),
        "the process status is still the guest's"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[run-elf] registers at stop: pc=0x") && stderr.contains("x0=0x000000000000002a"),
        "{stderr}"
    );
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    std::fs::remove_file(&report).unwrap();
    assert_eq!(json["schema"], "ounce.run.v1");
    assert_eq!(json["stop_kind"], "clean_exit");
    assert_eq!(json["exit_code"], 0);
    assert_eq!(json["guest_exit_status"], 42);
    assert_eq!(json["tier"], "tier3");
    assert!(json["insns_executed"].as_u64().unwrap() > 0);
    assert!(
        json["sp"].as_u64().is_some_and(|sp| sp != 0 && sp % 16 == 0),
        "run-elf reports the stack pointer: {json}"
    );
    assert!(json["lr"].is_u64(), "run-elf reports the link register: {json}");
    let doc = include_str!("../docs/OUNCE_RUN_V1_SCHEMA.md");
    let fields = doc
        .split("## Schema Fields")
        .nth(1)
        .unwrap()
        .split("\n---")
        .next()
        .unwrap();
    for key in json.as_object().unwrap().keys() {
        assert!(
            fields.contains(&format!("| `{key}` |")),
            "{key} is not in docs/OUNCE_RUN_V1_SCHEMA.md"
        );
    }
}

/// Only `.nro` programs have an argv; `run` must refuse guest arguments for a folder or another format, not drop them.
#[cfg(feature = "nro")]
#[test]
fn run_refuses_guest_arguments_for_targets_without_an_argv() {
    let dir = std::env::temp_dir().join(format!("ounce_args_dir_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::env::temp_dir().join(format!("ounce_args_file_{}.nso", std::process::id()));
    std::fs::write(&file, b"NSO0 not an nro").unwrap();
    for target in [&dir, &file] {
        let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
            .args(["run", target.to_str().unwrap(), "--present=headless", "--", "--verbose"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{}: {stderr}", target.display());
        assert!(stderr.contains("only passed to .nro programs"), "{stderr}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_file(&file).unwrap();
}

#[test]
fn version_banner_is_neutral_and_shows_features() {
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ARM64 EXECUTION & VERIFICATION RUNTIME"));
    assert!(!stdout.contains("TEGRA T239 HARDWARE & RUNTIME SYSTEM PROFILE"));
    assert!(stdout.contains("native-jit"));
    assert!(stdout.contains("nce"));
    assert!(stdout.contains("host-io"));

    let nro_heading = "NRO homebrew runtime (Horizon HLE, for unencrypted homebrew built from source)";
    let cpu_pos = stdout
        .find("CPU Subsystem: ARMv8.5-A AArch64 (PAC + BTI)")
        .unwrap_or_else(|| panic!("missing CPU subsystem heading:\n{stdout}"));
    let engines_pos = stdout
        .find("Execution Engines")
        .unwrap_or_else(|| panic!("missing Execution Engines row:\n{stdout}"));
    let nro_pos = stdout
        .find(nro_heading)
        .unwrap_or_else(|| panic!("missing NRO homebrew runtime heading:\n{stdout}"));
    assert!(
        cpu_pos < nro_pos && engines_pos < nro_pos,
        "NRO heading must follow ARM64 execution/engine lines:\n{stdout}"
    );

    let after_nro = &stdout[nro_pos..];
    for expected in [
        "39-bit & 48-bit Horizon Address Space",
        "Bounded Maxwell/Ampere guest command emulation",
        "Software paths, with optional host-GPU presentation",
        "nvdrv GPU mappings",
        "GPFIFO Channel Methods",
        "Guest SASS -> IR -> CPU",
        "Sparse guest pages allocated on demand",
        "Horizon audren:u Audio Renderer",
        "audout:u",
        "Guest HID state from configured host input",
        "Selected Horizon service ABI behaviours",
        "Single host thread; guest scheduling is emulated",
    ] {
        assert!(
            after_nro.contains(expected),
            "missing '{expected}' under NRO homebrew runtime heading:\n{stdout}"
        );
    }

    for forbidden in [
        "[ Graphics Emulation ]",
        "[ Guest Memory Model ]",
        "[ Audio and Input Emulation ]",
        "[ Homebrew Formats & Storage ]",
        "[ Horizon OS 64-bit Microkernel & HLE Services ]",
        "OunceCake",
        "Joy-Con",
        "Handheld",
        "Docked",
    ] {
        assert!(
            !stdout.contains(forbidden),
            "unexpected '{forbidden}' in --version output:\n{stdout}"
        );
    }
}

#[test]
fn help_and_info_describe_the_actual_execution_model() {
    let help = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .arg("help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("tier3 (decoded block cache, default)"), "{help}");
    assert!(!help.contains("tier3 (reference interpreter"), "{help}");
    assert!(!help.contains("Joy-Con"), "{help}");
    assert!(help.contains("Interactive demo game with controller input"), "{help}");

    let info = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .arg("info")
        .output()
        .unwrap();
    assert!(info.status.success());
    let info = String::from_utf8_lossy(&info.stdout);
    let arch_pos = info
        .find("Single-threaded AArch64 execution for bounded program testing")
        .unwrap_or_else(|| panic!("missing arch line:\n{info}"));
    let engines_pos = info
        .find("Execution Engines")
        .unwrap_or_else(|| panic!("missing Execution Engines line:\n{info}"));
    let nro_heading = "NRO homebrew runtime (Horizon HLE, for unencrypted homebrew built from source)";
    let nro_pos = info
        .find(nro_heading)
        .unwrap_or_else(|| panic!("missing NRO homebrew runtime heading:\n{info}"));
    assert!(
        arch_pos < nro_pos && engines_pos < nro_pos,
        "NRO heading must follow ARM64 execution/engine lines:\n{info}"
    );
    let after_nro = &info[nro_pos..];
    for expected in [
        "39-bit & 48-bit Horizon Address Space",
        "nvdrv GPU mappings",
        "Horizon audren:u Audio Renderer",
    ] {
        assert!(
            after_nro.contains(expected),
            "missing '{expected}' under NRO homebrew runtime heading:\n{info}"
        );
    }
    assert!(!info.contains("Octa-Core Preemptive Multi-Core Scheduler"), "{info}");
    assert!(!info.contains("Tegra T239 Custom"), "{info}");
    assert!(!info.contains("Preemptive Multi-Threaded Scheduler"), "{info}");
    assert!(!info.contains("Joy-Con"), "{info}");
    assert!(!info.contains("OunceCake"), "{info}");
    assert!(!info.contains("[ Graphics Emulation ]"), "{info}");
    assert!(
        !info.contains("[ Horizon OS 64-bit Microkernel & HLE Services ]"),
        "{info}"
    );
}

#[test]
fn run_elf_diverge_and_trace_diff_print_usage_on_help() {
    let help = [
        ["run-elf", "--help"],
        ["run-elf", "-h"],
        ["diverge", "--help"],
        ["diverge", "-h"],
        ["trace-diff", "--help"],
        ["trace-diff", "-h"],
    ];
    for args in help {
        let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(
            stdout.contains(&format!("usage: ounce-core {}", args[0])),
            "{args:?}: {stdout}"
        );
    }
}

#[test]
fn help_after_the_guest_separator_reaches_the_program() {
    let missing = std::env::temp_dir().join(format!("ounce_help_{}_not_created.elf", std::process::id()));
    let missing = missing.to_str().unwrap();
    for args in [
        ["run-elf", missing, "--", "--help"],
        ["diverge", missing, "--", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?}: {output:?}");
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("usage:"),
            "{args:?}: {output:?}"
        );
    }
}
