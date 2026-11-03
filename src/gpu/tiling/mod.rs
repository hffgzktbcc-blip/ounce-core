//! Surface tiling: NVIDIA block-linear and pitch-linear layouts, and conversion between
//! them.
//!
//! Shared by the Ampere 3D path (`src/gpu/ampere`, which writes render targets and reads
//! textures in guest memory) and the present path (`src/gpu/present`, which detiles the
//! raw block-linear frame the kernel publishes). Pure functions over byte slices: no guest
//! memory, no host graphics.
//!
//! ```ignore
//! use crate::gpu::tiling::{detile, Format, Layout, Surface};
//! let surface = Surface::new_2d(1280, 720, Format::R8G8B8A8Unorm,
//!     Layout::BlockLinear { height_log2: 4, depth_log2: 0 });
//! let rgba = detile(&surface, guest_bytes)?; // tightly packed rows, 1280 * 4 bytes each
//! ```
//!
//! Layout sources are cited in `block_linear.rs`.

// Consumers (the Ampere decoder and the present path) land in their own PRs.
#![allow(dead_code)]

mod block_linear;
mod format;
#[cfg(test)]
mod tests;

pub use block_linear::{
    clamp_block_log2, gob_offset, BlockLinear, GOB_BYTES, GOB_HEIGHT, GOB_WIDTH_BYTES, MAX_BLOCK_LOG2,
};
pub use format::Format;

use std::fmt;

/// How a surface's elements are placed in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Rows `stride` bytes apart; slices `stride * rows` apart. `stride` must cover one row.
    Pitch { stride: u32 },
    /// Block linear with `2^height_log2` GOBs per block vertically and `2^depth_log2` in depth.
    BlockLinear { height_log2: u8, depth_log2: u8 },
}

/// One surface level: extent in texels, format and layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub format: Format,
    pub layout: Layout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TilingError {
    ZeroExtent,
    /// Pitch stride shorter than one row of elements.
    StrideTooSmall { stride: u32, row_bytes: u32 },
    BlockLog2TooLarge { height_log2: u8, depth_log2: u8 },
    /// Source and destination differ in extent or element size.
    Mismatch,
    /// A buffer is shorter than the surface needs.
    BufferTooSmall { needed: usize, got: usize },
    Overflow,
}

impl fmt::Display for TilingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TilingError::ZeroExtent => write!(f, "surface has a zero extent"),
            TilingError::StrideTooSmall { stride, row_bytes } => {
                write!(f, "pitch stride {stride} is below the row size {row_bytes}")
            }
            TilingError::BlockLog2TooLarge { height_log2, depth_log2 } => write!(
                f,
                "block height/depth log2 {height_log2}/{depth_log2} exceeds {MAX_BLOCK_LOG2}"
            ),
            TilingError::Mismatch => write!(f, "source and destination surfaces differ"),
            TilingError::BufferTooSmall { needed, got } => {
                write!(f, "buffer holds {got} bytes, surface needs {needed}")
            }
            TilingError::Overflow => write!(f, "surface size overflows"),
        }
    }
}

impl std::error::Error for TilingError {}

/// Contiguous run that both layouts keep together: 16 bytes is one GOB row segment, and
/// every element size (1..=16 bytes) divides it.
const RUN_BYTES: u32 = 16;

impl Surface {
    pub fn new_2d(width: u32, height: u32, format: Format, layout: Layout) -> Self {
        Self { width, height, depth: 1, format, layout }
    }

    /// Tightly packed pitch-linear surface (stride = one row of elements).
    pub fn packed(width: u32, height: u32, depth: u32, format: Format) -> Self {
        let mut s = Self { width, height, depth, format, layout: Layout::Pitch { stride: 0 } };
        s.layout = Layout::Pitch { stride: s.row_bytes() };
        s
    }

    /// Extent in elements: BCn surfaces count 4x4 blocks, rounded up.
    pub fn extent_elements(&self) -> (u32, u32, u32) {
        let (bw, bh) = self.format.element_extent();
        (self.width.div_ceil(bw), self.height.div_ceil(bh), self.depth)
    }

    /// Bytes in one row of elements, without padding. Only meaningful for a surface that
    /// passed [`Surface::validate`]; use [`Surface::checked_row_bytes`] on untrusted input.
    pub fn row_bytes(&self) -> u32 {
        self.checked_row_bytes().unwrap_or(0)
    }

    /// Bytes in one row of elements, or `None` if the row does not fit in a `u32`.
    pub fn checked_row_bytes(&self) -> Option<u32> {
        self.extent_elements().0.checked_mul(self.format.bytes_per_element())
    }

    /// Block-linear geometry, or `None` for pitch surfaces.
    pub fn block_linear(&self) -> Option<BlockLinear> {
        match self.layout {
            Layout::BlockLinear { height_log2, depth_log2 } => {
                let (_, rows, depth) = self.extent_elements();
                Some(BlockLinear::new(self.row_bytes(), rows, depth, height_log2, depth_log2))
            }
            Layout::Pitch { .. } => None,
        }
    }

    pub fn validate(&self) -> Result<(), TilingError> {
        if self.width == 0 || self.height == 0 || self.depth == 0 {
            return Err(TilingError::ZeroExtent);
        }
        let row_bytes = self.checked_row_bytes().ok_or(TilingError::Overflow)?;
        match self.layout {
            Layout::Pitch { stride } => {
                if stride < row_bytes {
                    return Err(TilingError::StrideTooSmall { stride, row_bytes });
                }
            }
            Layout::BlockLinear { height_log2, depth_log2 } => {
                if height_log2 > MAX_BLOCK_LOG2 || depth_log2 > MAX_BLOCK_LOG2 {
                    return Err(TilingError::BlockLog2TooLarge { height_log2, depth_log2 });
                }
                // The padded row (whole GOBs) must also fit, so `BlockLinear::stride` holds.
                row_bytes.div_ceil(GOB_WIDTH_BYTES).checked_mul(GOB_WIDTH_BYTES).ok_or(TilingError::Overflow)?;
            }
        }
        Ok(())
    }

    /// Bytes the surface occupies in memory (block-linear sizes include block padding;
    /// pitch sizes end at the last byte of the last row).
    pub fn size_bytes(&self) -> Result<usize, TilingError> {
        self.validate()?;
        match self.layout {
            Layout::Pitch { stride } => {
                let (_, rows, depth) = self.extent_elements();
                let full_rows = (rows as u64) * (depth as u64) - 1;
                let size = full_rows
                    .checked_mul(stride as u64)
                    .and_then(|bytes| bytes.checked_add(self.row_bytes() as u64))
                    .ok_or(TilingError::Overflow)?;
                usize::try_from(size).map_err(|_| TilingError::Overflow)
            }
            Layout::BlockLinear { .. } => {
                let bl = self.block_linear().expect("block linear");
                let size = (bl.gobs_wide as u128)
                    * (bl.blocks_high as u128)
                    * (bl.blocks_deep as u128)
                    * (bl.block_bytes() as u128);
                usize::try_from(size).map_err(|_| TilingError::Overflow)
            }
        }
    }

    /// Byte offset of byte column `x` within element row `y` of slice `z`. For many
    /// lookups, `convert` precomputes the geometry once instead.
    pub fn byte_offset(&self, x: u32, y: u32, z: u32) -> usize {
        Mapper::new(self).offset(x, y, z)
    }
}

/// Copies every element of `src` into `dst`, converting layouts. Both surfaces must have
/// the same extent and element size; padding bytes in `dst` are left as they were.
pub fn convert(src: &Surface, src_bytes: &[u8], dst: &Surface, dst_bytes: &mut [u8]) -> Result<(), TilingError> {
    if src.extent_elements() != dst.extent_elements()
        || src.format.bytes_per_element() != dst.format.bytes_per_element()
    {
        return Err(TilingError::Mismatch);
    }
    let src_needed = src.size_bytes()?;
    let dst_needed = dst.size_bytes()?;
    if src_bytes.len() < src_needed {
        return Err(TilingError::BufferTooSmall { needed: src_needed, got: src_bytes.len() });
    }
    if dst_bytes.len() < dst_needed {
        return Err(TilingError::BufferTooSmall { needed: dst_needed, got: dst_bytes.len() });
    }

    let src_map = Mapper::new(src);
    let dst_map = Mapper::new(dst);
    let (_, rows, depth) = src.extent_elements();
    let row_bytes = src.row_bytes();
    for z in 0..depth {
        for y in 0..rows {
            let mut x = 0;
            while x < row_bytes {
                let len = RUN_BYTES.min(row_bytes - x) as usize;
                let s = src_map.offset(x, y, z);
                let d = dst_map.offset(x, y, z);
                dst_bytes[d..d + len].copy_from_slice(&src_bytes[s..s + len]);
                x += RUN_BYTES;
            }
        }
    }
    Ok(())
}

/// Reads `src` into a tightly packed pitch-linear buffer (rows of `row_bytes()`, then
/// slices).
pub fn detile(src: &Surface, src_bytes: &[u8]) -> Result<Vec<u8>, TilingError> {
    let dst = Surface::packed(src.width, src.height, src.depth, src.format);
    let mut out = vec![0; dst.size_bytes()?];
    convert(src, src_bytes, &dst, &mut out)?;
    Ok(out)
}

/// Writes tightly packed pitch-linear `linear` into a new buffer laid out as `dst`
/// (padding zeroed).
pub fn tile(dst: &Surface, linear: &[u8]) -> Result<Vec<u8>, TilingError> {
    let src = Surface::packed(dst.width, dst.height, dst.depth, dst.format);
    let mut out = vec![0; dst.size_bytes()?];
    convert(&src, linear, dst, &mut out)?;
    Ok(out)
}

/// Offset calculator with the geometry precomputed once per surface.
enum Mapper {
    Pitch { stride: usize, rows: usize },
    Block(BlockLinear),
}

impl Mapper {
    fn new(surface: &Surface) -> Self {
        match surface.layout {
            Layout::Pitch { stride } => {
                Mapper::Pitch { stride: stride as usize, rows: surface.extent_elements().1 as usize }
            }
            Layout::BlockLinear { .. } => Mapper::Block(surface.block_linear().expect("block linear")),
        }
    }

    #[inline]
    fn offset(&self, x: u32, y: u32, z: u32) -> usize {
        match self {
            Mapper::Pitch { stride, rows } => (z as usize * rows + y as usize) * stride + x as usize,
            Mapper::Block(bl) => bl.offset(x, y, z),
        }
    }
}
