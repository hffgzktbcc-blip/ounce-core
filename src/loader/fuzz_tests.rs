//! Bounded fuzz tests for the parsers that read untrusted guest files: NRO, NSO, ELF `PT_TLS`, the
//! MOD0 dynamic section (RELA/REL/RELR, symbols, hash tables, init/fini arrays, TLS), NPDM and RomFS. Malformed input must return an error or a partial result, never panic,
//! hang or try to allocate the address space.
//!
//! Every test mutates valid seeds with a fixed-seed PRNG, so a failure names a case that reproduces.
//! `OUNCE_FUZZ_CASES=<n>` runs more cases per target (default 400, enough for CI).

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
    let seeds = [linked_nso_seed(), fixture("nro/hello/hello.nro")];
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

fn linux_elf_fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/linux-elf/bin/{name}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("linux-elf fixture {name}: {e}"))
}

/// Builds a minimal static AArch64 `ET_EXEC` ELF with the given `(type, flags, offset, vaddr, filesz, memsz)`
/// program headers and payload appended at offset `64 + phdrs.len() * 56`.
fn build_synthetic_linux_elf(entry: u64, phdrs: &[(u32, u32, u64, u64, u64, u64)], payload: &[u8]) -> Vec<u8> {
    let phoff = 64u64;
    let ph_bytes = phdrs.len() * 56;
    let mut elf = vec![0u8; 64 + ph_bytes];
    elf[0..6].copy_from_slice(b"\x7fELF\x02\x01");
    elf[6] = 1;
    elf[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    elf[18..20].copy_from_slice(&183u16.to_le_bytes()); // EM_AARCH64
    elf[20..24].copy_from_slice(&1u32.to_le_bytes());
    elf[24..32].copy_from_slice(&entry.to_le_bytes());
    elf[32..40].copy_from_slice(&phoff.to_le_bytes());
    elf[52..54].copy_from_slice(&64u16.to_le_bytes());
    elf[54..56].copy_from_slice(&56u16.to_le_bytes());
    elf[56..58].copy_from_slice(&(phdrs.len() as u16).to_le_bytes());
    for (idx, &(p_type, flags, offset, vaddr, filesz, memsz)) in phdrs.iter().enumerate() {
        let o = 64 + idx * 56;
        elf[o..o + 4].copy_from_slice(&p_type.to_le_bytes());
        elf[o + 4..o + 8].copy_from_slice(&flags.to_le_bytes());
        elf[o + 8..o + 16].copy_from_slice(&offset.to_le_bytes());
        elf[o + 16..o + 24].copy_from_slice(&vaddr.to_le_bytes());
        elf[o + 24..o + 32].copy_from_slice(&vaddr.to_le_bytes());
        elf[o + 32..o + 40].copy_from_slice(&filesz.to_le_bytes());
        elf[o + 40..o + 48].copy_from_slice(&memsz.to_le_bytes());
        elf[o + 48..o + 56].copy_from_slice(&0x1000u64.to_le_bytes());
    }
    elf.extend_from_slice(payload);
    elf
}

fn verify_linux_elf_load(bytes: &[u8]) {
    let mut mmu = Memory::new();
    if let Ok(image) = crate::linux::elf::load(bytes, &mut mmu) {
        assert!(image.loaded_bytes > 0);
        assert!(image.loaded_bytes <= (1 << 30));
        assert!(image.brk_start <= crate::linux::elf::MMAP_BASE);
        let argv = ["fuzz-elf".to_string(), "--flag".to_string()];
        let _ = crate::linux::elf::build_stack(&mut mmu, &image, &argv, &["K=V".to_string()], "fuzz-elf");
    }
}

#[test]
fn fuzz_linux_elf_load() {
    // 1x RX PT_LOAD seed + multi-segment seed with PT_LOAD (RX/RW), PT_TLS, and PT_GNU_STACK.
    let single = build_synthetic_linux_elf(
        0x40_0078,
        &[(1, 5, 0, 0x40_0000, 124, 124)],
        &0xD65F_03C0u32.to_le_bytes(),
    );
    let multi = build_synthetic_linux_elf(
        0x40_0100,
        &[
            (1, 5, 0x100, 0x40_0100, 8, 16),          // RX unaligned start/size
            (1, 6, 0x100, 0x40_0080, 8, 0x2080),      // overlapping RW spanning pages
            (7, 4, 0x100, 0x40_0200, 4, 32),          // PT_TLS
            (0x6474_e551, 6, 0, 0, 0, 0),             // PT_GNU_STACK
        ],
        &[0x1F, 0x20, 0x03, 0xD5, 0xC0, 0x03, 0x5F, 0xD6],
    );
    let mut seeds = vec![single, multi];
    for name in ["hello.elf", "basic_math.elf", "args.elf", "exitcode.elf"] {
        seeds.push(linux_elf_fixture(name));
    }
    for (idx, seed) in seeds.iter().enumerate() {
        assert!(crate::linux::elf::load(seed, &mut Memory::new()).is_ok(), "seed {idx} must load");
    }
    fuzz("linux-elf", 0x454C_4636, &seeds, verify_linux_elf_load);
}

#[test]
fn fuzz_linux_elf_adversarial_headers() {
    let base = build_synthetic_linux_elf(
        0x40_0078,
        &[(1, 5, 0, 0x40_0000, 124, 124)],
        &0xD65F_03C0u32.to_le_bytes(),
    );
    // Regression checks for overflow / O(1) oversized mapping / PT_DYNAMIC / PT_INTERP.
    let mut bad_phoff = base.clone();
    bad_phoff[32..40].copy_from_slice(&(u64::MAX - 2).to_le_bytes());
    assert!(matches!(crate::linux::elf::load(&bad_phoff, &mut Memory::new()), Err(crate::linux::elf::ElfError::Malformed(_))));
    let huge_mem = build_synthetic_linux_elf(
        0x40_0078,
        &[(1, 5, 0, 0x40_0000, 124, 1u64 << 40)],
        &0xD65F_03C0u32.to_le_bytes(),
    );
    assert!(matches!(crate::linux::elf::load(&huge_mem, &mut Memory::new()), Err(crate::linux::elf::ElfError::Unsupported(_))));
    for pt in [2u32, 3u32] {
        let dyn_elf = build_synthetic_linux_elf(
            0x40_0078,
            &[(pt, 4, 0, 0x40_0000, 0, 0), (1, 5, 0, 0x40_0000, 124, 124)],
            &0xD65F_03C0u32.to_le_bytes(),
        );
        assert!(matches!(crate::linux::elf::load(&dyn_elf, &mut Memory::new()), Err(crate::linux::elf::ElfError::Unsupported(_))));
    }
    // All truncations up to base.len() and truncated real fixture.
    for cut in 0..=base.len() {
        verify_linux_elf_load(&base[..cut]);
    }
    let hello = linux_elf_fixture("hello.elf");
    for cut in [0usize, 1, 4, 16, 56, 63, 64, 68, 119, 120, 200] {
        verify_linux_elf_load(&hello[..cut.min(hello.len())]);
    }

    let ph_types: [u32; 6] = [1, 2, 3, 7, 0x6474_e551, 0];
    let huge_sizes: [u64; 9] = [0, 1, 0xFFF, 0x1000, (1 << 30) + 0x1000, 1 << 40, u64::MAX / 2, u64::MAX - 2, u64::MAX];
    let bad_phoffs: [u64; 6] = [0, 64, 65, 0xFFFF, u64::MAX - 56, u64::MAX - 2];
    let mut rng = Rng::new(0xBAD0_E1F6);

    for case in 0..cases() {
        let start = Instant::now();
        let ph_count = (rng.below(4) + 1) as usize;
        let mut phdrs = Vec::with_capacity(ph_count);
        for _ in 0..ph_count {
            let p_type = ph_types[rng.below(ph_types.len() as u64) as usize];
            let p_flags = if rng.below(2) == 0 { 5 } else { rng.next() as u32 };
            let p_offset = if rng.below(3) == 0 { rng.interesting_u64() } else { rng.below(256) };
            let p_vaddr = match rng.below(4) {
                0 => 0x40_0000 + rng.below(0x4000),
                1 => 0x1000 + rng.below(0x1000), // unaligned near min window
                2 => crate::linux::elf::MMAP_BASE.saturating_sub(rng.below(0x2000)),
                _ => rng.interesting_u64(),
            };
            let p_filesz = huge_sizes[rng.below(huge_sizes.len() as u64) as usize];
            let p_memsz = huge_sizes[rng.below(huge_sizes.len() as u64) as usize];
            phdrs.push((p_type, p_flags, p_offset, p_vaddr, p_filesz, p_memsz));
        }
        let mut crafted = build_synthetic_linux_elf(0x40_0078 + rng.below(0x100), &phdrs, &[0xC0, 0x03, 0x5F, 0xD6]);
        match rng.below(5) {
            0 => {
                let phoff = bad_phoffs[rng.below(bad_phoffs.len() as u64) as usize];
                crafted[32..40].copy_from_slice(&phoff.to_le_bytes());
            }
            1 => {
                let phentsize = [0u16, 1, 55, 56, 57, 64, u16::MAX][rng.below(7) as usize];
                crafted[54..56].copy_from_slice(&phentsize.to_le_bytes());
            }
            2 => {
                let phnum = [0u16, 1, 128, 129, 0x7FFF, u16::MAX][rng.below(6) as usize];
                crafted[56..58].copy_from_slice(&phnum.to_le_bytes());
            }
            3 => {
                crafted[16..18].copy_from_slice(&[2u16, 3, 4, u16::MAX][rng.below(4) as usize].to_le_bytes());
                crafted[18..20].copy_from_slice(&[183u16, 62, 0][rng.below(3) as usize].to_le_bytes());
            }
            _ => {}
        }
        if catch_unwind(AssertUnwindSafe(|| verify_linux_elf_load(&crafted))).is_err() {
            panic!("linux-elf adversarial case {case} panicked");
        }
        assert!(start.elapsed() < CASE_BUDGET, "linux-elf adversarial case {case} took {:?}", start.elapsed());
    }
}

/// Builds structured AArch64 opcodes spanning all 16 major `op0` (`[28:25]`) encoding groups and common sub-families.
fn structured_opcode(rng: &mut Rng, step: u64) -> u32 {
    let op0 = ((step + rng.below(16)) & 0xF) as u32;
    let rd = [0u32, 1, 15, 29, 30, 31][rng.below(6) as usize];
    let rn = [0u32, 1, 16, 30, 31][rng.below(5) as usize];
    let rm = [0u32, 2, 17, 30, 31][rng.below(5) as usize];
    let sf = (rng.below(2) as u32) << 31;
    let op = (rng.below(2) as u32) << 30;
    let s = (rng.below(2) as u32) << 29;
    let size = (rng.below(4) as u32) << 22;
    let base = sf | op | s | (op0 << 25) | size | (rm << 16) | (rn << 5) | rd;
    let mix = (rng.next() as u32) & !((0xF << 25) | 0x3FF | (0x1F << 16));
    match step % 6 {
        0 => (op0 << 25) | (rng.next() as u32 & !(0xF << 25)),
        1 => base ^ (mix & 0x01FF_FFFF),
        // AdvSIMD / SHA3 / Scalar FP encodings ([28:25] = 0b0111 / 0b1110 / 0b0110)
        2 => (rng.below(16) as u32) << 28 | (0b0111 << 24) | ((rng.next() as u32) & 0x00FF_FFFF),
        // Branches / System / Exception ([28:26] = 0b101)
        3 => (0b1101_0100 << 24) | ((rng.next() as u32) & 0x00FF_FFFF),
        // Loads / Stores ([27] = 1, [25] = 0)
        4 => ((rng.below(4) as u32) << 30) | (0b111 << 27) | ((rng.next() as u32) & 0x07FF_FFFF),
        // Data processing / PAuth / HINT
        _ => (0xD500_0000 | ((rng.next() as u32) & 0x000F_FFFF)) ^ ((rng.below(2) as u32) * 0x00FF_0000),
    }
}

#[test]
fn fuzz_decoder_and_cpu_step() {
    let mut rng = Rng::new(0xDEC0_DE64);
    let total = cases();

    // 1. Uniform full-32-bit PRNG sweep across the opcode space.
    for case in 0..total * 128 {
        let word = rng.next() as u32;
        if catch_unwind(|| {
            let a = crate::cpu::decoder::decode(word);
            let b = crate::cpu::decoder::decode(word);
            assert_eq!(a, b);
        })
        .is_err()
        {
            panic!("decoder uniform PRNG sweep panicked at case {case}, word {word:#010x}");
        }
    }

    // 2. Structured encoding groups + near-edge perturbations of representative instructions.
    const SEEDS: [u32; 28] = [
        0xD503_201F, 0xD503_245F, 0xD400_0001, 0xD420_0020, 0xD65F_03C0, 0x1400_0000,
        0x8B02_0020, 0xEB02_003F, 0x9AC2_0820, 0x5304_1C20, 0x885F_7C20, 0xF940_03E0,
        0xA9BF_7BFD, 0x1E20_0820, 0x1E22_2020, 0x1F02_0C20, 0x4E22_0C20, 0x4E62_CC20,
        0x4E82_9420, 0xDAC1_0020, 0xDAC1_1020, 0xCE02_0C20, 0xCE82_1020, 0xD53B_D040,
        0xD51B_4420, 0x0000_0000, 0xFFFF_FFFF, 0xD50B_7420,
    ];
    for step in 0..total * 32 {
        let word = structured_opcode(&mut rng, step);
        let _ = crate::cpu::decoder::decode(word);
    }
    for &base in &SEEDS {
        let _ = crate::cpu::decoder::decode(base);
        for bit in 0..32u32 {
            let _ = crate::cpu::decoder::decode(base ^ (1 << bit));
            let _ = crate::cpu::decoder::decode(base ^ (1 << bit) ^ (1 << ((bit + 13) % 32)));
        }
    }

    // 3. Randomized Cpu registers + short isolated Cpu::step execution without panics.
    const CODE_PAGE: u64 = 0x10_0000;
    const DATA_PAGE: u64 = 0x20_0000;
    const DATA_WORDS: [u64; 8] = [
        0,
        1,
        u64::MAX,
        i64::MIN as u64,
        0x7FC0_0000_7FF8_0001,
        0x8000_0000_0000_0001,
        0x3FF0_0000_3F80_0000,
        DATA_PAGE + 0x100,
    ];
    for case in 0..total {
        let start = Instant::now();
        let mut mmu = Memory::new();
        mmu.map_range(CODE_PAGE, 0x1000, crate::mmu::Permission::READ_WRITE_EXECUTE);
        mmu.map_range(DATA_PAGE, 0x1000, crate::mmu::Permission::READ_WRITE);
        for off in (0..0x200u64).step_by(8) {
            let val = DATA_WORDS[rng.below(DATA_WORDS.len() as u64) as usize];
            mmu.write_u64(DATA_PAGE + off, val).unwrap();
        }
        let mut cpu = crate::cpu::Cpu::new();
        cpu.regs.pc = CODE_PAGE;
        cpu.regs.sp = DATA_PAGE + 0x800 + (rng.below(4) * 8);
        cpu.regs.tpidr_el0 = DATA_PAGE + 0x400;
        cpu.regs.tpidrro_el0 = DATA_PAGE + 0x480;
        cpu.regs.fpcr = ((rng.below(4) as u32) << 22) | ((rng.below(2) as u32) << 24) | ((rng.below(2) as u32) << 25);
        cpu.regs.fpsr = (rng.next() as u32) & 0x0800_009F;
        cpu.regs.pstate.from_bits((rng.next() as u32) & 0xF000_0000);
        cpu.regs.pstate.btype = rng.below(4) as u8;
        for i in 0..31 {
            cpu.regs.x[i] = match rng.below(4) {
                0 => DATA_PAGE + (rng.below(64) * 8),
                1 => CODE_PAGE + (rng.below(16) * 4),
                2 => rng.interesting_u64(),
                _ => rng.next(),
            };
        }
        for i in 0..32 {
            let lo = match rng.below(3) {
                0 => DATA_WORDS[rng.below(DATA_WORDS.len() as u64) as usize],
                1 => rng.interesting_u64(),
                _ => rng.next(),
            };
            let hi = rng.next();
            cpu.regs.v[i] = (lo as u128) | ((hi as u128) << 64);
        }
        for slot in 0..4u64 {
            // Bias toward decoded instructions so the interpreter exercises real execution paths.
            let mut word = structured_opcode(&mut rng, case * 4 + slot);
            for attempt in 0..8 {
                if !matches!(crate::cpu::decoder::decode(word), crate::cpu::Instruction::Unimplemented { .. }) {
                    break;
                }
                word = structured_opcode(&mut rng, case * 16 + attempt);
            }
            mmu.write_u32(CODE_PAGE + slot * 4, word).unwrap();
        }
        let res = catch_unwind(AssertUnwindSafe(|| {
            for _ in 0..4 {
                match cpu.step(&mut mmu) {
                    Ok(crate::cpu::StepResult::Continue) => {}
                    _ => break,
                }
            }
        }));
        if res.is_err() {
            panic!("cpu::step panicked in isolated fuzz case {case}");
        }
        assert!(start.elapsed() < CASE_BUDGET, "cpu::step case {case} took {:?}", start.elapsed());
    }
}
