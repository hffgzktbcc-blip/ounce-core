//! ELF64 little-endian AArch64 `ET_EXEC` loader and initial process stack.

use crate::mmu::{Memory, Permission};
use std::collections::BTreeMap;

pub const PAGE: u64 = 4096;
/// Top of the user stack (exclusive). Stack grows down from here.
pub const STACK_TOP: u64 = 0x0000_7fff_0000_0000;
pub const STACK_SIZE: u64 = 2 * 1024 * 1024;
/// Anonymous `mmap` allocations are carved upward from here; ELF segments must end below it.
pub const MMAP_BASE: u64 = 0x0000_4000_0000_0000;
const MAX_LOAD_BYTES: u64 = 1 << 30;

const EM_AARCH64: u16 = 183;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const PF_X: u32 = 1;

const AT_NULL: u64 = 0;
const AT_PHDR: u64 = 3;
const AT_PHENT: u64 = 4;
const AT_PHNUM: u64 = 5;
const AT_PAGESZ: u64 = 6;
const AT_ENTRY: u64 = 9;
const AT_UID: u64 = 11;
const AT_EUID: u64 = 12;
const AT_GID: u64 = 13;
const AT_EGID: u64 = 14;
const AT_PLATFORM: u64 = 15;
const AT_HWCAP: u64 = 16;
const AT_CLKTCK: u64 = 17;
const AT_SECURE: u64 = 23;
const AT_RANDOM: u64 = 25;
const AT_EXECFN: u64 = 31;

#[derive(Debug, thiserror::Error)]
pub enum ElfError {
    #[error("not an ELF file (bad magic or file too short)")]
    NotElf,
    #[error("unsupported ELF: {0}")]
    Unsupported(String),
    #[error("malformed ELF: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone)]
pub struct Image {
    pub entry: u64,
    /// First page-aligned address after the highest loaded segment (initial program break).
    pub brk_start: u64,
    pub phdr_addr: u64,
    pub phnum: u64,
    pub phentsize: u64,
    pub loaded_bytes: u64,
}

fn rd16(b: &[u8], o: usize) -> Result<u16, ElfError> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| ElfError::Malformed("header past end of file".into()))
}
fn rd32(b: &[u8], o: usize) -> Result<u32, ElfError> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| ElfError::Malformed("header past end of file".into()))
}
fn rd64(b: &[u8], o: usize) -> Result<u64, ElfError> {
    b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| ElfError::Malformed("header past end of file".into()))
}

pub fn align_up(v: u64) -> u64 {
    (v + PAGE - 1) & !(PAGE - 1)
}

/// Parses `bytes` and maps its `PT_LOAD` segments into `mmu`.
///
/// Mapping limit: the memory model has no per-page permission change, so segment pages are mapped RW,
/// or RWX when any segment touching the page is executable. Read-only data is not write-protected.
pub fn load(bytes: &[u8], mmu: &mut Memory) -> Result<Image, ElfError> {
    if bytes.len() < 64 || &bytes[0..4] != b"\x7fELF" {
        return Err(ElfError::NotElf);
    }
    if bytes[4] != 2 {
        return Err(ElfError::Unsupported("32-bit ELF (EI_CLASS != ELFCLASS64)".into()));
    }
    if bytes[5] != 1 {
        return Err(ElfError::Unsupported("big-endian ELF".into()));
    }
    let e_type = rd16(bytes, 16)?;
    let e_machine = rd16(bytes, 18)?;
    if e_machine != EM_AARCH64 {
        return Err(ElfError::Unsupported(format!("e_machine {e_machine} is not AArch64 (183)")));
    }
    match e_type {
        ET_EXEC => {}
        ET_DYN => {
            return Err(ElfError::Unsupported(
                "ET_DYN (dynamic executable, shared object or static-PIE); link with -static -no-pie".into(),
            ))
        }
        t => return Err(ElfError::Unsupported(format!("e_type {t} is not ET_EXEC"))),
    }
    let entry = rd64(bytes, 24)?;
    let phoff = rd64(bytes, 32)?;
    let phentsize = rd16(bytes, 54)? as u64;
    let phnum = rd16(bytes, 56)? as u64;
    if phentsize != 56 {
        return Err(ElfError::Malformed(format!("e_phentsize {phentsize}, expected 56")));
    }
    if phnum == 0 || phnum > 128 {
        return Err(ElfError::Malformed(format!("e_phnum {phnum} out of range 1..=128")));
    }
    struct Seg {
        offset: u64,
        vaddr: u64,
        filesz: u64,
        memsz: u64,
        exec: bool,
    }
    let mut segs = Vec::new();
    for i in 0..phnum {
        let o = phoff
            .checked_add(i * 56)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| ElfError::Malformed("program header offset overflow".into()))?;
        let p_type = rd32(bytes, o)?;
        let flags = rd32(bytes, o + 4)?;
        let offset = rd64(bytes, o + 8)?;
        let vaddr = rd64(bytes, o + 16)?;
        let filesz = rd64(bytes, o + 32)?;
        let memsz = rd64(bytes, o + 40)?;
        match p_type {
            PT_INTERP => return Err(ElfError::Unsupported("PT_INTERP: dynamically linked executable".into())),
            PT_DYNAMIC => return Err(ElfError::Unsupported("PT_DYNAMIC: dynamically linked executable".into())),
            PT_LOAD => {
                if filesz > memsz {
                    return Err(ElfError::Malformed("PT_LOAD p_filesz > p_memsz".into()));
                }
                let end_file = offset.checked_add(filesz).ok_or_else(|| ElfError::Malformed("p_offset+p_filesz overflow".into()))?;
                if end_file > bytes.len() as u64 {
                    return Err(ElfError::Malformed("PT_LOAD file range past end of file".into()));
                }
                let end = vaddr.checked_add(memsz).ok_or_else(|| ElfError::Malformed("p_vaddr+p_memsz overflow".into()))?;
                if memsz == 0 {
                    continue;
                }
                if vaddr < PAGE || end > MMAP_BASE {
                    return Err(ElfError::Unsupported(format!("segment {vaddr:#x}..{end:#x} outside the loadable window {PAGE:#x}..{MMAP_BASE:#x}")));
                }
                segs.push(Seg { offset, vaddr, filesz, memsz, exec: flags & PF_X != 0 });
            }
            _ => {}
        }
    }
    if segs.is_empty() {
        return Err(ElfError::Malformed("no PT_LOAD segments".into()));
    }
    let mut pages: BTreeMap<u64, bool> = BTreeMap::new();
    for s in &segs {
        let first = s.vaddr & !(PAGE - 1);
        let last = align_up(s.vaddr + s.memsz);
        let mut p = first;
        while p < last {
            let e = pages.entry(p).or_insert(false);
            *e |= s.exec;
            p += PAGE;
        }
    }
    let loaded_bytes = pages.len() as u64 * PAGE;
    if loaded_bytes > MAX_LOAD_BYTES {
        return Err(ElfError::Unsupported(format!("{loaded_bytes} bytes to map exceeds the {MAX_LOAD_BYTES} byte limit")));
    }
    for (&p, &exec) in &pages {
        mmu.map_range(p, PAGE as usize, if exec { Permission::READ_WRITE_EXECUTE } else { Permission::READ_WRITE });
    }
    for s in &segs {
        let data = &bytes[s.offset as usize..(s.offset + s.filesz) as usize];
        mmu.write_bytes(s.vaddr, data).map_err(|e| ElfError::Malformed(format!("mapping segment: {e}")))?;
    }
    if !segs.iter().any(|s| s.exec && entry >= s.vaddr && entry < s.vaddr + s.memsz) {
        return Err(ElfError::Malformed(format!("entry point {entry:#x} is not inside an executable segment")));
    }
    let brk_start = pages.keys().next_back().map(|p| p + PAGE).unwrap();
    // AT_PHDR: address where the program headers are visible in memory, if a segment covers them.
    let phdr_addr = segs
        .iter()
        .find(|s| phoff >= s.offset && phoff + phnum * 56 <= s.offset + s.filesz)
        .map(|s| s.vaddr + (phoff - s.offset))
        .unwrap_or(0);
    Ok(Image { entry, brk_start, phdr_addr, phnum, phentsize, loaded_bytes })
}

/// Maps the stack and builds argc/argv/envp/auxv. Returns the initial `sp` (16-byte aligned).
pub fn build_stack(mmu: &mut Memory, image: &Image, argv: &[String], envp: &[String], execfn: &str) -> Result<u64, ElfError> {
    mmu.map_range(STACK_TOP - STACK_SIZE, STACK_SIZE as usize, Permission::READ_WRITE);
    let mut sp = STACK_TOP - 16;
    let push_bytes = |mmu: &mut Memory, sp: &mut u64, data: &[u8]| -> Result<u64, ElfError> {
        *sp -= data.len() as u64;
        mmu.write_bytes(*sp, data).map_err(|e| ElfError::Malformed(format!("stack write: {e}")))?;
        Ok(*sp)
    };
    let strings_size: usize = argv.iter().chain(envp).map(|s| s.len() + 1).sum::<usize>() + execfn.len() + 1 + 64;
    if strings_size > (STACK_SIZE / 2) as usize {
        return Err(ElfError::Unsupported("argv/envp too large for the 2 MiB stack".into()));
    }
    let execfn_p = push_bytes(mmu, &mut sp, &[execfn.as_bytes(), b"\0"].concat())?;
    let platform_p = push_bytes(mmu, &mut sp, b"aarch64\0")?;
    // Fixed bytes: runs are deterministic, so AT_RANDOM is not random.
    let random_p = push_bytes(mmu, &mut sp, &[0x5au8; 16])?;
    let mut env_ptrs = Vec::new();
    for s in envp.iter().rev() {
        env_ptrs.push(push_bytes(mmu, &mut sp, &[s.as_bytes(), b"\0"].concat())?);
    }
    env_ptrs.reverse();
    let mut arg_ptrs = Vec::new();
    for s in argv.iter().rev() {
        arg_ptrs.push(push_bytes(mmu, &mut sp, &[s.as_bytes(), b"\0"].concat())?);
    }
    arg_ptrs.reverse();
    sp &= !15;
    let auxv: Vec<(u64, u64)> = vec![
        (AT_PHDR, image.phdr_addr),
        (AT_PHENT, image.phentsize),
        (AT_PHNUM, image.phnum),
        (AT_PAGESZ, PAGE),
        (AT_ENTRY, image.entry),
        (AT_UID, 1000),
        (AT_EUID, 1000),
        (AT_GID, 1000),
        (AT_EGID, 1000),
        (AT_HWCAP, 0x3), // HWCAP_FP | HWCAP_ASIMD
        (AT_CLKTCK, 100),
        (AT_SECURE, 0),
        (AT_RANDOM, random_p),
        (AT_PLATFORM, platform_p),
        (AT_EXECFN, execfn_p),
        (AT_NULL, 0),
    ];
    let mut words: Vec<u64> = vec![argv.len() as u64];
    words.extend(&arg_ptrs);
    words.push(0);
    words.extend(&env_ptrs);
    words.push(0);
    for (k, v) in auxv {
        words.push(k);
        words.push(v);
    }
    let total = words.len() as u64 * 8;
    let new_sp = (sp - total) & !15;
    for (i, w) in words.iter().enumerate() {
        mmu.write_u64(new_sp + i as u64 * 8, *w).map_err(|e| ElfError::Malformed(format!("stack write: {e}")))?;
    }
    Ok(new_sp)
}
