//! Minimal Linux AArch64 syscall layer: `SVC #0` with the number in x8 and arguments in x0..x5.
//! Anything not handled here is reported as [`Outcome::Unimplemented`] so the runner can name it.

use super::elf::{align_up, MMAP_BASE, PAGE, STACK_SIZE, STACK_TOP};
use crate::mmu::{Memory, Permission};
use std::time::Instant;

pub const ENOENT: i64 = 2;
pub const EBADF: i64 = 9;
pub const ENOMEM: i64 = 12;
pub const EFAULT: i64 = 14;
pub const EINVAL: i64 = 22;
pub const ENOTTY: i64 = 25;

pub const SYS_IOCTL: u64 = 29;
pub const SYS_OPENAT: u64 = 56;
pub const SYS_WRITE: u64 = 64;
pub const SYS_WRITEV: u64 = 66;
pub const SYS_READLINKAT: u64 = 78;
pub const SYS_EXIT: u64 = 93;
pub const SYS_EXIT_GROUP: u64 = 94;
pub const SYS_SET_TID_ADDRESS: u64 = 96;
pub const SYS_CLOCK_GETTIME: u64 = 113;
pub const SYS_RT_SIGACTION: u64 = 134;
pub const SYS_RT_SIGPROCMASK: u64 = 135;
pub const SYS_UNAME: u64 = 160;
pub const SYS_GETPID: u64 = 172;
pub const SYS_BRK: u64 = 214;
pub const SYS_MUNMAP: u64 = 215;
pub const SYS_MMAP: u64 = 222;

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
pub const PID: u64 = 1;

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
    started: Instant,
    deterministic_ticks: Option<u64>,
}

impl Process {
    pub fn new(brk_start: u64) -> Self {
        Self { brk_start, brk_cur: brk_start, brk_mapped_end: brk_start, mmap_next: MMAP_BASE, mmap_total: 0, started: Instant::now(), deterministic_ticks: None }
    }

    /// Fixed virtual time for reproducible single-process experiments. Each successful
    /// clock read advances the shared clock by 1 ms; rejected reads do not advance it.
    pub fn new_deterministic(brk_start: u64) -> Self {
        Self { deterministic_ticks: Some(0), ..Self::new(brk_start) }
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
            SYS_MUNMAP => self.sys_munmap(a[0], a[1], mmu),
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
                // oldact (x2), if requested, reads back as SIG_DFL with an empty mask.
                if a[2] != 0 && mmu.write_bytes(a[2], &[0u8; 32]).is_err() {
                    -EFAULT
                } else {
                    0
                }
            }
            SYS_RT_SIGPROCMASK => {
                if a[2] != 0 && mmu.write_bytes(a[2], &[0u8; 8]).is_err() {
                    -EFAULT
                } else {
                    0
                }
            }
            SYS_GETPID => PID as i64,
            SYS_CLOCK_GETTIME => self.sys_clock_gettime(a[0], a[1], mmu),
            SYS_UNAME => sys_uname(a[0], mmu),
            // The guest has no filesystem.
            SYS_OPENAT | SYS_READLINKAT => -ENOENT,
            _ => return Outcome::Unimplemented(describe_unimplemented(nr, &a)),
        };
        Outcome::Return(r)
    }

    fn sys_write(&mut self, a: [u64; 6], mmu: &mut Memory, out: &mut dyn Output) -> i64 {
        let (fd, buf, count) = (a[0], a[1], a[2].min(MAX_IO_CALL));
        if fd != 1 && fd != 2 {
            return -EBADF;
        }
        let mut data = vec![0u8; count as usize];
        if mmu.read_bytes(buf, &mut data).is_err() {
            return -EFAULT;
        }
        out.write(fd, &data);
        count as i64
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
            mmu.map_range(self.brk_mapped_end, (new_end - self.brk_mapped_end) as usize, Permission::READ_WRITE);
        } else if new_end < self.brk_mapped_end {
            mmu.unmap_range(new_end, (self.brk_mapped_end - new_end) as usize);
        }
        self.brk_mapped_end = new_end;
        self.brk_cur = addr;
        addr as i64
    }

    fn mapped_pages(mmu: &Memory, addr: u64, len: u64) -> u64 {
        (0..len / PAGE).filter(|i| mmu.is_mapped(addr + i * PAGE)).count() as u64
    }

    fn sys_mmap(&mut self, a: [u64; 6], mmu: &mut Memory) -> Result<i64, String> {
        let (addr, len, prot, flags, fd) = (a[0], a[1], a[2], a[3], a[4] as i64);
        if flags & MAP_ANONYMOUS == 0 {
            return Err(format!("mmap of file descriptor {fd} (only MAP_ANONYMOUS is supported), flags={flags:#x} at x0={addr:#x} len={len:#x}"));
        }
        if len == 0 {
            return Ok(-EINVAL);
        }
        let len = align_up(len);
        if len > MAX_MMAP_CALL || self.mmap_total + len > MAX_MMAP_TOTAL {
            return Ok(-ENOMEM);
        }
        let perm = Permission {
            read: prot & PROT_READ != 0 || prot & (PROT_WRITE | PROT_EXEC) != 0,
            write: prot & PROT_WRITE != 0,
            execute: prot & PROT_EXEC != 0,
        };
        let base = if flags & MAP_FIXED != 0 {
            if addr % PAGE != 0 || addr < MMAP_BASE || addr.checked_add(len).is_none_or(|e| e > STACK_TOP - STACK_SIZE) {
                return Ok(-EINVAL);
            }
            // Replaces whatever was there.
            let old = Self::mapped_pages(mmu, addr, len) * PAGE;
            self.mmap_total -= old.min(self.mmap_total);
            addr
        } else {
            let b = self.mmap_next;
            if b + len > STACK_TOP - STACK_SIZE - (1 << 32) {
                return Ok(-ENOMEM);
            }
            self.mmap_next = b + len;
            b
        };
        mmu.map_range(base, len as usize, perm);
        self.mmap_total += len;
        Ok(base as i64)
    }

    fn sys_munmap(&mut self, addr: u64, len: u64, mmu: &mut Memory) -> i64 {
        if !addr.is_multiple_of(PAGE) || len == 0 || len > MAX_MMAP_TOTAL * 4 {
            return -EINVAL;
        }
        let len = align_up(len);
        if addr < MMAP_BASE || addr.checked_add(len).is_none_or(|e| e > STACK_TOP - STACK_SIZE) {
            return -EINVAL;
        }
        // Count only pages that exist; a huge range over empty space is cheap to skip.
        let freed = Self::mapped_pages(mmu, addr, len) * PAGE;
        mmu.unmap_range(addr, len as usize);
        self.mmap_total -= freed.min(self.mmap_total);
        0
    }

    fn sys_clock_gettime(&mut self, clk: u64, ts: u64, mmu: &mut Memory) -> i64 {
        if !matches!(clk, 0 | 1 | 4 | 7) {
            return -EINVAL;
        }
        let (sec, nsec) = if let Some(ticks) = self.deterministic_ticks {
            let epoch = if clk == 0 { 1_700_000_000 } else { 0 };
            (epoch + ticks / 1000, (ticks % 1000) * 1_000_000)
        } else {
            match clk {
                0 => {
                    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                    (d.as_secs(), d.subsec_nanos() as u64)
                }
                1 | 4 | 7 => {
                    let d = self.started.elapsed() + std::time::Duration::from_secs(1000);
                    (d.as_secs(), d.subsec_nanos() as u64)
                }
                _ => unreachable!(),
            }
        };
        let mut buf = [0u8; 16];
        buf[..8].copy_from_slice(&sec.to_le_bytes());
        buf[8..].copy_from_slice(&nsec.to_le_bytes());
        if mmu.write_bytes(ts, &buf).is_err() {
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

pub fn syscall_name(nr: u64) -> Option<&'static str> {
    Some(match nr {
        25 => "fcntl",
        48 => "faccessat",
        57 => "close",
        62 => "lseek",
        63 => "read",
        65 => "readv",
        72 => "pselect6",
        73 => "ppoll",
        79 => "newfstatat",
        80 => "fstat",
        98 => "futex",
        99 => "set_robust_list",
        101 => "nanosleep",
        115 => "clock_nanosleep",
        124 => "sched_yield",
        129 => "kill",
        131 => "tgkill",
        132 => "sigaltstack",
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
