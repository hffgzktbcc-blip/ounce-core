//! Minimal Linux AArch64 syscall layer: `SVC #0` with the number in x8 and arguments in x0..x5.
//! Anything not handled here is reported as [`Outcome::Unimplemented`] so the runner can name it.

use super::elf::{align_up, MMAP_BASE, PAGE, STACK_SIZE, STACK_TOP};
use crate::mmu::{Memory, Permission};
use std::time::Instant;

pub const ENOENT: i64 = 2;
pub const ESRCH: i64 = 3;
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
pub const SYS_SCHED_GETAFFINITY: u64 = 123;
pub const SYS_RT_SIGACTION: u64 = 134;
pub const SYS_RT_SIGPROCMASK: u64 = 135;
pub const SYS_UNAME: u64 = 160;
pub const SYS_GETPID: u64 = 172;
pub const SYS_BRK: u64 = 214;
pub const SYS_MUNMAP: u64 = 215;
pub const SYS_MREMAP: u64 = 216;
pub const SYS_MMAP: u64 = 222;
pub const SYS_MPROTECT: u64 = 226;

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
    /// Virtual anonymous mappings, including unbacked PROT_NONE reservations.
    mappings: Vec<(u64, u64)>,
    pub(super) signal_mask: u64,
    pub(super) alt_stack: [u64; 3],
    pub(super) sigactions: [[u8; 32]; 65],
    started: Instant,
    deterministic_ticks: Option<u64>,
}

impl Process {
    pub fn new(brk_start: u64) -> Self {
        Self { brk_start, brk_cur: brk_start, brk_mapped_end: brk_start, mmap_next: MMAP_BASE, mmap_total: 0, mappings: Vec::new(), sigactions: [[0; 32]; 65], signal_mask: 0, alt_stack: [0, 2, 0], started: Instant::now(), deterministic_ticks: None }
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
                    if a[1] != 0 && mmu.read_bytes(a[1], &mut action).is_err() {
                        -EFAULT
                    } else if a[2] != 0 && mmu.write_bytes(a[2], &self.sigactions[a[0] as usize]).is_err() {
                        -EFAULT
                    } else {
                        if a[1] != 0 { self.sigactions[a[0] as usize] = action; }
                        0
                    }
                }
            }
            SYS_RT_SIGPROCMASK => {
                let old = self.signal_mask;
                if a[3] != 8 || (a[1] != 0 && a[0] > 2) { -EINVAL }
                else if a[2] != 0 && mmu.write_u64(a[2], old).is_err() { -EFAULT }
                else if a[1] == 0 { 0 }
                else if let Ok(mask) = mmu.read_u64(a[1]) {
                    self.signal_mask = match a[0] { 0 => old | mask, 1 => old & !mask, _ => mask } & !(1 << 8 | 1 << 18);
                    0
                } else { -EFAULT }
            }
            132 => self.sys_sigaltstack(a, mmu),
            SYS_GETPID | 178 => PID as i64,
            261 => {
                if a[0] != 0 && a[0] != PID { -ESRCH }
                else if a[1] != 3 || a[2] != 0 { -EINVAL }
                else if a[3] == 0 { 0 }
                else if mmu.write_u64(a[3], STACK_SIZE).is_err() || mmu.write_u64(a[3].saturating_add(8), STACK_SIZE).is_err() { -EFAULT }
                else { 0 }
            }
            163 => {
                if a[0] != 3 { -EINVAL }
                else if mmu.write_u64(a[1], STACK_SIZE).is_err() || mmu.write_u64(a[1].saturating_add(8), STACK_SIZE).is_err() { -EFAULT }
                else { 0 }
            }
            SYS_CLOCK_GETTIME => self.sys_clock_gettime(a[0], a[1], mmu),
            SYS_UNAME => sys_uname(a[0], mmu),
            SYS_SCHED_GETAFFINITY => sys_sched_getaffinity(a[0], a[1], a[2], mmu),
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
        mmu.regions().iter().map(|&(s, e, _)| e.min(addr.saturating_add(len)).saturating_sub(s.max(addr)) / PAGE).sum()
    }

    fn forget_mapping(&mut self, start: u64, end: u64) {
        let mut ranges = Vec::new();
        for &(s, e) in &self.mappings {
            if e <= start || s >= end { ranges.push((s, e)); }
            else {
                if s < start { ranges.push((s, start)); }
                if e > end { ranges.push((end, e)); }
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
            if s > next { break; }
            next = next.max(e);
            if next >= end { return true; }
        }
        false
    }

    fn accounted_bytes(&self, mmu: &Memory, start: u64, len: u64) -> u64 {
        self.mappings.iter().map(|&(s, e)| {
            let lo = s.max(start);
            let hi = e.min(start + len);
            if hi > lo { Self::mapped_pages(mmu, lo, hi - lo) * PAGE } else { 0 }
        }).sum()
    }

    fn sys_mmap(&mut self, a: [u64; 6], mmu: &mut Memory) -> Result<i64, String> {
        let (addr, len, prot, flags, fd) = (a[0], a[1], a[2], a[3], a[4] as i64);
        if flags & MAP_ANONYMOUS == 0 {
            return Err(format!("mmap of file descriptor {fd} (only MAP_ANONYMOUS is supported), flags={flags:#x} at x0={addr:#x} len={len:#x}"));
        }
        if len == 0 || len > u64::MAX - (PAGE - 1) || prot & !7 != 0 { return Ok(-EINVAL); }
        let len = align_up(len);
        let backed = if prot == 0 { 0 } else { len };
        if backed > MAX_MMAP_CALL { return Ok(-ENOMEM); }
        let base = if flags & MAP_FIXED != 0 {
            if addr % PAGE != 0 || addr < PAGE || addr.checked_add(len).is_none_or(|e| e > STACK_TOP - STACK_SIZE) {
                return Ok(-EINVAL);
            }
            addr
        } else {
            // Hints are advisory. Never overlap a live reservation or a fixed mapping.
            let hint = addr & !(PAGE - 1);
            let mut b = if hint >= MMAP_BASE && hint.checked_add(len).is_some_and(|e| e <= STACK_TOP - STACK_SIZE - (1 << 32)) { hint } else { self.mmap_next };
            loop {
                let Some(end) = b.checked_add(len).filter(|&e| e <= STACK_TOP - STACK_SIZE - (1 << 32)) else { return Ok(-ENOMEM); };
                let overlap = self.mappings.iter().filter(|&&(s, e)| s < end && e > b).map(|&(_, e)| e).max();
                match overlap { Some(e) => b = e, None => break b }
            }
        };
        let old = self.accounted_bytes(mmu, base, len);
        if self.mmap_total.saturating_sub(old) + backed > MAX_MMAP_TOTAL { return Ok(-ENOMEM); }
        mmu.unmap_range(base, len as usize);
        self.forget_mapping(base, base + len);
        self.mappings.push((base, base + len));
        if backed != 0 { mmu.map_range(base, len as usize, protection(prot)); }
        self.mmap_total = self.mmap_total.saturating_sub(old) + backed;
        self.mmap_next = self.mmap_next.max(base + len);
        Ok(base as i64)
    }

    fn sys_mprotect(&mut self, a: [u64; 6], mmu: &mut Memory) -> i64 {
        let (addr, len, prot) = (a[0], a[1], a[2]);
        if addr % PAGE != 0 || prot & !7 != 0 || len > u64::MAX - (PAGE - 1) { return -EINVAL; }
        let len = align_up(len);
        let Some(end) = addr.checked_add(len) else { return -ENOMEM; };
        if len == 0 { return 0; }
        if !self.covered(addr, end, mmu) { return -ENOMEM; }
        let old = Self::mapped_pages(mmu, addr, len) * PAGE;
        if prot != 0 {
            let additional = len - old;
            if len > MAX_MMAP_CALL || self.mmap_total + additional > MAX_MMAP_TOTAL { return -ENOMEM; }
            mmu.commit_range(addr, len as usize, protection(prot));
            self.mmap_total += additional;
        } else {
            mmu.protect_range(addr, len as usize, Permission::NONE);
        }
        0
    }

    fn sys_munmap(&mut self, addr: u64, len: u64, mmu: &mut Memory) -> i64 {
        if addr % PAGE != 0 || len == 0 || len > u64::MAX - (PAGE - 1) { return -EINVAL; }
        let len = align_up(len);
        if addr < PAGE || addr.checked_add(len).is_none_or(|e| e > STACK_TOP - STACK_SIZE) { return -EINVAL; }
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
        if old >= STACK_TOP - STACK_SIZE && old.checked_add(old_len).is_some_and(|e| e <= STACK_TOP)
            && flags == 0 && new_len > old_len && Self::mapped_pages(mmu, old, old_len) * PAGE == old_len {
            return -ENOMEM;
        }
        if old < MMAP_BASE || old.checked_add(old_len).is_none_or(|e| e > STACK_TOP - STACK_SIZE) || Self::mapped_pages(mmu, old, old_len) * PAGE != old_len {
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
        if flags & MREMAP_MAYMOVE == 0 || new_len > MAX_MMAP_CALL || self.mmap_total - old_len + new_len > MAX_MMAP_TOTAL {
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
        mmu.map_range(base, new_len as usize, Permission { read: true, write: true, execute: false });
        if mmu.write_bytes(base, &data).is_err() {
            return -EFAULT;
        }
        mmu.unmap_range(old, old_len as usize);
        self.mmap_total = self.mmap_total - old_len + new_len;
        base as i64
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

/// The guest sees one CPU. Like the kernel, the mask is one `unsigned long` (8 bytes,
/// CPU 0 set), `len` must hold it and be a multiple of 8, and the return value is the
/// number of bytes written. musl's `sysconf(_SC_NPROCESSORS_ONLN)` asks this at startup.
fn sys_sched_getaffinity(pid: u64, len: u64, mask: u64, mmu: &mut Memory) -> i64 {
    if pid != 0 && pid != PID {
        return -ESRCH;
    }
    if len < 8 || len % 8 != 0 {
        return -EINVAL;
    }
    if mmu.write_bytes(mask, &1u64.to_le_bytes()).is_err() {
        -EFAULT
    } else {
        8
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

fn protection(prot: u64) -> Permission {
    Permission { read: prot & (PROT_READ | PROT_WRITE | PROT_EXEC) != 0, write: prot & PROT_WRITE != 0, execute: prot & PROT_EXEC != 0 }
}
