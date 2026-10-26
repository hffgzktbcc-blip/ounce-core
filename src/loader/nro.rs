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
pub const DT_JMPREL: i64 = 23;
pub const R_AARCH64_ABS64: u32 = 257;
pub const R_AARCH64_GLOB_DAT: u32 = 1025;
pub const R_AARCH64_JUMP_SLOT: u32 = 1026;

const RELA_ENTRY_SIZE: u64 = 24;
const SYM_ENTRY_SIZE: u64 = 24;
const SHN_UNDEF: u16 = 0;
const STB_LOCAL: u8 = 0;
const STB_WEAK: u8 = 2;

/// Parsed `.dynamic` section of a MOD0-described module. Pointer values are image-relative offsets.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DynamicInfo {
    pub rela: Option<u64>,
    pub rela_size: u64,
    pub rela_entry_size: u64,
    pub jmprel: Option<u64>,
    pub pltrel_size: u64,
    pub symtab: Option<u64>,
    pub strtab: Option<u64>,
    pub syment: u64,
    pub hash: Option<u64>,
    pub init: Option<u64>,
    pub fini: Option<u64>,
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
    shndx: u16,
    value: u64,
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
    /// (such as Switch 2 `RO` / `FS` system modules and retail ExeFS `main` binaries) and applies
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
                DT_JMPREL => info.jmprel = Some(d_val),
                DT_PLTRELSZ => info.pltrel_size = d_val,
                DT_SYMTAB => info.symtab = Some(d_val),
                DT_STRTAB => info.strtab = Some(d_val),
                DT_SYMENT => info.syment = d_val,
                DT_HASH => info.hash = Some(d_val),
                DT_INIT => info.init = Some(d_val),
                DT_FINI => info.fini = Some(d_val),
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
            shndx: LittleEndian::read_u16(&raw[6..8]),
            value: LittleEndian::read_u64(&raw[8..16]),
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

    /// Symbol count from `DT_HASH` (the `nchain` field). Modules without `DT_HASH` export nothing.
    fn symbol_count(base_addr: u64, mmu: &Memory, info: &DynamicInfo) -> u64 {
        info.hash
            .and_then(|h| mmu.read_u32(base_addr + h + 4).ok())
            .unwrap_or(0) as u64
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
            if sym.shndx == SHN_UNDEF || sym.bind == STB_LOCAL {
                continue;
            }
            let name = Self::symbol_name(base_addr, mmu, &info, sym.name);
            if !name.is_empty() {
                exports.entry(name).or_insert(base_addr + sym.value);
            }
        }
        exports
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

    /// Applies the module's `.rela.dyn` and `.rela.plt` tables. Imports are looked up in `imports`.
    pub fn apply_relocations(
        base_addr: u64,
        mmu: &mut Memory,
        imports: &HashMap<String, u64>,
    ) -> RelocationReport {
        let mut report = RelocationReport::default();
        let info = match Self::parse_dynamic(base_addr, mmu) {
            Some(i) => i,
            None => return report,
        };
        let entry_size = if info.rela_entry_size == 0 { RELA_ENTRY_SIZE } else { info.rela_entry_size };

        let mut tables = Vec::new();
        if let Some(rela) = info.rela {
            tables.push((rela, info.rela_size));
        }
        if let Some(jmprel) = info.jmprel {
            tables.push((jmprel, info.pltrel_size));
        }

        for (table, table_size) in tables {
            for i in 0..table_size / entry_size {
                let entry_addr = base_addr + table + i * entry_size;
                let mut raw = [0u8; 24];
                if mmu.read_bytes(entry_addr, &mut raw).is_err() {
                    continue;
                }
                let r_offset = LittleEndian::read_u64(&raw[0..8]);
                let r_info = LittleEndian::read_u64(&raw[8..16]);
                let r_addend = LittleEndian::read_u64(&raw[16..24]);
                let sym_index = r_info >> 32;
                let r_type = (r_info & 0xFFFF_FFFF) as u32;
                let target_va = base_addr + r_offset;

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

/// Standard Nintendo Switch / Switch 2 Homebrew Environment (`ARGV` parameter block).
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
    mmu.map_range(base, 0x2000, Permission::READ_WRITE_EXECUTE);
    mmu.write_u32(base + 4, 0x100).unwrap();
    mmu.write_bytes(base + 0x100, b"MOD0").unwrap();
    mmu.write_u32(base + 0x104, 0x20).unwrap();

    let entries: [(i64, u64); 10] = [
        (DT_HASH, 0x880),
        (DT_STRTAB, 0x900),
        (DT_SYMTAB, 0x800),
        (DT_SYMENT, 24),
        (DT_RELA, 0xA00),
        (DT_RELASZ, 24 * rela.len() as u64),
        (DT_RELAENT, 24),
        (DT_JMPREL, 0xB00),
        (DT_PLTRELSZ, 24 * jmprel.len() as u64),
        (DT_NULL, 0),
    ];
    for (i, (tag, val)) in entries.iter().enumerate() {
        mmu.write_u64(base + 0x120 + (i as u64) * 16, *tag as u64).unwrap();
        mmu.write_u64(base + 0x128 + (i as u64) * 16, *val).unwrap();
    }

    // DT_HASH: only `nchain` (the symbol count, including the null symbol) matters here.
    mmu.write_u32(base + 0x884, (symbols.len() + 1) as u32).unwrap();

    // Symbol 0 is the null symbol; string offset 0 is the empty string.
    let mut strtab_off = 1u32;
    for (i, (name, shndx, bind, value)) in symbols.iter().enumerate() {
        let sym = base + 0x800 + ((i as u64) + 1) * 24;
        mmu.write_u32(sym, strtab_off).unwrap();
        mmu.write_bytes(sym + 4, &[(bind << 4) | 0x2]).unwrap();
        mmu.write_bytes(sym + 6, &shndx.to_le_bytes()).unwrap();
        mmu.write_u64(sym + 8, *value).unwrap();
        mmu.write_bytes(base + 0x900 + strtab_off as u64, name.as_bytes()).unwrap();
        mmu.write_bytes(base + 0x900 + strtab_off as u64 + name.len() as u64, &[0]).unwrap();
        strtab_off += name.len() as u32 + 1;
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
    const IMAGE_SIZE: usize = 0x2000;
    let scratch_base = 0x8000_0000u64;
    let mut scratch = Memory::new();
    write_test_module(&mut scratch, scratch_base, symbols, rela, jmprel);
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

#[cfg(test)]
mod tests {
    use super::*;

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
