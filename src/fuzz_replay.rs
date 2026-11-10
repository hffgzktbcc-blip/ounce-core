//! `ounce-core fuzz-replay <trace|->`: replays a single-instruction trace made by
//! `docs/verify/insn-fuzzer/fuzz_host.c` (real Apple Silicon results) in the Tier-3 interpreter
//! (`Cpu::step`) and compares every X register, V register, NZCV and FPSR.
//!
//! Trace lines (hex): `U <word>` (host raised SIGILL) or
//! `T <word> <nzcv_in> <31 x> <32 v> => <nzcv_out> <fpsr_out> <31 x> <32 v>`.
//! `TF <fpcr> <word> ...` is the same case run under that FPCR value (`T` means FPCR 0).
//!
//! `--engine interp|tier3|tier4` picks the engine that runs the word. Every engine runs the same
//! two-word block `[word, svc #0]` (see [`exec`]); `tier3` is the Tier-3 super-block engine and
//! `tier4` the native recompiler (`--features native-jit`). `--engine-vs-engine` needs no host
//! trace: it generates random words and inputs and compares interpreter, Tier-3 and Tier-4
//! against each other (see [`run_engines`]).

use crate::cpu::decoder::decode;
use crate::cpu::tier3_jit::Tier3JitEngine;
use crate::cpu::{BlockExit, Cpu, Instruction, StepResult};
use crate::mmu::{Memory, Permission};
use std::collections::BTreeMap;
use std::io::BufRead;

const CODE: u64 = 0x1000;
/// FPSR.QC (bit 27): saturation, modelled.
const QC: u32 = 1 << 27;
/// FPSR exception flags (IOC, DZC, OFC, UFC, IXC, IDC): the interpreter does not model them.
const EXC: u32 = 0x9F;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub fpcr: u32,
    pub word: u32,
    pub nzcv_in: u32,
    pub x_in: [u64; 31],
    pub v_in: [u128; 32],
    pub nzcv_out: u32,
    pub fpsr_out: u32,
    pub x_out: [u64; 31],
    pub v_out: [u128; 32],
}

#[derive(Debug, PartialEq, Eq)]
pub enum Line {
    Undefined(u32),
    Test(Box<Case>),
}

fn hex(t: &mut std::str::SplitWhitespace) -> Result<u128, String> {
    u128::from_str_radix(t.next().ok_or("short line")?, 16).map_err(|e| e.to_string())
}

fn bounded_hex(t: &mut std::str::SplitWhitespace, bits: u32) -> Result<u128, String> {
    let value = hex(t)?;
    if bits < 128 && value >> bits != 0 { return Err(format!("value exceeds {bits}-bit field")); }
    Ok(value)
}

pub fn parse_line(line: &str) -> Result<Line, String> {
    let mut t = line.split_whitespace();
    match t.next() {
        Some("U") => {
            let word = bounded_hex(&mut t, 32)? as u32;
            if t.next().is_some() { return Err("extra fields after U case".into()); }
            Ok(Line::Undefined(word))
        },
        Some(tag @ ("T" | "TF")) => {
            let fpcr = if tag == "TF" { bounded_hex(&mut t, 32)? as u32 } else { 0 };
            let word = bounded_hex(&mut t, 32)? as u32;
            let nzcv_in = bounded_hex(&mut t, 4)? as u32;
            let mut x_in = [0u64; 31];
            let mut v_in = [0u128; 32];
            let mut x_out = [0u64; 31];
            let mut v_out = [0u128; 32];
            for x in x_in.iter_mut() {
                *x = bounded_hex(&mut t, 64)? as u64;
            }
            for v in v_in.iter_mut() {
                *v = hex(&mut t)?;
            }
            if t.next() != Some("=>") {
                return Err("missing =>".into());
            }
            let nzcv_out = bounded_hex(&mut t, 4)? as u32;
            let fpsr_out = bounded_hex(&mut t, 32)? as u32;
            for x in x_out.iter_mut() {
                *x = bounded_hex(&mut t, 64)? as u64;
            }
            for v in v_out.iter_mut() {
                *v = hex(&mut t)?;
            }
            if t.next().is_some() { return Err("extra fields after T case".into()); }
            Ok(Line::Test(Box::new(Case { fpcr, word, nzcv_in, x_in, v_in, nzcv_out, fpsr_out, x_out, v_out })))
        }
        _ => Err(format!("bad line: {line:.40}")),
    }
}

/// What the interpreter did with one word.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Executed; first differences against the host (empty = match).
    Ran(Vec<String>, u32),
    Unimplemented,
    OtherError(String),
}

/// Which engine runs the word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Interp,
    Tier3,
    Tier4,
}

impl Engine {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "interp" => Some(Self::Interp),
            "tier3" => Some(Self::Tier3),
            "tier4" => Some(Self::Tier4),
            _ => None,
        }
    }
}

/// Where the word sits. The rest of the page is `svc #0` so a stray branch lands on a terminator.
const WORD_PC: u64 = CODE + 0x800;
const SVC0: u32 = 0xD400_0001;
/// A data page for loads/stores in engine-vs-engine mode (host traces contain no memory ops).
const DATA: u64 = 0x2_0000;
const DATA_LEN: usize = 0x2000;
/// Instruction budget for one case (the wrapped block is 2 instructions; the rest covers branches
/// that land on the `svc` filler).
const LIMIT: usize = 16;

/// Everything the engines are compared on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    x: [u64; 31],
    sp: u64,
    pc: u64,
    nzcv: u32,
    fpsr: u32,
    fpcr: u32,
    v: [u128; 32],
    /// `svc <imm>`, `fault <pc>` or `budget`.
    exit: String,
    data: Vec<u8>,
}

/// Input registers of one case.
#[derive(Clone)]
struct Init {
    nzcv: u32,
    x: [u64; 31],
    sp: u64,
    v: [u128; 32],
    fpcr: u32,
    /// Initial contents of the data page (empty = no data page).
    data: Vec<u8>,
}

fn setup(word: u32, i: &Init) -> (Cpu, Memory) {
    let mut mmu = Memory::new();
    mmu.map_range(CODE, 0x1000, Permission::READ_WRITE_EXECUTE);
    for k in 0..0x1000 / 4 {
        mmu.write_u32(CODE + 4 * k, SVC0).expect("code page mapped");
    }
    mmu.write_u32(WORD_PC, word).expect("code page mapped");
    if !i.data.is_empty() {
        mmu.map_range(DATA, DATA_LEN, Permission::READ_WRITE);
        mmu.write_bytes(DATA, &i.data).expect("data page mapped");
    }
    let mut cpu = Cpu::new();
    cpu.regs.pc = WORD_PC;
    cpu.regs.x = i.x;
    cpu.regs.sp = i.sp;
    cpu.regs.v = i.v;
    cpu.regs.fpcr = i.fpcr;
    cpu.regs.pstate.from_bits(i.nzcv << 28);
    (cpu, mmu)
}

fn capture(cpu: &Cpu, mmu: &Memory, exit: String, has_data: bool) -> State {
    let mut data = Vec::new();
    if has_data {
        data = vec![0u8; DATA_LEN];
        mmu.read_bytes(DATA, &mut data).expect("data page mapped");
    }
    State {
        x: cpu.regs.x,
        sp: cpu.regs.sp,
        pc: cpu.regs.pc,
        nzcv: cpu.regs.pstate.to_bits() >> 28,
        fpsr: cpu.regs.fpsr,
        fpcr: cpu.regs.fpcr,
        v: cpu.regs.v,
        exit,
        data,
    }
}

fn exit_name(e: &BlockExit) -> String {
    match e {
        BlockExit::Svc { svc_id } => format!("svc {svc_id}"),
        BlockExit::Fault { pc } => format!("fault {pc:#x}"),
        _ => "budget".to_string(),
    }
}

/// Result of running one word on one engine.
#[derive(Debug)]
enum Exec {
    Done(Box<State>),
    /// The interpreter reports the word as `UnimplementedInstruction`.
    Unimplemented,
    /// Tier-4 is not available on this host / build.
    Unavailable,
    /// Tier-4 ran nothing natively (extra info only).
    Other(String),
}

/// Runs `[word, svc #0]` from fresh state on `engine`. For Tier-3 the block is made hot first
/// (counter at threshold - 1) so it runs as a super-block, not as interpreted steps; Tier-4 uses
/// threshold 1 so the block is translated on first entry if the lowerer supports it. Returns the
/// state and, for Tier-4, the number of guest instructions that ran natively.
fn exec(engine: Engine, word: u32, init: &Init) -> (Exec, u64) {
    let (mut cpu, mut mmu) = setup(word, init);
    let has_data = !init.data.is_empty();
    #[cfg_attr(not(feature = "native-jit"), allow(unused_mut))]
    let mut native = 0u64;
    let exit = match engine {
        Engine::Interp => {
            let mut exit = "budget".to_string();
            for _ in 0..LIMIT {
                let pc = cpu.regs.pc;
                match cpu.step(&mut mmu) {
                    Ok(StepResult::Continue) => {}
                    Ok(StepResult::Svc(imm)) => {
                        exit = format!("svc {imm}");
                        break;
                    }
                    Err(crate::cpu::CpuError::UnimplementedInstruction { .. }) if pc == WORD_PC => {
                        return (Exec::Unimplemented, 0);
                    }
                    Ok(other) if pc == WORD_PC => return (Exec::Other(format!("{other:?}")), 0),
                    Ok(_) | Err(_) => {
                        exit = format!("fault {pc:#x}");
                        break;
                    }
                }
            }
            exit
        }
        Engine::Tier3 => {
            let mut e = Tier3JitEngine::new();
            e.hot_pc_counters.insert(WORD_PC, Tier3JitEngine::HOT_THRESHOLD - 1);
            drive(|c, m, n| e.run_slice(c, m, n), &mut cpu, &mut mmu)
        }
        #[cfg(feature = "native-jit")]
        Engine::Tier4 => {
            let mut e = crate::cpu::tier4::Tier4JitEngine::with_threshold(1);
            if e.unavailable.is_some() {
                return (Exec::Unavailable, 0);
            }
            e.tier3.hot_pc_counters.insert(WORD_PC, Tier3JitEngine::HOT_THRESHOLD - 1);
            let exit = drive(|c, m, n| e.run_slice(c, m, n), &mut cpu, &mut mmu);
            native = e.native_insns;
            exit
        }
        #[cfg(not(feature = "native-jit"))]
        Engine::Tier4 => return (Exec::Unavailable, 0),
    };
    (Exec::Done(Box::new(capture(&cpu, &mmu, exit, has_data))), native)
}

/// Runs engine slices until `svc`, a fault or [`LIMIT`] instructions; `ret` exits are stepped over.
fn drive(mut slice: impl FnMut(&mut Cpu, &mut Memory, usize) -> (usize, BlockExit), cpu: &mut Cpu, mmu: &mut Memory) -> String {
    let mut total = 0;
    while total < LIMIT {
        let (n, exit) = slice(cpu, mmu, LIMIT - total);
        total += n;
        match exit {
            BlockExit::Svc { .. } | BlockExit::Fault { .. } => return exit_name(&exit),
            _ if n == 0 => return "stuck".to_string(),
            _ => {}
        }
    }
    "budget".to_string()
}

fn diff_states(a: &State, b: &State, an: &str, bn: &str) -> Vec<String> {
    let mut d = Vec::new();
    if a.exit != b.exit {
        d.push(format!("exit: {an} {} {bn} {}", a.exit, b.exit));
    }
    if a.pc != b.pc {
        d.push(format!("pc: {an} {:#x} {bn} {:#x}", a.pc, b.pc));
    }
    for i in 0..31 {
        if a.x[i] != b.x[i] {
            d.push(format!("x{i}: {an} {:#018x} {bn} {:#018x}", a.x[i], b.x[i]));
        }
    }
    if a.sp != b.sp {
        d.push(format!("sp: {an} {:#x} {bn} {:#x}", a.sp, b.sp));
    }
    for i in 0..32 {
        if a.v[i] != b.v[i] {
            d.push(format!("v{i}: {an} {:#034x} {bn} {:#034x}", a.v[i], b.v[i]));
        }
    }
    if a.nzcv != b.nzcv {
        d.push(format!("nzcv: {an} {:#x} {bn} {:#x}", a.nzcv, b.nzcv));
    }
    if a.fpsr != b.fpsr {
        d.push(format!("fpsr: {an} {:#x} {bn} {:#x}", a.fpsr, b.fpsr));
    }
    if a.fpcr != b.fpcr {
        d.push(format!("fpcr: {an} {:#x} {bn} {:#x}", a.fpcr, b.fpcr));
    }
    if a.data != b.data {
        let at = a.data.iter().zip(&b.data).position(|(p, q)| p != q).unwrap_or(0);
        d.push(format!("data page differs, first at +{at:#x}"));
    }
    d
}

/// Replays one case in the interpreter. The second `Ran` field is the FPSR the interpreter produced.
#[cfg(test)]
pub fn replay(c: &Case) -> Outcome {
    replay_on(c, Engine::Interp).0
}

/// Replays one case on `engine`; the second value is the number of instructions Tier-4 ran natively.
/// Words the interpreter does not implement are classified from an interpreter run first so the
/// counts are comparable across engines.
#[cfg(test)]
fn replay_on(c: &Case, engine: Engine) -> (Outcome, u64) {
    let (outcome, native, _) = replay_details(c, engine);
    (outcome, native)
}

fn replay_details(c: &Case, engine: Engine) -> (Outcome, u64, Option<Box<State>>) {
    let init = Init { nzcv: c.nzcv_in, x: c.x_in, sp: DATA, v: c.v_in, fpcr: c.fpcr, data: Vec::new() };
    if engine != Engine::Interp {
        match exec(Engine::Interp, c.word, &init).0 {
            Exec::Unimplemented => return (Outcome::Unimplemented, 0, None),
            Exec::Other(e) => return (Outcome::OtherError(e), 0, None),
            _ => {}
        }
    }
    let (r, native) = exec(engine, c.word, &init);
    let st = match r {
        Exec::Done(st) => st,
        Exec::Unimplemented => return (Outcome::Unimplemented, 0, None),
        Exec::Unavailable => return (Outcome::OtherError("engine unavailable".into()), 0, None),
        Exec::Other(e) => return (Outcome::OtherError(e), 0, None),
    };
    if st.exit != "svc 0" {
        return (Outcome::OtherError(format!("exit {} pc {:#x}", st.exit, st.pc)), native, Some(st));
    }
    let mut diffs = Vec::new();
    for i in 0..31 {
        // x18 is reserved by macOS and cleared by the kernel at will, so the host value is meaningless
        if i != 18 && st.x[i] != c.x_out[i] {
            diffs.push(format!("x{i}: host {:#018x} emu {:#018x}", c.x_out[i], st.x[i]));
        }
    }
    for i in 0..32 {
        if st.v[i] != c.v_out[i] {
            diffs.push(format!("v{i}: host {:#034x} emu {:#034x}", c.v_out[i], st.v[i]));
        }
    }
    if st.nzcv != c.nzcv_out {
        diffs.push(format!("nzcv: host {:#x} emu {:#x}", c.nzcv_out, st.nzcv));
    }
    if st.fpsr & QC != c.fpsr_out & QC {
        diffs.push(format!("fpsr.QC: host {} emu {}", (c.fpsr_out & QC) >> 27, (st.fpsr & QC) >> 27));
    }
    (Outcome::Ran(diffs, st.fpsr), native, Some(st))
}

fn register_json(x: &[u64], v: &[u128], nzcv: u32, fpsr: u32) -> serde_json::Value {
    serde_json::json!({
        "x": x.iter().map(|v| format!("{v:#018x}")).collect::<Vec<_>>(),
        "v": v.iter().map(|v| format!("{v:#034x}")).collect::<Vec<_>>(),
        "nzcv": nzcv, "fpsr": fpsr
    })
}

/// Retain a single compared instruction, not a trace of an arbitrary program.
fn save_difference(dir: &std::path::Path, line: &str, line_number: usize, c: &Case, st: &State,
                   engine: Engine, native: u64, differences: &[String]) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let mut input = register_json(&c.x_in, &c.v_in, c.nzcv_in, 0);
    input["fpcr"] = c.fpcr.into();
    let trace = format!("{line}\n");
    let engine_name = match engine { Engine::Interp => "interp", Engine::Tier3 => "tier3", Engine::Tier4 => "tier4" };
    let runtime = std::env::current_exe().and_then(std::fs::read).map_err(|e| e.to_string())?;
    let doc = serde_json::json!({
        "schema": "ounce-core/instruction-difference-v1",
        "scope": "first differing compared case in an independent-instruction trace; not an arbitrary program's first divergence",
        "reference_provenance": "supplied trace; hardware origin must be established separately",
        "line_number": line_number,
        "word": format!("{:#010x}", c.word),
        "harness_pc": format!("{WORD_PC:#x}"),
        "engine": engine_name,
        "tier4_native_instructions_including_wrapper": native,
        "host": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "runtime_sha256": format!("{:x}", Sha256::digest(&runtime)),
        "case_sha256": format!("{:x}", Sha256::digest(trace.as_bytes())),
        "input": input,
        "reference": register_json(&c.x_out, &c.v_out, c.nzcv_out, c.fpsr_out),
        "actual": register_json(&st.x, &st.v, st.nzcv, st.fpsr),
        "comparison": {"excluded_x": [18], "fpsr_mask": format!("{QC:#x}"), "fpcr": c.fpcr,
                       "wrapper": "test word followed by SVC #0; PC/SP are not hardware-reference comparisons"},
        "differences": differences,
        "replay_argv": ["ounce-core", "fuzz-replay", "--engine", engine_name, "case.trace"],
        "replay_cwd": "this bundle directory"
    });
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::create_dir(dir).map_err(|e| format!("cannot create new dossier {}: {e}", dir.display()))?;
    std::fs::write(dir.join("case.trace"), trace).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("diagnosis.json"), bytes).map_err(|e| e.to_string())?;
    Ok(())
}

/// Re-execute retained instruction bytes. Success means reproduction, not correctness.
pub fn replay_bundle(args: &[String]) -> i32 {
    match replay_bundle_inner(args) {
        Ok((report, reproduced)) => { println!("{report}"); if reproduced { 0 } else { 1 } }
        Err(e) => { eprintln!("replay-instruction: {e}"); 2 }
    }
}

fn replay_bundle_inner(args: &[String]) -> Result<(serde_json::Value, bool), String> {
    use sha2::{Digest, Sha256};
    if args.len() != 1 && !(args.len() == 3 && args[1] == "--engine") {
        return Err("usage: replay-instruction BUNDLE [--engine interp|tier3|tier4]".into());
    }
    let dir = std::path::Path::new(&args[0]);
    let read = |name: &str| -> Result<Vec<u8>, String> {
        let path = dir.join(name);
        if std::fs::metadata(&path).map_err(|e| format!("{name}: {e}"))?.len() > 65536 {
            return Err(format!("{name} exceeds the 64 KiB dossier limit"));
        }
        std::fs::read(path).map_err(|e| format!("{name}: {e}"))
    };
    let manifest = read("diagnosis.json")?;
    let doc: serde_json::Value = serde_json::from_slice(&manifest).map_err(|e| format!("invalid diagnosis.json: {e}"))?;
    if doc["schema"] != "ounce-core/instruction-difference-v1" {
        return Err("unsupported dossier schema".into());
    }
    let trace = read("case.trace")?;
    let trace_hash = format!("{:x}", Sha256::digest(&trace));
    if doc["case_sha256"].as_str() != Some(trace_hash.as_str()) {
        return Err("case.trace hash mismatch; no execution performed".into());
    }
    let text = std::str::from_utf8(&trace).map_err(|e| e.to_string())?;
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() != 1 { return Err("dossier must contain exactly one case".into()); }
    let Line::Test(c) = parse_line(lines[0])? else { return Err("dossier must contain a compared T case".into()); };
    let mut input = register_json(&c.x_in, &c.v_in, c.nzcv_in, 0);
    input["fpcr"] = c.fpcr.into();
    if doc["word"] != format!("{:#010x}", c.word) || doc["input"] != input
        || doc["reference"] != register_json(&c.x_out, &c.v_out, c.nzcv_out, c.fpsr_out) {
        return Err("diagnosis state disagrees with retained case.trace".into());
    }
    if doc["comparison"]["excluded_x"] != serde_json::json!([18])
        || doc["comparison"]["fpsr_mask"] != format!("{QC:#x}") || doc["comparison"]["fpcr"] != c.fpcr {
        return Err("unsupported comparison scope".into());
    }
    let actual = &doc["actual"];
    let valid_regs = |name: &str, count: usize, width: usize| {
        actual[name].as_array().is_some_and(|values| values.len() == count && values.iter().all(|v| {
            v.as_str().is_some_and(|s| s.len() == width && s.strip_prefix("0x")
                .is_some_and(|n| u128::from_str_radix(n, 16).is_ok()))
        }))
    };
    if !actual.as_object().is_some_and(|o| o.len() == 4) || !valid_regs("x", 31, 18) || !valid_regs("v", 32, 34)
        || !actual["nzcv"].as_u64().is_some_and(|n| n <= 15)
        || !actual["fpsr"].as_u64().is_some_and(|n| n <= u32::MAX as u64) {
        return Err("malformed recorded actual state".into());
    }
    let recorded_engine = doc["engine"].as_str().and_then(Engine::parse).ok_or("invalid recorded engine")?;
    let engine = if args.len() == 3 { Engine::parse(&args[2]).ok_or("unknown replay engine")? } else { recorded_engine };
    if engine == Engine::Tier4 && !tier4_available() { return Err("Tier-4 unavailable; native-jit and a supported host are required".into()); }
    let (outcome, native, state) = replay_details(&c, engine);
    let actual = state.as_deref().map(|st| register_json(&st.x, &st.v, st.nzcv, st.fpsr));
    let (differences, reference_verdict, execution_outcome) = match outcome {
        Outcome::Ran(differences, _) => {
            if actual.is_none() { return Err("missing replay execution state".into()); }
            let verdict = if differences.is_empty() { "match" } else { "mismatch" };
            (differences, verdict, "completed".to_string())
        }
        Outcome::Unimplemented => (Vec::new(), "not-compared", "unimplemented".to_string()),
        Outcome::OtherError(error) => (Vec::new(), "not-compared", format!("execution-error: {error}")),
    };
    let reproduced = execution_outcome == "completed" && actual.as_ref() == Some(&doc["actual"]);
    let runtime = std::env::current_exe().and_then(std::fs::read).map_err(|e| e.to_string())?;
    Ok((serde_json::json!({
        "schema": "ounce-core/instruction-replay-v1",
        "replay_verdict": if reproduced { "reproduced" } else { "changed" },
        "reference_verdict": reference_verdict,
        "execution_outcome": execution_outcome,
        "scope": "retained single-instruction state only; hash checks do not establish reference provenance",
        "case_sha256": trace_hash,
        "diagnosis_sha256": format!("{:x}", Sha256::digest(&manifest)),
        "runtime_sha256": format!("{:x}", Sha256::digest(&runtime)),
        "recorded_runtime_sha256": doc["runtime_sha256"],
        "engine": format!("{engine:?}"),
        "tier4_native_instructions_including_wrapper": native,
        "host": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "actual": actual, "differences": differences
    }), reproduced))
}

/// Does the interpreter run a word the host calls UNDEFINED?
fn runs_undefined(word: u32) -> bool {
    let init = Init { nzcv: 0, x: [0; 31], sp: DATA, v: [0; 32], fpcr: 0, data: Vec::new() };
    matches!(exec(Engine::Interp, word, &init).0, Exec::Done(st) if st.exit == "svc 0")
}

#[derive(Default)]
struct Tally {
    host_undefined: u64,
    emu_runs_undefined: u64,
    executed: u64,
    matched: u64,
    unimplemented: u64,
    other_errors: u64,
    value_mismatch: u64,
    fpsr_exc_only: u64,
    qc_mismatch: u64,
}

fn class(word: u32) -> u32 {
    word & 0xFFE0_FC00
}

pub fn run(args: &[String]) -> i32 {
    let mut show = 20usize;
    let mut dossier: Option<std::path::PathBuf> = None;
    let mut dossier_saved = false;
    let mut top = 60usize;
    let mut list_unimpl = false;
    // `--filter MASK:VALUE` (hex): also count executed/matching words with `word & MASK == VALUE`, for per-class evidence
    let mut filter: Vec<(u32, u32)> = Vec::new();
    let (mut f_exec, mut f_match) = (0u64, 0u64);
    let mut input = None;
    let mut results_path: Option<std::path::PathBuf> = None;
    let mut engine = Engine::Interp;
    let mut ee = false;
    let mut seed = 1u64;
    let mut count = 20_000u64;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--show" => show = it.next().and_then(|v| v.parse().ok()).unwrap_or(20),
            "--filter" => {
                let spec = it.next().cloned().unwrap_or_default();
                let p = |x: Option<&str>| x.and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok());
                let mut parts = spec.split(':');
                match (p(parts.next()), p(parts.next())) {
                    (Some(m), Some(v)) => filter.push((m, v)),
                    _ => {
                        eprintln!("--filter MASK:VALUE (hex)");
                        return 2;
                    }
                }
            }
            "--first-difference" => match it.next() {
                Some(path) => dossier = Some(path.into()),
                None => { eprintln!("--first-difference requires a new directory"); return 2; }
            },
            "--results" => match it.next() {
                Some(path) => results_path = Some(path.into()),
                None => { eprintln!("--results requires a new file name"); return 2; }
            },
            "--list-unimplemented" => list_unimpl = true,
            "--top" => top = it.next().and_then(|v| v.parse().ok()).unwrap_or(60),
            "--engine" => match it.next().and_then(|v| Engine::parse(v)) {
                Some(e) => engine = e,
                None => {
                    eprintln!("--engine takes interp, tier3 or tier4");
                    return 2;
                }
            },
            "--engine-vs-engine" => ee = true,
            "--seed" => seed = it.next().and_then(|v| v.parse().ok()).unwrap_or(1),
            "--count" => count = it.next().and_then(|v| v.parse().ok()).unwrap_or(20_000),
            _ => input = Some(a.clone()),
        }
    }
    if dossier.as_ref().is_some_and(|p| p.exists()) {
        eprintln!("--first-difference requires a new directory; existing evidence is never overwritten");
        return 2;
    }
    if results_path.as_ref().is_some_and(|p| p.exists()) {
        eprintln!("--results requires a new file; existing evidence is never overwritten");
        return 2;
    }
    if results_path.is_some() && ee {
        eprintln!("--results needs a reference trace, not --engine-vs-engine");
        return 2;
    }
    if ee {
        if dossier.is_some() {
            eprintln!("--first-difference currently requires a reference trace, not --engine-vs-engine");
            return 2;
        }
        if count == 0 {
            eprintln!("--count must be positive; no cases would be compared");
            return 2;
        }
        return run_engines(seed, count, show);
    }
    let Some(input) = input else {
        eprintln!("usage: fuzz-replay [--show N] [--top N] [--filter MASK:VALUE]... [--list-unimplemented] [--engine interp|tier3|tier4] [--first-difference NEW-DIR] [--results NEW-FILE] <trace-file|->");
        eprintln!("       fuzz-replay --engine-vs-engine [--seed N] [--count N] [--show N]");
        return 2;
    };
    if engine == Engine::Tier4 && !tier4_available() {
        eprintln!("tier4 is not available (build with --features native-jit on a supported host)");
        return 2;
    }
    let reader: Box<dyn BufRead> = if input == "-" {
        Box::new(std::io::BufReader::new(std::io::stdin()))
    } else {
        match std::fs::File::open(&input) {
            Ok(f) => Box::new(std::io::BufReader::new(f)),
            Err(e) => {
                eprintln!("{input}: {e}");
                return 2;
            }
        }
    };
    // One TSV row per reference case: line, fpcr, word, status, fpsr-exception-flags-differ, detail.
    let mut results = match results_path.as_ref().map(std::fs::File::create_new) {
        Some(Ok(f)) => Some(std::io::BufWriter::new(f)),
        Some(Err(e)) => { eprintln!("--results: {e}"); return 2; }
        None => None,
    };
    let mut record = |n: usize, fpcr: u32, word: u32, status: &str, exc: bool, detail: &str| {
        if let Some(w) = results.as_mut() {
            use std::io::Write;
            let _ = writeln!(w, "{n}\t{fpcr:#x}\t{word:08x}\t{status}\t{}\t{detail}", u8::from(exc));
        }
    };
    let mut t = Tally::default();
    let mut native_cases = 0u64;
    let mut mism_classes: BTreeMap<u32, (u64, u32)> = BTreeMap::new();
    let mut unimpl_classes: BTreeMap<u32, (u64, u32)> = BTreeMap::new();
    let mut accepts: BTreeMap<u32, (u64, u32)> = BTreeMap::new();
    let mut shown = 0;
    let mut ran_classes = std::collections::BTreeSet::new();
    for (n, line) in reader.lines().enumerate() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                eprintln!("line {}: trace read error: {e}", n + 1);
                return 2;
            }
        };
        let parsed = match parse_line(&line) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("line {}: {e}", n + 1);
                return 2;
            }
        };
        match parsed {
            Line::Undefined(w) => {
                record(n + 1, 0, w, "host-undefined", false, "");
                t.host_undefined += 1;
                if runs_undefined(w) {
                    t.emu_runs_undefined += 1;
                    accepts.entry(class(w)).or_insert((0, w)).0 += 1;
                }
            }
            Line::Test(c) => {
                let (outcome, native, state) = replay_details(&c, engine);
                native_cases += u64::from(native > 0);
                match outcome {
                Outcome::Unimplemented => {
                    record(n + 1, c.fpcr, c.word, "unsupported", false, "");
                    t.unimplemented += 1;
                    if list_unimpl {
                        println!("UNIMPL {:08x}", c.word);
                    }
                    unimpl_classes.entry(class(c.word)).or_insert((0, c.word)).0 += 1;
                }
                Outcome::OtherError(e) => {
                    record(n + 1, c.fpcr, c.word, "error", false, &e);
                    t.other_errors += 1;
                    if shown < show {
                        shown += 1;
                        println!("ERROR {:08x}: {e}", c.word);
                    }
                }
                Outcome::Ran(diffs, fpsr) => {
                    t.executed += 1;
                    if filter.iter().any(|&(m, v)| c.word & m == v) {
                        f_exec += 1;
                        f_match += diffs.is_empty() as u64;
                    }
                    ran_classes.insert(class(c.word));
                    let exc_differs = fpsr & EXC != c.fpsr_out & EXC;
                    record(n + 1, c.fpcr, c.word, if diffs.is_empty() { "match" } else { "mismatch" }, exc_differs,
                           &diffs.iter().take(4).cloned().collect::<Vec<_>>().join(" | "));
                    if diffs.is_empty() {
                        t.matched += 1;
                        if exc_differs {
                            t.fpsr_exc_only += 1;
                        }
                    } else {
                        t.value_mismatch += 1;
                        if let Some(dir) = dossier.as_ref().filter(|_| !dossier_saved) {
                            let Some(st) = state.as_deref() else {
                                eprintln!("dossier: missing execution state"); return 2;
                            };
                            if let Err(e) = save_difference(dir, &line, n + 1, &c, st, engine, native, &diffs) {
                                eprintln!("dossier: {e}"); return 2;
                            }
                            dossier_saved = true;
                        }
                        if diffs.iter().any(|d| d.starts_with("fpsr.QC")) {
                            t.qc_mismatch += 1;
                        }
                        mism_classes.entry(class(c.word)).or_insert((0, c.word)).0 += 1;
                        if shown < show {
                            shown += 1;
                            println!("MISMATCH {:08x} nzcv_in={:x} fpcr={:#x}", c.word, c.nzcv_in, c.fpcr);
                            for d in diffs.iter().take(6) {
                                println!("    {d}");
                            }
                        }
                    }
                }
                }
            },
        }
    }
    if t.host_undefined + t.executed + t.unimplemented + t.other_errors == 0 {
        eprintln!("trace contains no cases; nothing was compared");
        return 2;
    }
    println!("--- fuzz-replay summary ---");
    println!("engine:                         {engine:?}");
    if engine == Engine::Tier4 {
        println!("words with native Tier-4 insns: {native_cases}");
    }
    println!("host executed (not SIGILL):     {}", t.executed + t.unimplemented + t.other_errors);
    println!("host UNDEFINED (SIGILL):        {}", t.host_undefined);
    println!("emulator unimplemented:         {} ({} classes)", t.unimplemented, unimpl_classes.len());
    println!("emulator other errors:          {}", t.other_errors);
    println!("emulator executed:              {}", t.executed);
    println!("  distinct classes executed:    {}", ran_classes.len());
    println!("  exact match (X,V,NZCV,QC):    {}", t.matched);
    println!("  MISMATCH:                     {} ({} with QC wrong)", t.value_mismatch, t.qc_mismatch);
    println!("  FPSR exception flags differ on matching results (not modelled): {}", t.fpsr_exc_only);
    println!("emulator runs a word the host calls UNDEFINED: {}", t.emu_runs_undefined);
    if !filter.is_empty() {
        println!("filter (any of {} masks): executed {f_exec}, exact match {f_match}, mismatch {}", filter.len(), f_exec - f_match);
    }
    let dump = |title: &str, m: &BTreeMap<u32, (u64, u32)>| {
        if m.is_empty() {
            return;
        }
        println!("{title} (class = word & 0xFFE0FC00: count, example):");
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        for (k, (cnt, ex)) in v.iter().take(top) {
            println!("  {k:#010x} {cnt:>6} {ex:#010x}");
        }
    };
    dump("mismatching classes", &mism_classes);
    dump("unimplemented classes", &unimpl_classes);
    dump("classes the emulator runs but the host rejects", &accepts);
    if t.value_mismatch + t.other_errors + t.emu_runs_undefined > 0 {
        1
    } else {
        0
    }
}

fn tier4_available() -> bool {
    #[cfg(feature = "native-jit")]
    {
        crate::cpu::tier4::Tier4JitEngine::new().unavailable.is_none()
    }
    #[cfg(not(feature = "native-jit"))]
    {
        false
    }
}

/// xoshiro256** seeded through splitmix64 (same family as the host fuzzer).
struct Rng([u64; 4]);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut z = seed;
        let mut next = || {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^ (x >> 31)
        };
        Rng([next(), next(), next(), next()])
    }

    fn next(&mut self) -> u64 {
        let s = &mut self.0;
        let r = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        r
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const F32_SPECIAL: [u32; 12] = [
    0, 0x8000_0000, 0x3f80_0000, 0xbf80_0000, 0x7f80_0000, 0xff80_0000, 0x7fc0_0000, 0x7f80_0001, 0x0000_0001,
    0x7f7f_ffff, 0x0080_0000, 0x4b00_0000,
];
const F64_SPECIAL: [u64; 12] = [
    0, 0x8000_0000_0000_0000, 0x3ff0_0000_0000_0000, 0xbff0_0000_0000_0000, 0x7ff0_0000_0000_0000,
    0xfff0_0000_0000_0000, 0x7ff8_0000_0000_0000, 0x7ff0_0000_0000_0001, 1, 0x7fef_ffff_ffff_ffff,
    0x0010_0000_0000_0000, 0x4330_0000_0000_0000,
];

fn rand_x(r: &mut Rng) -> u64 {
    match r.below(10) {
        0 => 0,
        1 => 1,
        2 => u64::MAX,
        3 => r.below(0x100),
        4 | 5 => DATA + 0x400 + r.below(0x1000) & if r.below(2) == 0 { !7 } else { !0 },
        6 => [0x8000_0000, 0x7fff_ffff, 0xffff_ffff, 0x8000_0000_0000_0000, 0x7fff_ffff_ffff_ffff][r.below(5) as usize],
        _ => r.next(),
    }
}

fn rand_v(r: &mut Rng) -> u128 {
    let lane = |r: &mut Rng, w: u32| -> u64 {
        match w {
            0 => F32_SPECIAL[r.below(12) as usize] as u64,
            1 => F64_SPECIAL[r.below(12) as usize],
            _ => r.next(),
        }
    };
    match r.below(6) {
        0 => 0,
        1 => {
            let l = lane(r, 0);
            (0..4).fold(0u128, |a, i| a | ((if r.below(2) == 0 { l } else { lane(r, 0) }) as u128) << (32 * i))
        }
        2 => (lane(r, 1) as u128) | (lane(r, 1) as u128) << 64,
        3 => {
            let b = r.next() as u8;
            u128::from_ne_bytes([b; 16])
        }
        _ => (r.next() as u128) | (r.next() as u128) << 64,
    }
}

fn rand_init(r: &mut Rng) -> Init {
    let mut x = [0u64; 31];
    for v in x.iter_mut() {
        *v = rand_x(r);
    }
    let mut v = [0u128; 32];
    for q in v.iter_mut() {
        *q = rand_v(r);
    }
    // Aliased registers make rd == rn/rm cases likely.
    if r.below(4) == 0 {
        v[1] = v[0];
    }
    if r.below(4) == 0 {
        v[2] = v[0];
    }
    let mut data = vec![0u8; DATA_LEN];
    for ch in data.chunks_mut(8) {
        ch.copy_from_slice(&r.next().to_le_bytes());
    }
    let sp = if r.below(2) == 0 { DATA + 0x1000 } else { (DATA + 0x400 + r.below(0x1000)) & !0xF };
    let fpcr = [0, 0, 0, 0x0040_0000, 0x0080_0000, 0x0100_0000, 0x0200_0000, 0x0300_0000][r.below(8) as usize];
    Init { nzcv: r.below(16) as u32, x, sp, v, fpcr, data }
}

/// A random word the decoder accepts (rejection sampling per top-level group, bits 28:25).
fn rand_word(r: &mut Rng) -> u32 {
    let mut w = r.next() as u32;
    for _ in 0..64 {
        w = (r.next() as u32 & !(0xF << 25)) | ((r.below(16) as u32) << 25);
        if !matches!(decode(w), Instruction::Unimplemented { .. }) {
            break;
        }
    }
    w
}

/// Engine-vs-engine mode: needs no host. Random decoder-accepted words and inputs (including a data
/// page and pointer-like registers) run through the interpreter, Tier-3 and, if built, Tier-4.
/// Compared: every X, SP, PC, V, NZCV, FPSR, FPCR, the exit, and the data page.
pub fn run_engines(seed: u64, count: u64, show: usize) -> i32 {
    let tier4 = tier4_available();
    if !tier4 {
        eprintln!("note: tier4 not available in this build/host; comparing interpreter vs Tier-3 only");
    }
    let mut r = Rng::new(seed);
    let (mut unimpl, mut compared, mut native_words, mut tier4_n) = (0u64, 0u64, 0u64, 0u64);
    let mut mism = [0u64; 2];
    let mut shown = 0;
    let mut classes = std::collections::BTreeSet::new();
    let mut exits: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_class: BTreeMap<(usize, u32), (u64, u32)> = BTreeMap::new();
    for _ in 0..count {
        let word = rand_word(&mut r);
        let init = rand_init(&mut r);
        let reference = match exec(Engine::Interp, word, &init).0 {
            Exec::Done(st) => st,
            _ => {
                unimpl += 1;
                continue;
            }
        };
        compared += 1;
        classes.insert(class(word));
        *exits.entry(reference.exit.split(' ').next().unwrap_or("").to_string()).or_default() += 1;
        for (k, (eng, name)) in [(Engine::Tier3, "tier3"), (Engine::Tier4, "tier4")].into_iter().enumerate() {
            if eng == Engine::Tier4 && !tier4 {
                continue;
            }
            let (res, native) = exec(eng, word, &init);
            if eng == Engine::Tier4 {
                tier4_n += 1;
                native_words += u64::from(native > 0);
            }
            let d = match res {
                Exec::Done(st) => diff_states(&reference, &st, "interp", name),
                other => vec![format!("{name}: {other:?}")],
            };
            if !d.is_empty() {
                mism[k] += 1;
                by_class.entry((k, class(word))).or_insert((0, word)).0 += 1;
                if shown < show {
                    shown += 1;
                    println!("MISMATCH {name} {word:08x} (seed {seed})");
                    for l in d.iter().take(6) {
                        println!("    {l}");
                    }
                }
            }
        }
    }
    println!("--- fuzz-replay engine-vs-engine summary (seed {seed}) ---");
    println!("words generated:                {count}");
    println!("interpreter unimplemented/other (skipped): {unimpl}");
    println!("compared:                       {compared} ({} distinct classes)", classes.len());
    println!("  interpreter exits:            {exits:?}");
    println!("  tier3 vs interp MISMATCH:     {}", mism[0]);
    if tier4 {
        println!("  tier4 vs interp MISMATCH:     {}", mism[1]);
        println!("  words Tier-4 ran natively:    {native_words} of {tier4_n} (the rest fell back to Tier-3/interpreter)");
    }
    if !by_class.is_empty() {
        println!("mismatching classes (tier: 0=tier3 1=tier4; class = word & 0xFFE0FC00: count, example):");
        for ((k, c), (n, ex)) in &by_class {
            println!("  {k} {c:#010x} {n:>6} {ex:#010x}");
        }
    }
    i32::from(mism[0] + mism[1] > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(word: u32, v: &[(usize, u128)], out_v: &[(usize, u128)], nzcv_in: u32, nzcv_out: u32, fpsr: u32) -> String {
        let mut vi = [0u128; 32];
        let mut vo = [0u128; 32];
        for &(i, x) in v {
            vi[i] = x;
        }
        for &(i, x) in out_v {
            vo[i] = x;
        }
        let xs = vec!["0"; 31].join(" ");
        let f = |a: &[u128; 32]| a.iter().map(|x| format!("{x:032x}")).collect::<Vec<_>>().join(" ");
        format!("T {word:08x} {nzcv_in:x} {xs} {} => {nzcv_out:x} {fpsr:x} {xs} {}", f(&vi), f(&vo))
    }

    fn case(l: &str) -> Case {
        match parse_line(l).unwrap() {
            Line::Test(c) => *c,
            _ => panic!("not a test line"),
        }
    }

    #[test]
    fn parses_both_line_kinds() {
        assert_eq!(parse_line("U 0fc710c5").unwrap(), Line::Undefined(0x0fc7_10c5));
        assert!(parse_line("T 1 2").is_err());
        assert!(parse_line("garbage").is_err());
    }

    #[test]
    fn tf_line_carries_fpcr_and_t_means_zero() {
        // FMAX S3,S0,S1 of +0 and a denormal: flush-to-zero (FPCR.FZ, bit 24) reads the denormal as zero.
        let t = line(0x1e21_4803, &[(0, 0), (1, 1)], &[(0, 0), (1, 1), (3, 0)], 0, 0, 0x80);
        assert_eq!(case(&t).fpcr, 0);
        let tf = t.replacen("T ", "TF 1000000 ", 1);
        let c = case(&tf);
        assert_eq!(c.fpcr, 0x100_0000);
        assert_eq!(replay(&c), Outcome::Ran(Vec::new(), 0), "FZ run must flush the denormal to +0");
        assert!(matches!(replay(&case(&t)), Outcome::Ran(d, _) if !d.is_empty()), "FPCR 0 keeps the denormal, so the FZ expectation differs");
        assert!(parse_line("TF 100000000 1e212803").is_err());
    }

    #[test]
    fn parser_rejects_overwide_fields_and_trailing_data() {
        assert!(parse_line("U 1d2800020").is_err());
        assert!(parse_line("U d2800020 extra").is_err());
        let good = line(0x1e22_2820, &[], &[], 0, 0, 0);
        assert!(parse_line(&good).is_ok());
        assert!(parse_line(&(good.clone() + " extra")).is_err());
        for (index, value) in [(1, "100000000"), (2, "10"), (3, "10000000000000000"), (67, "10"), (68, "100000000")] {
            let mut tokens = good.split_whitespace().collect::<Vec<_>>();
            tokens[index] = value;
            assert!(parse_line(&tokens.join(" ")).is_err(), "accepted overwide field {index}");
        }
    }

    #[test]
    fn replay_matches_a_known_good_case() {
        // fadd s0, s1, s2 : 1.0 + 2.0 = 3.0 (upper lanes cleared)
        let l = line(0x1e22_2820, &[(1, 0x3f80_0000), (2, 0x4000_0000)], &[(0, 0x4040_0000), (1, 0x3f80_0000), (2, 0x4000_0000)], 0, 0, 0);
        assert_eq!(replay(&case(&l)), Outcome::Ran(vec![], 0));
    }

    #[test]
    fn hardware_zero_times_infinity_vector_multiply_matches_on_all_engines() {
        let trace = include_str!("../docs/verify/insn-fuzzer/default-nan-mul2d/hardware.trace");
        let c = case(trace);
        for engine in [Engine::Interp, Engine::Tier3, Engine::Tier4] {
            if engine == Engine::Tier4 && !tier4_available() { continue; }
            match replay_on(&c, engine).0 {
                Outcome::Ran(differences, _) => assert!(differences.is_empty(), "{engine:?}: {differences:?}"),
                outcome => panic!("{engine:?}: {outcome:?}"),
            }
        }
    }

    #[test]
    fn replay_reports_a_wrong_host_result() {
        let l = line(0x1e22_2820, &[(1, 0x3f80_0000), (2, 0x4000_0000)], &[(0, 0x4040_0001), (1, 0x3f80_0000), (2, 0x4000_0000)], 0, 0, 0);
        match replay(&case(&l)) {
            Outcome::Ran(d, _) => assert!(d.len() == 1 && d[0].starts_with("v0:"), "{d:?}"),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn tiers_agree_on_a_known_good_case() {
        let l = line(0x1e22_2820, &[(1, 0x3f80_0000), (2, 0x4000_0000)], &[(0, 0x4040_0000), (1, 0x3f80_0000), (2, 0x4000_0000)], 0, 0, 0);
        for e in [Engine::Tier3, Engine::Tier4] {
            if e == Engine::Tier4 && !tier4_available() {
                continue;
            }
            assert_eq!(replay_on(&case(&l), e).0, Outcome::Ran(vec![], 0), "{e:?}");
        }
    }

    #[test]
    fn engine_vs_engine_small_run_has_no_mismatch() {
        assert_eq!(run_engines(7, 300, 5), 0);
    }

    #[test]
    fn unimplemented_words_are_reported_as_such() {
        let l = line(0x0000_0000, &[], &[], 0, 0, 0);
        assert_eq!(replay(&case(&l)), Outcome::Unimplemented);
    }
}
