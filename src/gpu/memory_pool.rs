#![allow(dead_code)]

use std::collections::HashMap;

/// Memory access flags for `nvnMemoryPoolInitialize`
pub const NVN_MEMORY_POOL_CPU_NO_ACCESS: u32 = 0x0001;
pub const NVN_MEMORY_POOL_CPU_UNCACHED: u32 = 0x0002;
pub const NVN_MEMORY_POOL_CPU_CACHED: u32 = 0x0004;
pub const NVN_MEMORY_POOL_GPU_NO_ACCESS: u32 = 0x0008;
pub const NVN_MEMORY_POOL_GPU_UNCACHED: u32 = 0x0010;
pub const NVN_MEMORY_POOL_GPU_CACHED: u32 = 0x0020;
pub const NVN_MEMORY_POOL_SHADER_CODE: u32 = 0x0040;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvnBufferType {
    Vertex,
    Index,
    Uniform,
    Texture,
    ShaderCode,
}

/// Represents a sub-allocated `NVNbuffer` descriptor inside a memory pool.
#[derive(Debug, Clone)]
pub struct NvnBuffer {
    pub id: u32,
    pub pool_id: u32,
    pub buffer_type: NvnBufferType,
    pub pool_offset: usize,
    pub size: usize,
    pub gpu_address: u64,
}

/// High-Level NVIDIA Tegra T239 `nvnMemoryPool` allocator.
/// Manages physical & virtual VRAM sub-allocations for vertex arrays, index lists,
/// uniform constant blocks, and textures.
#[derive(Debug, Clone)]
pub struct NvnMemoryPool {
    pub pool_id: u32,
    pub flags: u32,
    pub base_gpu_addr: u64,
    pub size: usize,
    pub bytes_allocated: usize,
    pub buffers: HashMap<u32, NvnBuffer>,
    next_buf_id: u32,
}

impl NvnMemoryPool {
    pub fn new(pool_id: u32, base_gpu_addr: u64, size: usize, flags: u32) -> Self {
        Self {
            pool_id,
            flags,
            base_gpu_addr,
            size,
            bytes_allocated: 0,
            buffers: HashMap::new(),
            next_buf_id: 1,
        }
    }

    /// Allocates an `NVNbuffer` within this memory pool.
    pub fn allocate_buffer(&mut self, size: usize, alignment: usize, buffer_type: NvnBufferType) -> Option<NvnBuffer> {
        let align_mask = alignment.saturating_sub(1);
        let aligned_offset = (self.bytes_allocated + align_mask) & !align_mask;

        if aligned_offset + size > self.size {
            return None; // Out of pool memory
        }

        let buf_id = self.next_buf_id;
        self.next_buf_id += 1;
        self.bytes_allocated = aligned_offset + size;

        let buf = NvnBuffer {
            id: buf_id,
            pool_id: self.pool_id,
            buffer_type,
            pool_offset: aligned_offset,
            size,
            gpu_address: self.base_gpu_addr + aligned_offset as u64,
        };

        self.buffers.insert(buf_id, buf.clone());
        Some(buf)
    }

    pub fn get_buffer(&self, buf_id: u32) -> Option<&NvnBuffer> {
        self.buffers.get(&buf_id)
    }
}

/// Manages all initialized `NVN` memory pools across the emulator runtime.
#[derive(Debug, Default)]
pub struct NvnMemoryPoolManager {
    pub pools: HashMap<u32, NvnMemoryPool>,
    next_pool_id: u32,
    pub total_vram_mapped: usize,
}

impl NvnMemoryPoolManager {
    pub fn new() -> Self {
        Self {
            pools: HashMap::new(),
            next_pool_id: 1,
            total_vram_mapped: 0,
        }
    }

    pub fn create_pool(&mut self, base_gpu_addr: u64, size: usize, flags: u32) -> u32 {
        let id = self.next_pool_id;
        self.next_pool_id += 1;
        let pool = NvnMemoryPool::new(id, base_gpu_addr, size, flags);
        self.total_vram_mapped += size;
        self.pools.insert(id, pool);
        id
    }

    pub fn get_pool_mut(&mut self, pool_id: u32) -> Option<&mut NvnMemoryPool> {
        self.pools.get_mut(&pool_id)
    }

    pub fn get_pool(&self, pool_id: u32) -> Option<&NvnMemoryPool> {
        self.pools.get(&pool_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nvn_memory_pool_and_buffer_allocations() {
        let mut mgr = NvnMemoryPoolManager::new();
        let pool_id = mgr.create_pool(
            0x9800_0000,
            1024 * 1024, // 1MB pool
            NVN_MEMORY_POOL_CPU_CACHED | NVN_MEMORY_POOL_GPU_CACHED,
        );

        let pool = mgr.get_pool_mut(pool_id).unwrap();
        // Allocate a 256-byte aligned Uniform Buffer
        let ubo = pool.allocate_buffer(512, 256, NvnBufferType::Uniform).unwrap();
        assert_eq!(ubo.gpu_address, 0x9800_0000);
        assert_eq!(ubo.size, 512);

        // Allocate a 64-byte aligned Vertex Buffer
        let vbo = pool.allocate_buffer(1024, 64, NvnBufferType::Vertex).unwrap();
        assert_eq!(vbo.gpu_address, 0x9800_0200);
        assert_eq!(vbo.size, 1024);

        assert_eq!(pool.bytes_allocated, 0x200 + 1024);
    }
}
