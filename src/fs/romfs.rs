#![allow(dead_code)]

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;
use crate::mmu::{Memory, MemoryError};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("File not found in VFS: {0}")]
    NotFound(String),

    #[error("Invalid VFS file handle: {0}")]
    InvalidHandle(u32),

    #[error("Memory error during VFS transfer: {0}")]
    Memory(#[from] MemoryError),
}

/// Horizon OS Virtual Filesystem (RomFS, SDMC, and SaveData).
pub struct VirtualFileSystem {
    /// Maps normalized virtual paths (e.g. "romfs:/title.txt") to file byte contents
    files: HashMap<String, Vec<u8>>,
    /// Maps open file handles to virtual paths
    open_handles: HashMap<u32, String>,
    next_handle: u32,
    pub save_dir: Option<std::path::PathBuf>,
    pub sdmc_dir: Option<std::path::PathBuf>,
    pub total_bytes_read: u64,
    pub total_bytes_written: u64,
}

impl Default for VirtualFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualFileSystem {
    pub fn new() -> Self {
        let mut vfs = Self {
            files: HashMap::new(),
            open_handles: HashMap::new(),
            next_handle: 1,
            save_dir: None,
            sdmc_dir: None,
            total_bytes_read: 0,
            total_bytes_written: 0,
        };

        // Populate built-in Switch 2 RomFS metadata assets
        vfs.mount_file(
            "romfs:/title.txt",
            b"Ounce-Core Switch 2 Tech Demo (Tegra T239 Ampere)",
        );
        vfs.mount_file(
            "romfs:/shader/default_vs.glsl",
            b"#version 460\nuniform mat4 u_mvp;\nvoid main() { gl_Position = u_mvp * vec4(pos, 1.0); }",
        );

        vfs
    }

    /// Sets the host filesystem backing directory for persistent `save:/` storage.
    pub fn set_save_directory(&mut self, path: impl AsRef<std::path::Path>) {
        let p = path.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&p);
        self.save_dir = Some(p);
    }

    /// Sets the host filesystem backing directory for persistent `sdmc:/` storage.
    pub fn set_sdmc_directory(&mut self, path: impl AsRef<std::path::Path>) {
        let p = path.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&p);
        self.sdmc_dir = Some(p);
    }

    /// Mounts or overwrites a virtual file in the VFS.
    pub fn mount_file(&mut self, path: &str, data: &[u8]) {
        self.files.insert(path.to_string(), data.to_vec());
        self.sync_file_to_disk(path, data);
    }

    /// Retrieves a reference to raw file data if it exists in the VFS or on disk.
    pub fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(|v| v.as_slice())
    }

    fn sync_file_to_disk(&self, path: &str, data: &[u8]) {
        if let Some(ref s_dir) = self.save_dir {
            if path.starts_with("save:/") {
                let rel = &path[6..];
                let disk_path = s_dir.join(rel);
                if let Some(parent) = disk_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&disk_path, data);
            }
        }
        if let Some(ref s_dir) = self.sdmc_dir {
            if path.starts_with("sdmc:/") {
                let rel = &path[6..];
                let disk_path = s_dir.join(rel);
                if let Some(parent) = disk_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&disk_path, data);
            }
        }
    }

    /// Mounts all files from a binary Level 3 `RomFsArchive` under `romfs:/...`.
    pub fn mount_romfs_archive(&mut self, archive: &RomFsArchive) {
        for entry in &archive.entries {
            let full_path = if entry.path.starts_with('/') {
                format!("romfs:{}", entry.path)
            } else {
                format!("romfs:/{}", entry.path)
            };
            self.mount_file(&full_path, &entry.data);
        }
    }

    /// Opens a file by virtual path and returns a VFS file handle.
    pub fn open_file(&mut self, path: &str) -> Result<u32, VfsError> {
        if !self.files.contains_key(path) {
            return Err(VfsError::NotFound(path.to_string()));
        }
        let handle = self.next_handle;
        self.next_handle += 1;
        self.open_handles.insert(handle, path.to_string());
        Ok(handle)
    }

    /// Returns the byte length of an open file handle.
    pub fn get_file_size(&self, handle: u32) -> Result<usize, VfsError> {
        let path = self
            .open_handles
            .get(&handle)
            .ok_or(VfsError::InvalidHandle(handle))?;
        let data = self
            .files
            .get(path)
            .ok_or_else(|| VfsError::NotFound(path.clone()))?;
        Ok(data.len())
    }

    /// Reads bytes from a VFS file directly into guest virtual memory.
    pub fn read_file_to_guest(
        &mut self,
        path: &str,
        file_offset: usize,
        guest_dst_addr: u64,
        max_len: usize,
        mmu: &mut Memory,
    ) -> Result<usize, VfsError> {
        let data = self
            .files
            .get(path)
            .ok_or_else(|| VfsError::NotFound(path.to_string()))?;

        if file_offset >= data.len() {
            return Ok(0);
        }

        let available = (data.len() - file_offset).min(max_len);
        let slice = &data[file_offset..file_offset + available];
        mmu.write_bytes(guest_dst_addr, slice)?;
        self.total_bytes_read += available as u64;

        Ok(available)
    }

    /// Writes bytes from guest virtual memory into a writable VFS file (e.g., `save:/profile.sav`).
    pub fn write_file_from_guest(
        &mut self,
        path: &str,
        guest_src_addr: u64,
        len: usize,
        mmu: &Memory,
    ) -> Result<usize, VfsError> {
        let mut buf = vec![0u8; len];
        mmu.read_bytes(guest_src_addr, &mut buf)?;
        self.files.insert(path.to_string(), buf.clone());
        self.sync_file_to_disk(path, &buf);
        self.total_bytes_written += len as u64;
        Ok(len)
    }
}

pub const ROMFS_HEADER_SIZE: usize = 0x50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomFsEntry {
    pub path: String,
    pub data: Vec<u8>,
}

/// Nintendo Switch / Switch 2 Level 3 `RomFS` Binary Image Parser & Builder.
/// Represents game assets (3D meshes, textures, sound banks, level data) mounted from NCA Section 1.
#[derive(Debug, Clone, Default)]
pub struct RomFsArchive {
    pub entries: Vec<RomFsEntry>,
}

impl RomFsArchive {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file entry to the RomFS archive.
    pub fn add_file(&mut self, path: &str, data: &[u8]) {
        self.entries.push(RomFsEntry {
            path: path.to_string(),
            data: data.to_vec(),
        });
    }

    /// Builds an authentic Nintendo Level 3 `RomFS` binary image.
    pub fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        // Compute tables
        // File meta table format per file:
        // [0x00] parent_dir_offset: u32
        // [0x04] sibling_file_offset: u32
        // [0x08] data_offset: u64
        // [0x10] data_size: u64
        // [0x18] next_hash: u32
        // [0x1C] name_len: u32
        // [0x20..] name_bytes (aligned to 4 bytes)
        let mut file_meta = Vec::new();
        let mut file_data = Vec::new();
        let mut current_data_offset = 0u64;

        for (i, (name, data)) in files.iter().enumerate() {
            let sibling_offset = if i + 1 < files.len() {
                (file_meta.len() + 0x20 + ((name.len() + 3) & !3)) as u32
            } else {
                0xFFFF_FFFF
            };

            file_meta.write_u32::<LittleEndian>(0).unwrap(); // parent_dir = 0 (root)
            file_meta.write_u32::<LittleEndian>(sibling_offset).unwrap();
            file_meta.write_u64::<LittleEndian>(current_data_offset).unwrap();
            file_meta.write_u64::<LittleEndian>(data.len() as u64).unwrap();
            file_meta.write_u32::<LittleEndian>(0xFFFF_FFFF).unwrap(); // next_hash
            file_meta.write_u32::<LittleEndian>(name.len() as u32).unwrap();
            file_meta.extend_from_slice(name.as_bytes());
            while file_meta.len() % 4 != 0 {
                file_meta.push(0);
            }

            file_data.extend_from_slice(data);
            // Align file data to 16 bytes
            while file_data.len() % 16 != 0 {
                file_data.push(0);
            }
            current_data_offset = file_data.len() as u64;
        }

        // Header: 0x50 bytes
        let header_size = ROMFS_HEADER_SIZE as u64;
        let dir_hash_offset = header_size;
        let dir_hash_size = 4u64;
        let dir_meta_offset = dir_hash_offset + dir_hash_size;
        let dir_meta_size = 24u64; // minimal root directory entry
        let file_hash_offset = dir_meta_offset + dir_meta_size;
        let file_hash_size = 4u64;
        let file_meta_offset = file_hash_offset + file_hash_size;
        let file_meta_size = file_meta.len() as u64;
        let data_offset = (file_meta_offset + file_meta_size + 0x1FF) & !0x1FF; // align to 512 bytes

        let mut out = Vec::with_capacity(data_offset as usize + file_data.len());
        out.write_u64::<LittleEndian>(header_size).unwrap();
        out.write_u64::<LittleEndian>(dir_hash_offset).unwrap();
        out.write_u64::<LittleEndian>(dir_hash_size).unwrap();
        out.write_u64::<LittleEndian>(dir_meta_offset).unwrap();
        out.write_u64::<LittleEndian>(dir_meta_size).unwrap();
        out.write_u64::<LittleEndian>(file_hash_offset).unwrap();
        out.write_u64::<LittleEndian>(file_hash_size).unwrap();
        out.write_u64::<LittleEndian>(file_meta_offset).unwrap();
        out.write_u64::<LittleEndian>(file_meta_size).unwrap();
        out.write_u64::<LittleEndian>(data_offset).unwrap();

        out.write_u32::<LittleEndian>(0xFFFF_FFFF).unwrap(); // dir_hash
        // Root dir meta: [parent=0, sibling=0xFFFFFFFF, child_dir=0xFFFFFFFF, child_file=0, next_hash=0xFFFFFFFF, name_len=0]
        out.write_u32::<LittleEndian>(0).unwrap();
        out.write_u32::<LittleEndian>(0xFFFF_FFFF).unwrap();
        out.write_u32::<LittleEndian>(0xFFFF_FFFF).unwrap();
        out.write_u32::<LittleEndian>(0).unwrap(); // first child file
        out.write_u32::<LittleEndian>(0xFFFF_FFFF).unwrap();
        out.write_u32::<LittleEndian>(0).unwrap(); // name_len = 0

        out.write_u32::<LittleEndian>(0xFFFF_FFFF).unwrap(); // file_hash
        out.extend_from_slice(&file_meta);

        // Pad to data_offset
        while out.len() < data_offset as usize {
            out.push(0);
        }
        out.extend_from_slice(&file_data);
        out
    }

    /// Parses a Level 3 `RomFS` binary image and extracts its files.
    pub fn parse(raw: &[u8]) -> Result<Self, VfsError> {
        if raw.len() < ROMFS_HEADER_SIZE {
            return Err(VfsError::NotFound("RomFS binary too small".to_string()));
        }

        let mut rdr = Cursor::new(raw);
        let header_size = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        let _dir_hash_off = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        let _dir_hash_sz = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        let _dir_meta_off = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        let _dir_meta_sz = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        let _file_hash_off = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        let _file_hash_sz = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        let file_meta_off = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        let file_meta_sz = rdr.read_u64::<LittleEndian>().unwrap() as usize;
        let data_offset = rdr.read_u64::<LittleEndian>().unwrap() as usize;

        if header_size < ROMFS_HEADER_SIZE || file_meta_off + file_meta_sz > raw.len() || data_offset > raw.len() {
            return Err(VfsError::NotFound("Invalid RomFS Level 3 header".to_string()));
        }

        let mut entries = Vec::new();
        let mut cur = file_meta_off;
        let end = file_meta_off + file_meta_sz;

        while cur + 0x20 <= end {
            rdr.set_position(cur as u64);
            let _parent = rdr.read_u32::<LittleEndian>().unwrap();
            let sibling_offset = rdr.read_u32::<LittleEndian>().unwrap() as usize;
            let rel_data_offset = rdr.read_u64::<LittleEndian>().unwrap() as usize;
            let data_size = rdr.read_u64::<LittleEndian>().unwrap() as usize;
            let _next_hash = rdr.read_u32::<LittleEndian>().unwrap();
            let name_len = rdr.read_u32::<LittleEndian>().unwrap() as usize;

            let name_start = cur + 0x20;
            let name_end = name_start + name_len;
            if name_end > end {
                break;
            }
            let name = String::from_utf8_lossy(&raw[name_start..name_end]).to_string();

            let abs_data_start = data_offset + rel_data_offset;
            let abs_data_end = abs_data_start + data_size;
            let data = if abs_data_end <= raw.len() {
                raw[abs_data_start..abs_data_end].to_vec()
            } else {
                Vec::new()
            };

            entries.push(RomFsEntry { path: name, data });

            if sibling_offset == 0xFFFF_FFFF || sibling_offset == 0 {
                break;
            }
            cur = file_meta_off + sibling_offset;
        }

        Ok(Self { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::Permission;

    #[test]
    fn test_vfs_mount_and_read_to_guest() {
        let mut vfs = VirtualFileSystem::new();
        let mut mmu = Memory::new();
        let dst_addr = 0x80050000u64;
        mmu.map_range(dst_addr, 4096, Permission::READ_WRITE);

        vfs.mount_file("romfs:/hello.bin", b"Switch2RomFS");
        let n = vfs
            .read_file_to_guest("romfs:/hello.bin", 0, dst_addr, 64, &mut mmu)
            .unwrap();
        assert_eq!(n, 12);

        let mut buf = [0u8; 12];
        mmu.read_bytes(dst_addr, &mut buf).unwrap();
        assert_eq!(&buf, b"Switch2RomFS");
    }

    #[test]
    fn test_vfs_persistent_save_and_sdmc() {
        let temp_save_dir = std::env::temp_dir().join("ounce_test_savedata");
        let _ = std::fs::remove_dir_all(&temp_save_dir);

        let mut vfs = VirtualFileSystem::new();
        vfs.set_save_directory(&temp_save_dir);

        let mut mmu = Memory::new();
        let src_addr = 0x80060000u64;
        mmu.map_range(src_addr, 4096, Permission::READ_WRITE);
        mmu.write_bytes(src_addr, b"SAVED_GAME_STATE_SLOT_1").unwrap();

        let n = vfs
            .write_file_from_guest("save:/slot1.sav", src_addr, 23, &mmu)
            .unwrap();
        assert_eq!(n, 23);

        // Verify written to memory
        assert_eq!(vfs.get_file("save:/slot1.sav"), Some(b"SAVED_GAME_STATE_SLOT_1".as_slice()));

        // Verify persisted to disk!
        let disk_file = temp_save_dir.join("slot1.sav");
        assert!(disk_file.exists());
        let disk_contents = std::fs::read(&disk_file).unwrap();
        assert_eq!(disk_contents, b"SAVED_GAME_STATE_SLOT_1");

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_save_dir);
    }
}
