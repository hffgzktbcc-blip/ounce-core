//! `ounce-core diverge <program.nro>`: find the first instruction where this emulator disagrees with the
//! CPU of the machine it runs on.
//!
//! It runs the program once on the interpreter with `OUNCE_TRACE` set (see `trace.rs`), builds the small C
//! helper `tools/hwcheck`, and has the helper replay every recorded register-only instruction on the host
//! CPU from the same starting registers. A mismatch names the program counter, the opcode and the register.
//! Only AArch64 hosts can replay, and only sampled register-only instructions are checked, so a clean
//! result means "no disagreement in what was checked", not a proof.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const HWCHECK_SOURCE: &str = include_str!("../tools/hwcheck/hwcheck.c");
const DEFAULT_BUDGET: u64 = 3_000_000;

struct Options {
    program: PathBuf,
    budget: u64,
    json: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut program = None;
    let mut budget = DEFAULT_BUDGET;
    let mut json = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--budget" => budget = it.next().and_then(|v| v.parse().ok()).filter(|n| *n > 0).ok_or("--budget needs a positive number")?,
            "--json" => json = Some(PathBuf::from(it.next().ok_or("--json needs a path")?)),
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => program = Some(PathBuf::from(other)),
        }
    }
    Ok(Options { program: program.ok_or("usage: ounce-core diverge <program.nro> [--budget N] [--json out.json]")?, budget, json })
}

fn work_dir() -> PathBuf {
    std::env::temp_dir().join(format!("ounce-diverge-{}", std::process::id()))
}

pub(crate) fn build_helper(dir: &Path) -> Result<PathBuf, String> {
    let source = dir.join("hwcheck.c");
    let binary = dir.join("hwcheck");
    std::fs::write(&source, HWCHECK_SOURCE).map_err(|e| format!("could not write the helper source: {e}"))?;
    let status = Command::new("cc")
        .arg("-O1").arg("-o").arg(&binary).arg(&source)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("no C compiler found to build the replay helper ({e}); install Xcode command line tools or gcc"))?;
    if status.success() { Ok(binary) } else { Err("the replay helper did not compile".into()) }
}

fn hex(value: &Value) -> &str {
    value.as_str().unwrap_or("?")
}

/// Returns the process exit code: 0 no disagreement, 1 disagreement found, 2 could not run.
pub fn run_cli(args: &[String]) -> i32 {
    let options = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("diverge: {e}");
            return 2;
        }
    };
    if !cfg!(target_arch = "aarch64") {
        eprintln!("diverge: this machine is not AArch64, so it cannot replay instructions as a hardware reference.");
        return 2;
    }
    if !options.program.is_file() {
        eprintln!("diverge: {} is not a file", options.program.display());
        return 2;
    }
    let dir = work_dir();
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("diverge: could not create {}: {e}", dir.display());
        return 2;
    }
    let outcome = diverge(&options, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.unwrap_or_else(|e| {
        eprintln!("diverge: {e}");
        2
    })
}

fn diverge(options: &Options, dir: &Path) -> Result<i32, String> {
    let helper = build_helper(dir)?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
    let trace = dir.join("trace.bin");
    println!("Running {} on the interpreter and recording instructions (budget {} instructions)...", options.program.display(), options.budget);
    let run = Command::new(&exe)
        .arg("run-nro").arg(&options.program).arg("--present=headless").arg(format!("--sdmc={}", dir.join("sdmc").display()))
        .env("OUNCE_TRACE", &trace).env("OUNCE_NRO_BUDGET", options.budget.to_string())
        .env_remove("OUNCE_CPU")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status()
        .map_err(|e| format!("could not start the interpreter run: {e}"))?;
    if !trace.is_file() {
        return Err(format!("the run recorded no trace (exit {:?}); is this a homebrew .nro?", run.code()));
    }
    let report = options.json.clone().unwrap_or_else(|| dir.join("report.json"));
    println!("Replaying the recorded instructions on this machine's CPU...");
    Command::new(&helper).arg(&trace).arg("--json").arg(&report)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status()
        .map_err(|e| format!("could not run the replay helper: {e}"))?;
    let text = std::fs::read_to_string(&report).map_err(|e| format!("the replay helper wrote no report: {e}"))?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("the replay report is not valid JSON: {e}"))?;
    let (checked, mismatched) = (json["checked"].as_u64().unwrap_or(0), json["mismatched"].as_u64().unwrap_or(0));
    println!("\n{checked} sampled instructions replayed on this CPU, {mismatched} disagreed.");
    let Some(first) = json["mismatches"].as_array().and_then(|m| m.first()) else {
        println!("No disagreement in what was checked. This covers register-only instructions on this one run, not memory or branches, and it is not a proof.");
        return Ok(0);
    };
    println!("\nFirst disagreement: record {} at pc {}, opcode {}", first["record"], hex(&first["pc"]), hex(&first["opcode"]));
    for diff in first["diffs"].as_array().into_iter().flatten() {
        println!("  {}: this CPU gives {}, Ounce-Core gives {}", diff["reg"].as_str().unwrap_or("?"), hex(&diff["expected"]), hex(&diff["observed"]));
    }
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options_and_rejects_bad_ones() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let o = parse(&a(&["x.nro", "--budget", "100", "--json", "r.json"])).unwrap();
        assert_eq!((o.budget, o.json.as_deref()), (100, Some(Path::new("r.json"))));
        assert!(parse(&a(&[])).is_err());
        assert!(parse(&a(&["x.nro", "--budget", "0"])).is_err());
        assert!(parse(&a(&["x.nro", "--nope"])).is_err());
    }
}
