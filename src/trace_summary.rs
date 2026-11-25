//! `ounce.trace.v1`: a bounded step trace and a first-divergence summary (`docs/OUNCE_TRACE_V1_SCHEMA.md`).
//!
//! - With `OUNCE_TRACE_SUMMARY=<path>` set, `run` / `run-nro` write the first `OUNCE_TRACE_STEPS` (default 1024,
//!   at most 65536) retired guest instructions as a `kind: "trace"` document: pc, opcode and the registers each
//!   instruction changed. The run goes one instruction at a time while this is set.
//! - `ounce-core trace-diff <expected.json> <actual.json>` compares two such documents (from any engine that
//!   implements the contract) and prints or writes a `kind: "divergence"` summary: the first differing step, the
//!   expected and actual value, and a small window of the steps before it.
//!
//! The contract is data only. This file is the active ounce-core implementation; archived engine names may still appear
//! in traces as historical producer identifiers.

use crate::cpu::Cpu;
use crate::mmu::Memory;
use crate::trace::State;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

pub const SCHEMA: &str = "ounce.trace.v1";
pub const DEFAULT_STEP_LIMIT: u64 = 1024;
pub const MAX_STEP_LIMIT: u64 = 65_536;
pub const DEFAULT_WINDOW: usize = 4;
pub const MAX_WINDOW: usize = 16;

const USAGE: &str =
    "usage: ounce-core trace-diff <expected.json> <actual.json> [--window N] [--elf program.elf] [--json out.json]";

/// Register ranks in comparison order: x0..x30, sp, nzcv, fpsr, fpcr, v0..v31.
const REG_COUNT: usize = 67;
const RANK_SP: usize = 31;
const RANK_NZCV: usize = 32;
const RANK_FPSR: usize = 33;
const RANK_FPCR: usize = 34;
const RANK_V0: usize = 35;

/// A store observed during one step (optional in the contract; ounce-core does not record these yet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemWrite {
    pub addr: u64,
    pub size: u8,
    pub value: u128,
}

/// One retired instruction: where it was, what it was, and which registers changed value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub pc: u64,
    pub opcode: u32,
    /// `(register rank, new value)` sorted by rank; see [`reg_name`].
    pub regs: Vec<(usize, u128)>,
    pub mem: Option<Vec<MemWrite>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    pub engine: String,
    pub engine_version: String,
    pub step_limit: u64,
    pub steps_executed: u64,
    pub truncated: bool,
    pub steps: Vec<Step>,
}

pub fn reg_name(rank: usize) -> String {
    match rank {
        0..=30 => format!("x{rank}"),
        RANK_SP => "sp".into(),
        RANK_NZCV => "nzcv".into(),
        RANK_FPSR => "fpsr".into(),
        RANK_FPCR => "fpcr".into(),
        _ => format!("v{}", rank - RANK_V0),
    }
}

pub fn reg_rank(name: &str) -> Option<usize> {
    match name {
        "sp" => return Some(RANK_SP),
        "nzcv" => return Some(RANK_NZCV),
        "fpsr" => return Some(RANK_FPSR),
        "fpcr" => return Some(RANK_FPCR),
        _ => {}
    }
    // `get` is None when the first character is multi-byte, so a hostile register name cannot panic here.
    let prefix = name.get(..1)?;
    let digits = &name[1..];
    if digits.is_empty()
        || digits.len() > 2
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || (digits.len() == 2 && digits.starts_with('0'))
    {
        return None;
    }
    let n: usize = digits.parse().ok()?;
    match prefix {
        "x" if n <= 30 => Some(n),
        "v" if n <= 31 => Some(RANK_V0 + n),
        _ => None,
    }
}

/// Fixed-width lowercase hex: 16 digits for x/sp, 8 for nzcv/fpsr/fpcr, 32 for v registers.
fn reg_hex(rank: usize, value: u128) -> String {
    match rank {
        RANK_NZCV..=RANK_FPCR => format!("0x{value:08x}"),
        RANK_V0.. => format!("0x{value:032x}"),
        _ => format!("0x{value:016x}"),
    }
}

fn hex64(value: u64) -> String {
    format!("0x{value:016x}")
}

fn parse_hex(v: &Value, what: &str, max_bits: u32) -> Result<u128, String> {
    let s = v.as_str().ok_or_else(|| format!("{what}: expected a hex string"))?;
    let digits = s
        .strip_prefix("0x")
        .ok_or_else(|| format!("{what}: expected 0x-prefixed hex, got {s:?}"))?;
    if digits.is_empty() || digits.len() > 32 {
        return Err(format!("{what}: bad hex {s:?}"));
    }
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{what}: bad hex {s:?}"));
    }
    let n = u128::from_str_radix(digits, 16).map_err(|_| format!("{what}: bad hex {s:?}"))?;
    if max_bits < 128 && n >> max_bits != 0 {
        return Err(format!("{what}: {s} does not fit in {max_bits} bits"));
    }
    Ok(n)
}

fn field<'a>(v: &'a Value, key: &str, ctx: &str) -> Result<&'a Value, String> {
    v.get(key).ok_or_else(|| format!("{ctx}: missing field {key}"))
}

fn string_field(v: &Value, key: &str) -> Result<String, String> {
    field(v, key, "document")?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("document: {key} must be a string"))
}

fn u64_field(v: &Value, key: &str) -> Result<u64, String> {
    field(v, key, "document")?
        .as_u64()
        .ok_or_else(|| format!("document: {key} must be a non-negative integer"))
}

impl MemWrite {
    fn to_json(&self) -> Value {
        json!({"addr": hex64(self.addr), "size": self.size, "value": format!("0x{:0width$x}", self.value, width = self.size as usize * 2)})
    }

    fn describe(&self) -> String {
        format!(
            "{}/{}=0x{:0width$x}",
            hex64(self.addr),
            self.size,
            self.value,
            width = self.size as usize * 2
        )
    }
}

impl Step {
    fn to_json(&self, index: usize) -> Value {
        let regs: Map<String, Value> = self
            .regs
            .iter()
            .map(|(rank, v)| (reg_name(*rank), json!(reg_hex(*rank, *v))))
            .collect();
        let mut step =
            json!({"i": index, "pc": hex64(self.pc), "opcode": format!("0x{:08x}", self.opcode), "regs": regs});
        if let Some(mem) = &self.mem {
            step["mem"] = Value::Array(mem.iter().map(MemWrite::to_json).collect());
        }
        step
    }

    fn from_json(v: &Value, index: usize) -> Result<Step, String> {
        let ctx = format!("steps[{index}]");
        if v.get("i").and_then(Value::as_u64) != Some(index as u64) {
            return Err(format!("{ctx}.i must equal its array position {index}"));
        }
        let pc = parse_hex(field(v, "pc", &ctx)?, &format!("{ctx}.pc"), 64)? as u64;
        let opcode = parse_hex(field(v, "opcode", &ctx)?, &format!("{ctx}.opcode"), 32)? as u32;
        let mut regs = Vec::new();
        for (name, value) in field(v, "regs", &ctx)?
            .as_object()
            .ok_or_else(|| format!("{ctx}.regs must be an object"))?
        {
            let rank = reg_rank(name).ok_or_else(|| format!("{ctx}.regs: unknown register {name}"))?;
            let bits = if rank >= RANK_V0 {
                128
            } else if rank >= RANK_NZCV {
                32
            } else {
                64
            };
            regs.push((rank, parse_hex(value, &format!("{ctx}.regs.{name}"), bits)?));
        }
        regs.sort();
        let mem = match v.get("mem") {
            None | Some(Value::Null) => None,
            Some(list) => {
                let list = list.as_array().ok_or_else(|| format!("{ctx}.mem must be an array"))?;
                let mut out = Vec::new();
                for (k, w) in list.iter().enumerate() {
                    let c = format!("{ctx}.mem[{k}]");
                    let size = field(w, "size", &c)?
                        .as_u64()
                        .filter(|s| (1..=16).contains(s))
                        .ok_or_else(|| format!("{c}.size must be 1..=16"))? as u8;
                    out.push(MemWrite {
                        addr: parse_hex(field(w, "addr", &c)?, &format!("{c}.addr"), 64)? as u64,
                        size,
                        value: parse_hex(field(w, "value", &c)?, &format!("{c}.value"), size as u32 * 8)?,
                    });
                }
                Some(out)
            }
        };
        Ok(Step { pc, opcode, regs, mem })
    }
}

impl Trace {
    pub fn to_json(&self) -> Value {
        json!({
            "schema": SCHEMA,
            "kind": "trace",
            "engine": self.engine,
            "engine_version": self.engine_version,
            "step_limit": self.step_limit,
            "steps_executed": self.steps_executed,
            "steps_recorded": self.steps.len(),
            "truncated": self.truncated,
            "steps": self.steps.iter().enumerate().map(|(i, s)| s.to_json(i)).collect::<Vec<_>>(),
        })
    }

    pub fn from_json(v: &Value) -> Result<Trace, String> {
        if v.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
            return Err(format!("document: schema must be {SCHEMA:?}"));
        }
        if v.get("kind").and_then(Value::as_str) != Some("trace") {
            return Err("document: kind must be \"trace\" (is this a divergence summary?)".into());
        }
        let step_limit = u64_field(v, "step_limit")?;
        if step_limit == 0 || step_limit > MAX_STEP_LIMIT {
            return Err(format!(
                "document: step_limit {step_limit} is outside 1..={MAX_STEP_LIMIT}"
            ));
        }
        let list = field(v, "steps", "document")?
            .as_array()
            .ok_or("document: steps must be an array")?;
        let recorded = u64_field(v, "steps_recorded")?;
        if recorded != list.len() as u64 || recorded > step_limit {
            return Err(format!(
                "document: steps_recorded {recorded} must equal the steps array length {} and not exceed step_limit {step_limit}",
                list.len()
            ));
        }
        let steps_executed = u64_field(v, "steps_executed")?;
        if steps_executed < recorded {
            return Err("document: steps_executed is smaller than steps_recorded".into());
        }
        let truncated = field(v, "truncated", "document")?
            .as_bool()
            .ok_or("document: truncated must be a boolean")?;
        if steps_executed > recorded && !truncated {
            return Err(format!(
                "document: truncated is false but steps_executed {steps_executed} exceeds steps_recorded {recorded}"
            ));
        }
        Ok(Trace {
            engine: string_field(v, "engine")?,
            engine_version: string_field(v, "engine_version")?,
            step_limit,
            steps_executed,
            truncated,
            steps: list
                .iter()
                .enumerate()
                .map(|(i, s)| Step::from_json(s, i))
                .collect::<Result<_, _>>()?,
        })
    }
}

fn state_reg(bytes: &[u8; crate::trace::STATE_BYTES], rank: usize) -> u128 {
    let le = |lo: usize, len: usize| {
        let mut b = [0u8; 16];
        b[..len].copy_from_slice(&bytes[lo..lo + len]);
        u128::from_le_bytes(b)
    };
    match rank {
        0..=30 => le(rank * 8, 8),
        RANK_SP => le(248, 8),
        RANK_NZCV => le(768, 4),
        RANK_FPSR => le(772, 4),
        RANK_FPCR => le(776, 4),
        _ => le(256 + (rank - RANK_V0) * 16, 16),
    }
}

/// A step that has started but not yet retired.
pub struct Pending {
    pc: u64,
    opcode: u32,
    pre: State,
}

/// Collects a bounded [`Trace`] from the run loop and writes it with [`Recorder::finish`].
pub struct Recorder {
    path: PathBuf,
    trace: Trace,
    broken: bool,
}

impl Recorder {
    pub fn new(path: PathBuf, step_limit: u64) -> Recorder {
        Recorder {
            path,
            trace: Trace {
                engine: "ounce-core".into(),
                engine_version: env!("CARGO_PKG_VERSION").into(),
                step_limit: step_limit.clamp(1, MAX_STEP_LIMIT),
                steps_executed: 0,
                truncated: false,
                steps: Vec::new(),
            },
            broken: false,
        }
    }

    /// `OUNCE_TRACE_SUMMARY=<path>` enables recording; `OUNCE_TRACE_STEPS=<n>` sets the bound (clamped to the maximum).
    pub fn from_env() -> Option<Recorder> {
        let path = std::env::var("OUNCE_TRACE_SUMMARY").ok().filter(|p| !p.is_empty())?;
        let limit = match std::env::var("OUNCE_TRACE_STEPS").ok().filter(|v| !v.is_empty()) {
            None => DEFAULT_STEP_LIMIT,
            Some(v) => match v.parse::<u64>() {
                Ok(n) if n > 0 => n.min(MAX_STEP_LIMIT),
                _ => {
                    eprintln!("[Trace] OUNCE_TRACE_STEPS={v} is not a positive integer; using {DEFAULT_STEP_LIMIT}");
                    DEFAULT_STEP_LIMIT
                }
            },
        };
        Some(Recorder::new(PathBuf::from(path), limit))
    }

    fn full(&self) -> bool {
        self.broken || self.trace.steps.len() as u64 >= self.trace.step_limit
    }

    /// Call before executing one instruction. `None` once the bound is reached (or recording was abandoned).
    pub fn pre(&mut self, cpu: &Cpu, mmu: &Memory) -> Option<Pending> {
        if self.full() {
            return None;
        }
        let pc = cpu.regs.pc;
        let opcode = mmu.read_u32(pc).ok()?;
        Some(Pending {
            pc,
            opcode,
            pre: State::capture(cpu),
        })
    }

    /// Call after an instruction retired, with what `pre` returned. Instructions past the bound are counted only.
    pub fn post(&mut self, cpu: &Cpu, pending: Option<Pending>) {
        self.trace.steps_executed += 1;
        let Some(p) = pending else {
            if self.full() {
                self.trace.truncated = true;
            }
            return;
        };
        let post = State::capture(cpu);
        let regs = (0..REG_COUNT)
            .filter_map(|rank| {
                let (before, after) = (state_reg(&p.pre.0, rank), state_reg(&post.0, rank));
                (before != after).then_some((rank, after))
            })
            .collect();
        self.trace.steps.push(Step {
            pc: p.pc,
            opcode: p.opcode,
            regs,
            mem: None,
        });
    }

    /// The engine ran something other than exactly one instruction, so the step index would drift: stop recording.
    pub fn abandon(&mut self) {
        self.broken = true;
        self.trace.truncated = true;
    }

    #[cfg(test)]
    pub fn trace(&self) -> &Trace {
        &self.trace
    }

    pub fn finish(&mut self) {
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(parent);
        }
        let written = serde_json::to_vec_pretty(&self.trace.to_json())
            .map_err(|e| e.to_string())
            .and_then(|b| std::fs::write(&self.path, b).map_err(|e| e.to_string()));
        match written {
            Ok(()) => eprintln!(
                "[Trace] {} of {} steps written to {}{}",
                self.trace.steps.len(),
                self.trace.steps_executed,
                self.path.display(),
                if self.trace.truncated { " (truncated)" } else { "" }
            ),
            Err(err) => eprintln!("[Trace] cannot write {}: {err}", self.path.display()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    pub step: u64,
    /// `pc`, `opcode`, `register`, `memory` or `length`.
    pub field: &'static str,
    pub name: Option<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub expected_engine: String,
    pub actual_engine: String,
    pub compared_steps: u64,
    pub truncated: bool,
    pub divergence: Option<Divergence>,
    /// `(step, pc, opcode)` of the steps just before the divergence (both sides agree on them).
    pub window: Vec<(u64, u64, u32)>,
    pub pc: (Option<u64>, Option<u64>),
    pub opcode: (Option<u32>, Option<u32>),
    /// The two steps at the divergence index (either may be absent for a `length` divergence).
    pub steps: (Option<Step>, Option<Step>),
}

fn first_reg_diff(a: &[(usize, u128)], b: &[(usize, u128)]) -> Option<(usize, Option<u128>, Option<u128>)> {
    let (mut i, mut j) = (0, 0);
    loop {
        match (a.get(i), b.get(j)) {
            (None, None) => return None,
            (Some(&(ra, va)), Some(&(rb, vb))) if ra == rb => {
                if va != vb {
                    return Some((ra, Some(va), Some(vb)));
                }
                i += 1;
                j += 1;
            }
            (Some(&(ra, va)), Some(&(rb, _))) if ra < rb => return Some((ra, Some(va), None)),
            (Some(_), Some(&(rb, vb))) => return Some((rb, None, Some(vb))),
            (Some(&(ra, va)), None) => return Some((ra, Some(va), None)),
            (None, Some(&(rb, vb))) => return Some((rb, None, Some(vb))),
        }
    }
}

fn first_mem_diff(a: &[MemWrite], b: &[MemWrite]) -> Option<(usize, Option<String>, Option<String>)> {
    (0..a.len().max(b.len())).find_map(|k| match (a.get(k), b.get(k)) {
        (Some(x), Some(y)) if x == y => None,
        (x, y) => Some((k, x.map(MemWrite::describe), y.map(MemWrite::describe))),
    })
}

fn step_diff(index: u64, e: &Step, a: &Step) -> Option<Divergence> {
    let hit = |field, name: Option<String>, expected: Option<String>, actual: Option<String>| {
        Some(Divergence {
            step: index,
            field,
            name,
            expected,
            actual,
        })
    };
    if e.pc != a.pc {
        return hit("pc", None, Some(hex64(e.pc)), Some(hex64(a.pc)));
    }
    if e.opcode != a.opcode {
        return hit(
            "opcode",
            None,
            Some(format!("0x{:08x}", e.opcode)),
            Some(format!("0x{:08x}", a.opcode)),
        );
    }
    if let Some((rank, x, y)) = first_reg_diff(&e.regs, &a.regs) {
        return hit(
            "register",
            Some(reg_name(rank)),
            x.map(|v| reg_hex(rank, v)),
            y.map(|v| reg_hex(rank, v)),
        );
    }
    if let (Some(em), Some(am)) = (&e.mem, &a.mem)
        && let Some((k, x, y)) = first_mem_diff(em, am)
    {
        return hit("memory", Some(format!("mem[{k}]")), x, y);
    }
    None
}

/// Find the first step where `actual` differs from `expected`. Per step the checks run in the order pc, opcode,
/// register (canonical register order), memory (only when both sides record it). `window` is clamped to [`MAX_WINDOW`].
pub fn compare(expected: &Trace, actual: &Trace, window: usize) -> Comparison {
    let common = expected.steps.len().min(actual.steps.len());
    let mut divergence = None;
    let mut compared = common as u64;
    for i in 0..common {
        if let Some(d) = step_diff(i as u64, &expected.steps[i], &actual.steps[i]) {
            compared = i as u64 + 1;
            divergence = Some(d);
            break;
        }
    }
    if divergence.is_none() && expected.steps.len() != actual.steps.len() {
        let shorter_is_complete = if expected.steps.len() < actual.steps.len() {
            !expected.truncated
        } else {
            !actual.truncated
        };
        if shorter_is_complete {
            divergence = Some(Divergence {
                step: common as u64,
                field: "length",
                name: None,
                expected: Some(expected.steps.len().to_string()),
                actual: Some(actual.steps.len().to_string()),
            });
        }
    }
    let at = divergence.as_ref().map(|d| d.step as usize);
    let window_steps = match at {
        Some(at) => {
            let take = window.min(MAX_WINDOW).min(at);
            expected.steps[at - take..at]
                .iter()
                .enumerate()
                .map(|(k, s)| ((at - take + k) as u64, s.pc, s.opcode))
                .collect()
        }
        None => Vec::new(),
    };
    let (pc, opcode) = match at {
        Some(at) => (
            (expected.steps.get(at).map(|s| s.pc), actual.steps.get(at).map(|s| s.pc)),
            (
                expected.steps.get(at).map(|s| s.opcode),
                actual.steps.get(at).map(|s| s.opcode),
            ),
        ),
        None => ((None, None), (None, None)),
    };
    Comparison {
        expected_engine: expected.engine.clone(),
        actual_engine: actual.engine.clone(),
        compared_steps: compared,
        truncated: expected.truncated || actual.truncated,
        divergence,
        window: window_steps,
        pc,
        opcode,
        steps: match at {
            Some(at) => (expected.steps.get(at).cloned(), actual.steps.get(at).cloned()),
            None => (None, None),
        },
    }
}

/// Registers whose recorded change differs between the two steps, in canonical order: `(rank, expected, actual)`,
/// where `None` means that side did not change the register in this step.
fn reg_diffs(e: &Step, a: &Step) -> Vec<(usize, Option<u128>, Option<u128>)> {
    let mut ranks: Vec<usize> = e.regs.iter().chain(&a.regs).map(|r| r.0).collect();
    ranks.sort_unstable();
    ranks.dedup();
    let find = |s: &Step, r: usize| s.regs.iter().find(|x| x.0 == r).map(|x| x.1);
    ranks
        .into_iter()
        .filter_map(|r| {
            let (x, y) = (find(e, r), find(a, r));
            (x != y).then_some((r, x, y))
        })
        .collect()
}

/// `NZcv`-style flags (upper case = set), FPSR cumulative flags, or FPCR mode bits, for the three status registers.
fn flag_split(rank: usize, v: u128) -> String {
    let v = v as u32;
    let names = |bits: &[(u32, &str)]| {
        let set: Vec<&str> = bits.iter().filter(|(b, _)| v >> b & 1 == 1).map(|(_, n)| *n).collect();
        if set.is_empty() { "-".to_string() } else { set.join("|") }
    };
    match rank {
        RANK_NZCV => [(31, "N"), (30, "Z"), (29, "C"), (28, "V")]
            .iter()
            .map(|(b, n)| {
                if v >> b & 1 == 1 {
                    n.to_string()
                } else {
                    n.to_lowercase()
                }
            })
            .collect(),
        RANK_FPSR => names(&[
            (0, "IOC"),
            (1, "DZC"),
            (2, "OFC"),
            (3, "UFC"),
            (4, "IXC"),
            (7, "IDC"),
            (27, "QC"),
        ]),
        _ => {
            let mode = ["RN", "RP", "RM", "RZ"][(v >> 22 & 3) as usize];
            format!(
                "RMode={mode} {}",
                names(&[(26, "AHP"), (25, "DN"), (24, "FZ"), (19, "FZ16")])
            )
        }
    }
}

/// Instruction name from the decoder's `Debug` form, e.g. `AddShifted { rd: 0, .. }`.
fn decoded(opcode: u32) -> String {
    format!("{:?}", crate::cpu::decoder::decode(opcode))
}

impl Comparison {
    /// The multi-line readout printed after the headline: step, pc (+ `symbol+0xoff` when `symbols` is given), the
    /// decoded instruction, the registers that differ side by side, and the NZCV/FPSR/FPCR split.
    pub fn readout(&self, symbols: Option<&crate::linux::elf::SymbolTable>) -> Vec<String> {
        let (Some(d), (e, a)) = (&self.divergence, &self.steps) else {
            return Vec::new();
        };
        let sym = |pc: u64| match symbols.and_then(|t| t.resolve(pc)) {
            Some(s) => format!("{} <{s}>", hex64(pc)),
            None => hex64(pc),
        };
        let pair = |x: Option<String>, y: Option<String>| match (&x, &y) {
            (Some(x), Some(y)) if x == y => x.clone(),
            _ => format!(
                "expected {} | actual {}",
                x.unwrap_or_else(|| "(absent)".into()),
                y.unwrap_or_else(|| "(absent)".into())
            ),
        };
        let mut out = vec![format!("  step     {}", d.step)];
        out.push(format!(
            "  pc       {}",
            pair(e.as_ref().map(|s| sym(s.pc)), a.as_ref().map(|s| sym(s.pc)))
        ));
        out.push(format!(
            "  insn     {}",
            pair(
                e.as_ref().map(|s| format!("0x{:08x} {}", s.opcode, decoded(s.opcode))),
                a.as_ref().map(|s| format!("0x{:08x} {}", s.opcode, decoded(s.opcode)))
            )
        ));
        if let (Some(e), Some(a)) = (e, a) {
            let show = |rank, v: Option<u128>| v.map_or("(unchanged)".to_string(), |v| reg_hex(rank, v));
            for (rank, x, y) in reg_diffs(e, a) {
                out.push(format!(
                    "  {:<8} expected {} | actual {}",
                    reg_name(rank),
                    show(rank, x),
                    show(rank, y)
                ));
                if (RANK_NZCV..=RANK_FPCR).contains(&rank) {
                    let split = |v: Option<u128>| v.map_or("(unchanged)".to_string(), |v| flag_split(rank, v));
                    out.push(format!(
                        "           split    expected {} | actual {}",
                        split(x),
                        split(y)
                    ));
                }
            }
        }
        out
    }

    /// Additive JSON for the readout (only written when `--elf` is given; see docs/DIVERGE.md).
    fn readout_json(&self, symbols: &crate::linux::elf::SymbolTable) -> Value {
        let (e, a) = &self.steps;
        let reg = |rank: usize, v: Option<u128>| v.map(|v| reg_hex(rank, v));
        let diffs: Vec<Value> = match (e, a) {
            (Some(e), Some(a)) => reg_diffs(e, a)
                .into_iter()
                .map(|(r, x, y)| json!({"reg": reg_name(r), "expected": reg(r, x), "actual": reg(r, y)}))
                .collect(),
            _ => Vec::new(),
        };
        json!({
            "symbol_expected": e.as_ref().and_then(|s| symbols.resolve(s.pc)),
            "symbol_actual": a.as_ref().and_then(|s| symbols.resolve(s.pc)),
            "insn_expected": e.as_ref().map(|s| decoded(s.opcode)),
            "insn_actual": a.as_ref().map(|s| decoded(s.opcode)),
            "reg_diffs": diffs,
        })
    }
}

impl Comparison {
    pub fn to_json(&self) -> Value {
        let d = self.divergence.as_ref();
        let doc = json!({
            "schema": SCHEMA,
            "kind": "divergence",
            "engine": "ounce-core",
            "engine_version": env!("CARGO_PKG_VERSION"),
            "expected_engine": self.expected_engine,
            "actual_engine": self.actual_engine,
            "match": d.is_none(),
            "compared_steps": self.compared_steps,
            "truncated": self.truncated,
            "step": d.map(|d| d.step),
            "field": d.map(|d| d.field),
            "name": d.and_then(|d| d.name.clone()),
            "expected": d.and_then(|d| d.expected.clone()),
            "actual": d.and_then(|d| d.actual.clone()),
            "pc_expected": self.pc.0.map(hex64),
            "pc_actual": self.pc.1.map(hex64),
            "opcode_expected": self.opcode.0.map(|o| format!("0x{o:08x}")),
            "opcode_actual": self.opcode.1.map(|o| format!("0x{o:08x}")),
            "window": self.window.iter().map(|(i, pc, op)| json!({"i": i, "pc": hex64(*pc), "opcode": format!("0x{op:08x}")})).collect::<Vec<_>>(),
        });
        doc
    }

    /// [`Comparison::to_json`] plus a `readout` object (symbols, decoded instructions, differing registers).
    pub fn to_json_with_readout(&self, symbols: &crate::linux::elf::SymbolTable) -> Value {
        let mut doc = self.to_json();
        if self.divergence.is_some() {
            doc["readout"] = self.readout_json(symbols);
        }
        doc
    }

    fn describe(&self) -> String {
        let Some(d) = &self.divergence else {
            let note = if self.truncated {
                " (a trace was truncated: only the compared steps are covered)"
            } else {
                ""
            };
            return format!("no divergence in {} compared steps{note}", self.compared_steps);
        };
        let show = |v: &Option<String>| v.clone().unwrap_or_else(|| "(unchanged / absent)".into());
        let what = match &d.name {
            Some(n) => format!("{} {n}", d.field),
            None => d.field.to_string(),
        };
        let pc = self
            .pc
            .0
            .or(self.pc.1)
            .map(|p| format!(" at pc {}", hex64(p)))
            .unwrap_or_default();
        format!(
            "first divergence at step {}{pc}: {what}: expected {} ({}), actual {} ({})",
            d.step,
            show(&d.expected),
            self.expected_engine,
            show(&d.actual),
            self.actual_engine
        )
    }
}

fn load(path: &str) -> Result<Trace, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| format!("{path}: not JSON: {e}"))?;
    Trace::from_json(&value).map_err(|e| format!("{path}: {e}"))
}

/// `ounce-core trace-diff`: exit `0` no divergence in the compared steps, `1` divergence, `2` bad input.
pub fn run_cli(args: &[String]) -> i32 {
    match cli(args) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("{msg}");
            2
        }
    }
}

fn cli(args: &[String]) -> Result<i32, String> {
    let (mut files, mut window, mut json_out, mut elf) = (Vec::new(), DEFAULT_WINDOW, None, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--window" => {
                let v = it.next().ok_or(USAGE)?;
                window = v
                    .parse()
                    .map_err(|_| format!("--window needs a non-negative integer, got {v:?}\n{USAGE}"))?;
            }
            "--json" => json_out = Some(it.next().ok_or(USAGE)?.clone()),
            "--elf" => elf = Some(it.next().ok_or(USAGE)?.clone()),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}\n{USAGE}")),
            _ => files.push(arg.as_str()),
        }
    }
    let [expected, actual] = files[..] else {
        return Err(USAGE.into());
    };
    let comparison = compare(&load(expected)?, &load(actual)?, window);
    let symbols = match &elf {
        Some(path) => Some(crate::linux::elf::SymbolTable::parse(
            &std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?,
        )),
        None => None,
    };
    println!("{}", comparison.describe());
    for line in comparison.readout(symbols.as_ref()) {
        println!("{line}");
    }
    if let Some(path) = json_out {
        let doc = match &symbols {
            Some(t) => comparison.to_json_with_readout(t),
            None => comparison.to_json(),
        };
        let bytes = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
        if let Some(dir) = std::path::Path::new(&path)
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
        {
            std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("cannot write {path}: {e}"))?;
    }
    Ok(if comparison.divergence.is_some() { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::StepResult;
    use crate::mmu::Permission;
    use std::path::Path;

    const CODE: u64 = 0x8000_0000;
    const ADD_X0_X1_X2: u32 = 0x8B02_0020;
    const MOVZ_X3_7: u32 = 0xD280_00E3;

    fn step(pc: u64, opcode: u32, regs: &[(&str, u128)]) -> Step {
        let mut regs: Vec<(usize, u128)> = regs.iter().map(|(n, v)| (reg_rank(n).unwrap(), *v)).collect();
        regs.sort();
        Step {
            pc,
            opcode,
            regs,
            mem: None,
        }
    }

    fn trace(engine: &str, steps: Vec<Step>) -> Trace {
        Trace {
            engine: engine.into(),
            engine_version: "0.0.0".into(),
            step_limit: 1024,
            steps_executed: steps.len() as u64,
            truncated: false,
            steps,
        }
    }

    /// `n` steps with deterministic deltas, so two copies compare equal.
    fn base(engine: &str, n: u64) -> Trace {
        trace(
            engine,
            (0..n)
                .map(|i| {
                    step(
                        CODE + 4 * i,
                        ADD_X0_X1_X2,
                        &[("x0", i as u128 + 100), ("nzcv", 0x6000_0000)],
                    )
                })
                .collect(),
        )
    }

    fn machine(ops: &[u32]) -> (Cpu, Memory) {
        let mut mem = Memory::new();
        mem.map_range(CODE, 4096, Permission::READ_WRITE_EXECUTE);
        for (i, op) in ops.iter().enumerate() {
            mem.write_u32(CODE + 4 * i as u64, *op).unwrap();
        }
        let mut cpu = Cpu::new();
        cpu.regs.pc = CODE;
        cpu.regs.x[1] = 5;
        cpu.regs.x[2] = 6;
        (cpu, mem)
    }

    fn record(rec: &mut Recorder, cpu: &mut Cpu, mem: &mut Memory) {
        let pending = rec.pre(cpu, mem);
        assert!(matches!(cpu.step(mem), Ok(StepResult::Continue)));
        rec.post(cpu, pending);
    }

    // ---- recorder ----

    #[test]
    fn recorder_captures_pc_opcode_and_only_changed_registers() {
        let (mut cpu, mut mem) = machine(&[ADD_X0_X1_X2, MOVZ_X3_7]);
        let mut rec = Recorder::new(PathBuf::from("unused"), 16);
        record(&mut rec, &mut cpu, &mut mem);
        record(&mut rec, &mut cpu, &mut mem);
        let t = rec.trace();
        assert_eq!(t.steps_executed, 2);
        assert!(!t.truncated);
        assert_eq!(t.steps[0], step(CODE, ADD_X0_X1_X2, &[("x0", 11)]));
        assert_eq!(t.steps[1], step(CODE + 4, MOVZ_X3_7, &[("x3", 7)]));
    }

    #[test]
    fn recorder_is_bounded_and_reports_truncation() {
        let (mut cpu, mut mem) = machine(&[ADD_X0_X1_X2, MOVZ_X3_7, ADD_X0_X1_X2, MOVZ_X3_7, ADD_X0_X1_X2]);
        let mut rec = Recorder::new(PathBuf::from("unused"), 2);
        for _ in 0..5 {
            record(&mut rec, &mut cpu, &mut mem);
        }
        let t = rec.trace();
        assert_eq!(t.steps.len(), 2, "never stores more than the limit");
        assert_eq!(t.steps_executed, 5, "but still counts every retired step");
        assert!(t.truncated);
        assert_eq!(t.step_limit, 2);
    }

    #[test]
    fn recorder_at_exactly_the_limit_is_not_truncated() {
        let (mut cpu, mut mem) = machine(&[ADD_X0_X1_X2, MOVZ_X3_7]);
        let mut rec = Recorder::new(PathBuf::from("unused"), 2);
        record(&mut rec, &mut cpu, &mut mem);
        record(&mut rec, &mut cpu, &mut mem);
        assert!(!rec.trace().truncated);
    }

    #[test]
    fn abandoned_step_stops_recording_and_marks_truncated() {
        let (mut cpu, mut mem) = machine(&[ADD_X0_X1_X2, MOVZ_X3_7]);
        let mut rec = Recorder::new(PathBuf::from("unused"), 16);
        record(&mut rec, &mut cpu, &mut mem);
        rec.abandon();
        assert!(rec.pre(&cpu, &mem).is_none());
        assert!(rec.trace().truncated);
        assert_eq!(rec.trace().steps.len(), 1);
    }

    // ---- document round trip and validation ----

    #[test]
    fn trace_round_trips_through_json() {
        let mut t = base("ounce-core", 3);
        t.steps[1]
            .regs
            .push((reg_rank("v7").unwrap(), 0x0123_4567_89ab_cdef_0011_2233_4455_6677));
        t.steps[2].mem = Some(vec![MemWrite {
            addr: 0x1000,
            size: 8,
            value: 0xdead_beef,
        }]);
        t.truncated = true;
        let v = t.to_json();
        assert_eq!(v["schema"], "ounce.trace.v1");
        assert_eq!(v["kind"], "trace");
        assert_eq!(v["steps_recorded"], 3);
        assert_eq!(v["steps"][0]["pc"], "0x0000000080000000");
        assert_eq!(v["steps"][0]["regs"]["x0"], "0x0000000000000064");
        assert_eq!(Trace::from_json(&v).unwrap(), t);
    }

    #[test]
    fn from_json_rejects_malformed_documents() {
        let good = base("e", 2).to_json();
        let mutate = |f: &dyn Fn(&mut Value)| {
            let mut v = good.clone();
            f(&mut v);
            Trace::from_json(&v).unwrap_err()
        };
        assert!(mutate(&|v| v["schema"] = json!("ounce.trace.v2")).contains("schema"));
        assert!(mutate(&|v| v["kind"] = json!("divergence")).contains("kind"));
        assert!(mutate(&|v| v["steps_recorded"] = json!(5)).contains("steps_recorded"));
        assert!(mutate(&|v| v["step_limit"] = json!(MAX_STEP_LIMIT + 1)).contains("step_limit"));
        assert!(mutate(&|v| v["steps"][0]["regs"] = json!({"x31": "0x0"})).contains("x31"));
        assert!(mutate(&|v| v["steps"][0]["pc"] = json!("80000000")).contains("pc"));
        assert!(mutate(&|v| v["steps"][0]["pc"] = json!("0xZZ")).contains("pc"));
        assert!(mutate(&|v| v["steps"][1]["i"] = json!(7)).contains("steps[1].i"));
        assert!(mutate(&|v| v["steps"][0]["regs"]["x0"] = json!(5)).contains("x0"));
        assert!(mutate(&|v| v["steps"][0]["opcode"] = json!("0x100000000")).contains("opcode"));
        // A multi-byte first character in a register name is a malformed document, not a panic (found by review).
        assert!(mutate(&|v| v["steps"][0]["regs"] = json!({"\u{e9}x": "0x1"})).contains("regs"));
        // "0x+1f" is not canonical hex even though from_str_radix would take it.
        assert!(mutate(&|v| v["steps"][0]["pc"] = json!("0x+1f")).contains("pc"));
        // More steps retired than stored, yet `truncated` is false: the writer is inconsistent, and trusting it would
        // report a false "length" divergence.
        assert!(mutate(&|v| v["steps_executed"] = json!(99)).contains("truncated"));
    }

    #[test]
    fn register_names_are_canonical() {
        assert_eq!(reg_name(0), "x0");
        assert_eq!(reg_name(30), "x30");
        assert_eq!(reg_name(31), "sp");
        assert_eq!(reg_name(32), "nzcv");
        assert_eq!(reg_name(34), "fpcr");
        assert_eq!(reg_name(35), "v0");
        assert_eq!(reg_name(66), "v31");
        for rank in 0..67 {
            assert_eq!(reg_rank(&reg_name(rank)), Some(rank));
        }
        assert_eq!(reg_rank("x31"), None);
        assert_eq!(reg_rank("v32"), None);
        assert_eq!(reg_rank("pc"), None);
        assert_eq!(reg_rank("\u{e9}x"), None);
        assert_eq!(reg_rank("x\u{e9}"), None);
        assert_eq!(reg_rank(""), None);
    }

    // ---- comparison ----

    #[test]
    fn identical_traces_match() {
        let c = compare(&base("a", 10), &base("b", 10), DEFAULT_WINDOW);
        assert_eq!(c.divergence, None);
        assert_eq!(c.compared_steps, 10);
        assert!(!c.truncated);
        let j = c.to_json();
        assert_eq!(j["match"], true);
        assert_eq!(j["step"], Value::Null);
        assert_eq!(j["field"], Value::Null);
        assert_eq!(j["window"], json!([]));
        assert_eq!(j["expected_engine"], "a");
        assert_eq!(j["actual_engine"], "b");
    }

    #[test]
    fn first_register_divergence_is_reported_with_values_and_window() {
        let expected = base("ounce-core", 10);
        let mut actual = base("ounce-runtime", 10);
        actual.steps[6].regs[0].1 = 0xbad;
        actual.steps[8].regs[0].1 = 0xbad2; // a later difference must not win
        let c = compare(&expected, &actual, 3);
        assert_eq!(
            c.divergence,
            Some(Divergence {
                step: 6,
                field: "register",
                name: Some("x0".into()),
                expected: Some("0x000000000000006a".into()),
                actual: Some("0x0000000000000bad".into()),
            })
        );
        assert_eq!(c.compared_steps, 7);
        assert_eq!(
            c.window,
            vec![
                (3, CODE + 12, ADD_X0_X1_X2),
                (4, CODE + 16, ADD_X0_X1_X2),
                (5, CODE + 20, ADD_X0_X1_X2)
            ]
        );
        let j = c.to_json();
        assert_eq!(j["match"], false);
        assert_eq!(j["step"], 6);
        assert_eq!(j["field"], "register");
        assert_eq!(j["name"], "x0");
        assert_eq!(j["pc_expected"], "0x0000000080000018");
        assert_eq!(j["pc_actual"], "0x0000000080000018");
        assert_eq!(j["window"][0]["i"], 3);
    }

    #[test]
    fn register_walk_uses_canonical_order_and_unchanged_is_null() {
        let expected = trace("a", vec![step(CODE, 1, &[("x5", 1), ("nzcv", 2)])]);
        let actual = trace("b", vec![step(CODE, 1, &[("x5", 1), ("x9", 3), ("nzcv", 2)])]);
        let d = compare(&expected, &actual, 4).divergence.unwrap();
        assert_eq!((d.field, d.name.as_deref()), ("register", Some("x9")));
        assert_eq!(d.expected, None, "expected side left x9 unchanged");
        assert_eq!(d.actual.as_deref(), Some("0x0000000000000003"));
        // scalar registers sort before vector registers
        let expected = trace("a", vec![step(CODE, 1, &[("v0", 1)])]);
        let actual = trace("b", vec![step(CODE, 1, &[("v0", 2), ("x10", 4)])]);
        assert_eq!(
            compare(&expected, &actual, 4).divergence.unwrap().name.as_deref(),
            Some("x10")
        );
    }

    #[test]
    fn pc_beats_opcode_beats_register() {
        let expected = base("a", 4);
        let mut actual = base("b", 4);
        actual.steps[2].opcode = MOVZ_X3_7;
        actual.steps[2].regs[0].1 = 0;
        assert_eq!(compare(&expected, &actual, 4).divergence.unwrap().field, "opcode");
        actual.steps[2].pc += 4;
        let d = compare(&expected, &actual, 4).divergence.unwrap();
        assert_eq!((d.field, d.step), ("pc", 2));
        assert_eq!(d.expected.as_deref(), Some("0x0000000080000008"));
        assert_eq!(d.actual.as_deref(), Some("0x000000008000000c"));
    }

    #[test]
    fn window_is_bounded() {
        let mut actual = base("b", 40);
        actual.steps[30].regs[0].1 = 0;
        let expected = base("a", 40);
        assert_eq!(compare(&expected, &actual, 0).window.len(), 0);
        let c = compare(&expected, &actual, 1_000_000);
        assert_eq!(c.window.len(), MAX_WINDOW);
        assert_eq!(c.window.first().unwrap().0, 30 - MAX_WINDOW as u64);
        assert_eq!(c.window.last().unwrap().0, 29);
        let mut early = base("b", 3);
        early.steps[1].regs[0].1 = 0;
        assert_eq!(
            compare(&base("a", 3), &early, 8).window.len(),
            1,
            "cannot reach before step 0"
        );
    }

    #[test]
    fn shorter_complete_trace_is_a_length_divergence() {
        let d = compare(&base("a", 6), &base("b", 4), 2).divergence.unwrap();
        assert_eq!((d.step, d.field), (4, "length"));
        assert_eq!((d.expected.as_deref(), d.actual.as_deref()), (Some("6"), Some("4")));
        let d = compare(&base("a", 4), &base("b", 6), 2).divergence.unwrap();
        assert_eq!((d.expected.as_deref(), d.actual.as_deref()), (Some("4"), Some("6")));
    }

    #[test]
    fn shorter_truncated_trace_is_not_a_divergence() {
        let mut short = base("b", 4);
        short.truncated = true;
        short.steps_executed = 99;
        let c = compare(&base("a", 6), &short, 2);
        assert_eq!(c.divergence, None);
        assert_eq!(c.compared_steps, 4);
        assert!(
            c.truncated,
            "a match over a truncated trace only covers the compared steps"
        );
        let mut a = base("a", 4);
        a.truncated = true;
        let c = compare(&a, &short, 2);
        assert!(c.divergence.is_none() && c.truncated);
    }

    #[test]
    fn memory_deltas_compare_only_when_both_sides_record_them() {
        let w = |v: u128| {
            Some(vec![MemWrite {
                addr: 0x2000,
                size: 4,
                value: v,
            }])
        };
        let mut expected = base("a", 3);
        let mut actual = base("b", 3);
        expected.steps[1].mem = w(1);
        assert_eq!(
            compare(&expected, &actual, 2).divergence,
            None,
            "absent means not recorded"
        );
        actual.steps[1].mem = w(2);
        let d = compare(&expected, &actual, 2).divergence.unwrap();
        assert_eq!((d.step, d.field, d.name.as_deref()), (1, "memory", Some("mem[0]")));
        assert_eq!(d.expected.as_deref(), Some("0x0000000000002000/4=0x00000001"));
        assert_eq!(d.actual.as_deref(), Some("0x0000000000002000/4=0x00000002"));
        actual.steps[1].mem = Some(vec![]);
        let d = compare(&expected, &actual, 2).divergence.unwrap();
        assert_eq!((d.field, d.actual), ("memory", None));
    }

    // ---- schema document ----

    fn doc_fields(heading: &str) -> Vec<String> {
        let doc = include_str!("../docs/OUNCE_TRACE_V1_SCHEMA.md");
        let start = doc
            .find(&format!("## {heading}"))
            .unwrap_or_else(|| panic!("section {heading} missing"));
        let body = &doc[start + 3..];
        let end = body.find("\n## ").unwrap_or(body.len());
        sorted(
            body[..end]
                .lines()
                .filter_map(|l| l.strip_prefix("| `"))
                .filter_map(|l| l.split('`').next())
                .map(str::to_string)
                .collect(),
        )
    }

    fn keys(v: &Value) -> Vec<String> {
        sorted(v.as_object().unwrap().keys().cloned().collect())
    }

    fn sorted(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v
    }

    #[test]
    fn schema_doc_lists_exactly_the_emitted_keys() {
        let mut t = base("ounce-core", 3);
        t.steps[0].mem = Some(vec![MemWrite {
            addr: 1,
            size: 1,
            value: 1,
        }]);
        let tv = t.to_json();
        assert_eq!(keys(&tv), doc_fields("Trace document"));
        assert_eq!(keys(&tv["steps"][0]), doc_fields("Step"));
        assert_eq!(keys(&tv["steps"][0]["mem"][0]), doc_fields("Memory write"));

        let mut a = base("ounce-runtime", 3);
        a.steps[2].regs[0].1 = 0;
        let dv = compare(&t, &a, 4).to_json();
        assert_eq!(keys(&dv), doc_fields("Divergence summary"));
        assert_eq!(keys(&dv["window"][0]), doc_fields("Window entry"));
        assert_eq!(
            keys(&compare(&t, &t, 4).to_json()),
            keys(&dv),
            "match and divergence summaries share one key set"
        );
    }

    #[test]
    fn schema_doc_names_every_env_var_value_and_limit() {
        let doc = include_str!("../docs/OUNCE_TRACE_V1_SCHEMA.md");
        for needle in [
            "OUNCE_TRACE_SUMMARY",
            "OUNCE_TRACE_STEPS",
            "ounce.trace.v1",
            "`length`",
            "`memory`",
            "`register`",
            "`opcode`",
            "`pc`",
        ] {
            assert!(doc.contains(needle), "doc lacks {needle}");
        }
        assert!(doc.contains(&MAX_STEP_LIMIT.to_string()) && doc.contains(&DEFAULT_STEP_LIMIT.to_string()));
        assert!(doc.contains(&MAX_WINDOW.to_string()));
    }

    // ---- CLI ----

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ounce-trace-summary-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn write(path: &Path, t: &Trace) {
        std::fs::write(path, serde_json::to_vec_pretty(&t.to_json()).unwrap()).unwrap();
    }

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cli_exit_codes_and_json_output() {
        let (e, a, out) = (tmp("e.json"), tmp("a.json"), tmp("out.json"));
        write(&e, &base("a", 5));
        write(&a, &base("b", 5));
        let (es, as_, os) = (e.to_str().unwrap(), a.to_str().unwrap(), out.to_str().unwrap());
        assert_eq!(run_cli(&args(&[es, as_, "--json", os])), 0);
        let j: Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        assert_eq!(j["match"], true);

        let mut bad = base("b", 5);
        bad.steps[3].regs[0].1 = 1;
        write(&a, &bad);
        assert_eq!(run_cli(&args(&[es, as_, "--window", "2", "--json", os])), 1);
        let j: Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        assert_eq!((j["match"].clone(), j["step"].clone()), (json!(false), json!(3)));
        assert_eq!(j["window"].as_array().unwrap().len(), 2);

        assert_eq!(run_cli(&args(&[es, "/nonexistent/trace.json"])), 2);
        assert_eq!(run_cli(&args(&[es])), 2);
        assert_eq!(run_cli(&args(&[es, as_, "--window", "x"])), 2);
        std::fs::write(&a, b"{\"schema\":\"other\"}").unwrap();
        assert_eq!(run_cli(&args(&[es, as_])), 2);
    }

    #[test]
    fn finish_writes_a_valid_document() {
        let (mut cpu, mut mem) = machine(&[ADD_X0_X1_X2]);
        let path = tmp("rec.json");
        let mut rec = Recorder::new(path.clone(), 8);
        record(&mut rec, &mut cpu, &mut mem);
        rec.finish();
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let t = Trace::from_json(&v).unwrap();
        assert_eq!(t.engine, "ounce-core");
        assert_eq!(t.engine_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(t.steps.len(), 1);
    }
}
