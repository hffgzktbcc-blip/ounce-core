//! `ounce-core diverge <program.nro>`: find the first instruction where this emulator disagrees with the
//! CPU of the machine it runs on.
//!
//! It runs the program on the interpreter, then Tier-3 and Tier-4 (one-instruction slices), each with `OUNCE_TRACE` set (see `trace.rs`), builds the small C
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
    engines: Vec<Engine>,
}

/// Which execution engine produced the recorded trace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Engine {
    Interpreter,
    Tier3,
    Tier4,
}

impl Engine {
    fn label(self) -> &'static str {
        match self {
            Engine::Interpreter => "interpreter",
            Engine::Tier3 => "Tier-3",
            Engine::Tier4 => "Tier-4",
        }
    }
    /// The `--cpu=` value, or `None` for the one-instruction interpreter.
    fn cpu_flag(self) -> Option<&'static str> {
        match self {
            Engine::Interpreter => None,
            Engine::Tier3 => Some("tier3"),
            Engine::Tier4 => Some("tier4"),
        }
    }
    /// The name the run loop prints in `[CPU] Block engine: <name>`, used to catch silent fallbacks.
    fn banner(self) -> Option<&'static str> {
        match self {
            Engine::Interpreter => None,
            Engine::Tier3 => Some("Tier-3"),
            Engine::Tier4 => Some("Tier-4"),
        }
    }
}

fn parse_engines(list: &str) -> Result<Vec<Engine>, String> {
    let mut out = Vec::new();
    for name in list.split(',') {
        let engine = match name.trim().to_ascii_lowercase().as_str() {
            "interpreter" | "interp" | "tier1" => Engine::Interpreter,
            "tier3" | "3" => Engine::Tier3,
            "tier4" | "4" => Engine::Tier4,
            "all" => {
                out.extend([Engine::Interpreter, Engine::Tier3, Engine::Tier4]);
                continue;
            }
            other => return Err(format!("unknown engine '{other}' (expected interpreter, tier3, tier4 or all)")),
        };
        out.push(engine);
    }
    out.dedup();
    if out.is_empty() {
        return Err("--engine needs at least one engine".into());
    }
    Ok(out)
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut program = None;
    let mut budget = DEFAULT_BUDGET;
    let mut json = None;
    let mut engines = vec![Engine::Interpreter, Engine::Tier3, Engine::Tier4];
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--budget" => budget = it.next().and_then(|v| v.parse().ok()).filter(|n| *n > 0).ok_or("--budget needs a positive number")?,
            "--engine" => engines = parse_engines(it.next().ok_or("--engine needs a value")?)?,
            "--json" => json = Some(PathBuf::from(it.next().ok_or("--json needs a path")?)),
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => program = Some(PathBuf::from(other)),
        }
    }
    Ok(Options { program: program.ok_or("usage: ounce-core diverge <program.nro> [--budget N] [--engine interpreter,tier3,tier4] [--json out.json]")?, budget, json, engines })
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

/// One engine's replay result.
struct Outcome {
    engine: Engine,
    checked: u64,
    mismatched: u64,
    mismatches: Vec<Value>,
}

/// Why the engine could not be traced, found by reading the run's stdout banner.
fn engine_fallback(engine: Engine, stdout: &str) -> Option<String> {
    let want = engine.banner()?;
    let banner = stdout.lines().find_map(|l| l.strip_prefix("[CPU] Block engine: "));
    match banner {
        Some(got) if got.trim() == want => None,
        Some(got) => Some(format!("it ran {} instead", got.trim())),
        None => Some("the run never selected a block engine".into()),
    }
}

fn trace_engine(options: &Options, dir: &Path, helper: &Path, exe: &Path, engine: Engine) -> Result<Outcome, String> {
    let tag = engine.label().to_ascii_lowercase().replace('-', "");
    let trace = dir.join(format!("trace-{tag}.bin"));
    println!("Running {} on the {} and recording instructions (budget {} instructions)...", options.program.display(), engine.label(), options.budget);
    let mut cmd = Command::new(exe);
    cmd.arg("run-nro").arg(&options.program).arg("--present=headless").arg(format!("--sdmc={}", dir.join(format!("sdmc-{tag}")).display()));
    if let Some(flag) = engine.cpu_flag() {
        cmd.arg(format!("--cpu={flag}"));
    }
    let run = cmd
        .env("OUNCE_TRACE", &trace).env("OUNCE_NRO_BUDGET", options.budget.to_string())
        .env_remove("OUNCE_CPU")
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("could not start the {} run: {e}", engine.label()))?;
    if let Some(why) = engine_fallback(engine, &String::from_utf8_lossy(&run.stdout)) {
        return Err(format!("{} could not be traced: {why}. Not counted as checked.", engine.label()));
    }
    for line in String::from_utf8_lossy(&run.stderr).lines().filter(|l| l.starts_with("[Trace]")) {
        println!("  {line}");
    }
    if !trace.is_file() {
        return Err(format!("the {} run recorded no trace (exit {:?}); is this a homebrew .nro?", engine.label(), run.status.code()));
    }
    let report = dir.join(format!("report-{tag}.json"));
    println!("Replaying the {}'s recorded instructions on this machine's CPU...", engine.label());
    Command::new(helper).arg(&trace).arg("--json").arg(&report)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status()
        .map_err(|e| format!("could not run the replay helper: {e}"))?;
    let text = std::fs::read_to_string(&report).map_err(|e| format!("the replay helper wrote no report: {e}"))?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("the replay report is not valid JSON: {e}"))?;
    Ok(Outcome {
        engine,
        checked: json["checked"].as_u64().unwrap_or(0),
        mismatched: json["mismatched"].as_u64().unwrap_or(0),
        mismatches: json["mismatches"].as_array().cloned().unwrap_or_default(),
    })
}

fn diverge(options: &Options, dir: &Path) -> Result<i32, String> {
    let helper = build_helper(dir)?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
    let explicit = options.engines.len() < 3;
    let mut outcomes = Vec::new();
    let mut skipped: Vec<(Engine, String)> = Vec::new();
    for &engine in &options.engines {
        match trace_engine(options, dir, &helper, &exe, engine) {
            Ok(o) => outcomes.push(o),
            // The default set tolerates a Tier-4 that this build or host cannot run, and says so; a tier the
            // user asked for by name must run, otherwise the answer would be a silent pass.
            Err(e) if !explicit && engine == Engine::Tier4 && e.contains("could not be traced") => skipped.push((engine, e)),
            Err(e) => return Err(e),
        }
    }
    let mut total_mismatched = 0;
    for o in &outcomes {
        println!("\n{}: {} sampled instructions replayed on this CPU, {} disagreed.", o.engine.label(), o.checked, o.mismatched);
        total_mismatched += o.mismatched;
        if let Some(first) = o.mismatches.first() {
            println!("First {} disagreement: record {} at pc {}, opcode {}", o.engine.label(), first["record"], hex(&first["pc"]), hex(&first["opcode"]));
            for diff in first["diffs"].as_array().into_iter().flatten() {
                println!("  {}: this CPU gives {}, {} gives {}", diff["reg"].as_str().unwrap_or("?"), hex(&diff["expected"]), o.engine.label(), hex(&diff["observed"]));
            }
        }
    }
    for (engine, why) in &skipped {
        println!("\n{}: NOT CHECKED. {why} Rebuild with `--features native-jit` on a host that can run Tier-4, or pass --engine to require it.", engine.label());
    }
    if let Some(path) = &options.json {
        let engines: Vec<Value> = outcomes
            .iter()
            .map(|o| serde_json::json!({"engine": o.engine.label(), "checked": o.checked, "mismatched": o.mismatched, "mismatches": o.mismatches}))
            .collect();
        let report = serde_json::json!({
            "checked": outcomes.iter().map(|o| o.checked).sum::<u64>(),
            "mismatched": total_mismatched,
            "mismatches": outcomes.iter().flat_map(|o| o.mismatches.iter().cloned().map(move |mut m| { m["engine"] = o.engine.label().into(); m })).collect::<Vec<_>>(),
            "engines": engines,
            "not_checked": skipped.iter().map(|(e, why)| serde_json::json!({"engine": e.label(), "reason": why})).collect::<Vec<_>>(),
        });
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap_or_default()).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    }
    if total_mismatched == 0 {
        println!("\nNo disagreement in what was checked. This covers sampled register-only instructions on this one run, executed one at a time by each engine; not memory, branches, or multi-instruction block fusion, and it is not a proof.");
        Ok(0)
    } else {
        Ok(1)
    }
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
        assert_eq!(parse(&a(&["x.nro"])).unwrap().engines, [Engine::Interpreter, Engine::Tier3, Engine::Tier4]);
        assert_eq!(parse(&a(&["x.nro", "--engine", "tier4,3"])).unwrap().engines, [Engine::Tier4, Engine::Tier3]);
        assert!(parse(&a(&["x.nro", "--engine", "tier9"])).is_err());
    }

    #[test]
    fn a_silent_tier_fallback_is_detected() {
        assert_eq!(engine_fallback(Engine::Tier3, "x\n[CPU] Block engine: Tier-3\n"), None);
        assert!(engine_fallback(Engine::Tier4, "[CPU] Block engine: Tier-3\n").unwrap().contains("Tier-3 instead"));
        assert!(engine_fallback(Engine::Tier4, "nothing").is_some());
        assert_eq!(engine_fallback(Engine::Interpreter, "nothing"), None);
    }
}
