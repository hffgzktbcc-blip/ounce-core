//! NVIDIA block-linear addressing (Fermi and later, unchanged through Ampere for 2D
//! color surfaces).
//!
//! Sources (public):
//! - Linux `include/uapi/drm/drm_fourcc.h` ("Generalized Block Linear layout",
//!   `DRM_FORMAT_MOD_NVIDIA_BLOCK_LINEAR_2D`): GOBs are 64 B x 8 rows for Fermi through
//!   Turing+ (gob height field `g` = 0 or 2), stacked vertically 1 to 32 GOBs per block
//!   (log2 0..5), no block-width parameter, "16B x 2 lines sectors laid in Z-shape" inside
//!   the GOB. It points to Tegra X1 TRM chapter 20 "Pixel Memory Formats" for the full layout.
//! - NVIDIA Tegra X1 TRM, chapter 20: the in-GOB byte offset used by `gob_offset`
//!   (bits `x[3:0] | y[0]<<4 | x[4]<<5 | y[2:1]<<6 | x[5]<<8`), and blocks laid out
//!   row-major (X, then Y, then Z).
//! - Mesa NVK `nil` (`src/nouveau/nil/tiling.rs`, `image.rs`): GOB extent and the
//!   block height/depth clamp for small surfaces (`clamp_block_log2`). Not re-fetched
//!   from this environment; follows what `nil` is documented to do.
//!
//! Known gap: `drm_fourcc.h` also defines a "sector layout" bit-remap step below the block
//! linear swizzle. Layout 0 is Tegra K1 to Parker/TX2 (what the TRM formula describes);
//! layout 1 is pre-GB20x desktop, Tegra Xavier to Orin (the family T239 belongs to). The
//! exact difference is not spelled out in public headers, so `gob_offset` is the TRM
//! layout and is self-consistent across ounce-core, not a claim of byte-exact Orin-class
//! memory. Callers handing surfaces to real hardware would need that remap.
//!
//! Not covered: Turing+ depth/stencil "ZS" GOB kinds and compressed kinds. ounce-core
//! stores depth surfaces with the color GOB swizzle.

/// Bytes per GOB row.
pub const GOB_WIDTH_BYTES: u32 = 64;
/// Rows per GOB.
pub const GOB_HEIGHT: u32 = 8;
/// Bytes per GOB.
pub const GOB_BYTES: u32 = GOB_WIDTH_BYTES * GOB_HEIGHT;
/// Largest block height / depth exponent the hardware accepts (32 GOBs).
pub const MAX_BLOCK_LOG2: u8 = 5;

/// Byte offset of byte column `x` (0..64) and row `y` (0..8) inside one GOB.
///
/// Tegra X1 TRM chapter 20 block-linear GOB formula:
/// `(x/32)*256 + (y/2)*64 + ((x%32)/16)*32 + (y%2)*16 + x%16`.
#[inline]
pub const fn gob_offset(x: u32, y: u32) -> u32 {
    ((x % 64) / 32) * 256 + ((y % 8) / 2) * 64 + ((x % 32) / 16) * 32 + (y % 2) * 16 + (x % 16)
}

/// Shrinks block height/depth exponents so one block is not much larger than the surface,
/// the way NVK's `nil` clamps its tiling for small images. `rows` and `depth` are in
/// elements (rows of 4x4 blocks for BCn).
pub fn clamp_block_log2(rows: u32, depth: u32, mut height_log2: u8, mut depth_log2: u8) -> (u8, u8) {
    height_log2 = height_log2.min(MAX_BLOCK_LOG2);
    depth_log2 = depth_log2.min(MAX_BLOCK_LOG2);
    while height_log2 > 0 && rows <= GOB_HEIGHT << (height_log2 - 1) {
        height_log2 -= 1;
    }
    while depth_log2 > 0 && depth <= 1 << (depth_log2 - 1) {
        depth_log2 -= 1;
    }
    (height_log2, depth_log2)
}

/// Geometry of one block-linear surface level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockLinear {
    /// Surface width in GOBs (row bytes rounded up to 64).
    pub gobs_wide: u32,
    /// Surface height in blocks.
    pub blocks_high: u32,
    /// Surface depth in blocks.
    pub blocks_deep: u32,
    pub height_log2: u8,
    pub depth_log2: u8,
}

impl BlockLinear {
    /// `row_bytes` is one row of elements in bytes; `rows` and `depth` are in elements.
    pub fn new(row_bytes: u32, rows: u32, depth: u32, height_log2: u8, depth_log2: u8) -> Self {
        let block_rows = GOB_HEIGHT << height_log2;
        let block_depth = 1u32 << depth_log2;
        Self {
            gobs_wide: row_bytes.div_ceil(GOB_WIDTH_BYTES),
            blocks_high: rows.div_ceil(block_rows),
            blocks_deep: depth.div_ceil(block_depth),
            height_log2,
            depth_log2,
        }
    }

    /// Bytes per block.
    pub fn block_bytes(&self) -> usize {
        (GOB_BYTES as usize) << (self.height_log2 + self.depth_log2)
    }

    /// Row pitch of the padded surface, as a pitch-style stride (`gobs_wide * 64`).
    pub fn stride(&self) -> u32 {
        self.gobs_wide * GOB_WIDTH_BYTES
    }

    /// Total bytes, including padding to whole blocks.
    pub fn size(&self) -> usize {
        self.gobs_wide as usize * self.blocks_high as usize * self.blocks_deep as usize * self.block_bytes()
    }

    /// Byte offset of byte column `x`, row `y`, slice `z`.
    #[inline]
    pub fn offset(&self, x: u32, y: u32, z: u32) -> usize {
        let gob_x = (x / GOB_WIDTH_BYTES) as usize;
        let gob_row = y / GOB_HEIGHT;
        let block_y = (gob_row >> self.height_log2) as usize;
        let gob_y_in_block = (gob_row & ((1 << self.height_log2) - 1)) as usize;
        let block_z = (z >> self.depth_log2) as usize;
        let gob_z_in_block = (z & ((1 << self.depth_log2) - 1)) as usize;

        let block_index =
            (block_z * self.blocks_high as usize + block_y) * self.gobs_wide as usize + gob_x;
        let gob_in_block = (gob_z_in_block << self.height_log2) + gob_y_in_block;
        block_index * self.block_bytes()
            + gob_in_block * GOB_BYTES as usize
            + gob_offset(x % GOB_WIDTH_BYTES, y % GOB_HEIGHT) as usize
    }
}
