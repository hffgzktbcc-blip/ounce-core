#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use crate::mmu::{Memory, MemoryError};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;

#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("File not found in VFS: {0}")]
    NotFound(String),

    #[error("Invalid VFS file handle: {0}")]
    InvalidHandle(u32),

    #[error("Memory error during VFS transfer: {0}")]
    Memory(#[from] MemoryError),

    #[error("I/O error in VFS: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path already exists in VFS: {0}")]
    AlreadyExists(String),

    #[error("Invalid VFS path: {0}")]
    InvalidPath(String),

    #[error("Path leaves its mount or host folder: {0}")]
    Escape(String),

    #[error("Read-only VFS path: {0}")]
    ReadOnly(String),

    #[error("Directory not empty: {0}")]
    NotEmpty(String),

    #[error("File larger than the VFS limit: {0}")]
    TooLarge(String),
}

/// Largest file the VFS holds. Files live in memory (host-backed ones are loaded on open), so a guest
/// size or offset past this is refused instead of allocated.
pub const MAX_FILE_SIZE: u64 = 256 * 1024 * 1024;

/// What a VFS path names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsEntryKind {
    Directory,
    /// A file and its size in bytes.
    File(u64),
}

/// Resolves a path a guest sent to the filesystem mounted at `mount` (e.g. `"sdmc:/"`) into a normalized
/// VFS path. Empty and `.` components are dropped and `..` pops one level. A `..` above the mount root,
/// a backslash, a NUL, or another mount's scheme (`save:/x` sent to the SD card) is refused, so a guest
/// can only name paths inside its own mount.
pub fn resolve_guest_path(mount: &str, raw: &str) -> Result<String, VfsError> {
    let scheme = mount.trim_end_matches('/');
    let rest = match raw.find(':') {
        Some(i) if raw[..=i] == *scheme => &raw[i + 1..],
        Some(_) => return Err(VfsError::InvalidPath(raw.to_string())),
        None => raw,
    };
    if rest.contains(['\\', '\0']) {
        return Err(VfsError::InvalidPath(raw.to_string()));
    }
    let mut parts: Vec<&str> = Vec::new();
    for c in rest.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(VfsError::Escape(raw.to_string()));
                }
            }
            c => parts.push(c),
        }
    }
    Ok(format!("{}/{}", scheme, parts.join("/")))
}

/// Normalizes a virtual path to standard format (e.g. "romfs:/path" or "/path").
fn normalize_vfs_path(path: &str) -> String {
    let p = path.replace('\\', "/");
    let p = p.trim();
    if p.is_empty() {
        return "/".to_string();
    }
    if let Some(idx) = p.find(':') {
        let scheme = &p[..idx];
        let rest = p[idx + 1..].trim_start_matches('/');
        format!("{}:/{}", scheme, rest)
    } else {
        format!("/{}", p.trim_start_matches('/'))
    }
}

/// Horizon OS Virtual Filesystem (RomFS, SDMC, and SaveData).
pub struct VirtualFileSystem {
    /// Maps normalized virtual paths (e.g. "romfs:/title.txt") to file byte contents
    files: HashMap<String, Vec<u8>>,
    /// Maps open file handles to virtual paths
    open_handles: HashMap<u32, String>,
    /// `FsOpenMode` bits of handles opened through `open_file_with_mode`.
    open_modes: HashMap<u32, u32>,
    next_handle: u32,
    /// Explicitly tracked directories in VFS
    directories: HashSet<String>,
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
        let mut directories = HashSet::new();
        directories.insert("romfs:/".to_string());
        directories.insert("romfs:/shader".to_string());
        directories.insert("save:/".to_string());
        directories.insert("sdmc:/".to_string());

        let mut vfs = Self {
            files: HashMap::new(),
            open_handles: HashMap::new(),
            open_modes: HashMap::new(),
            next_handle: 1,
            directories,
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
        let norm = normalize_vfs_path(path);
        self.files.insert(norm.clone(), data.to_vec());
        if let Some(idx) = norm.rfind('/') {
            let parent = &norm[..idx];
            if !parent.is_empty() {
                self.directories.insert(parent.to_string());
                self.directories.insert(format!("{}/", parent));
            }
        }
        self.sync_file_to_disk(&norm, data);
    }

    /// Recursively mounts all files from a host filesystem directory under the given VFS prefix (e.g. "romfs").
    pub fn mount_host_directory(
        &mut self,
        dir_path: &std::path::Path,
        vfs_prefix: &str,
    ) -> std::io::Result<usize> {
        let mut count = 0;
        if !dir_path.exists() || !dir_path.is_dir() {
            return Ok(0);
        }
        let clean_prefix = vfs_prefix.trim_end_matches(':');

        fn scan_dir(
            dir: &std::path::Path,
            root: &std::path::Path,
            prefix: &str,
            files: &mut HashMap<String, Vec<u8>>,
            count: &mut usize,
        ) -> std::io::Result<()> {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    scan_dir(&path, root, prefix, files, count)?;
                } else if path.is_file() {
                    if let Ok(rel) = path.strip_prefix(root) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        let vfs_path = format!("{}:/{}", prefix, rel_str);
                        if let Ok(data) = std::fs::read(&path) {
                            files.insert(vfs_path, data);
                            *count += 1;
                        }
                    }
                }
            }
            Ok(())
        }

        scan_dir(dir_path, dir_path, clean_prefix, &mut self.files, &mut count)?;
        Ok(count)
    }

    /// Retrieves a reference to raw file data if it exists in the VFS or on disk.
    pub fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(|v| v.as_slice())
    }

    fn sync_file_to_disk(&self, path: &str, data: &[u8]) {
        if let Ok(Some(disk_path)) = self.host_path(path) {
            if let Some(parent) = disk_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&disk_path, data);
        }
    }

    /// Host path backing a `save:/` or `sdmc:/` path, or `None` when the path is on another mount or its
    /// mount has no host folder. Every component must be a plain name, and every component that exists
    /// on the host (symlinks included, dangling ones too) must resolve inside the host folder, so neither
    /// `..` nor a symlink placed in the folder reaches anything outside it.
    fn host_path(&self, path: &str) -> Result<Option<PathBuf>, VfsError> {
        let norm = normalize_vfs_path(path);
        let (root, rel) = if let Some(rel) = norm.strip_prefix("save:/") {
            (&self.save_dir, rel)
        } else if let Some(rel) = norm.strip_prefix("sdmc:/") {
            (&self.sdmc_dir, rel)
        } else {
            return Ok(None);
        };
        let Some(root) = root else { return Ok(None) };
        let rel = Path::new(rel.trim_matches('/'));
        if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(VfsError::Escape(path.to_string()));
        }
        let canon_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        let mut probe = root.clone();
        for c in rel.components() {
            probe.push(c);
            if std::fs::symlink_metadata(&probe).is_err() {
                break;
            }
            match probe.canonicalize() {
                Ok(real) if real.starts_with(&canon_root) => {}
                _ => return Err(VfsError::Escape(path.to_string())),
            }
        }
        Ok(Some(root.join(rel)))
    }

    /// Fails when `path` is on a host-backed mount and would leave its host folder.
    pub fn check_host_path(&self, path: &str) -> Result<(), VfsError> {
        self.host_path(path).map(|_| ())
    }

    /// What `path` names: an in-memory file, a host file or folder, or a directory known to the VFS.
    pub fn entry_kind(&self, path: &str) -> Option<VfsEntryKind> {
        let norm = normalize_vfs_path(path);
        if let Some(data) = self.files.get(&norm) {
            return Some(VfsEntryKind::File(data.len() as u64));
        }
        if let Ok(Some(p)) = self.host_path(&norm) {
            if let Ok(meta) = std::fs::metadata(&p) {
                return Some(if meta.is_dir() { VfsEntryKind::Directory } else { VfsEntryKind::File(meta.len()) });
            }
        }
        let clean = norm.trim_end_matches('/');
        let prefix = format!("{clean}/");
        let is_dir = clean.ends_with(':')
            || self.directories.contains(clean)
            || self.files.keys().any(|k| k.starts_with(&prefix))
            || self.directories.iter().any(|d| d.starts_with(&prefix));
        is_dir.then_some(VfsEntryKind::Directory)
    }

    /// Loads a host-backed file into memory if it is not there yet. False when there is no such file.
    fn ensure_loaded(&mut self, norm: &str) -> Result<bool, VfsError> {
        if self.files.contains_key(norm) {
            return Ok(true);
        }
        let Some(p) = self.host_path(norm)? else { return Ok(false) };
        match std::fs::metadata(&p) {
            Ok(meta) if meta.is_file() => {
                if meta.len() > MAX_FILE_SIZE {
                    return Err(VfsError::TooLarge(norm.to_string()));
                }
                let data = std::fs::read(&p)?;
                self.files.insert(norm.to_string(), data);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Refuses writes to `romfs:/`, which is read-only.
    fn check_writable(norm: &str) -> Result<(), VfsError> {
        if norm.starts_with("romfs:/") {
            return Err(VfsError::ReadOnly(norm.to_string()));
        }
        Ok(())
    }

    /// The parent of `norm` must be a directory.
    pub fn check_parent(&self, norm: &str) -> Result<(), VfsError> {
        let clean = norm.trim_end_matches('/');
        let parent = match clean.rfind('/') {
            Some(i) if clean[..i].ends_with(':') => &clean[..=i],
            Some(i) => &clean[..i],
            None => return Err(VfsError::InvalidPath(norm.to_string())),
        };
        match self.entry_kind(parent) {
            Some(VfsEntryKind::Directory) => Ok(()),
            _ => Err(VfsError::NotFound(parent.to_string())),
        }
    }

    /// Creates a zero-filled file of `size` bytes. Fails if the path exists or its parent does not.
    pub fn create_file(&mut self, path: &str, size: u64) -> Result<(), VfsError> {
        let norm = normalize_vfs_path(path);
        Self::check_writable(&norm)?;
        self.host_path(&norm)?;
        if self.entry_kind(&norm).is_some() {
            return Err(VfsError::AlreadyExists(norm));
        }
        self.check_parent(&norm)?;
        if size > MAX_FILE_SIZE {
            return Err(VfsError::TooLarge(norm));
        }
        let data = vec![0u8; size as usize];
        self.sync_file_to_disk(&norm, &data);
        self.files.insert(norm, data);
        Ok(())
    }

    /// Writes `data` at `offset` into an existing file, growing it if needed.
    pub fn write_file_at(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<usize, VfsError> {
        let norm = normalize_vfs_path(path);
        Self::check_writable(&norm)?;
        let end = offset.checked_add(data.len() as u64).filter(|&e| e <= MAX_FILE_SIZE);
        let Some(end) = end else { return Err(VfsError::TooLarge(norm)) };
        if !self.ensure_loaded(&norm)? {
            return Err(VfsError::NotFound(norm));
        }
        let file = self.files.get_mut(&norm).ok_or_else(|| VfsError::NotFound(norm.clone()))?;
        if (file.len() as u64) < end {
            file.resize(end as usize, 0);
        }
        file[offset as usize..end as usize].copy_from_slice(data);
        if let Some(p) = self.host_path(&norm)?.filter(|p| p.is_file()) {
            use std::io::{Seek, SeekFrom, Write};
            let mut f = std::fs::OpenOptions::new().write(true).open(p)?;
            f.seek(SeekFrom::Start(offset))?;
            f.write_all(data)?;
        }
        self.total_bytes_written += data.len() as u64;
        Ok(data.len())
    }

    /// Truncates or zero-extends an existing file to `size` bytes.
    pub fn set_file_size(&mut self, path: &str, size: u64) -> Result<(), VfsError> {
        let norm = normalize_vfs_path(path);
        Self::check_writable(&norm)?;
        if size > MAX_FILE_SIZE {
            return Err(VfsError::TooLarge(norm));
        }
        if !self.ensure_loaded(&norm)? {
            return Err(VfsError::NotFound(norm));
        }
        if let Some(file) = self.files.get_mut(&norm) {
            file.resize(size as usize, 0);
        }
        if let Some(p) = self.host_path(&norm)?.filter(|p| p.is_file()) {
            std::fs::OpenOptions::new().write(true).open(p)?.set_len(size)?;
        }
        Ok(())
    }

    /// Renames a file or directory (with everything under it). The target must not exist and its parent must.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), VfsError> {
        let (from, to) = (normalize_vfs_path(from), normalize_vfs_path(to));
        let (from, to) = (from.trim_end_matches('/').to_string(), to.trim_end_matches('/').to_string());
        Self::check_writable(&from)?;
        Self::check_writable(&to)?;
        let (host_from, host_to) = (self.host_path(&from)?, self.host_path(&to)?);
        if from.ends_with(':') || to.ends_with(':') || to.starts_with(&format!("{from}/")) {
            return Err(VfsError::InvalidPath(to));
        }
        if self.entry_kind(&from).is_none() {
            return Err(VfsError::NotFound(from));
        }
        if self.entry_kind(&to).is_some() {
            return Err(VfsError::AlreadyExists(to));
        }
        self.check_parent(&to)?;
        if let (Some(a), Some(b)) = (host_from, host_to) {
            if std::fs::symlink_metadata(&a).is_ok() {
                std::fs::rename(a, b)?;
            }
        }
        let moved = |k: &str| -> Option<String> {
            if k == from {
                Some(to.clone())
            } else {
                k.strip_prefix(&from).filter(|r| r.starts_with('/')).map(|r| format!("{to}{r}"))
            }
        };
        self.files = std::mem::take(&mut self.files).into_iter().map(|(k, v)| (moved(&k).unwrap_or(k), v)).collect();
        self.directories = std::mem::take(&mut self.directories).into_iter().map(|k| moved(&k).unwrap_or(k)).collect();
        for v in self.open_handles.values_mut() {
            if let Some(n) = moved(v) {
                *v = n;
            }
        }
        Ok(())
    }

    /// Deletes everything inside a directory and keeps the directory.
    pub fn clean_directory(&mut self, path: &str) -> Result<(), VfsError> {
        let norm = normalize_vfs_path(path);
        Self::check_writable(&norm)?;
        let base = norm.trim_end_matches('/');
        for name in self.list_directory(&norm)? {
            let child = format!("{base}/{name}");
            match self.entry_kind(&child) {
                Some(VfsEntryKind::Directory) => self.delete_directory(&child)?,
                Some(VfsEntryKind::File(_)) => self.delete_file(&child)?,
                None => {}
            }
        }
        Ok(())
    }

    /// Host creation, modification and access times (seconds since the Unix epoch) of a host-backed file.
    pub fn timestamps(&self, path: &str) -> Option<(u64, u64, u64)> {
        let meta = std::fs::metadata(self.host_path(path).ok()??).ok()?;
        let secs = |t: std::io::Result<std::time::SystemTime>| {
            t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
        };
        Some((secs(meta.created()), secs(meta.modified()), secs(meta.accessed())))
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
        if !self.ensure_loaded(path)? {
            return Err(VfsError::NotFound(path.to_string()));
        }
        let handle = self.next_handle;
        self.next_handle += 1;
        self.open_handles.insert(handle, path.to_string());
        Ok(handle)
    }

    /// Opens a file and records its `FsOpenMode` bits (1 read, 2 write, 4 append).
    pub fn open_file_with_mode(&mut self, path: &str, mode: u32) -> Result<u32, VfsError> {
        let handle = self.open_file(&normalize_vfs_path(path))?;
        self.open_modes.insert(handle, mode);
        Ok(handle)
    }

    /// `FsOpenMode` bits of an open handle (read only when it was opened without a mode).
    pub fn handle_mode(&self, handle: u32) -> u32 {
        self.open_modes.get(&handle).copied().unwrap_or(1)
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

    /// Path of an open file handle.
    pub fn handle_path(&self, handle: u32) -> Option<&str> {
        self.open_handles.get(&handle).map(|p| p.as_str())
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
        let norm = normalize_vfs_path(path);
        Self::check_writable(&norm)?;
        if len as u64 > MAX_FILE_SIZE {
            return Err(VfsError::TooLarge(norm));
        }
        let mut buf = vec![0u8; len];
        mmu.read_bytes(guest_src_addr, &mut buf)?;
        self.files.insert(norm.clone(), buf.clone());
        if let Some(idx) = norm.rfind('/') {
            let parent = &norm[..idx];
            if !parent.is_empty() {
                self.directories.insert(parent.to_string());
                self.directories.insert(format!("{}/", parent));
            }
        }
        self.sync_file_to_disk(&norm, &buf);
        self.total_bytes_written += len as u64;
        Ok(len)
    }

    /// Enumerates directory contents, returning the direct child file and folder names.
    pub fn list_directory(&self, path: &str) -> Result<Vec<String>, VfsError> {
        let norm = normalize_vfs_path(path);
        let prefix = if norm.ends_with('/') {
            norm.clone()
        } else {
            format!("{}/", norm)
        };

        let mut entries = std::collections::BTreeSet::new();
        let mut dir_found = false;

        let norm_no_slash = norm.trim_end_matches('/');
        if norm == "romfs:/" || norm == "save:/" || norm == "sdmc:/"
            || norm_no_slash == "romfs:" || norm_no_slash == "save:" || norm_no_slash == "sdmc:"
            || self.directories.contains(&norm) || self.directories.contains(norm_no_slash)
        {
            dir_found = true;
        }

        // 1. Files in memory
        for file_path in self.files.keys() {
            if file_path.starts_with(&prefix) {
                dir_found = true;
                let rel = &file_path[prefix.len()..];
                if !rel.is_empty() {
                    let entry = match rel.find('/') {
                        Some(idx) => &rel[..idx],
                        None => rel,
                    };
                    if !entry.is_empty() {
                        entries.insert(entry.to_string());
                    }
                }
            }
        }

        // 2. Explicit directories
        for d in &self.directories {
            let d_clean = d.trim_end_matches('/');
            let prefix_clean = prefix.trim_end_matches('/');
            if d_clean.starts_with(prefix_clean) && d_clean.len() > prefix_clean.len() {
                let rel = d_clean[prefix_clean.len()..].trim_start_matches('/');
                if !rel.is_empty() {
                    let entry = match rel.find('/') {
                        Some(idx) => &rel[..idx],
                        None => rel,
                    };
                    if !entry.is_empty() {
                        entries.insert(entry.to_string());
                    }
                }
            }
        }

        // 3. Host folder behind save:/ and sdmc:/
        if let Ok(Some(disk_dir)) = self.host_path(&norm) {
            if disk_dir.is_dir() {
                dir_found = true;
                if let Ok(rd) = std::fs::read_dir(&disk_dir) {
                    for e in rd.flatten() {
                        entries.insert(e.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }

        if !dir_found {
            return Err(VfsError::NotFound(path.to_string()));
        }

        Ok(entries.into_iter().collect())
    }

    /// Creates a directory in the VFS and persistent storage if mounted.
    pub fn create_directory(&mut self, path: &str) -> Result<(), VfsError> {
        let norm = normalize_vfs_path(path);
        let clean = norm.trim_end_matches('/').to_string();
        Self::check_writable(&clean)?;
        let host = self.host_path(&clean)?;
        self.directories.insert(clean.clone());
        self.directories.insert(format!("{}/", clean));

        // Register intermediate parent directories
        let mut curr = clean.as_str();
        while let Some(idx) = curr.rfind('/') {
            let parent = &curr[..idx];
            if !parent.is_empty() && parent != "save:" && parent != "sdmc:" && parent != "romfs:" {
                self.directories.insert(parent.to_string());
                self.directories.insert(format!("{}/", parent));
                curr = parent;
            } else {
                break;
            }
        }

        if let Some(disk_path) = host {
            std::fs::create_dir_all(&disk_path)?;
        }
        Ok(())
    }

    /// Deletes a file from the VFS and persistent storage if mounted.
    pub fn delete_file(&mut self, path: &str) -> Result<(), VfsError> {
        let norm = normalize_vfs_path(path);
        Self::check_writable(&norm)?;
        let host = self.host_path(&norm)?;
        let removed = self.files.remove(&norm).is_some();
        self.open_handles.retain(|_, v| v != &norm);

        let mut disk_removed = false;
        if let Some(disk_path) = host {
            if disk_path.is_file() {
                std::fs::remove_file(&disk_path)?;
                disk_removed = true;
            }
        }

        if !removed && !disk_removed {
            return Err(VfsError::NotFound(path.to_string()));
        }
        Ok(())
    }

    /// Deletes a directory and its contents from the VFS and persistent storage.
    pub fn delete_directory(&mut self, path: &str) -> Result<(), VfsError> {
        let norm = normalize_vfs_path(path);
        let clean = norm.trim_end_matches('/').to_string();
        Self::check_writable(&clean)?;
        // Never the mount root: on a host-backed mount that would remove the whole host folder.
        if clean.ends_with(':') {
            return Err(VfsError::InvalidPath(path.to_string()));
        }
        let host = self.host_path(&clean)?;
        let prefix = format!("{}/", clean);

        let dir_existed = self.directories.remove(&clean) || self.directories.remove(&prefix);

        let to_remove: Vec<String> = self.files.keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        for k in &to_remove {
            self.files.remove(k);
            self.open_handles.retain(|_, v| v != k);
        }

        let subdirs_removed = {
            let before = self.directories.len();
            self.directories.retain(|d| !d.starts_with(&prefix) && d != &clean);
            self.directories.len() < before
        };

        let mut disk_removed = false;
        if let Some(disk_path) = host {
            if disk_path.is_dir() {
                std::fs::remove_dir_all(&disk_path)?;
                disk_removed = true;
            }
        }

        if !dir_existed && !subdirs_removed && to_remove.is_empty() && !disk_removed {
            return Err(VfsError::NotFound(path.to_string()));
        }
        Ok(())
    }

    /// Checks if a file exists in the VFS.
    pub fn file_exists(&self, path: &str) -> bool {
        let norm = normalize_vfs_path(path);
        self.files.contains_key(&norm)
    }

    /// Checks if a directory exists in the VFS.
    pub fn directory_exists(&self, path: &str) -> bool {
        self.list_directory(path).is_ok()
    }

    /// Returns free space size in bytes for a virtual volume (e.g. 64 GB).
    pub fn get_free_space_size(&self, _path: &str) -> u64 {
        64 * 1024 * 1024 * 1024
    }

    /// Returns total space size in bytes for a virtual volume (e.g. 128 GB).
    pub fn get_total_space_size(&self, _path: &str) -> u64 {
        128 * 1024 * 1024 * 1024
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

    /// Builds a Level 3 `RomFS` binary image.
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

        if header_size < ROMFS_HEADER_SIZE || file_meta_off.checked_add(file_meta_sz).is_none_or(|e| e > raw.len()) || data_offset > raw.len() {
            return Err(VfsError::NotFound("Invalid RomFS Level 3 header".to_string()));
        }

        let mut entries = Vec::new();
        let mut cur = file_meta_off;
        let end = file_meta_off + file_meta_sz;
        // Each entry takes at least 0x20 bytes, so more than this many means the sibling chain loops.
        let max_entries = file_meta_sz / 0x20;

        while cur + 0x20 <= end && entries.len() <= max_entries {
            rdr.set_position(cur as u64);
            let _parent = rdr.read_u32::<LittleEndian>().unwrap();
            let sibling_offset = rdr.read_u32::<LittleEndian>().unwrap() as usize;
            let rel_data_offset = rdr.read_u64::<LittleEndian>().unwrap() as usize;
            let data_size = rdr.read_u64::<LittleEndian>().unwrap() as usize;
            let _next_hash = rdr.read_u32::<LittleEndian>().unwrap();
            let name_len = rdr.read_u32::<LittleEndian>().unwrap() as usize;

            let name_start = cur + 0x20;
            let Some(name_end) = name_start.checked_add(name_len) else { break };
            if name_end > end {
                break;
            }
            let name = String::from_utf8_lossy(&raw[name_start..name_end]).to_string();

            let data = if let Some((abs_data_start, abs_data_end)) = data_offset
                .checked_add(rel_data_offset)
                .and_then(|s| Some((s, s.checked_add(data_size)?)))
                .filter(|&(_, e)| e <= raw.len())
            {
                raw[abs_data_start..abs_data_end].to_vec()
            } else {
                Vec::new()
            };

            entries.push(RomFsEntry { path: name, data });

            if sibling_offset == 0xFFFF_FFFF || sibling_offset == 0 {
                break;
            }
            let Some(next) = file_meta_off.checked_add(sibling_offset) else { break };
            cur = next;
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

    #[test]
    fn test_vfs_mount_host_directory_recursive() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ounce_vfs_host_mount_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sub = temp_dir.join("sub/folder");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(temp_dir.join("root_asset.txt"), b"RootAsset").unwrap();
        std::fs::write(sub.join("nested_mesh.bin"), b"MeshBinaryData").unwrap();

        let mut vfs = VirtualFileSystem::new();
        let mounted = vfs
            .mount_host_directory(&temp_dir, "romfs")
            .expect("Failed to mount host dir");
        assert_eq!(mounted, 2);

        assert_eq!(
            vfs.get_file("romfs:/root_asset.txt"),
            Some(b"RootAsset".as_slice())
        );
        assert_eq!(
            vfs.get_file("romfs:/sub/folder/nested_mesh.bin"),
            Some(b"MeshBinaryData".as_slice())
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_vfs_directory_enumeration_and_save_storage_helpers() {
        let temp_save = std::env::temp_dir().join(format!(
            "ounce_vfs_test_save_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_save);

        let mut vfs = VirtualFileSystem::new();
        vfs.set_save_directory(&temp_save);

        // 1. Check built-in romfs root enumeration
        let romfs_entries = vfs.list_directory("romfs:/").unwrap();
        assert!(romfs_entries.contains(&"title.txt".to_string()));
        assert!(romfs_entries.contains(&"shader".to_string()));

        // 2. Subdirectory enumeration
        let shader_entries = vfs.list_directory("romfs:/shader").unwrap();
        assert_eq!(shader_entries, vec!["default_vs.glsl".to_string()]);

        // 3. Create directory in persistent save
        vfs.create_directory("save:/profiles/slot0").unwrap();
        assert!(vfs.directory_exists("save:/profiles"));
        assert!(vfs.directory_exists("save:/profiles/slot0"));
        assert!(temp_save.join("profiles/slot0").is_dir());

        // 4. Create files and enumerate
        vfs.mount_file("save:/profiles/slot0/meta.json", b"{\"name\":\"Hero\"}");
        vfs.mount_file("save:/profiles/slot0/data.bin", b"DATA0123");
        let slot0_files = vfs.list_directory("save:/profiles/slot0").unwrap();
        assert_eq!(slot0_files, vec!["data.bin".to_string(), "meta.json".to_string()]);

        // 5. Delete file
        vfs.delete_file("save:/profiles/slot0/data.bin").unwrap();
        assert!(!vfs.file_exists("save:/profiles/slot0/data.bin"));
        assert!(!temp_save.join("profiles/slot0/data.bin").exists());

        let remaining = vfs.list_directory("save:/profiles/slot0").unwrap();
        assert_eq!(remaining, vec!["meta.json".to_string()]);

        // 6. Delete directory
        vfs.delete_directory("save:/profiles").unwrap();
        assert!(!vfs.file_exists("save:/profiles/slot0/meta.json"));
        assert!(!temp_save.join("profiles").exists());

        // 7. Space queries
        let free_space = vfs.get_free_space_size("save:/");
        assert_eq!(free_space, 64 * 1024 * 1024 * 1024);
        let total_space = vfs.get_total_space_size("save:/");
        assert_eq!(total_space, 128 * 1024 * 1024 * 1024);

        let _ = std::fs::remove_dir_all(&temp_save);
    }
}
