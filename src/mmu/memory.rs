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
}

impl Page {
    pub fn new(perm: Permission) -> Self {
        Self {
            data: Box::new([0u8; PAGE_SIZE]),
            perm,
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
}

impl Memory {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
        }
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

    /// Allocate and map a contiguous virtual memory range.
    ///
    /// Accounts for intra-page byte offsets (`base_addr & (PAGE_SIZE - 1)`), ensuring that
    /// multi-page unaligned ranges intersecting page boundaries map all required pages.
    pub fn map_range(&mut self, base_addr: u64, size: usize, perm: Permission) {
        if size == 0 {
            return;
        }

        let start_page = base_addr >> 12;
        let page_offset = (base_addr as usize) & (PAGE_SIZE - 1);
        let num_pages = ((page_offset as u128 + size as u128 + (PAGE_SIZE as u128 - 1))
            / (PAGE_SIZE as u128)) as usize;

        for i in 0..num_pages {
            let page_idx = match start_page.checked_add(i as u64) {
                Some(idx) => idx,
                None => break,
            };
            self.pages.insert(page_idx, Page::new(perm));
        }
    }

    /// Checks if a virtual address is mapped.
    pub fn is_mapped(&self, addr: u64) -> bool {
        self.pages.contains_key(&(addr >> 12))
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
    fn test_map_range_zero_size() {
        let mut mmu = Memory::new();
        mmu.map_range(0x1000, 0, Permission::READ_WRITE);
        mmu.map_range(0x1FFF, 0, Permission::READ_WRITE);
        assert_eq!(mmu.pages.len(), 0);
        assert!(!mmu.is_mapped(0x1000));
        assert!(!mmu.is_mapped(0x1FFF));
        assert_eq!(mmu.bucket_stats().total_pages, 0);
    }

    #[test]
    fn test_map_range_aligned_single_and_multi_page() {
        let mut mmu = Memory::new();
        // 1 page at 0x1000..0x2000
        mmu.map_range(0x1000, PAGE_SIZE, Permission::READ_WRITE);
        assert_eq!(mmu.pages.len(), 1);
        assert!(mmu.is_mapped(0x1000));
        assert!(mmu.is_mapped(0x1FFF));
        assert!(!mmu.is_mapped(0x2000));

        // 2 pages at 0x4000..0x6000
        mmu.map_range(0x4000, PAGE_SIZE * 2, Permission::READ_ONLY);
        assert_eq!(mmu.pages.len(), 3);
        assert!(mmu.is_mapped(0x4000));
        assert!(mmu.is_mapped(0x5FFF));
        assert!(!mmu.is_mapped(0x6000));
    }

    #[test]
    fn test_map_range_unaligned_single_page() {
        let mut mmu = Memory::new();
        // 0x1001..0x1500 (inside page 1)
        mmu.map_range(0x1001, 0x4FF, Permission::READ_WRITE);
        assert_eq!(mmu.pages.len(), 1);
        assert!(mmu.is_mapped(0x1000));
        assert!(!mmu.is_mapped(0x2000));

        // 0x1FFF..0x2000 (last byte of page 1, size = 1)
        let mut mmu2 = Memory::new();
        mmu2.map_range(0x1FFF, 1, Permission::READ_WRITE);
        assert_eq!(mmu2.pages.len(), 1);
        assert!(mmu2.is_mapped(0x1FFF));
        assert!(!mmu2.is_mapped(0x2000));
    }

    #[test]
    fn test_map_range_unaligned_two_pages_boundary_crossing() {
        let mut mmu = Memory::new();
        // R3 core requirement: 2 bytes across 0x1FFF..0x2001
        mmu.map_range(0x1FFF, 2, Permission::READ_WRITE);

        assert_eq!(mmu.pages.len(), 2, "Both intersecting pages must be mapped");
        assert!(mmu.is_mapped(0x1FFF), "Page 1 must be mapped");
        assert!(mmu.is_mapped(0x2000), "Page 2 must be mapped");
        assert_eq!(mmu.bucket_stats().total_pages, 2);

        // Byte operations across boundary
        mmu.write_bytes(0x1FFF, &[0xDE, 0xAD]).unwrap();
        let mut read_buf = [0u8; 2];
        mmu.read_bytes(0x1FFF, &mut read_buf).unwrap();
        assert_eq!(read_buf, [0xDE, 0xAD]);

        // Scalar u16 across boundary
        mmu.write_u16(0x1FFF, 0xBEEF).unwrap();
        assert_eq!(mmu.read_u16(0x1FFF).unwrap(), 0xBEEF);

        // Verify byte distribution in little endian
        assert_eq!(mmu.read_u8(0x1FFF).unwrap(), 0xEF);
        assert_eq!(mmu.read_u8(0x2000).unwrap(), 0xBE);
    }

    #[test]
    fn test_map_range_unaligned_multi_page_three_pages() {
        let mut mmu = Memory::new();
        // 0x1FFF..0x3001 (size = 4098): 1 byte in Page 1, 4096 in Page 2, 1 byte in Page 3
        mmu.map_range(0x1FFF, 4098, Permission::READ_WRITE);

        assert_eq!(mmu.pages.len(), 3, "Three intersecting pages must be mapped");
        assert!(mmu.is_mapped(0x1FFF));
        assert!(mmu.is_mapped(0x2000));
        assert!(mmu.is_mapped(0x3000));
        assert!(!mmu.is_mapped(0x4000));

        mmu.write_u8(0x1FFF, 0x11).unwrap();
        mmu.write_u8(0x2800, 0x22).unwrap();
        mmu.write_u8(0x3000, 0x33).unwrap();

        assert_eq!(mmu.read_u8(0x1FFF).unwrap(), 0x11);
        assert_eq!(mmu.read_u8(0x2800).unwrap(), 0x22);
        assert_eq!(mmu.read_u8(0x3000).unwrap(), 0x33);
    }

    #[test]
    fn test_cross_page_scalar_reads_and_writes() {
        let mut mmu = Memory::new();
        mmu.map_range(0x1000, PAGE_SIZE * 2, Permission::READ_WRITE);

        // Cross-page u32: 0x1FFE (2 bytes in page 1, 2 bytes in page 2)
        mmu.write_u32(0x1FFE, 0x12345678).unwrap();
        assert_eq!(mmu.read_u32(0x1FFE).unwrap(), 0x12345678);
        assert_eq!(mmu.read_u8(0x1FFE).unwrap(), 0x78);
        assert_eq!(mmu.read_u8(0x1FFF).unwrap(), 0x56);
        assert_eq!(mmu.read_u8(0x2000).unwrap(), 0x34);
        assert_eq!(mmu.read_u8(0x2001).unwrap(), 0x12);

        // Cross-page u64: 0x1FFC (4 bytes in page 1, 4 bytes in page 2)
        mmu.write_u64(0x1FFC, 0x0123456789ABCDEF).unwrap();
        assert_eq!(mmu.read_u64(0x1FFC).unwrap(), 0x0123456789ABCDEF);
    }

    #[test]
    fn test_permission_checks_across_page_boundary() {
        let mut mmu = Memory::new();
        // Page 1: READ_WRITE
        mmu.map_range(0x1000, PAGE_SIZE, Permission::READ_WRITE);
        // Page 2: READ_ONLY
        mmu.map_range(0x2000, PAGE_SIZE, Permission::READ_ONLY);

        // Read across boundary: both have read permission -> Ok
        mmu.write_u8(0x1FFF, 0xAA).unwrap();
        let val = mmu.read_u16(0x1FFF);
        assert!(val.is_ok());

        // Write across boundary: Page 2 lacks write permission -> Err
        let write_res = mmu.write_u16(0x1FFF, 0xBBBB);
        match write_res {
            Err(MemoryError::PermissionDenied { addr, required, actual }) => {
                assert_eq!(addr, 0x2000);
                assert_eq!(required, "WRITE");
                assert_eq!(actual, Permission::READ_ONLY);
            }
            other => panic!("Expected PermissionDenied at 0x2000, got {:?}", other),
        }

        // Page 3: NONE
        mmu.map_range(0x3000, PAGE_SIZE, Permission::NONE);
        let read_p3 = mmu.read_u16(0x2FFF);
        match read_p3 {
            Err(MemoryError::PermissionDenied { addr, required, actual }) => {
                assert_eq!(addr, 0x3000);
                assert_eq!(required, "READ");
                assert_eq!(actual, Permission::NONE);
            }
            other => panic!("Expected PermissionDenied at 0x3000, got {:?}", other),
        }
    }

    #[test]
    fn test_unmapped_access_across_page_boundary() {
        let mut mmu = Memory::new();
        // Only page 1 is mapped
        mmu.map_range(0x1000, PAGE_SIZE, Permission::READ_WRITE);

        // Attempt read across boundary into unmapped page 2
        match mmu.read_u16(0x1FFF) {
            Err(MemoryError::Unmapped { addr }) => assert_eq!(addr, 0x2000),
            other => panic!("Expected Unmapped at 0x2000, got {:?}", other),
        }

        // Attempt write across boundary into unmapped page 2
        match mmu.write_u16(0x1FFF, 0x42) {
            Err(MemoryError::Unmapped { addr }) => assert_eq!(addr, 0x2000),
            other => panic!("Expected Unmapped at 0x2000, got {:?}", other),
        }
    }

    #[test]
    fn test_unaligned_mapping_preserves_telemetry_buckets() {
        let mut mmu = Memory::new();
        // Map 2 unaligned pages in VRAM region (0x9000_0000..0xA000_0000)
        mmu.map_range(0x9000_0FFF, 2, Permission::READ_WRITE);
        let stats = mmu.bucket_stats();
        assert_eq!(stats.total_pages, 2);
        assert_eq!(stats.vram_bytes, PAGE_SIZE * 2);
        assert_eq!(stats.total_bytes, PAGE_SIZE * 2);
    }

    #[test]
    fn test_map_range_arithmetic_overflow_safety() {
        let mut mmu = Memory::new();
        // High 64-bit address space (0x7FFF_FFFF_EFFF)
        mmu.map_range(0x7FFF_FFFF_EFFF, 2, Permission::READ_WRITE);
        assert_eq!(mmu.pages.len(), 2);
        assert!(mmu.is_mapped(0x7FFF_FFFF_E000));
        assert!(mmu.is_mapped(0x7FFF_FFFF_F000));

        // Near u64 boundary: start_page is 0x000F_FFFF_FFFF_FFFF
        let mut mmu_top = Memory::new();
        mmu_top.map_range(0xFFFF_FFFF_FFFF_FFF0, 32, Permission::READ_WRITE);
        assert!(mmu_top.pages.len() >= 1);
    }
}
