#![allow(dead_code)]

use byteorder::{ByteOrder, LittleEndian};
use std::collections::HashMap;

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_MASK: u64 = !(PAGE_SIZE as u64 - 1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permission {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Permission {
    pub const NONE: Self = Self { read: false, write: false, execute: false };
    pub const READ_ONLY: Self = Self { read: true, write: false, execute: false };
    pub const READ_WRITE: Self = Self { read: true, write: true, execute: false };
    pub const READ_EXECUTE: Self = Self { read: true, write: false, execute: true };
    pub const READ_WRITE_EXECUTE: Self = Self { read: true, write: true, execute: true };
}

#[derive(Debug, Clone)]
pub struct Page {
    pub data: Box<[u8; PAGE_SIZE]>,
    pub perm: Permission,
    /// Set when a block cache has compiled instructions from this page.
    pub holds_code: bool,
    /// Value of `Memory::code_write_gen` at the last write to this page while it held code.
    pub code_write_gen: u64,
    /// Guarded page (the stage-1 `GP` bit, FEAT_BTI): indirect branches into it need a landing pad.
    pub guarded: bool,
}

impl Page {
    pub fn new(perm: Permission) -> Self {
        Self {
            data: Box::new([0u8; PAGE_SIZE]),
            perm,
            holds_code: false,
            code_write_gen: 0,
            guarded: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Unmapped memory access at virtual address: 0x{addr:016X}")]
    Unmapped { addr: u64 },

    #[error("Permission denied at 0x{addr:016X}: required {required:?}, actual {actual:?}")]
    PermissionDenied {
        addr: u64,
        required: &'static str,
        actual: Permission,
    },

    #[error("Misaligned memory access at 0x{addr:016X} (size: {size} bytes)")]
    Misaligned { addr: u64, size: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemoryBucketStats {
    pub total_pages: usize,
    pub total_bytes: usize,
    pub code_bytes: usize,
    pub vram_bytes: usize,
    pub stack_tls_bytes: usize,
    pub heap_data_bytes: usize,
}

/// Virtual Memory Manager (VMM) supporting 64-bit sparse page tables.
#[derive(Debug, Default)]
pub struct Memory {
    /// Maps 4KB-aligned virtual page numbers (VA >> 12) to allocated pages
    pages: HashMap<u64, Page>,
    /// Number of mapped pages carrying the FEAT_BTI guarded bit.
    guarded_pages: usize,
    /// Bumped on writes and mapping changes that affect pages holding cached code.
    /// Block caches compare it against the value they last saw to detect self-modifying code.
    code_write_gen: u64,
    /// Last invalidation generation for unmapped pages that held cached code. Retained until a
    /// replacement mapping is installed so unmap+remap between cache sweeps remains observable.
    unmapped_code_gen: HashMap<u64, u64>,
    /// Bumped whenever executable pages are mapped (native execution rewrites new code then).
    exec_map_gen: u64,
    /// Native execution (`--cpu=native`): page bytes live in this host window at guest address ==
    /// host address, where guest code running on the host CPU sees them too. `pages` then holds the
    /// mapping and permissions only.
    #[cfg(feature = "nce")]
    arena: Option<ounce_nce::Arena>,
}

impl Memory {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            guarded_pages: 0,
            code_write_gen: 0,
            unmapped_code_gen: HashMap::new(),
            exec_map_gen: 0,
            #[cfg(feature = "nce")]
            arena: None,
        }
    }

    /// Memory whose page bytes live in `arena` (see the `arena` field).
    #[cfg(feature = "nce")]
    pub fn with_arena(arena: ounce_nce::Arena) -> Self {
        Self { arena: Some(arena), ..Self::new() }
    }

    #[cfg(feature = "nce")]
    pub fn arena(&self) -> Option<&ounce_nce::Arena> {
        self.arena.as_ref()
    }

    #[cfg(feature = "nce")]
    pub fn arena_mut(&mut self) -> Option<&mut ounce_nce::Arena> {
        self.arena.as_mut()
    }

    /// Changes whenever executable pages are mapped.
    pub fn exec_map_gen(&self) -> u64 {
        self.exec_map_gen
    }

    /// Mapped pages `(start, end, perm)` merged into runs of equal permission, in address order.
    pub fn regions(&self) -> Vec<(u64, u64, Permission)> {
        let mut keys: Vec<u64> = self.pages.keys().copied().collect();
        keys.sort_unstable();
        let mut out: Vec<(u64, u64, Permission)> = Vec::new();
        for k in keys {
            let perm = self.pages[&k].perm;
            match out.last_mut() {
                Some(last) if last.1 == k << 12 && last.2 == perm => last.1 = (k + 1) << 12,
                _ => out.push((k << 12, (k + 1) << 12, perm)),
            }
        }
        out
    }

    /// Page bytes from the arena when there is one; `None` without an arena (use `Page::data`).
    #[inline]
    fn arena_read(&self, addr: u64, buf: &mut [u8]) -> Option<Result<(), MemoryError>> {
        #[cfg(feature = "nce")]
        if let Some(arena) = &self.arena {
            return Some(arena.read(addr, buf).map_err(|_| MemoryError::Unmapped { addr }));
        }
        let _ = (addr, buf);
        None
    }

    #[inline]
    fn arena_write(&mut self, addr: u64, data: &[u8]) -> Option<Result<(), MemoryError>> {
        #[cfg(feature = "nce")]
        if let Some(arena) = &mut self.arena {
            return Some(arena.write(addr, data).map_err(|_| MemoryError::Unmapped { addr }));
        }
        let _ = (addr, data);
        None
    }

    /// Current code generation. Unchanged means cached code mappings were not written or changed.
    #[inline]
    pub fn code_write_gen(&self) -> u64 {
        self.code_write_gen
    }

    /// Flags every mapped page in `[start, end)` as holding cached code, so later writes to it
    /// bump `code_write_gen`.
    pub fn mark_code_range(&mut self, start: u64, end: u64) {
        if end <= start {
            return;
        }
        for page_idx in (start >> 12)..=((end - 1) >> 12) {
            if let Some(page) = self.pages.get_mut(&page_idx) {
                page.holds_code = true;
            }
        }
    }

    /// True if any page in `[start, end)` was written, unmapped, or remapped after `since`.
    pub fn code_written_since(&self, start: u64, end: u64, since: u64) -> bool {
        if end <= start {
            return false;
        }
        ((start >> 12)..=((end - 1) >> 12)).any(|page_idx| match self.pages.get(&page_idx) {
            Some(page) => page.code_write_gen > since,
            // Unmapped since compile: the cached copy is stale too.
            None => true,
        })
    }

    /// Computes live memory allocation telemetry categorized by Horizon OS virtual address buckets.
    pub fn bucket_stats(&self) -> MemoryBucketStats {
        let mut stats = MemoryBucketStats {
            total_pages: self.pages.len(),
            total_bytes: self.pages.len() * PAGE_SIZE,
            ..Default::default()
        };
        for (&page_idx, page) in &self.pages {
            let va = page_idx << 12;
            if (0x9000_0000..0xA000_0000).contains(&va) {
                stats.vram_bytes += PAGE_SIZE;
            } else if va >= 0x7FFF_0000_00 {
                stats.stack_tls_bytes += PAGE_SIZE;
            } else if page.perm.execute {
                stats.code_bytes += PAGE_SIZE;
            } else {
                stats.heap_data_bytes += PAGE_SIZE;
            }
        }
        stats
    }

    /// Allocate every page intersecting a contiguous virtual memory range.
    /// An empty range leaves existing mappings unchanged.
    ///
    /// # Panics
    /// Panics before changing any mappings if the range exceeds the 64-bit address space.
    pub fn map_range(&mut self, base_addr: u64, size: usize, perm: Permission) {
        if size == 0 {
            return;
        }

        let last_addr = base_addr
            .checked_add((size - 1) as u64)
            .expect("memory range exceeds the 64-bit address space");
        let start_page = base_addr >> 12;
        let last_page = last_addr >> 12;
        if perm.execute {
            self.exec_map_gen += 1;
        }

        for page_idx in start_page..=last_page {
            if self.pages.get(&page_idx).is_some_and(|old| old.guarded) {
                self.guarded_pages -= 1;
            }
            let mut page = Page::new(perm);
            if self.pages.get(&page_idx).is_some_and(|old| old.holds_code) {
                // Remapping over cached code replaces it.
                self.code_write_gen += 1;
                page.code_write_gen = self.code_write_gen;
            } else if let Some(generation) = self.unmapped_code_gen.remove(&page_idx) {
                // Preserve an unmap invalidation when the page is remapped before a JIT cache
                // gets a chance to observe that the address was unmapped.
                page.code_write_gen = generation;
            }
            self.pages.insert(page_idx, page);
        }
        #[cfg(feature = "nce")]
        if let Some(arena) = &mut self.arena {
            let (start, end) = (start_page << 12, (last_page + 1) << 12);
            if let Err(error) = arena.commit(start, end - start) {
                eprintln!("[NCE] guest mapping {start:#x}..{end:#x} has no host memory: {error}");
            }
        }
    }

    /// Copy software-backed memory for a fault probe without changing the live guest.
    /// Native arenas cannot be copied this way.
    pub(crate) fn fault_snapshot(&self) -> Option<Self> {
        #[cfg(feature = "nce")]
        if self.arena.is_some() { return None; }
        Some(Self {
            pages: self.pages.clone(),
            guarded_pages: self.guarded_pages,
            code_write_gen: self.code_write_gen,
            unmapped_code_gen: self.unmapped_code_gen.clone(),
            exec_map_gen: self.exec_map_gen,
            ..Self::new()
        })
    }

    /// Commit missing pages and change permissions without discarding existing bytes.
    pub fn commit_range(&mut self, start: u64, size: usize, perm: Permission) {
        for addr in (start..start + size as u64).step_by(PAGE_SIZE) {
            if !self.is_mapped(addr) { self.map_range(addr, PAGE_SIZE, perm); }
        }
        self.protect_range(start, size, perm);
    }

    /// Change only existing pages; holes remain unbacked. Invalidate compiled code.
    pub fn protect_range(&mut self, start: u64, size: usize, perm: Permission) {
        let end = start.saturating_add(size as u64);
        for (&idx, page) in &mut self.pages {
            if idx << 12 >= start && idx << 12 < end {
                if page.holds_code { self.code_write_gen += 1; page.code_write_gen = self.code_write_gen; }
                page.perm = perm;
            }
        }
        if perm.execute { self.exec_map_gen += 1; }
    }

    /// Checks if a virtual address is mapped.
    /// Moves the pages of `src..src + size` to `dst` (both page aligned): the data appears at `dst` and
    /// `src` becomes unmapped. Models svcMapMemory, after which Horizon makes the source inaccessible.
    /// Unmapped source pages leave the matching destination pages unmapped.
    pub fn move_range(&mut self, src: u64, dst: u64, size: usize) {
        let pages = (size as u64).div_ceil(PAGE_SIZE as u64);
        for i in 0..pages {
            let src_page_idx = (src >> 12) + i;
            let dst_page_idx = (dst >> 12) + i;
            #[cfg(feature = "nce")]
            if let Some(arena) = &mut self.arena
                && self.pages.contains_key(&src_page_idx)
            {
                let mut bytes = vec![0u8; PAGE_SIZE];
                let (from, to) = (src_page_idx << 12, dst_page_idx << 12);
                let moved = arena.read(from, &mut bytes)
                    .and_then(|_| arena.commit(to, PAGE_SIZE as u64))
                    .and_then(|_| arena.write(to, &bytes));
                if let Err(error) = moved {
                    eprintln!("[NCE] moving guest page {from:#x} -> {to:#x} failed: {error}");
                }
            }
            if let Some(mut page) = self.pages.remove(&src_page_idx) {
                let moved_guarded = page.guarded;
                if moved_guarded {
                    self.guarded_pages -= 1;
                }

                // The translation embeds its original guest PC, so moving a code page invalidates
                // the old source address even though the bytes themselves did not change.
                if page.holds_code {
                    self.code_write_gen += 1;
                    self.unmapped_code_gen.insert(src_page_idx, self.code_write_gen);
                }

                let replaced_code = self.pages.remove(&dst_page_idx).is_some_and(|old| {
                    if old.guarded {
                        self.guarded_pages -= 1;
                    }
                    old.holds_code
                });
                let prior_unmapped_generation = self.unmapped_code_gen.remove(&dst_page_idx);
                if replaced_code {
                    self.code_write_gen += 1;
                    page.code_write_gen = self.code_write_gen;
                } else if let Some(generation) = prior_unmapped_generation {
                    page.code_write_gen = page.code_write_gen.max(generation);
                }

                self.pages.insert(dst_page_idx, page);
                if moved_guarded {
                    self.guarded_pages += 1;
                }
            }
        }
        #[cfg(feature = "nce")]
        if let Some(arena) = &mut self.arena {
            let _ = arena.decommit(src, pages * PAGE_SIZE as u64);
        }
    }

    /// Unmaps every page that overlaps `base_addr..base_addr + size`. Unmapped pages are skipped.
    pub fn unmap_range(&mut self, base_addr: u64, size: usize) {
        if size == 0 {
            return;
        }
        let last = base_addr.saturating_add(size as u64 - 1) >> 12;
        let first = base_addr >> 12;
        // A range wider than the address space holds pages visits the mapped pages, not every index.
        let doomed: Vec<u64> = if last - first >= self.pages.len() as u64 {
            self.pages.keys().copied().filter(|i| (first..=last).contains(i)).collect()
        } else {
            (first..=last).collect()
        };
        for page_idx in doomed {
            if let Some(page) = self.pages.remove(&page_idx) {
                if page.guarded {
                    self.guarded_pages -= 1;
                }
                if page.holds_code {
                    self.code_write_gen += 1;
                    self.unmapped_code_gen.insert(page_idx, self.code_write_gen);
                }
            }
        }
        #[cfg(feature = "nce")]
        if let Some(arena) = &mut self.arena {
            let start = base_addr & PAGE_MASK;
            let _ = arena.decommit(start, ((last + 1) << 12) - start);
        }
    }

    /// The run of pages around `addr` that are all mapped or all unmapped, as `(start, end, mapped)`.
    /// An unmapped run is bounded by the nearest mapped pages, or by the 48-bit address space.
    pub fn region_at(&self, addr: u64) -> (u64, u64, bool) {
        let page = addr >> 12;
        if self.pages.contains_key(&page) {
            let mut lo = page;
            while lo > 0 && self.pages.contains_key(&(lo - 1)) {
                lo -= 1;
            }
            let mut hi = page + 1;
            while self.pages.contains_key(&hi) {
                hi += 1;
            }
            (lo << 12, hi << 12, true)
        } else {
            let below = self.pages.keys().filter(|&&p| p < page).max().map_or(0, |p| p + 1);
            let above = self.pages.keys().filter(|&&p| p > page).min().map_or(1u64 << 36, |&p| p);
            (below << 12, above << 12, false)
        }
    }

    pub fn is_mapped(&self, addr: u64) -> bool {
        self.pages.contains_key(&(addr >> 12))
    }

    /// Sets or clears the guarded-page flag (FEAT_BTI) on every mapped page in `[start, start + size)`.
    /// Nothing sets it by default; remapping a page clears it.
    pub fn set_guarded(&mut self, start: u64, size: u64, guarded: bool) {
        if size == 0 {
            return;
        }
        let end = start.saturating_add(size - 1);
        for page_idx in (start >> 12)..=(end >> 12) {
            if let Some(page) = self.pages.get_mut(&page_idx) {
                if page.guarded != guarded {
                    if guarded {
                        self.guarded_pages += 1;
                    } else {
                        self.guarded_pages -= 1;
                    }
                }
                page.guarded = guarded;
            }
        }
    }

    /// True if any mapped page requires FEAT_BTI landing-pad checks.
    #[inline]
    pub fn has_guarded_pages(&self) -> bool {
        self.guarded_pages != 0
    }

    /// True if `addr` is in a mapped guarded page.
    #[inline]
    pub fn is_guarded(&self, addr: u64) -> bool {
        self.pages.get(&(addr >> 12)).is_some_and(|p| p.guarded)
    }

    /// Borrows actual page storage for one translated access. Arena-backed memory must use
    /// the ordinary accessors: its Page::data is metadata storage, not the guest bytes.
    pub(crate) fn direct_page(&mut self, page_num: u64) -> Option<&mut Page> {
        #[cfg(feature = "nce")]
        if self.arena.is_some() { return None; }
        self.pages.get_mut(&page_num)
    }

    /// Access the Page for a page number (VA >> 12), if mapped.
    pub fn get_page(&self, page_num: u64) -> Option<&Page> {
        self.pages.get(&page_num)
    }

    /// Read raw byte slice from virtual memory.
    #[inline]
    pub fn read_bytes(&self, addr: u64, buf: &mut [u8]) -> Result<(), MemoryError> {
        if !buf.is_empty() && addr.checked_add((buf.len() - 1) as u64).is_none() {
            return Err(MemoryError::Unmapped { addr });
        }
        let mut curr_addr = addr;
        let mut offset = 0;

        while offset < buf.len() {
            let page_idx = curr_addr >> 12;
            let page_offset = (curr_addr as usize) & (PAGE_SIZE - 1);
            let bytes_in_page = (PAGE_SIZE - page_offset).min(buf.len() - offset);

            let page = self.pages.get(&page_idx).ok_or(MemoryError::Unmapped {
                addr: curr_addr,
            })?;

            if !page.perm.read {
                return Err(MemoryError::PermissionDenied {
                    addr: curr_addr,
                    required: "READ",
                    actual: page.perm,
                });
            }

            match self.arena_read(curr_addr, &mut buf[offset..offset + bytes_in_page]) {
                Some(result) => result?,
                None => buf[offset..offset + bytes_in_page]
                    .copy_from_slice(&page.data[page_offset..page_offset + bytes_in_page]),
            }

            offset += bytes_in_page;
            if offset < buf.len() { curr_addr += bytes_in_page as u64; }
        }

        Ok(())
    }

    /// Write raw byte slice into virtual memory.
    #[inline]
    pub fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), MemoryError> {
        if !data.is_empty() && addr.checked_add((data.len() - 1) as u64).is_none() {
            return Err(MemoryError::Unmapped { addr });
        }
        // A faulting cross-page guest store must not modify its accessible prefix.
        // Keep the common single-page path to one page-table lookup.
        if !data.is_empty() && addr >> 12 != (addr + (data.len() - 1) as u64) >> 12 {
            let mut next = addr;
            let end = addr + (data.len() - 1) as u64;
            loop {
                let page = self.pages.get(&(next >> 12)).ok_or(MemoryError::Unmapped { addr: next })?;
                if !page.perm.write {
                    return Err(MemoryError::PermissionDenied { addr: next, required: "WRITE", actual: page.perm });
                }
                if next >> 12 == end >> 12 { break; }
                next = (next | (PAGE_SIZE as u64 - 1)) + 1;
            }
        }
        let mut curr_addr = addr;
        let mut offset = 0;

        while offset < data.len() {
            let page_idx = curr_addr >> 12;
            let page_offset = (curr_addr as usize) & (PAGE_SIZE - 1);
            let bytes_in_page = (PAGE_SIZE - page_offset).min(data.len() - offset);

            let page = self.pages.get_mut(&page_idx).ok_or(MemoryError::Unmapped {
                addr: curr_addr,
            })?;

            if !page.perm.write {
                return Err(MemoryError::PermissionDenied {
                    addr: curr_addr,
                    required: "WRITE",
                    actual: page.perm,
                });
            }

            // Borrow the disjoint arena field directly so permission checking, copying and
            // code invalidation share one page-table lookup, including for ordinary RAM.
            #[cfg(feature = "nce")]
            if let Some(arena) = &mut self.arena {
                arena.write(curr_addr, &data[offset..offset + bytes_in_page])
                    .map_err(|_| MemoryError::Unmapped { addr: curr_addr })?;
            }
            #[cfg(feature = "nce")]
            let in_arena = self.arena.is_some();
            #[cfg(not(feature = "nce"))]
            let in_arena = false;
            if !in_arena {
                page.data[page_offset..page_offset + bytes_in_page]
                    .copy_from_slice(&data[offset..offset + bytes_in_page]);
            }
            if page.holds_code {
                self.code_write_gen += 1;
                page.code_write_gen = self.code_write_gen;
            }

            offset += bytes_in_page;
            if offset < data.len() { curr_addr += bytes_in_page as u64; }
        }

        Ok(())
    }

    // Scalar reads (Little Endian)
    #[inline]
    pub fn read_u8(&self, addr: u64) -> Result<u8, MemoryError> {
        let mut buf = [0u8; 1];
        self.read_bytes(addr, &mut buf)?;
        Ok(buf[0])
    }

    #[inline]
    pub fn read_u16(&self, addr: u64) -> Result<u16, MemoryError> {
        let mut buf = [0u8; 2];
        self.read_bytes(addr, &mut buf)?;
        Ok(LittleEndian::read_u16(&buf))
    }

    #[inline]
    pub fn read_u32(&self, addr: u64) -> Result<u32, MemoryError> {
        let mut buf = [0u8; 4];
        self.read_bytes(addr, &mut buf)?;
        Ok(LittleEndian::read_u32(&buf))
    }

    #[inline]
    pub fn read_u64(&self, addr: u64) -> Result<u64, MemoryError> {
        let mut buf = [0u8; 8];
        self.read_bytes(addr, &mut buf)?;
        Ok(LittleEndian::read_u64(&buf))
    }

    // Scalar writes (Little Endian)
    #[inline]
    pub fn write_u8(&mut self, addr: u64, val: u8) -> Result<(), MemoryError> {
        self.write_bytes(addr, &[val])
    }

    #[inline]
    pub fn write_u16(&mut self, addr: u64, val: u16) -> Result<(), MemoryError> {
        let mut buf = [0u8; 2];
        LittleEndian::write_u16(&mut buf, val);
        self.write_bytes(addr, &buf)
    }

    #[inline]
    pub fn write_u32(&mut self, addr: u64, val: u32) -> Result<(), MemoryError> {
        let mut buf = [0u8; 4];
        LittleEndian::write_u32(&mut buf, val);
        self.write_bytes(addr, &buf)
    }

    #[inline]
    pub fn write_u64(&mut self, addr: u64, val: u64) -> Result<(), MemoryError> {
        let mut buf = [0u8; 8];
        LittleEndian::write_u64(&mut buf, val);
        self.write_bytes(addr, &buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accesses_at_address_space_end_do_not_wrap_or_overflow() {
        let mut memory = Memory::new();
        memory.map_range(u64::MAX, 1, Permission::READ_WRITE);
        memory.write_u8(u64::MAX, 7).unwrap();
        assert_eq!(memory.read_u8(u64::MAX).unwrap(), 7);
        assert!(memory.read_bytes(u64::MAX, &mut [0; 2]).is_err());
        assert!(memory.write_bytes(u64::MAX, &[8, 9]).is_err());
        assert_eq!(memory.read_u8(u64::MAX).unwrap(), 7);
    }

    #[test]
    fn guarded_page_count_tracks_mapping_changes() {
        let mut memory = Memory::new();
        memory.map_range(0x1000, 0x3000, Permission::READ_WRITE);
        memory.set_guarded(0x1000, 0x1000, true);
        memory.set_guarded(0x3000, 0x1000, true);
        assert!(memory.has_guarded_pages());

        memory.map_range(0x1000, 0x1000, Permission::READ_WRITE);
        assert!(memory.has_guarded_pages());
        memory.move_range(0x3000, 0x4000, PAGE_SIZE);
        memory.unmap_range(0x4000, PAGE_SIZE);
        assert!(!memory.has_guarded_pages());
    }

    #[test]
    fn test_map_range_covers_exactly_the_intersecting_pages() {
        let cases = [
            (0x0000, 1, 1),
            (0x1000, PAGE_SIZE, 1),
            (0x1000, PAGE_SIZE + 1, 2),
            (0x1001, PAGE_SIZE - 1, 1),
            (0x1001, PAGE_SIZE, 2),
            (0x1FFF, 1, 1),
            (0x1FFF, 2, 2),
            (0x1FFF, PAGE_SIZE + 1, 2),
            (0x1FFF, PAGE_SIZE + 2, 3),
        ];

        for (base_addr, size, expected_pages) in cases {
            let mut memory = Memory::new();
            memory.map_range(base_addr, size, Permission::READ_WRITE);

            let first_page = base_addr >> 12;
            assert_eq!(memory.pages.len(), expected_pages);
            for page in first_page..first_page + expected_pages as u64 {
                assert!(memory.is_mapped(page * PAGE_SIZE as u64));
            }
            if first_page > 0 {
                assert!(!memory.is_mapped((first_page - 1) * PAGE_SIZE as u64));
            }
            assert!(!memory.is_mapped((first_page + expected_pages as u64) * PAGE_SIZE as u64));
        }
    }

    #[test]
    fn test_read_write_bytes_across_unaligned_mapped_pages() {
        let mut memory = Memory::new();
        let base_addr = 0x1FFF;
        let data: Vec<u8> = (0..PAGE_SIZE + 2).map(|i| i as u8).collect();
        memory.map_range(base_addr, data.len(), Permission::READ_WRITE);

        memory.write_bytes(base_addr, &data).unwrap();
        let mut actual = vec![0; data.len()];
        memory.read_bytes(base_addr, &mut actual).unwrap();
        assert_eq!(actual, data);
    }

    #[test]
    fn test_read_write_scalar_across_page_boundary() {
        let mut memory = Memory::new();
        memory.map_range(0x1FFC, 8, Permission::READ_WRITE);

        memory.write_u64(0x1FFC, 0x0123_4567_89AB_CDEF).unwrap();
        assert_eq!(memory.read_u64(0x1FFC).unwrap(), 0x0123_4567_89AB_CDEF);
        assert_eq!(memory.read_u32(0x2000).unwrap(), 0x0123_4567);
    }

    #[test]
    fn test_read_only_permissions_apply_to_every_mapped_page() {
        let mut memory = Memory::new();
        memory.map_range(0x1FFF, 2, Permission::READ_ONLY);

        let mut actual = [1; 2];
        memory.read_bytes(0x1FFF, &mut actual).unwrap();
        assert_eq!(actual, [0; 2]);

        for addr in [0x1FFF, 0x2000] {
            assert!(matches!(
                memory.write_u8(addr, 1),
                Err(MemoryError::PermissionDenied {
                    addr: denied_addr,
                    required: "WRITE",
                    actual: Permission::READ_ONLY,
                }) if denied_addr == addr
            ));
            assert_eq!(memory.read_u8(addr).unwrap(), 0);
        }
    }

    #[test]
    fn test_access_to_adjacent_unmapped_page_is_rejected() {
        let mut memory = Memory::new();
        memory.map_range(0x1FFF, 2, Permission::READ_WRITE);

        assert!(matches!(
            memory.read_u8(0x3000),
            Err(MemoryError::Unmapped { addr: 0x3000 })
        ));
        assert!(matches!(
            memory.write_u8(0x3000, 1),
            Err(MemoryError::Unmapped { addr: 0x3000 })
        ));
    }

    #[test]
    fn test_empty_range_does_not_map_or_replace_pages() {
        let mut memory = Memory::new();
        memory.map_range(0x1000, PAGE_SIZE, Permission::READ_WRITE);
        memory.write_u8(0x1FFF, 42).unwrap();

        for base_addr in [0, 0x1000, 0x1FFF, 0x2000, u64::MAX] {
            memory.map_range(base_addr, 0, Permission::NONE);
        }

        assert_eq!(memory.pages.len(), 1);
        assert_eq!(memory.read_u8(0x1FFF).unwrap(), 42);
        memory.write_u8(0x1FFF, 43).unwrap();
        assert_eq!(memory.read_u8(0x1FFF).unwrap(), 43);
    }

    #[test]
    fn test_map_range_can_end_at_the_last_address() {
        let cases = [
            (u64::MAX, 1, 1),
            (u64::MAX - PAGE_SIZE as u64 + 1, PAGE_SIZE, 1),
            (u64::MAX - PAGE_SIZE as u64, PAGE_SIZE + 1, 2),
        ];

        for (base_addr, size, expected_pages) in cases {
            let mut memory = Memory::new();
            memory.map_range(base_addr, size, Permission::READ_WRITE);

            assert_eq!(memory.pages.len(), expected_pages);
            assert!(memory.is_mapped(base_addr));
            assert!(memory.is_mapped(u64::MAX));
            assert!(!memory.is_mapped(0));
        }
    }

    #[test]
    fn test_overflowing_range_is_rejected_before_mapping() {
        let mut memory = Memory::new();
        memory.map_range(0x1000, PAGE_SIZE, Permission::READ_WRITE);
        memory.write_u8(0x1000, 42).unwrap();

        for (base_addr, size) in [
            (u64::MAX, 2),
            (u64::MAX - PAGE_SIZE as u64 + 1, PAGE_SIZE + 1),
        ] {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                memory.map_range(base_addr, size, Permission::READ_WRITE);
            }));

            assert!(result.is_err());
            assert_eq!(memory.pages.len(), 1);
            assert_eq!(memory.read_u8(0x1000).unwrap(), 42);
        }
    }
}

#[cfg(test)]
mod fault_atomicity_tests {
    use super::*;

    #[test]
    fn cross_page_store_fault_leaves_prefix_and_code_generation_unchanged() {
        for perm in [None, Some(Permission::NONE), Some(Permission::READ_ONLY)] {
            let mut mem = Memory::new();
            mem.map_range(0x1000, PAGE_SIZE, Permission::READ_WRITE_EXECUTE);
            mem.write_u64(0x1ff8, u64::MAX).unwrap();
            mem.mark_code_range(0x1000, 0x2000);
            let generation = mem.code_write_gen();
            if let Some(perm) = perm { mem.map_range(0x2000, PAGE_SIZE, perm); }
            assert!(mem.write_u64(0x1ffc, 0).is_err());
            assert_eq!(mem.read_u64(0x1ff8).unwrap(), u64::MAX);
            assert_eq!(mem.code_write_gen(), generation);
            mem.map_range(0x2000, PAGE_SIZE, Permission::READ_WRITE);
            mem.write_u64(0x1ffc, 42).unwrap();
            assert_eq!(mem.read_u64(0x1ffc).unwrap(), 42);
        }
    }
}
