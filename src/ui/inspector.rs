#![allow(dead_code)]

use crate::loader::BootLoader;
use crate::mmu::Memory;

/// Formats byte counts into human-readable strings (KB, MB, GB).
pub fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Report for a homebrew executable: format, code size and AArch64 security extensions.
#[derive(Debug, Clone)]
pub struct PackageReport {
    pub file_path: String,
    pub total_file_size: usize,
    pub format: String,
    pub code_size: usize,
    pub security_features: Vec<String>,
}

/// Standalone inspector for homebrew `.nro`/`.nso` executables.
pub struct PackageInspector;

impl PackageInspector {
    /// Inspects a homebrew `.nro`/`.nso` executable and generates a report.
    pub fn inspect_bytes(file_path: &str, raw: &[u8]) -> Result<PackageReport, String> {
        let is_nso = raw.starts_with(b"NSO0");
        let is_nro = raw.len() >= 0x14 && &raw[0x10..0x14] == b"NRO0";
        if !is_nso && !is_nro {
            return Err(format!(
                "unrecognized file format for '{}' (expected Switch NRO0 or NSO0 executable)",
                file_path
            ));
        }

        let mut mmu = Memory::new();
        let info = if is_nso {
            if raw.len() < 0x100 {
                return Err("truncated NSO0 header (< 256 bytes)".to_string());
            }
            BootLoader::load_any_package(raw, 0x8000_0000, &mut mmu)
                .map_err(|e| format!("invalid NSO0 executable: {:?}", e))?
        } else {
            BootLoader::load_homebrew_nro(raw, 0x8000_0000, &mut mmu)
                .map_err(|e| format!("invalid NRO0 executable: {:?}", e))?
        };

        let mut security_features = Vec::new();
        let mut has_pac = false;
        let mut has_bti = false;

        // Scan 32-bit words for ARMv8.3/8.5 security extensions (PAC, BTI)
        for chunk in raw.chunks_exact(4) {
            let inst = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            match inst {
                0xD503237F | 0xD503233F | 0xD50323FF | 0xD65F0FFF | 0xD65F0BFF => {
                    has_pac = true;
                }
                0xD503245F | 0xD503241F | 0xD50324DF => {
                    has_bti = true;
                }
                _ => {}
            }
        }
        if has_pac {
            security_features.push("ARMv8.3-A PAC (Pointer Authentication)".to_string());
        }
        if has_bti {
            security_features.push("ARMv8.5-A BTI (Branch Target Identification)".to_string());
        }
        if security_features.is_empty() {
            security_features.push("Standard AArch64 (ARMv8.0-A)".to_string());
        }

        Ok(PackageReport {
            file_path: file_path.to_string(),
            total_file_size: raw.len(),
            format: info.format.to_string(),
            code_size: info.code_size,
            security_features,
        })
    }

    /// Prints a formatted terminal report.
    pub fn print_report(report: &PackageReport) {
        println!("============================================================");
        println!("  HOMEBREW EXECUTABLE INSPECTOR");
        println!("============================================================");
        println!("  File Path     : {}", report.file_path);
        println!("  File Size     : {} ({} bytes)", format_bytes(report.total_file_size), report.total_file_size);
        println!("  Format        : {}", report.format);
        println!("  Code (.text)  : {} ({} bytes)", format_bytes(report.code_size), report.code_size);
        println!("  ISA Security  : {}", report.security_features.join(" | "));
        println!("============================================================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_reports_synthetic_nso() {
        let nso = crate::loader::NroLoader::synthetic_nso_payload();
        let report = PackageInspector::inspect_bytes("synthetic.nso", &nso).unwrap();
        assert_eq!(report.format, "NSO0 (Nintendo Shared Object)");
        assert!(report.code_size > 0);
    }

    #[test]
    fn inspector_rejects_non_switch_bytes() {
        let err = PackageInspector::inspect_bytes("plain.txt", b"this is plain text, not an NRO binary\n").unwrap_err();
        assert!(err.contains("unrecognized file format"), "unexpected error: {err}");

        let err_elf = PackageInspector::inspect_bytes("prog.elf", b"\x7fELF\x02\x01\x01\x00").unwrap_err();
        assert!(err_elf.contains("unrecognized file format"), "unexpected error: {err_elf}");
    }

    #[test]
    fn inspector_rejects_truncated_nro_and_nso() {
        let mut hdr = vec![0u8; 0x100];
        hdr[0x10..0x14].copy_from_slice(b"NRO0");
        hdr[0x18..0x1C].copy_from_slice(&0x3000u32.to_le_bytes());
        hdr[0x24..0x28].copy_from_slice(&0x1000u32.to_le_bytes());
        let err = PackageInspector::inspect_bytes("truncated.nro", &hdr).unwrap_err();
        assert!(err.contains("invalid NRO0"), "unexpected error: {err}");

        let err_nso = PackageInspector::inspect_bytes("truncated.nso", b"NSO0\x00\x00").unwrap_err();
        assert!(err_nso.contains("truncated NSO0"), "unexpected error: {err_nso}");
    }
}
