#![allow(dead_code)]

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;
use crate::crypto::Aes128;
use crate::loader::nro::{LoadError, NroLoader};
use crate::mmu::{Memory, Permission};

pub const PFS0_MAGIC: u32 = 0x30534650; // "PFS0" in Little Endian
pub const HFS0_MAGIC: u32 = 0x30534648; // "HFS0" in Little Endian
pub const XCI_HEAD_MAGIC: u32 = 0x44414548; // "HEAD" in Little Endian (at offset 0x100)
pub const NCA3_MAGIC: u32 = 0x3341434E; // "NCA3" in Little Endian

/// NIST FIPS 197 reference test key used exclusively for synthetic container testing.
pub const SYNTHETIC_TEST_KEY: [u8; 16] = [
    0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
    0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
];

#[derive(Debug, Clone)]
pub struct Pfs0FileEntry {
    pub name: String,
    pub data_offset: usize,
    pub size: usize,
}

/// Partition File System 0 (`PFS0`) container parser — used for `.nsp` packages
/// and the `ExeFS` (Executable File System) section inside an NCA.
pub struct Pfs0Archive {
    raw_data: Vec<u8>,
    data_base_offset: usize,
    entries: Vec<Pfs0FileEntry>,
}

impl Pfs0Archive {
    pub fn parse(raw: Vec<u8>) -> Result<Self, LoadError> {
        if raw.len() < 16 {
            return Err(LoadError::TooSmall);
        }

        let mut rdr = Cursor::new(&raw);
        let magic = rdr.read_u32::<LittleEndian>().unwrap();
        if magic != PFS0_MAGIC {
            return Err(LoadError::InvalidMagic(magic.to_le_bytes()));
        }

        let num_files = rdr.read_u32::<LittleEndian>().unwrap() as usize;
        let string_table_size = rdr.read_u32::<LittleEndian>().unwrap() as usize;
        let _reserved = rdr.read_u32::<LittleEndian>().unwrap();

        let file_table_offset = 16;
        let table_end = num_files.checked_mul(24).and_then(|t| t.checked_add(file_table_offset)).ok_or(LoadError::TooSmall)?;
        let string_table_offset = table_end;
        let data_base_offset = string_table_offset.checked_add(string_table_size).ok_or(LoadError::TooSmall)?;

        if raw.len() < data_base_offset {
            return Err(LoadError::TooSmall);
        }

        let mut entries = Vec::with_capacity(num_files);
        for i in 0..num_files {
            rdr.set_position((file_table_offset + i * 24) as u64);
            let data_offset = rdr.read_u64::<LittleEndian>().unwrap() as usize;
            let size = rdr.read_u64::<LittleEndian>().unwrap() as usize;
            let str_offset = rdr.read_u32::<LittleEndian>().unwrap() as usize;
            let _zero = rdr.read_u32::<LittleEndian>().unwrap();

            let name_start = string_table_offset.saturating_add(str_offset).min(data_base_offset);
            let name_end = raw[name_start..data_base_offset]
                .iter()
                .position(|&b| b == 0)
                .map_or(data_base_offset, |n| name_start + n);
            let name = String::from_utf8_lossy(&raw[name_start..name_end]).to_string();

            entries.push(Pfs0FileEntry {
                name,
                data_offset,
                size,
            });
        }

        Ok(Self {
            raw_data: raw,
            data_base_offset,
            entries,
        })
    }

    pub fn list_files(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.name.as_str()).collect()
    }

    pub fn extract_file(&self, name: &str) -> Option<&[u8]> {
        for entry in &self.entries {
            if entry.name == name {
                let Some(start) = self.data_base_offset.checked_add(entry.data_offset) else { continue };
                let Some(end) = start.checked_add(entry.size) else { continue };
                if end <= self.raw_data.len() {
                    return Some(&self.raw_data[start..end]);
                }
            }
        }
        None
    }

    /// Builds a valid `PFS0` archive (e.g., `.nsp` or `ExeFS`) from a slice of `(filename, bytes)`.
    pub fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut string_table = Vec::new();
        let mut str_offsets = Vec::with_capacity(files.len());

        for (name, _) in files {
            str_offsets.push(string_table.len() as u32);
            string_table.extend_from_slice(name.as_bytes());
            string_table.push(0);
        }
        // Align string table to 16 bytes
        while string_table.len() % 16 != 0 {
            string_table.push(0);
        }

        let mut buf = Vec::new();
        buf.write_u32::<LittleEndian>(PFS0_MAGIC).unwrap();
        buf.write_u32::<LittleEndian>(files.len() as u32).unwrap();
        buf.write_u32::<LittleEndian>(string_table.len() as u32).unwrap();
        buf.write_u32::<LittleEndian>(0).unwrap();

        let mut current_data_offset = 0u64;
        for (i, (_, data)) in files.iter().enumerate() {
            buf.write_u64::<LittleEndian>(current_data_offset).unwrap();
            buf.write_u64::<LittleEndian>(data.len() as u64).unwrap();
            buf.write_u32::<LittleEndian>(str_offsets[i]).unwrap();
            buf.write_u32::<LittleEndian>(0).unwrap();
            current_data_offset += data.len() as u64;
        }

        buf.extend_from_slice(&string_table);
        for (_, data) in files {
            buf.extend_from_slice(data);
        }

        buf
    }
}

/// Hashed File System 0 (`HFS0`) container parser & builder — used for Switch / Switch 2
/// `.xci` Game Card partitions (`update`, `normal`, `logo`, `secure`).
/// Each file table entry is 64 bytes (`0x40`), including a 32-byte SHA-256 region hash.
pub struct Hfs0Archive {
    raw_data: Vec<u8>,
    data_base_offset: usize,
    entries: Vec<Pfs0FileEntry>,
}

impl Hfs0Archive {
    pub fn parse(raw: Vec<u8>) -> Result<Self, LoadError> {
        if raw.len() < 16 {
            return Err(LoadError::TooSmall);
        }

        let mut rdr = Cursor::new(&raw);
        let magic = rdr.read_u32::<LittleEndian>().unwrap();
        if magic != HFS0_MAGIC {
            return Err(LoadError::InvalidMagic(magic.to_le_bytes()));
        }

        let num_files = rdr.read_u32::<LittleEndian>().unwrap() as usize;
        let string_table_size = rdr.read_u32::<LittleEndian>().unwrap() as usize;
        let _reserved = rdr.read_u32::<LittleEndian>().unwrap();

        let file_table_offset = 16;
        let table_end = num_files.checked_mul(0x40).and_then(|t| t.checked_add(file_table_offset)).ok_or(LoadError::TooSmall)?;
        let string_table_offset = table_end;
        let data_base_offset = string_table_offset.checked_add(string_table_size).ok_or(LoadError::TooSmall)?;

        if raw.len() < data_base_offset {
            return Err(LoadError::TooSmall);
        }

        let mut entries = Vec::with_capacity(num_files);
        for i in 0..num_files {
            rdr.set_position((file_table_offset + i * 0x40) as u64);
            let data_offset = rdr.read_u64::<LittleEndian>().unwrap() as usize;
            let size = rdr.read_u64::<LittleEndian>().unwrap() as usize;
            let str_offset = rdr.read_u32::<LittleEndian>().unwrap() as usize;

            let name_start = string_table_offset.saturating_add(str_offset).min(data_base_offset);
            let name_end = raw[name_start..data_base_offset]
                .iter()
                .position(|&b| b == 0)
                .map_or(data_base_offset, |n| name_start + n);
            let name = String::from_utf8_lossy(&raw[name_start..name_end]).to_string();

            entries.push(Pfs0FileEntry {
                name,
                data_offset,
                size,
            });
        }

        Ok(Self {
            raw_data: raw,
            data_base_offset,
            entries,
        })
    }

    pub fn list_files(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.name.as_str()).collect()
    }

    pub fn extract_file(&self, name: &str) -> Option<&[u8]> {
        for entry in &self.entries {
            if entry.name == name {
                let Some(start) = self.data_base_offset.checked_add(entry.data_offset) else { continue };
                let Some(end) = start.checked_add(entry.size) else { continue };
                if end <= self.raw_data.len() {
                    return Some(&self.raw_data[start..end]);
                }
            }
        }
        None
    }

    /// Builds a valid `HFS0` partition archive from `(filename, bytes)` pairs.
    pub fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut string_table = Vec::new();
        let mut str_offsets = Vec::with_capacity(files.len());

        for (name, _) in files {
            str_offsets.push(string_table.len() as u32);
            string_table.extend_from_slice(name.as_bytes());
            string_table.push(0);
        }
        while string_table.len() % 16 != 0 {
            string_table.push(0);
        }

        let mut buf = Vec::new();
        buf.write_u32::<LittleEndian>(HFS0_MAGIC).unwrap();
        buf.write_u32::<LittleEndian>(files.len() as u32).unwrap();
        buf.write_u32::<LittleEndian>(string_table.len() as u32).unwrap();
        buf.write_u32::<LittleEndian>(0).unwrap();

        let mut current_data_offset = 0u64;
        for (i, (_, data)) in files.iter().enumerate() {
            buf.write_u64::<LittleEndian>(current_data_offset).unwrap();
            buf.write_u64::<LittleEndian>(data.len() as u64).unwrap();
            buf.write_u32::<LittleEndian>(str_offsets[i]).unwrap();
            buf.write_u32::<LittleEndian>(data.len().min(0x200) as u32).unwrap(); // hashed_region_size
            buf.write_u64::<LittleEndian>(0).unwrap(); // reserved
            // 32-byte placeholder SHA-256 digest
            buf.extend_from_slice(&[0xAAu8; 32]);
            current_data_offset += data.len() as u64;
        }

        buf.extend_from_slice(&string_table);
        for (_, data) in files {
            buf.extend_from_slice(data);
        }
        buf
    }
}

/// Nintendo Switch / Switch 2 Game Card Image (`.xci`) parser & builder.
/// Parses the `HEAD` header at `0x100`, locates the root `HFS0` partition table,
/// and extracts `.nca` archives from the `secure` partition.
pub struct XciCartridge {
    pub cart_size_code: u8,
    pub hfs0_offset: usize,
    pub root_hfs0: Hfs0Archive,
}

impl XciCartridge {
    pub fn parse(raw: &[u8]) -> Result<Self, LoadError> {
        if raw.len() < 0x200 {
            return Err(LoadError::TooSmall);
        }
        let mut rdr = Cursor::new(raw);
        rdr.set_position(0x100);
        let magic = rdr.read_u32::<LittleEndian>().unwrap();
        if magic != XCI_HEAD_MAGIC {
            return Err(LoadError::InvalidMagic(magic.to_le_bytes()));
        }

        let cart_size_code = raw[0x10D];
        rdr.set_position(0x130);
        let hfs0_offset = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        if hfs0_offset >= raw.len() {
            return Err(LoadError::TooSmall);
        }

        let root_hfs0 = Hfs0Archive::parse(raw[hfs0_offset..].to_vec())?;
        Ok(Self {
            cart_size_code,
            hfs0_offset,
            root_hfs0,
        })
    }

    /// Extracts the `secure` `HFS0` partition from the `.xci` image.
    pub fn open_secure_partition(&self) -> Result<Hfs0Archive, LoadError> {
        let secure_bytes = self
            .root_hfs0
            .extract_file("secure")
            .ok_or(LoadError::TooSmall)?;
        Hfs0Archive::parse(secure_bytes.to_vec())
    }

    /// Synthesizes a Switch 2 `.xci` (64GB Game Card code `0xE3`) image with `update`, `normal`, and `secure` `HFS0` partitions.
    pub fn build_cartridge(secure_files: &[(&str, &[u8])]) -> Vec<u8> {
        let update_hfs0 = Hfs0Archive::build(&[]);
        let normal_hfs0 = Hfs0Archive::build(&[]);
        let secure_hfs0 = Hfs0Archive::build(secure_files);

        let root_hfs0 = Hfs0Archive::build(&[
            ("update", &update_hfs0),
            ("normal", &normal_hfs0),
            ("secure", &secure_hfs0),
        ]);

        let hfs0_offset = 0x1000usize; // Compact aligned HFS0 root offset
        let mut xci = vec![0u8; hfs0_offset + root_hfs0.len()];

        // Write "HEAD" magic at 0x100
        (&mut xci[0x100..0x104])
            .write_u32::<LittleEndian>(XCI_HEAD_MAGIC)
            .unwrap();
        // Switch 2 64GB High-Speed Game Card size code (0xE3) at 0x10D
        xci[0x10D] = 0xE3;
        // Write root HFS0 offset at 0x130 and header size at 0x138
        (&mut xci[0x130..0x138])
            .write_u64::<LittleEndian>(hfs0_offset as u64)
            .unwrap();
        (&mut xci[0x138..0x140])
            .write_u64::<LittleEndian>(0x100)
            .unwrap();

        xci[hfs0_offset..].copy_from_slice(&root_hfs0);
        xci
    }
}

/// Nintendo Content Archive (`NCA3`) Decryptor & Section Extractor.
/// Uses AES-128-CTR to decrypt the ExeFS (Section 0) and RomFS (Section 1) of an NCA container.
pub struct NcaImage {
    pub title_id: u64,
    pub section_offset: usize,
    pub section_size: usize,
    pub ctr_iv: [u8; 16],
    pub decrypted_payload: Vec<u8>,
    pub romfs_offset: usize,
    pub romfs_size: usize,
    pub decrypted_romfs: Option<Vec<u8>>,
}

impl NcaImage {
    /// Parses an NCA3 image and decrypts Section 0 (ExeFS) and Section 1 (RomFS) using AES-128-CTR.
    /// In accordance with clean-room policy, decryption is restricted strictly to synthetic test vectors.
    pub fn parse_and_decrypt(raw: &[u8], section_key: &[u8; 16]) -> Result<Self, LoadError> {
        if section_key != &SYNTHETIC_TEST_KEY {
            return Err(LoadError::RestrictedCipher(
                "Commercial retail key decryption is disabled by policy. Container decryption is restricted strictly to synthetic test images. For research binaries, provide unencrypted loose packages.".into(),
            ));
        }

        if raw.len() < 0x400 {
            return Err(LoadError::TooSmall);
        }

        let mut rdr = Cursor::new(raw);
        rdr.set_position(0x200);
        let magic = rdr.read_u32::<LittleEndian>().unwrap();
        if magic != NCA3_MAGIC {
            return Err(LoadError::InvalidMagic(magic.to_le_bytes()));
        }

        rdr.set_position(0x210);
        let title_id = rdr.read_u64::<LittleEndian>().unwrap();

        // Section 0 table entry at 0x240 (media start offset & media end offset, in 0x200-byte media units)
        rdr.set_position(0x240);
        let media_start = rdr.read_u32::<LittleEndian>().unwrap() as usize;
        let media_end = rdr.read_u32::<LittleEndian>().unwrap() as usize;
        let section_offset = media_start * 0x200;
        let section_size = (media_end.saturating_sub(media_start)) * 0x200;

        if section_offset.checked_add(section_size).is_none_or(|e| e > raw.len()) {
            return Err(LoadError::TooSmall);
        }

        // Section 0 CTR IV stored at 0x340
        let mut ctr_iv = [0u8; 16];
        ctr_iv.copy_from_slice(&raw[0x340..0x350]);

        let mut decrypted_payload = raw[section_offset..section_offset + section_size].to_vec();
        let cipher = Aes128::new(section_key);
        cipher.crypt_ctr(&ctr_iv, &mut decrypted_payload);

        // Section 1 (RomFS) table entry at 0x248
        rdr.set_position(0x248);
        let romfs_start = rdr.read_u32::<LittleEndian>().unwrap() as usize;
        let romfs_end = rdr.read_u32::<LittleEndian>().unwrap() as usize;
        let mut decrypted_romfs = None;
        let mut romfs_offset = 0;
        let mut romfs_size = 0;

        if romfs_end > romfs_start {
            romfs_offset = romfs_start * 0x200;
            romfs_size = (romfs_end - romfs_start) * 0x200;
            if romfs_offset.checked_add(romfs_size).is_some_and(|e| e <= raw.len()) {
                let mut romfs_ctr_iv = [0u8; 16];
                romfs_ctr_iv.copy_from_slice(&raw[0x350..0x360]);
                let mut romfs_buf = raw[romfs_offset..romfs_offset + romfs_size].to_vec();
                let cipher_romfs = Aes128::new(section_key);
                cipher_romfs.crypt_ctr(&romfs_ctr_iv, &mut romfs_buf);
                decrypted_romfs = Some(romfs_buf);
            }
        }

        Ok(Self {
            title_id,
            section_offset,
            section_size,
            ctr_iv,
            decrypted_payload,
            romfs_offset,
            romfs_size,
            decrypted_romfs,
        })
    }

    /// Synthesizes an AES-128-CTR encrypted `NCA3` container holding the given `exefs_pfs0` payload
    /// and optional `romfs_binary` payload in Section 1.
    pub fn build_encrypted(title_id: u64, section_key: &[u8; 16], ctr_iv: &[u8; 16], payload: &[u8]) -> Vec<u8> {
        Self::build_encrypted_with_romfs(title_id, section_key, ctr_iv, &[0x55u8; 16], payload, None)
    }

    /// Synthesizes an encrypted `NCA3` container with both Section 0 (ExeFS) and Section 1 (RomFS).
    pub fn build_encrypted_with_romfs(
        title_id: u64,
        section_key: &[u8; 16],
        ctr_iv_0: &[u8; 16],
        ctr_iv_1: &[u8; 16],
        exefs_payload: &[u8],
        romfs_payload: Option<&[u8]>,
    ) -> Vec<u8> {
        let header_size = 0x400;
        let aligned_exefs_len = (exefs_payload.len() + 0x1FF) & !0x1FF;
        let aligned_romfs_len = if let Some(r) = romfs_payload {
            (r.len() + 0x1FF) & !0x1FF
        } else {
            0
        };

        let total_size = header_size + aligned_exefs_len + aligned_romfs_len;
        let mut nca = vec![0u8; total_size];

        // Write NCA3 magic at 0x200
        (&mut nca[0x200..0x204]).write_u32::<LittleEndian>(NCA3_MAGIC).unwrap();
        // Write Title ID at 0x210
        (&mut nca[0x210..0x218]).write_u64::<LittleEndian>(title_id).unwrap();

        // Write Section 0 bounds in 0x200 media units at 0x240
        let sec0_start = (header_size / 0x200) as u32;
        let sec0_end = ((header_size + aligned_exefs_len) / 0x200) as u32;
        (&mut nca[0x240..0x244]).write_u32::<LittleEndian>(sec0_start).unwrap();
        (&mut nca[0x244..0x248]).write_u32::<LittleEndian>(sec0_end).unwrap();

        // Write Section 0 CTR IV at 0x340
        nca[0x340..0x350].copy_from_slice(ctr_iv_0);

        // Copy Section 0 plaintext and encrypt
        nca[header_size..header_size + exefs_payload.len()].copy_from_slice(exefs_payload);
        let cipher0 = Aes128::new(section_key);
        cipher0.crypt_ctr(ctr_iv_0, &mut nca[header_size..header_size + aligned_exefs_len]);

        // If Section 1 (RomFS) present:
        if let Some(romfs) = romfs_payload {
            let sec1_start = sec0_end;
            let sec1_end = ((header_size + aligned_exefs_len + aligned_romfs_len) / 0x200) as u32;
            (&mut nca[0x248..0x24C]).write_u32::<LittleEndian>(sec1_start).unwrap();
            (&mut nca[0x24C..0x250]).write_u32::<LittleEndian>(sec1_end).unwrap();

            // Write Section 1 CTR IV at 0x350
            nca[0x350..0x360].copy_from_slice(ctr_iv_1);

            let sec1_offset = header_size + aligned_exefs_len;
            nca[sec1_offset..sec1_offset + romfs.len()].copy_from_slice(romfs);
            let cipher1 = Aes128::new(section_key);
            cipher1.crypt_ctr(ctr_iv_1, &mut nca[sec1_offset..sec1_offset + aligned_romfs_len]);
        }

        nca
    }
}

#[derive(Debug, Clone)]
pub struct BootPackageInfo {
    pub format: &'static str,
    pub title_id: u64,
    pub code_size: usize,
    pub relocations_applied: usize,
}

/// Unified Switch 2 Boot Loader:
/// Automatically detects `.nsp` (`PFS0`), `.nca` (`NCA3`), or `.nro` (`NRO0`) images,
/// decrypts any encrypted `NCA3` containers via AES-128-CTR, extracts `ExeFS` `main`,
/// and loads the executable into guest RAM at `base_addr`.
pub struct BootLoader;

impl BootLoader {
    pub fn load_any_package(
        raw: &[u8],
        base_addr: u64,
        section_key: &[u8; 16],
        mmu: &mut Memory,
    ) -> Result<BootPackageInfo, LoadError> {
        Self::load_package_with_vfs(raw, base_addr, section_key, mmu, None)
    }

    pub fn load_package_with_vfs(
        raw: &[u8],
        base_addr: u64,
        section_key: &[u8; 16],
        mmu: &mut Memory,
        mut vfs: Option<&mut crate::fs::VirtualFileSystem>,
    ) -> Result<BootPackageInfo, LoadError> {
        // 0. Check if outer container is a Switch / Switch 2 `.xci` Game Card image ("HEAD" at 0x100)
        if raw.len() >= 0x200 && (&raw[0x100..0x104] == b"HEAD") {
            let xci = XciCartridge::parse(raw)?;
            let secure = xci.open_secure_partition()?;
            let files = secure.list_files();
            if let Some(&nca_name) = files.iter().find(|&&name| name.ends_with(".nca")) {
                let nca_bytes = secure.extract_file(nca_name).ok_or(LoadError::TooSmall)?;
                let mut info = Self::load_nca_bytes(nca_bytes, base_addr, section_key, mmu, vfs.as_deref_mut())?;
                info.format = "XCI (GameCard HEAD) -> HFS0 (secure) -> NCA3 -> ExeFS";
                return Ok(info);
            } else if let Some(main_bytes) = secure.extract_file("main") {
                return Self::load_executable_bytes(
                    main_bytes,
                    base_addr,
                    0x0400_0000_0001_0000,
                    "XCI (GameCard HEAD) -> HFS0 (secure)",
                    mmu,
                );
            }
        }

        // 0b. Check if outer container is a standalone `HFS0` partition archive
        if raw.len() >= 16 && (&raw[0..4] == b"HFS0") {
            let hfs0 = Hfs0Archive::parse(raw.to_vec())?;
            let files = hfs0.list_files();
            if let Some(&nca_name) = files.iter().find(|&&name| name.ends_with(".nca")) {
                let nca_bytes = hfs0.extract_file(nca_name).ok_or(LoadError::TooSmall)?;
                let mut info = Self::load_nca_bytes(nca_bytes, base_addr, section_key, mmu, vfs.as_deref_mut())?;
                info.format = "HFS0 -> NCA3 (AES-128-CTR) -> ExeFS";
                return Ok(info);
            }
        }

        // 1. Check if outer container is a PFS0 (.nsp) archive
        if raw.len() >= 16 && (&raw[0..4] == b"PFS0") {
            let pfs0 = Pfs0Archive::parse(raw.to_vec())?;
            // Look for the first .nca file inside the .nsp, or fallback to "main"
            let files = pfs0.list_files();
            if let Some(&nca_name) = files.iter().find(|&&name| name.ends_with(".nca")) {
                let nca_bytes = pfs0.extract_file(nca_name).ok_or(LoadError::TooSmall)?;
                let mut info = Self::load_nca_bytes(nca_bytes, base_addr, section_key, mmu, vfs.as_deref_mut())?;
                info.format = "NSP -> NCA3 (AES-128-CTR) -> ExeFS";
                return Ok(info);
            } else if let Some(main_bytes) = pfs0.extract_file("main") {
                return Self::load_executable_bytes(
                    main_bytes,
                    base_addr,
                    0,
                    "PFS0 (ExeFS)",
                    mmu,
                );
            }
        }

        // 2. Check if image is a raw encrypted NCA3 container (magic "NCA3" at 0x200)
        if raw.len() >= 0x400 && (&raw[0x200..0x204] == b"NCA3") {
            return Self::load_nca_bytes(raw, base_addr, section_key, mmu, vfs.as_deref_mut());
        }

        // 3. Otherwise treat as NRO0 or raw AArch64 executable
        Self::load_executable_bytes(raw, base_addr, 0, "NRO0", mmu)
    }

    fn load_nca_bytes(
        nca_bytes: &[u8],
        base_addr: u64,
        section_key: &[u8; 16],
        mmu: &mut Memory,
        vfs: Option<&mut crate::fs::VirtualFileSystem>,
    ) -> Result<BootPackageInfo, LoadError> {
        let decrypted = NcaImage::parse_and_decrypt(nca_bytes, section_key)?;
        let title_id = decrypted.title_id;

        // If Section 1 (RomFS) is present and a VFS was provided, automatically parse and mount it!
        if let (Some(romfs_bytes), Some(vfs_ref)) = (&decrypted.decrypted_romfs, vfs) {
            if let Ok(archive) = crate::fs::RomFsArchive::parse(romfs_bytes) {
                vfs_ref.mount_romfs_archive(&archive);
            }
        }

        // Check if decrypted Section 0 is an ExeFS (PFS0) archive
        if decrypted.decrypted_payload.len() >= 16 && (&decrypted.decrypted_payload[0..4] == b"PFS0")
        {
            let exefs = Pfs0Archive::parse(decrypted.decrypted_payload)?;
            let main_code = exefs.extract_file("main").ok_or(LoadError::TooSmall)?;
            return Self::load_executable_bytes(
                main_code,
                base_addr,
                title_id,
                "NCA3 (AES-128-CTR) -> ExeFS",
                mmu,
            );
        }

        Self::load_executable_bytes(
            &decrypted.decrypted_payload,
            base_addr,
            title_id,
            "NCA3 (AES-128-CTR)",
            mmu,
        )
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
            let hdr = NroLoader::load_into_memory(exec_bytes, base_addr, mmu)?;
            return Ok(BootPackageInfo {
                format,
                title_id,
                code_size: hdr.text_size as usize,
                relocations_applied: hdr.relocations_applied,
            });
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
    fn test_nsp_pfs0_and_nca_aes_ctr_pipeline() {
        let key: [u8; 16] = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
            0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
        ];
        let iv: [u8; 16] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        ];

        // 1. Build an ExeFS PFS0 containing a mock "main" executable
        let main_code = b"OUNCE_SWITCH2_DECRYPTED_EXECUTABLE_CODE_2026";
        let exefs_bytes = Pfs0Archive::build(&[("main", main_code), ("main.npdm", b"META")]);

        // 2. Encrypt the ExeFS inside an NCA3 container
        let title_id = 0x0100_F2C0_115B_6000;
        let encrypted_nca = NcaImage::build_encrypted(title_id, &key, &iv, &exefs_bytes);

        // Verify the ciphertext does NOT contain the plaintext string
        assert!(!encrypted_nca.windows(main_code.len()).any(|w| w == main_code));

        // 3. Wrap the encrypted NCA inside an outer .nsp (PFS0) package
        let nsp_bytes = Pfs0Archive::build(&[("program.nca", &encrypted_nca)]);

        // 4. Parse the outer .nsp, extract "program.nca", decrypt with AES-128-CTR, and read "main"
        let nsp = Pfs0Archive::parse(nsp_bytes).unwrap();
        assert_eq!(nsp.list_files(), vec!["program.nca"]);

        let nca_raw = nsp.extract_file("program.nca").unwrap();
        let decrypted_nca = NcaImage::parse_and_decrypt(nca_raw, &key).unwrap();
        assert_eq!(decrypted_nca.title_id, title_id);

        let exefs = Pfs0Archive::parse(decrypted_nca.decrypted_payload).unwrap();
        let extracted_main = exefs.extract_file("main").unwrap();
        assert_eq!(extracted_main, main_code);
    }

    #[test]
    fn test_xci_cartridge_hfs0_and_nso_boot_pipeline() {
        let key: [u8; 16] = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
            0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
        ];
        let iv: [u8; 16] = [
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
            0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];

        // Pack NSO0 module inside ExeFS -> encrypted NCA3 -> HFS0 `secure` -> `.xci` GameCard
        let nso_payload = crate::loader::nro::NroLoader::synthetic_nso_payload();
        let exefs_bytes = Pfs0Archive::build(&[("main", &nso_payload), ("main.npdm", b"META")]);
        let title_id = 0x0400_0000_0000_0037;
        let encrypted_nca = NcaImage::build_encrypted(title_id, &key, &iv, &exefs_bytes);
        let xci_bytes = XciCartridge::build_cartridge(&[("0400000000000037.nca", &encrypted_nca)]);

        let xci = XciCartridge::parse(&xci_bytes).unwrap();
        assert_eq!(xci.cart_size_code, 0xE3); // 64GB Switch 2 High-Speed Game Card
        assert_eq!(xci.root_hfs0.list_files(), vec!["update", "normal", "secure"]);

        let mut mmu = Memory::new();
        let boot_info = BootLoader::load_any_package(&xci_bytes, 0x8000_0000, &key, &mut mmu).unwrap();
        assert_eq!(boot_info.format, "XCI (GameCard HEAD) -> HFS0 (secure) -> NCA3 -> ExeFS");
        assert_eq!(boot_info.title_id, title_id);
        assert!(boot_info.code_size > 0);
    }

    #[test]
    fn test_nca_multi_section_romfs_mount() {
        let key: [u8; 16] = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
            0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
        ];
        let iv0: [u8; 16] = [
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
            0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let iv1: [u8; 16] = [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
            0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
        ];

        // 1. Create RomFS binary archive with game assets
        let romfs_data = crate::fs::RomFsArchive::build(&[
            ("/textures/hero.png", b"\x89PNG\r\n\x1a\nHERO_TEXTURE"),
            ("/config/game.json", b"{\"title\":\"Switch 2 Game\",\"fps\":60}"),
        ]);

        // 2. Create ExeFS (PFS0) with main code
        let main_code = b"MOCK_AARCH64_MAIN_EXECUTABLE_CODE_FOR_NCA_SECTION_0";
        let exefs_bytes = Pfs0Archive::build(&[("main", main_code), ("main.npdm", b"META")]);

        // 3. Encrypt both Section 0 (ExeFS) and Section 1 (RomFS) into NCA3
        let title_id = 0x0100_F2C0_115B_7000;
        let encrypted_nca = NcaImage::build_encrypted_with_romfs(
            title_id,
            &key,
            &iv0,
            &iv1,
            &exefs_bytes,
            Some(&romfs_data),
        );

        // 4. Wrap inside .nsp
        let nsp_bytes = Pfs0Archive::build(&[("game.nca", &encrypted_nca)]);

        // 5. Load through Unified BootLoader with VirtualFileSystem
        let mut mmu = Memory::new();
        let mut vfs = crate::fs::VirtualFileSystem::new();
        let boot_info = BootLoader::load_package_with_vfs(
            &nsp_bytes,
            0x8000_0000,
            &key,
            &mut mmu,
            Some(&mut vfs),
        ).unwrap();

        assert_eq!(boot_info.title_id, title_id);
        assert_eq!(boot_info.format, "NSP -> NCA3 (AES-128-CTR) -> ExeFS");

        // Verify that RomFS Section 1 was decrypted and automatically mounted into VFS!
        let tex = vfs.get_file("romfs:/textures/hero.png").expect("hero.png mounted");
        assert_eq!(tex, b"\x89PNG\r\n\x1a\nHERO_TEXTURE");

        let cfg = vfs.get_file("romfs:/config/game.json").expect("game.json mounted");
        assert_eq!(cfg, b"{\"title\":\"Switch 2 Game\",\"fps\":60}");
    }

    #[test]
    fn test_commercial_key_decryption_rejected_by_policy() {
        let dummy_nca = vec![0u8; 0x1000];
        let non_synthetic_key = [0x99u8; 16]; // Simulated retail/custom key
        let res = NcaImage::parse_and_decrypt(&dummy_nca, &non_synthetic_key);
        match res {
            Err(LoadError::RestrictedCipher(msg)) => {
                assert!(msg.contains("Commercial retail key decryption is disabled"));
            }
            _ => panic!("Expected RestrictedCipher error for non-synthetic key"),
        }
    }
}
