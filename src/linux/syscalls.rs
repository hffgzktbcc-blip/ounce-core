//! Minimal Linux AArch64 syscall layer: `SVC #0` with the number in x8 and arguments in x0..x5.
//! Anything not handled here is reported as [`Outcome::Unimplemented`] so the runner can name it.

use super::elf::{MMAP_BASE, PAGE, STACK_SIZE, STACK_TOP, align_up};
use crate::mmu::{Memory, Permission};
use std::time::Instant;

pub const EPERM: i64 = 1;
pub const ENOENT: i64 = 2;
pub const ESRCH: i64 = 3;
pub const EBADF: i64 = 9;
pub const ENOMEM: i64 = 12;
pub const EACCES: i64 = 13;
pub const EFAULT: i64 = 14;
pub const EEXIST: i64 = 17;
#[allow(dead_code)]
pub const ENOTDIR: i64 = 20;
pub const EISDIR: i64 = 21;
pub const EINVAL: i64 = 22;
pub const EMFILE: i64 = 24;
pub const ENOTTY: i64 = 25;
pub const EFBIG: i64 = 27;
pub const EROFS: i64 = 30;
pub const ENOSPC: i64 = 28;
pub const ESPIPE: i64 = 29;
pub const ENAMETOOLONG: i64 = 36;
pub const ENOSYS: i64 = 38;

pub const SYS_FCNTL: u64 = 25;
pub const SYS_IOCTL: u64 = 29;
pub const SYS_UNLINKAT: u64 = 35;
pub const SYS_FTRUNCATE: u64 = 46;
pub const SYS_OPENAT: u64 = 56;
pub const SYS_GETDENTS64: u64 = 61;
pub const SYS_FACCESSAT: u64 = 48;
pub const SYS_CLOSE: u64 = 57;
pub const SYS_LSEEK: u64 = 62;
pub const SYS_READ: u64 = 63;
pub const SYS_WRITE: u64 = 64;
pub const SYS_READV: u64 = 65;
pub const SYS_WRITEV: u64 = 66;
pub const SYS_PREAD64: u64 = 67;
pub const SYS_PWRITE64: u64 = 68;
pub const SYS_PPOLL: u64 = 73;
pub const SYS_READLINKAT: u64 = 78;
pub const SYS_NEWFSTATAT: u64 = 79;
pub const SYS_FSTAT: u64 = 80;
pub const SYS_EXIT: u64 = 93;
pub const SYS_EXIT_GROUP: u64 = 94;
pub const SYS_SET_TID_ADDRESS: u64 = 96;
pub const SYS_CLOCK_GETTIME: u64 = 113;
pub const SYS_SCHED_SETAFFINITY: u64 = 122;
pub const SYS_SCHED_GETAFFINITY: u64 = 123;
pub const SYS_SCHED_YIELD: u64 = 124;
pub const SYS_RT_SIGACTION: u64 = 134;
pub const SYS_RT_SIGPROCMASK: u64 = 135;
pub const SYS_UNAME: u64 = 160;
pub const SYS_GETCPU: u64 = 168;
pub const SYS_GETTIMEOFDAY: u64 = 169;
pub const SYS_NANOSLEEP: u64 = 101;
pub const SYS_CLOCK_NANOSLEEP: u64 = 115;
pub const SYS_GETPID: u64 = 172;
pub const SYS_GETPPID: u64 = 173;
pub const SYS_GETUID: u64 = 174;
pub const SYS_GETEUID: u64 = 175;
pub const SYS_GETGID: u64 = 176;
pub const SYS_GETEGID: u64 = 177;
pub const SYS_GETTID: u64 = 178;
pub const SYS_BRK: u64 = 214;
pub const SYS_MUNMAP: u64 = 215;
pub const SYS_MREMAP: u64 = 216;
pub const SYS_CLONE: u64 = 220;
pub const SYS_MMAP: u64 = 222;
pub const SYS_MPROTECT: u64 = 226;
pub const SYS_CLONE3: u64 = 435;
pub const CLONE_THREAD: u64 = 0x0001_0000;

const O_ACCMODE: u32 = 0o3;
const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;
const O_RDWR: u32 = 2;
const O_CREAT: u32 = 0o100;
const O_EXCL: u32 = 0o200;
const O_TRUNC: u32 = 0o1000;
const O_APPEND: u32 = 0o2000;
const O_DIRECTORY: u32 = 0o200000;
const O_CLOEXEC: u32 = 0o2000000;

pub const MAX_OPEN_FDS: usize = 64;
pub const MAX_SCRATCH_FILES: usize = 64;
pub const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_TOTAL_SCRATCH_BYTES: usize = 64 * 1024 * 1024;

#[allow(dead_code)]
const AT_FDCWD: i64 = -100;
const AT_EMPTY_PATH: u64 = 0x1000;

#[derive(Debug, Clone)]
enum InodeKind {
    Regular(Vec<u8>),
    DevNull,
    ProcAuxv,
    Directory,
    /// A file under `--root`, read in full when opened (read-only; freed with its last descriptor)
    HostFile(Vec<u8>),
    /// A directory under `--root`: its entries (name, `d_type`), `.` and `..` first, then sorted
    HostDir(Vec<(String, u8)>),
}

/// The largest host file `--root` reads into the guest.
const MAX_HOST_FILE: u64 = 256 << 20;

#[derive(Debug, Clone)]
struct Inode {
    kind: InodeKind,
    #[allow(dead_code)]
    ino: u64,
    mode: u32,
    nlink: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Stdin,
    Stdout,
    Stderr,
    Inode(u64),
}

#[derive(Debug, Clone)]
struct OpenFile {
    kind: FileKind,
    offset: u64,
    flags: u32,
    cloexec: bool,
}

const MREMAP_MAYMOVE: u64 = 1;
const MAP_FIXED: u64 = 0x10;
const MAP_ANONYMOUS: u64 = 0x20;
const PROT_READ: u64 = 1;
const PROT_WRITE: u64 = 2;
const PROT_EXEC: u64 = 4;
const TCGETS: u64 = 0x5401;
const MAX_BRK_GROWTH: u64 = 512 << 20;
const MAX_MMAP_CALL: u64 = 512 << 20;
const MAX_MMAP_TOTAL: u64 = 1 << 30;
const MAX_IO_CALL: u64 = 64 << 20;
const MAX_IOV: u64 = 1024;
const MAX_GETRANDOM: u64 = 256;
const RNG_SEED: u64 = 0x6f75_6e63_655f_726e;
pub const PID: u64 = 1;
pub const UID: u64 = 1000;
pub const GID: u64 = 1000;

/// Where guest `write`s to fd 1 and 2 go.
pub trait Output {
    fn write(&mut self, fd: u64, data: &[u8]);
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Value for x0 (negative errno on failure, as the kernel returns it).
    Return(i64),
    Exit(i32),
    /// Syscall (or syscall mode) this layer does not implement; the string says which.
    Unimplemented(String),
}

pub struct Process {
    pub brk_start: u64,
    pub brk_cur: u64,
    brk_mapped_end: u64,
    mmap_next: u64,
    mmap_total: u64,
    /// Virtual anonymous mappings, including unbacked PROT_NONE reservations.
    mappings: Vec<(u64, u64)>,
    pub(super) signal_mask: u64,
    /// Signals raised with `tgkill` while blocked, waiting for `rt_sigprocmask` to unblock them (bit `n - 1`).
    pub(super) signal_pending: u64,
    pub(super) alt_stack: [u64; 3],
    pub(super) sigactions: [[u8; 32]; 65],
    started: Instant,
    deterministic_ticks: Option<u64>,
    retired_instructions: u64,
    pub auxv_data: Vec<u8>,
    pub stdin_data: Vec<u8>,
    files: std::collections::BTreeMap<u32, OpenFile>,
    inodes: std::collections::BTreeMap<u64, Inode>,
    dentries: std::collections::BTreeMap<String, u64>,
    next_ino: u64,
    rng: u64,
    warned_clone_thread: bool,
    /// `--root=DIR` (canonical): guest paths outside the scratch area name files under it, read-only
    host_root: Option<std::path::PathBuf>,
}

impl Process {
    pub fn new(brk_start: u64) -> Self {
        let mut files = std::collections::BTreeMap::new();
        files.insert(
            0,
            OpenFile {
                kind: FileKind::Stdin,
                offset: 0,
                flags: O_RDONLY,
                cloexec: false,
            },
        );
        files.insert(
            1,
            OpenFile {
                kind: FileKind::Stdout,
                offset: 0,
                flags: O_WRONLY,
                cloexec: false,
            },
        );
        files.insert(
            2,
            OpenFile {
                kind: FileKind::Stderr,
                offset: 0,
                flags: O_WRONLY,
                cloexec: false,
            },
        );

        let mut inodes = std::collections::BTreeMap::new();
        let mut dentries = std::collections::BTreeMap::new();

        inodes.insert(
            1,
            Inode {
                kind: InodeKind::DevNull,
                ino: 1,
                mode: 0o20666,
                nlink: 1,
            },
        );
        dentries.insert("/dev/null".to_string(), 1);

        inodes.insert(
            2,
            Inode {
                kind: InodeKind::ProcAuxv,
                ino: 2,
                mode: 0o100444,
                nlink: 1,
            },
        );
        dentries.insert("/proc/self/auxv".to_string(), 2);

        inodes.insert(
            3,
            Inode {
                kind: InodeKind::Directory,
                ino: 3,
                mode: 0o40777,
                nlink: 2,
            },
        );
        dentries.insert("/tmp".to_string(), 3);
        dentries.insert("/var/tmp".to_string(), 3);

        Self {
            brk_start,
            brk_cur: brk_start,
            brk_mapped_end: brk_start,
            mmap_next: MMAP_BASE,
            mmap_total: 0,
            mappings: Vec::new(),
            sigactions: [[0; 32]; 65],
            signal_mask: 0,
            signal_pending: 0,
            alt_stack: [0, 2, 0],
            started: Instant::now(),
            deterministic_ticks: None,
            retired_instructions: 0,
            auxv_data: Vec::new(),
            stdin_data: Vec::new(),
            files,
            inodes,
            dentries,
            next_ino: 10,
            rng: RNG_SEED,
            warned_clone_thread: false,
            host_root: None,
        }
    }

    pub fn set_auxv(&mut self, data: Vec<u8>) {
        self.auxv_data = data;
    }

    pub fn set_stdin(&mut self, data: Vec<u8>) {
        self.stdin_data = data;
    }

    pub fn set_retired_instructions(&mut self, retired: u64) {
        self.retired_instructions = retired;
    }

    pub fn is_fd_open(&self, fd: u64) -> bool {
        u32::try_from(fd)
            .map(|fd32| self.files.contains_key(&fd32))
            .unwrap_or(false)
    }

    pub fn is_fd_readable(&self, fd: u64) -> bool {
        let Ok(fd32) = u32::try_from(fd) else { return false };
        let Some(f) = self.files.get(&fd32) else { return false };
        match f.kind {
            FileKind::Stdin => true,
            FileKind::Stdout | FileKind::Stderr => false,
            FileKind::Inode(ino) => {
                if let Some(inode) = self.inodes.get(&ino) {
                    match &inode.kind {
                        InodeKind::DevNull | InodeKind::ProcAuxv => true,
                        InodeKind::Regular(_) => f.flags & O_ACCMODE != O_WRONLY,
                        InodeKind::Directory | InodeKind::HostDir(_) => false,
                        InodeKind::HostFile(_) => true,
                    }
                } else {
                    false
                }
            }
        }
    }

    pub fn is_fd_writable(&self, fd: u64) -> bool {
        let Ok(fd32) = u32::try_from(fd) else { return false };
        let Some(f) = self.files.get(&fd32) else { return false };
        match f.kind {
            FileKind::Stdin => false,
            FileKind::Stdout | FileKind::Stderr => true,
            FileKind::Inode(ino) => {
                if let Some(inode) = self.inodes.get(&ino) {
                    match &inode.kind {
                        InodeKind::DevNull => true,
                        InodeKind::ProcAuxv | InodeKind::Directory | InodeKind::HostFile(_) | InodeKind::HostDir(_) => {
                            false
                        }
                        InodeKind::Regular(_) => f.flags & O_ACCMODE != O_RDONLY,
                    }
                } else {
                    false
                }
            }
        }
    }

    fn alloc_fd(&mut self, file: OpenFile, min_fd: u32) -> Result<u32, i64> {
        if self.files.len() >= MAX_OPEN_FDS {
            return Err(-EMFILE);
        }
        let mut fd = min_fd;
        while fd < 1024 {
            if let std::collections::btree_map::Entry::Vacant(e) = self.files.entry(fd) {
                e.insert(file);
                return Ok(fd);
            }
            fd += 1;
        }
        Err(-EMFILE)
    }

    fn close_fd(&mut self, fd: u32) -> i64 {
        let Some(file) = self.files.remove(&fd) else {
            return -EBADF;
        };
        if let FileKind::Inode(ino) = file.kind {
            self.cleanup_inode(ino);
        }
        0
    }

    fn cleanup_inode(&mut self, ino: u64) {
        let still_open = self.files.values().any(|f| f.kind == FileKind::Inode(ino));
        if !still_open
            && let Some(inode) = self.inodes.get(&ino)
            && inode.nlink == 0
        {
            self.inodes.remove(&ino);
        }
    }

    fn total_scratch_bytes(&self) -> usize {
        self.inodes
            .values()
            .map(|i| {
                if let InodeKind::Regular(data) = &i.kind {
                    data.len()
                } else {
                    0
                }
            })
            .sum()
    }

    fn regular_file_count(&self) -> usize {
        self.inodes
            .values()
            .filter(|i| matches!(i.kind, InodeKind::Regular(_)))
            .count()
    }

    /// `--root=DIR`: expose the host directory `dir` read-only as the guest's file system outside the scratch
    /// area (`/tmp`, `/var/tmp`, `/dev/null`, `/proc/self/auxv`). Nothing outside `dir` can be reached.
    pub fn set_host_root(&mut self, dir: &std::path::Path) -> std::io::Result<()> {
        let canonical = dir.canonicalize()?;
        if !canonical.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                "--root must be a directory",
            ));
        }
        self.host_root = Some(canonical);
        Ok(())
    }

    /// The host path of guest path `norm` (normalized, absolute) under `--root`, if it exists there. Symlinks are
    /// resolved first and anything that resolves outside the root is treated as missing.
    fn host_path(&self, norm: &str) -> Option<std::path::PathBuf> {
        let root = self.host_root.as_ref()?;
        let real = root.join(norm.trim_start_matches('/')).canonicalize().ok()?;
        real.starts_with(root).then_some(real)
    }

    /// `openat` of a path under `--root`: read-only; a directory gets its listing, a file its contents.
    fn open_host(&mut self, norm: &str, flags: u32) -> i64 {
        let writes = flags & O_ACCMODE != O_RDONLY || flags & O_TRUNC != 0;
        let Some(real) = self.host_path(norm) else {
            return if flags & O_CREAT != 0 { -EROFS } else { -ENOENT };
        };
        let Ok(meta) = std::fs::metadata(&real) else {
            return -ENOENT;
        };
        let (kind, mode) = if meta.is_dir() {
            if writes {
                return -EISDIR;
            }
            let mut entries = vec![(".".to_string(), 4u8), ("..".to_string(), 4u8)];
            let mut children: Vec<(String, u8)> = std::fs::read_dir(&real)
                .map(|rd| {
                    rd.filter_map(Result::ok)
                        .map(|e| {
                            let t = e.file_type().ok();
                            let d_type = match t {
                                Some(t) if t.is_dir() => 4,
                                Some(t) if t.is_symlink() => 10,
                                Some(t) if t.is_file() => 8,
                                _ => 0,
                            };
                            (e.file_name().to_string_lossy().into_owned(), d_type)
                        })
                        .collect()
                })
                .unwrap_or_default();
            children.sort();
            entries.extend(children);
            (InodeKind::HostDir(entries), 0o40555)
        } else {
            if writes {
                return -EROFS;
            }
            if flags & O_DIRECTORY != 0 {
                return -ENOTDIR;
            }
            if meta.len() > MAX_HOST_FILE {
                return -EFBIG;
            }
            let Ok(data) = std::fs::read(&real) else { return -EACCES };
            (InodeKind::HostFile(data), 0o100444)
        };
        let ino = self.next_ino;
        self.next_ino += 1;
        // nlink 0: no dentry in the scratch area, so the inode goes with its last descriptor
        self.inodes.insert(
            ino,
            Inode {
                kind,
                ino,
                mode,
                nlink: 0,
            },
        );
        let file = OpenFile {
            kind: FileKind::Inode(ino),
            offset: 0,
            flags,
            cloexec: flags & O_CLOEXEC != 0,
        };
        match self.alloc_fd(file, 0) {
            Ok(fd) => fd as i64,
            Err(e) => {
                self.inodes.remove(&ino);
                e
            }
        }
    }

    /// `getdents64`: `linux_dirent64` records (`d_ino`, `d_off`, `d_reclen`, `d_type`, name) for a `--root`
    /// directory, continuing from the descriptor's offset (an entry index).
    fn sys_getdents64(&mut self, fd: u64, dirp: u64, count: u64, mmu: &mut Memory) -> Outcome {
        let Ok(fd32) = u32::try_from(fd) else {
            return Outcome::Return(-EBADF);
        };
        let Some(file) = self.files.get_mut(&fd32) else {
            return Outcome::Return(-EBADF);
        };
        let FileKind::Inode(ino) = file.kind else {
            return Outcome::Return(-ENOTDIR);
        };
        let entries = match self.inodes.get(&ino).map(|i| &i.kind) {
            Some(InodeKind::HostDir(e)) => e,
            Some(InodeKind::Directory) => {
                return Outcome::Unimplemented("getdents64 on a scratch-area directory is not implemented".into());
            }
            _ => return Outcome::Return(-ENOTDIR),
        };
        let mut buf = Vec::new();
        let mut next = file.offset as usize;
        while let Some((name, d_type)) = entries.get(next) {
            let reclen = (19 + name.len() + 1).next_multiple_of(8);
            if buf.len() + reclen > count as usize {
                break;
            }
            let mut rec = vec![0u8; reclen];
            rec[0..8].copy_from_slice(&(ino * 4096 + next as u64 + 1).to_le_bytes());
            rec[8..16].copy_from_slice(&(next as i64 + 1).to_le_bytes());
            rec[16..18].copy_from_slice(&(reclen as u16).to_le_bytes());
            rec[18] = *d_type;
            rec[19..19 + name.len()].copy_from_slice(name.as_bytes());
            buf.extend_from_slice(&rec);
            next += 1;
        }
        if buf.is_empty() && next < entries.len() {
            return Outcome::Return(-EINVAL); // the next entry does not fit
        }
        if mmu.write_bytes(dirp, &buf).is_err() {
            return Outcome::Return(-EFAULT);
        }
        file.offset = next as u64;
        Outcome::Return(buf.len() as i64)
    }

    /// `faccessat`: existence (`F_OK`) and read permission (`R_OK`) on the scratch area and `--root`; `W_OK` under
    /// `--root` is EROFS and `X_OK` there is EACCES (guests cannot exec).
    fn sys_faccessat(&self, path_ptr: u64, mode: u64, mmu: &mut Memory) -> i64 {
        let path_str = match read_path(path_ptr, mmu) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let norm = normalize_path(&path_str);
        if is_allowed_path(&norm) {
            let known = matches!(norm.as_str(), "/dev/null" | "/proc/self/auxv" | "/tmp" | "/var/tmp")
                || self.dentries.contains_key(&norm);
            return if known { 0 } else { -ENOENT };
        }
        let Some(real) = self.host_path(&norm) else {
            return -ENOENT;
        };
        if mode & 2 != 0 {
            return -EROFS;
        }
        if mode & 1 != 0 && !real.is_dir() {
            return -EACCES;
        }
        0
    }

    /// Fixed virtual time for reproducible single-process experiments. Each successful
    /// clock read advances the shared clock by 1 ms; rejected reads do not advance it.
    pub fn new_deterministic(brk_start: u64) -> Self {
        Self {
            deterministic_ticks: Some(0),
            ..Self::new(brk_start)
        }
    }

    pub fn dispatch(&mut self, nr: u64, a: [u64; 6], mmu: &mut Memory, out: &mut dyn Output) -> Outcome {
        let r = match nr {
            SYS_WRITE => self.sys_write(a, mmu, out),
            SYS_WRITEV => self.sys_writev(a, mmu, out),
            SYS_EXIT | SYS_EXIT_GROUP => return Outcome::Exit(a[0] as i32),
            SYS_BRK => self.sys_brk(a[0], mmu),
            SYS_MMAP => match self.sys_mmap(a, mmu) {
                Ok(v) => v,
                Err(why) => return Outcome::Unimplemented(why),
            },
            SYS_MPROTECT => self.sys_mprotect(a, mmu),
            SYS_MUNMAP => self.sys_munmap(a[0], a[1], mmu),
            SYS_MREMAP => self.sys_mremap(a, mmu),
            SYS_IOCTL => {
                // Terminal queries are answered "not a tty" so stdio libraries choose full buffering.
                if a[0] > 2 {
                    -EBADF
                } else {
                    let _ = TCGETS;
                    -ENOTTY
                }
            }
            SYS_SET_TID_ADDRESS => PID as i64,
            SYS_RT_SIGACTION => {
                if a[0] == 0 || a[0] > 64 || matches!(a[0], 9 | 19) || a[3] != 8 {
                    -EINVAL
                } else {
                    let mut action = [0; 32];
                    // Read the new action before writing the old one back, as the kernel does.
                    if (a[1] != 0 && mmu.read_bytes(a[1], &mut action).is_err())
                        || (a[2] != 0 && mmu.write_bytes(a[2], &self.sigactions[a[0] as usize]).is_err())
                    {
                        -EFAULT
                    } else {
                        if a[1] != 0 {
                            self.sigactions[a[0] as usize] = action;
                        }
                        0
                    }
                }
            }
            SYS_RT_SIGPROCMASK => {
                let old = self.signal_mask;
                if a[3] != 8 || (a[1] != 0 && a[0] > 2) {
                    -EINVAL
                } else if a[2] != 0 && mmu.write_u64(a[2], old).is_err() {
                    -EFAULT
                } else if a[1] == 0 {
                    0
                } else if let Ok(mask) = mmu.read_u64(a[1]) {
                    self.signal_mask = match a[0] {
                        0 => old | mask,
                        1 => old & !mask,
                        _ => mask,
                    } & !(1 << 8 | 1 << 18);
                    if let Some(outcome) = self.deliver_pending() {
                        return outcome;
                    }
                    0
                } else {
                    -EFAULT
                }
            }
            130 => return self.sys_tkill(a),
            131 => return self.sys_tgkill(a),
            132 => self.sys_sigaltstack(a, mmu),
            SYS_CLOSE => self.close_fd(a[0] as u32),
            SYS_FCNTL => self.sys_fcntl(a[0], a[1], a[2]),
            SYS_UNLINKAT => self.sys_unlinkat(a[0] as i64, a[1], a[2] as i32, mmu),
            SYS_FTRUNCATE => self.sys_ftruncate(a[0], a[1]),
            SYS_OPENAT => self.sys_openat(a, mmu),
            SYS_LSEEK => self.sys_lseek(a[0], a[1] as i64, a[2]),
            SYS_READ => self.sys_read(a[0], a[1], a[2], mmu),
            SYS_READV => self.sys_readv(a[0], a[1], a[2], mmu),
            SYS_PREAD64 => self.sys_pread64(a[0], a[1], a[2], a[3], mmu),
            SYS_PWRITE64 => self.sys_pwrite64(a[0], a[1], a[2], a[3], mmu),
            SYS_PPOLL => self.sys_ppoll(a, mmu),
            SYS_READLINKAT => -ENOENT,
            SYS_NEWFSTATAT => self.sys_newfstatat(a[0] as i64, a[1], a[2], a[3] as i32, mmu),
            SYS_FSTAT => self.sys_fstat(a[0], a[1], mmu),
            SYS_GETDENTS64 => return self.sys_getdents64(a[0], a[1], a[2], mmu),
            SYS_FACCESSAT | 439 => self.sys_faccessat(a[1], a[2], mmu),
            SYS_SCHED_YIELD => 0,
            SYS_GETTIMEOFDAY => self.sys_gettimeofday(a[0], a[1], mmu),
            SYS_NANOSLEEP => self.sys_nanosleep(None, 0, a[0], mmu),
            SYS_CLOCK_NANOSLEEP => self.sys_nanosleep(Some(a[0]), a[1], a[2], mmu),
            SYS_GETPID | SYS_GETTID => PID as i64,
            SYS_GETPPID => 0,
            SYS_GETUID | SYS_GETEUID => UID as i64,
            SYS_GETGID | SYS_GETEGID => GID as i64,
            278 => self.sys_getrandom(a[0], a[1], a[2], mmu),
            261 => {
                if a[0] != 0 && a[0] != PID {
                    -ESRCH
                } else if a[1] != 3 || a[2] != 0 {
                    -EINVAL
                } else if a[3] == 0 {
                    0
                } else if mmu.write_u64(a[3], STACK_SIZE).is_err()
                    || mmu.write_u64(a[3].saturating_add(8), STACK_SIZE).is_err()
                {
                    -EFAULT
                } else {
                    0
                }
            }
            163 => {
                if a[0] != 3 {
                    -EINVAL
                } else if mmu.write_u64(a[1], STACK_SIZE).is_err()
                    || mmu.write_u64(a[1].saturating_add(8), STACK_SIZE).is_err()
                {
                    -EFAULT
                } else {
                    0
                }
            }
            SYS_CLOCK_GETTIME => self.sys_clock_gettime(a[0], a[1], mmu),
            SYS_UNAME => sys_uname(a[0], mmu),
            SYS_SCHED_SETAFFINITY => sys_sched_setaffinity(a[0], a[1], a[2], mmu),
            SYS_SCHED_GETAFFINITY => sys_sched_getaffinity(a[0], a[1], a[2], mmu),
            SYS_GETCPU => sys_getcpu(a[0], a[1], a[2], mmu),
            SYS_CLONE | SYS_CLONE3 => {
                let flags = if nr == SYS_CLONE3 {
                    // clone3 takes (struct clone_args *uargs, size_t size); flags is the first u64 field.
                    mmu.read_u64(a[0]).unwrap_or(0)
                } else {
                    a[0]
                };
                // threads come through `clone` (the runner's `threads` module takes those before dispatch);
                // `clone3` is ENOSYS so glibc falls back to `clone`. New processes (fork) are not supported.
                if (flags & CLONE_THREAD) == 0 && !self.warned_clone_thread {
                    self.warned_clone_thread = true;
                    let msg = format!(
                        "[run-elf] guest tried to start a process (clone flags {:#x}); fork is not supported, clone returned ENOSYS\n",
                        flags
                    );
                    out.write(2, msg.as_bytes());
                }
                -ENOSYS
            }
            _ => return Outcome::Unimplemented(describe_unimplemented(nr, &a)),
        };
        Outcome::Return(r)
    }

    fn sys_openat(&mut self, a: [u64; 6], mmu: &mut Memory) -> i64 {
        let (_dirfd, path_ptr, flags, mode) = (a[0] as i64, a[1], a[2] as u32, a[3] as u32);
        let path_str = match read_path(path_ptr, mmu) {
            Ok(p) => p,
            Err(_) => return -ENOENT,
        };
        if path_str.is_empty() {
            return -ENOENT;
        }
        let norm = normalize_path(&path_str);
        if !is_allowed_path(&norm) {
            return if self.host_root.is_some() {
                self.open_host(&norm, flags)
            } else {
                -ENOENT
            };
        }
        if norm == "/dev/null" {
            let cloexec = flags & O_CLOEXEC != 0;
            return match self.alloc_fd(
                OpenFile {
                    kind: FileKind::Inode(1),
                    offset: 0,
                    flags,
                    cloexec,
                },
                0,
            ) {
                Ok(fd) => fd as i64,
                Err(e) => e,
            };
        }
        if norm == "/proc/self/auxv" {
            if flags & O_ACCMODE != O_RDONLY {
                return -EACCES;
            }
            let cloexec = flags & O_CLOEXEC != 0;
            return match self.alloc_fd(
                OpenFile {
                    kind: FileKind::Inode(2),
                    offset: 0,
                    flags,
                    cloexec,
                },
                0,
            ) {
                Ok(fd) => fd as i64,
                Err(e) => e,
            };
        }
        if norm == "/tmp" || norm == "/var/tmp" {
            if flags & O_ACCMODE != O_RDONLY && flags & O_DIRECTORY == 0 {
                return -EISDIR;
            }
            let cloexec = flags & O_CLOEXEC != 0;
            return match self.alloc_fd(
                OpenFile {
                    kind: FileKind::Inode(3),
                    offset: 0,
                    flags,
                    cloexec,
                },
                0,
            ) {
                Ok(fd) => fd as i64,
                Err(e) => e,
            };
        }
        if let Some(&ino) = self.dentries.get(&norm) {
            if flags & O_CREAT != 0 && flags & O_EXCL != 0 {
                return -EEXIST;
            }
            if let Some(inode) = self.inodes.get_mut(&ino)
                && let InodeKind::Regular(buf) = &mut inode.kind
                && flags & O_TRUNC != 0
                && (flags & O_ACCMODE == O_WRONLY || flags & O_ACCMODE == O_RDWR)
            {
                buf.clear();
            }
            let cloexec = flags & O_CLOEXEC != 0;
            match self.alloc_fd(
                OpenFile {
                    kind: FileKind::Inode(ino),
                    offset: 0,
                    flags,
                    cloexec,
                },
                0,
            ) {
                Ok(fd) => fd as i64,
                Err(e) => e,
            }
        } else {
            if flags & O_CREAT == 0 {
                return -ENOENT;
            }
            if self.regular_file_count() >= MAX_SCRATCH_FILES {
                return -ENOSPC;
            }
            let ino = self.next_ino;
            self.next_ino += 1;
            self.inodes.insert(
                ino,
                Inode {
                    kind: InodeKind::Regular(Vec::new()),
                    ino,
                    mode: 0o100000 | (mode & 0o777),
                    nlink: 1,
                },
            );
            self.dentries.insert(norm, ino);
            let cloexec = flags & O_CLOEXEC != 0;
            match self.alloc_fd(
                OpenFile {
                    kind: FileKind::Inode(ino),
                    offset: 0,
                    flags,
                    cloexec,
                },
                0,
            ) {
                Ok(fd) => fd as i64,
                Err(e) => e,
            }
        }
    }

    fn sys_unlinkat(&mut self, _dirfd: i64, path_ptr: u64, _flags: i32, mmu: &Memory) -> i64 {
        let path_str = match read_path(path_ptr, mmu) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let norm = normalize_path(&path_str);
        if !is_allowed_path(&norm) {
            return -ENOENT;
        }
        if norm == "/dev/null" || norm == "/proc/self/auxv" || norm == "/tmp" || norm == "/var/tmp" {
            return -EPERM;
        }
        if let Some(ino) = self.dentries.remove(&norm) {
            if let Some(inode) = self.inodes.get_mut(&ino) {
                inode.nlink = inode.nlink.saturating_sub(1);
            }
            self.cleanup_inode(ino);
            0
        } else {
            -ENOENT
        }
    }

    fn sys_read(&mut self, fd: u64, buf: u64, count: u64, mmu: &mut Memory) -> i64 {
        let Ok(fd32) = u32::try_from(fd) else { return -EBADF };
        let Some(file) = self.files.get_mut(&fd32) else {
            return -EBADF;
        };
        if file.flags & O_ACCMODE == O_WRONLY {
            return -EBADF;
        }
        let count = count.min(MAX_IO_CALL) as usize;
        match file.kind {
            FileKind::Stdin => {
                let off = file.offset as usize;
                if off >= self.stdin_data.len() {
                    return 0;
                }
                let avail = (self.stdin_data.len() - off).min(count);
                if mmu.write_bytes(buf, &self.stdin_data[off..off + avail]).is_err() {
                    return -EFAULT;
                }
                file.offset += avail as u64;
                avail as i64
            }
            FileKind::Stdout | FileKind::Stderr => -EBADF,
            FileKind::Inode(ino) => {
                let Some(inode) = self.inodes.get(&ino) else {
                    return -EBADF;
                };
                match &inode.kind {
                    InodeKind::DevNull => 0,
                    InodeKind::ProcAuxv => {
                        let off = file.offset as usize;
                        if off >= self.auxv_data.len() {
                            return 0;
                        }
                        let avail = (self.auxv_data.len() - off).min(count);
                        if mmu.write_bytes(buf, &self.auxv_data[off..off + avail]).is_err() {
                            return -EFAULT;
                        }
                        file.offset += avail as u64;
                        avail as i64
                    }
                    InodeKind::Regular(data) | InodeKind::HostFile(data) => {
                        let off = file.offset as usize;
                        if off >= data.len() {
                            return 0;
                        }
                        let avail = (data.len() - off).min(count);
                        if mmu.write_bytes(buf, &data[off..off + avail]).is_err() {
                            return -EFAULT;
                        }
                        file.offset += avail as u64;
                        avail as i64
                    }
                    InodeKind::Directory | InodeKind::HostDir(_) => -EISDIR,
                }
            }
        }
    }

    fn sys_pread64(&self, fd: u64, buf: u64, count: u64, offset: u64, mmu: &mut Memory) -> i64 {
        let Ok(fd32) = u32::try_from(fd) else { return -EBADF };
        let Some(file) = self.files.get(&fd32) else {
            return -EBADF;
        };
        if file.flags & O_ACCMODE == O_WRONLY {
            return -EBADF;
        }
        if offset > i64::MAX as u64 {
            return -EINVAL;
        }
        let count = count.min(MAX_IO_CALL) as usize;
        let off = offset as usize;
        match file.kind {
            FileKind::Stdin => {
                if off >= self.stdin_data.len() {
                    return 0;
                }
                let avail = (self.stdin_data.len() - off).min(count);
                if mmu.write_bytes(buf, &self.stdin_data[off..off + avail]).is_err() {
                    return -EFAULT;
                }
                avail as i64
            }
            FileKind::Stdout | FileKind::Stderr => -ESPIPE,
            FileKind::Inode(ino) => {
                let Some(inode) = self.inodes.get(&ino) else {
                    return -EBADF;
                };
                match &inode.kind {
                    InodeKind::DevNull => 0,
                    InodeKind::ProcAuxv => {
                        if off >= self.auxv_data.len() {
                            return 0;
                        }
                        let avail = (self.auxv_data.len() - off).min(count);
                        if mmu.write_bytes(buf, &self.auxv_data[off..off + avail]).is_err() {
                            return -EFAULT;
                        }
                        avail as i64
                    }
                    InodeKind::Regular(data) | InodeKind::HostFile(data) => {
                        if off >= data.len() {
                            return 0;
                        }
                        let avail = (data.len() - off).min(count);
                        if mmu.write_bytes(buf, &data[off..off + avail]).is_err() {
                            return -EFAULT;
                        }
                        avail as i64
                    }
                    InodeKind::Directory | InodeKind::HostDir(_) => -EISDIR,
                }
            }
        }
    }

    fn sys_readv(&mut self, fd: u64, iov: u64, cnt: u64, mmu: &mut Memory) -> i64 {
        if cnt > MAX_IOV {
            return -EINVAL;
        }
        let mut total = 0i64;
        for i in 0..cnt {
            let (Ok(base), Ok(len)) = (mmu.read_u64(iov + i * 16), mmu.read_u64(iov + i * 16 + 8)) else {
                return -EFAULT;
            };
            if len == 0 {
                continue;
            }
            let n = self.sys_read(fd, base, len, mmu);
            if n < 0 {
                return if total > 0 { total } else { n };
            }
            total += n;
            if (n as u64) < len {
                break;
            }
        }
        total
    }

    fn sys_write(&mut self, a: [u64; 6], mmu: &mut Memory, out: &mut dyn Output) -> i64 {
        let (fd, buf, count) = (a[0], a[1], a[2].min(MAX_IO_CALL));
        let Ok(fd32) = u32::try_from(fd) else { return -EBADF };
        let Some(file) = self.files.get(&fd32) else {
            return -EBADF;
        };
        if file.flags & O_ACCMODE == O_RDONLY {
            return -EBADF;
        }
        let mut data = vec![0u8; count as usize];
        if mmu.read_bytes(buf, &mut data).is_err() {
            return -EFAULT;
        }
        match file.kind {
            FileKind::Stdin => -EBADF,
            FileKind::Stdout => {
                out.write(1, &data);
                count as i64
            }
            FileKind::Stderr => {
                out.write(2, &data);
                count as i64
            }
            FileKind::Inode(ino) => {
                let is_append = file.flags & O_APPEND != 0;
                let cur_offset = file.offset;
                let total_scratch = self.total_scratch_bytes();
                let Some(inode) = self.inodes.get_mut(&ino) else {
                    return -EBADF;
                };
                match &mut inode.kind {
                    InodeKind::DevNull => count as i64,
                    InodeKind::ProcAuxv => -EACCES,
                    InodeKind::Directory | InodeKind::HostDir(_) => -EISDIR,
                    InodeKind::HostFile(_) => -EBADF,
                    InodeKind::Regular(content) => {
                        let write_at = if is_append { content.len() as u64 } else { cur_offset };
                        let Some(new_end) = write_at.checked_add(count) else {
                            return -EFBIG;
                        };
                        if new_end as usize > MAX_FILE_BYTES {
                            return -EFBIG;
                        }
                        let cur_len = content.len();
                        let growth = (new_end as usize).saturating_sub(cur_len);
                        if total_scratch + growth > MAX_TOTAL_SCRATCH_BYTES {
                            return -ENOSPC;
                        }
                        if (write_at as usize) > cur_len {
                            content.resize(write_at as usize, 0);
                        }
                        if (new_end as usize) > content.len() {
                            content.resize(new_end as usize, 0);
                        }
                        content[write_at as usize..new_end as usize].copy_from_slice(&data);
                        let file_mut = self.files.get_mut(&fd32).unwrap();
                        file_mut.offset = new_end;
                        count as i64
                    }
                }
            }
        }
    }

    fn sys_pwrite64(&mut self, fd: u64, buf: u64, count: u64, offset: u64, mmu: &mut Memory) -> i64 {
        let count = count.min(MAX_IO_CALL);
        let Ok(fd32) = u32::try_from(fd) else { return -EBADF };
        let Some(file) = self.files.get(&fd32) else {
            return -EBADF;
        };
        if file.flags & O_ACCMODE == O_RDONLY {
            return -EBADF;
        }
        if offset > i64::MAX as u64 {
            return -EINVAL;
        }
        let mut data = vec![0u8; count as usize];
        if mmu.read_bytes(buf, &mut data).is_err() {
            return -EFAULT;
        }
        match file.kind {
            FileKind::Stdin => -EBADF,
            FileKind::Stdout | FileKind::Stderr => -ESPIPE,
            FileKind::Inode(ino) => {
                let total_scratch = self.total_scratch_bytes();
                let Some(inode) = self.inodes.get_mut(&ino) else {
                    return -EBADF;
                };
                match &mut inode.kind {
                    InodeKind::DevNull => count as i64,
                    InodeKind::ProcAuxv => -EACCES,
                    InodeKind::Directory | InodeKind::HostDir(_) => -EISDIR,
                    InodeKind::HostFile(_) => -EBADF,
                    InodeKind::Regular(content) => {
                        let write_at = offset;
                        let Some(new_end) = write_at.checked_add(count) else {
                            return -EFBIG;
                        };
                        if new_end as usize > MAX_FILE_BYTES {
                            return -EFBIG;
                        }
                        let cur_len = content.len();
                        let growth = (new_end as usize).saturating_sub(cur_len);
                        if total_scratch + growth > MAX_TOTAL_SCRATCH_BYTES {
                            return -ENOSPC;
                        }
                        if (write_at as usize) > cur_len {
                            content.resize(write_at as usize, 0);
                        }
                        if (new_end as usize) > content.len() {
                            content.resize(new_end as usize, 0);
                        }
                        content[write_at as usize..new_end as usize].copy_from_slice(&data);
                        count as i64
                    }
                }
            }
        }
    }

    fn sys_lseek(&mut self, fd: u64, offset: i64, whence: u64) -> i64 {
        let Ok(fd32) = u32::try_from(fd) else { return -EBADF };
        let Some(file) = self.files.get_mut(&fd32) else {
            return -EBADF;
        };
        let cur = file.offset as i64;
        let len = match file.kind {
            FileKind::Stdin => self.stdin_data.len() as i64,
            FileKind::Stdout | FileKind::Stderr => return -ESPIPE,
            FileKind::Inode(ino) => {
                let Some(inode) = self.inodes.get(&ino) else {
                    return -EBADF;
                };
                match &inode.kind {
                    InodeKind::DevNull => 0,
                    InodeKind::ProcAuxv => self.auxv_data.len() as i64,
                    InodeKind::Regular(d) | InodeKind::HostFile(d) => d.len() as i64,
                    InodeKind::Directory | InodeKind::HostDir(_) => 4096,
                }
            }
        };
        let new_offset = match whence {
            0 => offset,
            1 => cur.checked_add(offset).unwrap_or(-1),
            2 => len.checked_add(offset).unwrap_or(-1),
            _ => return -EINVAL,
        };
        if new_offset < 0 {
            return -EINVAL;
        }
        file.offset = new_offset as u64;
        new_offset
    }

    fn sys_ftruncate(&mut self, fd: u64, length: u64) -> i64 {
        let Ok(fd32) = u32::try_from(fd) else { return -EBADF };
        let Some(file) = self.files.get(&fd32) else {
            return -EBADF;
        };
        if file.flags & O_ACCMODE == O_RDONLY {
            return -EINVAL;
        }
        let ino = match file.kind {
            FileKind::Inode(ino) => ino,
            _ => return -EINVAL,
        };
        if length as usize > MAX_FILE_BYTES {
            return -EFBIG;
        }
        let total_scratch = self.total_scratch_bytes();
        let Some(inode) = self.inodes.get_mut(&ino) else {
            return -EBADF;
        };
        let InodeKind::Regular(data) = &mut inode.kind else {
            return -EINVAL;
        };
        let cur_len = data.len();
        if length as usize > cur_len {
            let growth = length as usize - cur_len;
            if total_scratch + growth > MAX_TOTAL_SCRATCH_BYTES {
                return -ENOSPC;
            }
        }
        data.resize(length as usize, 0);
        0
    }

    fn sys_fcntl(&mut self, fd: u64, cmd: u64, arg: u64) -> i64 {
        let Ok(fd32) = u32::try_from(fd) else { return -EBADF };
        let Some(file) = self.files.get(&fd32).cloned() else {
            return -EBADF;
        };
        match cmd {
            0 => match self.alloc_fd(OpenFile { cloexec: false, ..file }, arg as u32) {
                Ok(new_fd) => new_fd as i64,
                Err(e) => e,
            },
            1030 => match self.alloc_fd(OpenFile { cloexec: true, ..file }, arg as u32) {
                Ok(new_fd) => new_fd as i64,
                Err(e) => e,
            },
            1 => {
                if file.cloexec {
                    1
                } else {
                    0
                }
            }
            2 => {
                if let Some(f) = self.files.get_mut(&fd32) {
                    f.cloexec = (arg & 1) != 0;
                }
                0
            }
            3 => file.flags as i64,
            4 => {
                if let Some(f) = self.files.get_mut(&fd32) {
                    f.flags = (f.flags & !O_APPEND) | (arg as u32 & O_APPEND);
                }
                0
            }
            _ => 0,
        }
    }

    /// Deterministic `getrandom`: a fixed-seed xorshift stream, never host entropy, so runs replay
    /// byte for byte. Accepts GRND_NONBLOCK|GRND_RANDOM|GRND_INSECURE; other bits are EINVAL.
    fn sys_getrandom(&mut self, buf: u64, len: u64, flags: u64, mmu: &mut Memory) -> i64 {
        if flags & !0x7 != 0 {
            return -EINVAL;
        }
        let n = len.min(MAX_GETRANDOM);
        let mut data = Vec::with_capacity(n as usize);
        while (data.len() as u64) < n {
            let mut x = self.rng;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.rng = x;
            data.extend_from_slice(&x.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes());
        }
        data.truncate(n as usize);
        if mmu.write_bytes(buf, &data).is_err() {
            return -EFAULT;
        }
        n as i64
    }

    fn sys_ppoll(&mut self, a: [u64; 6], mmu: &mut Memory) -> i64 {
        let (fds_ptr, nfds, tmo_ptr, sigmask_ptr, sigsetsize) = (a[0], a[1], a[2], a[3], a[4]);
        if nfds > 1024 {
            return -EINVAL;
        }
        if sigmask_ptr != 0 {
            if sigsetsize != 8 {
                return -EINVAL;
            }
            let mut mask = [0u8; 8];
            if mmu.read_bytes(sigmask_ptr, &mut mask).is_err() {
                return -EFAULT;
            }
        }
        if tmo_ptr != 0 {
            let mut ts = [0u8; 16];
            if mmu.read_bytes(tmo_ptr, &mut ts).is_err() {
                return -EFAULT;
            }
            let sec = i64::from_le_bytes(ts[0..8].try_into().unwrap());
            let nsec = i64::from_le_bytes(ts[8..16].try_into().unwrap());
            if sec < 0 || !(0..1_000_000_000).contains(&nsec) {
                return -EINVAL;
            }
        }
        let mut count = 0i64;
        for i in 0..nfds {
            let entry_addr = match fds_ptr.checked_add(i * 8) {
                Some(addr) => addr,
                None => return -EFAULT,
            };
            let mut buf = [0u8; 8];
            if mmu.read_bytes(entry_addr, &mut buf).is_err() {
                return -EFAULT;
            }
            let fd = i32::from_le_bytes(buf[0..4].try_into().unwrap());
            let events = i16::from_le_bytes(buf[4..6].try_into().unwrap());
            let revents: i16 = if fd < 0 {
                0
            } else if !self.is_fd_open(fd as u64) {
                0x0020
            } else {
                let mut r = 0i16;
                if self.is_fd_readable(fd as u64) {
                    r |= events & (0x0001 | 0x0040);
                }
                if self.is_fd_writable(fd as u64) {
                    r |= events & (0x0004 | 0x0100);
                }
                r
            };
            if mmu.write_bytes(entry_addr + 6, &revents.to_le_bytes()).is_err() {
                return -EFAULT;
            }
            if revents != 0 {
                count += 1;
            }
        }
        count
    }

    #[allow(clippy::too_many_arguments)]
    fn write_stat(
        &self,
        statbuf: u64,
        dev: u64,
        ino: u64,
        mode: u32,
        nlink: u32,
        rdev: u64,
        size: i64,
        mmu: &mut Memory,
    ) -> i64 {
        let mut b = [0u8; 128];
        b[0..8].copy_from_slice(&dev.to_le_bytes());
        b[8..16].copy_from_slice(&ino.to_le_bytes());
        b[16..20].copy_from_slice(&mode.to_le_bytes());
        b[20..24].copy_from_slice(&nlink.to_le_bytes());
        b[24..28].copy_from_slice(&(UID as u32).to_le_bytes());
        b[28..32].copy_from_slice(&(GID as u32).to_le_bytes());
        b[32..40].copy_from_slice(&rdev.to_le_bytes());
        b[48..56].copy_from_slice(&size.to_le_bytes());
        b[56..60].copy_from_slice(&4096i32.to_le_bytes());
        let blocks = (size.max(0) + 511) / 512;
        b[64..72].copy_from_slice(&blocks.to_le_bytes());
        let (sec, nsec) = if let Some(ticks) = self.deterministic_ticks {
            (1_700_000_000 + ticks / 1000, (ticks % 1000) * 1_000_000)
        } else {
            (1_700_000_000, 0)
        };
        b[72..80].copy_from_slice(&sec.to_le_bytes());
        b[80..88].copy_from_slice(&nsec.to_le_bytes());
        b[88..96].copy_from_slice(&sec.to_le_bytes());
        b[96..104].copy_from_slice(&nsec.to_le_bytes());
        b[104..112].copy_from_slice(&sec.to_le_bytes());
        b[112..120].copy_from_slice(&nsec.to_le_bytes());
        if mmu.write_bytes(statbuf, &b).is_err() {
            -EFAULT
        } else {
            0
        }
    }

    fn sys_fstat(&self, fd: u64, statbuf: u64, mmu: &mut Memory) -> i64 {
        let Ok(fd32) = u32::try_from(fd) else { return -EBADF };
        let Some(file) = self.files.get(&fd32) else {
            return -EBADF;
        };
        match file.kind {
            FileKind::Stdin => self.write_stat(statbuf, 1, 0, 0o100644, 1, 0, self.stdin_data.len() as i64, mmu),
            FileKind::Stdout => self.write_stat(statbuf, 2, 1, 0o20620, 1, 0x0501, 0, mmu),
            FileKind::Stderr => self.write_stat(statbuf, 2, 2, 0o20620, 1, 0x0501, 0, mmu),
            FileKind::Inode(ino) => {
                let Some(inode) = self.inodes.get(&ino) else {
                    return -EBADF;
                };
                match &inode.kind {
                    InodeKind::DevNull => self.write_stat(statbuf, 5, 1, 0o20666, 1, 0x0103, 0, mmu),
                    InodeKind::ProcAuxv => {
                        self.write_stat(statbuf, 6, 2, 0o100444, 1, 0, self.auxv_data.len() as i64, mmu)
                    }
                    InodeKind::Regular(d) | InodeKind::HostFile(d) => {
                        self.write_stat(statbuf, 10, ino, inode.mode, inode.nlink, 0, d.len() as i64, mmu)
                    }
                    InodeKind::Directory | InodeKind::HostDir(_) => {
                        self.write_stat(statbuf, 10, ino, inode.mode, inode.nlink, 0, 4096, mmu)
                    }
                }
            }
        }
    }

    fn sys_newfstatat(&self, dirfd: i64, path_ptr: u64, statbuf: u64, flags: i32, mmu: &mut Memory) -> i64 {
        if flags as u64 & AT_EMPTY_PATH != 0 {
            let mut first = [0u8; 1];
            if mmu.read_bytes(path_ptr, &mut first).is_err() {
                return -EFAULT;
            }
            if first[0] == 0 {
                return self.sys_fstat(dirfd as u64, statbuf, mmu);
            }
        }
        let path_str = match read_path(path_ptr, mmu) {
            Ok(p) => p,
            Err(e) => return e,
        };
        if path_str.is_empty() {
            return -ENOENT;
        }
        let norm = normalize_path(&path_str);
        if !is_allowed_path(&norm) {
            let Some(meta) = self.host_path(&norm).and_then(|p| std::fs::metadata(p).ok()) else {
                return -ENOENT;
            };
            #[cfg(unix)]
            let ino = std::os::unix::fs::MetadataExt::ino(&meta);
            #[cfg(not(unix))]
            let ino = 0;
            let (mode, nlink) = if meta.is_dir() { (0o40555, 2) } else { (0o100444, 1) };
            return self.write_stat(statbuf, 11, ino, mode, nlink, 0, meta.len() as i64, mmu);
        }
        if norm == "/dev/null" {
            return self.write_stat(statbuf, 5, 1, 0o20666, 1, 0x0103, 0, mmu);
        }
        if norm == "/proc/self/auxv" {
            return self.write_stat(statbuf, 6, 2, 0o100444, 1, 0, self.auxv_data.len() as i64, mmu);
        }
        if norm == "/tmp" || norm == "/var/tmp" {
            return self.write_stat(statbuf, 10, 3, 0o40777, 2, 0, 4096, mmu);
        }
        if let Some(&ino) = self.dentries.get(&norm)
            && let Some(inode) = self.inodes.get(&ino)
        {
            let size = if let InodeKind::Regular(d) = &inode.kind {
                d.len() as i64
            } else {
                0
            };
            return self.write_stat(statbuf, 10, ino, inode.mode, inode.nlink, 0, size, mmu);
        }
        -ENOENT
    }

    fn sys_writev(&mut self, a: [u64; 6], mmu: &mut Memory, out: &mut dyn Output) -> i64 {
        let (fd, iov, cnt) = (a[0], a[1], a[2]);
        if fd != 1 && fd != 2 {
            return -EBADF;
        }
        if cnt > MAX_IOV {
            return -EINVAL;
        }
        let mut total = 0u64;
        let mut chunks = Vec::new();
        for i in 0..cnt {
            let (Ok(base), Ok(len)) = (mmu.read_u64(iov + i * 16), mmu.read_u64(iov + i * 16 + 8)) else {
                return -EFAULT;
            };
            if len > MAX_IO_CALL || total + len > MAX_IO_CALL {
                return -EINVAL;
            }
            let mut data = vec![0u8; len as usize];
            if mmu.read_bytes(base, &mut data).is_err() {
                return -EFAULT;
            }
            total += len;
            chunks.push(data);
        }
        for c in chunks {
            out.write(fd, &c);
        }
        total as i64
    }

    fn sys_brk(&mut self, addr: u64, mmu: &mut Memory) -> i64 {
        if addr < self.brk_start || addr > self.brk_start + MAX_BRK_GROWTH {
            return self.brk_cur as i64; // Linux returns the unchanged break on failure.
        }
        let new_end = align_up(addr);
        if new_end > self.brk_mapped_end {
            mmu.map_range(
                self.brk_mapped_end,
                (new_end - self.brk_mapped_end) as usize,
                Permission::READ_WRITE,
            );
        } else if new_end < self.brk_mapped_end {
            mmu.unmap_range(new_end, (self.brk_mapped_end - new_end) as usize);
        }
        self.brk_mapped_end = new_end;
        self.brk_cur = addr;
        addr as i64
    }

    fn mapped_pages(mmu: &Memory, addr: u64, len: u64) -> u64 {
        mmu.regions()
            .iter()
            .map(|&(s, e, _)| e.min(addr.saturating_add(len)).saturating_sub(s.max(addr)) / PAGE)
            .sum()
    }

    fn forget_mapping(&mut self, start: u64, end: u64) {
        let mut ranges = Vec::new();
        for &(s, e) in &self.mappings {
            if e <= start || s >= end {
                ranges.push((s, e));
            } else {
                if s < start {
                    ranges.push((s, start));
                }
                if e > end {
                    ranges.push((end, e));
                }
            }
        }
        self.mappings = ranges;
    }

    pub(super) fn covered(&self, start: u64, end: u64, mmu: &Memory) -> bool {
        let mut ranges = self.mappings.clone();
        ranges.extend(mmu.regions().into_iter().map(|(s, e, _)| (s, e)));
        ranges.sort_unstable();
        let mut next = start;
        for (s, e) in ranges {
            if s > next {
                break;
            }
            next = next.max(e);
            if next >= end {
                return true;
            }
        }
        false
    }

    fn accounted_bytes(&self, mmu: &Memory, start: u64, len: u64) -> u64 {
        self.mappings
            .iter()
            .map(|&(s, e)| {
                let lo = s.max(start);
                let hi = e.min(start + len);
                if hi > lo {
                    Self::mapped_pages(mmu, lo, hi - lo) * PAGE
                } else {
                    0
                }
            })
            .sum()
    }

    fn sys_mmap(&mut self, a: [u64; 6], mmu: &mut Memory) -> Result<i64, String> {
        let (addr, len, prot, flags, fd) = (a[0], a[1], a[2], a[3], a[4] as i64);
        if flags & MAP_ANONYMOUS == 0 {
            return Err(format!(
                "mmap of file descriptor {fd} (only MAP_ANONYMOUS is supported), flags={flags:#x} at x0={addr:#x} len={len:#x}"
            ));
        }
        if len == 0 || len > u64::MAX - (PAGE - 1) || prot & !7 != 0 {
            return Ok(-EINVAL);
        }
        let len = align_up(len);
        let backed = if prot == 0 { 0 } else { len };
        if backed > MAX_MMAP_CALL {
            return Ok(-ENOMEM);
        }
        let base = if flags & MAP_FIXED != 0 {
            if addr % PAGE != 0 || addr < PAGE || addr.checked_add(len).is_none_or(|e| e > STACK_TOP - STACK_SIZE) {
                return Ok(-EINVAL);
            }
            addr
        } else {
            // Hints are advisory. Never overlap a live reservation or a fixed mapping.
            let hint = addr & !(PAGE - 1);
            let mut b = if hint >= MMAP_BASE
                && hint
                    .checked_add(len)
                    .is_some_and(|e| e <= STACK_TOP - STACK_SIZE - (1 << 32))
            {
                hint
            } else {
                self.mmap_next
            };
            loop {
                let Some(end) = b.checked_add(len).filter(|&e| e <= STACK_TOP - STACK_SIZE - (1 << 32)) else {
                    return Ok(-ENOMEM);
                };
                let overlap = self
                    .mappings
                    .iter()
                    .filter(|&&(s, e)| s < end && e > b)
                    .map(|&(_, e)| e)
                    .max();
                match overlap {
                    Some(e) => b = e,
                    None => break b,
                }
            }
        };
        let old = self.accounted_bytes(mmu, base, len);
        if self.mmap_total.saturating_sub(old) + backed > MAX_MMAP_TOTAL {
            return Ok(-ENOMEM);
        }
        mmu.unmap_range(base, len as usize);
        self.forget_mapping(base, base + len);
        self.mappings.push((base, base + len));
        if backed != 0 {
            mmu.map_range(base, len as usize, protection(prot));
        }
        self.mmap_total = self.mmap_total.saturating_sub(old) + backed;
        self.mmap_next = self.mmap_next.max(base + len);
        Ok(base as i64)
    }

    fn sys_mprotect(&mut self, a: [u64; 6], mmu: &mut Memory) -> i64 {
        let (addr, len, prot) = (a[0], a[1], a[2]);
        if addr % PAGE != 0 || prot & !7 != 0 || len > u64::MAX - (PAGE - 1) {
            return -EINVAL;
        }
        let len = align_up(len);
        let Some(end) = addr.checked_add(len) else {
            return -ENOMEM;
        };
        if len == 0 {
            return 0;
        }
        if !self.covered(addr, end, mmu) {
            return -ENOMEM;
        }
        let old = Self::mapped_pages(mmu, addr, len) * PAGE;
        if prot != 0 {
            let additional = len - old;
            if len > MAX_MMAP_CALL || self.mmap_total + additional > MAX_MMAP_TOTAL {
                return -ENOMEM;
            }
            mmu.commit_range(addr, len as usize, protection(prot));
            self.mmap_total += additional;
        } else {
            mmu.protect_range(addr, len as usize, Permission::NONE);
        }
        0
    }

    fn sys_munmap(&mut self, addr: u64, len: u64, mmu: &mut Memory) -> i64 {
        if !addr.is_multiple_of(PAGE) || len == 0 || len > u64::MAX - (PAGE - 1) {
            return -EINVAL;
        }
        let len = align_up(len);
        if addr < PAGE || addr.checked_add(len).is_none_or(|e| e > STACK_TOP - STACK_SIZE) {
            return -EINVAL;
        }
        let freed = self.accounted_bytes(mmu, addr, len);
        mmu.unmap_range(addr, len as usize);
        self.forget_mapping(addr, addr + len);
        self.mmap_total = self.mmap_total.saturating_sub(freed);
        0
    }

    /// `mremap(old, old_len, new_len, flags)` for anonymous read/write mappings, as musl's
    /// `realloc` of large blocks uses it. Shrinking is in place. Growing moves the block
    /// (MREMAP_MAYMOVE) to fresh address space and copies the contents; without MAYMOVE, or
    /// with MREMAP_FIXED, growth fails with ENOMEM/EINVAL so callers fall back to copying.
    fn sys_mremap(&mut self, a: [u64; 6], mmu: &mut Memory) -> i64 {
        let (old, old_len, new_len, flags) = (a[0], a[1], a[2], a[3]);
        if !old.is_multiple_of(PAGE) || old_len == 0 || new_len == 0 || flags & !MREMAP_MAYMOVE != 0 {
            return -EINVAL;
        }
        let (old_len, new_len) = (align_up(old_len), align_up(new_len));
        // musl pthread_getattr_np probes the main stack with a non-moving
        // growth request, stopping when it reaches an unmapped page.
        if old >= STACK_TOP - STACK_SIZE
            && old.checked_add(old_len).is_some_and(|e| e <= STACK_TOP)
            && flags == 0
            && new_len > old_len
            && Self::mapped_pages(mmu, old, old_len) * PAGE == old_len
        {
            return -ENOMEM;
        }
        if old < MMAP_BASE
            || old.checked_add(old_len).is_none_or(|e| e > STACK_TOP - STACK_SIZE)
            || Self::mapped_pages(mmu, old, old_len) * PAGE != old_len
        {
            return -EFAULT;
        }
        if new_len == old_len {
            return old as i64;
        }
        if new_len < old_len {
            let freed = old_len - new_len;
            mmu.unmap_range(old + new_len, freed as usize);
            self.forget_mapping(old + new_len, old + old_len);
            self.mmap_total -= freed.min(self.mmap_total);
            return old as i64;
        }
        if flags & MREMAP_MAYMOVE == 0
            || new_len > MAX_MMAP_CALL
            || self.mmap_total - old_len + new_len > MAX_MMAP_TOTAL
        {
            return -ENOMEM;
        }
        let base = self.mmap_next;
        if base + new_len > STACK_TOP - STACK_SIZE - (1 << 32) {
            return -ENOMEM;
        }
        let mut data = vec![0u8; old_len as usize];
        if mmu.read_bytes(old, &mut data).is_err() {
            return -EFAULT;
        }
        self.forget_mapping(old, old + old_len);
        self.mappings.push((base, base + new_len));
        self.mmap_next = base + new_len;
        mmu.map_range(
            base,
            new_len as usize,
            Permission {
                read: true,
                write: true,
                execute: false,
            },
        );
        if mmu.write_bytes(base, &data).is_err() {
            return -EFAULT;
        }
        mmu.unmap_range(old, old_len as usize);
        self.mmap_total = self.mmap_total - old_len + new_len;
        base as i64
    }

    /// The current time of clock `clk` (`CLOCK_REALTIME` 0, `MONOTONIC` 1, the CPU-time clocks 2 and 3,
    /// `MONOTONIC_RAW` 4, `BOOTTIME` 7) as seconds and nanoseconds, without advancing the deterministic clock.
    fn clock_now(&self, clk: u64) -> Option<(u64, u64)> {
        if !matches!(clk, 0 | 1 | 2 | 3 | 4 | 7) {
            return None;
        }
        Some(if let Some(ticks) = self.deterministic_ticks {
            if matches!(clk, 2 | 3) {
                (
                    self.retired_instructions / 1_000_000_000,
                    self.retired_instructions % 1_000_000_000,
                )
            } else {
                let epoch = if clk == 0 { 1_700_000_000 } else { 0 };
                (epoch + ticks / 1000, (ticks % 1000) * 1_000_000)
            }
        } else {
            match clk {
                0 => {
                    let d = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    (d.as_secs(), d.subsec_nanos() as u64)
                }
                1 | 4 | 7 => {
                    let d = self.started.elapsed() + std::time::Duration::from_secs(1000);
                    (d.as_secs(), d.subsec_nanos() as u64)
                }
                2 | 3 => {
                    let d = self.started.elapsed();
                    (d.as_secs(), d.subsec_nanos() as u64)
                }
                _ => unreachable!(),
            }
        })
    }

    fn sys_clock_gettime(&mut self, clk: u64, ts: u64, mmu: &mut Memory) -> i64 {
        let Some((sec, nsec)) = self.clock_now(clk) else {
            return -EINVAL;
        };
        let mut buf = [0u8; 16];
        buf[..8].copy_from_slice(&sec.to_le_bytes());
        buf[8..].copy_from_slice(&nsec.to_le_bytes());
        if mmu.write_bytes(ts, &buf).is_err() {
            -EFAULT
        } else {
            if let Some(ticks) = &mut self.deterministic_ticks
                && !matches!(clk, 2 | 3)
            {
                *ticks = ticks.saturating_add(1);
            }
            0
        }
    }

    /// `nanosleep(req, rem)` (`clk` None) and `clock_nanosleep(clk, flags, req, rem)`. With `--deterministic`
    /// the wall and monotonic clocks jump ahead by the sleep (whole milliseconds, rounded up) instead of waiting;
    /// otherwise the host sleeps. `TIMER_ABSTIME` sleeps until that time on `clk`. Never interrupted, so
    /// `rem` is left alone.
    fn sys_nanosleep(&mut self, clk: Option<u64>, flags: u64, req: u64, mmu: &mut Memory) -> i64 {
        const TIMER_ABSTIME: u64 = 1;
        let (Ok(sec), Ok(nsec)) = (mmu.read_u64(req), mmu.read_u64(req + 8)) else {
            return -EFAULT;
        };
        if (sec as i64) < 0 || nsec >= 1_000_000_000 {
            return -EINVAL;
        }
        let mut ns = u128::from(sec) * 1_000_000_000 + u128::from(nsec);
        if let Some(clk) = clk {
            if matches!(clk, 2 | 3) {
                return -EINVAL; // sleeping on the calling thread's own CPU-time clock
            }
            let Some((now_s, now_ns)) = self.clock_now(clk) else {
                return -EINVAL;
            };
            if flags & TIMER_ABSTIME != 0 {
                ns = ns.saturating_sub(u128::from(now_s) * 1_000_000_000 + u128::from(now_ns));
            }
        }
        if let Some(ticks) = &mut self.deterministic_ticks {
            *ticks = ticks.saturating_add(ns.div_ceil(1_000_000) as u64);
        } else {
            std::thread::sleep(std::time::Duration::from_nanos(ns.min(u128::from(u64::MAX)) as u64));
        }
        0
    }

    fn sys_gettimeofday(&mut self, tv: u64, _tz: u64, mmu: &mut Memory) -> i64 {
        if tv == 0 {
            return 0;
        }
        let (sec, nsec) = if let Some(ticks) = self.deterministic_ticks {
            let epoch = 1_700_000_000u64;
            (epoch + ticks / 1000, (ticks % 1000) * 1_000_000)
        } else {
            let d = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            (d.as_secs(), d.subsec_nanos() as u64)
        };
        let usec = nsec / 1000;
        let mut buf = [0u8; 16];
        buf[..8].copy_from_slice(&sec.to_le_bytes());
        buf[8..].copy_from_slice(&usec.to_le_bytes());
        if mmu.write_bytes(tv, &buf).is_err() {
            -EFAULT
        } else {
            if let Some(ticks) = &mut self.deterministic_ticks {
                *ticks = ticks.saturating_add(1);
            }
            0
        }
    }
}

fn sys_uname(buf: u64, mmu: &mut Memory) -> i64 {
    let fields = ["Linux", "ounce-core", "6.1.0-ounce-core", "#1 SMP", "aarch64", "(none)"];
    let mut data = vec![0u8; 65 * 6];
    for (i, f) in fields.iter().enumerate() {
        data[i * 65..i * 65 + f.len()].copy_from_slice(f.as_bytes());
    }
    if mmu.write_bytes(buf, &data).is_err() {
        -EFAULT
    } else {
        0
    }
}

/// Setting affinity on a 1-CPU machine: pid 0 or PID is accepted. The mask must be a multiple of 8
/// and include CPU 0 (the only online CPU); requesting a mask without CPU 0 returns -EINVAL.
fn sys_sched_setaffinity(pid: u64, len: u64, mask: u64, mmu: &Memory) -> i64 {
    if pid != 0 && pid != PID {
        return -ESRCH;
    }
    if len < 8 || !len.is_multiple_of(8) {
        return -EINVAL;
    }
    let mut buf = [0u8; 8];
    if mmu.read_bytes(mask, &mut buf).is_err() {
        return -EFAULT;
    }
    let mask_val = u64::from_le_bytes(buf);
    if (mask_val & 1) == 0 {
        return -EINVAL;
    }
    0
}

/// The guest sees one CPU. Like the kernel, the mask is one `unsigned long` (8 bytes,
/// CPU 0 set), `len` must hold it and be a multiple of 8, and the return value is the
/// number of bytes written. musl's `sysconf(_SC_NPROCESSORS_ONLN)` asks this at startup.
fn sys_sched_getaffinity(pid: u64, len: u64, mask: u64, mmu: &mut Memory) -> i64 {
    if pid != 0 && pid != PID {
        return -ESRCH;
    }
    if len < 8 || !len.is_multiple_of(8) {
        return -EINVAL;
    }
    if mmu.write_bytes(mask, &1u64.to_le_bytes()).is_err() {
        -EFAULT
    } else {
        8
    }
}

/// Reports CPU 0 and NUMA node 0. tcache is unused in modern Linux.
fn sys_getcpu(cpu_ptr: u64, node_ptr: u64, _tcache: u64, mmu: &mut Memory) -> i64 {
    if cpu_ptr != 0 && mmu.write_u32(cpu_ptr, 0).is_err() {
        return -EFAULT;
    }
    if node_ptr != 0 && mmu.write_u32(node_ptr, 0).is_err() {
        return -EFAULT;
    }
    0
}

pub fn syscall_name(nr: u64) -> Option<&'static str> {
    Some(match nr {
        25 => "fcntl",
        35 => "unlinkat",
        46 => "ftruncate",
        48 => "faccessat",
        56 => "openat",
        57 => "close",
        62 => "lseek",
        63 => "read",
        64 => "write",
        65 => "readv",
        66 => "writev",
        67 => "pread64",
        68 => "pwrite64",
        72 => "pselect6",
        73 => "ppoll",
        78 => "readlinkat",
        79 => "newfstatat",
        80 => "fstat",
        98 => "futex",
        99 => "set_robust_list",
        101 => "nanosleep",
        115 => "clock_nanosleep",
        122 => "sched_setaffinity",
        123 => "sched_getaffinity",
        124 => "sched_yield",
        129 => "kill",
        131 => "tgkill",
        132 => "sigaltstack",
        168 => "getcpu",
        169 => "gettimeofday",
        173 => "getppid",
        174 => "getuid",
        175 => "geteuid",
        176 => "getgid",
        177 => "getegid",
        178 => "gettid",
        220 => "clone",
        221 => "execve",
        226 => "mprotect",
        233 => "madvise",
        261 => "prlimit64",
        278 => "getrandom",
        280 => "bpf",
        291 => "statx",
        293 => "rseq",
        435 => "clone3",
        _ => return None,
    })
}

fn describe_unimplemented(nr: u64, a: &[u64; 6]) -> String {
    let name = syscall_name(nr).map(|n| format!(" ({n})")).unwrap_or_default();
    format!(
        "unimplemented Linux syscall {nr}{name}; args x0..x5 = {:#x}, {:#x}, {:#x}, {:#x}, {:#x}, {:#x}",
        a[0], a[1], a[2], a[3], a[4], a[5]
    )
}

fn protection(prot: u64) -> Permission {
    Permission {
        read: prot & (PROT_READ | PROT_WRITE | PROT_EXEC) != 0,
        write: prot & PROT_WRITE != 0,
        execute: prot & PROT_EXEC != 0,
    }
}

fn read_path(ptr: u64, mmu: &Memory) -> Result<String, i64> {
    let mut bytes = Vec::new();
    for i in 0..4096 {
        let mut b = [0u8; 1];
        if mmu.read_bytes(ptr.wrapping_add(i), &mut b).is_err() {
            return Err(-EFAULT);
        }
        if b[0] == 0 {
            let s = std::str::from_utf8(&bytes).map_err(|_| -ENOENT)?;
            return Ok(s.to_string());
        }
        bytes.push(b[0]);
    }
    Err(-ENAMETOOLONG)
}

fn normalize_path(raw: &str) -> String {
    let mut parts = Vec::new();
    for p in raw.split('/') {
        if p.is_empty() || p == "." {
            continue;
        }
        if p == ".." {
            parts.pop();
        } else {
            parts.push(p);
        }
    }
    format!("/{}", parts.join("/"))
}

fn is_allowed_path(path: &str) -> bool {
    path == "/dev/null"
        || path == "/proc/self/auxv"
        || path == "/tmp"
        || path.starts_with("/tmp/")
        || path == "/var/tmp"
        || path.starts_with("/var/tmp/")
}
