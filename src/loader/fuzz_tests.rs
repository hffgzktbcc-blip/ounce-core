//! Bounded fuzz tests for the parsers that read untrusted guest files: NRO, NSO, ELF `PT_TLS`, the
//! MOD0 dynamic section (RELA/REL/RELR, symbols, hash tables, init/fini arrays, TLS), PFS0, HFS0,
//! XCI, NCA, NPDM and RomFS. Malformed input must return an error or a partial result, never panic,
//! hang or try to allocate the address space.
//!
//! Every test mutates valid seeds with a fixed-seed PRNG, so a failure names a case that reproduces.
//! `OUNCE_FUZZ_CASES=<n>` runs more cases per target (default 400, enough for CI).

use super::nca::{Hfs0Archive, NcaImage, Pfs0Archive, XciCartridge, SYNTHETIC_TEST_KEY};
use super::nro::{build_linked_nso, NroLoader};
use super::package::NpdmDescriptor;
use crate::fs::RomFsArchive;
use crate::mmu::Memory;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// SplitMix64: small, fast, and good enough to drive mutations.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// Mostly boundary values (0, 1, sign bits, all ones, page sizes), sometimes fully random.
    pub(crate) fn interesting_u64(&mut self) -> u64 {
        const VALUES: [u64; 16] = [
            0, 1, 2, 7, 8, 0x10, 0x18, 0x1000, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFF,
            0x1_0000_0000, 0x7FFF_FFFF_FFFF_FFFF, 0x8000_0000_0000_0000, u64::MAX - 7, u64::MAX,
        ];
        match self.below(4) {
            0 => self.next(),
            1 => self.below(0x4000),
            _ => VALUES[self.below(VALUES.len() as u64) as usize],
        }
    }
}

/// Cases per target: `OUNCE_FUZZ_CASES` or 400.
pub(crate) fn cases() -> u64 {
    std::env::var("OUNCE_FUZZ_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(400)
}

/// A single case may take this long before it counts as a hang.
const CASE_BUDGET: Duration = Duration::from_secs(5);

/// Applies 1 to 8 random edits to a copy of `seed`.
pub(crate) fn mutate(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut out = seed.to_vec();
    for _ in 0..1 + rng.below(8) {
        let len = out.len() as u64;
        match rng.below(7) {
            0 if len > 0 => {
                let i = rng.below(len) as usize;
                out[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len) as usize;
                out[i] = rng.next() as u8;
            }
            2 if len >= 4 => {
                let i = (rng.below(len - 3) & !3) as usize;
                out[i..i + 4].copy_from_slice(&(rng.interesting_u64() as u32).to_le_bytes());
            }
            3 if len >= 8 => {
                let i = (rng.below(len - 7) & !7) as usize;
                out[i..i + 8].copy_from_slice(&rng.interesting_u64().to_le_bytes());
            }
            4 if len > 0 => out.truncate(rng.below(len) as usize),
            5 => {
                let extra = rng.below(64) as usize;
                out.extend((0..extra).map(|_| rng.next() as u8));
            }
            _ if len >= 8 => {
                // Copy a chunk over another place (duplicates headers, table entries, names).
                let n = 1 + rng.below(len.min(64)) as usize;
                let src = rng.below(len - n as u64 + 1) as usize;
                let dst = rng.below(len - n as u64 + 1) as usize;
                let chunk = out[src..src + n].to_vec();
                out[dst..dst + n].copy_from_slice(&chunk);
            }
            _ => {}
        }
    }
    out
}

/// Feeds `cases()` mutated inputs (plus each unmodified seed and some pure noise) to `target`.
/// Panics with the target name, case number and input size if `target` panics or runs over budget.
pub(crate) fn fuzz(name: &str, seed: u64, seeds: &[Vec<u8>], target: impl Fn(&[u8])) {
    let mut rng = Rng::new(seed);
    let total = cases();
    let trace = std::env::var_os("OUNCE_FUZZ_TRACE").is_some();
    for case in 0..total + seeds.len() as u64 {
        let input = if let Some(s) = seeds.get(case as usize) {
            s.clone()
        } else if rng.below(16) == 0 {
            (0..rng.below(0x400)).map(|_| rng.next() as u8).collect()
        } else {
            let s = &seeds[rng.below(seeds.len() as u64) as usize];
            mutate(&mut rng, s)
        };
        if trace {
            eprintln!("{name}: case {case} ({} bytes)", input.len());
        }
        let start = Instant::now();
        if catch_unwind(AssertUnwindSafe(|| target(&input))).is_err() {
            panic!("{name}: case {case} panicked (input {} bytes, fuzz seed {seed:#x})", input.len());
        }
        let elapsed = start.elapsed();
        assert!(elapsed < CASE_BUDGET, "{name}: case {case} took {elapsed:?} (input {} bytes)", input.len());
    }
}

const BASE: u64 = 0x0800_0000;

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("fixture {path}: {e}"))
}

/// A MOD0 module with exports, imports, RELATIVE/ABS64/GLOB_DAT/JUMP_SLOT relocations and a TLS symbol.
fn linked_nso_seed() -> Vec<u8> {
    build_linked_nso(
        &[("", 0, 0, 0), ("exported_fn", 1, 1, 0x40), ("imported_fn", 0, 1, 0), ("weak_fn", 0, 2, 0)],
        &[(0x1800, 1027, 0, 0x100), (0x1808, 257, 1, 4), (0x1810, 1025, 2, 0)],
        &[(0x1818, 1026, 2, 0), (0x1820, 1026, 3, 0)],
    )
}

/// Every pass the loader makes over a mapped module's dynamic section.
fn walk_dynamic(mmu: &mut Memory) {
    let _ = NroLoader::parse_dynamic(BASE, mmu);
    let exports = NroLoader::collect_exports(BASE, mmu);
    let tls = NroLoader::build_tls_layout(&[BASE], mmu);
    let _ = NroLoader::tls_thread_image(&tls, mmu);
    let mut imports: HashMap<String, u64> = exports;
    imports.insert("imported_fn".into(), 0x1234_0000);
    let _ = NroLoader::apply_relocations_with_tls(BASE, mmu, &imports, &tls);
    let _ = NroLoader::apply_relr(BASE, mmu);
    let _ = NroLoader::init_functions(BASE, mmu, true);
    let _ = NroLoader::fini_functions(BASE, mmu);
}

/// Address of the `.dynamic` table the MOD0 header of the module at `BASE` points to.
fn dynamic_addr(mmu: &Memory) -> Option<u64> {
    let mod0 = BASE.wrapping_add(mmu.read_u32(BASE + 4).ok()? as u64);
    Some(mod0.wrapping_add(mmu.read_u32(mod0.wrapping_add(4)).ok()? as i32 as i64 as u64))
}

#[test]
fn fuzz_nro_header_and_load() {
    let seeds = vec![
        fixture("nro/hello/hello.nro"),
        fixture("nro/romfs/romfs.nro"),
        NroLoader::build_synthetic_nro(&[0x1F, 0x20, 0x03, 0xD5]),
    ];
    fuzz("nro", 0x4E52_4F30, &seeds, |bytes| {
        let _ = NroLoader::parse_header(bytes);
        let mut mmu = Memory::new();
        if NroLoader::load_into_memory(bytes, BASE, &mut mmu).is_ok() {
            walk_dynamic(&mut mmu);
        }
    });
}

#[test]
fn fuzz_nso_load() {
    let seeds = vec![linked_nso_seed(), NroLoader::build_synthetic_nso(&[0x1F, 0x20, 0x03, 0xD5])];
    fuzz("nso", 0x4E53_4F30, &seeds, |bytes| {
        let mut mmu = Memory::new();
        if NroLoader::load_nso_segments(bytes, BASE, &mut mmu).is_ok() {
            walk_dynamic(&mut mmu);
        }
        let _ = NroLoader::load_nso_into_memory(bytes, BASE, &mut Memory::new());
    });
}

/// Byte mutations rarely land on the dynamic table, so this target rewrites its entries directly:
/// random tags from the set the loader reads, with boundary values (huge sizes, wrapping offsets).
#[test]
fn fuzz_dynamic_section_entries() {
    const TAGS: [u64; 24] = [1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 17, 18, 19, 20, 23, 25, 26, 27, 28, 32, 33, 35];
    let seeds = vec![linked_nso_seed(), fixture("nro/hello/hello.nro")];
    let mut rng = Rng::new(0x0D17_A11C);
    for case in 0..cases() {
        let seed = &seeds[rng.below(seeds.len() as u64) as usize];
        let mut mmu = Memory::new();
        let loaded = if seed.starts_with(b"NSO0") {
            NroLoader::load_nso_segments(seed, BASE, &mut mmu).is_ok()
        } else {
            NroLoader::load_into_memory(seed, BASE, &mut mmu).is_ok()
        };
        assert!(loaded, "seed must load");
        let dynamic = dynamic_addr(&mmu).expect("seed has MOD0 and .dynamic");
        let entries = (0..64).take_while(|i| mmu.read_u64(dynamic + i * 16).is_ok_and(|t| t != 0)).count() as u64;
        for _ in 0..1 + rng.below(6) {
            let entry = dynamic + rng.below(entries + 1) * 16;
            match rng.below(3) {
                0 => mmu.write_u64(entry + 8, rng.interesting_u64()),
                1 => mmu.write_u64(entry, TAGS[rng.below(TAGS.len() as u64) as usize]),
                _ => mmu
                    .write_u64(entry, 36 + rng.below(2)) // DT_RELR / DT_RELRSZ
                    .and_then(|_| mmu.write_u64(entry + 8, rng.interesting_u64())),
            }
            .unwrap();
        }
        // Also corrupt whatever the table points at: symbols, hash tables, relocation entries.
        for _ in 0..rng.below(8) {
            let addr = BASE + (rng.below(0x2000) & !7);
            let _ = mmu.write_u64(addr, rng.interesting_u64());
        }
        let start = Instant::now();
        if catch_unwind(AssertUnwindSafe(|| walk_dynamic(&mut mmu))).is_err() {
            panic!("dynamic: case {case} panicked");
        }
        assert!(start.elapsed() < CASE_BUDGET, "dynamic: case {case} took {:?}", start.elapsed());
    }
}

#[test]
fn fuzz_elf_pt_tls() {
    // Minimal ELF64 header plus one PT_TLS program header at 0x40.
    let mut elf = vec![0u8; 0x78];
    elf[0..6].copy_from_slice(b"\x7fELF\x02\x01");
    elf[0x20..0x28].copy_from_slice(&0x40u64.to_le_bytes());
    elf[0x36..0x38].copy_from_slice(&0x38u16.to_le_bytes());
    elf[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes());
    elf[0x40..0x44].copy_from_slice(&7u32.to_le_bytes());
    elf[0x68..0x70].copy_from_slice(&0x10u64.to_le_bytes());
    assert!(NroLoader::parse_pt_tls(&elf).is_some());
    fuzz("elf", 0x7F45_4C46, &[elf], |bytes| {
        if let Some(t) = NroLoader::parse_pt_tls(bytes) {
            let mut mmu = Memory::new();
            let _ = NroLoader::load_nso_segments(&linked_nso_seed(), BASE, &mut mmu);
            let layout = NroLoader::build_tls_layout_with_templates(&[BASE], &mmu, &HashMap::from([(BASE, t)]));
            let _ = NroLoader::tls_thread_image(&layout, &mmu);
        }
    });
}

#[test]
fn fuzz_containers() {
    let files: &[(&str, &[u8])] = &[("main", b"\x1f\x20\x03\xd5"), ("main.npdm", b"META"), ("x", b"")];
    let pfs0 = Pfs0Archive::build(files);
    let hfs0 = Hfs0Archive::build(files);
    let xci = XciCartridge::build_cartridge(files);
    let nca = NcaImage::build_encrypted_with_romfs(
        0x0100_0000_0000_1000,
        &SYNTHETIC_TEST_KEY,
        &[0x11; 16],
        &[0x22; 16],
        &pfs0,
        Some(&RomFsArchive::build(&[("a.txt", b"hello")])),
    );
    fuzz("pfs0", 0x5046_5330, &[pfs0], |b| {
        if let Ok(a) = Pfs0Archive::parse(b.to_vec()) {
            let _ = a.extract_file("main");
        }
    });
    fuzz("hfs0", 0x4846_5330, &[hfs0], |b| {
        if let Ok(a) = Hfs0Archive::parse(b.to_vec()) {
            let _ = a.extract_file("main");
        }
    });
    fuzz("xci", 0x4845_4144, &[xci], |b| {
        if let Ok(x) = XciCartridge::parse(b) {
            let _ = x.open_secure_partition();
        }
    });
    fuzz("nca", 0x4E43_4133, &[nca], |b| {
        let _ = NcaImage::parse_and_decrypt(b, &SYNTHETIC_TEST_KEY);
    });
}

#[test]
fn fuzz_npdm() {
    let seed = NpdmDescriptor::create_synthetic("fuzz", "PROD", 0x0100_0000_0000_2000, 44, 0, &["sm:", "fsp-srv"]);
    assert!(NpdmDescriptor::parse(&seed).is_ok());
    fuzz("npdm", 0x4D45_5441, &[seed], |b| {
        let _ = NpdmDescriptor::parse(b);
    });
}

#[test]
fn fuzz_romfs() {
    let seed = RomFsArchive::build(&[("a.txt", b"hello"), ("dir/b.bin", &[1, 2, 3]), ("c", b"")]);
    assert!(RomFsArchive::parse(&seed).is_ok());
    fuzz("romfs", 0x524F_4D46, &[seed], |b| {
        let _ = RomFsArchive::parse(b);
    });
}
