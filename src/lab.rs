//! Local verification workbench: `ounce-core ui`.
//!
//! A small localhost-only web server (safe Rust, `std::net`, no new dependencies). The page it serves
//! (`assets/lab/index.html`) lets a person drop a homebrew `.nro`; the server runs it on the real engines as
//! child processes of this same binary and compares what each one did:
//!
//! * **Tier-3** and **Tier-4** always (`--cpu=tier3|tier4`, headless, with an instruction budget).
//! * **Native** (the program on this computer's CPU, the hardware reference) only when the person asks for it,
//!   because it runs the guest program directly on the host. It also needs a build with `--features nce` on an
//!   AArch64 host.
//!
//! "Compared" means: how each run stopped, its exit code, and every file the program wrote under `sdmc:/`
//! (by SHA-256, with the first differing line when both are text). That is evidence for one run of one
//! program. It is not a proof of correctness, and the page says so.
//!
//! Security: bound to 127.0.0.1 only; every API call needs a per-session random token; requests whose `Host`
//! or `Origin` is not this server are refused (DNS rebinding, cross-site posts); uploads are capped; the program
//! never runs in this process.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_UPLOAD: usize = 64 * 1024 * 1024;
const MAX_HEADER: usize = 16 * 1024;
const LOG_CAP: usize = 512 * 1024;
const FILE_CAP: usize = 1024 * 1024;
const PAGE: &str = include_str!("../assets/lab/index.html");
const SCRIPT: &str = include_str!("../assets/lab/app.js");
const STYLE: &str = include_str!("../assets/lab/app.css");

/// Command-line options for `ounce-core ui`.
pub struct Options {
    pub port: u16,
    pub open_browser: bool,
    pub dir: PathBuf,
    pub budget: u64,
    pub timeout_secs: u64,
    /// Exit when stdin closes. The Mac app holds a pipe to the server so it can't outlive the app.
    pub parent_pipe: bool,
    /// Folder holding `differential.json` and `<harness>/expected-host.txt` for the committed-reference check.
    pub verify_dir: Option<PathBuf>,
}

impl Options {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let mut options = Options {
            port: 0,
            open_browser: true,
            dir: default_dir(),
            budget: 2_000_000_000,
            timeout_secs: 120,
            parent_pipe: false,
            verify_dir: default_verify_dir(),
        };
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
            match arg.as_str() {
                "--no-open" => options.open_browser = false,
                "--parent-pipe" => options.parent_pipe = true,
                "--verify-dir" => options.verify_dir = Some(PathBuf::from(value("--verify-dir")?)),
                "--port" => options.port = value("--port")?.parse().map_err(|_| "--port must be 0-65535")?,
                "--dir" => options.dir = PathBuf::from(value("--dir")?),
                "--budget" => options.budget = value("--budget")?.parse().map_err(|_| "--budget must be a number")?,
                "--timeout" => options.timeout_secs = value("--timeout")?.parse().map_err(|_| "--timeout must be seconds")?,
                other => return Err(format!("unknown option '{other}' (try --port N, --no-open, --dir DIR, --budget N, --timeout SECS, --verify-dir DIR, --parent-pipe)")),
            }
        }
        Ok(options)
    }
}

fn default_verify_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("OUNCE_VERIFY_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("docs/verify"));
    dir.join("differential.json").is_file().then_some(dir)
}

fn default_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("OUNCE_LAB_DIR") {
        return PathBuf::from(dir);
    }
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "windows") {
        // Windows has no HOME; keep results somewhere that survives a reboot.
        if let Some(base) = var("LOCALAPPDATA").or_else(|| var("USERPROFILE").map(|p| p.join("AppData/Local"))) {
            return base.join("Ounce Core").join("Checks");
        }
    } else if let Some(home) = var("HOME") {
        return if cfg!(target_os = "macos") {
            home.join("Library/Application Support/Ounce Core/Checks")
        } else {
            home.join(".ounce-core/checks")
        };
    }
    std::env::temp_dir().join("ounce-core-checks")
}

/// One engine's run of the program.
struct EngineRun {
    engine: String,
    status: &'static str, // "exited" (the process ended on its own) | "timeout" | "error"
    exit_code: Option<i32>,
    seconds: f64,
    stop_kind: &'static str,
    stop_line: String,
    log_tail: Vec<String>,
    files: BTreeMap<String, (String, u64)>,
    texts: BTreeMap<String, String>,
    note: String,
}

impl EngineRun {
    fn to_json(&self) -> Value {
        let files: BTreeMap<&String, Value> =
            self.files.iter().map(|(k, (sha, size))| (k, json!({"sha256": sha, "bytes": size}))).collect();
        json!({
            "engine": self.engine, "status": self.status, "exit_code": self.exit_code,
            "seconds": (self.seconds * 10.0).round() / 10.0, "stop": {"kind": self.stop_kind, "line": self.stop_line},
            "log_tail": self.log_tail, "files": files, "note": self.note,
        })
    }
    fn finished(&self) -> bool {
        self.stop_kind == "exited"
    }
    fn faulted(&self) -> bool {
        matches!(self.stop_kind, "fault" | "unimplemented" | "unhandled-svc" | "break" | "halt" | "loader")
    }
}

/// Reads the runtime's printed stop messages. The first failure line wins; it never invents a cause.
fn classify(log: &str) -> (&'static str, String) {
    let lines: Vec<&str> = log.lines().collect();
    let mut soft: Option<(&'static str, String)> = None;
    for (i, line) in lines.iter().enumerate() {
        let kind = if line.contains("Unimplemented instruction") {
            "unimplemented"
        } else if line.contains("[VM Error]") || line.contains("CPU fault (native signal") {
            "fault"
        } else if line.contains("unhandled SVC") {
            "unhandled-svc"
        } else if line.contains("Hit svcBreak") {
            "break"
        } else if line.contains("CPU Halted") {
            "halt"
        } else if line.contains("[Loader Error]") {
            "loader"
        } else {
            if soft.is_none() {
                if line.contains("Execution budget reached") {
                    soft = Some(("budget", line.trim().to_string()));
                } else if line.contains("Time limit reached") {
                    soft = Some(("timeout", line.trim().to_string()));
                } else if line.contains("Process exited gracefully") || line.contains("Guest exited (svcExitProcess)") {
                    soft = Some(("exited", line.trim().to_string()));
                }
            }
            continue;
        };
        let mut text = line.trim().to_string();
        for next in lines.iter().skip(i + 1).take(2) {
            let trimmed = next.trim();
            if trimmed.contains("Final PC") || trimmed.contains("Break PC:") || trimmed.contains("Break result:") {
                text = format!("{text} ({trimmed})");
            }
        }
        return (kind, text);
    }
    soft.unwrap_or(("unknown", String::new()))
}

/// What the guest itself printed: the runtime's own lines (banner, `[VM]`, `[NCE]`, summary) are removed, and a
/// guest line with no trailing newline is cut where a runtime message was glued onto it.
fn guest_console(log: &str) -> String {
    let mut out = String::new();
    for line in log.lines() {
        if line.starts_with('[') || line.starts_with("  ") || line.starts_with('=') || line.trim().is_empty() {
            continue;
        }
        let end = ["[VM]", "[NCE]", "[CPU]", "[HID]", "[Display"].iter().filter_map(|m| line.find(m)).min().unwrap_or(line.len());
        let text = line[..end].trim_end();
        if !text.is_empty() {
            out.push_str(text);
            out.push('\n');
        }
    }
    out
}

fn drain<R: Read>(reader: Option<R>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(mut reader) = reader {
        let mut chunk = [0u8; 8192];
        while let Ok(n) = reader.read(&mut chunk) {
            if n == 0 {
                break;
            }
            if out.len() < LOG_CAP {
                let take = n.min(LOG_CAP - out.len());
                out.extend_from_slice(&chunk[..take]);
            }
        }
    }
    out
}

fn collect_files(root: &Path, dir: &Path, run: &mut EngineRun) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, run);
        } else if let Ok(mut file) = std::fs::File::open(&path) {
            let mut bytes = Vec::new();
            let size = file.metadata().map(|m| m.len()).unwrap_or(0);
            if (&mut file).take(FILE_CAP as u64).read_to_end(&mut bytes).is_err() {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            run.files.insert(rel.clone(), (hex(&Sha256::digest(&bytes)), size));
            if let Ok(text) = String::from_utf8(bytes) {
                run.texts.insert(rel, text);
            }
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn run_engine(exe: &Path, nro: &Path, work: &Path, engine: &str, budget: u64, timeout: u64) -> EngineRun {
    let sdmc = work.join(engine).join("sdmc");
    let _ = std::fs::remove_dir_all(&sdmc);
    let mut run = EngineRun {
        engine: engine.to_string(), status: "error", exit_code: None, seconds: 0.0, stop_kind: "unknown",
        stop_line: String::new(), log_tail: Vec::new(), files: BTreeMap::new(), texts: BTreeMap::new(), note: String::new(),
    };
    if let Err(e) = std::fs::create_dir_all(&sdmc) {
        run.note = format!("could not create a work folder: {e}");
        return run;
    }
    let start = Instant::now();
    let mut command = Command::new(exe);
    command
        .arg("run-nro").arg(nro).arg("--present=headless")
        .arg(format!("--sdmc={}", sdmc.display())).arg(format!("--cpu={engine}"))
        .env("OUNCE_NRO_BUDGET", budget.to_string()).env("OUNCE_NRO_TIMEOUT_SECS", timeout.to_string())
        .env_remove("OUNCE_CPU")
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            run.note = format!("could not start the runtime: {e}");
            return run;
        }
    };
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let readers = (thread::spawn(move || drain(stdout)), thread::spawn(move || drain(stderr)));
    let deadline = start + Duration::from_secs(timeout + 15);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                run.status = "exited";
                run.exit_code = status.code();
                break;
            }
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                run.status = "timeout";
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(40)),
            Err(_) => break,
        }
    }
    run.seconds = start.elapsed().as_secs_f64();
    let mut log = String::from_utf8_lossy(&readers.0.join().unwrap_or_default()).into_owned();
    log.push_str(&String::from_utf8_lossy(&readers.1.join().unwrap_or_default()));
    let (kind, line) = classify(&log);
    run.stop_kind = if run.status == "timeout" { "timeout" } else { kind };
    run.stop_line = line;
    let lines: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    run.log_tail = lines[lines.len().saturating_sub(30)..].iter().map(|l| l.chars().take(240).collect()).collect();
    collect_files(&sdmc, &sdmc, &mut run);
    let console = guest_console(&log);
    if !console.is_empty() {
        run.files.insert(CONSOLE.into(), (hex(&Sha256::digest(console.as_bytes())), console.len() as u64));
        run.texts.insert(CONSOLE.into(), console);
    }
    run
}

const CONSOLE: &str = "(guest console output)";

/// Replays sampled register-only instructions on this CPU (`ounce-core diverge`). Null when it could not run.
fn instruction_check(exe: &Path, nro: &Path, work: &Path) -> Value {
    let report = work.join("diverge.json");
    let out = Command::new(exe).arg("diverge").arg(nro).arg("--json").arg(&report)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    let parsed = std::fs::read_to_string(&report).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    match (out, parsed) {
        (Ok(_), Some(r)) => json!({
            "checked": r["checked"], "mismatched": r["mismatched"], "first": r["mismatches"].get(0).cloned().unwrap_or(Value::Null),
        }),
        _ => Value::Null,
    }
}

fn label(engine: &str) -> &'static str {
    match engine {
        "tier3" => "Tier-3",
        "tier4" => "Tier-4",
        _ => "this computer (native)",
    }
}

/// The first output difference between two runs, as the page shows it.
fn first_diff(a: &EngineRun, b: &EngineRun) -> Option<Value> {
    let mut paths: Vec<&String> = a.files.keys().chain(b.files.keys()).collect();
    paths.sort();
    paths.dedup();
    for path in paths {
        match (a.files.get(path), b.files.get(path)) {
            (Some(x), Some(y)) if x.0 == y.0 => continue,
            (Some(_), Some(_)) => {
                if let (Some(ta), Some(tb)) = (a.texts.get(path), b.texts.get(path)) {
                    let (la, lb): (Vec<&str>, Vec<&str>) = (ta.lines().collect(), tb.lines().collect());
                    let n = la.len().max(lb.len());
                    for i in 0..n {
                        let (x, y) = (la.get(i).copied().unwrap_or("(no line)"), lb.get(i).copied().unwrap_or("(no line)"));
                        if x != y {
                            let cut = |s: &str| s.chars().take(200).collect::<String>();
                            return Some(json!({"path": path, "line": i + 1, "a_label": label(&a.engine), "a": cut(x), "b_label": label(&b.engine), "b": cut(y)}));
                        }
                    }
                }
                return Some(json!({"path": path, "note": "the files differ (binary or larger than the compare limit)"}));
            }
            (Some(_), None) => return Some(json!({"path": path, "note": format!("only {} wrote this file", label(&a.engine))})),
            (None, _) => return Some(json!({"path": path, "note": format!("only {} wrote this file", label(&b.engine))})),
        }
    }
    None
}

fn same_outcome(a: &EngineRun, b: &EngineRun) -> bool {
    a.stop_kind == b.stop_kind && a.exit_code == b.exit_code && first_diff(a, b).is_none()
}

fn verdict(runs: &[EngineRun]) -> Value {
    let verdict = |kind: &str, headline: String, detail: String, diff: Option<Value>| {
        json!({"kind": kind, "headline": headline, "detail": detail, "diff": diff,
               "reference": runs.iter().any(|r| r.engine == "native")})
    };
    let reference = runs.iter().find(|r| r.engine == "native");
    let tiers: Vec<&EngineRun> = runs.iter().filter(|r| r.engine != "native").collect();
    let unfinished = |r: &&EngineRun| !r.finished() && !r.faulted();
    let describe = |r: &EngineRun| format!("{}: {}", label(&r.engine), if r.stop_line.is_empty() { r.stop_kind.to_string() } else { r.stop_line.clone() });

    if let Some(refr) = reference {
        if unfinished(&refr) {
            return verdict("unfinished", "The program didn't finish on this computer".into(),
                "It was still running when the time limit passed (many programs wait for input), so there is nothing to compare yet.".into(), None);
        }
        if refr.faulted() {
            let same = tiers.iter().all(|t| t.stop_kind == refr.stop_kind);
            return if same {
                match refr.stop_kind {
                    "unhandled-svc" => verdict(
                        "fails",
                        "The program called a service Ounce-Core does not handle yet".into(),
                        format!(
                            "{} (Native runs also dispatch SVCs through Ounce-Core's Horizon kernel; report this missing service with .github/ISSUE_TEMPLATE/emulation_bug.yml.)",
                            describe(refr)
                        ),
                        None,
                    ),
                    "unimplemented" => verdict(
                        "fails",
                        "The program used an instruction Ounce-Core could not decode".into(),
                        format!(
                            "{} (Shared code/data pages use Ounce-Core's interpreter even on native runs; run `ounce-core diverge <program.nro>` or report the instruction word with .github/ISSUE_TEMPLATE/emulation_bug.yml.)",
                            describe(refr)
                        ),
                        None,
                    ),
                    _ => verdict("fails", "The program fails the same way on this computer".into(), describe(refr), None),
                }
            } else {
                verdict("diverges", "The program fails on this computer but Ounce-Core behaves differently".into(),
                    tiers.iter().map(|t| describe(t)).collect::<Vec<_>>().join(" | "), None)
            };
        }
        for t in &tiers {
            if t.faulted() {
                return verdict("diverges", format!("{} stops where this computer keeps going", label(&t.engine)),
                    format!("{} Run `ounce-core diverge <program.nro>` or report this gap with .github/ISSUE_TEMPLATE/emulation_bug.yml.", describe(t)), None);
            }
            if unfinished(t) {
                return verdict("unfinished", format!("{} hit its limit before the program finished", label(&t.engine)),
                    format!("{} Raise --budget or --timeout and check again.", describe(t)), None);
            }
            if !same_outcome(t, refr) {
                let diff = first_diff(t, refr);
                return verdict("diverges", format!("{} differs from this computer", label(&t.engine)),
                    "Both finished, but they did not produce the same output. Run `ounce-core diverge <program.nro>` to check whether a single instruction disagreed, or report this run with .github/ISSUE_TEMPLATE/emulation_bug.yml.".into(), diff);
            }
        }
        let files = refr.files.len();
        return verdict("matches", "Matches this computer on this run".into(),
            format!("Tier-3 and Tier-4 ended the same way as the program on this computer and produced identical output ({files} compared: the files it wrote and its console text). This is one run, not a proof."), None);
    }

    if let Some(t) = tiers.iter().find(|t| t.faulted()) {
        let all = tiers.iter().all(|x| x.stop_kind == t.stop_kind);
        return if all {
            match t.stop_kind {
                "unhandled-svc" => verdict(
                    "fails",
                    "The program called a service Ounce-Core does not handle yet".into(),
                    format!("{} This is a missing Horizon service in Ounce-Core; report the SVC line using .github/ISSUE_TEMPLATE/emulation_bug.yml.", describe(t)),
                    None,
                ),
                "unimplemented" => verdict(
                    "fails",
                    "The program used an instruction Ounce-Core could not decode".into(),
                    format!("{} Turn on 'Compare with this computer' or run `ounce-core diverge <program.nro>` to check it against the host CPU, and report the instruction word using .github/ISSUE_TEMPLATE/emulation_bug.yml.", describe(t)),
                    None,
                ),
                _ => verdict(
                    "fails",
                    "The program stops with an error".into(),
                    format!("{} No host reference was used, so this can't say whether the program or Ounce-Core is at fault. Turn on 'Compare with this computer' to check against the real CPU.", describe(t)),
                    None,
                ),
            }
        } else {
            verdict("diverges", "Tier-3 and Tier-4 disagree".into(),
                format!("{} | Run `ounce-core diverge <program.nro>` or report with .github/ISSUE_TEMPLATE/emulation_bug.yml.", tiers.iter().map(|x| describe(x)).collect::<Vec<_>>().join(" | ")), None)
        };
    }
    if let Some(t) = tiers.iter().find(|t| unfinished(t)) {
        return verdict("unfinished", "The program didn't finish within the limit".into(), describe(t), None);
    }
    if tiers.len() == 2 && !same_outcome(tiers[0], tiers[1]) {
        return verdict("diverges", "Tier-3 and Tier-4 disagree".into(), "An emulator bug between engines. Compare with this computer to see which is right, or run `ounce-core diverge <program.nro>` and report with .github/ISSUE_TEMPLATE/emulation_bug.yml.".into(), first_diff(tiers[0], tiers[1]));
    }
    verdict("agrees", "Tier-3 and Tier-4 agree".into(),
        "No host reference was used. Turn on 'Compare with this computer' to check against the real CPU.".into(), None)
}

/// What one check should do. Defaults reproduce the original behaviour: both emulator tiers, the server's budget.
struct CheckOptions {
    native: bool,
    engines: Vec<&'static str>,
    budget: Option<u64>,
    reference: bool,
}

/// One known harness the committed reference can be checked against.
struct Harness {
    dir: String,
    result_file: String,
}

fn norm(name: &str) -> String {
    name.to_lowercase().replace('_', "-")
}

/// Harness programs are matched by file name, as `scripts/verify_differential.py` names them.
fn find_harness(verify_dir: &Path, nro_name: &str) -> Option<Harness> {
    let doc: Value = serde_json::from_slice(&std::fs::read(verify_dir.join("differential.json")).ok()?).ok()?;
    let stem = norm(nro_name.strip_suffix(".nro").unwrap_or(nro_name));
    doc["harnesses"].as_array()?.iter().filter(|h| h["linux_elf_only"] != true).find_map(|h| {
        let (name, dir) = (norm(h["name"].as_str()?), norm(h["dir"].as_str()?));
        (stem == name || stem == dir || stem == format!("{name}-harness")).then(|| Harness {
            dir: h["dir"].as_str().unwrap_or_default().to_string(),
            result_file: h["result_file"].as_str().unwrap_or("results.txt").to_string(),
        })
    })
}

/// Byte-for-byte comparison of a harness's output with the committed host reference.
fn compare_reference(harness: &str, expected: &str, actual: &str) -> Value {
    let (want, got): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    let first = (0..want.len().max(got.len())).find(|&i| want.get(i) != got.get(i));
    let same = expected == actual;
    json!({
        "status": if same { "matches" } else { "differs" }, "harness": harness,
        "expected_lines": want.len(), "actual_lines": got.len(),
        "first_difference": first.map(|i| json!({"line": i + 1, "expected": want.get(i), "actual": got.get(i)})),
    })
}

fn host_reference(verify_dir: Option<&Path>, name: &str, runs: &[EngineRun]) -> Value {
    let Some(dir) = verify_dir else {
        return json!({"status": "unavailable", "note": "The recorded reference data is not installed with this copy."});
    };
    let Some(harness) = find_harness(dir, name) else {
        return json!({"status": "unknown-program", "note": "This program is not one of the known test programs, so there is no recorded reference to compare with."});
    };
    let Ok(expected) = std::fs::read_to_string(dir.join(&harness.dir).join("expected-host.txt")) else {
        return json!({"status": "unavailable", "harness": harness.dir, "note": "The recorded reference for this test program is missing."});
    };
    let Some(run) = runs.iter().find(|r| r.engine != "native" && r.finished()) else {
        return json!({"status": "not-run", "harness": harness.dir, "note": "The program did not finish, so there is no output to compare."});
    };
    let Some(actual) = run.texts.get(&harness.result_file) else {
        return json!({"status": "no-output", "harness": harness.dir, "note": format!("The program finished but did not write {}.", harness.result_file)});
    };
    compare_reference(&harness.dir, &expected, actual)
}

struct Lab {
    exe: PathBuf,
    dir: PathBuf,
    token: String,
    port: u16,
    budget: u64,
    timeout: u64,
    verify_dir: Option<PathBuf>,
    checks: Mutex<Vec<(String, Value)>>,
    counter: Mutex<u32>,
}

impl Lab {
    fn start_check(self: &Arc<Self>, name: &str, bytes: Vec<u8>, options: CheckOptions) -> Result<String, (u16, String)> {
        let CheckOptions { native, engines, budget, reference } = options;
        let budget = budget.unwrap_or(self.budget);
        if bytes.get(0x10..0x14) != Some(b"NRO0".as_slice()) {
            return Err((400, "That isn't a homebrew .nro file. ELF programs run with `ounce-core run-elf` for now.".into()));
        }
        let native = native && cfg!(all(feature = "nce", target_arch = "aarch64"));
        let sha = hex(&Sha256::digest(&bytes));
        let id = {
            let mut n = self.counter.lock().unwrap();
            *n += 1;
            format!("{:04}-{}", *n, &sha[..10])
        };
        let work = self.dir.join(&id);
        std::fs::create_dir_all(&work).map_err(|e| (500, format!("could not create {}: {e}", work.display())))?;
        let nro = work.join("program.nro");
        std::fs::write(&nro, &bytes).map_err(|e| (500, format!("could not save the program: {e}")))?;
        let name: String = name.chars().filter(|c| !c.is_control()).take(120).collect();
        let bytes_len = bytes.len();
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        self.checks.lock().unwrap().push((id.clone(), json!({
            "schema_version": "ounce-core/workbench-check-v1",
            "id": id, "name": name, "sha256": sha, "bytes": bytes_len, "native": native, "budget": budget,
            "status": "running", "started": started, "engines": [], "verdict": null,
        })));
        let lab = Arc::clone(self);
        let check_id = id.clone();
        let check_name = name;
        let check_sha = sha;
        thread::spawn(move || {
            let mut names = engines;
            if native {
                names.push("native");
            }
            let runs: Vec<EngineRun> = thread::scope(|scope| {
                let handles: Vec<_> = names.iter()
                    .map(|n| scope.spawn(|| run_engine(&lab.exe, &nro, &work, n, budget, lab.timeout)))
                    .collect();
                handles.into_iter().filter_map(|h| h.join().ok()).collect()
            });
            let instructions = if native { instruction_check(&lab.exe, &nro, &work) } else { Value::Null };
            let result = json!({
                "schema_version": "ounce-core/workbench-check-v1",
                "id": check_id,
                "name": check_name,
                "sha256": check_sha,
                "bytes": bytes_len,
                "native": native,
                "budget": budget,
                "started": started,
                "engines": runs.iter().map(EngineRun::to_json).collect::<Vec<_>>(),
                "verdict": verdict(&runs),
                "instructions": instructions,
                "host_reference": if reference { host_reference(lab.verify_dir.as_deref(), &check_name, &runs) } else { Value::Null },
                "status": "done"
            });
            let _ = std::fs::write(work.join("result.json"), serde_json::to_vec_pretty(&result).unwrap_or_default());
            let mut checks = lab.checks.lock().unwrap();
            if let Some((_, value)) = checks.iter_mut().find(|(i, _)| *i == check_id) {
                for key in ["engines", "verdict", "instructions", "host_reference", "status"] {
                    value[key] = result[key].clone();
                }
            }
        });
        Ok(id)
    }

    fn summary(value: &Value) -> Value {
        json!({"id": value["id"], "name": value["name"], "status": value["status"], "native": value["native"],
               "started": value["started"], "kind": value["verdict"]["kind"], "headline": value["verdict"]["headline"]})
    }
}

struct Req {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn pct_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn read_request(stream: &mut TcpStream) -> Result<Req, (u16, &'static str)> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let (mut buf, mut chunk) = (Vec::new(), [0u8; 8192]);
    let head_end = loop {
        if let Some(p) = find(&buf, b"\r\n\r\n") {
            break p;
        }
        if buf.len() > MAX_HEADER {
            return Err((431, "headers too large"));
        }
        let n = stream.read(&mut chunk).map_err(|_| (408, "request timed out"))?;
        if n == 0 {
            return Err((400, "connection closed"));
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (method, target) = (first.next().unwrap_or("").to_string(), first.next().unwrap_or("").to_string());
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())))
        .collect();
    if headers.contains_key("transfer-encoding") {
        return Err((411, "send a Content-Length"));
    }
    let len: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    if len > MAX_UPLOAD {
        return Err((413, "that file is larger than the 64 MiB limit"));
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        let n = stream.read(&mut chunk).map_err(|_| (408, "upload timed out"))?;
        if n == 0 {
            return Err((400, "upload ended early"));
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(len);
    let (path, query_text) = target.split_once('?').unwrap_or((&target, ""));
    let query = query_text
        .split('&')
        .filter_map(|p| p.split_once('=').map(|(k, v)| (pct_decode(k), pct_decode(v))))
        .collect();
    Ok(Req { method, path: path.to_string(), query, headers, body })
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK", 202 => "Accepted", 400 => "Bad Request", 403 => "Forbidden", 404 => "Not Found",
        408 => "Request Timeout", 411 => "Length Required", 413 => "Payload Too Large", 431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; img-src data:\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

/// Reads the optional settings of a new check: `native=1`, `engines=tier3,tier4`, `budget=N`, `reference=1`.
fn check_options(query: &HashMap<String, String>) -> Result<CheckOptions, String> {
    let engines = match query.get("engines").map(String::as_str) {
        None | Some("") => vec!["tier3", "tier4"],
        Some(list) => {
            let mut out: Vec<&'static str> = Vec::new();
            for name in list.split(',') {
                let name = match name {
                    "tier3" => "tier3",
                    "tier4" => "tier4",
                    _ => return Err(format!("Unknown engine '{name}'. Choose tier3, tier4 or both.")),
                };
                if !out.contains(&name) {
                    out.push(name);
                }
            }
            out
        }
    };
    let budget = match query.get("budget").map(String::as_str) {
        None | Some("") => None,
        Some(text) => match text.parse::<u64>() {
            Ok(n) if n > 0 => Some(n),
            _ => return Err("The instruction budget must be a whole number greater than zero.".into()),
        },
    };
    Ok(CheckOptions {
        native: query.get("native").is_some_and(|v| v == "1"),
        engines,
        budget,
        reference: query.get("reference").is_some_and(|v| v == "1"),
    })
}

fn route(lab: &Arc<Lab>, req: &Req) -> (u16, &'static str, Vec<u8>) {
    let json_body = |status: u16, value: Value| (status, "application/json", serde_json::to_vec(&value).unwrap_or_default());
    let error = |status: u16, message: &str| json_body(status, json!({"error": message}));
    let (own_host, own_local) = (format!("127.0.0.1:{}", lab.port), format!("localhost:{}", lab.port));
    if !req.headers.get("host").is_some_and(|h| *h == own_host || *h == own_local) {
        return error(403, "This server only answers requests addressed to it on localhost.");
    }
    if let Some(origin) = req.headers.get("origin") {
        if *origin != format!("http://{own_host}") && *origin != format!("http://{own_local}") {
            return error(403, "Cross-site requests are refused.");
        }
    }
    if req.method == "GET" && req.path == "/" {
        if req.query.get("t") != Some(&lab.token) {
            return (403, "text/plain; charset=utf-8", b"Open the address that `ounce-core ui` printed; it contains this session's key.".to_vec());
        }
        let page = PAGE.replace("__OUNCE_TOKEN__", &lab.token).replace("__OUNCE_NATIVE__", if cfg!(all(feature = "nce", target_arch = "aarch64")) { "true" } else { "false" });
        return (200, "text/html; charset=utf-8", page.into_bytes());
    }
    if req.method == "GET" && req.path == "/app.js" {
        return (200, "text/javascript; charset=utf-8", SCRIPT.as_bytes().to_vec());
    }
    if req.method == "GET" && req.path == "/app.css" {
        return (200, "text/css; charset=utf-8", STYLE.as_bytes().to_vec());
    }
    if !req.path.starts_with("/api/") {
        return error(404, "Not found.");
    }
    if req.headers.get("x-ounce-token") != Some(&lab.token) {
        return error(403, "Missing or wrong session key.");
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/checks") => {
            let checks = lab.checks.lock().unwrap();
            json_body(200, json!({"checks": checks.iter().rev().map(|(_, v)| Lab::summary(v)).collect::<Vec<_>>()}))
        }
        ("POST", "/api/checks") => {
            let name = req.query.get("name").map(String::as_str).unwrap_or("program.nro");
            let options = match check_options(&req.query) {
                Ok(options) => options,
                Err(message) => return error(400, &message),
            };
            match lab.start_check(name, req.body.clone(), options) {
                Ok(id) => json_body(202, json!({"id": id})),
                Err((status, message)) => error(status, &message),
            }
        }
        ("GET", "/api/config") => json_body(200, json!({
            "native": cfg!(all(feature = "nce", target_arch = "aarch64")),
            "default_budget": lab.budget,
            "timeout_seconds": lab.timeout,
            "engines": ["tier3", "tier4"],
            "reference_data": lab.verify_dir.is_some(),
        })),
        ("GET", path) if path.starts_with("/api/checks/") => {
            let id = &path["/api/checks/".len()..];
            match lab.checks.lock().unwrap().iter().find(|(i, _)| i == id) {
                Some((_, value)) => json_body(200, value.clone()),
                None => error(404, "No such check."),
            }
        }
        _ => error(404, "Not found."),
    }
}

fn token() -> String {
    let mut seed = Vec::new();
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let mut b = [0u8; 32];
        if f.read_exact(&mut b).is_ok() {
            seed.extend_from_slice(&b);
        }
    }
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    seed.extend_from_slice(&nanos.to_le_bytes());
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    seed.extend_from_slice(format!("{:p}", &seed).as_bytes());
    hex(&Sha256::digest(&seed))[..32].to_string()
}

pub fn run_cli(args: &[String]) {
    let options = match Options::from_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("[ui] {message}");
            std::process::exit(2);
        }
    };
    if let Err(message) = serve(options) {
        eprintln!("[ui] {message}");
        std::process::exit(1);
    }
}

fn serve(options: Options) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", options.port)).map_err(|e| format!("could not listen on 127.0.0.1:{}: {e}", options.port))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    std::fs::create_dir_all(&options.dir).map_err(|e| format!("could not create {}: {e}", options.dir.display()))?;
    let exe = std::env::current_exe().map_err(|e| format!("could not find this program: {e}"))?;
    let lab = Arc::new(Lab {
        exe, dir: options.dir.clone(), token: token(), port, budget: options.budget, timeout: options.timeout_secs, verify_dir: options.verify_dir.clone(),
        checks: Mutex::new(Vec::new()), counter: Mutex::new(0),
    });
    let url = format!("http://127.0.0.1:{port}/?t={}", lab.token);
    // A caller may read only the first line and close the pipe (the test harness and the Mac app
    // both do); `println!` would panic on the next line and kill the server before it accepts.
    let mut out = std::io::stdout();
    let _ = writeln!(out, "  Workbench: {url}");
    let _ = out.flush();
    let _ = writeln!(out, "  Checks are saved under {}", options.dir.display());
    let _ = writeln!(out, "  Only this computer can reach it. Press Ctrl-C to stop.");
    let _ = out.flush();
    if options.parent_pipe {
        thread::spawn(|| {
            let mut sink = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut sink);
            std::process::exit(0);
        });
    }
    if options.open_browser {
        // `explorer <url>` is unreliable; `start` hands the URL to the default browser (the empty
        // string is start's window title, and the URL has no characters cmd treats specially).
        let mut opener = if cfg!(target_os = "macos") {
            Command::new("open")
        } else if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", "start", ""]);
            c
        } else {
            Command::new("xdg-open")
        };
        let _ = opener.arg(&url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
    for stream in listener.incoming().flatten() {
        let lab = Arc::clone(&lab);
        thread::spawn(move || {
            let mut stream = stream;
            match read_request(&mut stream) {
                Ok(req) => {
                    let (status, content_type, body) = route(&lab, &req);
                    respond(&mut stream, status, content_type, &body);
                }
                Err((status, message)) => respond(&mut stream, status, "application/json", json!({"error": message}).to_string().as_bytes()),
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(engine: &str, kind: &'static str, files: &[(&str, &str)]) -> EngineRun {
        let mut r = EngineRun {
            engine: engine.into(), status: "exited", exit_code: Some(0), seconds: 1.0, stop_kind: kind,
            stop_line: if kind == "exited" { String::new() } else { format!("{kind} line") },
            log_tail: vec![], files: BTreeMap::new(), texts: BTreeMap::new(), note: String::new(),
        };
        for (path, text) in files {
            r.files.insert((*path).into(), (hex(&Sha256::digest(text.as_bytes())), text.len() as u64));
            r.texts.insert((*path).into(), (*text).into());
        }
        r
    }

    #[test]
    fn classify_reads_stop_messages() {
        assert_eq!(classify("a\n[VM] Process exited gracefully (svcExitProcess).").0, "exited");
        assert_eq!(classify("[VM] Execution budget reached (5 instructions).").0, "budget");
        let (kind, line) = classify("[VM Error] Unimplemented instruction 0xdeadbeef at PC: 0x80\n  Final PC: 0x80");
        assert_eq!(kind, "unimplemented");
        assert!(line.contains("Final PC"));
        let (bkind, bline) = classify(
            "[VM] Hit svcBreak: reason=0x0, info1=0x7FFFFFFED0, info2=0x4\n[VM] Break PC: 0x80000090\n[VM] Break result: 0x0000F601 (module=1, description=123)",
        );
        assert_eq!(bkind, "break");
        assert!(bline.contains("Break PC: 0x80000090"));
        assert!(bline.contains("Break result: 0x0000F601"));
        assert_eq!(classify("nothing useful").0, "unknown");
        // A failure line wins over an earlier graceful-looking one.
        assert_eq!(classify("Process exited gracefully\n[VM Error] boom").0, "fault");
    }

    #[test]
    fn native_and_emulator_exit_messages_both_count_as_finished() {
        assert_eq!(classify("[VM] Guest exited (svcExitProcess).").0, "exited");
    }

    #[test]
    fn guest_console_drops_runtime_lines_and_glued_messages() {
        let log = "====\n  OUNCE-CORE\n[Loader] x\nresult=0x1\nfixture result=0x2[VM] Guest exited (svcExitProcess).\n  -> stats\n[NCE] 3 SVCs\n";
        assert_eq!(guest_console(log), "result=0x1\nfixture result=0x2\n");
    }

    #[test]
    fn verdict_matches_when_everything_agrees() {
        let runs = [run("tier3", "exited", &[("results.txt", "a\nb\n")]), run("tier4", "exited", &[("results.txt", "a\nb\n")]), run("native", "exited", &[("results.txt", "a\nb\n")])];
        assert_eq!(verdict(&runs)["kind"], "matches");
    }

    #[test]
    fn verdict_names_the_first_differing_line_against_the_host() {
        let runs = [run("tier3", "exited", &[("results.txt", "a\nWRONG\n")]), run("tier4", "exited", &[("results.txt", "a\nb\n")]), run("native", "exited", &[("results.txt", "a\nb\n")])];
        let v = verdict(&runs);
        assert_eq!(v["kind"], "diverges");
        assert_eq!(v["diff"]["line"], 2);
        assert_eq!(v["diff"]["a"], "WRONG");
        assert_eq!(v["diff"]["b_label"], "this computer (native)");
    }

    #[test]
    fn verdict_without_reference_does_not_claim_a_match_with_the_mac() {
        let runs = [run("tier3", "exited", &[]), run("tier4", "exited", &[])];
        let v = verdict(&runs);
        assert_eq!(v["kind"], "agrees");
        assert_eq!(v["reference"], false);
    }

    #[test]
    fn verdict_flags_an_emulator_stop_where_the_mac_continues() {
        let runs = [run("tier3", "unimplemented", &[]), run("tier4", "unimplemented", &[]), run("native", "exited", &[])];
        assert_eq!(verdict(&runs)["kind"], "diverges");
    }

    #[test]
    fn verdict_identifies_missing_service_and_unimplemented_instruction_gaps() {
        let svc_runs = [run("tier3", "unhandled-svc", &[]), run("tier4", "unhandled-svc", &[])];
        let v = verdict(&svc_runs);
        assert_eq!(v["kind"], "fails");
        assert!(v["headline"].as_str().unwrap().contains("service Ounce-Core does not handle yet"));
        assert!(v["detail"].as_str().unwrap().contains("emulation_bug.yml"));

        let svc_native = [
            run("tier3", "unhandled-svc", &[]),
            run("tier4", "unhandled-svc", &[]),
            run("native", "unhandled-svc", &[]),
        ];
        let vn = verdict(&svc_native);
        assert_eq!(vn["kind"], "fails");
        assert!(vn["headline"].as_str().unwrap().contains("service Ounce-Core does not handle yet"));

        let unimpl_runs = [run("tier3", "unimplemented", &[]), run("tier4", "unimplemented", &[])];
        let vu = verdict(&unimpl_runs);
        assert_eq!(vu["kind"], "fails");
        assert!(vu["headline"].as_str().unwrap().contains("instruction Ounce-Core could not decode"));
    }

    #[test]
    fn unfinished_programs_are_not_called_divergent() {
        let runs = [run("tier3", "budget", &[]), run("tier4", "budget", &[]), run("native", "timeout", &[])];
        assert_eq!(verdict(&runs)["kind"], "unfinished");
    }

    #[test]
    fn reference_comparison_names_the_first_differing_line() {
        assert_eq!(compare_reference("lz4", "a\nb\n", "a\nb\n")["status"], "matches");
        let d = compare_reference("lz4", "a\nb\nc\n", "a\nX\nc\n");
        assert_eq!(d["status"], "differs");
        assert_eq!(d["first_difference"]["line"], 2);
        assert_eq!(compare_reference("lz4", "a\n", "a\nb\n")["first_difference"]["line"], 2);
    }

    #[test]
    fn harnesses_are_matched_by_file_name() {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/verify"));
        let h = find_harness(dir, "LZ4.nro").expect("lz4 is a known harness");
        assert!(dir.join(&h.dir).join("expected-host.txt").is_file());
        assert!(find_harness(dir, "integer_vector.nro").is_some(), "underscores and hyphens are interchangeable");
        assert!(find_harness(dir, "hello.nro").is_none());
        assert_eq!(host_reference(Some(dir), "hello.nro", &[])["status"], "unknown-program");
        assert_eq!(host_reference(None, "lz4.nro", &[])["status"], "unavailable");
    }

    #[test]
    fn check_options_default_to_both_tiers_and_reject_bad_values() {
        let q = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>();
        let o = check_options(&q(&[])).unwrap();
        assert_eq!(o.engines, vec!["tier3", "tier4"]);
        assert!(o.budget.is_none() && !o.native && !o.reference);
        assert_eq!(check_options(&q(&[("engines", "tier3"), ("budget", "5000")])).unwrap().budget, Some(5000));
        assert!(check_options(&q(&[("engines", "gpu")])).is_err());
        assert!(check_options(&q(&[("budget", "0")])).is_err());
        assert!(check_options(&q(&[("budget", "lots")])).is_err());
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(pct_decode("my%20game%2Enro+x"), "my game.nro x");
        assert_eq!(pct_decode("100%"), "100%");
        assert_eq!(pct_decode("%zz"), "%zz");
    }

    #[test]
    fn rejects_foreign_hosts_origins_and_missing_tokens() {
        let lab = Arc::new(Lab {
            exe: PathBuf::from("x"), dir: std::env::temp_dir(), token: "tok".into(), port: 4000, budget: 1, timeout: 1, verify_dir: None,
            checks: Mutex::new(vec![]), counter: Mutex::new(0),
        });
        let req = |method: &str, path: &str, headers: &[(&str, &str)]| Req {
            method: method.into(), path: path.into(), query: HashMap::new(),
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), body: vec![],
        };
        assert_eq!(route(&lab, &req("GET", "/api/checks", &[("host", "evil.example"), ("x-ounce-token", "tok")])).0, 403);
        assert_eq!(route(&lab, &req("GET", "/api/checks", &[("host", "127.0.0.1:4000"), ("origin", "http://evil.example")])).0, 403);
        assert_eq!(route(&lab, &req("GET", "/api/checks", &[("host", "127.0.0.1:4000")])).0, 403);
        assert_eq!(route(&lab, &req("GET", "/api/checks", &[("host", "127.0.0.1:4000"), ("x-ounce-token", "tok")])).0, 200);
        assert_eq!(route(&lab, &req("GET", "/", &[("host", "127.0.0.1:4000")])).0, 403);
    }

    #[test]
    fn uploads_must_be_nro_files() {
        let lab = Arc::new(Lab {
            exe: PathBuf::from("x"), dir: std::env::temp_dir(), token: "t".into(), port: 1, budget: 1, timeout: 1, verify_dir: None,
            checks: Mutex::new(vec![]), counter: Mutex::new(0),
        });
        assert_eq!(lab.start_check("a.elf", b"\x7fELF".to_vec(), check_options(&HashMap::new()).unwrap()).unwrap_err().0, 400);
    }
}
