//! `ounce-core diverge` end to end: replay a real program's instructions on the host CPU.
//! Needs an AArch64 host and a C compiler, so it skips elsewhere.

use std::process::Command;

#[test]
fn replaying_hello_on_this_cpu_finds_no_disagreement() {
    if !cfg!(target_arch = "aarch64") || Command::new("cc").arg("--version").output().is_err() {
        eprintln!("skipped: needs an AArch64 host and a C compiler");
        return;
    }
    let nro = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/hello/hello.nro");
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-core")).args(["diverge", nro]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("0 disagreed"), "{text}");
}

#[test]
fn every_committed_program_replays_cleanly_on_this_cpu() {
    if !cfg!(target_arch = "aarch64") || Command::new("cc").arg("--version").output().is_err() {
        eprintln!("skipped: needs an AArch64 host and a C compiler");
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nro");
    let mut checked = 0;
    for dir in std::fs::read_dir(&root).unwrap().flatten().filter(|d| d.path().is_dir()) {
        for file in std::fs::read_dir(dir.path()).unwrap().flatten().filter(|f| f.path().extension().is_some_and(|e| e == "nro")) {
            let out = Command::new(env!("CARGO_BIN_EXE_ounce-core")).arg("diverge").arg(file.path()).output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            assert_eq!(out.status.code(), Some(0), "{}: {text}{}", file.path().display(), String::from_utf8_lossy(&out.stderr));
            assert!(text.contains("0 disagreed"), "{}: {text}", file.path().display());
            checked += 1;
        }
    }
    assert!(checked >= 10, "expected the committed fixtures, found {checked}");
}

#[test]
fn a_missing_program_is_an_error_not_a_pass() {
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-core")).args(["diverge", "/no/such/file.nro"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}
