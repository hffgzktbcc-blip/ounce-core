//! Metadata-only `ounce.repro.v1` regression corpus checks (#472).
//!
//! Ensures `tests/repro/` contains only JSON manifests (never `.nro` or RomFS binaries), validates
//! their provenance metadata, and replays each referenced fixture through `ounce-core run-nro`
//! with `--present=headless --deterministic` to confirm exact deterministic agreement.
// Without the `nro` feature the NRO tests are compiled out, which leaves some of their helpers unused.
#![cfg_attr(not(feature = "nro"), allow(dead_code, unused_imports))]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_ounce-core");
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn repro_dir() -> PathBuf {
    Path::new(ROOT).join("tests/repro")
}

fn manifest_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(repro_dir())
        .expect("tests/repro directory must exist")
        .map(|e| e.expect("read_dir entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    paths
}

fn scratch_report(name: &str) -> PathBuf {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "ounce_repro_{name}_{}_{}.json",
        std::process::id(),
        CALLS.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn repro_directory_contains_only_metadata_and_no_binaries() {
    let mut json_count = 0usize;
    for entry in std::fs::read_dir(repro_dir()).expect("tests/repro directory must exist") {
        let entry = entry.expect("read_dir entry");
        let path = entry.path();
        assert!(
            path.is_file(),
            "unexpected subdirectory in tests/repro/: {}",
            path.display()
        );
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|e| e.to_str());
        if ext == Some("json") {
            json_count += 1;
        } else {
            assert_eq!(
                file_name, "README.md",
                "tests/repro/ must contain only .json manifests and README.md (no .nro/RomFS binaries): found {file_name}"
            );
        }
    }
    assert!(
        (5..=15).contains(&json_count),
        "expected 5..=15 metadata-only repro manifests in tests/repro/, found {json_count}"
    );
}

#[cfg(feature = "nro")]
#[test]
fn repro_manifests_have_valid_metadata_and_replay_deterministically() {
    let paths = manifest_paths();
    assert!(paths.len() >= 5, "expected at least 5 repro manifests");

    for path in paths {
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let doc: Value = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));

        assert_eq!(doc["schema"], "ounce.repro.v1", "{}", path.display());
        let name = doc["name"].as_str().expect("name string");
        assert!(!name.is_empty(), "{}", path.display());
        assert!(
            doc["source_repo"].as_str().is_some_and(|s| s.starts_with("https://")),
            "{}: source_repo must be an HTTPS URL",
            path.display()
        );
        assert!(
            doc["source_commit"]
                .as_str()
                .is_some_and(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())),
            "{}: source_commit must be a 40-char git SHA",
            path.display()
        );
        assert!(
            doc["toolchain"].as_object().is_some_and(|m| !m.is_empty()),
            "{}: toolchain metadata must be a non-empty object",
            path.display()
        );
        let sha256 = doc["nro_sha256"].as_str().expect("nro_sha256 string");
        assert!(
            sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "{}: nro_sha256 must be 64 hex chars",
            path.display()
        );

        let fixture_rel = doc["fixture_path"].as_str().expect("fixture_path string");
        let fixture_abs = Path::new(ROOT).join(fixture_rel);
        assert!(
            fixture_abs.is_file(),
            "{}: fixture_path {} does not exist",
            path.display(),
            fixture_rel
        );

        let run_args: Vec<&str> = doc["run_args"]
            .as_array()
            .expect("run_args array")
            .iter()
            .map(|v| v.as_str().expect("run_args string"))
            .collect();

        let report_path = scratch_report(name);
        let output = Command::new(BIN)
            .arg("run-nro")
            .arg(&fixture_abs)
            .args(&run_args)
            .arg(format!("--report-json={}", report_path.display()))
            .output()
            .unwrap_or_else(|e| panic!("failed to run {name}: {e}"));

        let report_bytes = std::fs::read(&report_path).unwrap_or_else(|e| {
            panic!(
                "{name}: missing report JSON ({e}); stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
        let _ = std::fs::remove_file(&report_path);
        let report: Value = serde_json::from_slice(&report_bytes).expect("valid report JSON");

        let expected = &doc["expected"];
        for field in [
            "stop_kind",
            "exit_code",
            "insns_executed",
            "frames_presented",
            "framebuffer_hash",
            "audio_frames_mixed",
            "audio_hash",
        ] {
            assert_eq!(
                report[field],
                expected[field],
                "{name} ({}): field `{field}` drifted (actual={}, expected={})",
                path.display(),
                report[field],
                expected[field]
            );
        }
    }
}
