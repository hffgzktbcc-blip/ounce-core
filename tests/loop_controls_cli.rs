//! `--max-frames` and `--press` on the real binary (fixtures are homebrew built in-repo).
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ounce-core")).args(args).output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const INPUT_NRO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/input/input.nro");

#[test]
fn press_reaches_the_guest_pad() {
    let o = run(&["run", INPUT_NRO, "--present=headless", "--press", "A@0", "--press=DRIGHT@0"]);
    assert!(o.status.success());
    assert!(stdout(&o).contains("ounce-input buttons=0x00004001"), "{}", stdout(&o));
}

#[test]
fn no_press_means_no_buttons() {
    let o = run(&["run", INPUT_NRO, "--present=headless"]);
    assert!(o.status.success());
    assert!(stdout(&o).contains("ounce-input buttons=0x00000000"));
}

#[test]
fn press_after_the_run_never_fires() {
    let o = run(&["run", INPUT_NRO, "--present=headless", "--press", "A@999"]);
    assert!(o.status.success());
    assert!(stdout(&o).contains("ounce-input buttons=0x00000000"));
}

#[test]
fn bad_press_is_a_usage_error() {
    let o = run(&["run", INPUT_NRO, "--press", "HOME@3"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("valid: A, B, X, Y"));
}

#[test]
fn max_frames_exits_zero_after_n_frames() {
    let o = run(&["gfx", "--present=headless", "--max-frames", "5"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(stdout(&o).contains("Frames: 5"), "{}", stdout(&o));
}

#[test]
fn max_frames_rejects_zero() {
    assert_eq!(run(&["gfx", "--max-frames", "0"]).status.code(), Some(2));
}

#[test]
fn help_lists_buttons() {
    let o = run(&["--help"]);
    let s = stdout(&o);
    assert!(s.contains("--press") && s.contains("PLUS") && s.contains("--max-frames"));
}
