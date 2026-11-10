//! The host address range guest memory lives in. Guest address == host address, so guest code can
//! dereference its own pointers. One `mmap(PROT_NONE)` reservation is made up front; guest mappings
//! are committed inside it with `MAP_FIXED`, so nothing else in the process is ever overwritten.
//!
//! Host pages may be larger than Horizon's 4 KiB (16 KiB on Apple Silicon). Commit and protection
//! work on whole host pages; the caller (ounce-core's `Memory`) keeps the 4 KiB guest permissions.
//!
//! W^X: a host page is read+write, or read+execute once [`Arena::seal_exec`] made it guest code.
//! `MAP_JIT` cannot be combined with `MAP_FIXED`, and its per-thread write toggle would make guest
//! data read-only while guest code runs, so code pages are sealed with `mprotect` instead. A host
//! write to a sealed page (the loader, the patcher, a launch slot) unseals, writes, reseals and
//! synchronises the instruction cache.

use crate::NceError;

#[cfg(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux")))]
mod imp {
    use super::NceError;
    use std::collections::HashSet;

    fn os_error(call: &'static str) -> NceError {
        NceError::Os(call, std::io::Error::last_os_error().raw_os_error().unwrap_or(0))
    }

    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn sys_icache_invalidate(start: *mut libc::c_void, len: libc::size_t);
    }

    /// Makes instructions written to `[addr, addr + len)` visible to instruction fetch.
    fn sync_icache(addr: u64, len: u64) {
        if len == 0 {
            return;
        }
        #[cfg(target_os = "macos")]
        // SAFETY: the range is mapped (callers pass committed arena pages); the call only performs
        // cache maintenance on it.
        unsafe {
            sys_icache_invalidate(addr as *mut libc::c_void, len as libc::size_t)
        };
        #[cfg(target_os = "linux")]
        {
            let ctr: u64;
            // SAFETY: CTR_EL0 is readable at EL0 on Linux (the kernel emulates it if needed).
            unsafe { core::arch::asm!("mrs {}, ctr_el0", out(reg) ctr) };
            let dline = 4u64 << ((ctr >> 16) & 0xF);
            let iline = 4u64 << (ctr & 0xF);
            let end = addr + len;
            let mut p = addr & !(dline - 1);
            while p < end {
                // SAFETY: cache maintenance by VA on mapped memory.
                unsafe { core::arch::asm!("dc cvau, {}", in(reg) p) };
                p += dline;
            }
            // SAFETY: barrier.
            unsafe { core::arch::asm!("dsb ish") };
            let mut p = addr & !(iline - 1);
            while p < end {
                // SAFETY: cache maintenance by VA on mapped memory.
                unsafe { core::arch::asm!("ic ivau, {}", in(reg) p) };
                p += iline;
            }
            // SAFETY: barriers.
            unsafe { core::arch::asm!("dsb ish", "isb") };
        }
    }

    pub struct Arena {
        base: u64,
        size: u64,
        page: u64,
        committed: HashSet<u64>,
        sealed: HashSet<u64>,
    }

    impl std::fmt::Debug for Arena {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Arena({:#x}+{:#x}, {} committed host pages, {} sealed)", self.base, self.size,
                self.committed.len(), self.sealed.len())
        }
    }

    // The arena only holds addresses; the mapping it owns is not tied to a thread.
    // SAFETY: no thread-affine state; all access goes through `&self` / `&mut self`.
    unsafe impl Send for Arena {}

    impl Arena {
        /// Reserves `size` bytes (rounded up to host pages) aligned to 4 GiB, inaccessible until committed.
        pub fn reserve(size: u64) -> Result<Self, NceError> {
            // SAFETY: sysconf has no preconditions.
            let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
            let size = size.div_ceil(page) * page;
            const ALIGN: u64 = 1 << 32;
            let span = size + ALIGN;
            // A high hint keeps the window clear of the executable, the dyld shared cache and malloc.
            let hint = 0x1000_0000_0000u64;
            // SAFETY: a fresh anonymous PROT_NONE mapping; no existing memory is touched (no MAP_FIXED).
            let raw = unsafe {
                libc::mmap(hint as *mut libc::c_void, span as libc::size_t, libc::PROT_NONE,
                    libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_NORESERVE, -1, 0)
            };
            if raw == libc::MAP_FAILED {
                return Err(os_error("mmap(reserve)"));
            }
            let raw = raw as u64;
            let base = (raw + ALIGN - 1) & !(ALIGN - 1);
            // SAFETY: both trimmed pieces lie inside the mapping made above.
            unsafe {
                if base > raw {
                    libc::munmap(raw as *mut libc::c_void, (base - raw) as libc::size_t);
                }
                let tail = raw + span - (base + size);
                if tail > 0 {
                    libc::munmap((base + size) as *mut libc::c_void, tail as libc::size_t);
                }
            }
            Ok(Self { base, size, page, committed: HashSet::new(), sealed: HashSet::new() })
        }

        /// Reserves exactly `[base, base + size)` (host-page aligned), or fails if any of it is taken.
        pub fn reserve_at(base: u64, size: u64) -> Result<Self, NceError> {
            // SAFETY: sysconf has no preconditions.
            let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
            if base % page != 0 || size % page != 0 {
                return Err(NceError::Range { va: base, len: size });
            }
            // SAFETY: a fresh anonymous PROT_NONE mapping at a hint; no MAP_FIXED, so nothing is replaced.
            let raw = unsafe {
                libc::mmap(base as *mut libc::c_void, size as libc::size_t, libc::PROT_NONE,
                    libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_NORESERVE, -1, 0)
            };
            if raw == libc::MAP_FAILED {
                return Err(os_error("mmap(reserve)"));
            }
            if raw as u64 != base {
                // SAFETY: unmaps the mapping just made, elsewhere than asked.
                unsafe { libc::munmap(raw, size as libc::size_t) };
                return Err(NceError::Range { va: base, len: size });
            }
            Ok(Self { base, size, page, committed: HashSet::new(), sealed: HashSet::new() })
        }

        pub fn base(&self) -> u64 { self.base }
        pub fn size(&self) -> u64 { self.size }
        pub fn host_page_size(&self) -> u64 { self.page }

        fn check(&self, va: u64, len: u64) -> Result<(), NceError> {
            match va.checked_add(len) {
                Some(end) if va >= self.base && end <= self.base + self.size => Ok(()),
                _ => Err(NceError::Range { va, len }),
            }
        }

        fn pages(&self, va: u64, len: u64) -> std::ops::Range<u64> {
            if len == 0 {
                return 0..0;
            }
            (va / self.page)..((va + len - 1) / self.page + 1)
        }

        fn committed_range(&self, va: u64, len: u64) -> Result<(), NceError> {
            self.check(va, len)?;
            if self.pages(va, len).all(|p| self.committed.contains(&p)) {
                Ok(())
            } else {
                Err(NceError::Range { va, len })
            }
        }

        /// Fresh zero pages at `[start, end)` (host-page aligned, inside the arena).
        fn fresh(&mut self, start: u64, end: u64, prot: libc::c_int) -> Result<(), NceError> {
            let flags = libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_FIXED
                | if prot == libc::PROT_NONE { libc::MAP_NORESERVE } else { 0 };
            // SAFETY: `[start, end)` lies inside this arena's reservation (checked by callers), so
            // MAP_FIXED replaces only memory this arena owns.
            let p = unsafe { libc::mmap(start as *mut libc::c_void, (end - start) as libc::size_t, prot, flags, -1, 0) };
            if p == libc::MAP_FAILED || p as u64 != start {
                return Err(os_error("mmap(fixed)"));
            }
            Ok(())
        }

        fn protect(&self, start: u64, end: u64, prot: libc::c_int) -> Result<(), NceError> {
            // SAFETY: `[start, end)` is host-page aligned inside the reservation.
            if unsafe { libc::mprotect(start as *mut libc::c_void, (end - start) as libc::size_t, prot) } != 0 {
                return Err(os_error("mprotect"));
            }
            Ok(())
        }

        /// Makes `[va, va + len)` accessible and zero, like a fresh Horizon mapping. Host pages only
        /// partly covered keep their other bytes.
        pub fn commit(&mut self, va: u64, len: u64) -> Result<(), NceError> {
            self.check(va, len)?;
            let end = va + len;
            let pages = self.pages(va, len);
            let mut run: Option<(u64, u64)> = None;
            for p in pages {
                let (ps, pe) = (p * self.page, (p + 1) * self.page);
                let whole = ps >= va && pe <= end;
                if whole || !self.committed.contains(&p) {
                    run = match run {
                        Some((s, e)) if e == ps => Some((s, pe)),
                        Some((s, e)) => { self.fresh(s, e, libc::PROT_READ | libc::PROT_WRITE)?; Some((ps, pe)) }
                        None => Some((ps, pe)),
                    };
                    self.committed.insert(p);
                    self.sealed.remove(&p);
                } else {
                    if self.sealed.remove(&p) {
                        self.protect(ps, pe, libc::PROT_READ | libc::PROT_WRITE)?;
                    }
                    let (zs, ze) = (va.max(ps), end.min(pe));
                    // SAFETY: committed read+write page; the range lies within it.
                    unsafe { std::ptr::write_bytes(zs as *mut u8, 0, (ze - zs) as usize) };
                }
            }
            if let Some((s, e)) = run {
                self.fresh(s, e, libc::PROT_READ | libc::PROT_WRITE)?;
            }
            Ok(())
        }

        /// Releases the host pages wholly inside `[va, va + len)`. Partly covered ones stay committed.
        pub fn decommit(&mut self, va: u64, len: u64) -> Result<(), NceError> {
            self.check(va, len)?;
            let end = va + len;
            for p in self.pages(va, len) {
                let (ps, pe) = (p * self.page, (p + 1) * self.page);
                if ps >= va && pe <= end && self.committed.remove(&p) {
                    self.sealed.remove(&p);
                    self.fresh(ps, pe, libc::PROT_NONE)?;
                }
            }
            Ok(())
        }

        pub fn read(&self, va: u64, buf: &mut [u8]) -> Result<(), NceError> {
            self.committed_range(va, buf.len() as u64)?;
            // SAFETY: the source range is committed (readable) arena memory; no guest code runs
            // while the host holds `&self`.
            unsafe { std::ptr::copy_nonoverlapping(va as *const u8, buf.as_mut_ptr(), buf.len()) };
            Ok(())
        }

        pub fn write(&mut self, va: u64, data: &[u8]) -> Result<(), NceError> {
            let len = data.len() as u64;
            self.committed_range(va, len)?;
            let sealed: Vec<u64> = self.pages(va, len).filter(|p| self.sealed.contains(p)).collect();
            for &p in &sealed {
                self.protect(p * self.page, (p + 1) * self.page, libc::PROT_READ | libc::PROT_WRITE)?;
            }
            // SAFETY: the destination is committed and now writable; no guest code runs while the
            // host holds `&mut self`.
            unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), va as *mut u8, data.len()) };
            for &p in &sealed {
                self.protect(p * self.page, (p + 1) * self.page, libc::PROT_READ | libc::PROT_EXEC)?;
            }
            if !sealed.is_empty() {
                sync_icache(va, len);
            }
            Ok(())
        }

        /// Seals the host pages covering `[va, va + len)` read+execute.
        pub fn seal_exec(&mut self, va: u64, len: u64) -> Result<(), NceError> {
            self.committed_range(va, len)?;
            for p in self.pages(va, len) {
                if self.sealed.insert(p) {
                    self.protect(p * self.page, (p + 1) * self.page, libc::PROT_READ | libc::PROT_EXEC)?;
                    sync_icache(p * self.page, self.page);
                }
            }
            Ok(())
        }

        pub fn is_sealed(&self, va: u64) -> bool {
            self.sealed.contains(&(va / self.page))
        }

        /// The [`crate::Context`] at `va` (committed, writable, 16-byte aligned), for the host to edit.
        pub fn context_mut(&mut self, va: u64) -> Result<&mut crate::Context, NceError> {
            let len = std::mem::size_of::<crate::Context>() as u64;
            self.committed_range(va, len)?;
            if !va.is_multiple_of(16) || self.pages(va, len).any(|p| self.sealed.contains(&p)) {
                return Err(NceError::Range { va, len });
            }
            // SAFETY: committed, writable, aligned, inside the reservation, and borrowed through
            // `&mut self`, so no other host reference to it exists and guest code is not running.
            Ok(unsafe { &mut *(va as *mut crate::Context) })
        }
    }

    impl Drop for Arena {
        fn drop(&mut self) {
            // SAFETY: unmaps exactly the reservation this arena made.
            unsafe { libc::munmap(self.base as *mut libc::c_void, self.size as libc::size_t) };
        }
    }
}

#[cfg(not(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux"))))]
mod imp {
    use super::NceError;

    /// Stub: native execution is not available on this host.
    #[derive(Debug)]
    pub struct Arena(());

    impl Arena {
        pub fn reserve(_size: u64) -> Result<Self, NceError> { Err(NceError::Unsupported) }
        pub fn reserve_at(_base: u64, _size: u64) -> Result<Self, NceError> { Err(NceError::Unsupported) }
        pub fn base(&self) -> u64 { 0 }
        pub fn size(&self) -> u64 { 0 }
        pub fn host_page_size(&self) -> u64 { 0x1000 }
        pub fn commit(&mut self, va: u64, len: u64) -> Result<(), NceError> { Err(NceError::Range { va, len }) }
        pub fn decommit(&mut self, _va: u64, _len: u64) -> Result<(), NceError> { Ok(()) }
        pub fn read(&self, va: u64, buf: &mut [u8]) -> Result<(), NceError> { Err(NceError::Range { va, len: buf.len() as u64 }) }
        pub fn write(&mut self, va: u64, data: &[u8]) -> Result<(), NceError> { Err(NceError::Range { va, len: data.len() as u64 }) }
        pub fn seal_exec(&mut self, va: u64, len: u64) -> Result<(), NceError> { Err(NceError::Range { va, len }) }
        pub fn is_sealed(&self, _va: u64) -> bool { false }
        pub fn context_mut(&mut self, va: u64) -> Result<&mut crate::Context, NceError> { Err(NceError::Range { va, len: 0 }) }
    }
}

pub use imp::Arena;

#[cfg(all(test, target_arch = "aarch64", any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn commit_read_write_decommit_and_seal() {
        // reserve_at: a spot known to be free (just released), and one that is taken.
        // Other tests run on parallel threads of this process and can claim the address between the release
        // and `reserve_at`, so retry with a fresh address instead of failing on that race.
        let (free, fixed) = (0..16)
            .find_map(|_| {
                let free = Arena::reserve(1 << 20).unwrap().base();
                Arena::reserve_at(free, 1 << 20).ok().map(|fixed| (free, fixed))
            })
            .expect("a released address could not be reserved again in 16 tries");
        assert_eq!(fixed.base(), free);
        assert!(Arena::reserve_at(free, 1 << 20).is_err(), "taken by `fixed`");
        let mut a = Arena::reserve(1 << 30).unwrap();
        let b = a.base();
        assert_eq!(b % (1 << 32), 0);
        assert!(a.read(b, &mut [0; 4]).is_err(), "uncommitted");
        a.commit(b + 0x1000, 0x2000).unwrap();
        a.write(b + 0x1000, &[1, 2, 3, 4]).unwrap();
        let mut got = [0; 4];
        a.read(b + 0x1000, &mut got).unwrap();
        assert_eq!(got, [1, 2, 3, 4]);
        // Recommitting zeroes the guest range, also inside a partly covered host page.
        a.commit(b + 0x1000, 0x1000).unwrap();
        a.read(b + 0x1000, &mut got).unwrap();
        assert_eq!(got, [0; 4]);
        // Seal a committed page holding guest data; a later host write still lands.
        a.seal_exec(b + 0x1000, 0x1000).unwrap();
        assert!(a.is_sealed(b + 0x1000));
        a.write(b + 0x1000, &[9]).unwrap();
        a.read(b + 0x1000, &mut got[..1]).unwrap();
        assert_eq!(got[0], 9);
        assert!(a.write(b - 1, &[0]).is_err() && a.write(b + a.size(), &[0]).is_err());
        let page = (b + 0x1000) & !(a.host_page_size() - 1);
        a.decommit(page, a.host_page_size()).unwrap();
        assert!(a.read(b + 0x1000, &mut got).is_err());
    }
}
