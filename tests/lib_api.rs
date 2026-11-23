use ounce_core::{EngineKind, RunOptions, StopClass, run_elf, run_nro};
use std::path::PathBuf;

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

#[test]
fn library_run_elf_returns_typed_stop_report() {
    let bytes = std::fs::read(fixture("tests/linux-elf/bin/exitcode.elf")).unwrap();
    let opts = RunOptions {
        engine: EngineKind::Tier3,
        deterministic: true,
        ..RunOptions::default()
    };
    let report = run_elf(&bytes, &opts);
    assert_eq!(report.stop_class(), StopClass::CleanExit);
    assert_eq!(report.guest_exit_status, Some(42));
    assert_eq!(report.to_json()["schema"], "ounce.run.v1");
}

#[test]
fn library_run_nro_returns_typed_stop_report() {
    let bytes = std::fs::read(fixture("tests/fixtures/nro/hello/hello.nro")).unwrap();
    let opts = RunOptions {
        engine: EngineKind::Tier3,
        deterministic: true,
        headless: true,
        ..RunOptions::default()
    };
    let report = run_nro(&bytes, &opts);
    assert_eq!(report.stop_class(), StopClass::CleanExit);
    assert_eq!(report.exit_code, 0);
    assert_eq!(report.to_json()["schema"], "ounce.run.v1");
}

#[test]
fn library_run_nro_rejects_invalid_bytes_with_loader_error() {
    let report = run_nro(b"not-an-nro", &RunOptions::default());
    assert_eq!(report.stop_class(), StopClass::LoaderError);
    assert_eq!(report.exit_code, 17);
}
