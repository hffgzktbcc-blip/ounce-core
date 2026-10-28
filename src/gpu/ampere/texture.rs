//! Texture headers (TIC) and samplers (TSC): the pools the 3D class points at, the 32-byte entries
//! in them, and texel fetch/sample through the shared tiling library.
//!
//! Field positions come from NVIDIA open-gpu-doc `classes/3d/clc097tex.h` (`NVC097_TEXHEAD_*`,
//! `NVC097_TEXSAMP*`, MIT licensed). `MW(hi:lo)` there numbers bits across the whole 256-bit entry,
//! so bit `n` is bit `n % 32` of little-endian word `n / 32`. Pascal introduced this header layout
//! and Mesa NVK uses the same `clc097tex.h` definitions for Turing and Ampere (recalled, not
//! re-checked here: Mesa is not reachable from this container).
//!
//! Only mip level 0 of 1D/2D textures is read. The shaders cannot use these yet: `gpu::sass` does
//! not decode `TEX`/`TLD`, so this is the fetch path that decode will call.

use super::methods as m;
use super::render::SurfaceView;
use super::surface::addr40;
use super::GpuMemoryWriter;

/// Bytes per texture header and per sampler entry.
pub const TIC_ENTRY_BYTES: u64 = 32;
pub const TSC_ENTRY_BYTES: u64 = 32;

/// `SET_TEX_HEADER_POOL_*` / `SET_TEX_SAMPLER_POOL_*` as last written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pool {
    pub gpu_va: u64,
    pub maximum_index: u32,
}

impl Pool {
    pub(crate) fn header(regs: &[u32]) -> Self {
        let r = |method: u32| regs[method as usize / 4];
        Pool {
            gpu_va: addr40(r(m::SET_TEX_HEADER_POOL_A), r(m::SET_TEX_HEADER_POOL_B)),
            maximum_index: r(m::SET_TEX_HEADER_POOL_C) & 0x3f_ffff,
        }
    }

    pub(crate) fn sampler(regs: &[u32]) -> Self {
        let r = |method: u32| regs[method as usize / 4];
        Pool {
            gpu_va: addr40(r(m::SET_TEX_SAMPLER_POOL_A), r(m::SET_TEX_SAMPLER_POOL_B)),
            maximum_index: r(m::SET_TEX_SAMPLER_POOL_C) & 0xf_ffff,
        }
    }

    /// Address of entry `index`, if the pool is set and the index is within `MAXIMUM_INDEX`.
    pub fn entry_va(&self, index: u32, entry_bytes: u64) -> Option<u64> {
        (self.gpu_va != 0 && index <= self.maximum_index).then(|| self.gpu_va + index as u64 * entry_bytes)
    }
}

/// `HEADER_VERSION` (MW(87:85)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderVersion {
    OneDBuffer,
    PitchColorKey,
    Pitch,
    BlockLinear,
    BlockLinearColorKey,
    Other(u32),
}

/// `TEXTURE_TYPE` (MW(154:151)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureType {
    OneD,
    TwoD,
    ThreeD,
    Cubemap,
    OneDArray,
    TwoDArray,
    OneDBuffer,
    TwoDNoMipmap,
    CubemapArray,
    Other(u32),
}

impl TextureType {
    fn from_raw(v: u32) -> Self {
        use TextureType::*;
        [OneD, TwoD, ThreeD, Cubemap, OneDArray, TwoDArray, OneDBuffer, TwoDNoMipmap, CubemapArray]
            .get(v as usize)
            .copied()
            .unwrap_or(Other(v))
    }
}

/// `*_DATA_TYPE` (`NUM_SNORM` 1 .. `NUM_FLOAT` 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Snorm,
    Unorm,
    Sint,
    Uint,
    Float,
    Other(u32),
}

impl DataType {
    fn from_raw(v: u32) -> Self {
        match v {
            1 | 5 => Self::Snorm,
            2 | 6 => Self::Unorm,
            3 => Self::Sint,
            4 => Self::Uint,
            7 => Self::Float,
            other => Self::Other(other),
        }
    }
}

/// `X_SOURCE`..`W_SOURCE`: where each output channel comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swizzle {
    Zero,
    R,
    G,
    B,
    A,
    OneInt,
    OneFloat,
}

impl Swizzle {
    fn from_raw(v: u32) -> Self {
        match v {
            2 => Self::R,
            3 => Self::G,
            4 => Self::B,
            5 => Self::A,
            6 => Self::OneInt,
            7 => Self::OneFloat,
            _ => Self::Zero,
        }
    }
}

/// Memory layout of a texture's level 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureLayout {
    /// `GOBS_PER_BLOCK_HEIGHT` / `_DEPTH` as log2 counts.
    BlockLinear { block_height_log2: u8, block_depth_log2: u8 },
    Pitch { pitch_bytes: u32 },
}

/// One decoded texture header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureHeader {
    pub raw: [u32; 8],
    pub version: HeaderVersion,
    /// `COMPONENTS` (`COMPONENTS_SIZES_*`), e.g. `0x08` = A8B8G8R8.
    pub components: u32,
    pub data_types: [DataType; 4],
    pub swizzle: [Swizzle; 4],
    pub gpu_va: u64,
    pub layout: TextureLayout,
    pub texture_type: TextureType,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub max_mip_level: u32,
    pub srgb: bool,
    pub normalized_coords: bool,
}

/// `COMPONENTS_SIZES_*` values this module can read texels of, with their byte size.
fn component_bytes(components: u32) -> Option<u32> {
    Some(match components {
        0x01 => 16, // R32_G32_B32_A32
        0x03 => 8,  // R16_G16_B16_A16
        0x04 => 8,  // R32_G32
        0x07 | 0x08 | 0x09 | 0x0c | 0x0f => 4, // X8B8G8R8, A8B8G8R8, A2B10G10R10, R16_G16, R32
        0x15 | 0x18 | 0x1b => 2, // B5G6R5, G8R8, R16
        0x1d => 1,  // R8
        _ => return None,
    })
}

fn bits(words: &[u32; 8], hi: u32, lo: u32) -> u32 {
    // Fields used here never cross a word boundary.
    debug_assert_eq!(hi / 32, lo / 32);
    let w = words[(lo / 32) as usize];
    let width = hi - lo + 1;
    let v = w >> (lo % 32);
    if width == 32 { v } else { v & ((1 << width) - 1) }
}

impl TextureHeader {
    pub fn parse(bytes: &[u8; 32]) -> Self {
        let mut w = [0u32; 8];
        for (i, word) in w.iter_mut().enumerate() {
            *word = u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().expect("4 bytes"));
        }
        let version = match bits(&w, 87, 85) {
            0 => HeaderVersion::OneDBuffer,
            1 => HeaderVersion::PitchColorKey,
            2 => HeaderVersion::Pitch,
            3 => HeaderVersion::BlockLinear,
            4 => HeaderVersion::BlockLinearColorKey,
            other => HeaderVersion::Other(other),
        };
        let pitch = matches!(version, HeaderVersion::Pitch | HeaderVersion::PitchColorKey);
        // ADDRESS_BITS48TO32 MW(80:64) in both layouts; the low part is BITS31TO9 MW(63:41) for
        // block linear and BITS31TO5 MW(63:37) for pitch.
        let upper = (bits(&w, 80, 64) as u64) << 32;
        let gpu_va = if pitch { upper | (w[1] & !0x1f) as u64 } else { upper | (w[1] & !0x1ff) as u64 };
        let layout = if pitch {
            // PITCH_BITS20TO5 MW(111:96), PITCH_BIT21 MW(145:145).
            TextureLayout::Pitch { pitch_bytes: (bits(&w, 111, 96) << 5) | (bits(&w, 145, 145) << 21) }
        } else {
            TextureLayout::BlockLinear {
                block_height_log2: bits(&w, 101, 99) as u8,
                block_depth_log2: bits(&w, 104, 102) as u8,
            }
        };
        // DEPTH_MINUS_ONE_BIT14 MW(145:145) exists only in the block-linear layout (pitch reuses the
        // bit for PITCH_BIT21).
        let depth_bit14 = if pitch { 0 } else { bits(&w, 145, 145) << 14 };
        TextureHeader {
            raw: w,
            version,
            components: bits(&w, 6, 0),
            data_types: [bits(&w, 9, 7), bits(&w, 12, 10), bits(&w, 15, 13), bits(&w, 18, 16)].map(DataType::from_raw),
            swizzle: [bits(&w, 21, 19), bits(&w, 24, 22), bits(&w, 27, 25), bits(&w, 30, 28)].map(Swizzle::from_raw),
            gpu_va,
            layout,
            texture_type: TextureType::from_raw(bits(&w, 154, 151)),
            width: bits(&w, 144, 128) + 1,
            height: (bits(&w, 175, 160) | (bits(&w, 146, 146) << 16)) + 1,
            depth: (bits(&w, 189, 176) | depth_bit14) + 1,
            max_mip_level: bits(&w, 127, 124),
            srgb: bits(&w, 150, 150) == 1,
            normalized_coords: bits(&w, 191, 191) == 1,
        }
    }

    pub fn bytes_per_texel(&self) -> Option<u32> {
        component_bytes(self.components)
    }

    /// Level 0, slice 0 as a surface view, or `None` for formats or layouts this module cannot read.
    pub(crate) fn view(&self) -> Option<SurfaceView> {
        let bpp = self.bytes_per_texel()?;
        match self.layout {
            TextureLayout::BlockLinear { block_height_log2, block_depth_log2 } => {
                SurfaceView::block_linear(self.gpu_va, self.width, self.height, bpp, block_height_log2, block_depth_log2)
            }
            TextureLayout::Pitch { pitch_bytes } => SurfaceView::pitch(self.gpu_va, self.width, self.height, bpp, pitch_bytes),
        }
    }

    /// Decodes one texel's raw bytes to RGBA (before the header swizzle).
    fn decode(&self, t: &[u8]) -> Option<[f32; 4]> {
        let unorm8 = |b: u8| b as f32 / 255.0;
        let f32_at = |i: usize| f32::from_le_bytes(t[4 * i..4 * i + 4].try_into().unwrap());
        let u16_at = |i: usize| u16::from_le_bytes(t[2 * i..2 * i + 2].try_into().unwrap());
        let float = self.data_types[0] == DataType::Float;
        Some(match self.components {
            0x01 if float => [f32_at(0), f32_at(1), f32_at(2), f32_at(3)],
            0x04 if float => [f32_at(0), f32_at(1), 0.0, 1.0],
            0x0f if float => [f32_at(0), 0.0, 0.0, 1.0],
            0x03 if self.data_types[0] == DataType::Unorm => {
                [0, 1, 2, 3].map(|i| u16_at(i) as f32 / 65535.0)
            }
            0x08 => [unorm8(t[0]), unorm8(t[1]), unorm8(t[2]), unorm8(t[3])],
            0x07 => [unorm8(t[0]), unorm8(t[1]), unorm8(t[2]), 1.0],
            0x09 => {
                let v = u32::from_le_bytes(t[0..4].try_into().unwrap());
                let c = |s: u32, n: u32| ((v >> s) & ((1 << n) - 1)) as f32 / ((1 << n) - 1) as f32;
                [c(0, 10), c(10, 10), c(20, 10), c(30, 2)]
            }
            0x0c if self.data_types[0] == DataType::Unorm => [u16_at(0) as f32 / 65535.0, u16_at(1) as f32 / 65535.0, 0.0, 1.0],
            // Component names list the most significant field first: B5G6R5 has R in bits 4:0.
            0x15 => {
                let v = u16_at(0) as u32;
                [(v & 0x1f) as f32 / 31.0, ((v >> 5) & 0x3f) as f32 / 63.0, (v >> 11) as f32 / 31.0, 1.0]
            }
            0x18 => [unorm8(t[0]), unorm8(t[1]), 0.0, 1.0],
            0x1b if self.data_types[0] == DataType::Unorm => [u16_at(0) as f32 / 65535.0, 0.0, 0.0, 1.0],
            0x1d => [unorm8(t[0]), 0.0, 0.0, 1.0],
            _ => return None,
        })
    }

    fn apply_swizzle(&self, c: [f32; 4]) -> [f32; 4] {
        self.swizzle.map(|s| match s {
            Swizzle::Zero => 0.0,
            Swizzle::R => c[0],
            Swizzle::G => c[1],
            Swizzle::B => c[2],
            Swizzle::A => c[3],
            Swizzle::OneInt | Swizzle::OneFloat => 1.0,
        })
    }

    /// Texel `(x, y)` of level 0 after the header swizzle; `None` if out of range, the format is
    /// unsupported or the memory cannot be read.
    pub fn fetch(&self, x: u32, y: u32, mem: &mut dyn GpuMemoryWriter) -> Option<[f32; 4]> {
        let view = self.view()?;
        if x >= view.width || y >= view.height {
            return None;
        }
        let mut t = [0u8; 16];
        let t = &mut t[..view.bpp as usize];
        mem.read(view.texel_va(x, y), t).then_some(())?;
        self.decode(t).map(|c| self.apply_swizzle(c))
    }

    /// Samples level 0 at `(u, v)` with `sampler`'s filter and wrap modes. Coordinates are
    /// normalized when the header says so, texel units otherwise.
    pub fn sample(&self, sampler: &Sampler, u: f32, v: f32, mem: &mut dyn GpuMemoryWriter) -> Option<[f32; 4]> {
        let (w, h) = (self.width as f32, self.height as f32);
        let (x, y) = if self.normalized_coords { (u * w, v * h) } else { (u, v) };
        let mut texel = |tx: i64, ty: i64| -> Option<[f32; 4]> {
            match (wrap(sampler.address[0], tx, self.width), wrap(sampler.address[1], ty, self.height)) {
                (Some(cx), Some(cy)) => self.fetch(cx, cy, mem),
                _ => Some(sampler.border),
            }
        };
        if sampler.mag_filter == Filter::Linear {
            let (fx, fy) = (x - 0.5, y - 0.5);
            let (x0, y0) = (fx.floor(), fy.floor());
            let (ax, ay) = (fx - x0, fy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let (c00, c10, c01, c11) = (texel(x0, y0)?, texel(x0 + 1, y0)?, texel(x0, y0 + 1)?, texel(x0 + 1, y0 + 1)?);
            Some([0, 1, 2, 3].map(|i| {
                let top = c00[i] + (c10[i] - c00[i]) * ax;
                let bottom = c01[i] + (c11[i] - c01[i]) * ax;
                top + (bottom - top) * ay
            }))
        } else {
            texel(x.floor() as i64, y.floor() as i64)
        }
    }
}

/// `ADDRESS_U/V/P` wrap modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    Wrap,
    Mirror,
    ClampToEdge,
    Border,
    ClampOgl,
    MirrorOnceClampToEdge,
    MirrorOnceBorder,
    MirrorOnceClampOgl,
}

/// `MAG_FILTER` / `MIN_FILTER` (POINT 1, LINEAR 2, others kept as Other).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Point,
    Linear,
    Other(u32),
}

/// One decoded sampler entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sampler {
    pub raw: [u32; 8],
    /// U, V, P.
    pub address: [AddressMode; 3],
    pub mag_filter: Filter,
    pub min_filter: Filter,
    /// `MIP_FILTER` 7:6 of word 1.
    pub mip_filter: u32,
    /// `BORDER_COLOR_R/G/B/A` (words 4..7, floats).
    pub border: [f32; 4],
}

impl Sampler {
    pub fn parse(bytes: &[u8; 32]) -> Self {
        let mut w = [0u32; 8];
        for (i, word) in w.iter_mut().enumerate() {
            *word = u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().expect("4 bytes"));
        }
        let addr = |v: u32| {
            use AddressMode::*;
            [Wrap, Mirror, ClampToEdge, Border, ClampOgl, MirrorOnceClampToEdge, MirrorOnceBorder, MirrorOnceClampOgl]
                [(v & 7) as usize]
        };
        let filter = |v: u32| match v {
            1 => Filter::Point,
            2 => Filter::Linear,
            other => Filter::Other(other),
        };
        Sampler {
            raw: w,
            // TEXSAMP0: ADDRESS_U 2:0, ADDRESS_V 5:3, ADDRESS_P 8:6.
            address: [addr(w[0]), addr(w[0] >> 3), addr(w[0] >> 6)],
            // TEXSAMP1: MAG_FILTER 2:0, MIN_FILTER 5:4, MIP_FILTER 7:6.
            mag_filter: filter(w[1] & 7),
            min_filter: filter((w[1] >> 4) & 3),
            mip_filter: (w[1] >> 6) & 3,
            border: [f32::from_bits(w[4]), f32::from_bits(w[5]), f32::from_bits(w[6]), f32::from_bits(w[7])],
        }
    }
}

/// Applies a wrap mode to integer texel coordinate `c` in `[0, size)`; `None` means border colour.
fn wrap(mode: AddressMode, c: i64, size: u32) -> Option<u32> {
    let n = size as i64;
    let mirror = |c: i64| {
        let period = c.rem_euclid(2 * n);
        if period < n { period } else { 2 * n - 1 - period }
    };
    let v = match mode {
        AddressMode::Wrap => c.rem_euclid(n),
        AddressMode::Mirror => mirror(c),
        AddressMode::ClampToEdge | AddressMode::ClampOgl => c.clamp(0, n - 1),
        AddressMode::Border => {
            if c < 0 || c >= n {
                return None;
            }
            c
        }
        AddressMode::MirrorOnceClampToEdge | AddressMode::MirrorOnceClampOgl => {
            let a = if c < 0 { -c - 1 } else { c };
            a.clamp(0, n - 1)
        }
        AddressMode::MirrorOnceBorder => {
            let a = if c < 0 { -c - 1 } else { c };
            if a >= n {
                return None;
            }
            a
        }
    };
    Some(v as u32)
}

/// Reads one 32-byte pool entry.
pub(crate) fn read_entry(mem: &mut dyn GpuMemoryWriter, va: u64) -> Option<[u8; 32]> {
    let mut b = [0u8; 32];
    mem.read(va, &mut b).then_some(b)
}
