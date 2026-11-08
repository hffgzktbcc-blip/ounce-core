//! Decoder census: walks the executable segment of NRO / AArch64 ELF files as 32-bit words,
//! runs `cpu::decoder::decode`, and ranks the words that decode to `Instruction::Unimplemented`
//! by encoding class.
//!
//! Caveat: text segments contain data (literal pools, jump tables, padding), so some
//! "undecoded" words are false positives. The report header says so.

use crate::cpu::decoder::decode;
use crate::cpu::instructions::Instruction;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Default class mask: keep bits 31..21 and 15..10, mask Rd/Rn (4..0, 9..5) and Rm/imm (20..16).
pub const DEFAULT_MASK: u32 = 0xFFE0_FC00;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub key: u32,
    pub count: u64,
    pub example: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Census {
    pub total: u64,
    pub decoded: u64,
    pub classes: Vec<Class>,
}

/// Counts decodable and undecodable words; classes are sorted by count descending, then key.
pub fn census_words(words: &[u32], mask: u32) -> Census {
    let mut map: HashMap<u32, (u64, u32)> = HashMap::new();
    let mut decoded = 0u64;
    for &w in words {
        if matches!(decode(w), Instruction::Unimplemented { .. }) {
            map.entry(w & mask).or_insert((0, w)).0 += 1;
        } else {
            decoded += 1;
        }
    }
    let mut classes: Vec<Class> = map
        .into_iter()
        .map(|(key, (count, example))| Class { key, count, example })
        .collect();
    classes.sort_by(|a, b| b.count.cmp(&a.count).then(a.key.cmp(&b.key)));
    Census { total: words.len() as u64, decoded, classes }
}

/// Merges `other` into `acc` (same mask assumed); keeps the first example seen.
pub fn merge(acc: &mut Census, other: Census) {
    acc.total += other.total;
    acc.decoded += other.decoded;
    for c in other.classes {
        match acc.classes.iter_mut().find(|x| x.key == c.key) {
            Some(x) => x.count += c.count,
            None => acc.classes.push(c),
        }
    }
    acc.classes.sort_by(|a, b| b.count.cmp(&a.count).then(a.key.cmp(&b.key)));
}

fn rd_u32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o.checked_add(4)?)?.try_into().ok()?))
}
fn rd_u64(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o.checked_add(8)?)?.try_into().ok()?))
}
fn rd_u16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o.checked_add(2)?)?.try_into().ok()?))
}

fn words_of(bytes: &[u8]) -> Vec<u32> {
    bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// Extracts the executable words of an NRO (text segment) or a 64-bit little-endian AArch64 ELF
/// (all PF_X PT_LOAD segments). Returns Err with a reason for anything else.
pub fn extract_text_words(bytes: &[u8]) -> Result<Vec<u32>, String> {
    if bytes.len() >= 0x14 && &bytes[0x10..0x14] == b"NRO0" {
        let off = rd_u32(bytes, 0x20).ok_or("short NRO")? as usize;
        let size = rd_u32(bytes, 0x24).ok_or("short NRO")? as usize;
        let end = off.checked_add(size).ok_or("bad text range")?;
        let text = bytes.get(off..end).ok_or("text segment outside file")?;
        return Ok(words_of(text));
    }
    if bytes.len() >= 0x40 && &bytes[0..4] == b"\x7fELF" {
        if bytes[4] != 2 || bytes[5] != 1 {
            return Err("not a 64-bit little-endian ELF".into());
        }
        if rd_u16(bytes, 0x12) != Some(183) {
            return Err("ELF machine is not AArch64".into());
        }
        let phoff = rd_u64(bytes, 0x20).ok_or("short ELF")? as usize;
        let phentsize = rd_u16(bytes, 0x36).ok_or("short ELF")? as usize;
        let phnum = rd_u16(bytes, 0x38).ok_or("short ELF")? as usize;
        let mut out = Vec::new();
        for i in 0..phnum {
            let p = phoff.saturating_add(i.saturating_mul(phentsize));
            let (Some(ty), Some(flags), Some(foff), Some(fsz)) =
                (rd_u32(bytes, p), rd_u32(bytes, p + 4), rd_u64(bytes, p + 8), rd_u64(bytes, p + 32))
            else {
                return Err("truncated program header".into());
            };
            if ty == 1 && flags & 1 != 0 {
                let end = (foff as usize).checked_add(fsz as usize).ok_or("bad segment")?;
                let seg = bytes.get(foff as usize..end).ok_or("segment outside file")?;
                out.extend(words_of(seg));
            }
        }
        return Ok(out);
    }
    Err("neither NRO0 nor AArch64 ELF".into())
}

fn is_candidate(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()), Some("nro") | Some("elf"))
}

/// Expands files and directories (recursively) into a sorted list of files. Files named
/// explicitly are always kept; files found inside directories must be .nro or .elf.
pub fn collect_files(inputs: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = inputs.iter().map(PathBuf::from).collect();
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
        } else if p.is_file() && (inputs.iter().any(|i| Path::new(i) == p) || is_candidate(&p)) {
            out.push(p);
        }
    }
    out.sort();
    out
}

pub fn render_text(c: &Census, mask: u32, min_count: u64, top: usize, files: usize) -> String {
    let undecoded = c.total - c.decoded;
    let mut s = String::new();
    s.push_str("Decoder census (NOTE: text segments also hold data such as literal pools and\n");
    s.push_str("jump tables, so some undecoded words are false positives, not instructions)\n");
    s.push_str(&format!(
        "files: {files}\nwords: {}\ndecoded: {}\nundecoded: {} ({:.3}%)\n",
        c.total,
        c.decoded,
        undecoded,
        if c.total == 0 { 0.0 } else { undecoded as f64 * 100.0 / c.total as f64 }
    ));
    s.push_str(&format!("class key = word & {mask:#010x} (Rd/Rn and bits 20..16 masked)\n"));
    s.push_str("rank  count   class       example\n");
    for (i, k) in c.classes.iter().filter(|k| k.count >= min_count).take(top).enumerate() {
        s.push_str(&format!("{:>4}  {:>6}  {:#010x}  {:#010x}\n", i + 1, k.count, k.key, k.example));
    }
    s
}

pub fn render_json(c: &Census, mask: u32, min_count: u64, top: usize, files: usize) -> String {
    let rows: Vec<String> = c
        .classes
        .iter()
        .filter(|k| k.count >= min_count)
        .take(top)
        .map(|k| {
            format!("{{\"class\":\"{:#010x}\",\"count\":{},\"example\":\"{:#010x}\"}}", k.key, k.count, k.example)
        })
        .collect();
    format!(
        "{{\"files\":{files},\"mask\":\"{mask:#010x}\",\"total\":{},\"decoded\":{},\"undecoded\":{},\"classes\":[{}]}}",
        c.total,
        c.decoded,
        c.total - c.decoded,
        rows.join(",")
    )
}

/// CLI entry: `census [--min-count N] [--json] [--top N] [--mask HEX] <file-or-dir>...`
pub fn run(args: &[String]) -> i32 {
    let (mut min_count, mut json, mut top, mut mask) = (1u64, false, usize::MAX, DEFAULT_MASK);
    let mut inputs = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--min-count" => min_count = it.next().and_then(|v| v.parse().ok()).unwrap_or(1),
            "--top" => top = it.next().and_then(|v| v.parse().ok()).unwrap_or(usize::MAX),
            "--mask" => {
                let v = it.next().map(|v| v.trim_start_matches("0x"));
                mask = v.and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(DEFAULT_MASK);
            }
            _ => inputs.push(a.clone()),
        }
    }
    if inputs.is_empty() {
        eprintln!("usage: census [--min-count N] [--json] [--top N] [--mask HEX] <file-or-dir>...");
        return 2;
    }
    let files = collect_files(&inputs);
    let mut acc = Census::default();
    let mut n = 0;
    for f in &files {
        match std::fs::read(f).map_err(|e| e.to_string()).and_then(|b| extract_text_words(&b)) {
            Ok(w) => {
                merge(&mut acc, census_words(&w, mask));
                n += 1;
            }
            Err(e) => eprintln!("skip {}: {e}", f.display()),
        }
    }
    let out = if json {
        render_json(&acc, mask, min_count, top, n)
    } else {
        render_text(&acc, mask, min_count, top, n)
    };
    println!("{out}");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_known_words() {
        // NOP and RET decode; 0x00000000 and 0xFFFFFFFF do not.
        let w = [0xD503_201F, 0xD65F_03C0, 0, 0, 0xFFFF_FFFF];
        let c = census_words(&w, DEFAULT_MASK);
        assert_eq!(c.total, 5);
        assert_eq!(c.decoded, 2);
        assert_eq!(c.classes[0], Class { key: 0, count: 2, example: 0 });
        assert_eq!(c.classes[1].key, 0xFFFF_FFFF & DEFAULT_MASK);
    }

    #[test]
    fn register_fields_are_masked() {
        let c = census_words(&[0xFFFF_FFE0, 0xFFF5_FFE7], DEFAULT_MASK);
        assert_eq!(c.classes.len(), 1);
        assert_eq!(c.classes[0].count, 2);
    }

    #[test]
    fn extracts_nro_text() {
        let mut f = vec![0u8; 0x88];
        f[0x10..0x14].copy_from_slice(b"NRO0");
        f[0x20..0x24].copy_from_slice(&0x80u32.to_le_bytes());
        f[0x24..0x28].copy_from_slice(&8u32.to_le_bytes());
        f[0x80..0x84].copy_from_slice(&0xD503_201Fu32.to_le_bytes());
        assert_eq!(extract_text_words(&f).unwrap(), vec![0xD503_201F, 0]);
        assert!(extract_text_words(b"garbage").is_err());
    }
}
