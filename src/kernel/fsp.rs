//! `fsp-srv` filesystem objects for libnx homebrew: `IFileSystem`, `IFile` and `IDirectory`, with the
//! command IDs and argument layouts of libnx's public `fs.c`. Paths come from the X buffers (`FsPath`),
//! file data from the A (write) and B (read) buffers, and directory entries go to the B buffer as
//! `FsDirectoryEntry` (0x310 bytes).
//!
//! Every guest path goes through `resolve_guest_path`, so a guest cannot leave its mount (`..` above the
//! root, another mount's scheme), and the VFS re-checks host-backed paths (`run --sdmc=<dir>`) so they
//! cannot leave the host folder. The harness's inline form (no HIPC header, path in the CMIF data) keeps
//! working: `path_arg` falls back to it.

use super::ipc::{IpcManager, ServiceType};
use crate::fs::{resolve_guest_path, VfsEntryKind, VfsError, VirtualFileSystem, MAX_FILE_SIZE};
use crate::mmu::Memory;

/// A result in Horizon's `fs` module (2). libnx's fsdev maps not-found and already-exists to
/// `ENOENT` / `EEXIST`; for the rest it only checks `R_FAILED`.
const fn fs_result(desc: u32) -> u32 {
    desc << 9 | 2
}
pub const RESULT_PATH_NOT_FOUND: u32 = fs_result(1);
pub const RESULT_PATH_ALREADY_EXISTS: u32 = fs_result(2);
pub const RESULT_DIRECTORY_NOT_EMPTY: u32 = fs_result(8);
pub const RESULT_USABLE_SPACE_NOT_ENOUGH: u32 = fs_result(30);
pub const RESULT_INVALID_PATH: u32 = fs_result(6001);
pub const RESULT_DIRECTORY_UNOBTAINABLE: u32 = fs_result(6006);
pub const RESULT_INVALID_OPEN_MODE: u32 = fs_result(6200);
pub const RESULT_UNSUPPORTED_OPERATION: u32 = fs_result(6300);

/// `FsDirectoryEntry`: name[0x301], pad[3], type (s8) at 0x304, pad[3], file_size (s64) at 0x308.
pub const DIRECTORY_ENTRY_SIZE: u64 = 0x310;
/// `FsDirEntryType`.
const ENTRY_TYPE_DIR: u8 = 0;
const ENTRY_TYPE_FILE: u8 = 1;
/// `FsDirOpenMode`.
const DIR_OPEN_READ_DIRS: u32 = 1;
const DIR_OPEN_READ_FILES: u32 = 2;
/// `FsOpenMode`.
const OPEN_WRITE: u32 = 2;

fn vfs_result(e: &VfsError) -> u32 {
    match e {
        VfsError::NotFound(_) | VfsError::InvalidHandle(_) => RESULT_PATH_NOT_FOUND,
        VfsError::AlreadyExists(_) => RESULT_PATH_ALREADY_EXISTS,
        VfsError::NotEmpty(_) => RESULT_DIRECTORY_NOT_EMPTY,
        VfsError::TooLarge(_) => RESULT_USABLE_SPACE_NOT_ENOUGH,
        VfsError::InvalidPath(_) => RESULT_INVALID_PATH,
        VfsError::Escape(_) => RESULT_DIRECTORY_UNOBTAINABLE,
        VfsError::ReadOnly(_) | VfsError::Io(_) | VfsError::Memory(_) => RESULT_UNSUPPORTED_OPERATION,
    }
}

fn u64_words(v: u64) -> [u32; 2] {
    [v as u32, (v >> 32) as u32]
}

/// Child path of a resolved directory path (`sdmc:/` + `a` or `sdmc:/d` + `a`).
fn child_path(dir: &str, name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches('/'), name)
}

impl IpcManager {
    /// Path argument `index` (X buffer `index`; the first falls back to the CMIF data at `fallback`),
    /// resolved against `mount` and checked against the mount's host folder.
    fn fs_path(&self, mmu: &Memory, vfs: &VirtualFileSystem, index: usize, fallback: u64, mount: &str) -> Result<String, VfsError> {
        let path = resolve_guest_path(mount, &self.path_arg_at(mmu, index, fallback))?;
        vfs.check_host_path(&path)?;
        Ok(path)
    }

    /// `IFileSystem` commands on the filesystem mounted at `mount` (`"sdmc:/"` or `"save:/"`).
    pub(super) fn handle_filesystem(
        &mut self,
        mount: &str,
        cmd_id: u32,
        args_base: u64,
        tls_addr: u64,
        mmu: &mut Memory,
        vfs: &mut VirtualFileSystem,
    ) -> u64 {
        let kind = |vfs: &VirtualFileSystem, p: &str| vfs.entry_kind(p);
        let want_file = |vfs: &VirtualFileSystem, p: &str| match kind(vfs, p) {
            Some(VfsEntryKind::File(size)) => Ok(size),
            _ => Err(VfsError::NotFound(p.to_string())),
        };
        let want_dir = |vfs: &VirtualFileSystem, p: &str| match kind(vfs, p) {
            Some(VfsEntryKind::Directory) => Ok(()),
            _ => Err(VfsError::NotFound(p.to_string())),
        };
        // (result, out words, new object)
        let outcome: Result<(Vec<u32>, Option<u32>), VfsError> = (|| match cmd_id {
            // CreateFile(u32 option, s64 size), path in X0
            0 => {
                let size = mmu.read_u64(args_base + 8).unwrap_or(0);
                let path = self.fs_path(mmu, vfs, 0, args_base + 16, mount)?;
                vfs.create_file(&path, size)?;
                Ok((vec![], None))
            }
            // DeleteFile
            1 => {
                let path = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                want_file(vfs, &path)?;
                vfs.delete_file(&path)?;
                Ok((vec![], None))
            }
            // CreateDirectory
            2 => {
                let path = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                if kind(vfs, &path).is_some() {
                    return Err(VfsError::AlreadyExists(path));
                }
                vfs.check_parent(&path)?;
                vfs.create_directory(&path)?;
                Ok((vec![], None))
            }
            // DeleteDirectory: the directory must be empty.
            3 => {
                let path = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                want_dir(vfs, &path)?;
                if !vfs.list_directory(&path)?.is_empty() {
                    return Err(VfsError::NotEmpty(path));
                }
                vfs.delete_directory(&path)?;
                Ok((vec![], None))
            }
            // DeleteDirectoryRecursively
            4 => {
                let path = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                want_dir(vfs, &path)?;
                vfs.delete_directory(&path)?;
                Ok((vec![], None))
            }
            // RenameFile / RenameDirectory: old path in X0, new path in X1.
            5 | 6 => {
                let from = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                let to = self.fs_path(mmu, vfs, 1, args_base + 0x301, mount)?;
                if cmd_id == 5 {
                    want_file(vfs, &from)?;
                } else {
                    want_dir(vfs, &from)?;
                }
                vfs.rename(&from, &to)?;
                Ok((vec![], None))
            }
            // GetEntryType -> u32 FsDirEntryType
            7 => {
                let path = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                match kind(vfs, &path) {
                    Some(VfsEntryKind::Directory) => Ok((vec![ENTRY_TYPE_DIR as u32], None)),
                    Some(VfsEntryKind::File(_)) => Ok((vec![ENTRY_TYPE_FILE as u32], None)),
                    None => Err(VfsError::NotFound(path)),
                }
            }
            // OpenFile(u32 mode) -> IFile
            8 => {
                let mode = mmu.read_u32(args_base).unwrap_or(0);
                let path = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                want_file(vfs, &path)?;
                let fh = vfs.open_file_with_mode(&path, if mode == 0 { 1 } else { mode })?;
                let h = self.allocate_handle(ServiceType::File(fh));
                Ok((vec![h], Some(h)))
            }
            // OpenDirectory(u32 mode) -> IDirectory, entries filtered by FsDirOpenMode.
            9 => {
                let mode = mmu.read_u32(args_base).unwrap_or(0);
                let path = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                want_dir(vfs, &path)?;
                let filter = match mode & (DIR_OPEN_READ_DIRS | DIR_OPEN_READ_FILES) {
                    0 => DIR_OPEN_READ_DIRS | DIR_OPEN_READ_FILES,
                    m => m,
                };
                let entries = vfs
                    .list_directory(&path)?
                    .into_iter()
                    .filter(|name| match kind(vfs, &child_path(&path, name)) {
                        Some(VfsEntryKind::Directory) => filter & DIR_OPEN_READ_DIRS != 0,
                        Some(VfsEntryKind::File(_)) => filter & DIR_OPEN_READ_FILES != 0,
                        None => false,
                    })
                    .collect();
                let h = self.allocate_handle(ServiceType::Directory { path, entries, index: 0 });
                Ok((vec![h], Some(h)))
            }
            // Commit
            10 => Ok((vec![], None)),
            // GetFreeSpaceSize / GetTotalSpaceSize -> u64
            11 => Ok((u64_words(vfs.get_free_space_size(mount)).to_vec(), None)),
            12 => Ok((u64_words(vfs.get_total_space_size(mount)).to_vec(), None)),
            // CleanDirectoryRecursively
            13 => {
                let path = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                want_dir(vfs, &path)?;
                vfs.clean_directory(&path)?;
                Ok((vec![], None))
            }
            // GetFileTimeStampRaw -> FsTimeStampRaw { created, modified, accessed: u64, is_valid: u8, pad[7] }
            14 => {
                let path = self.fs_path(mmu, vfs, 0, args_base, mount)?;
                want_file(vfs, &path)?;
                let mut out = vec![0u32; 8];
                if let Some((c, m, a)) = vfs.timestamps(&path) {
                    out[0..2].copy_from_slice(&u64_words(c));
                    out[2..4].copy_from_slice(&u64_words(m));
                    out[4..6].copy_from_slice(&u64_words(a));
                    out[6] = 1;
                }
                Ok((out, None))
            }
            _ => Ok((vec![0], None)),
        })();
        match outcome {
            Ok((words, object)) => self.write_cmif_response(tls_addr, mmu, 0, &words, object),
            Err(e) => self.write_cmif_response(tls_addr, mmu, vfs_result(&e), &[], None),
        }
        0
    }

    /// `IFile` commands on VFS handle `handle`.
    pub(super) fn handle_file(
        &mut self,
        handle: u32,
        cmd_id: u32,
        args_base: u64,
        tls_addr: u64,
        mmu: &mut Memory,
        vfs: &mut VirtualFileSystem,
    ) -> u64 {
        let Some(path) = vfs.handle_path(handle).map(str::to_string) else {
            self.write_cmif_response(tls_addr, mmu, RESULT_PATH_NOT_FOUND, &[], None);
            return 0;
        };
        let writable = vfs.handle_mode(handle) & OPEN_WRITE != 0;
        let outcome: Result<Vec<u32>, VfsError> = (|| match cmd_id {
            // Read(u32 option, s64 offset, u64 size) -> u64 bytes read, data into the B buffer
            0 => {
                let offset = mmu.read_u64(args_base + 8).unwrap_or(0) as usize;
                let size = mmu.read_u64(args_base + 16).unwrap_or(0) as usize;
                let buf = self.request.recv_buffers.first().copied().unwrap_or_default();
                let len = size.min(buf.size as usize);
                let read = vfs.read_file_to_guest(&path, offset, buf.addr, len, mmu).unwrap_or(0);
                Ok(u64_words(read as u64).to_vec())
            }
            // Write(u32 option, s64 offset, u64 size), data from the A buffer
            1 => {
                if !writable {
                    return Err(VfsError::ReadOnly(path.clone()));
                }
                let offset = mmu.read_u64(args_base + 8).unwrap_or(0);
                let size = mmu.read_u64(args_base + 16).unwrap_or(0);
                let buf = self.request.send_buffers.first().copied().unwrap_or_default();
                let len = size.min(buf.size);
                if len > MAX_FILE_SIZE {
                    return Err(VfsError::TooLarge(path.clone()));
                }
                let mut data = vec![0u8; len as usize];
                mmu.read_bytes(buf.addr, &mut data)?;
                vfs.write_file_at(&path, offset, &data)?;
                Ok(vec![])
            }
            // Flush: writes already reached the host.
            2 => Ok(vec![]),
            // SetSize(s64 size)
            3 => {
                if !writable {
                    return Err(VfsError::ReadOnly(path.clone()));
                }
                vfs.set_file_size(&path, mmu.read_u64(args_base).unwrap_or(0))?;
                Ok(vec![])
            }
            // GetSize -> u64
            4 => Ok(u64_words(vfs.get_file_size(handle)? as u64).to_vec()),
            _ => Ok(vec![0]),
        })();
        match outcome {
            Ok(words) => self.write_cmif_response(tls_addr, mmu, 0, &words, None),
            Err(VfsError::ReadOnly(_)) => self.write_cmif_response(tls_addr, mmu, RESULT_INVALID_OPEN_MODE, &[], None),
            Err(e) => self.write_cmif_response(tls_addr, mmu, vfs_result(&e), &[], None),
        }
        0
    }

    /// `IDirectory` commands. `entries` were listed (and filtered) when the directory was opened.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_directory(
        &mut self,
        session_handle: u32,
        path: String,
        entries: Vec<String>,
        mut index: usize,
        cmd_id: u32,
        args_base: u64,
        tls_addr: u64,
        mmu: &mut Memory,
        vfs: &mut VirtualFileSystem,
    ) -> u64 {
        match cmd_id {
            // Read -> s64 entries read, FsDirectoryEntry array into the B buffer. The harness's inline
            // form passes the buffer address and an entry limit (0 = all) in the CMIF data instead.
            0 => {
                let (buf_addr, max) = match self.request.recv_buffers.first().filter(|b| b.size > 0) {
                    Some(b) => (b.addr, (b.size / DIRECTORY_ENTRY_SIZE) as usize),
                    None => {
                        let max = mmu.read_u64(args_base + 8).unwrap_or(0) as usize;
                        (mmu.read_u64(args_base).unwrap_or(0), if max == 0 { usize::MAX } else { max })
                    }
                };
                let remaining = entries.len().saturating_sub(index);
                let count = remaining.min(max);
                if buf_addr >= 0x10000 && mmu.is_mapped(buf_addr) {
                    for (i, name) in entries[index..index + count].iter().enumerate() {
                        let mut entry = [0u8; DIRECTORY_ENTRY_SIZE as usize];
                        let n = name.len().min(0x300);
                        entry[..n].copy_from_slice(&name.as_bytes()[..n]);
                        let (ty, size) = match vfs.entry_kind(&child_path(&path, name)) {
                            Some(VfsEntryKind::File(size)) => (ENTRY_TYPE_FILE, size),
                            _ => (ENTRY_TYPE_DIR, 0),
                        };
                        entry[0x304] = ty;
                        entry[0x308..0x310].copy_from_slice(&size.to_le_bytes());
                        let _ = mmu.write_bytes(buf_addr + i as u64 * DIRECTORY_ENTRY_SIZE, &entry);
                    }
                }
                index += count;
                self.handles.insert(session_handle, ServiceType::Directory { path, entries, index });
                self.write_cmif_response(tls_addr, mmu, 0, &u64_words(count as u64), None);
            }
            // GetEntryCount -> s64
            1 => self.write_cmif_response(tls_addr, mmu, 0, &u64_words(entries.len() as u64), None),
            _ => self.write_cmif_response(tls_addr, mmu, 0, &[0], None),
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioOutServer;
    use crate::gpu::nvgpu::NvgpuChannel;
    use crate::kernel::ipc::CMIF_IN_MAGIC;
    use crate::mmu::Permission;
    use std::path::{Path, PathBuf};

    const TLS: u64 = 0x8000_0000;
    const PATH0: u64 = 0x8100_0000;
    const PATH1: u64 = 0x8100_1000;
    const BUF: u64 = 0x8200_0000;
    const BUF_SIZE: u64 = 0x4000;

    /// A libnx-shaped `fsp-srv` session: requests carry a HIPC header with X buffers for paths and an
    /// A or B buffer for data, exactly as libnx's `fs.c` sends them.
    struct Fsp {
        mmu: Memory,
        ipc: IpcManager,
        vfs: VirtualFileSystem,
        nvgpu: NvgpuChannel,
        audio: AudioOutServer,
    }

    #[derive(Clone, Copy)]
    enum Data<'a> {
        None,
        /// A buffer (guest -> service).
        Send(&'a [u8]),
        /// B buffer of this many bytes (service -> guest).
        Recv(u64),
    }

    impl Fsp {
        fn new(sdmc: Option<&Path>) -> Self {
            let mut mmu = Memory::new();
            mmu.map_range(TLS, 0x1000, Permission::READ_WRITE);
            mmu.map_range(PATH0, 0x2000, Permission::READ_WRITE);
            mmu.map_range(BUF, BUF_SIZE as usize, Permission::READ_WRITE);
            let mut vfs = VirtualFileSystem::new();
            if let Some(dir) = sdmc {
                vfs.set_sdmc_directory(dir);
            }
            Fsp { mmu, ipc: IpcManager::new(), vfs, nvgpu: NvgpuChannel::new(), audio: AudioOutServer::new() }
        }

        /// Sends `cmd` with `args` (CMIF data), `paths` in X buffers 0 and 1 and `data`. Returns the result
        /// and the CMIF output words.
        fn call(&mut self, h: u32, cmd: u32, args: &[u64], paths: &[&str], data: Data) -> (u32, Vec<u32>) {
            let m = &mut self.mmu;
            for i in 0..0x100 {
                m.write_u32(TLS + i * 4, 0).unwrap();
            }
            let (num_send, num_recv) = match data {
                Data::None => (0, 0),
                Data::Send(_) => (1, 0),
                Data::Recv(_) => (0, 1),
            };
            m.write_u32(TLS, 4 | (paths.len() as u32) << 16 | num_send << 20 | num_recv << 24).unwrap();
            m.write_u32(TLS + 4, 16).unwrap();
            let mut off = 8;
            for (i, p) in paths.iter().enumerate() {
                let addr = [PATH0, PATH1][i];
                let mut bytes = p.as_bytes().to_vec();
                bytes.resize(0x301, 0);
                m.write_bytes(addr, &bytes).unwrap();
                m.write_u32(TLS + off, i as u32 | 0x301 << 16).unwrap();
                m.write_u32(TLS + off + 4, addr as u32).unwrap();
                off += 8;
            }
            let size = match data {
                Data::Send(bytes) => {
                    m.write_bytes(BUF, bytes).unwrap();
                    bytes.len() as u64
                }
                Data::Recv(n) => {
                    m.write_bytes(BUF, &vec![0xCC; n as usize]).unwrap();
                    n
                }
                Data::None => 0,
            };
            if num_send + num_recv > 0 {
                m.write_u32(TLS + off, size as u32).unwrap();
                m.write_u32(TLS + off + 4, BUF as u32).unwrap();
                off += 12;
            }
            let cmif = (off + 15) & !15;
            m.write_u32(TLS + cmif, CMIF_IN_MAGIC).unwrap();
            m.write_u32(TLS + cmif + 8, cmd).unwrap();
            for (i, a) in args.iter().enumerate() {
                m.write_u64(TLS + cmif + 16 + i as u64 * 8, *a).unwrap();
            }
            self.ipc.handle_sync_request(h, TLS, &mut self.mmu, &mut self.nvgpu, &mut self.audio, &mut self.vfs);
            // Response: header, optional special header + handles, CMIF aligned to 16.
            let w1 = self.mmu.read_u32(TLS + 4).unwrap();
            let mut off = 8;
            if w1 >> 31 != 0 {
                let sh = self.mmu.read_u32(TLS + 8).unwrap();
                off += 4 + 4 * (((sh >> 1) & 0xF) + ((sh >> 5) & 0xF)) as u64;
            }
            let cmif = (off + 15) & !15;
            let result = self.mmu.read_u32(TLS + cmif + 8).unwrap();
            let words = (0..8).map(|i| self.mmu.read_u32(TLS + cmif + 16 + i * 4).unwrap()).collect();
            (result, words)
        }

        fn sdmc(&mut self) -> u32 {
            let fsp = self.ipc.allocate_handle(ServiceType::FspSrv);
            let (rc, out) = self.call(fsp, 18, &[], &[], Data::None);
            assert_eq!(rc, 0);
            out[0]
        }

        fn buf(&self, len: usize) -> Vec<u8> {
            let mut v = vec![0; len];
            self.mmu.read_bytes(BUF, &mut v).unwrap();
            v
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ounce_fsp_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_guest_path_stays_inside_the_mount() {
        assert_eq!(resolve_guest_path("sdmc:/", "/").unwrap(), "sdmc:/");
        assert_eq!(resolve_guest_path("sdmc:/", "/a//./b/../c").unwrap(), "sdmc:/a/c");
        assert_eq!(resolve_guest_path("sdmc:/", "sdmc:/switch/x.nro").unwrap(), "sdmc:/switch/x.nro");
        assert_eq!(resolve_guest_path("save:/", "/slot0.sav").unwrap(), "save:/slot0.sav");
        for bad in ["/..", "/../x", "/a/../../x", "../../etc/passwd"] {
            assert!(matches!(resolve_guest_path("sdmc:/", bad), Err(VfsError::Escape(_))), "{bad}");
        }
        for bad in ["save:/x", "romfs:/title.txt", "/a:b", "C:/x", "/a\\..\\..\\x"] {
            assert!(matches!(resolve_guest_path("sdmc:/", bad), Err(VfsError::InvalidPath(_))), "{bad}");
        }
    }

    /// A host folder behind `sdmc:/`: existing host files are visible, and every libnx file and directory
    /// operation lands in the folder.
    #[test]
    fn libnx_shaped_sdmc_operations_reach_the_host_folder() {
        let root = temp_root("ops");
        std::fs::write(root.join("hello.txt"), b"hi host").unwrap();
        std::fs::create_dir(root.join("data")).unwrap();
        let mut t = Fsp::new(Some(&root));
        let fs = t.sdmc();

        // GetEntryType
        assert_eq!(t.call(fs, 7, &[], &["/hello.txt"], Data::None), (0, vec![1, 0, 0, 0, 0, 0, 0, 0]));
        assert_eq!(t.call(fs, 7, &[], &["/data"], Data::None).1[0], 0);
        assert_eq!(t.call(fs, 7, &[], &["/nope"], Data::None).0, RESULT_PATH_NOT_FOUND);

        // OpenFile (read) + Read into the B buffer + GetSize
        let (rc, out) = t.call(fs, 8, &[1], &["/hello.txt"], Data::None);
        assert_eq!(rc, 0);
        let f = out[0];
        assert_eq!(t.call(f, 0, &[0, 0, 64], &[], Data::Recv(64)), (0, vec![7, 0, 0, 0, 0, 0, 0, 0]));
        assert_eq!(t.buf(7), b"hi host");
        assert_eq!(t.call(f, 4, &[], &[], Data::None).1[..2], [7, 0]);
        // Writing through a read-only handle fails.
        assert_eq!(t.call(f, 1, &[0, 0, 1], &[], Data::Send(b"x")).0, RESULT_INVALID_OPEN_MODE);
        assert_eq!(std::fs::read(root.join("hello.txt")).unwrap(), b"hi host");

        // CreateFile: new, existing, missing parent.
        assert_eq!(t.call(fs, 0, &[0, 0], &["/data/out.bin"], Data::None).0, 0);
        assert_eq!(t.call(fs, 0, &[0, 0], &["/data/out.bin"], Data::None).0, RESULT_PATH_ALREADY_EXISTS);
        assert_eq!(t.call(fs, 0, &[0, 0], &["/missing/x"], Data::None).0, RESULT_PATH_NOT_FOUND);
        assert!(root.join("data/out.bin").is_file());

        // Write (A buffer) at two offsets, SetSize.
        let (rc, out) = t.call(fs, 8, &[2 | 4], &["/data/out.bin"], Data::None);
        assert_eq!(rc, 0);
        let w = out[0];
        assert_eq!(t.call(w, 1, &[0, 0, 3], &[], Data::Send(b"abc")).0, 0);
        assert_eq!(t.call(w, 1, &[0, 5, 2], &[], Data::Send(b"XY")).0, 0);
        assert_eq!(std::fs::read(root.join("data/out.bin")).unwrap(), b"abc\0\0XY");
        assert_eq!(t.call(w, 4, &[], &[], Data::None).1[..2], [7, 0]);
        assert_eq!(t.call(w, 3, &[2], &[], Data::None).0, 0);
        assert_eq!(std::fs::read(root.join("data/out.bin")).unwrap(), b"ab");

        // OpenDirectory + Read: FsDirectoryEntry with real type and size, filtered by mode.
        let (_, out) = t.call(fs, 9, &[3], &["/"], Data::None);
        let d = out[0];
        assert_eq!(t.call(d, 1, &[], &[], Data::None).1[..2], [2, 0]);
        assert_eq!(t.call(d, 0, &[], &[], Data::Recv(4 * DIRECTORY_ENTRY_SIZE)).1[..2], [2, 0]);
        let entries = t.buf(2 * DIRECTORY_ENTRY_SIZE as usize);
        let entry = |i: usize| &entries[i * 0x310..(i + 1) * 0x310];
        assert_eq!(&entry(0)[..5], b"data\0");
        assert_eq!(entry(0)[0x304], ENTRY_TYPE_DIR);
        assert_eq!(&entry(1)[..10], b"hello.txt\0");
        assert_eq!(entry(1)[0x304], ENTRY_TYPE_FILE);
        assert_eq!(u64::from_le_bytes(entry(1)[0x308..0x310].try_into().unwrap()), 7);
        assert_eq!(t.call(d, 0, &[], &[], Data::Recv(4 * DIRECTORY_ENTRY_SIZE)).1[..2], [0, 0], "exhausted");
        let (_, out) = t.call(fs, 9, &[DIR_OPEN_READ_FILES as u64], &["/"], Data::None);
        assert_eq!(t.call(out[0], 1, &[], &[], Data::None).1[0], 1);
        assert_eq!(t.call(fs, 9, &[3], &["/nope"], Data::None).0, RESULT_PATH_NOT_FOUND);

        // RenameFile / RenameDirectory
        assert_eq!(t.call(fs, 5, &[], &["/data/out.bin", "/data/renamed.bin"], Data::None).0, 0);
        assert!(root.join("data/renamed.bin").is_file() && !root.join("data/out.bin").exists());
        assert_eq!(t.call(fs, 5, &[], &["/data", "/data2"], Data::None).0, RESULT_PATH_NOT_FOUND, "not a file");
        assert_eq!(t.call(fs, 6, &[], &["/data", "/data2"], Data::None).0, 0);
        assert!(root.join("data2/renamed.bin").is_file());
        assert_eq!(t.call(w, 4, &[], &[], Data::None).1[0], 2, "open handle follows the rename");

        // CreateDirectory, DeleteDirectory (must be empty), DeleteDirectoryRecursively, DeleteFile.
        assert_eq!(t.call(fs, 2, &[], &["/newdir"], Data::None).0, 0);
        assert!(root.join("newdir").is_dir());
        assert_eq!(t.call(fs, 2, &[], &["/newdir"], Data::None).0, RESULT_PATH_ALREADY_EXISTS);
        assert_eq!(t.call(fs, 2, &[], &["/no/such"], Data::None).0, RESULT_PATH_NOT_FOUND);
        assert_eq!(t.call(fs, 3, &[], &["/data2"], Data::None).0, RESULT_DIRECTORY_NOT_EMPTY);
        assert_eq!(t.call(fs, 3, &[], &["/newdir"], Data::None).0, 0);
        assert!(!root.join("newdir").exists());
        assert_eq!(t.call(fs, 4, &[], &["/data2"], Data::None).0, 0);
        assert!(!root.join("data2").exists());
        assert_eq!(t.call(fs, 1, &[], &["/hello.txt"], Data::None).0, 0);
        assert!(!root.join("hello.txt").exists());

        // CleanDirectoryRecursively keeps the directory; the mount root itself cannot be deleted.
        std::fs::create_dir_all(root.join("c/d")).unwrap();
        std::fs::write(root.join("c/d/f"), b"1").unwrap();
        assert_eq!(t.call(fs, 13, &[], &["/c"], Data::None).0, 0);
        assert!(root.join("c").is_dir() && std::fs::read_dir(root.join("c")).unwrap().count() == 0);
        assert_eq!(t.call(fs, 4, &[], &["/"], Data::None).0, RESULT_INVALID_PATH);
        assert!(root.join("c").is_dir());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Paths that try to leave `sdmc:/`: `..`, another mount, and (on Unix) symlinks in the host folder.
    #[test]
    fn sdmc_paths_cannot_leave_the_host_folder() {
        let base = temp_root("escape");
        let (root, outside) = (base.join("sd"), base.join("outside"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), b"secret").unwrap();
        let mut t = Fsp::new(Some(&root));
        let fs = t.sdmc();

        assert_eq!(t.call(fs, 8, &[1], &["/../outside/secret.txt"], Data::None).0, RESULT_DIRECTORY_UNOBTAINABLE);
        assert_eq!(t.call(fs, 0, &[0, 0], &["/a/../../outside/new"], Data::None).0, RESULT_DIRECTORY_UNOBTAINABLE);
        assert_eq!(t.call(fs, 9, &[3], &["/.."], Data::None).0, RESULT_DIRECTORY_UNOBTAINABLE);
        assert_eq!(t.call(fs, 8, &[1], &["romfs:/title.txt"], Data::None).0, RESULT_INVALID_PATH);
        assert_eq!(t.call(fs, 0, &[0, 0], &["save:/x"], Data::None).0, RESULT_INVALID_PATH);
        assert_eq!(t.call(fs, 5, &[], &["/x", "/../moved"], Data::None).0, RESULT_DIRECTORY_UNOBTAINABLE);

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
            std::os::unix::fs::symlink(outside.join("made-by-guest"), root.join("dangling")).unwrap();
            assert_eq!(t.call(fs, 8, &[1], &["/link/secret.txt"], Data::None).0, RESULT_DIRECTORY_UNOBTAINABLE);
            assert_eq!(t.call(fs, 0, &[0, 0], &["/link/new.txt"], Data::None).0, RESULT_DIRECTORY_UNOBTAINABLE);
            assert_eq!(t.call(fs, 0, &[0, 0], &["/dangling"], Data::None).0, RESULT_DIRECTORY_UNOBTAINABLE);
            assert_eq!(t.call(fs, 4, &[], &["/link"], Data::None).0, RESULT_DIRECTORY_UNOBTAINABLE);
            assert!(!outside.join("new.txt").exists() && !outside.join("made-by-guest").exists());
        }
        assert_eq!(std::fs::read(outside.join("secret.txt")).unwrap(), b"secret");
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Without a host folder the SD card is in memory; the same commands work and nothing reaches the host.
    /// Oversized creates are refused instead of allocated.
    #[test]
    fn in_memory_sdmc_and_size_limits() {
        let mut t = Fsp::new(None);
        let fs = t.sdmc();
        assert_eq!(t.call(fs, 2, &[], &["/switch"], Data::None).0, 0);
        assert_eq!(t.call(fs, 0, &[0, 4], &["/switch/a.cfg"], Data::None).0, 0);
        assert_eq!(t.call(fs, 7, &[], &["/switch"], Data::None).1[0], 0);
        let (_, out) = t.call(fs, 8, &[3], &["/switch/a.cfg"], Data::None);
        assert_eq!(t.call(out[0], 1, &[0, 2, 2], &[], Data::Send(b"hi")).0, 0);
        assert_eq!(t.vfs.get_file("sdmc:/switch/a.cfg"), Some(&b"\0\0hi"[..]));
        assert_eq!(t.call(out[0], 1, &[0, MAX_FILE_SIZE, 2], &[], Data::Send(b"hi")).0, RESULT_USABLE_SPACE_NOT_ENOUGH);
        assert_eq!(t.call(fs, 0, &[0, MAX_FILE_SIZE + 1], &["/big"], Data::None).0, RESULT_USABLE_SPACE_NOT_ENOUGH);
        assert_eq!(t.call(fs, 0, &[0, u64::MAX], &["/big"], Data::None).0, RESULT_USABLE_SPACE_NOT_ENOUGH);
    }
}
