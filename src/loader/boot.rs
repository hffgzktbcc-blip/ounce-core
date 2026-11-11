#![allow(dead_code)]

use crate::loader::nro::{LoadError, NroLoader};
use crate::mmu::{Memory, Permission};

#[derive(Debug, Clone)]
pub struct BootPackageInfo {
    pub format: &'static str,
    pub title_id: u64,
    pub code_size: usize,
    pub relocations_applied: usize,
}

/// Homebrew boot loader: detects `.nro` (`NRO0`), `.nso` (`NSO0`) or raw AArch64
/// images and loads the executable into guest RAM at `base_addr`.
/// Only unencrypted homebrew formats are supported.
pub struct BootLoader;

impl BootLoader {
    pub fn load_homebrew_nro(
        bytes: &[u8],
        base_addr: u64,
        mmu: &mut Memory,
    ) -> Result<BootPackageInfo, LoadError> {
        Self::load_nro_executable(bytes, base_addr, 0, "NRO0", mmu)
    }

    fn load_nro_executable(
        bytes: &[u8],
        base_addr: u64,
        title_id: u64,
        format: &'static str,
        mmu: &mut Memory,
    ) -> Result<BootPackageInfo, LoadError> {
        let hdr = NroLoader::load_into_memory(bytes, base_addr, mmu)?;
        Ok(BootPackageInfo {
            format,
            title_id,
            code_size: hdr.text_size as usize,
            relocations_applied: hdr.relocations_applied,
        })
    }

    /// Loads a homebrew `.nro`, `.nso` or raw AArch64 image at `base_addr`.
    pub fn load_any_package(
        raw: &[u8],
        base_addr: u64,
        mmu: &mut Memory,
    ) -> Result<BootPackageInfo, LoadError> {
        Self::load_executable_bytes(raw, base_addr, 0, "NRO0", mmu)
    }

    fn load_executable_bytes(
        exec_bytes: &[u8],
        base_addr: u64,
        title_id: u64,
        format: &'static str,
        mmu: &mut Memory,
    ) -> Result<BootPackageInfo, LoadError> {
        // Check if the executable is an NSO0 binary (magic "NSO0" at offset 0x00)
        if exec_bytes.len() >= 0x100 && (&exec_bytes[0x00..0x04] == b"NSO0") {
            let hdr = NroLoader::load_nso_into_memory(exec_bytes, base_addr, mmu)?;
            return Ok(BootPackageInfo {
                format: "NSO0 (Nintendo Shared Object)",
                title_id: if title_id != 0 {
                    title_id
                } else {
                    0x0400_0000_0000_0037
                },
                code_size: hdr.text_size as usize,
                relocations_applied: hdr.relocations_applied,
            });
        }

        // Check if the executable is an NRO0 binary (magic "NRO0" at offset 0x10)
        if exec_bytes.len() >= 0x80 && (&exec_bytes[0x10..0x14] == b"NRO0") {
            return Self::load_nro_executable(exec_bytes, base_addr, title_id, format, mmu);
        }

        // Otherwise map raw AArch64 machine code directly at base_addr
        let aligned_size = ((exec_bytes.len() + 0xFFF) & !0xFFF).max(4096);
        mmu.map_range(base_addr, aligned_size, Permission::READ_WRITE_EXECUTE);
        mmu.write_bytes(base_addr, exec_bytes)?;

        Ok(BootPackageInfo {
            format,
            title_id,
            code_size: exec_bytes.len(),
            relocations_applied: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_aarch64_image_is_mapped_at_base() {
        let code: Vec<u8> = [0xD2800000u32, 0xD40000E1].iter().flat_map(|w| w.to_le_bytes()).collect();
        let mut mmu = Memory::new();
        let info = BootLoader::load_any_package(&code, 0x8000_0000, &mut mmu).unwrap();
        assert_eq!(info.code_size, code.len());
        assert_eq!(mmu.read_u32(0x8000_0000).unwrap(), 0xD2800000);
    }

    #[test]
    fn synthetic_nso_boots_through_boot_loader() {
        let nso = NroLoader::synthetic_nso_payload();
        let mut mmu = Memory::new();
        let info = BootLoader::load_any_package(&nso, 0x8000_0000, &mut mmu).unwrap();
        assert_eq!(info.format, "NSO0 (Nintendo Shared Object)");
        assert!(info.code_size > 0);
    }
}
