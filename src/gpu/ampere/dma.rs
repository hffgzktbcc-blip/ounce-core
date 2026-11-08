//! MAXWELL_DMA_COPY_A (`0xB0B5`) copy engine: the subset deko3d uses to upload buffers into images.
//!
//! Method offsets and `LAUNCH_DMA` fields come from NVIDIA's open-gpu-doc `clb0b5.h`. Modelled: one
//! launch copies `LINE_COUNT` lines (one without `MULTI_LINE_ENABLE`) of `LINE_LENGTH_IN` bytes between
//! pitch and block-linear surfaces. Not modelled (reported, nothing copied): component remapping and
//! semaphore or interrupt requests.

use super::engine::{Diagnostic, GpuMemoryWriter};
use crate::gpu::tiling::BlockLinear;

pub const LAUNCH_DMA: u32 = 0x0300;
pub const OFFSET_IN_UPPER: u32 = 0x0400;
pub const OFFSET_IN_LOWER: u32 = 0x0404;
pub const OFFSET_OUT_UPPER: u32 = 0x0408;
pub const OFFSET_OUT_LOWER: u32 = 0x040C;
pub const PITCH_IN: u32 = 0x0410;
pub const PITCH_OUT: u32 = 0x0414;
pub const LINE_LENGTH_IN: u32 = 0x0418;
pub const LINE_COUNT: u32 = 0x041C;
/// `SET_DST_BLOCK_SIZE` .. `SET_DST_ORIGIN`, then the same six for the source at `SET_SRC_BLOCK_SIZE`.
pub const SET_DST_BLOCK_SIZE: u32 = 0x070C;
pub const SET_SRC_BLOCK_SIZE: u32 = 0x0728;
/// Highest method offset the engine keeps.
const METHOD_SPACE: u32 = 0x0800;

/// One side of a copy: `SET_*_BLOCK_SIZE`, `_WIDTH` (bytes), `_HEIGHT`, `_DEPTH`, `_LAYER`, `_ORIGIN`.
struct BlockSurface {
    height_log2: u8,
    depth_log2: u8,
    width: u32,
    height: u32,
    depth: u32,
    layer: u32,
    origin_x: u32,
    origin_y: u32,
}

#[derive(Debug, Clone)]
pub struct DmaCopy {
    regs: Vec<u32>,
}

impl Default for DmaCopy {
    fn default() -> Self {
        Self { regs: vec![0; (METHOD_SPACE / 4) as usize] }
    }
}

impl DmaCopy {
    fn reg(&self, method: u32) -> u32 {
        self.regs[(method / 4) as usize]
    }

    fn addr(&self, upper: u32) -> u64 {
        ((self.reg(upper) as u64 & 0xff) << 32) | self.reg(upper + 4) as u64
    }

    fn block_surface(&self, base: u32) -> BlockSurface {
        let size = self.reg(base);
        let origin = self.reg(base + 20);
        BlockSurface {
            height_log2: ((size >> 4) & 0xf) as u8,
            depth_log2: ((size >> 8) & 0xf) as u8,
            width: self.reg(base + 4),
            height: self.reg(base + 8),
            depth: self.reg(base + 12).max(1),
            layer: self.reg(base + 16),
            origin_x: origin & 0xffff,
            origin_y: origin >> 16,
        }
    }

    /// Writes a method; `LAUNCH_DMA` runs the copy.
    pub fn execute(&mut self, method: u32, data: u32, mem: &mut dyn GpuMemoryWriter, diagnostics: &mut Vec<Diagnostic>) {
        if method >= METHOD_SPACE {
            return;
        }
        self.regs[(method / 4) as usize] = data;
        if method == LAUNCH_DMA {
            if let Err(reason) = self.launch(data, mem) {
                diagnostics.push(Diagnostic::DmaSkipped { reason });
            }
        }
    }

    fn launch(&self, launch: u32, mem: &mut dyn GpuMemoryWriter) -> Result<(), &'static str> {
        // SEMAPHORE_TYPE 4:3, INTERRUPT_TYPE 6:5, REMAP_ENABLE 10
        if (launch >> 3) & 3 != 0 || (launch >> 5) & 3 != 0 {
            return Err("DMA semaphore or interrupt request");
        }
        if launch & (1 << 10) != 0 {
            return Err("DMA component remap");
        }
        let src_pitch = launch & (1 << 7) != 0;
        let dst_pitch = launch & (1 << 8) != 0;
        let lines = if launch & (1 << 9) != 0 { self.reg(LINE_COUNT) } else { 1 };
        let len = self.reg(LINE_LENGTH_IN) as usize;
        // Bound both allocation and total copy work before touching guest buffers.
        if len as u64 * lines as u64 > 64 * 1024 * 1024 || lines > 1024 * 1024 {
            return Err("DMA transfer exceeds host work limit");
        }
        let (src, dst) = (self.addr(OFFSET_IN_UPPER), self.addr(OFFSET_OUT_UPPER));
        let src_bl = self.block_surface(SET_SRC_BLOCK_SIZE);
        let dst_bl = self.block_surface(SET_DST_BLOCK_SIZE);
        let mut line = vec![0u8; len];
        for y in 0..lines {
            self.line(src, src_pitch, self.reg(PITCH_IN), &src_bl, y, &mut line, mem, false)?;
            self.line(dst, dst_pitch, self.reg(PITCH_OUT), &dst_bl, y, &mut line, mem, true)?;
        }
        Ok(())
    }

    /// Reads (or with `write`, writes) line `y` of a surface, byte by byte through the tiling for
    /// block-linear surfaces (a line may cross GOBs).
    #[allow(clippy::too_many_arguments)]
    fn line(&self, base: u64, pitch: bool, pitch_bytes: u32, bl: &BlockSurface, y: u32, line: &mut [u8],
        mem: &mut dyn GpuMemoryWriter, write: bool) -> Result<(), &'static str> {
        if pitch {
            let va = base + y as u64 * pitch_bytes as u64;
            return match write {
                true => Ok(mem.write(va, line)),
                false => mem.read(va, line).then_some(()).ok_or("DMA source is not readable"),
            };
        }
        let geo = BlockLinear::new(bl.width, bl.height, bl.depth, bl.height_log2, bl.depth_log2);
        let row = bl.origin_y + y;
        for (i, byte) in line.iter_mut().enumerate() {
            let va = base + geo.offset(bl.origin_x + i as u32, row, bl.layer) as u64;
            if write {
                mem.write(va, std::slice::from_ref(byte));
            } else if !mem.read(va, std::slice::from_mut(byte)) {
                return Err("DMA source is not readable");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn oversized_dma_is_rejected_before_allocating_or_writing() {
        let mut memory = Mem::default();
        let mut dma = DmaCopy::default();
        let mut diagnostics = Vec::new();
        dma.execute(LINE_LENGTH_IN, u32::MAX, &mut memory, &mut diagnostics);
        dma.execute(LAUNCH_DMA, (1 << 7) | (1 << 8), &mut memory, &mut diagnostics);
        assert!(memory.0.is_empty());
        assert!(matches!(diagnostics.as_slice(), [Diagnostic::DmaSkipped { reason: "DMA transfer exceeds host work limit" }]));
    }

    #[derive(Default)]
    struct Mem(HashMap<u64, u8>);

    impl GpuMemoryWriter for Mem {
        fn write(&mut self, va: u64, bytes: &[u8]) {
            for (i, b) in bytes.iter().enumerate() {
                self.0.insert(va + i as u64, *b);
            }
        }
        fn read(&mut self, va: u64, buf: &mut [u8]) -> bool {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = self.0.get(&(va + i as u64)).copied().unwrap_or(0);
            }
            true
        }
    }

    /// deko3d's buffer-to-image upload of one 16x16 R32 layer (methods as deko_console sends them).
    #[test]
    fn pitch_to_block_linear_layer() {
        let mut mem = Mem::default();
        let src: Vec<u8> = (0..1024u32).map(|i| i as u8 ^ (i >> 8) as u8).collect();
        mem.write(0x5_0181_0000, &src);
        let mut dma = DmaCopy::default();
        let mut diags = Vec::new();
        for (m, d) in [(PITCH_IN, 0x40), (0x70c, 0x1010), (0x710, 0x40), (0x714, 0x10), (0x718, 1), (0x71c, 0),
            (0x720, 0), (OFFSET_IN_UPPER, 5), (OFFSET_IN_LOWER, 0x0181_0000), (OFFSET_OUT_UPPER, 5),
            (OFFSET_OUT_LOWER, 0x00ff_0000), (LINE_LENGTH_IN, 0x40), (LINE_COUNT, 0x10), (LAUNCH_DMA, 0x286)] {
            dma.execute(m, d, &mut mem, &mut diags);
        }
        assert!(diags.is_empty(), "{diags:?}");
        // Block height 2 GOBs: rows 0..8 fill the first GOB, rows 8..16 the second.
        let geo = BlockLinear::new(0x40, 16, 1, 1, 0);
        for y in 0..16u32 {
            for x in 0..64u32 {
                let got = mem.0[&(0x5_00ff_0000 + geo.offset(x, y, 0) as u64)];
                assert_eq!(got, src[(y * 64 + x) as usize], "({x}, {y})");
            }
        }
        dma.execute(LAUNCH_DMA, 0x286 | 1 << 3, &mut mem, &mut diags);
        assert_eq!(diags, [Diagnostic::DmaSkipped { reason: "DMA semaphore or interrupt request" }]);
    }
}
