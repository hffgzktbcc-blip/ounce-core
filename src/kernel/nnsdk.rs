#![allow(dead_code)]

use crate::fs::VirtualFileSystem;
use crate::mmu::{Memory, MemoryError, Permission};
use std::collections::HashMap;

/// Result codes conforming to Nintendo's `nn::Result` (Module 0x02 = OS, 0x05 = FS, 0x09 = Mem).
pub const RESULT_SUCCESS: u32 = 0;
pub const RESULT_OS_TIMEOUT: u32 = 0x0002_0008;
pub const RESULT_FS_PATH_NOT_FOUND: u32 = 0x0005_0202;
pub const RESULT_FS_NOT_MOUNTED: u32 = 0x0005_0203;
pub const RESULT_MEM_OUT_OF_MEMORY: u32 = 0x0009_0104;

/// Subsystem allocation block tracked by `nn::mem::StandardAllocator`.
#[derive(Debug, Clone)]
pub struct AllocBlock {
    pub address: u64,
    pub size: usize,
    pub alignment: usize,
    pub allocated: bool,
}

/// Nintendo SDK `nn::mem::StandardAllocator` High-Level Emulation.
#[derive(Debug, Clone)]
pub struct StandardAllocator {
    pub heap_base: u64,
    pub heap_size: usize,
    pub allocated_bytes: usize,
    pub peak_allocated_bytes: usize,
    pub blocks: Vec<AllocBlock>,
    pub initialized: bool,
}

impl StandardAllocator {
    pub fn new() -> Self {
        Self {
            heap_base: 0,
            heap_size: 0,
            allocated_bytes: 0,
            peak_allocated_bytes: 0,
            blocks: Vec::new(),
            initialized: false,
        }
    }

    /// Initializes allocator arena at guest virtual address `arena_addr`.
    pub fn initialize(&mut self, arena_addr: u64, size: usize, mmu: &mut Memory) -> Result<u32, MemoryError> {
        if size == 0 {
            return Ok(RESULT_MEM_OUT_OF_MEMORY);
        }
        mmu.map_range(arena_addr, size, Permission::READ_WRITE);
        self.heap_base = arena_addr;
        self.heap_size = size;
        self.allocated_bytes = 0;
        self.peak_allocated_bytes = 0;
        self.blocks.clear();
        self.blocks.push(AllocBlock {
            address: arena_addr,
            size,
            alignment: 16,
            allocated: false,
        });
        self.initialized = true;
        Ok(RESULT_SUCCESS)
    }

    /// Allocates `size` bytes with specified `alignment` (minimum 16 bytes).
    pub fn allocate(&mut self, size: usize, alignment: usize) -> u64 {
        if !self.initialized || size == 0 {
            return 0;
        }
        let align = alignment.max(16);
        let aligned_size = (size + 15) & !15;

        for i in 0..self.blocks.len() {
            let block = &self.blocks[i];
            if !block.allocated {
                let aligned_addr = (block.address + (align as u64 - 1)) & !(align as u64 - 1);
                let padding = (aligned_addr - block.address) as usize;
                if block.size >= padding + aligned_size {
                    let orig_addr = block.address;
                    let orig_size = block.size;

                    // If padding is needed before the aligned start
                    if padding > 0 {
                        self.blocks[i] = AllocBlock {
                            address: orig_addr,
                            size: padding,
                            alignment: 16,
                            allocated: false,
                        };
                        self.blocks.insert(
                            i + 1,
                            AllocBlock {
                                address: aligned_addr,
                                size: aligned_size,
                                alignment: align,
                                allocated: true,
                            },
                        );
                        let remaining = orig_size - padding - aligned_size;
                        if remaining > 0 {
                            self.blocks.insert(
                                i + 2,
                                AllocBlock {
                                    address: aligned_addr + aligned_size as u64,
                                    size: remaining,
                                    alignment: 16,
                                    allocated: false,
                                },
                            );
                        }
                    } else {
                        self.blocks[i] = AllocBlock {
                            address: aligned_addr,
                            size: aligned_size,
                            alignment: align,
                            allocated: true,
                        };
                        let remaining = orig_size - aligned_size;
                        if remaining > 0 {
                            self.blocks.insert(
                                i + 1,
                                AllocBlock {
                                    address: aligned_addr + aligned_size as u64,
                                    size: remaining,
                                    alignment: 16,
                                    allocated: false,
                                },
                            );
                        }
                    }

                    self.allocated_bytes += aligned_size;
                    self.peak_allocated_bytes = self.peak_allocated_bytes.max(self.allocated_bytes);
                    return aligned_addr;
                }
            }
        }
        0 // Out of memory
    }

    /// Frees an allocation by guest address.
    pub fn free(&mut self, addr: u64) -> bool {
        if addr == 0 || !self.initialized {
            return false;
        }
        for block in &mut self.blocks {
            if block.address == addr && block.allocated {
                block.allocated = false;
                self.allocated_bytes = self.allocated_bytes.saturating_sub(block.size);
                return true;
            }
        }
        false
    }

    /// Returns total allocatable free memory bytes.
    pub fn get_total_free_size(&self) -> usize {
        self.blocks
            .iter()
            .filter(|b| !b.allocated)
            .map(|b| b.size)
            .sum()
    }
}

/// File handle descriptor inside `nn::fs`.
#[derive(Debug, Clone)]
pub struct NnFsFile {
    pub path: String,
    pub open_mode: u32,
    pub current_offset: i64,
}

/// Nintendo SDK `nn::fs` High-Level Emulation.
#[derive(Debug)]
pub struct NnFsManager {
    pub open_files: HashMap<u32, NnFsFile>,
    pub next_handle: u32,
    pub mounted_rom: bool,
    pub mounted_save: bool,
}

impl NnFsManager {
    pub fn new() -> Self {
        Self {
            open_files: HashMap::new(),
            next_handle: 100,
            mounted_rom: false,
            mounted_save: false,
        }
    }

    pub fn mount_rom(&mut self, vfs: &mut VirtualFileSystem) -> u32 {
        self.mounted_rom = true;
        let manifest = b"{\"package\":\"Switch 2 Title\",\"runtime\":\"nnSdk 23.0.1\"}";
        vfs.mount_file("rom:/manifest.json", manifest);
        RESULT_SUCCESS
    }

    pub fn mount_save_data(&mut self) -> u32 {
        self.mounted_save = true;
        RESULT_SUCCESS
    }

    pub fn open_file(&mut self, path: &str, mode: u32, vfs: &VirtualFileSystem) -> (u32, u32) {
        let normalized = if path.starts_with("rom:/") || path.starts_with("romfs:/") || path.starts_with("save:/") {
            path.to_string()
        } else {
            format!("rom:/{}", path.trim_start_matches('/'))
        };

        if vfs.get_file(&normalized).is_none() {
            return (RESULT_FS_PATH_NOT_FOUND, 0);
        }

        let handle = self.next_handle;
        self.next_handle += 1;
        self.open_files.insert(
            handle,
            NnFsFile {
                path: normalized,
                open_mode: mode,
                current_offset: 0,
            },
        );
        (RESULT_SUCCESS, handle)
    }

    pub fn read_file(
        &mut self,
        handle: u32,
        offset: i64,
        dest_addr: u64,
        size: usize,
        vfs: &VirtualFileSystem,
        mmu: &mut Memory,
    ) -> (u32, usize) {
        if let Some(file) = self.open_files.get_mut(&handle) {
            if let Some(data) = vfs.get_file(&file.path) {
                let off = offset.max(0) as usize;
                if off >= data.len() {
                    return (RESULT_SUCCESS, 0);
                }
                let bytes_to_read = size.min(data.len() - off);
                if mmu.write_bytes(dest_addr, &data[off..off + bytes_to_read]).is_ok() {
                    file.current_offset = (off + bytes_to_read) as i64;
                    return (RESULT_SUCCESS, bytes_to_read);
                }
            }
        }
        (RESULT_FS_PATH_NOT_FOUND, 0)
    }

    pub fn get_file_size(&self, handle: u32, vfs: &VirtualFileSystem) -> (u32, i64) {
        if let Some(file) = self.open_files.get(&handle) {
            if let Some(data) = vfs.get_file(&file.path) {
                return (RESULT_SUCCESS, data.len() as i64);
            }
        }
        (RESULT_FS_PATH_NOT_FOUND, 0)
    }

    pub fn close_file(&mut self, handle: u32) -> u32 {
        if self.open_files.remove(&handle).is_some() {
            RESULT_SUCCESS
        } else {
            RESULT_FS_PATH_NOT_FOUND
        }
    }
}

/// Nintendo SDK `nn::os` High-Level Emulation Manager.
#[derive(Debug, Clone)]
pub struct NnOsManager {
    pub system_tick_freq: u64,
    pub total_sleep_nanos: u64,
    pub active_events: HashMap<u64, bool>,
    pub active_mutexes: HashMap<u64, u32>,
}

impl NnOsManager {
    pub fn new() -> Self {
        Self {
            system_tick_freq: 19_200_000, // 19.2 MHz Switch 2 hardware clock
            total_sleep_nanos: 0,
            active_events: HashMap::new(),
            active_mutexes: HashMap::new(),
        }
    }

    pub fn sleep_thread(&mut self, nanoseconds: u64) {
        self.total_sleep_nanos += nanoseconds;
    }

    pub fn initialize_event(&mut self, event_ptr: u64, manual_reset: bool) -> u32 {
        self.active_events.insert(event_ptr, manual_reset);
        RESULT_SUCCESS
    }

    pub fn signal_event(&mut self, event_ptr: u64) -> u32 {
        self.active_events.insert(event_ptr, true);
        RESULT_SUCCESS
    }

    pub fn clear_event(&mut self, event_ptr: u64) -> u32 {
        self.active_events.insert(event_ptr, false);
        RESULT_SUCCESS
    }

    pub fn lock_mutex(&mut self, mutex_ptr: u64, thread_id: u32) -> u32 {
        self.active_mutexes.insert(mutex_ptr, thread_id);
        RESULT_SUCCESS
    }

    pub fn unlock_mutex(&mut self, mutex_ptr: u64) -> u32 {
        self.active_mutexes.remove(&mutex_ptr);
        RESULT_SUCCESS
    }
}

/// Unified Nintendo SDK (`nnSdk`) Runtime Engine.
#[derive(Debug)]
pub struct NnSdkRuntime {
    pub allocator: StandardAllocator,
    pub fs: NnFsManager,
    pub os: NnOsManager,
    pub initialized: bool,
    pub total_sdk_calls: u64,
}

impl NnSdkRuntime {
    pub fn new() -> Self {
        Self {
            allocator: StandardAllocator::new(),
            fs: NnFsManager::new(),
            os: NnOsManager::new(),
            initialized: false,
            total_sdk_calls: 0,
        }
    }

    /// Initializes core C++ runtime heap and mounts.
    pub fn initialize_runtime(&mut self, heap_addr: u64, heap_size: usize, vfs: &mut VirtualFileSystem, mmu: &mut Memory) -> u32 {
        self.allocator.initialize(heap_addr, heap_size, mmu).unwrap_or(RESULT_MEM_OUT_OF_MEMORY);
        self.fs.mount_rom(vfs);
        self.fs.mount_save_data();
        self.initialized = true;
        RESULT_SUCCESS
    }

    /// Dispatches an SDK call invoked from guest AArch64 code.
    pub fn dispatch_call(
        &mut self,
        function_id: u32,
        arg0: u64,
        arg1: u64,
        arg2: u64,
        arg3: u64,
        vfs: &mut VirtualFileSystem,
        mmu: &mut Memory,
    ) -> (u32, u64) {
        self.total_sdk_calls += 1;
        match function_id {
            // nn::init::InitializeAllocator(addr, size)
            0x1001 => {
                let res = self.allocator.initialize(arg0, arg1 as usize, mmu).unwrap_or(RESULT_MEM_OUT_OF_MEMORY);
                (res, 0)
            }
            // nn::mem::StandardAllocator::Allocate(size, alignment) -> addr
            0x2001 => {
                let addr = self.allocator.allocate(arg0 as usize, arg1 as usize);
                let res = if addr != 0 { RESULT_SUCCESS } else { RESULT_MEM_OUT_OF_MEMORY };
                (res, addr)
            }
            // nn::mem::StandardAllocator::Free(addr)
            0x2002 => {
                let ok = self.allocator.free(arg0);
                (if ok { RESULT_SUCCESS } else { 1 }, 0)
            }
            // nn::os::SleepThread(nanos)
            0x3001 => {
                self.os.sleep_thread(arg0);
                (RESULT_SUCCESS, 0)
            }
            // nn::os::GetSystemTick() -> tick
            0x3002 => {
                (RESULT_SUCCESS, self.os.system_tick_freq)
            }
            // nn::fs::OpenFile(path_ptr, path_len, mode) -> (res, handle)
            0x4001 => {
                let mut path_buf = vec![0u8; arg1 as usize];
                if mmu.read_bytes(arg0, &mut path_buf).is_ok() {
                    let path_str = String::from_utf8_lossy(&path_buf);
                    let (res, handle) = self.fs.open_file(&path_str, arg2 as u32, vfs);
                    (res, handle as u64)
                } else {
                    (RESULT_FS_PATH_NOT_FOUND, 0)
                }
            }
            // nn::fs::ReadFile(handle, offset, dest_addr, size) -> (res, bytes_read)
            0x4002 => {
                let (res, bytes_read) = self.fs.read_file(arg0 as u32, arg1 as i64, arg2, arg3 as usize, vfs, mmu);
                (res, bytes_read as u64)
            }
            // nn::fs::CloseFile(handle)
            0x4003 => {
                let res = self.fs.close_file(arg0 as u32);
                (res, 0)
            }
            _ => (1, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nnsdk_standard_allocator_roundtrip() {
        let mut mmu = Memory::new();
        let mut allocator = StandardAllocator::new();

        let heap_addr = 0x8000_0000;
        let heap_size = 0x10_0000; // 1MB
        assert_eq!(allocator.initialize(heap_addr, heap_size, &mut mmu).unwrap(), RESULT_SUCCESS);

        let p1 = allocator.allocate(1024, 64);
        assert_ne!(p1, 0);
        assert_eq!(p1 % 64, 0);

        let p2 = allocator.allocate(4096, 128);
        assert_ne!(p2, 0);
        assert_eq!(p2 % 128, 0);
        assert!(p2 >= p1 + 1024);

        assert!(allocator.free(p1));
        assert!(allocator.free(p2));
        assert_eq!(allocator.allocated_bytes, 0);
    }

    #[test]
    fn test_nnsdk_fs_and_os_primitives() {
        let mut mmu = Memory::new();
        let mut vfs = VirtualFileSystem::new();
        let mut runtime = NnSdkRuntime::new();

        runtime.initialize_runtime(0x8000_0000, 0x10_0000, &mut vfs, &mut mmu);

        // Open and read manifest mounted by nnSdk
        let (res, handle) = runtime.fs.open_file("rom:/manifest.json", 1, &vfs);
        assert_eq!(res, RESULT_SUCCESS);
        assert!(handle >= 100);

        let dest_buf = 0x8001_0000;
        let (read_res, bytes) = runtime.fs.read_file(handle, 0, dest_buf, 64, &vfs, &mut mmu);
        assert_eq!(read_res, RESULT_SUCCESS);
        assert!(bytes > 0);

        let mut read_back = vec![0u8; bytes];
        mmu.read_bytes(dest_buf, &mut read_back).unwrap();
        let text = String::from_utf8_lossy(&read_back);
        assert!(text.contains("Switch 2 Title"));

        assert_eq!(runtime.fs.close_file(handle), RESULT_SUCCESS);

        // Test OS sleep and ticks
        runtime.os.sleep_thread(1_000_000);
        assert_eq!(runtime.os.total_sleep_nanos, 1_000_000);
        assert_eq!(runtime.os.system_tick_freq, 19_200_000);
    }
}
