//! Render target descriptors handed to the present side (Track A detiles block-linear surfaces).
//!
//! These describe raw guest memory as the GPU state names it. Nothing here detiles or converts:
//! the bytes at `gpu_va` stay in the layout `layout` describes.

use super::methods as m;

/// Memory layout of a surface, from `SET_COLOR_TARGET_MEMORY` / `SET_ZT_BLOCK_SIZE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceLayout {
    /// Block linear. Block dimensions are log2 counts of GOBs, exactly as the class fields hold them
    /// (`BLOCK_WIDTH` 3:0, `BLOCK_HEIGHT` 7:4, `BLOCK_DEPTH` 11:8; `0` = one GOB).
    BlockLinear { block_width_log2: u8, block_height_log2: u8, block_depth_log2: u8 },
    /// Pitch linear. `pitch_bytes` is the `WIDTH` field: for pitch color targets the driver programs
    /// the row stride in bytes there (the convention Mesa NVK and nouveau use).
    Pitch { pitch_bytes: u32 },
}

/// `SET_COLOR_TARGET_FORMAT` values from `clc797.h` that step 1 knows the size of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorFormat {
    Disabled,
    Rf32Gf32Bf32Af32,
    R16G16B16A16,
    A8R8G8B8,
    A2B10G10R10,
    A8B8G8R8,
    X8B8G8R8,
    R5G6B5,
    /// Any other value, kept raw.
    Other(u32),
}

impl ColorFormat {
    pub fn from_raw(v: u32) -> Self {
        match v {
            0x00 => Self::Disabled,
            0xc0 => Self::Rf32Gf32Bf32Af32,
            0xc6 => Self::R16G16B16A16,
            0xcf => Self::A8R8G8B8,
            0xd1 => Self::A2B10G10R10,
            0xd5 => Self::A8B8G8R8,
            0xf9 => Self::X8B8G8R8,
            0xe8 => Self::R5G6B5,
            other => Self::Other(other),
        }
    }

    pub fn bytes_per_pixel(self) -> Option<u32> {
        match self {
            Self::Rf32Gf32Bf32Af32 => Some(16),
            Self::R16G16B16A16 => Some(8),
            Self::A8R8G8B8 | Self::A2B10G10R10 | Self::A8B8G8R8 | Self::X8B8G8R8 => Some(4),
            Self::R5G6B5 => Some(2),
            Self::Disabled | Self::Other(_) => None,
        }
    }
}

/// One bound color target: what the present side needs to read and detile it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorTarget {
    /// Render target slot, 0..8.
    pub index: u8,
    /// 40-bit GPU virtual address (`OFFSET_UPPER` 7:0 + `OFFSET_LOWER`).
    pub gpu_va: u64,
    /// `WIDTH` field as written: texels when block linear, row pitch in bytes when pitch linear.
    pub width_field: u32,
    pub height: u32,
    pub format: ColorFormat,
    pub layout: SurfaceLayout,
    pub third_dimension: u32,
    pub array_pitch: u32,
    pub layer: u32,
}

impl ColorTarget {
    /// Width in texels, when it can be known from the state alone.
    pub fn width_texels(&self) -> Option<u32> {
        match self.layout {
            SurfaceLayout::BlockLinear { .. } => Some(self.width_field),
            SurfaceLayout::Pitch { pitch_bytes } => self.format.bytes_per_pixel().map(|bpp| pitch_bytes / bpp),
        }
    }
}

/// `SET_ZT_FORMAT` values from `clc797.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthFormat {
    Z16,
    Z24S8,
    X8Z24,
    S8Z24,
    Zf32,
    Zf32X24S8,
    Other(u32),
}

impl DepthFormat {
    pub fn from_raw(v: u32) -> Self {
        match v {
            0x13 => Self::Z16,
            0x14 => Self::Z24S8,
            0x15 => Self::X8Z24,
            0x16 => Self::S8Z24,
            0x0a => Self::Zf32,
            0x19 => Self::Zf32X24S8,
            other => Self::Other(other),
        }
    }
}

/// The bound depth/stencil target. Depth targets are always block linear on this class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthTarget {
    pub gpu_va: u64,
    pub width: u32,
    pub height: u32,
    pub format: DepthFormat,
    pub layout: SurfaceLayout,
    pub array_pitch: u32,
    pub layer: u32,
}

pub(crate) fn block_linear(block_size: u32) -> SurfaceLayout {
    SurfaceLayout::BlockLinear {
        block_width_log2: (block_size & 0xf) as u8,
        block_height_log2: ((block_size >> 4) & 0xf) as u8,
        block_depth_log2: ((block_size >> 8) & 0xf) as u8,
    }
}

pub(crate) fn color_target(regs: &[u32], index: usize) -> ColorTarget {
    let base = (index as u32 * m::COLOR_TARGET_STRIDE) as usize / 4;
    let r = |method: u32| regs[method as usize / 4 + base];
    let memory = r(m::SET_COLOR_TARGET_MEMORY);
    let width_field = r(m::SET_COLOR_TARGET_WIDTH) & 0x0fff_ffff;
    let layout = if (memory >> 12) & 1 == 1 {
        SurfaceLayout::Pitch { pitch_bytes: width_field }
    } else {
        block_linear(memory)
    };
    ColorTarget {
        index: index as u8,
        gpu_va: addr40(r(m::SET_COLOR_TARGET_A), r(m::SET_COLOR_TARGET_B)),
        width_field,
        height: r(m::SET_COLOR_TARGET_HEIGHT) & 0x1_ffff,
        format: ColorFormat::from_raw(r(m::SET_COLOR_TARGET_FORMAT)),
        layout,
        third_dimension: r(m::SET_COLOR_TARGET_THIRD_DIMENSION),
        array_pitch: r(m::SET_COLOR_TARGET_ARRAY_PITCH),
        layer: r(m::SET_COLOR_TARGET_LAYER),
    }
}

pub(crate) fn depth_target(regs: &[u32]) -> DepthTarget {
    let r = |method: u32| regs[method as usize / 4];
    DepthTarget {
        gpu_va: addr40(r(m::SET_ZT_A), r(m::SET_ZT_B)),
        width: r(m::SET_ZT_SIZE_A) & 0x0fff_ffff,
        height: r(m::SET_ZT_SIZE_B) & 0x3_ffff,
        format: DepthFormat::from_raw(r(m::SET_ZT_FORMAT) & 0x1f),
        layout: block_linear(r(m::SET_ZT_BLOCK_SIZE)),
        array_pitch: r(m::SET_ZT_ARRAY_PITCH),
        layer: r(m::SET_ZT_LAYER) & 0xffff,
    }
}

/// Joins an `_A` (upper 7:0) / `_B` (lower 31:0) pair into a 40-bit GPU address.
pub(crate) fn addr40(upper: u32, lower: u32) -> u64 {
    (((upper & 0xff) as u64) << 32) | lower as u64
}
