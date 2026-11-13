use std::process::Command;

#[test]
fn missing_input_is_a_nonzero_cli_failure() {
    let path = std::env::temp_dir().join(format!(
        "ounce_missing_{}_not_created.nro",
        std::process::id()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run", path.to_str().unwrap(), "--present=headless"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to read file"));
}

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

/// The issue's minimal NRO: `mov w0, #0; svc #7` (svcExitProcess), 128 bytes.
fn exit_nro_bytes() -> Vec<u8> {
    let text: Vec<u8> = [0x5280_0000u32, 0xD400_00E1].iter().flat_map(|w| w.to_le_bytes()).collect();
    let total = 0x80 + text.len();
    let mut b = vec![0u8; total];
    b[0..4].copy_from_slice(&0x1400_0020u32.to_le_bytes());
    b[0x10..0x14].copy_from_slice(b"NRO0");
    b[0x18..0x1c].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x24..0x28].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x80..].copy_from_slice(&text);
    b
}

/// Runs the exit NRO with one guest argument of `arg_len` bytes. Returns the output and the file name, which sets
/// the program path that run-nro puts in front of the guest argument (`sdmc:/<file>`).
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
#[test]
fn guest_argument_at_the_documented_limit_runs() {
    // The argument string is "sdmc:/<file> " plus the guest argument; 62,855 bytes is the limit (`MAX_ARGV_LEN`).
    let file_name_len = format!("ounce_argv_at_{}.nro", std::process::id()).len();
    let prefix_len = "sdmc:/".len() + file_name_len + " ".len();
    let output = run_exit_nro_with_argument("at", 62_855 - prefix_len);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("Process exited gracefully"));
}

// Windows caps a command line at 32,767 characters, so these arguments cannot be passed there; the
// loader unit tests cover the same limit on every platform.
#[cfg(not(windows))]
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

#[test]
fn linux_deterministic_clock_cli_repeats_the_same_output() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/linux-elf/bin/clock.elf");
    for _ in 0..2 {
        let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
            .args(["run-elf", fixture, "--deterministic", "--cpu=tier3"])
            .output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, b"realtime=1700000000:0\nmonotonic=0:1000000\nraw=0:2000000\nboottime=0:3000000\n");
    }
}

#[test]
fn linux_guest_arguments_after_separator_are_not_host_options() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/linux-elf/bin/args.elf");
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run-elf", fixture, "--cpu=tier3", "--", "--cpu=tier4", "--present=headless", "--sdmc=/guest"])
        .output().unwrap();
    assert_eq!(output.status.code(), Some(4)); // fixture exits with argc
    let expected = format!("argc=4\nargv: {fixture}\nargv: --cpu=tier4\nargv: --present=headless\nargv: --sdmc=/guest\npagesz=4096\n");
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(String::from_utf8_lossy(&output.stderr).contains("engine=Tier-3"));
}

#[test]
fn fuzzer_rejects_empty_trace_instead_of_reporting_zero_mismatches_as_success() {
    let path = std::env::temp_dir().join(format!("ounce_empty_trace_{}.txt", std::process::id()));
    std::fs::write(&path, b"").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["fuzz-replay", path.to_str().unwrap()]).output().unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no cases"));
}

#[test]
fn fuzzer_does_not_ignore_trace_read_errors() {
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["fuzz-replay", std::env::temp_dir().to_str().unwrap()]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn fuzzer_rejects_zero_generated_cases() {
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["fuzz-replay", "--engine-vs-engine", "--count", "0"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn fuzzer_fails_when_the_engine_accepts_a_host_undefined_word() {
    let path = std::env::temp_dir().join(format!("ounce_undefined_trace_{}.txt", std::process::id()));
    // A deliberately contradictory reference: MOV X0,#1 is implemented, not UNDEFINED.
    std::fs::write(&path, b"U d2800020\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["fuzz-replay", path.to_str().unwrap()]).output().unwrap();
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
        .args(["fuzz-replay", "--engine", "tier3", "--first-difference", bundle.to_str().unwrap(), trace.to_str().unwrap()])
        .output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(bundle.join("diagnosis.json").exists(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(bundle.join("diagnosis.json")).unwrap()).unwrap();
    assert_eq!(report["line_number"], 2);
    assert_eq!(report["word"], "0xd2800020");
    assert_eq!(report["input"]["x"][0], "0x0000000000000000");
    assert_eq!(report["reference"]["x"][0], "0x0000000000000002");
    assert_eq!(report["actual"]["x"][0], "0x0000000000000001");
    assert_eq!(std::fs::read_to_string(bundle.join("case.trace")).unwrap(), mov_trace(2));
    // The bundle is immutable; rerunning never overwrites the original diagnosis.
    let before = std::fs::read(bundle.join("diagnosis.json")).unwrap();
    let again = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["fuzz-replay", "--first-difference", bundle.to_str().unwrap(), trace.to_str().unwrap()]).output().unwrap();
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
        .args(["fuzz-replay", "--engine", "tier3", "--first-difference", bundle.to_str().unwrap(), trace.to_str().unwrap()])
        .output().unwrap();
    assert_eq!(saved.status.code(), Some(1));
    let replay = || Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["replay-instruction", bundle.to_str().unwrap()]).output().unwrap();
    let result = replay();
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.starts_with(b"{"), "{}", String::from_utf8_lossy(&result.stdout));
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
        .output().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[Loader Error]") && stderr.contains("file"), "{stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Successfully loaded package"));
}

fn run_cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ounce-core")).args(args).output().unwrap()
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

#[test]
fn commands_without_a_path_are_usage_errors() {
    for command in ["run", "run-nro", "inspect"] {
        let output = run_cli(&[command]);
        assert_eq!(output.status.code(), Some(2), "{command}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(&format!("'{command}' needs")));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("OUNCE-CORE:"), "no banner before the error");
    }
}

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
    assert!(stderr.contains("issues/new?template=emulation_bug.yml") && stderr.contains("--cpu=tier3"), "{stderr}");
}
