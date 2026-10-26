#![allow(dead_code)]
use byteorder::{ByteOrder, LittleEndian};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use crate::kernel::HorizonKernel;
use crate::loader::nro::{NroHeader, NroLoader};
use crate::mmu::Memory;

pub const NPDM_MAGIC: [u8; 4] = *b"META";
pub const ACI0_MAGIC: [u8; 4] = *b"ACI0";
pub const ACID_MAGIC: [u8; 4] = *b"ACID";

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("Directory not found or inaccessible: {0}")]
    NotFound(String),

    #[error("Main executable 'main' not found in exefs directory: {0}")]
    MissingMainExecutable(String),

    #[error("Invalid NPDM metadata: {0}")]
    InvalidNpdm(String),

    #[error("NSO loader error: {0}")]
    NsoLoad(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Memory mapping error: {0}")]
    Memory(#[from] crate::mmu::MemoryError),
}

/// Parsed Nintendo Process Descriptor Metadata (NPDM / `main.npdm`).
#[derive(Debug, Clone)]
pub struct NpdmDescriptor {
    pub process_name: String,
    pub product_code: String,
    pub title_id: u64,
    pub flags: u32,
    pub main_thread_priority: u8,
    pub default_cpu_core: u8,
    pub system_resource_size: u32,
    pub allowed_services: Vec<String>,
    pub allowed_syscalls_mask: u64,
}

impl NpdmDescriptor {
    /// Parses raw bytes from a `main.npdm` file.
    pub fn parse(bytes: &[u8]) -> Result<Self, PackageError> {
        if bytes.len() < 0x80 {
            return Err(PackageError::InvalidNpdm("File too small (< 128 bytes)".into()));
        }

        if &bytes[0x00..0x04] != &NPDM_MAGIC {
            return Err(PackageError::InvalidNpdm(format!(
                "Invalid magic: expected 'META', got {:?}",
                &bytes[0x00..0x04]
            )));
        }

        let flags = LittleEndian::read_u32(&bytes[0x0C..0x10]);
        let main_thread_priority = bytes[0x0D];
        let default_cpu_core = bytes[0x0E];
        let system_resource_size = LittleEndian::read_u32(&bytes[0x10..0x14]);

        // Process Name (12 bytes at 0x14)
        let name_slice = &bytes[0x14..0x20];
        let process_name = String::from_utf8_lossy(
            name_slice.split(|&b| b == 0).next().unwrap_or(name_slice),
        )
        .trim()
        .to_string();

        // Product Code (16 bytes at 0x20)
        let code_slice = &bytes[0x20..0x30];
        let product_code = String::from_utf8_lossy(
            code_slice.split(|&b| b == 0).next().unwrap_or(code_slice),
        )
        .trim()
        .to_string();

        let aci0_offset = LittleEndian::read_u32(&bytes[0x30..0x34]) as usize;
        let aci0_size = LittleEndian::read_u32(&bytes[0x34..0x38]) as usize;

        let mut title_id = 0x0100023900002026u64;
        let mut allowed_services = Vec::new();
        let allowed_syscalls_mask = 0xFFFFFFFFFFFFFFFFu64;

        if aci0_offset + 0x20 <= bytes.len() && aci0_size >= 0x20 {
            let aci0_bytes = &bytes[aci0_offset..aci0_offset + aci0_size];
            if aci0_bytes.len() >= 0x20 && &aci0_bytes[0x00..0x04] == &ACI0_MAGIC {
                title_id = LittleEndian::read_u64(&aci0_bytes[0x10..0x18]);

                let sac_offset = LittleEndian::read_u32(&aci0_bytes[0x18..0x1C]) as usize;
                let sac_size = LittleEndian::read_u32(&aci0_bytes[0x1C..0x20]) as usize;

                if sac_offset + sac_size <= aci0_bytes.len() && sac_size > 0 {
                    let sac_data = &aci0_bytes[sac_offset..sac_offset + sac_size];
                    let mut cursor = 0;
                    while cursor < sac_data.len() {
                        let len = sac_data[cursor] as usize;
                        cursor += 1;
                        if len == 0 || cursor + len > sac_data.len() {
                            break;
                        }
                        if let Ok(srv) = std::str::from_utf8(&sac_data[cursor..cursor + len]) {
                            allowed_services.push(srv.to_string());
                        }
                        cursor += len;
                    }
                }
            }
        }

        // If no explicit SAC services are parsed, provide standard retail default set
        if allowed_services.is_empty() {
            allowed_services = vec![
                "sm:".into(),
                "set:sys".into(),
                "am:appletOE".into(),
                "vi:m".into(),
                "hid".into(),
                "fsp-srv".into(),
                "ro:1".into(),
                "nvdrv".into(),
                "audren:u".into(),
                "audout:u".into(),
            ];
        }

        Ok(Self {
            process_name: if process_name.is_empty() { "Switch2Program".into() } else { process_name },
            product_code: if product_code.is_empty() { "OUNCE-P-BASE".into() } else { product_code },
            title_id,
            flags,
            main_thread_priority,
            default_cpu_core,
            system_resource_size,
            allowed_services,
            allowed_syscalls_mask,
        })
    }

    /// Assembles a clean-room synthetic NPDM binary for testing or packaging homebrew.
    pub fn create_synthetic(
        process_name: &str,
        product_code: &str,
        title_id: u64,
        priority: u8,
        core_id: u8,
        services: &[&str],
    ) -> Vec<u8> {
        let mut buffer = vec![0u8; 0x200];

        // 0x00: Magic 'META'
        buffer[0x00..0x04].copy_from_slice(&NPDM_MAGIC);
        buffer[0x0D] = priority;
        buffer[0x0E] = core_id;
        LittleEndian::write_u32(&mut buffer[0x10..0x14], 0x100000); // 1MB sys resource

        // 0x14: Process name (up to 12 bytes)
        let name_bytes = process_name.as_bytes();
        let copy_len = name_bytes.len().min(12);
        buffer[0x14..0x14 + copy_len].copy_from_slice(&name_bytes[..copy_len]);

        // 0x20: Product code (up to 16 bytes)
        let prod_bytes = product_code.as_bytes();
        let prod_len = prod_bytes.len().min(16);
        buffer[0x20..0x20 + prod_len].copy_from_slice(&prod_bytes[..prod_len]);

        // ACI0 at 0x80
        let aci0_off = 0x80usize;
        LittleEndian::write_u32(&mut buffer[0x30..0x34], aci0_off as u32);
        LittleEndian::write_u32(&mut buffer[0x34..0x38], 0x100); // 256 bytes ACI0

        buffer[aci0_off..aci0_off + 4].copy_from_slice(&ACI0_MAGIC);
        LittleEndian::write_u64(&mut buffer[aci0_off + 0x10..aci0_off + 0x18], title_id);

        // SAC data offset within ACI0 (at +0x30)
        let sac_off = 0x30usize;
        LittleEndian::write_u32(&mut buffer[aci0_off + 0x18..aci0_off + 0x1C], sac_off as u32);

        let mut sac_cursor = aci0_off + sac_off;
        for s in services {
            let sb = s.as_bytes();
            if sac_cursor + 1 + sb.len() < buffer.len() {
                buffer[sac_cursor] = sb.len() as u8;
                sac_cursor += 1;
                buffer[sac_cursor..sac_cursor + sb.len()].copy_from_slice(sb);
                sac_cursor += sb.len();
            }
        }
        let sac_len = (sac_cursor - (aci0_off + sac_off)) as u32;
        LittleEndian::write_u32(&mut buffer[aci0_off + 0x1C..aci0_off + 0x20], sac_len);

        buffer
    }
}

/// Represents a loose package layout on disk (e.g., dumped by OunceCake / Atmosphere / hactool).
#[derive(Debug, Clone)]
pub struct LoosePackage {
    pub root_dir: PathBuf,
    pub exefs_dir: PathBuf,
    pub romfs_dir: Option<PathBuf>,
    pub npdm: Option<NpdmDescriptor>,
    pub main_nso: PathBuf,
    pub companion_nsos: Vec<(String, PathBuf)>,
}

/// Information returned after successfully bootstrapping a loose package into virtual memory.
#[derive(Debug, Clone)]
pub struct LooseBootInfo {
    pub title_id: u64,
    pub process_name: String,
    pub product_code: String,
    pub main_code_size: usize,
    pub modules_loaded: Vec<(String, u64, usize)>,
    pub romfs_files_mounted: usize,
    pub relocations_applied: usize,
    /// Strong imports that no loaded module exports (resolved at runtime by `ro:1` on hardware).
    pub unresolved_imports: Vec<String>,
    pub allowed_services_count: usize,
}

pub struct LoosePackageLoader;

impl LoosePackageLoader {
    /// Discovers and validates a loose directory layout.
    ///
    /// Accepts either:
    /// - The game root folder containing `exefs/` (and optional `romfs/`), or
    /// - Direct path to the `exefs/` folder.
    pub fn discover(target_path: impl AsRef<Path>) -> Result<LoosePackage, PackageError> {
        let path = target_path.as_ref();
        if !path.exists() {
            return Err(PackageError::NotFound(path.to_string_lossy().to_string()));
        }

        let (exefs_dir, romfs_dir) = if path.is_dir() {
            let sub_exefs = path.join("exefs");
            let sub_romfs = path.join("romfs");
            if sub_exefs.is_dir() {
                (
                    sub_exefs,
                    if sub_romfs.is_dir() { Some(sub_romfs) } else { None },
                )
            } else {
                // The given directory is the exefs folder itself
                let sibling_romfs = path.parent().map(|p| p.join("romfs"));
                let romfs = sibling_romfs.filter(|p| p.is_dir());
                (path.to_path_buf(), romfs)
            }
        } else {
            return Err(PackageError::NotFound(format!(
                "'{}' is not a directory. Expected loose package root or exefs folder.",
                path.display()
            )));
        };

        // Locate 'main' or 'main.nso'
        let main_candidate_1 = exefs_dir.join("main");
        let main_candidate_2 = exefs_dir.join("main.nso");
        let main_nso = if main_candidate_1.is_file() {
            main_candidate_1
        } else if main_candidate_2.is_file() {
            main_candidate_2
        } else {
            return Err(PackageError::MissingMainExecutable(
                exefs_dir.to_string_lossy().to_string(),
            ));
        };

        // Locate and parse optional 'main.npdm'
        let npdm_path = exefs_dir.join("main.npdm");
        let npdm = if npdm_path.is_file() {
            match fs::read(&npdm_path) {
                Ok(bytes) => NpdmDescriptor::parse(&bytes).ok(),
                Err(_) => None,
            }
        } else {
            None
        };

        // Discover companion NSO modules (sdk, subsdk0..subsdk9, rtld)
        let mut companion_nsos = Vec::new();
        if let Ok(entries) = fs::read_dir(&exefs_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let file_name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let stem = file_name.trim_end_matches(".nso");
                    if stem != "main" && (stem == "sdk" || stem.starts_with("subsdk") || stem == "rtld") {
                        companion_nsos.push((stem.to_string(), p));
                    }
                }
            }
        }
        companion_nsos.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(LoosePackage {
            root_dir: path.to_path_buf(),
            exefs_dir,
            romfs_dir,
            npdm,
            main_nso,
            companion_nsos,
        })
    }

    /// Loads the discovered package into guest memory, applies relocations across modules,
    /// mounts any RomFS directory assets, and configures kernel service capabilities.
    pub fn load_into_kernel(
        package: &LoosePackage,
        base_addr: u64,
        mmu: &mut Memory,
        kernel: &mut HorizonKernel,
    ) -> Result<LooseBootInfo, PackageError> {
        let mut modules_loaded = Vec::new();

        // 1. Map every module's segments before relocating, so imports can resolve across modules.
        //    `main` sits at `base_addr`; companions (`sdk`, `subsdk*`) follow in 64 MB slots.
        let main_bytes = fs::read(&package.main_nso)?;
        NroLoader::load_nso_segments(&main_bytes, base_addr, mmu)
            .map_err(|e| PackageError::NsoLoad(format!("Failed to load 'main': {}", e)))?;
        modules_loaded.push(("main".to_string(), base_addr, main_bytes.len()));
        let mut module_bases = vec![base_addr];

        for (i, (mod_name, mod_path)) in package.companion_nsos.iter().enumerate() {
            let mod_base = base_addr + ((i as u64 + 1) * 0x04000000);
            if let Ok(mod_bytes) = fs::read(mod_path) {
                if NroLoader::load_nso_segments(&mod_bytes, mod_base, mmu).is_ok() {
                    modules_loaded.push((mod_name.clone(), mod_base, mod_bytes.len()));
                    module_bases.push(mod_base);
                }
            }
        }

        // 2. Build one export table. `main` is first, so its definitions win over duplicates.
        let mut exports: HashMap<String, u64> = HashMap::new();
        for &module_base in &module_bases {
            for (name, addr) in NroLoader::collect_exports(module_base, mmu) {
                exports.entry(name).or_insert(addr);
            }
        }

        // 3. Relocate every module against the combined export table.
        let mut total_relocations = 0usize;
        let mut unresolved_imports: Vec<String> = Vec::new();
        for &module_base in &module_bases {
            let report = NroLoader::apply_relocations(module_base, mmu, &exports);
            total_relocations += report.applied;
            for name in report.unresolved {
                if !unresolved_imports.contains(&name) {
                    unresolved_imports.push(name);
                }
            }
        }

        // 3. Mount RomFS Directory Assets (if present)
        let mut romfs_files_mounted = 0usize;
        if let Some(ref romfs_path) = package.romfs_dir {
            if let Ok(count) = kernel.vfs.mount_host_directory(romfs_path, "romfs") {
                romfs_files_mounted = count;
            }
        }

        // 4. Configure Title and Capabilities from NPDM (or default)
        let (title_id, process_name, product_code, allowed_services_count) = if let Some(ref desc) = package.npdm {
            (
                desc.title_id,
                desc.process_name.clone(),
                desc.product_code.clone(),
                desc.allowed_services.len(),
            )
        } else {
            (
                0x0100023900002026u64,
                "Switch2Game".to_string(),
                "OUNCE-P-BASE".to_string(),
                10,
            )
        };

        Ok(LooseBootInfo {
            title_id,
            process_name,
            product_code,
            main_code_size: main_bytes.len(),
            modules_loaded,
            romfs_files_mounted,
            relocations_applied: total_relocations,
            unresolved_imports,
            allowed_services_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_npdm_synthetic_builder_and_parser() {
        let npdm_bytes = NpdmDescriptor::create_synthetic(
            "Duskbloods",
            "OUNCE-P-DUSK",
            0x0100F2C0115B6000,
            44,
            2,
            &["sm:", "set:sys", "am:appletOE", "vi:m", "hid", "nvdrv"],
        );

        let parsed = NpdmDescriptor::parse(&npdm_bytes).expect("Failed to parse synthetic NPDM");
        assert_eq!(parsed.process_name, "Duskbloods");
        assert_eq!(parsed.product_code, "OUNCE-P-DUSK");
        assert_eq!(parsed.title_id, 0x0100F2C0115B6000);
        assert_eq!(parsed.main_thread_priority, 44);
        assert_eq!(parsed.default_cpu_core, 2);
        assert!(parsed.allowed_services.contains(&"sm:".to_string()));
        assert!(parsed.allowed_services.contains(&"nvdrv".to_string()));
    }

    #[test]
    fn test_companion_imports_resolve_against_main_exports() {
        use crate::loader::nro::{build_linked_nso, R_AARCH64_GLOB_DAT};

        let temp_dir = std::env::temp_dir().join(format!(
            "ounce_test_link_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let exefs = temp_dir.join("exefs");
        fs::create_dir_all(&exefs).unwrap();

        let npdm_bytes = NpdmDescriptor::create_synthetic(
            "LinkDemo",
            "OUNCE-P-LINK",
            0x0100023900002027,
            40,
            0,
            &["sm:", "fsp-srv"],
        );
        fs::write(exefs.join("main.npdm"), &npdm_bytes).unwrap();

        // `main` exports `ounce_add` at +0x500.
        let main_nso = build_linked_nso(&[("ounce_add", 1, 1, 0x500)], &[], &[]);
        fs::write(exefs.join("main"), &main_nso).unwrap();

        // `subsdk0` imports `ounce_add` (resolved from `main`) and `missing_hook` (exported by nobody).
        let subsdk_nso = build_linked_nso(
            &[("ounce_add", 0, 1, 0), ("missing_hook", 0, 1, 0)],
            &[(0x300, R_AARCH64_GLOB_DAT, 1, 0), (0x308, R_AARCH64_GLOB_DAT, 2, 0)],
            &[],
        );
        fs::write(exefs.join("subsdk0"), &subsdk_nso).unwrap();

        let pkg = LoosePackageLoader::discover(&temp_dir).expect("Failed to discover loose package");
        let mut mmu = Memory::new();
        let mut kernel = HorizonKernel::new();
        let base = 0x80000000u64;
        let boot_info = LoosePackageLoader::load_into_kernel(&pkg, base, &mut mmu, &mut kernel)
            .expect("Failed to load into kernel");

        let subsdk_base = base + 0x04000000;
        assert_eq!(mmu.read_u64(subsdk_base + 0x300).unwrap(), base + 0x500);
        assert_eq!(boot_info.relocations_applied, 1);
        assert_eq!(boot_info.unresolved_imports, vec!["missing_hook".to_string()]);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_loose_package_discovery_and_multimodule_load() {
        // Create a temporary directory structure mimicking an OunceCake dump
        let temp_dir = std::env::temp_dir().join(format!("ounce_test_pkg_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let exefs = temp_dir.join("exefs");
        let romfs = temp_dir.join("romfs");
        fs::create_dir_all(&exefs).unwrap();
        fs::create_dir_all(&romfs).unwrap();

        // 1. Write synthetic NPDM
        let npdm_bytes = NpdmDescriptor::create_synthetic(
            "Showcase2",
            "OUNCE-P-SHOW",
            0x0100023900002026,
            40,
            0,
            &["sm:", "fsp-srv", "nvdrv"],
        );
        fs::write(exefs.join("main.npdm"), &npdm_bytes).unwrap();

        // 2. Write valid synthetic main NSO
        let main_nso = NroLoader::build_synthetic_nso(&[
            0x20, 0x00, 0x80, 0xD2, // MOV X0, #1
            0xC0, 0x03, 0x5F, 0xD6, // RET
        ]);
        fs::write(exefs.join("main"), &main_nso).unwrap();

        // 3. Write valid companion subsdk0 NSO
        let subsdk_nso = NroLoader::build_synthetic_nso(&[
            0x40, 0x00, 0x80, 0xD2, // MOV X0, #2
            0xC0, 0x03, 0x5F, 0xD6, // RET
        ]);
        fs::write(exefs.join("subsdk0"), &subsdk_nso).unwrap();

        // 4. Write loose RomFS assets
        fs::write(romfs.join("manifest.json"), b"{\"game\":\"Showcase2\"}").unwrap();
        fs::create_dir_all(romfs.join("data")).unwrap();
        fs::write(romfs.join("data/config.bin"), b"\x01\x02\x03\x04").unwrap();

        // Discover package
        let pkg = LoosePackageLoader::discover(&temp_dir).expect("Failed to discover loose package");
        assert_eq!(pkg.companion_nsos.len(), 1);
        assert_eq!(pkg.companion_nsos[0].0, "subsdk0");
        assert!(pkg.romfs_dir.is_some());

        // Load into VM
        let mut mmu = Memory::new();
        let mut kernel = HorizonKernel::new();
        let boot_info = LoosePackageLoader::load_into_kernel(&pkg, 0x80000000, &mut mmu, &mut kernel)
            .expect("Failed to load into kernel");

        assert_eq!(boot_info.process_name, "Showcase2");
        assert_eq!(boot_info.title_id, 0x0100023900002026);
        assert_eq!(boot_info.modules_loaded.len(), 2); // main + subsdk0
        assert_eq!(boot_info.romfs_files_mounted, 2);

        // Verify RomFS files are accessible in kernel VFS
        let manifest = kernel.vfs.get_file("romfs:/manifest.json").expect("Missing manifest in VFS");
        assert_eq!(manifest, b"{\"game\":\"Showcase2\"}");

        let config = kernel.vfs.get_file("romfs:/data/config.bin").expect("Missing config in VFS");
        assert_eq!(config, b"\x01\x02\x03\x04");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
