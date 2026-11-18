//! `OUNCE_TRACE_SUMMARY` and `ounce-core trace-diff` on the real binary (`ounce.trace.v1`,
//! docs/OUNCE_TRACE_V1_SCHEMA.md). The program is the in-repo `input` homebrew fixture.
//!
//! The fixture reads host-clock-derived data after about 4500 instructions, so two runs of even the same engine
//! differ from step 4527 (`v30`). These tests therefore bound the trace to the first 4000 steps.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const INPUT_NRO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/input/input.nro");
const STEPS: &str = "4000";

fn dir(test: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ounce-trace-cli-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn record(out: &Path, cpu: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ounce-core"));
    cmd.args(["run", INPUT_NRO, "--present=headless"]);
    if let Some(cpu) = cpu {
        cmd.arg(format!("--cpu={cpu}"));
    }
    cmd.env("OUNCE_TRACE_SUMMARY", out)
        .env("OUNCE_TRACE_STEPS", STEPS)
        .output()
        .unwrap()
}

fn diff(expected: &Path, actual: &Path, json: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args([
            "trace-diff",
            expected.to_str().unwrap(),
            actual.to_str().unwrap(),
            "--json",
            json.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn engines_agree_on_the_bounded_prefix() {
    let d = dir("agree");
    let (interp, tier3, summary) = (d.join("interp.json"), d.join("tier3.json"), d.join("summary.json"));
    assert!(record(&interp, Some("interpreter")).status.success());
    assert!(record(&tier3, Some("tier3")).status.success());

    let t = read(&interp);
    assert_eq!(t["schema"], "ounce.trace.v1");
    assert_eq!(t["kind"], "trace");
    assert_eq!(t["engine"], "ounce-core");
    assert_eq!(t["step_limit"], 4000);
    assert_eq!(t["steps_recorded"], 4000);
    assert_eq!(t["truncated"], true);
    assert!(
        t["steps_executed"].as_u64().unwrap() > 4000,
        "the run kept going past the bound"
    );
    assert_eq!(t["steps"].as_array().unwrap().len(), 4000, "bounded");
    assert_eq!(
        t["steps"][0]["pc"], "0x0000000080000000",
        "first step is the entry point"
    );

    let out = diff(&interp, &tier3, &summary);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let s = read(&summary);
    assert_eq!(s["kind"], "divergence");
    assert_eq!(s["match"], true);
    assert_eq!(s["compared_steps"], 4000);
    assert_eq!(s["truncated"], true);
}

#[test]
fn default_engine_matches_the_explicit_interpreter() {
    let d = dir("default");
    let (a, b, summary) = (d.join("default.json"), d.join("interp.json"), d.join("summary.json"));
    assert!(record(&a, None).status.success());
    assert!(record(&b, Some("interpreter")).status.success());
    assert_eq!(diff(&a, &b, &summary).status.code(), Some(0));
}

/// The planted regression: corrupt one register write in an otherwise identical trace. The comparer must name
/// that exact step, register, and both values.
#[test]
fn planted_register_regression_is_found_at_the_right_step() {
    let d = dir("planted");
    let (good, bad, summary) = (d.join("good.json"), d.join("bad.json"), d.join("bad-summary.json"));
    assert!(record(&good, Some("tier3")).status.success());
    let mut doc = read(&good);
    let target = (2000..4000)
        .find(|i| {
            doc["steps"][*i]["regs"]
                .as_object()
                .is_some_and(|r| r.keys().any(|k| k.starts_with('x')))
        })
        .unwrap();
    let (reg, real) = {
        let regs = doc["steps"][target]["regs"].as_object().unwrap();
        let (k, v) = regs.iter().find(|(k, _)| k.starts_with('x')).unwrap();
        (k.clone(), v.as_str().unwrap().to_string())
    };
    let planted = "0xdeadbeefdeadbeef";
    doc["steps"][target]["regs"][reg.as_str()] = Value::from(planted);
    std::fs::write(&bad, serde_json::to_vec(&doc).unwrap()).unwrap();

    let out = diff(&good, &bad, &summary);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let s = read(&summary);
    assert_eq!(s["match"], false);
    assert_eq!(s["step"], target as u64);
    assert_eq!(s["field"], "register");
    assert_eq!(s["name"], reg.as_str());
    assert_eq!(s["expected"], real.as_str());
    assert_eq!(s["actual"], planted);
    assert_eq!(s["compared_steps"], target as u64 + 1);
    assert_eq!(s["window"].as_array().unwrap().len(), 4);
    assert_eq!(s["window"][3]["i"], target as u64 - 1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("step {target}")) && stdout.contains(planted),
        "{stdout}"
    );
}

#[test]
fn a_shorter_bound_is_not_a_divergence() {
    let d = dir("bound");
    let (long, short, summary) = (d.join("long.json"), d.join("short.json"), d.join("summary.json"));
    assert!(record(&long, Some("tier3")).status.success());
    let ok = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run", INPUT_NRO, "--present=headless", "--cpu=tier3"])
        .env("OUNCE_TRACE_SUMMARY", &short)
        .env("OUNCE_TRACE_STEPS", "100")
        .output()
        .unwrap();
    assert!(ok.status.success());
    assert_eq!(read(&short)["steps_recorded"], 100);
    assert_eq!(diff(&long, &short, &summary).status.code(), Some(0));
    let s = read(&summary);
    assert_eq!(
        (s["compared_steps"].clone(), s["truncated"].clone()),
        (Value::from(100), Value::from(true))
    );
}

#[test]
fn no_env_var_writes_nothing() {
    let d = dir("off");
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .args(["run", INPUT_NRO, "--present=headless"])
        .env_remove("OUNCE_TRACE_SUMMARY")
        .current_dir(&d)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
}

#[test]
fn trace_diff_reports_bad_input_with_exit_2() {
    let d = dir("bad-input");
    let missing = d.join("missing.json");
    let out = diff(&missing, &missing, &d.join("o.json"));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot read"));
    let usage = Command::new(env!("CARGO_BIN_EXE_ounce-core"))
        .arg("trace-diff")
        .output()
        .unwrap();
    assert_eq!(usage.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&usage.stderr).contains("usage: ounce-core trace-diff"));
}

#[test]
fn tier4_is_refused_instead_of_recording_a_trace_that_silently_stops_early() {
    let d = dir("tier4");
    let out = d.join("t4.json");
    let run = record(&out, Some("tier4"));
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("one-instruction slices"));
    assert!(
        !out.exists(),
        "no trace may be written for an engine that cannot record one"
    );
}

#[test]
fn a_non_ascii_register_name_is_bad_input_not_a_crash() {
    let d = dir("non-ascii");
    let good = d.join("good.json");
    assert!(record(&good, Some("interpreter")).status.success());
    let mut doc = read(&good);
    doc["steps"][0]["regs"] = serde_json::json!({"\u{e9}x": "0x1"});
    let bad = d.join("bad.json");
    std::fs::write(&bad, serde_json::to_vec(&doc).unwrap()).unwrap();
    let out = diff(&bad, &bad, &d.join("o.json"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
}

#[test]
fn trace_diff_creates_the_directory_of_its_json_output() {
    let d = dir("json-dir");
    let t = d.join("t.json");
    assert!(record(&t, Some("interpreter")).status.success());
    let nested = d.join("a").join("b").join("summary.json");
    let out = diff(&t, &t, &nested);
    assert_eq!(out.status.code(), Some(0));
    assert!(nested.exists());
}
