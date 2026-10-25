#![allow(dead_code)]

use crate::fs::RomFsArchive;
use crate::loader::{BootLoader, NcaImage, Pfs0Archive, XciCartridge};
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

/// Package Inspector Report holding extracted metadata and asset manifests.
#[derive(Debug, Clone)]
pub struct PackageReport {
    pub file_path: String,
    pub total_file_size: usize,
    pub format: String,
    pub title_id: u64,
    pub code_size: usize,
    pub romfs_files: Vec<(String, usize)>,
}

/// Standalone Nintendo Switch 2 Package & RomFS Asset Inspector.
pub struct PackageInspector;

impl PackageInspector {
    /// Inspects any `.xci`, `.nsp`, `.nca`, or `.nro` package and generates a detailed report.
    pub fn inspect_bytes(file_path: &str, raw: &[u8], key: &[u8; 16]) -> Result<PackageReport, &'static str> {
        let total_file_size = raw.len();
        let mut romfs_files = Vec::new();
        let mut format = "Unknown".to_string();
        let mut title_id = 0u64;
        let mut code_size = 0usize;

        // Check if XCI
        if raw.len() >= 0x200 && (&raw[0x100..0x104] == b"HEAD") {
            let xci = XciCartridge::parse(raw).map_err(|_| "Failed to parse XCI GameCard header")?;
            format = format!("XCI GameCard (Size Code 0x{:02X}, Partition Root HFS0)", xci.cart_size_code);

            if let Ok(secure) = xci.open_secure_partition() {
                for nca_name in secure.list_files() {
                    if nca_name.ends_with(".nca") {
                        if let Some(nca_raw) = secure.extract_file(nca_name) {
                            if let Ok(nca) = NcaImage::parse_and_decrypt(nca_raw, key) {
                                title_id = nca.title_id;
                                code_size = nca.section_size;
                                if let Some(ref r_bytes) = nca.decrypted_romfs {
                                    if let Ok(archive) = RomFsArchive::parse(r_bytes) {
                                        for entry in archive.entries {
                                            romfs_files.push((entry.path, entry.data.len()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else if raw.len() >= 16 && (&raw[0..4] == b"PFS0") {
            // NSP Package
            format = "NSP (Nintendo Submission Package / PFS0)".to_string();
            let pfs0 = Pfs0Archive::parse(raw.to_vec()).map_err(|_| "Failed to parse PFS0")?;
            for name in pfs0.list_files() {
                if name.ends_with(".nca") {
                    if let Some(nca_raw) = pfs0.extract_file(name) {
                        if let Ok(nca) = NcaImage::parse_and_decrypt(nca_raw, key) {
                            title_id = nca.title_id;
                            code_size = nca.section_size;
                            if let Some(ref r_bytes) = nca.decrypted_romfs {
                                if let Ok(archive) = RomFsArchive::parse(r_bytes) {
                                    for entry in archive.entries {
                                        romfs_files.push((entry.path, entry.data.len()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Try loading through Unified BootLoader
            let mut mmu = Memory::new();
            if let Ok(info) = BootLoader::load_any_package(raw, 0x8000_0000, key, &mut mmu) {
                format = info.format.to_string();
                title_id = info.title_id;
                code_size = info.code_size;
            }
        }

        Ok(PackageReport {
            file_path: file_path.to_string(),
            total_file_size,
            format,
            title_id,
            code_size,
            romfs_files,
        })
    }

    /// Prints a beautiful formatted ASCII terminal report for a package.
    pub fn print_report(report: &PackageReport) {
        println!("============================================================");
        println!("  🔍 SWITCH 2 PACKAGE & RomFS ASSET INSPECTOR");
        println!("============================================================");
        println!("  File Path     : {}", report.file_path);
        println!("  Package Size  : {} ({} bytes)", format_bytes(report.total_file_size), report.total_file_size);
        println!("  Format        : {}", report.format);
        println!("  Title ID      : 0x{:016X}", report.title_id);
        println!("  Code (.text)  : {} ({} bytes)", format_bytes(report.code_size), report.code_size);
        println!("------------------------------------------------------------");
        if report.romfs_files.is_empty() {
            println!("  RomFS Assets  : None or Section 1 not present.");
        } else {
            println!("  RomFS Level 3 Assets ({} files):", report.romfs_files.len());
            for (path, size) in &report.romfs_files {
                println!("    ├── {:<42} [{:>9}]", path, format_bytes(*size));
            }
        }
        println!("============================================================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_inspector_romfs_extraction() {
        let key: [u8; 16] = [0x2b; 16];
        let iv0: [u8; 16] = [0x10; 16];
        let iv1: [u8; 16] = [0x20; 16];

        let romfs = RomFsArchive::build(&[
            ("/textures/ui.png", &[0x89, 0x50, 0x4E, 0x47]),
            ("/audio/bgm.wav", &[0x52, 0x49, 0x46, 0x46]),
        ]);
        let exefs = Pfs0Archive::build(&[("main", b"EXECUTABLE_BYTES")]);
        let nca = NcaImage::build_encrypted_with_romfs(
            0x0100_1234_5678_0000,
            &key,
            &iv0,
            &iv1,
            &exefs,
            Some(&romfs),
        );
        let nsp = Pfs0Archive::build(&[("title.nca", &nca)]);

        let report = PackageInspector::inspect_bytes("test_game.nsp", &nsp, &key).unwrap();
        assert_eq!(report.title_id, 0x0100_1234_5678_0000);
        assert_eq!(report.romfs_files.len(), 2);
        assert_eq!(report.romfs_files[0].0, "/textures/ui.png");
        assert_eq!(report.romfs_files[1].0, "/audio/bgm.wav");
    }
}
