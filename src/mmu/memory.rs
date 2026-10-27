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
}

impl Page {
    pub fn new(perm: Permission) -> Self {
        Self {
            data: Box::new([0u8; PAGE_SIZE]),
            perm,
            holds_code: false,
            code_write_gen: 0,
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
    /// Bumped on every write (or remap) that touches a page holding cached code.
    /// Block caches compare it against the value they last saw to detect self-modifying code.
    code_write_gen: u64,
}

impl Memory {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            code_write_gen: 0,
        }
    }

    /// Current code-write generation. Unchanged means no cached code page was written.
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

    /// True if any page in `[start, end)` was written (or remapped) after generation `since`.
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

        for page_idx in start_page..=last_page {
            let mut page = Page::new(perm);
            if self.pages.get(&page_idx).is_some_and(|old| old.holds_code) {
                // Remapping over cached code replaces it.
                self.code_write_gen += 1;
                page.code_write_gen = self.code_write_gen;
            }
            self.pages.insert(page_idx, page);
        }
    }

    /// Checks if a virtual address is mapped.
    pub fn is_mapped(&self, addr: u64) -> bool {
        self.pages.contains_key(&(addr >> 12))
    }

    /// Access the Page for a page number (VA >> 12), if mapped.
    pub fn get_page(&self, page_num: u64) -> Option<&Page> {
        self.pages.get(&page_num)
    }

    /// Read raw byte slice from virtual memory.
    pub fn read_bytes(&self, addr: u64, buf: &mut [u8]) -> Result<(), MemoryError> {
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

            buf[offset..offset + bytes_in_page]
                .copy_from_slice(&page.data[page_offset..page_offset + bytes_in_page]);

            curr_addr += bytes_in_page as u64;
            offset += bytes_in_page;
        }

        Ok(())
    }

    /// Write raw byte slice into virtual memory.
    pub fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), MemoryError> {
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

            page.data[page_offset..page_offset + bytes_in_page]
                .copy_from_slice(&data[offset..offset + bytes_in_page]);
            if page.holds_code {
                self.code_write_gen += 1;
                page.code_write_gen = self.code_write_gen;
            }

            curr_addr += bytes_in_page as u64;
            offset += bytes_in_page;
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
