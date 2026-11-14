//! `ounce-core diverge <program.nro|program.elf>`: check whether engines (or this machine's CPU)
//! disagree on a guest program.
//!
//! - `.nro` targets run on each engine in one-instruction slices with `OUNCE_TRACE` set (see `trace.rs`),
//!   build `tools/hwcheck`, and replay sampled register-only instructions on the AArch64 host CPU.
//! - Static AArch64 Linux ELF targets (`\x7fELF`) run via `ounce-core run-elf --deterministic` across
//!   `interpreter`, `tier3`, and `tier4`, comparing exit codes, stdout/stderr, and stop diagnoses (plus
//!   native Linux ELF execution when running on a Linux AArch64 host).

use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const HWCHECK_SOURCE: &str = include_str!("../tools/hwcheck/hwcheck.c");
const DEFAULT_BUDGET: u64 = 3_000_000;
const USAGE: &str = "usage: ounce-core diverge <program.nro|program.elf> [--budget N] [--engine interpreter,tier3,tier4] [--json out.json] [-- program_args...]";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ProgramKind {
    Nro,
    Elf,
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    program: PathBuf,
    budget: u64,
    json: Option<PathBuf>,
    engines: Vec<Engine>,
    program_args: Vec<String>,
}

/// Which execution engine produced the recorded trace or ELF run.
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

    /// The `--cpu=` value for `run-nro`, or `None` for the one-instruction interpreter.
    fn nro_cpu_flag(self) -> Option<&'static str> {
        match self {
            Engine::Interpreter => None,
            Engine::Tier3 => Some("tier3"),
            Engine::Tier4 => Some("tier4"),
        }
    }

    /// The `--cpu=` value for `run-elf` (which defaults to Tier-3 when `--cpu=` is omitted).
    fn elf_cpu_flag(self) -> &'static str {
        match self {
            Engine::Interpreter => "interpreter",
            Engine::Tier3 => "tier3",
            Engine::Tier4 => "tier4",
        }
    }

    /// The name `run-nro` prints in `[CPU] Block engine: <name>`, used to catch silent fallbacks.
    fn nro_banner(self) -> Option<&'static str> {
        match self {
            Engine::Interpreter => None,
            Engine::Tier3 => Some("Tier-3"),
            Engine::Tier4 => Some("Tier-4"),
        }
    }

    /// The engine name `run-elf` prints in `[run-elf] engine=<name>`.
    fn elf_banner(self) -> &'static str {
        match self {
            Engine::Interpreter => "Interpreter",
            Engine::Tier3 => "Tier-3",
            Engine::Tier4 => "Tier-4",
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
    let mut program_args = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--" => {
                program_args.extend(it.cloned());
                break;
            }
            "--budget" => budget = it.next().and_then(|v| v.parse().ok()).filter(|n| *n > 0).ok_or("--budget needs a positive number")?,
            "--engine" => engines = parse_engines(it.next().ok_or("--engine needs a value")?)?,
            "--json" => json = Some(PathBuf::from(it.next().ok_or("--json needs a path")?)),
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other if program.is_none() => program = Some(PathBuf::from(other)),
            other => program_args.push(other.to_string()),
        }
    }
    Ok(Options {
        program: program.ok_or(USAGE)?,
        budget,
        json,
        engines,
        program_args,
    })
}

/// Identifies NRO (`NRO0` at offset 0x10) or ELF (`\x7fELF` at offset 0) from header bytes, falling
/// back to `.elf`/`.nro` extension when header magic is truncated or unrecognizable.
fn detect_program_kind(path: &Path, header: &[u8]) -> Result<ProgramKind, String> {
    if header.starts_with(b"\x7fELF") {
        return Ok(ProgramKind::Elf);
    }
    if header.get(0x10..0x14) == Some(b"NRO0".as_slice()) {
        return Ok(ProgramKind::Nro);
    }
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("elf") => Ok(ProgramKind::Elf),
        Some("nro") => Ok(ProgramKind::Nro),
        _ => Err(format!("unrecognised program format for {} (expected .nro or static AArch64 Linux ELF)", path.display())),
    }
}

fn detect_program(path: &Path) -> Result<ProgramKind, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut header = [0u8; 64];
    let mut read = 0;
    while read < header.len() {
        let n = file.read(&mut header[read..]).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        read += n;
    }
    detect_program_kind(path, &header[..read])
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

fn write_json_report(path: Option<&Path>, report: &Value) -> Result<(), String> {
    let Some(path) = path else { return Ok(()) };
    let text = serde_json::to_string_pretty(report).unwrap_or_default();
    std::fs::write(path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
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
    if !options.program.is_file() {
        eprintln!("diverge: {} is not a file", options.program.display());
        return 2;
    }
    let kind = match detect_program(&options.program) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("diverge: {e}");
            return 2;
        }
    };
    if kind == ProgramKind::Nro && !options.program_args.is_empty() {
        eprintln!("diverge: program arguments are only supported for Linux ELF targets");
        return 2;
    }
    if kind == ProgramKind::Nro && !cfg!(target_arch = "aarch64") {
        eprintln!("diverge: this machine is not AArch64, so it cannot replay instructions as a hardware reference.");
        return 2;
    }
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("diverge: cannot find this program: {e}");
            return 2;
        }
    };
    let outcome = match kind {
        ProgramKind::Nro => {
            let dir = work_dir();
            let _ = std::fs::remove_dir_all(&dir);
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("diverge: could not create {}: {e}", dir.display());
                return 2;
            }
            let r = diverge_nro(&options, &dir, &exe);
            let _ = std::fs::remove_dir_all(&dir);
            r
        }
        ProgramKind::Elf => diverge_elf(&options, &exe),
    };
    outcome.unwrap_or_else(|e| {
        eprintln!("diverge: {e}");
        2
    })
}

/// One engine's NRO instruction-replay result.
struct Outcome {
    engine: Engine,
    checked: u64,
    mismatched: u64,
    mismatches: Vec<Value>,
}

/// Why the NRO engine could not be traced, found by reading the run's stdout banner.
fn engine_fallback(engine: Engine, stdout: &str) -> Option<String> {
    let want = engine.nro_banner()?;
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
    if let Some(flag) = engine.nro_cpu_flag() {
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

fn diverge_nro(options: &Options, dir: &Path, exe: &Path) -> Result<i32, String> {
    let helper = build_helper(dir)?;
    let explicit = options.engines.len() < 3;
    let mut outcomes = Vec::new();
    let mut skipped: Vec<(Engine, String)> = Vec::new();
    for &engine in &options.engines {
        match trace_engine(options, dir, &helper, exe, engine) {
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
    let engines: Vec<Value> = outcomes
        .iter()
        .map(|o| serde_json::json!({"engine": o.engine.label(), "checked": o.checked, "mismatched": o.mismatched, "mismatches": o.mismatches}))
        .collect();
    let report = serde_json::json!({
        "mode": "nro",
        "checked": outcomes.iter().map(|o| o.checked).sum::<u64>(),
        "mismatched": total_mismatched,
        "mismatches": outcomes.iter().flat_map(|o| o.mismatches.iter().cloned().map(move |mut m| { m["engine"] = o.engine.label().into(); m })).collect::<Vec<_>>(),
        "engines": engines,
        "not_checked": skipped.iter().map(|(e, why)| serde_json::json!({"engine": e.label(), "reason": why})).collect::<Vec<_>>(),
    });
    write_json_report(options.json.as_deref(), &report)?;
    if total_mismatched == 0 {
        println!("\nNo disagreement in what was checked. This covers sampled register-only instructions on this one run, executed one at a time by each engine; not memory, branches, or multi-instruction block fusion, and it is not a proof.");
        Ok(0)
    } else {
        Ok(1)
    }
}

/// Argument list passed to `ounce-core` when driving an ELF under one engine.
fn build_elf_command_args(options: &Options, engine: Engine) -> Vec<String> {
    let mut args = vec![
        "run-elf".to_string(),
        options.program.display().to_string(),
        format!("--cpu={}", engine.elf_cpu_flag()),
        "--deterministic".to_string(),
        format!("--max-insns={}", options.budget),
    ];
    if !options.program_args.is_empty() {
        args.push("--".to_string());
        args.extend(options.program_args.iter().cloned());
    }
    args
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ElfSummary {
    engine: Option<String>,
    instructions: u64,
    syscalls: u64,
    diagnosis: Option<String>,
    stop_kind: &'static str,
    guest_stderr: Vec<u8>,
}

fn parse_run_elf_stderr(stderr: &[u8]) -> ElfSummary {
    let text = String::from_utf8_lossy(stderr);
    let mut out = ElfSummary { stop_kind: "unknown", ..Default::default() };
    let mut guest = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("[run-elf] engine=") {
            let mut fields = rest.split_whitespace();
            out.engine = fields.next().map(str::to_string);
            for kv in fields {
                if let Some(v) = kv.strip_prefix("instructions=") {
                    out.instructions = v.parse().unwrap_or(0);
                } else if let Some(v) = kv.strip_prefix("syscalls=") {
                    out.syscalls = v.parse().unwrap_or(0);
                }
            }
        } else if let Some(diag) = line.strip_prefix("run-elf: DIAGNOSIS:") {
            out.diagnosis = Some(diag.trim().to_string());
        } else if line.starts_with("[Tier-4] ") || line.starts_with("[CPU] ") {
            // Runtime engine-selection notices are not guest stderr.
        } else {
            guest.push_str(line);
            guest.push('\n');
        }
    }
    out.stop_kind = match out.diagnosis.as_deref() {
        None if out.engine.is_some() => "exited",
        None => "unknown",
        Some(d) if d.starts_with("instruction budget") => "budget",
        Some(d) if d.starts_with("cannot load ELF") || d.starts_with("cannot build process stack") => "loader",
        Some(d) if d.contains("syscall") => "unhandled-svc",
        Some(d) if d.contains("Unimplemented") => "unimplemented",
        Some(_) => "fault",
    };
    out.guest_stderr = guest.into_bytes();
    out
}

fn elf_engine_fallback(engine: Engine, summary: &ElfSummary) -> Option<String> {
    match summary.engine.as_deref() {
        Some("none") => None,
        Some(got) if got == engine.elf_banner() => None,
        Some(got) => Some(format!("it ran {got} instead")),
        None => Some("the run produced no [run-elf] engine summary".into()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ElfRun {
    engine: Engine,
    exit_code: Option<i32>,
    stop_kind: &'static str,
    instructions: u64,
    syscalls: u64,
    diagnosis: Option<String>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeRun {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ElfMismatch {
    engine: String,
    reference: String,
    kind: &'static str,
    detail: String,
    json: Value,
}

#[derive(Debug)]
struct ElfReport {
    checked: u64,
    mismatched: u64,
    reference_label: String,
    mismatches: Vec<ElfMismatch>,
    per_engine_mismatches: Vec<(Engine, Vec<Value>)>,
}

fn byte_diff_preview(expected: &[u8], observed: &[u8]) -> (usize, Value) {
    let idx = expected.iter().zip(observed.iter()).position(|(a, b)| a != b).unwrap_or_else(|| expected.len().min(observed.len()));
    let slice = |bytes: &[u8]| {
        let start = idx.saturating_sub(16);
        let end = (idx + 32).min(bytes.len());
        String::from_utf8_lossy(&bytes[start..end]).into_owned()
    };
    (
        idx,
        serde_json::json!({
            "first_diff_byte": idx,
            "expected_len": expected.len(),
            "observed_len": observed.len(),
            "expected_preview": slice(expected),
            "observed_preview": slice(observed),
        }),
    )
}

struct ObservedRun<'a> {
    label: &'a str,
    exit_code: Option<i32>,
    stop_kind: Option<&'a str>,
    diagnosis: Option<&'a str>,
    stdout: &'a [u8],
    stderr: &'a [u8],
}

fn compare_pair(obs: &ObservedRun<'_>, exp: &ObservedRun<'_>) -> Vec<ElfMismatch> {
    let mut diffs = Vec::new();
    if let (Some(es), Some(os)) = (exp.stop_kind, obs.stop_kind)
        && es != os
    {
        diffs.push(ElfMismatch {
            engine: obs.label.into(),
            reference: exp.label.into(),
            kind: "stop",
            detail: format!("{} stopped with '{es}', {} stopped with '{os}'", exp.label, obs.label),
            json: serde_json::json!({"engine": obs.label, "reference": exp.label, "kind": "stop", "expected": es, "observed": os}),
        });
    }
    if exp.exit_code != obs.exit_code {
        diffs.push(ElfMismatch {
            engine: obs.label.into(),
            reference: exp.label.into(),
            kind: "exit_code",
            detail: format!("{} exited {:?}, {} exited {:?}", exp.label, exp.exit_code, obs.label, obs.exit_code),
            json: serde_json::json!({"engine": obs.label, "reference": exp.label, "kind": "exit_code", "expected": exp.exit_code, "observed": obs.exit_code}),
        });
    }
    if exp.stdout != obs.stdout {
        let (idx, info) = byte_diff_preview(exp.stdout, obs.stdout);
        diffs.push(ElfMismatch {
            engine: obs.label.into(),
            reference: exp.label.into(),
            kind: "stdout",
            detail: format!("stdout differs at byte {idx} ({} vs {} bytes)", exp.stdout.len(), obs.stdout.len()),
            json: serde_json::json!({
                "engine": obs.label, "reference": exp.label, "kind": "stdout",
                "first_diff_byte": info["first_diff_byte"],
                "expected": info["expected_preview"], "observed": info["observed_preview"],
                "expected_len": info["expected_len"], "observed_len": info["observed_len"],
            }),
        });
    }
    if exp.stderr != obs.stderr {
        let (idx, info) = byte_diff_preview(exp.stderr, obs.stderr);
        diffs.push(ElfMismatch {
            engine: obs.label.into(),
            reference: exp.label.into(),
            kind: "stderr",
            detail: format!("guest stderr differs at byte {idx} ({} vs {} bytes)", exp.stderr.len(), obs.stderr.len()),
            json: serde_json::json!({
                "engine": obs.label, "reference": exp.label, "kind": "stderr",
                "first_diff_byte": info["first_diff_byte"],
                "expected": info["expected_preview"], "observed": info["observed_preview"],
            }),
        });
    }
    if let (Some(ed), Some(od)) = (exp.diagnosis, obs.diagnosis)
        && ed != od
    {
        diffs.push(ElfMismatch {
            engine: obs.label.into(),
            reference: exp.label.into(),
            kind: "diagnosis",
            detail: format!("{} diagnosed '{ed}', {} diagnosed '{od}'", exp.label, obs.label),
            json: serde_json::json!({"engine": obs.label, "reference": exp.label, "kind": "diagnosis", "expected": ed, "observed": od}),
        });
    }
    diffs
}

fn as_observed(r: &ElfRun) -> ObservedRun<'_> {
    ObservedRun {
        label: r.engine.label(),
        exit_code: r.exit_code,
        stop_kind: Some(r.stop_kind),
        diagnosis: r.diagnosis.as_deref(),
        stdout: &r.stdout,
        stderr: &r.stderr,
    }
}

fn compare_elf_runs(runs: &[ElfRun], native: Option<&NativeRun>) -> ElfReport {
    let mut mismatches = Vec::new();
    let mut per_engine: Vec<(Engine, Vec<Value>)> = runs.iter().map(|r| (r.engine, Vec::new())).collect();
    let mut checked = 0u64;

    if let Some((first, rest)) = runs.split_first() {
        let ref_obs = as_observed(first);
        for (idx, run) in rest.iter().enumerate() {
            checked += 1;
            for d in compare_pair(&as_observed(run), &ref_obs) {
                per_engine[idx + 1].1.push(d.json.clone());
                mismatches.push(d);
            }
        }
    }

    let reference_label = if let (Some(nat), Some(first)) = (native, runs.first()) {
        checked += 1;
        let nat_obs = ObservedRun {
            label: "host (Linux AArch64)",
            exit_code: nat.exit_code,
            stop_kind: None,
            diagnosis: None,
            stdout: &nat.stdout,
            stderr: &nat.stderr,
        };
        for d in compare_pair(&as_observed(first), &nat_obs) {
            per_engine[0].1.push(d.json.clone());
            mismatches.push(d);
        }
        "host (Linux AArch64)".to_string()
    } else {
        runs.first().map(|r| r.engine.label().to_string()).unwrap_or_else(|| "none".to_string())
    };

    ElfReport {
        checked,
        mismatched: mismatches.len() as u64,
        reference_label,
        mismatches,
        per_engine_mismatches: per_engine,
    }
}

fn run_elf_engine(options: &Options, exe: &Path, engine: Engine) -> Result<ElfRun, String> {
    let args = build_elf_command_args(options, engine);
    println!("Running {} on {} (--cpu={} --deterministic, budget {} instructions)...", options.program.display(), engine.label(), engine.elf_cpu_flag(), options.budget);
    let out = Command::new(exe)
        .args(&args)
        .env_remove("OUNCE_CPU")
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("could not start the {} run: {e}", engine.label()))?;
    let summary = parse_run_elf_stderr(&out.stderr);
    if summary.engine.as_deref() == Some("none") {
        let diag = summary.diagnosis.unwrap_or_else(|| format!("load failed with status {:?}", out.status.code()));
        return Err(diag);
    }
    if let Some(why) = elf_engine_fallback(engine, &summary) {
        return Err(format!("{} could not be checked: {why}", engine.label()));
    }
    Ok(ElfRun {
        engine,
        exit_code: out.status.code(),
        stop_kind: summary.stop_kind,
        instructions: summary.instructions,
        syscalls: summary.syscalls,
        diagnosis: summary.diagnosis,
        stdout: out.stdout,
        stderr: summary.guest_stderr,
    })
}

fn try_run_native_elf(options: &Options) -> Result<NativeRun, String> {
    if !cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        return Err(format!(
            "native ELF execution requires a Linux AArch64 host (this host is {}/{}); compared emulated engines only",
            std::env::consts::OS,
            std::env::consts::ARCH,
        ));
    }
    let exec_path = if options.program.components().count() == 1 {
        Path::new(".").join(&options.program)
    } else {
        options.program.clone()
    };
    let out = Command::new(&exec_path)
        .args(&options.program_args)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("native Linux host could not execute {}: {e}", options.program.display()))?;
    Ok(NativeRun {
        exit_code: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

fn diverge_elf(options: &Options, exe: &Path) -> Result<i32, String> {
    let explicit = options.engines.len() < 3;
    let mut runs = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();

    for &engine in &options.engines {
        match run_elf_engine(options, exe, engine) {
            Ok(r) => runs.push(r),
            Err(e) if !explicit && engine == Engine::Tier4 && e.contains("could not be checked") => {
                skipped.push((engine.label().to_string(), e));
            }
            Err(e) => return Err(e),
        }
    }

    let native = match try_run_native_elf(options) {
        Ok(n) => Some(n),
        Err(why) => {
            skipped.push(("host (Linux AArch64)".to_string(), why));
            None
        }
    };

    if runs.is_empty() || (runs.len() < 2 && native.is_none()) {
        return Err("need at least two engines (or a Linux AArch64 host) to compare an ELF for divergence".into());
    }

    let report = compare_elf_runs(&runs, native.as_ref());
    println!();
    for run in &runs {
        println!(
            "{}: exit {:?} ({}, {} instructions, {} syscalls), stdout {} bytes",
            run.engine.label(),
            run.exit_code,
            run.stop_kind,
            run.instructions,
            run.syscalls,
            run.stdout.len(),
        );
    }
    if let Some(nat) = &native {
        println!("host (Linux AArch64): exit {:?}, stdout {} bytes", nat.exit_code, nat.stdout.len());
    }
    if let Some(first) = report.mismatches.first() {
        println!("\nFirst disagreement ({} vs {}, {}): {}", first.engine, first.reference, first.kind, first.detail);
    }
    for (name, why) in &skipped {
        println!("\n{name}: NOT CHECKED. {why}");
    }

    let ref_run = &runs[0];
    let engines_json: Vec<Value> = runs
        .iter()
        .zip(report.per_engine_mismatches.iter())
        .map(|(r, (_, ms))| {
            serde_json::json!({
                "engine": r.engine.label(),
                "checked": 1,
                "mismatched": ms.len(),
                "exit_code": r.exit_code,
                "stop": r.stop_kind,
                "instructions": r.instructions,
                "syscalls": r.syscalls,
                "diagnosis": r.diagnosis,
                "mismatches": ms,
            })
        })
        .collect();
    let json = serde_json::json!({
        "mode": "elf",
        "checked": report.checked,
        "mismatched": report.mismatched,
        "mismatches": report.mismatches.iter().map(|m| m.json.clone()).collect::<Vec<_>>(),
        "reference": {
            "engine": report.reference_label,
            "exit_code": native.as_ref().map_or(ref_run.exit_code, |n| n.exit_code),
            "stop": if native.is_some() { "exited" } else { ref_run.stop_kind },
            "stdout_bytes": native.as_ref().map_or(ref_run.stdout.len(), |n| n.stdout.len()),
        },
        "engines": engines_json,
        "not_checked": skipped.iter().map(|(e, why)| serde_json::json!({"engine": e, "reason": why})).collect::<Vec<_>>(),
    });
    write_json_report(options.json.as_deref(), &json)?;

    if report.mismatched == 0 {
        println!(
            "\nNo disagreement in what was checked ({} comparison{}, 0 disagreed). ELF mode compares exit codes, stdout/stderr, and stop diagnoses under --deterministic; per-instruction hardware replay is NRO-only.",
            report.checked,
            if report.checked == 1 { "" } else { "s" },
        );
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
        assert!(o.program_args.is_empty());
        assert!(parse(&a(&[])).is_err());
        assert!(parse(&a(&["x.nro", "--budget", "0"])).is_err());
        assert!(parse(&a(&["x.nro", "--nope"])).is_err());
        assert_eq!(parse(&a(&["x.nro"])).unwrap().engines, [Engine::Interpreter, Engine::Tier3, Engine::Tier4]);
        assert_eq!(parse(&a(&["x.nro", "--engine", "tier4,3"])).unwrap().engines, [Engine::Tier4, Engine::Tier3]);
        assert!(parse(&a(&["x.nro", "--engine", "tier9"])).is_err());

        let elf = parse(&a(&["prog.elf", "--engine", "interp,tier3", "--", "--flag", "val"])).unwrap();
        assert_eq!(elf.program, PathBuf::from("prog.elf"));
        assert_eq!(elf.engines, [Engine::Interpreter, Engine::Tier3]);
        assert_eq!(elf.program_args, ["--flag", "val"]);

        let positional = parse(&a(&["prog.elf", "one", "two"])).unwrap();
        assert_eq!(positional.program_args, ["one", "two"]);
    }

    #[test]
    fn detects_nro_elf_and_unknown_formats() {
        let mut elf = [0u8; 64];
        elf[..4].copy_from_slice(b"\x7fELF");
        assert_eq!(detect_program_kind(Path::new("bin/no_ext"), &elf).unwrap(), ProgramKind::Elf);

        let mut nro = [0u8; 32];
        nro[0x10..0x14].copy_from_slice(b"NRO0");
        assert_eq!(detect_program_kind(Path::new("bin/no_ext"), &nro).unwrap(), ProgramKind::Nro);

        assert_eq!(detect_program_kind(Path::new("fallback.ELF"), &[]).unwrap(), ProgramKind::Elf);
        assert_eq!(detect_program_kind(Path::new("fallback.nro"), &[]).unwrap(), ProgramKind::Nro);
        assert!(detect_program_kind(Path::new("unknown.bin"), b"NSO0").is_err());
    }

    #[test]
    fn builds_elf_cli_args_with_deterministic_and_guest_args() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let opts = parse(&a(&["tests/linux-elf/bin/args.elf", "--budget", "50000", "--", "one", "--cpu=ignored"])).unwrap();
        assert_eq!(
            build_elf_command_args(&opts, Engine::Interpreter),
            ["run-elf", "tests/linux-elf/bin/args.elf", "--cpu=interpreter", "--deterministic", "--max-insns=50000", "--", "one", "--cpu=ignored"]
        );
        assert_eq!(
            build_elf_command_args(&opts, Engine::Tier3),
            ["run-elf", "tests/linux-elf/bin/args.elf", "--cpu=tier3", "--deterministic", "--max-insns=50000", "--", "one", "--cpu=ignored"]
        );
    }

    #[test]
    fn parses_run_elf_stderr_and_catches_fallback() {
        let s = parse_run_elf_stderr(b"guest err\n[Tier-4] Not available: rebuild with `--features native-jit`; running Tier-3.\n[run-elf] engine=Tier-3 instructions=42 syscalls=3\n");
        assert_eq!(s.engine.as_deref(), Some("Tier-3"));
        assert_eq!((s.instructions, s.syscalls, s.stop_kind), (42, 3, "exited"));
        assert_eq!(s.guest_stderr, b"guest err\n");
        assert!(elf_engine_fallback(Engine::Tier3, &s).is_none());
        assert!(elf_engine_fallback(Engine::Tier4, &s).unwrap().contains("Tier-3 instead"));

        let load_err = parse_run_elf_stderr(b"[run-elf] engine=none instructions=0 syscalls=0\nrun-elf: DIAGNOSIS: cannot load ELF: malformed\n");
        assert_eq!(load_err.stop_kind, "loader");
        assert!(elf_engine_fallback(Engine::Interpreter, &load_err).is_none());
    }

    #[test]
    fn compares_elf_runs_across_engines_and_native() {
        let mk = |engine, exit_code, stop_kind, stdout: &[u8]| ElfRun {
            engine,
            exit_code: Some(exit_code),
            stop_kind,
            instructions: 100,
            syscalls: 2,
            diagnosis: None,
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
        };
        let r0 = mk(Engine::Interpreter, 42, "exited", b"hello 42\n");
        let r1 = mk(Engine::Tier3, 42, "exited", b"hello 42\n");
        let ok = compare_elf_runs(&[r0.clone(), r1.clone()], None);
        assert_eq!((ok.checked, ok.mismatched), (1, 0));

        let bad_exit = mk(Engine::Tier3, 41, "exited", b"hello 42\n");
        let diff_exit = compare_elf_runs(&[r0.clone(), bad_exit], None);
        assert_eq!(diff_exit.mismatched, 1);
        assert_eq!(diff_exit.mismatches[0].kind, "exit_code");

        let bad_out = mk(Engine::Tier4, 42, "exited", b"hello 99\n");
        let diff_out = compare_elf_runs(&[r0.clone(), r1, bad_out], None);
        assert_eq!(diff_out.mismatched, 1);
        assert_eq!(diff_out.mismatches[0].kind, "stdout");
        assert!(diff_out.mismatches[0].detail.contains("byte 6"), "{}", diff_out.mismatches[0].detail);

        let nat = NativeRun { exit_code: Some(42), stdout: b"hello 43\n".to_vec(), stderr: Vec::new() };
        let diff_nat = compare_elf_runs(&[r0], Some(&nat));
        assert_eq!((diff_nat.checked, diff_nat.mismatched), (1, 1));
        assert_eq!(diff_nat.mismatches[0].reference, "host (Linux AArch64)");
    }

    #[test]
    fn a_silent_tier_fallback_is_detected() {
        assert_eq!(engine_fallback(Engine::Tier3, "x\n[CPU] Block engine: Tier-3\n"), None);
        assert!(engine_fallback(Engine::Tier4, "[CPU] Block engine: Tier-3\n").unwrap().contains("Tier-3 instead"));
        assert!(engine_fallback(Engine::Tier4, "nothing").is_some());
        assert_eq!(engine_fallback(Engine::Interpreter, "nothing"), None);
    }
}
