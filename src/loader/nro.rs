#![allow(dead_code)]

use crate::mmu::{Memory, MemoryError, Permission};
use byteorder::{ByteOrder, LittleEndian};
use std::collections::HashMap;

pub const R_AARCH64_RELATIVE: u32 = 1027;
pub const DT_NULL: i64 = 0;
pub const DT_RELA: i64 = 7;
pub const DT_RELASZ: i64 = 8;
pub const DT_HASH: i64 = 4;
pub const DT_PLTRELSZ: i64 = 2;
pub const DT_STRTAB: i64 = 5;
pub const DT_SYMTAB: i64 = 6;
pub const DT_RELAENT: i64 = 9;
pub const DT_SYMENT: i64 = 11;
pub const DT_INIT: i64 = 12;
pub const DT_FINI: i64 = 13;
pub const DT_REL: i64 = 17;
pub const DT_RELSZ: i64 = 18;
pub const DT_RELENT: i64 = 19;
pub const DT_PLTREL: i64 = 20;
pub const DT_JMPREL: i64 = 23;
pub const PT_TLS: u32 = 7;
pub const R_AARCH64_ABS64: u32 = 257;
pub const R_AARCH64_GLOB_DAT: u32 = 1025;
pub const R_AARCH64_JUMP_SLOT: u32 = 1026;
pub const R_AARCH64_TLS_DTPMOD64: u32 = 1028;
pub const R_AARCH64_TLS_DTPREL64: u32 = 1029;
pub const R_AARCH64_TLS_TPREL64: u32 = 1030;
pub const R_AARCH64_TLSDESC: u32 = 1031;
pub const DT_INIT_ARRAY: i64 = 25;
pub const DT_FINI_ARRAY: i64 = 26;
pub const DT_INIT_ARRAYSZ: i64 = 27;
pub const DT_FINI_ARRAYSZ: i64 = 28;
pub const DT_PREINIT_ARRAY: i64 = 32;
pub const DT_PREINIT_ARRAYSZ: i64 = 33;
pub const DT_GNU_HASH: i64 = 0x6FFF_FEF5;
pub const DT_RELRSZ: i64 = 35;
pub const DT_RELR: i64 = 36;
pub const DT_RELRENT: i64 = 37;

/// Size of the AArch64 thread control block that sits at the thread pointer, before the first TLS block.
pub const TLS_TCB_SIZE: u64 = 16;
/// Static TLS blocks are aligned to this. NSO/NRO images carry no `PT_TLS`, so the real alignment is unknown.
pub const TLS_BLOCK_ALIGN: u64 = 16;
/// Guest code for the static TLSDESC resolver: `ldr x0, [x0, #8]; ret`.
pub const TLSDESC_RESOLVER_CODE: [u32; 2] = [0xF940_0400, 0xD65F_03C0];

const RELA_ENTRY_SIZE: u64 = 24;
const REL_ENTRY_SIZE: u64 = 16;
const SYM_ENTRY_SIZE: u64 = 24;
const SHN_UNDEF: u16 = 0;
const STB_LOCAL: u8 = 0;
const STB_WEAK: u8 = 2;
const STT_TLS: u8 = 6;

/// Parsed `.dynamic` section of a MOD0-described module. Pointer values are image-relative offsets.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DynamicInfo {
    pub rela: Option<u64>,
    pub rela_size: u64,
    pub rela_entry_size: u64,
    pub rel: Option<u64>,
    pub rel_size: u64,
    pub rel_entry_size: u64,
    pub jmprel: Option<u64>,
    pub pltrel_size: u64,
    /// `DT_PLTREL`: `DT_REL` or `DT_RELA`, the entry format of `DT_JMPREL`. Absent means `DT_RELA`.
    pub pltrel: Option<u64>,
    pub symtab: Option<u64>,
    pub strtab: Option<u64>,
    pub syment: u64,
    pub hash: Option<u64>,
    pub gnu_hash: Option<u64>,
    pub init: Option<u64>,
    pub fini: Option<u64>,
    pub init_array: Option<u64>,
    pub init_array_size: u64,
    pub fini_array: Option<u64>,
    pub fini_array_size: u64,
    pub preinit_array: Option<u64>,
    pub preinit_array_size: u64,
    /// `DT_RELR`: packed relative relocations (current devkitA64 links libnx homebrew with these).
    pub relr: Option<u64>,
    pub relr_size: u64,
}

/// One module's block in the static TLS area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlsBlock {
    /// 1-based module ID written by `R_AARCH64_TLS_DTPMOD64`.
    pub module_id: u64,
    /// Offset of the block from the thread pointer (`TPIDR_EL0`).
    pub tp_offset: u64,
    /// Block size: `PT_TLS` `p_memsz` when known, otherwise the end of the furthest `STT_TLS` symbol.
    pub size: u64,
    /// Block alignment: `PT_TLS` `p_align` when known, otherwise `TLS_BLOCK_ALIGN`.
    pub align: u64,
    /// Guest address of the `.tdata` initialisation image (0 when there is none).
    pub init_addr: u64,
    /// Bytes of `.tdata` to copy (`p_filesz`). The rest of the block (`.tbss`) is zeroed.
    pub init_size: u64,
}

/// A module's `PT_TLS` program header. `vaddr` is relative to the module's load base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlsTemplate {
    pub vaddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

/// Static TLS layout for a set of modules, in the AArch64 variant I model: the thread pointer
/// addresses a 16-byte TCB and each module's block follows it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TlsLayout {
    /// Module base address -> its TLS block. Modules without TLS symbols have no entry.
    pub blocks: HashMap<u64, TlsBlock>,
    /// Exported TLS symbols as `name -> (module base, offset within that module's block)`.
    pub exports: HashMap<String, (u64, u64)>,
    /// Bytes from the thread pointer to the end of the last block (includes the TCB).
    pub total_size: u64,
    /// Largest block alignment. The thread pointer must be aligned to this.
    pub align: u64,
    /// Guest address of the static TLSDESC resolver. `R_AARCH64_TLSDESC` is unsupported while `None`.
    pub tlsdesc_resolver: Option<u64>,
}

/// Outcome of applying one module's relocation tables.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RelocationReport {
    /// Relocations written to guest memory.
    pub applied: usize,
    /// Strong undefined symbols with no provider in the import table.
    pub unresolved: Vec<String>,
    /// Relocation types this loader does not implement.
    pub unsupported: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
struct Symbol {
    name: u32,
    bind: u8,
    kind: u8,
    shndx: u16,
    value: u64,
    size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("File is too small to contain an NRO header")]
    TooSmall,

    #[error("Invalid NRO magic: expected 'NRO0', got {0:?}")]
    InvalidMagic([u8; 4]),

    #[error("Memory mapping error: {0}")]
    Memory(#[from] MemoryError),

    #[error("Cryptographic policy restriction: {0}")]
    RestrictedCipher(String),
}

#[derive(Debug, Clone)]
pub struct NroHeader {
    pub version: u32,
    pub size: u32,
    pub flags: u32,
    pub text_offset: u32,
    pub text_size: u32,
    pub rodata_offset: u32,
    pub rodata_size: u32,
    pub data_offset: u32,
    pub data_size: u32,
    pub bss_size: u32,
    pub relocations_applied: usize,
}

pub struct NroLoader;

impl NroLoader {
    /// Parses an NRO0 header from raw file bytes.
    pub fn parse_header(bytes: &[u8]) -> Result<NroHeader, LoadError> {
        if bytes.len() < 0x80 {
            return Err(LoadError::TooSmall);
        }

        let magic = &bytes[0x10..0x14];
        if magic != b"NRO0" {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(magic);
            return Err(LoadError::InvalidMagic(arr));
        }

        Ok(NroHeader {
            version: LittleEndian::read_u32(&bytes[0x14..0x18]),
            size: LittleEndian::read_u32(&bytes[0x18..0x1C]),
            flags: LittleEndian::read_u32(&bytes[0x1C..0x20]),
            text_offset: LittleEndian::read_u32(&bytes[0x20..0x24]),
            text_size: LittleEndian::read_u32(&bytes[0x24..0x28]),
            rodata_offset: LittleEndian::read_u32(&bytes[0x28..0x2C]),
            rodata_size: LittleEndian::read_u32(&bytes[0x2C..0x30]),
            data_offset: LittleEndian::read_u32(&bytes[0x30..0x34]),
            data_size: LittleEndian::read_u32(&bytes[0x34..0x38]),
            bss_size: LittleEndian::read_u32(&bytes[0x38..0x3C]),
            relocations_applied: 0,
        })
    }

    /// Loads an NRO executable into virtual memory, configures segments, and applies MOD0 relocations.
    pub fn load_into_memory(
        bytes: &[u8],
        base_addr: u64,
        mmu: &mut Memory,
    ) -> Result<NroHeader, LoadError> {
        let mut header = Self::parse_header(bytes)?;

        // 1. Map and write .text segment (Read + Write + Execute)
        let text_start = header.text_offset as usize;
        let text_end = text_start + header.text_size as usize;
        if text_end <= bytes.len() && header.text_size > 0 {
            mmu.map_range(
                base_addr,
                header.text_size as usize,
                Permission::READ_WRITE_EXECUTE,
            );
            mmu.write_bytes(base_addr, &bytes[text_start..text_end])?;
        }

        // 2. Map and write .rodata segment
        let rodata_addr = base_addr + header.rodata_offset as u64;
        let ro_start = header.rodata_offset as usize;
        let ro_end = ro_start + header.rodata_size as usize;
        if ro_end <= bytes.len() && header.rodata_size > 0 {
            mmu.map_range(
                rodata_addr,
                header.rodata_size as usize,
                Permission::READ_WRITE,
            );
            mmu.write_bytes(rodata_addr, &bytes[ro_start..ro_end])?;
        }

        // 3. Map and write .data segment + .bss (Read + Write)
        let data_addr = base_addr + header.data_offset as u64;
        let data_start = header.data_offset as usize;
        let data_end = data_start + header.data_size as usize;
        let total_data_size = (header.data_size + header.bss_size) as usize;

        if data_end <= bytes.len() && total_data_size > 0 {
            mmu.map_range(data_addr, total_data_size, Permission::READ_WRITE);
            mmu.write_bytes(data_addr, &bytes[data_start..data_end])?;
        }

        // 4. Parse MOD0 header and apply dynamic R_AARCH64_RELATIVE relocations if present
        header.relocations_applied = Self::apply_mod0_relocations(base_addr, mmu);

        Ok(header)
    }

    /// Loads an uncompressed Nintendo Shared Object (`NSO0`) executable into virtual memory
    /// (such as Switch 2 `RO` / `FS` system modules and ExeFS `main` binaries) and applies
    /// its relocations against its own symbols only. Modules that import from other modules should
    /// use `load_nso_segments` and then `apply_relocations` with a combined export table.
    pub fn load_nso_into_memory(
        bytes: &[u8],
        base_addr: u64,
        mmu: &mut Memory,
    ) -> Result<NroHeader, LoadError> {
        let mut header = Self::load_nso_segments(bytes, base_addr, mmu)?;
        header.relocations_applied = Self::apply_mod0_relocations(base_addr, mmu);
        Ok(header)
    }

    /// Maps an NSO0 image's segments into memory without applying any relocations.
    pub fn load_nso_segments(
        bytes: &[u8],
        base_addr: u64,
        mmu: &mut Memory,
    ) -> Result<NroHeader, LoadError> {
        if bytes.len() < 0x100 {
            return Err(LoadError::TooSmall);
        }
        let magic = &bytes[0x00..0x04];
        if magic != b"NSO0" {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(magic);
            return Err(LoadError::InvalidMagic(arr));
        }

        let text_file_off = LittleEndian::read_u32(&bytes[0x10..0x14]) as usize;
        let text_mem_off = LittleEndian::read_u32(&bytes[0x14..0x18]) as u64;
        let text_size = LittleEndian::read_u32(&bytes[0x18..0x1C]) as usize;

        let ro_file_off = LittleEndian::read_u32(&bytes[0x20..0x24]) as usize;
        let ro_mem_off = LittleEndian::read_u32(&bytes[0x24..0x28]) as u64;
        let ro_size = LittleEndian::read_u32(&bytes[0x28..0x2C]) as usize;

        let data_file_off = LittleEndian::read_u32(&bytes[0x30..0x34]) as usize;
        let data_mem_off = LittleEndian::read_u32(&bytes[0x34..0x38]) as u64;
        let data_size = LittleEndian::read_u32(&bytes[0x38..0x3C]) as usize;
        let bss_size = LittleEndian::read_u32(&bytes[0x3C..0x40]) as usize;

        if text_file_off + text_size <= bytes.len() && text_size > 0 {
            mmu.map_range(
                base_addr + text_mem_off,
                text_size,
                Permission::READ_WRITE_EXECUTE,
            );
            mmu.write_bytes(
                base_addr + text_mem_off,
                &bytes[text_file_off..text_file_off + text_size],
            )?;
        }

        if ro_file_off + ro_size <= bytes.len() && ro_size > 0 {
            mmu.map_range(base_addr + ro_mem_off, ro_size, Permission::READ_WRITE);
            mmu.write_bytes(
                base_addr + ro_mem_off,
                &bytes[ro_file_off..ro_file_off + ro_size],
            )?;
        }

        let total_data = data_size + bss_size;
        if data_file_off + data_size <= bytes.len() && total_data > 0 {
            mmu.map_range(base_addr + data_mem_off, total_data, Permission::READ_WRITE);
            if data_size > 0 {
                mmu.write_bytes(
                    base_addr + data_mem_off,
                    &bytes[data_file_off..data_file_off + data_size],
                )?;
            }
        }

        Ok(NroHeader {
            version: LittleEndian::read_u32(&bytes[0x04..0x08]),
            size: bytes.len() as u32,
            flags: LittleEndian::read_u32(&bytes[0x0C..0x10]),
            text_offset: text_mem_off as u32,
            text_size: text_size as u32,
            rodata_offset: ro_mem_off as u32,
            rodata_size: ro_size as u32,
            data_offset: data_mem_off as u32,
            data_size: data_size as u32,
            bss_size: bss_size as u32,
            relocations_applied: 0,
        })
    }

    /// Returns the address of the `.dynamic` section located through the MOD0 header, if any.
    fn dynamic_table_addr(base_addr: u64, mmu: &Memory) -> Option<u64> {
        let mod0_rel = match mmu.read_u32(base_addr + 4) {
            Ok(off) if off > 0 => off as u64,
            _ => return None,
        };
        let mod0_addr = base_addr + mod0_rel;
        let mut magic = [0u8; 4];
        if mmu.read_bytes(mod0_addr, &mut magic).is_err() || &magic != b"MOD0" {
            return None;
        }
        let dyn_rel = mmu.read_u32(mod0_addr + 4).ok()? as i32 as i64;
        Some((mod0_addr as i64 + dyn_rel) as u64)
    }

    /// Parses the `.dynamic` section described by the MOD0 header.
    pub fn parse_dynamic(base_addr: u64, mmu: &Memory) -> Option<DynamicInfo> {
        let dyn_addr = Self::dynamic_table_addr(base_addr, mmu)?;
        let mut info = DynamicInfo::default();

        // Walk Elf64_Dyn entries (16 bytes each: d_tag i64, d_val u64)
        for i in 0..256u64 {
            let entry_addr = dyn_addr + i * 16;
            let d_tag = match mmu.read_u64(entry_addr) {
                Ok(t) => t as i64,
                Err(_) => break,
            };
            if d_tag == DT_NULL {
                break;
            }
            let d_val = mmu.read_u64(entry_addr + 8).unwrap_or(0);
            match d_tag {
                DT_RELA => info.rela = Some(d_val),
                DT_RELASZ => info.rela_size = d_val,
                DT_RELAENT => info.rela_entry_size = d_val,
                DT_REL => info.rel = Some(d_val),
                DT_RELSZ => info.rel_size = d_val,
                DT_RELENT => info.rel_entry_size = d_val,
                DT_PLTREL => info.pltrel = Some(d_val),
                DT_JMPREL => info.jmprel = Some(d_val),
                DT_PLTRELSZ => info.pltrel_size = d_val,
                DT_SYMTAB => info.symtab = Some(d_val),
                DT_STRTAB => info.strtab = Some(d_val),
                DT_SYMENT => info.syment = d_val,
                DT_HASH => info.hash = Some(d_val),
                DT_GNU_HASH => info.gnu_hash = Some(d_val),
                DT_INIT => info.init = Some(d_val),
                DT_FINI => info.fini = Some(d_val),
                DT_INIT_ARRAY => info.init_array = Some(d_val),
                DT_INIT_ARRAYSZ => info.init_array_size = d_val,
                DT_FINI_ARRAY => info.fini_array = Some(d_val),
                DT_FINI_ARRAYSZ => info.fini_array_size = d_val,
                DT_PREINIT_ARRAY => info.preinit_array = Some(d_val),
                DT_PREINIT_ARRAYSZ => info.preinit_array_size = d_val,
                DT_RELR => info.relr = Some(d_val),
                DT_RELRSZ => info.relr_size = d_val,
                _ => {}
            }
        }
        Some(info)
    }

    fn sym_stride(info: &DynamicInfo) -> u64 {
        if info.syment == 0 { SYM_ENTRY_SIZE } else { info.syment }
    }

    /// Reads one Elf64_Sym entry (24 bytes).
    fn read_symbol(mmu: &Memory, addr: u64) -> Option<Symbol> {
        let mut raw = [0u8; 24];
        mmu.read_bytes(addr, &mut raw).ok()?;
        Some(Symbol {
            name: LittleEndian::read_u32(&raw[0..4]),
            bind: raw[4] >> 4,
            kind: raw[4] & 0xF,
            shndx: LittleEndian::read_u16(&raw[6..8]),
            value: LittleEndian::read_u64(&raw[8..16]),
            size: LittleEndian::read_u64(&raw[16..24]),
        })
    }

    fn symbol_name(base_addr: u64, mmu: &Memory, info: &DynamicInfo, name_off: u32) -> String {
        let strtab = match info.strtab {
            Some(s) => base_addr + s + name_off as u64,
            None => return String::new(),
        };
        let mut name = Vec::new();
        for i in 0..256u64 {
            let mut b = [0u8; 1];
            if mmu.read_bytes(strtab + i, &mut b).is_err() || b[0] == 0 {
                break;
            }
            name.push(b[0]);
        }
        String::from_utf8_lossy(&name).into_owned()
    }

    /// Symbol count from `DT_HASH` (the `nchain` field), or from `DT_GNU_HASH` when that is the only
    /// hash table. Modules with neither export nothing.
    fn symbol_count(base_addr: u64, mmu: &Memory, info: &DynamicInfo) -> u64 {
        if let Some(h) = info.hash {
            return mmu.read_u32(base_addr + h + 4).unwrap_or(0) as u64;
        }
        info.gnu_hash
            .and_then(|h| Self::gnu_hash_symbol_count(mmu, base_addr + h))
            .unwrap_or(0)
    }

    /// `DT_GNU_HASH` does not store the symbol count. Every hashed symbol sits in some bucket's chain,
    /// and chains are ordered by symbol index, so the count is one past the end of the highest bucket's chain.
    fn gnu_hash_symbol_count(mmu: &Memory, table: u64) -> Option<u64> {
        let nbuckets = mmu.read_u32(table).ok()? as u64;
        let symoffset = mmu.read_u32(table + 4).ok()? as u64;
        let bloom_size = mmu.read_u32(table + 8).ok()? as u64;
        let buckets = table + 16 + bloom_size * 8;
        let chains = buckets + nbuckets * 4;

        let mut last = 0u64;
        for i in 0..nbuckets {
            last = last.max(mmu.read_u32(buckets + i * 4).ok()? as u64);
        }
        if last < symoffset {
            return Some(symoffset);
        }
        // Bound the walk so a corrupt table cannot loop for long.
        for _ in 0..0x10_0000 {
            let hash = mmu.read_u32(chains + (last - symoffset) * 4).ok()?;
            if hash & 1 != 0 {
                return Some(last + 1);
            }
            last += 1;
        }
        None
    }

    /// Collects the module's defined, non-local symbols as `name -> absolute address`.
    /// The first definition of a name wins.
    pub fn collect_exports(base_addr: u64, mmu: &Memory) -> HashMap<String, u64> {
        let mut exports = HashMap::new();
        let info = match Self::parse_dynamic(base_addr, mmu) {
            Some(i) => i,
            None => return exports,
        };
        let symtab = match info.symtab {
            Some(s) => base_addr + s,
            None => return exports,
        };
        let stride = Self::sym_stride(&info);
        for idx in 1..Self::symbol_count(base_addr, mmu, &info) {
            let Some(sym) = Self::read_symbol(mmu, symtab + idx * stride) else { break };
            if sym.shndx == SHN_UNDEF || sym.bind == STB_LOCAL || sym.kind == STT_TLS {
                continue;
            }
            let name = Self::symbol_name(base_addr, mmu, &info, sym.name);
            if !name.is_empty() {
                exports.entry(name).or_insert(base_addr + sym.value);
            }
        }
        exports
    }

    /// Reads the module's defined `STT_TLS` symbols as `(name, offset in block, size, bind)`.
    fn tls_symbols(base_addr: u64, mmu: &Memory) -> Vec<(String, u64, u64, u8)> {
        let mut out = Vec::new();
        let Some(info) = Self::parse_dynamic(base_addr, mmu) else { return out };
        let Some(symtab) = info.symtab.map(|s| base_addr + s) else { return out };
        let stride = Self::sym_stride(&info);
        for idx in 1..Self::symbol_count(base_addr, mmu, &info) {
            let Some(sym) = Self::read_symbol(mmu, symtab + idx * stride) else { break };
            if sym.shndx == SHN_UNDEF || sym.kind != STT_TLS {
                continue;
            }
            let name = Self::symbol_name(base_addr, mmu, &info, sym.name);
            out.push((name, sym.value, sym.size, sym.bind));
        }
        out
    }

    /// Lays out one static TLS block per module that defines TLS symbols, in `module_bases` order.
    /// NSO/NRO images have no `PT_TLS`; see `build_tls_layout_with_templates` for modules that do.
    pub fn build_tls_layout(module_bases: &[u64], mmu: &Memory) -> TlsLayout {
        Self::build_tls_layout_with_templates(module_bases, mmu, &HashMap::new())
    }

    /// Lays out one static TLS block per module, in `module_bases` order, in the AArch64 variant I
    /// model. A module with a `PT_TLS` entry in `templates` (keyed by module base) gets its block size,
    /// alignment and `.tdata` image from it. Otherwise the size is the end of its furthest `STT_TLS`
    /// symbol, the alignment is `TLS_BLOCK_ALIGN`, and the block starts zeroed. TLS that is only
    /// reached through local relocations (no symbol) is not counted in that fallback.
    pub fn build_tls_layout_with_templates(
        module_bases: &[u64],
        mmu: &Memory,
        templates: &HashMap<u64, TlsTemplate>,
    ) -> TlsLayout {
        let mut layout = TlsLayout { align: TLS_BLOCK_ALIGN, ..Default::default() };
        let mut cursor = TLS_TCB_SIZE;
        for &base in module_bases {
            let symbols = Self::tls_symbols(base, mmu);
            let symbol_size = symbols.iter().map(|(_, off, sz, _)| off + sz).max().unwrap_or(0);
            let template = templates.get(&base);
            let size = template.map_or(symbol_size, |t| t.memsz.max(symbol_size));
            if size == 0 {
                continue;
            }
            let align = template.map_or(TLS_BLOCK_ALIGN, |t| t.align.max(1).next_power_of_two());
            cursor = (cursor + align - 1) & !(align - 1);
            let module_id = layout.blocks.len() as u64 + 1;
            let (init_addr, init_size) = match template {
                Some(t) => (base + t.vaddr, t.filesz.min(size)),
                None => {
                    // Only fires when a module exports these names in `.dynsym`. libnx's `switch.ld`
                    // defines `__tdata_lma`/`__tdata_lma_end` with `PROVIDE_HIDDEN`, so libnx NROs and
                    // NSOs never export them and their block starts zeroed here. That is harmless for
                    // libnx: its crt0 path (`newlibSetup`, `threadCreate`) copies `.tdata` itself and
                    // keeps its own thread pointer at `TPIDRRO_EL0 + 0x1F8`, not in `TPIDR_EL0`.
                    let exports = Self::collect_exports(base, mmu);
                    if let (Some(&lma_start), Some(&lma_end)) = (exports.get("__tdata_lma"), exports.get("__tdata_lma_end")) {
                        if lma_end >= lma_start {
                            (lma_start, (lma_end - lma_start).min(size))
                        } else {
                            (0, 0)
                        }
                    } else if let (Some(&start), Some(&end)) = (exports.get("__tdata_start"), exports.get("__tdata_end")) {
                        if end >= start {
                            (start, (end - start).min(size))
                        } else {
                            (0, 0)
                        }
                    } else {
                        (0, 0)
                    }
                }
            };
            layout.blocks.insert(base, TlsBlock { module_id, tp_offset: cursor, size, align, init_addr, init_size });
            layout.align = layout.align.max(align);
            cursor += size;
            for (name, off, _, bind) in symbols {
                if bind != STB_LOCAL && !name.is_empty() {
                    layout.exports.entry(name).or_insert((base, off));
                }
            }
        }
        layout.total_size = if layout.blocks.is_empty() { 0 } else { cursor };
        layout
    }

    /// Reads the `PT_TLS` program header of a little-endian ELF64 file. `None` if the file is not
    /// ELF64 or has no `PT_TLS`.
    pub fn parse_pt_tls(elf: &[u8]) -> Option<TlsTemplate> {
        if elf.len() < 0x40 || &elf[0..4] != b"\x7fELF" || elf[4] != 2 || elf[5] != 1 {
            return None;
        }
        let phoff = LittleEndian::read_u64(&elf[0x20..0x28]) as usize;
        let phentsize = LittleEndian::read_u16(&elf[0x36..0x38]) as usize;
        let phnum = LittleEndian::read_u16(&elf[0x38..0x3A]) as usize;
        if phentsize < 0x38 {
            return None;
        }
        (0..phnum).find_map(|i| {
            let start = phoff.checked_add(i * phentsize)?;
            let ph = elf.get(start..start + 0x38)?;
            (LittleEndian::read_u32(&ph[0..4]) == PT_TLS).then(|| TlsTemplate {
                vaddr: LittleEndian::read_u64(&ph[0x10..0x18]),
                filesz: LittleEndian::read_u64(&ph[0x20..0x28]),
                memsz: LittleEndian::read_u64(&ph[0x28..0x30]),
                align: LittleEndian::read_u64(&ph[0x30..0x38]),
            })
        })
    }

    /// The initial contents of one thread's static TLS area, from the thread pointer onward:
    /// a zeroed TCB, then each block's `.tdata` image followed by zeroed `.tbss`.
    pub fn tls_thread_image(layout: &TlsLayout, mmu: &Memory) -> crate::kernel::ThreadTlsImage {
        let mut bytes = vec![0u8; layout.total_size as usize];
        for block in layout.blocks.values() {
            if block.init_size > 0 {
                let start = block.tp_offset as usize;
                let _ = mmu.read_bytes(block.init_addr, &mut bytes[start..start + block.init_size as usize]);
            }
        }
        crate::kernel::ThreadTlsImage { bytes, align: layout.align.max(1) }
    }

    /// Writes the static TLSDESC resolver stub at `addr` (mapped read/execute by the caller or here).
    pub fn install_tlsdesc_resolver(mmu: &mut Memory, addr: u64) -> Result<(), MemoryError> {
        mmu.map_range(addr, 0x1000, Permission::READ_WRITE_EXECUTE);
        for (i, insn) in TLSDESC_RESOLVER_CODE.iter().enumerate() {
            mmu.write_u32(addr + i as u64 * 4, *insn)?;
        }
        Ok(())
    }

    /// Maps a static TLS area at `thread_pointer` and writes `image` into it.
    /// The caller sets `TPIDR_EL0` to `thread_pointer`, which must be aligned to `image.align`.
    pub fn map_static_tls(
        mmu: &mut Memory,
        thread_pointer: u64,
        image: &crate::kernel::ThreadTlsImage,
    ) -> Result<(), MemoryError> {
        let size = image.mapped_size();
        mmu.map_range(thread_pointer, size as usize, Permission::READ_WRITE);
        mmu.write_bytes(thread_pointer, &vec![0u8; size as usize])?;
        mmu.write_bytes(thread_pointer, &image.bytes)
    }

    /// Reads a `DT_*_ARRAY` of function pointers, skipping 0 and -1 entries. Entries are read as stored,
    /// so the array must already be relocated.
    fn read_function_array(mmu: &Memory, base_addr: u64, array: Option<u64>, size: u64) -> Vec<u64> {
        let Some(array) = array else { return Vec::new() };
        (0..size / 8)
            .filter_map(|i| mmu.read_u64(base_addr + array + i * 8).ok())
            .filter(|&f| f != 0 && f != u64::MAX)
            .collect()
    }

    /// Initialisers in ELF order: `DT_PREINIT_ARRAY` (main executable only), `DT_INIT`, `DT_INIT_ARRAY`.
    /// Call after relocation.
    pub fn init_functions(base_addr: u64, mmu: &Memory, is_main: bool) -> Vec<u64> {
        let Some(info) = Self::parse_dynamic(base_addr, mmu) else { return Vec::new() };
        let mut out = Vec::new();
        if is_main {
            out.extend(Self::read_function_array(mmu, base_addr, info.preinit_array, info.preinit_array_size));
        }
        out.extend(info.init.map(|i| base_addr + i));
        out.extend(Self::read_function_array(mmu, base_addr, info.init_array, info.init_array_size));
        out
    }

    /// Finalisers in ELF order: `DT_FINI_ARRAY` in reverse, then `DT_FINI`. Call after relocation.
    pub fn fini_functions(base_addr: u64, mmu: &Memory) -> Vec<u64> {
        let Some(info) = Self::parse_dynamic(base_addr, mmu) else { return Vec::new() };
        let mut out = Self::read_function_array(mmu, base_addr, info.fini_array, info.fini_array_size);
        out.reverse();
        out.extend(info.fini.map(|f| base_addr + f));
        out
    }

    /// Address of the symbol referenced by a relocation. Defined symbols resolve locally;
    /// undefined ones come from `imports`, and undefined weak symbols resolve to 0.
    fn resolve_symbol(
        base_addr: u64,
        mmu: &Memory,
        info: &DynamicInfo,
        sym_index: u64,
        imports: &HashMap<String, u64>,
    ) -> Result<u64, String> {
        if sym_index == 0 {
            return Ok(0);
        }
        let unnamed = || format!("<symbol #{}>", sym_index);
        let symtab = info.symtab.map(|s| base_addr + s).ok_or_else(unnamed)?;
        let sym = Self::read_symbol(mmu, symtab + sym_index * Self::sym_stride(info)).ok_or_else(unnamed)?;
        if sym.shndx != SHN_UNDEF {
            return Ok(base_addr + sym.value);
        }
        let name = Self::symbol_name(base_addr, mmu, info, sym.name);
        if let Some(&value) = imports.get(&name) {
            return Ok(value);
        }
        if sym.bind == STB_WEAK {
            return Ok(0);
        }
        Err(name)
    }

    /// TLS symbol referenced by a relocation, as `(module base, offset in that module's block)`.
    /// Symbol index 0 refers to the relocating module itself (local-dynamic: offset is the addend).
    /// `Ok(None)` is an undefined weak TLS symbol.
    fn resolve_tls_symbol(
        base_addr: u64,
        mmu: &Memory,
        info: &DynamicInfo,
        sym_index: u64,
        tls: &TlsLayout,
    ) -> Result<Option<(u64, u64)>, String> {
        if sym_index == 0 {
            return Ok(Some((base_addr, 0)));
        }
        let unnamed = || format!("<symbol #{}>", sym_index);
        let symtab = info.symtab.map(|s| base_addr + s).ok_or_else(unnamed)?;
        let sym = Self::read_symbol(mmu, symtab + sym_index * Self::sym_stride(info)).ok_or_else(unnamed)?;
        if sym.shndx != SHN_UNDEF {
            return Ok(Some((base_addr, sym.value)));
        }
        let name = Self::symbol_name(base_addr, mmu, info, sym.name);
        if let Some(&found) = tls.exports.get(&name) {
            return Ok(Some(found));
        }
        if sym.bind == STB_WEAK {
            return Ok(None);
        }
        Err(name)
    }

    /// Applies the module's `.rela.dyn` and `.rela.plt` tables. Imports are looked up in `imports`.
    /// TLS relocations are reported as unsupported; use `apply_relocations_with_tls` for them.
    pub fn apply_relocations(
        base_addr: u64,
        mmu: &mut Memory,
        imports: &HashMap<String, u64>,
    ) -> RelocationReport {
        Self::apply_relocations_with_tls(base_addr, mmu, imports, &TlsLayout::default())
    }

    /// Like `apply_relocations`, and also applies TLS relocations against the static layout `tls`.
    pub fn apply_relocations_with_tls(
        base_addr: u64,
        mmu: &mut Memory,
        imports: &HashMap<String, u64>,
        tls: &TlsLayout,
    ) -> RelocationReport {
        let mut report = RelocationReport::default();
        let info = match Self::parse_dynamic(base_addr, mmu) {
            Some(i) => i,
            None => return report,
        };
        let rela_entry = if info.rela_entry_size == 0 { RELA_ENTRY_SIZE } else { info.rela_entry_size };
        let rel_entry = if info.rel_entry_size == 0 { REL_ENTRY_SIZE } else { info.rel_entry_size };

        // (table, size in bytes, entry size, has explicit addend)
        let mut tables = Vec::new();
        if let Some(rela) = info.rela {
            tables.push((rela, info.rela_size, rela_entry, true));
        }
        if let Some(rel) = info.rel {
            tables.push((rel, info.rel_size, rel_entry, false));
        }
        if let Some(jmprel) = info.jmprel {
            if info.pltrel == Some(DT_REL as u64) {
                tables.push((jmprel, info.pltrel_size, rel_entry, false));
            } else {
                tables.push((jmprel, info.pltrel_size, rela_entry, true));
            }
        }

        for (table, table_size, entry_size, has_addend) in tables {
            for i in 0..table_size / entry_size {
                let entry_addr = base_addr + table + i * entry_size;
                let mut raw = [0u8; 24];
                let len = if has_addend { 24 } else { 16 };
                if mmu.read_bytes(entry_addr, &mut raw[..len]).is_err() {
                    continue;
                }
                let r_offset = LittleEndian::read_u64(&raw[0..8]);
                let r_info = LittleEndian::read_u64(&raw[8..16]);
                let sym_index = r_info >> 32;
                let r_type = (r_info & 0xFFFF_FFFF) as u32;
                let target_va = base_addr + r_offset;
                // `Elf64_Rel` has no addend field: the addend is the value already stored at the target.
                let r_addend = if has_addend {
                    LittleEndian::read_u64(&raw[16..24])
                } else {
                    mmu.read_u64(target_va).unwrap_or(0)
                };

                match r_type {
                    R_AARCH64_RELATIVE => {
                        if mmu.write_u64(target_va, base_addr.wrapping_add(r_addend)).is_ok() {
                            report.applied += 1;
                        }
                    }
                    R_AARCH64_ABS64 | R_AARCH64_GLOB_DAT | R_AARCH64_JUMP_SLOT => {
                        let symbol_value = match Self::resolve_symbol(base_addr, mmu, &info, sym_index, imports) {
                            Ok(v) => v,
                            Err(name) => {
                                report.unresolved.push(name);
                                continue;
                            }
                        };
                        let value = if r_type == R_AARCH64_ABS64 {
                            symbol_value.wrapping_add(r_addend)
                        } else {
                            symbol_value
                        };
                        if mmu.write_u64(target_va, value).is_ok() {
                            report.applied += 1;
                        }
                    }
                    R_AARCH64_TLS_DTPMOD64 | R_AARCH64_TLS_DTPREL64 | R_AARCH64_TLS_TPREL64 | R_AARCH64_TLSDESC => {
                        let resolver = tls.tlsdesc_resolver;
                        if tls.blocks.is_empty() || r_type == R_AARCH64_TLSDESC && resolver.is_none() {
                            if !report.unsupported.contains(&r_type) {
                                report.unsupported.push(r_type);
                            }
                            continue;
                        }
                        let target = match Self::resolve_tls_symbol(base_addr, mmu, &info, sym_index, tls) {
                            Ok(t) => t,
                            Err(name) => {
                                report.unresolved.push(name);
                                continue;
                            }
                        };
                        // An undefined weak TLS symbol resolves to module 0, offset 0.
                        let block = target.and_then(|(module, off)| tls.blocks.get(&module).map(|b| (b, off)));
                        let (module_id, tp_offset, dtp_offset) = match (target, block) {
                            (None, _) => (0, 0, 0),
                            (Some(_), Some((b, off))) => (b.module_id, b.tp_offset + off, off),
                            (Some(_), None) => {
                                if !report.unsupported.contains(&r_type) {
                                    report.unsupported.push(r_type);
                                }
                                continue;
                            }
                        };
                        let written = match r_type {
                            R_AARCH64_TLS_DTPMOD64 => mmu.write_u64(target_va, module_id),
                            R_AARCH64_TLS_DTPREL64 => mmu.write_u64(target_va, dtp_offset.wrapping_add(r_addend)),
                            R_AARCH64_TLS_TPREL64 => mmu.write_u64(target_va, tp_offset.wrapping_add(r_addend)),
                            // Static TLSDESC: the resolver returns the descriptor's second word, the TP offset.
                            _ => mmu
                                .write_u64(target_va, resolver.unwrap_or(0))
                                .and_then(|_| mmu.write_u64(target_va + 8, tp_offset.wrapping_add(r_addend))),
                        };
                        if written.is_ok() {
                            report.applied += 1;
                        }
                    }
                    other => {
                        if !report.unsupported.contains(&other) {
                            report.unsupported.push(other);
                        }
                    }
                }
            }
        }
        report
    }

    /// Applies the module's `DT_RELR` table: each listed word gets `base_addr` added to it.
    /// Even entries name one word (image offset); odd entries are bitmaps over the next 63 words.
    /// Returns the number of words relocated.
    ///
    /// Not part of `apply_relocations`, and so not done by `load_into_memory`: unlike RELA, adding the
    /// base is not idempotent, and a libnx `crt0` applies `DT_RELR` itself at startup.
    /// Call this only for an image that will not relocate itself.
    pub fn apply_relr(base_addr: u64, mmu: &mut Memory) -> usize {
        let info = match Self::parse_dynamic(base_addr, mmu) {
            Some(i) => i,
            None => return 0,
        };
        let table = match info.relr {
            Some(t) => base_addr + t,
            None => return 0,
        };
        let mut applied = 0;
        let mut add_base = |mmu: &mut Memory, addr: u64| {
            if let Ok(v) = mmu.read_u64(addr) {
                if mmu.write_u64(addr, v.wrapping_add(base_addr)).is_ok() {
                    applied += 1;
                }
            }
        };
        let mut next = 0u64;
        for i in 0..info.relr_size / 8 {
            let entry = match mmu.read_u64(table + i * 8) {
                Ok(e) => e,
                Err(_) => break,
            };
            if entry & 1 == 0 {
                let addr = base_addr + entry;
                add_base(mmu, addr);
                next = addr + 8;
            } else {
                for bit in 1..64u64 {
                    if entry >> bit & 1 != 0 {
                        add_base(mmu, next + (bit - 1) * 8);
                    }
                }
                next += 63 * 8;
            }
        }
        applied
    }

    /// Applies the module's relocations using only its own symbols, returning the number applied.
    pub fn apply_mod0_relocations(base_addr: u64, mmu: &mut Memory) -> usize {
        Self::apply_relocations(base_addr, mmu, &HashMap::new()).applied
    }

    /// Helper to assemble a minimal valid NRO binary with custom text bytes for testing.
    pub fn build_synthetic_nro(text_code: &[u8]) -> Vec<u8> {
        let mut binary = vec![0u8; 0x80 + text_code.len()];

        // 0x00: Branch instruction B #0x80 (0x14000020)
        LittleEndian::write_u32(&mut binary[0x00..0x04], 0x14000020);
        // 0x04: MOD0 offset
        LittleEndian::write_u32(&mut binary[0x04..0x08], 0x00000000);

        // 0x10: Magic "NRO0"
        binary[0x10..0x14].copy_from_slice(b"NRO0");
        // 0x14: Version = 0
        LittleEndian::write_u32(&mut binary[0x14..0x18], 0);
        // 0x18: Total size
        let total_len = binary.len() as u32;
        LittleEndian::write_u32(&mut binary[0x18..0x1C], total_len);
        // 0x1C: Flags
        LittleEndian::write_u32(&mut binary[0x1C..0x20], 0);

        // 0x20: .text offset = 0x00 (includes NRO start header)
        LittleEndian::write_u32(&mut binary[0x20..0x24], 0x00);
        // 0x24: .text size
        let text_len = binary.len() as u32;
        LittleEndian::write_u32(&mut binary[0x24..0x28], text_len);

        // Copy text code at 0x80
        binary[0x80..0x80 + text_code.len()].copy_from_slice(text_code);

        binary
    }

    /// Helper to assemble a clean-room valid NSO0 binary with custom text bytes for testing.
    pub fn build_synthetic_nso(text_code: &[u8]) -> Vec<u8> {
        let text_size = text_code.len();
        let total_size = 0x100 + text_size;
        let mut binary = vec![0u8; total_size];

        // 0x00: Magic "NSO0"
        binary[0x00..0x04].copy_from_slice(b"NSO0");
        // 0x04: Version
        LittleEndian::write_u32(&mut binary[0x04..0x08], 0);
        // 0x08: Flags
        LittleEndian::write_u32(&mut binary[0x08..0x0C], 0);

        // Text segment descriptor:
        LittleEndian::write_u32(&mut binary[0x10..0x14], 0x100); // text_file_off
        LittleEndian::write_u32(&mut binary[0x14..0x18], 0x000); // text_mem_off
        LittleEndian::write_u32(&mut binary[0x18..0x1C], text_size as u32); // text_size

        // Copy text code starting at 0x100
        binary[0x100..0x100 + text_size].copy_from_slice(text_code);

        binary
    }

    /// Provides test NSO0 payload bytes: loads from external disk file if present,
    /// or generates a synthetic NSO0 module.
    pub fn synthetic_nso_payload() -> Vec<u8> {
        // Clean-room synthetic AArch64 NSO0 (generated in-process; never read from disk) that writes SFCO, NS2ROEXE, challenge, pid, and tid to TLS and exits cleanly:
        #[rustfmt::skip]
        let synthetic_code: [u32; 21] = [
            0xD53BD068, // mrs  x8, tpidrro_el0
            0x5288CA69, // movz w9, #0x4653
            0x72A9E869, // movk w9, #0x4f43, lsl #16 (0x4F434653 "SFCO")
            0xB9002109, // str  w9, [x8, #32]
            0xB900291F, // str  wzr, [x8, #40] (result = 0)
            0xD28A69CA, // movz x10, #0x534e
            0xF2AA464A, // movk x10, #0x5232, lsl #16
            0xF2C8A9EA, // movk x10, #0x454f, lsl #32
            0xF2E8AB0A, // movk x10, #0x4558, lsl #48 (0x4558454F5232534E "NS2ROEXE")
            0xF900190A, // str  x10, [x8, #48]
            0xD28E60EB, // movz x11, #0x7307
            0xF2B67A8B, // movk x11, #0xb3d4, lsl #16
            0xF2C122AB, // movk x11, #0x0915, lsl #32
            0xF2F18BCB, // movk x11, #0x8c5e, lsl #48 (0x8c5e0915b3d47307)
            0xF9001D0B, // str  x11, [x8, #56]
            0xD2800A4C, // movz x12, #0x52
            0xF900210C, // str  x12, [x8, #64] (pid = 0x52)
            0xD280002D, // movz x13, #1
            0xF900250D, // str  x13, [x8, #72] (tid = 1)
            0xD2800000, // movz x0, #0
            0xD40000E1, // svc  #0x07 (svcExitProcess)
        ];

        let mut code_bytes = Vec::new();
        for insn in &synthetic_code {
            code_bytes.extend_from_slice(&insn.to_le_bytes());
        }

        Self::build_synthetic_nso(&code_bytes)
    }
}

/// An `ARGV` parameter block. This is not the homebrew loader ABI: libnx `crt0` reads a `ConfigEntry`
/// list from `X0` (see `HomebrewConfig`), which `ounce-core run` uses for NROs. Kept for callers of this format.
pub struct HomebrewEnvironment;

impl HomebrewEnvironment {
    /// Formats and initializes the standard `ARGV` block in guest memory at `env_addr` (e.g., 0x7FFF_FE00)
    /// and configures guest CPU registers (`X0` = argv pointer, `X1` = main thread handle).
    pub fn setup(
        cpu: &mut crate::cpu::Cpu,
        mmu: &mut Memory,
        env_addr: u64,
        argv: &[&str],
    ) -> Result<(), MemoryError> {
        mmu.map_range(env_addr, 0x1000, Permission::READ_WRITE);

        // Header: "ARGV" (0x56475241) at offset 0x00
        // Argc at offset 0x04
        // Pointers at offset 0x08..
        let magic: [u8; 4] = *b"ARGV";
        mmu.write_bytes(env_addr, &magic)?;
        mmu.write_u32(env_addr + 4, argv.len() as u32)?;

        let mut str_offset = env_addr + 0x100;
        for (i, &arg) in argv.iter().enumerate() {
            let ptr_addr = env_addr + 8 + (i * 8) as u64;
            mmu.write_u64(ptr_addr, str_offset)?;
            mmu.write_bytes(str_offset, arg.as_bytes())?;
            mmu.write_u8(str_offset + arg.len() as u64, 0)?; // Null terminator
            str_offset += (arg.len() + 1 + 7) as u64 & !7; // Align to 8 bytes
        }

        // Set AArch64 entry ABI:
        // X0 = Context / argv block address
        // X1 = Main thread handle (0xFFFF_FFFF_FFFF_FFFF)
        cpu.regs.write_x(0, env_addr);
        cpu.regs.write_x(1, 0xFFFF_FFFF_FFFF_FFFF);

        Ok(())
    }
}

/// Writes a MOD0 module: `.dynamic` at +0x120, symbols at +0x800, strings at +0x900,
/// `DT_HASH` at +0x880, `.rela.dyn` at +0xA00 and `.rela.plt` at +0xB00.
/// `symbols` are `(name, shndx, bind, value)`; relocations are `(r_offset, r_type, sym_index, addend)`.
#[cfg(test)]
fn write_test_module(
    mmu: &mut Memory,
    base: u64,
    symbols: &[(&str, u16, u8, u64)],
    rela: &[(u64, u32, u64, u64)],
    jmprel: &[(u64, u32, u64, u64)],
) {
    let full: Vec<TestSymbol> = symbols
        .iter()
        .map(|&(name, shndx, bind, value)| TestSymbol { name, shndx, bind, kind: 2, value, size: 0 })
        .collect();
    write_test_module_with(mmu, base, &full, rela, jmprel, &[], false);
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) struct TestSymbol<'a> {
    pub name: &'a str,
    pub shndx: u16,
    pub bind: u8,
    pub kind: u8,
    pub value: u64,
    pub size: u64,
}

/// `write_test_module` with full symbols, extra `.dynamic` entries, and optionally `DT_GNU_HASH`
/// (at +0x880, replacing `DT_HASH`).
#[cfg(test)]
pub(crate) fn write_test_module_with(
    mmu: &mut Memory,
    base: u64,
    symbols: &[TestSymbol],
    rela: &[(u64, u32, u64, u64)],
    jmprel: &[(u64, u32, u64, u64)],
    extra_dynamic: &[(i64, u64)],
    gnu_hash: bool,
) {
    mmu.map_range(base, 0x2000, Permission::READ_WRITE_EXECUTE);
    mmu.write_u32(base + 4, 0x100).unwrap();
    mmu.write_bytes(base + 0x100, b"MOD0").unwrap();
    mmu.write_u32(base + 0x104, 0x20).unwrap();

    let mut entries: Vec<(i64, u64)> = vec![
        (if gnu_hash { DT_GNU_HASH } else { DT_HASH }, 0x880),
        (DT_STRTAB, 0x900),
        (DT_SYMTAB, 0x800),
        (DT_SYMENT, 24),
        (DT_RELA, 0xA00),
        (DT_RELASZ, 24 * rela.len() as u64),
        (DT_RELAENT, 24),
        (DT_JMPREL, 0xB00),
        (DT_PLTRELSZ, 24 * jmprel.len() as u64),
    ];
    entries.extend_from_slice(extra_dynamic);
    entries.push((DT_NULL, 0));
    for (i, (tag, val)) in entries.iter().enumerate() {
        mmu.write_u64(base + 0x120 + (i as u64) * 16, *tag as u64).unwrap();
        mmu.write_u64(base + 0x128 + (i as u64) * 16, *val).unwrap();
    }

    if gnu_hash {
        // One bucket holding every symbol from index 1; only the chain terminator bits matter here.
        mmu.write_u32(base + 0x880, 1).unwrap(); // nbuckets
        mmu.write_u32(base + 0x884, 1).unwrap(); // symoffset
        mmu.write_u32(base + 0x888, 1).unwrap(); // bloom_size
        mmu.write_u64(base + 0x890, u64::MAX).unwrap(); // bloom
        mmu.write_u32(base + 0x898, if symbols.is_empty() { 0 } else { 1 }).unwrap(); // bucket[0]
        for i in 0..symbols.len() {
            let last = i + 1 == symbols.len();
            mmu.write_u32(base + 0x89C + i as u64 * 4, (i as u32) << 1 | last as u32).unwrap();
        }
    } else {
        // DT_HASH: only `nchain` (the symbol count, including the null symbol) matters here.
        mmu.write_u32(base + 0x884, (symbols.len() + 1) as u32).unwrap();
    }

    // Symbol 0 is the null symbol; string offset 0 is the empty string.
    let mut strtab_off = 1u32;
    for (i, sym) in symbols.iter().enumerate() {
        let entry = base + 0x800 + ((i as u64) + 1) * 24;
        mmu.write_u32(entry, strtab_off).unwrap();
        mmu.write_bytes(entry + 4, &[(sym.bind << 4) | sym.kind]).unwrap();
        mmu.write_bytes(entry + 6, &sym.shndx.to_le_bytes()).unwrap();
        mmu.write_u64(entry + 8, sym.value).unwrap();
        mmu.write_u64(entry + 16, sym.size).unwrap();
        mmu.write_bytes(base + 0x900 + strtab_off as u64, sym.name.as_bytes()).unwrap();
        mmu.write_bytes(base + 0x900 + strtab_off as u64 + sym.name.len() as u64, &[0]).unwrap();
        strtab_off += sym.name.len() as u32 + 1;
    }

    for (table_base, table) in [(0xA00u64, rela), (0xB00u64, jmprel)] {
        for (i, (offset, r_type, sym, addend)) in table.iter().enumerate() {
            let e = base + table_base + (i as u64) * 24;
            mmu.write_u64(e, *offset).unwrap();
            mmu.write_u64(e + 8, (sym << 32) | (*r_type as u64)).unwrap();
            mmu.write_u64(e + 16, *addend).unwrap();
        }
    }
}

/// Builds a complete NSO0 file whose `.text` is a MOD0 module described by `symbols`, `rela`, and `jmprel`
/// (same layout as `write_test_module`). Used by tests that need a real loadable image with imports/exports.
#[cfg(test)]
pub(crate) fn build_linked_nso(
    symbols: &[(&str, u16, u8, u64)],
    rela: &[(u64, u32, u64, u64)],
    jmprel: &[(u64, u32, u64, u64)],
) -> Vec<u8> {
    let full: Vec<TestSymbol> = symbols
        .iter()
        .map(|&(name, shndx, bind, value)| TestSymbol { name, shndx, bind, kind: 2, value, size: 0 })
        .collect();
    build_linked_nso_with(&full, rela, jmprel)
}

/// `build_linked_nso` with full symbols (type and size), for TLS tests.
#[cfg(test)]
pub(crate) fn build_linked_nso_with(
    symbols: &[TestSymbol],
    rela: &[(u64, u32, u64, u64)],
    jmprel: &[(u64, u32, u64, u64)],
) -> Vec<u8> {
    const IMAGE_SIZE: usize = 0x2000;
    let scratch_base = 0x8000_0000u64;
    let mut scratch = Memory::new();
    write_test_module_with(&mut scratch, scratch_base, symbols, rela, jmprel, &[], false);
    let mut image = vec![0u8; IMAGE_SIZE];
    scratch.read_bytes(scratch_base, &mut image).unwrap();

    let mut nso = vec![0u8; 0x100];
    nso[0..4].copy_from_slice(b"NSO0");
    nso[0x10..0x14].copy_from_slice(&0x100u32.to_le_bytes()); // .text file offset
    nso[0x14..0x18].copy_from_slice(&0u32.to_le_bytes()); // .text memory offset
    nso[0x18..0x1C].copy_from_slice(&(IMAGE_SIZE as u32).to_le_bytes()); // .text size
    nso.extend_from_slice(&image);
    nso
}

/// Minimal ELF64 header with one `PT_LOAD` and one `PT_TLS` program header.
#[cfg(test)]
pub(crate) fn elf_with_pt_tls(tls: TlsTemplate) -> Vec<u8> {
    let mut elf = vec![0u8; 0x40 + 2 * 0x38];
    elf[0..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2; // ELFCLASS64
    elf[5] = 1; // little-endian
    LittleEndian::write_u64(&mut elf[0x20..0x28], 0x40); // e_phoff
    LittleEndian::write_u16(&mut elf[0x36..0x38], 0x38); // e_phentsize
    LittleEndian::write_u16(&mut elf[0x38..0x3A], 2); // e_phnum
    LittleEndian::write_u32(&mut elf[0x40..0x44], 1); // PT_LOAD
    let ph = 0x78;
    LittleEndian::write_u32(&mut elf[ph..ph + 4], PT_TLS);
    LittleEndian::write_u64(&mut elf[ph + 0x10..ph + 0x18], tls.vaddr);
    LittleEndian::write_u64(&mut elf[ph + 0x18..ph + 0x20], tls.vaddr); // p_paddr
    LittleEndian::write_u64(&mut elf[ph + 0x20..ph + 0x28], tls.filesz);
    LittleEndian::write_u64(&mut elf[ph + 0x28..ph + 0x30], tls.memsz);
    LittleEndian::write_u64(&mut elf[ph + 0x30..ph + 0x38], tls.align);
    elf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tls_sym(name: &str, shndx: u16, value: u64, size: u64) -> TestSymbol<'_> {
        TestSymbol { name, shndx, bind: 1, kind: STT_TLS, value, size }
    }

    #[test]
    fn test_gnu_hash_symbol_count_and_exports() {
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        let syms = [
            TestSymbol { name: "alpha", shndx: 1, bind: 1, kind: 2, value: 0x500, size: 0 },
            TestSymbol { name: "beta", shndx: 1, bind: 1, kind: 2, value: 0x600, size: 0 },
            TestSymbol { name: "gamma", shndx: 1, bind: 2, kind: 1, value: 0x700, size: 8 },
        ];
        write_test_module_with(&mut mmu, base, &syms, &[], &[], &[], true);

        let info = NroLoader::parse_dynamic(base, &mmu).unwrap();
        assert_eq!(info.hash, None);
        assert_eq!(info.gnu_hash, Some(0x880));
        assert_eq!(NroLoader::symbol_count(base, &mmu, &info), 4);

        let exports = NroLoader::collect_exports(base, &mmu);
        assert_eq!(exports.len(), 3);
        assert_eq!(exports.get("gamma"), Some(&(base + 0x700)));
    }

    #[test]
    fn test_gnu_hash_with_no_hashed_symbols() {
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        write_test_module_with(&mut mmu, base, &[], &[], &[], &[], true);
        let info = NroLoader::parse_dynamic(base, &mmu).unwrap();
        assert_eq!(NroLoader::symbol_count(base, &mmu, &info), 1);
        assert!(NroLoader::collect_exports(base, &mmu).is_empty());
    }

    #[test]
    fn test_tls_relocations_static_layout() {
        let mut mmu = Memory::new();
        let provider = 0x8000_0000u64;
        let consumer = 0x8001_0000u64;
        let resolver = 0x8002_0000u64;

        // Provider's TLS block: `tls_a` at 0 (8 bytes), `tls_b` at 8 (4 bytes) -> 12 bytes.
        write_test_module_with(
            &mut mmu,
            provider,
            &[tls_sym("tls_a", 1, 0, 8), tls_sym("tls_b", 1, 8, 4)],
            &[],
            &[],
            &[],
            false,
        );
        // Consumer has its own 0x20-byte block (`tls_c`) and imports `tls_b`.
        write_test_module_with(
            &mut mmu,
            consumer,
            &[tls_sym("tls_c", 1, 0, 0x20), tls_sym("tls_b", 0, 0, 0)],
            &[
                (0x300, R_AARCH64_TLS_TPREL64, 2, 0),
                (0x308, R_AARCH64_TLS_DTPMOD64, 2, 0),
                (0x310, R_AARCH64_TLS_DTPREL64, 2, 2),
                (0x318, R_AARCH64_TLS_TPREL64, 1, 4),
                (0x330, R_AARCH64_TLS_DTPMOD64, 0, 0),
            ],
            &[(0x320, R_AARCH64_TLSDESC, 2, 0)],
            &[],
            false,
        );

        // TLS symbols are not ordinary exports.
        assert!(NroLoader::collect_exports(provider, &mmu).is_empty());

        // Without a TLS layout the relocations are reported, not guessed.
        let report = NroLoader::apply_relocations(consumer, &mut mmu, &HashMap::new());
        assert_eq!(report.applied, 0);
        assert!(report.unresolved.is_empty());
        assert!(report.unsupported.contains(&R_AARCH64_TLS_TPREL64));
        assert!(report.unsupported.contains(&R_AARCH64_TLSDESC));

        let mut layout = NroLoader::build_tls_layout(&[provider, consumer], &mmu);
        let fallback = |module_id, tp_offset, size| TlsBlock { module_id, tp_offset, size, align: 16, init_addr: 0, init_size: 0 };
        assert_eq!(layout.blocks[&provider], fallback(1, 16, 12));
        // 16 + 12 = 28, aligned up to 32.
        assert_eq!(layout.blocks[&consumer], fallback(2, 32, 0x20));
        assert_eq!(layout.total_size, 64);
        assert_eq!(layout.exports.get("tls_b"), Some(&(provider, 8)));

        NroLoader::install_tlsdesc_resolver(&mut mmu, resolver).unwrap();
        layout.tlsdesc_resolver = Some(resolver);

        let report = NroLoader::apply_relocations_with_tls(consumer, &mut mmu, &HashMap::new(), &layout);
        assert_eq!(report.applied, 6);
        assert!(report.unresolved.is_empty());
        assert!(report.unsupported.is_empty());

        assert_eq!(mmu.read_u64(consumer + 0x300).unwrap(), 16 + 8);
        assert_eq!(mmu.read_u64(consumer + 0x308).unwrap(), 1);
        assert_eq!(mmu.read_u64(consumer + 0x310).unwrap(), 8 + 2);
        assert_eq!(mmu.read_u64(consumer + 0x318).unwrap(), 32 + 4);
        assert_eq!(mmu.read_u64(consumer + 0x320).unwrap(), resolver);
        assert_eq!(mmu.read_u64(consumer + 0x328).unwrap(), 16 + 8);
        assert_eq!(mmu.read_u64(consumer + 0x330).unwrap(), 2);

        // The guest resolver returns the TP offset from the descriptor.
        let mut cpu = crate::cpu::Cpu::new();
        cpu.regs.write_x(0, consumer + 0x320);
        let tp_offset = crate::loader::init::call_guest_function(&mut cpu, &mut mmu, resolver, 0x9000_0000, 16).unwrap();
        assert_eq!(tp_offset, 24);
    }

    #[test]
    fn test_tls_unresolved_and_weak_imports() {
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        let weak = TestSymbol { name: "tls_weak", shndx: 0, bind: 2, kind: STT_TLS, value: 0, size: 0 };
        write_test_module_with(
            &mut mmu,
            base,
            &[tls_sym("own", 1, 0, 8), tls_sym("tls_missing", 0, 0, 0), weak],
            &[(0x300, R_AARCH64_TLS_TPREL64, 2, 0), (0x308, R_AARCH64_TLS_DTPMOD64, 3, 0)],
            &[],
            &[],
            false,
        );
        mmu.write_u64(base + 0x300, 0xDEAD_BEEF).unwrap();
        mmu.write_u64(base + 0x308, 0xDEAD_BEEF).unwrap();
        let layout = NroLoader::build_tls_layout(&[base], &mmu);
        let report = NroLoader::apply_relocations_with_tls(base, &mut mmu, &HashMap::new(), &layout);
        assert_eq!(report.unresolved, vec!["tls_missing".to_string()]);
        assert_eq!(report.applied, 1);
        assert_eq!(mmu.read_u64(base + 0x300).unwrap(), 0xDEAD_BEEF);
        assert_eq!(mmu.read_u64(base + 0x308).unwrap(), 0);
    }

    #[test]
    fn test_parse_pt_tls() {
        let tls = TlsTemplate { vaddr: 0x1800, filesz: 8, memsz: 0x30, align: 64 };
        assert_eq!(NroLoader::parse_pt_tls(&elf_with_pt_tls(tls)), Some(tls));
        assert_eq!(NroLoader::parse_pt_tls(b"NSO0 not an elf file, padded out to sixty-four bytes........"), None);
        let mut no_tls = elf_with_pt_tls(tls);
        no_tls[0x78] = 1; // second header becomes PT_LOAD
        assert_eq!(NroLoader::parse_pt_tls(&no_tls), None);
    }

    #[test]
    fn test_pt_tls_template_initialises_tdata_and_zeroes_tbss() {
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        // One TLS symbol at offset 0; the template is larger (memsz 0x30) and 64-byte aligned.
        write_test_module_with(&mut mmu, base, &[tls_sym("counter", 1, 0, 8)], &[], &[], &[], false);
        // .tdata at +0x1800: 8 initialised bytes. The bytes after it in the image must not leak into .tbss.
        mmu.write_u64(base + 0x1800, 0x1122_3344_5566_7788).unwrap();
        mmu.write_u64(base + 0x1808, 0xFFFF_FFFF_FFFF_FFFF).unwrap();

        let tls = NroLoader::parse_pt_tls(&elf_with_pt_tls(TlsTemplate { vaddr: 0x1800, filesz: 8, memsz: 0x30, align: 64 })).unwrap();
        let layout = NroLoader::build_tls_layout_with_templates(&[base], &mmu, &HashMap::from([(base, tls)]));
        let block = layout.blocks[&base];
        assert_eq!(block.tp_offset, 64); // TCB (16) rounded up to p_align
        assert_eq!(block.size, 0x30);
        assert_eq!(layout.align, 64);
        assert_eq!(layout.total_size, 64 + 0x30);

        let image = NroLoader::tls_thread_image(&layout, &mmu);
        assert_eq!(image.align, 64);
        assert_eq!(LittleEndian::read_u64(&image.bytes[64..72]), 0x1122_3344_5566_7788);
        assert!(image.bytes[72..].iter().all(|&b| b == 0));
        assert!(image.bytes[..64].iter().all(|&b| b == 0));

        let tp = 0x9000_0000u64;
        NroLoader::map_static_tls(&mut mmu, tp, &image).unwrap();
        assert_eq!(mmu.read_u64(tp + 64).unwrap(), 0x1122_3344_5566_7788);
        assert_eq!(mmu.read_u64(tp + 72).unwrap(), 0);
    }

    #[test]
    fn test_rel_table_uses_implicit_addend() {
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        write_test_module_with(
            &mut mmu,
            base,
            &[TestSymbol { name: "target", shndx: 1, bind: 1, kind: 2, value: 0x500, size: 0 }],
            &[],
            &[],
            &[(DT_REL, 0xC00), (DT_RELSZ, 32), (DT_RELENT, 16)],
            false,
        );
        // Elf64_Rel: r_offset, r_info. The addend is the value already at r_offset.
        mmu.write_u64(base + 0xC00, 0x300).unwrap();
        mmu.write_u64(base + 0xC08, R_AARCH64_RELATIVE as u64).unwrap();
        mmu.write_u64(base + 0xC10, 0x308).unwrap();
        mmu.write_u64(base + 0xC18, (1u64 << 32) | R_AARCH64_ABS64 as u64).unwrap();
        mmu.write_u64(base + 0x300, 0x450).unwrap();
        mmu.write_u64(base + 0x308, 0x10).unwrap();

        let info = NroLoader::parse_dynamic(base, &mmu).unwrap();
        assert_eq!((info.rel, info.rel_size, info.rel_entry_size), (Some(0xC00), 32, 16));

        let report = NroLoader::apply_relocations(base, &mut mmu, &HashMap::new());
        assert_eq!(report.applied, 2);
        assert_eq!(mmu.read_u64(base + 0x300).unwrap(), base + 0x450);
        assert_eq!(mmu.read_u64(base + 0x308).unwrap(), base + 0x510);
    }

    #[test]
    fn test_jmprel_in_rel_format() {
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        write_test_module_with(
            &mut mmu,
            base,
            &[],
            &[],
            &[],
            &[(DT_PLTREL, DT_REL as u64), (DT_JMPREL, 0xC00), (DT_PLTRELSZ, 16)],
            false,
        );
        mmu.write_u64(base + 0xC00, 0x300).unwrap();
        mmu.write_u64(base + 0xC08, R_AARCH64_RELATIVE as u64).unwrap();
        mmu.write_u64(base + 0x300, 0x20).unwrap();
        assert_eq!(NroLoader::apply_mod0_relocations(base, &mut mmu), 1);
        assert_eq!(mmu.read_u64(base + 0x300).unwrap(), base + 0x20);
    }

    #[test]
    fn test_init_and_fini_function_lists() {
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        write_test_module_with(
            &mut mmu,
            base,
            &[],
            &[],
            &[],
            &[
                (DT_INIT, 0x1000),
                (DT_INIT_ARRAY, 0xC00),
                (DT_INIT_ARRAYSZ, 24),
                (DT_PREINIT_ARRAY, 0xC40),
                (DT_PREINIT_ARRAYSZ, 8),
                (DT_FINI_ARRAY, 0xC80),
                (DT_FINI_ARRAYSZ, 16),
                (DT_FINI, 0x1100),
            ],
            false,
        );
        // Arrays hold relocated absolute addresses; 0 and -1 entries are skipped.
        mmu.write_u64(base + 0xC00, base + 0x1020).unwrap();
        mmu.write_u64(base + 0xC08, u64::MAX).unwrap();
        mmu.write_u64(base + 0xC10, base + 0x1040).unwrap();
        mmu.write_u64(base + 0xC40, base + 0x1060).unwrap();
        mmu.write_u64(base + 0xC80, base + 0x1080).unwrap();
        mmu.write_u64(base + 0xC88, base + 0x10A0).unwrap();

        assert_eq!(
            NroLoader::init_functions(base, &mmu, true),
            vec![base + 0x1060, base + 0x1000, base + 0x1020, base + 0x1040]
        );
        assert_eq!(
            NroLoader::init_functions(base, &mmu, false),
            vec![base + 0x1000, base + 0x1020, base + 0x1040]
        );
        assert_eq!(
            NroLoader::fini_functions(base, &mmu),
            vec![base + 0x10A0, base + 0x1080, base + 0x1100]
        );
    }

    #[test]
    fn test_cross_module_imports_and_exports() {
        let mut mmu = Memory::new();
        let provider = 0x8000_0000u64;
        let consumer = 0x8001_0000u64;

        // Provider defines `ounce_add` at +0x500.
        write_test_module(&mut mmu, provider, &[("ounce_add", 1, 1, 0x500)], &[], &[]);

        // Consumer imports `ounce_add` (GLOB_DAT, JUMP_SLOT, ABS64 + 0x10), an undefined weak symbol,
        // a missing strong symbol, and uses an unimplemented relocation type (9999).
        write_test_module(
            &mut mmu,
            consumer,
            &[("ounce_add", 0, 1, 0), ("weak_missing", 0, 2, 0), ("missing_fn", 0, 1, 0)],
            &[
                (0x300, R_AARCH64_GLOB_DAT, 1, 0),
                (0x310, R_AARCH64_ABS64, 1, 0x10),
                (0x318, R_AARCH64_GLOB_DAT, 2, 0),
                (0x320, R_AARCH64_GLOB_DAT, 3, 0),
                (0x328, 9999, 0, 0),
            ],
            &[(0x308, R_AARCH64_JUMP_SLOT, 1, 0)],
        );
        mmu.write_u64(consumer + 0x320, 0xDEAD_BEEF).unwrap();

        let exports = NroLoader::collect_exports(provider, &mmu);
        assert_eq!(exports.len(), 1);
        assert_eq!(exports.get("ounce_add"), Some(&(provider + 0x500)));

        let report = NroLoader::apply_relocations(consumer, &mut mmu, &exports);
        assert_eq!(report.applied, 4);
        assert_eq!(report.unresolved, vec!["missing_fn".to_string()]);
        assert_eq!(report.unsupported, vec![9999]);

        assert_eq!(mmu.read_u64(consumer + 0x300).unwrap(), provider + 0x500);
        assert_eq!(mmu.read_u64(consumer + 0x308).unwrap(), provider + 0x500);
        assert_eq!(mmu.read_u64(consumer + 0x310).unwrap(), provider + 0x510);
        assert_eq!(mmu.read_u64(consumer + 0x318).unwrap(), 0);
        // Unresolved slot is left untouched rather than written with a bogus address.
        assert_eq!(mmu.read_u64(consumer + 0x320).unwrap(), 0xDEAD_BEEF);
    }

    #[test]
    fn test_mod0_relocations() {
        let mut mmu = Memory::new();
        let base_addr = 0x80000000u64;
        mmu.map_range(base_addr, 0x2000, Permission::READ_WRITE_EXECUTE);

        // Set MOD0 offset at base + 0x04 -> points to 0x100
        mmu.write_u32(base_addr + 4, 0x100).unwrap();
        // Write "MOD0" at base + 0x100 and dynamic offset +0x20 -> 0x120
        mmu.write_bytes(base_addr + 0x100, b"MOD0").unwrap();
        mmu.write_u32(base_addr + 0x104, 0x20).unwrap();

        // Write .dynamic table at base + 0x120:
        // Entry 0: DT_RELA (7) = 0x200
        mmu.write_u64(base_addr + 0x120, DT_RELA as u64).unwrap();
        mmu.write_u64(base_addr + 0x128, 0x200).unwrap();
        // Entry 1: DT_RELASZ (8) = 24 (1 entry)
        mmu.write_u64(base_addr + 0x130, DT_RELASZ as u64).unwrap();
        mmu.write_u64(base_addr + 0x138, 24).unwrap();
        // Entry 2: DT_NULL (0)
        mmu.write_u64(base_addr + 0x140, 0).unwrap();

        // Write Elf64_Rela entry at base + 0x200:
        // r_offset = 0x300, r_info = R_AARCH64_RELATIVE (1027), r_addend = 0x450
        mmu.write_u64(base_addr + 0x200, 0x300).unwrap();
        mmu.write_u64(base_addr + 0x208, R_AARCH64_RELATIVE as u64).unwrap();
        mmu.write_u64(base_addr + 0x210, 0x450).unwrap();

        let count = NroLoader::apply_mod0_relocations(base_addr, &mut mmu);
        assert_eq!(count, 1);

        // Verify pointer at base + 0x300 was relocated to base_addr + 0x450 (0x80000450)
        assert_eq!(mmu.read_u64(base_addr + 0x300).unwrap(), 0x80000450);
    }

    #[test]
    fn relr_address_and_bitmap_entries() {
        let mut mmu = Memory::new();
        let base = 0x8000_0000u64;
        write_test_module_with(&mut mmu, base, &[], &[], &[], &[(DT_RELR, 0xC00), (DT_RELRSZ, 16)], false);
        // Address entry for +0x1000, then a bitmap over the following words: words 0 and 2 (+0x1008, +0x1018).
        mmu.write_u64(base + 0xC00, 0x1000).unwrap();
        mmu.write_u64(base + 0xC08, 1 | 1 << 1 | 1 << 3).unwrap();
        mmu.map_range(base + 0x1000, 0x1000, Permission::READ_WRITE);
        for (i, v) in [0x10u64, 0x20, 0x30, 0x40].iter().enumerate() {
            mmu.write_u64(base + 0x1000 + i as u64 * 8, *v).unwrap();
        }

        assert_eq!(NroLoader::parse_dynamic(base, &mmu).unwrap().relr, Some(0xC00));
        assert_eq!(NroLoader::apply_relr(base, &mut mmu), 3);
        assert_eq!(mmu.read_u64(base + 0x1000).unwrap(), base + 0x10);
        assert_eq!(mmu.read_u64(base + 0x1008).unwrap(), base + 0x20);
        assert_eq!(mmu.read_u64(base + 0x1010).unwrap(), 0x30);
        assert_eq!(mmu.read_u64(base + 0x1018).unwrap(), base + 0x40);
        // RELA-only paths leave RELR words alone.
        assert_eq!(NroLoader::apply_mod0_relocations(base, &mut mmu), 0);
        assert_eq!(mmu.read_u64(base + 0x1000).unwrap(), base + 0x10);
    }

    #[test]
    fn test_homebrew_environment_setup() {
        let mut cpu = crate::cpu::Cpu::new();
        let mut mmu = Memory::new();
        let env_addr = 0x7FFF_FE00u64;

        HomebrewEnvironment::setup(&mut cpu, &mut mmu, env_addr, &["sdmc:/block_fall.nro", "--debug"]).unwrap();

        assert_eq!(cpu.regs.read_x(0), env_addr);
        assert_eq!(cpu.regs.read_x(1), 0xFFFF_FFFF_FFFF_FFFF);

        let mut magic = [0u8; 4];
        mmu.read_bytes(env_addr, &mut magic).unwrap();
        assert_eq!(&magic, b"ARGV");

        let argc = mmu.read_u32(env_addr + 4).unwrap();
        assert_eq!(argc, 2);

        let arg0_ptr = mmu.read_u64(env_addr + 8).unwrap();
        let mut str_buf = [0u8; 21];
        mmu.read_bytes(arg0_ptr, &mut str_buf).unwrap();
        assert_eq!(&str_buf, b"sdmc:/block_fall.nro\0");
    }
}
