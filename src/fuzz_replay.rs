//! `ounce-core fuzz-replay <trace|->`: replays a single-instruction trace made by
//! `docs/verify/insn-fuzzer/fuzz_host.c` (real Apple Silicon results) in the Tier-3 interpreter
//! (`Cpu::step`) and compares every X register, V register, NZCV and FPSR.
//!
//! Trace lines (hex): `U <word>` (host raised SIGILL) or
//! `T <word> <nzcv_in> <31 x> <32 v> => <nzcv_out> <fpsr_out> <31 x> <32 v>`.

use crate::cpu::{Cpu, StepResult};
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

pub fn parse_line(line: &str) -> Result<Line, String> {
    let mut t = line.split_whitespace();
    match t.next() {
        Some("U") => Ok(Line::Undefined(hex(&mut t)? as u32)),
        Some("T") => {
            let word = hex(&mut t)? as u32;
            let nzcv_in = hex(&mut t)? as u32;
            let mut x_in = [0u64; 31];
            let mut v_in = [0u128; 32];
            let mut x_out = [0u64; 31];
            let mut v_out = [0u128; 32];
            for x in x_in.iter_mut() {
                *x = hex(&mut t)? as u64;
            }
            for v in v_in.iter_mut() {
                *v = hex(&mut t)?;
            }
            if t.next() != Some("=>") {
                return Err("missing =>".into());
            }
            let nzcv_out = hex(&mut t)? as u32;
            let fpsr_out = hex(&mut t)? as u32;
            for x in x_out.iter_mut() {
                *x = hex(&mut t)? as u64;
            }
            for v in v_out.iter_mut() {
                *v = hex(&mut t)?;
            }
            Ok(Line::Test(Box::new(Case { word, nzcv_in, x_in, v_in, nzcv_out, fpsr_out, x_out, v_out })))
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

fn fresh(word: u32, nzcv: u32, x: &[u64; 31], v: &[u128; 32]) -> (Cpu, Memory) {
    let mut mmu = Memory::new();
    mmu.map_range(CODE, 0x1000, Permission::READ_WRITE_EXECUTE);
    mmu.write_u32(CODE, word).expect("code page mapped");
    let mut cpu = Cpu::new();
    cpu.regs.pc = CODE;
    cpu.regs.x = *x;
    cpu.regs.v = *v;
    cpu.regs.pstate.from_bits(nzcv << 28);
    (cpu, mmu)
}

/// Replays one case in the interpreter. The second `Ran` field is the FPSR the interpreter produced.
pub fn replay(c: &Case) -> Outcome {
    let (mut cpu, mut mmu) = fresh(c.word, c.nzcv_in, &c.x_in, &c.v_in);
    match cpu.step(&mut mmu) {
        Ok(StepResult::Continue) => {}
        Ok(other) => return Outcome::OtherError(format!("{other:?}")),
        Err(crate::cpu::CpuError::UnimplementedInstruction { .. }) => return Outcome::Unimplemented,
        Err(e) => return Outcome::OtherError(e.to_string()),
    }
    let mut diffs = Vec::new();
    for i in 0..31 {
        // x18 is reserved by macOS and cleared by the kernel at will, so the host value is meaningless
        if i != 18 && cpu.regs.x[i] != c.x_out[i] {
            diffs.push(format!("x{i}: host {:#018x} emu {:#018x}", c.x_out[i], cpu.regs.x[i]));
        }
    }
    for i in 0..32 {
        if cpu.regs.v[i] != c.v_out[i] {
            diffs.push(format!("v{i}: host {:#034x} emu {:#034x}", c.v_out[i], cpu.regs.v[i]));
        }
    }
    let nzcv = cpu.regs.pstate.to_bits() >> 28;
    if nzcv != c.nzcv_out {
        diffs.push(format!("nzcv: host {:#x} emu {nzcv:#x}", c.nzcv_out));
    }
    if cpu.regs.fpsr & QC != c.fpsr_out & QC {
        diffs.push(format!("fpsr.QC: host {} emu {}", (c.fpsr_out & QC) >> 27, (cpu.regs.fpsr & QC) >> 27));
    }
    Outcome::Ran(diffs, cpu.regs.fpsr)
}

/// Does the interpreter run a word the host calls UNDEFINED?
fn runs_undefined(word: u32) -> bool {
    let (mut cpu, mut mmu) = fresh(word, 0, &[0; 31], &[0; 32]);
    matches!(cpu.step(&mut mmu), Ok(StepResult::Continue))
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
    let mut input = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--show" => show = it.next().and_then(|v| v.parse().ok()).unwrap_or(20),
            _ => input = Some(a.clone()),
        }
    }
    let Some(input) = input else {
        eprintln!("usage: fuzz-replay [--show N] <trace-file|->");
        return 2;
    };
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
    let mut t = Tally::default();
    let mut mism_classes: BTreeMap<u32, (u64, u32)> = BTreeMap::new();
    let mut unimpl_classes: BTreeMap<u32, (u64, u32)> = BTreeMap::new();
    let mut accepts: BTreeMap<u32, (u64, u32)> = BTreeMap::new();
    let mut shown = 0;
    let mut ran_classes = std::collections::BTreeSet::new();
    for (n, line) in reader.lines().enumerate() {
        let Ok(line) = line else { break };
        let parsed = match parse_line(&line) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("line {}: {e}", n + 1);
                return 2;
            }
        };
        match parsed {
            Line::Undefined(w) => {
                t.host_undefined += 1;
                if runs_undefined(w) {
                    t.emu_runs_undefined += 1;
                    accepts.entry(class(w)).or_insert((0, w)).0 += 1;
                }
            }
            Line::Test(c) => match replay(&c) {
                Outcome::Unimplemented => {
                    t.unimplemented += 1;
                    unimpl_classes.entry(class(c.word)).or_insert((0, c.word)).0 += 1;
                }
                Outcome::OtherError(e) => {
                    t.other_errors += 1;
                    if shown < show {
                        shown += 1;
                        println!("ERROR {:08x}: {e}", c.word);
                    }
                }
                Outcome::Ran(diffs, fpsr) => {
                    t.executed += 1;
                    ran_classes.insert(class(c.word));
                    let exc_differs = fpsr & EXC != c.fpsr_out & EXC;
                    if diffs.is_empty() {
                        t.matched += 1;
                        if exc_differs {
                            t.fpsr_exc_only += 1;
                        }
                    } else {
                        t.value_mismatch += 1;
                        if diffs.iter().any(|d| d.starts_with("fpsr.QC")) {
                            t.qc_mismatch += 1;
                        }
                        mism_classes.entry(class(c.word)).or_insert((0, c.word)).0 += 1;
                        if shown < show {
                            shown += 1;
                            println!("MISMATCH {:08x} nzcv_in={:x}", c.word, c.nzcv_in);
                            for d in diffs.iter().take(6) {
                                println!("    {d}");
                            }
                        }
                    }
                }
            },
        }
    }
    println!("--- fuzz-replay summary ---");
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
    let dump = |title: &str, m: &BTreeMap<u32, (u64, u32)>| {
        if m.is_empty() {
            return;
        }
        println!("{title} (class = word & 0xFFE0FC00: count, example):");
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        for (k, (cnt, ex)) in v.iter().take(60) {
            println!("  {k:#010x} {cnt:>6} {ex:#010x}");
        }
    };
    dump("mismatching classes", &mism_classes);
    dump("unimplemented classes", &unimpl_classes);
    dump("classes the emulator runs but the host rejects", &accepts);
    if t.value_mismatch + t.other_errors > 0 {
        1
    } else {
        0
    }
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
    fn replay_matches_a_known_good_case() {
        // fadd s0, s1, s2 : 1.0 + 2.0 = 3.0 (upper lanes cleared)
        let l = line(0x1e22_2820, &[(1, 0x3f80_0000), (2, 0x4000_0000)], &[(0, 0x4040_0000), (1, 0x3f80_0000), (2, 0x4000_0000)], 0, 0, 0);
        assert_eq!(replay(&case(&l)), Outcome::Ran(vec![], 0));
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
    fn unimplemented_words_are_reported_as_such() {
        let l = line(0x0000_0000, &[], &[], 0, 0, 0);
        assert_eq!(replay(&case(&l)), Outcome::Unimplemented);
    }
}
