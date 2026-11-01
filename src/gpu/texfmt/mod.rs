//! Texel decoding for every texture format a texture header (TIC) can name: plain and packed
//! formats, depth/stencil, BC1-BC7, ETC2/EAC and 2D ASTC.
//!
//! The format codes are `NVC097_TEXHEAD_*_COMPONENTS_SIZES_*` from NVIDIA open-gpu-doc
//! `classes/3d/clc097tex.h` (MIT licensed). Component names there list the most significant field
//! first, so `A1B5G5R5` keeps R in bits 4:0 and A in bit 15. The compressed formats follow the public
//! Khronos Data Format Specification 1.3 (BC1-BC7, ETC2, EAC) and the Khronos ASTC specification in
//! the same document. Nothing here is derived from vendor firmware or driver binaries.
//!
//! The entry point is [`decode_texel`]: one element's bytes in, one RGBA texel out. An element is
//! one texel for plain formats and one compressed block for the rest; [`Element`] says how big it
//! is so callers can lay elements out with `gpu::tiling`.
//!
//! Results are `f32` per channel. Integer channels (`NUM_SINT`/`NUM_UINT`) return the integer's bit
//! pattern through `f32::from_bits`, because shaders read those registers as integers.

mod astc;
mod bc;
mod etc;
mod packed;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod vectors;

/// `*_DATA_TYPE` of one channel (`NUM_SNORM` 1 .. `NUM_FLOAT` 7; the `FORCE_FP16` variants only
/// change precision and decode like their plain forms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumType {
    Snorm,
    Unorm,
    Sint,
    Uint,
    Float,
}

impl NumType {
    fn is_int(self) -> bool {
        matches!(self, Self::Sint | Self::Uint)
    }
}

/// What a texture header says about its texel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatDesc {
    /// `COMPONENTS_SIZES` code.
    pub components: u32,
    /// `R/G/B/A_DATA_TYPE`, `None` for reserved values.
    pub types: [Option<NumType>; 4],
    /// `SRGB_CONVERSION`: R, G and B of unorm data go through the sRGB transfer function.
    pub srgb: bool,
}

/// Size of one element: `block_w x block_h` texels stored in `bytes` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Element {
    pub block_w: u32,
    pub block_h: u32,
    pub bytes: u32,
}

impl Element {
    const fn texel(bytes: u32) -> Self {
        Element { block_w: 1, block_h: 1, bytes }
    }

    const fn block(block_w: u32, block_h: u32, bytes: u32) -> Self {
        Element { block_w, block_h, bytes }
    }

    pub fn is_block(&self) -> bool {
        self.block_w > 1 || self.block_h > 1
    }
}

// COMPONENTS_SIZES codes (clc097tex.h) for the compressed formats.
pub const DXT1: u32 = 0x24;
pub const DXT23: u32 = 0x25;
pub const DXT45: u32 = 0x26;
pub const DXN1: u32 = 0x27;
pub const DXN2: u32 = 0x28;
pub const BC6H_SF16: u32 = 0x10;
pub const BC6H_UF16: u32 = 0x11;
pub const BC7U: u32 = 0x17;
pub const ETC2_RGB: u32 = 0x06;
pub const ETC2_RGB_PTA: u32 = 0x0a;
pub const ETC2_RGBA: u32 = 0x0b;
pub const EAC: u32 = 0x19;
pub const EACX2: u32 = 0x1a;

/// ASTC 2D footprints by `COMPONENTS_SIZES` code.
const ASTC: [(u32, u32, u32); 14] = [
    (0x40, 4, 4),
    (0x50, 5, 4),
    (0x41, 5, 5),
    (0x51, 6, 5),
    (0x42, 6, 6),
    (0x55, 8, 5),
    (0x52, 8, 6),
    (0x44, 8, 8),
    (0x56, 10, 5),
    (0x57, 10, 6),
    (0x53, 10, 8),
    (0x45, 10, 10),
    (0x54, 12, 10),
    (0x46, 12, 12),
];

/// ASTC block footprint for a `COMPONENTS_SIZES` code.
pub fn astc_footprint(components: u32) -> Option<(u32, u32)> {
    ASTC.iter().find(|a| a.0 == components).map(|a| (a.1, a.2))
}

/// Element size for a `COMPONENTS_SIZES` code, or `None` if it cannot be decoded.
pub fn element(components: u32) -> Option<Element> {
    if let Some((w, h)) = astc_footprint(components) {
        return Some(Element::block(w, h, 16));
    }
    Some(match components {
        DXT1 | DXN1 | ETC2_RGB | ETC2_RGB_PTA | EAC => Element::block(4, 4, 8),
        DXT23 | DXT45 | DXN2 | BC6H_SF16 | BC6H_UF16 | BC7U | ETC2_RGBA | EACX2 => Element::block(4, 4, 16),
        c => Element::texel(packed::bytes(c)?),
    })
}

/// Decodes texel `(dx, dy)` of one element (`dx < block_w`, `dy < block_h`) to RGBA, before the
/// header swizzle. `elem` must hold at least [`Element::bytes`] bytes. `None` for formats or data
/// type combinations this module does not decode.
pub fn decode_texel(desc: &FormatDesc, elem: &[u8], dx: u32, dy: u32) -> Option<[f32; 4]> {
    let e = element(desc.components)?;
    let elem = elem.get(..e.bytes as usize)?;
    if dx >= e.block_w || dy >= e.block_h {
        return None;
    }
    let i = (dy * e.block_w + dx) as usize;
    let r = desc.types[0];
    let snorm = r == Some(NumType::Snorm);
    let c = match desc.components {
        DXT1 => bc::bc1(elem, i, true),
        DXT23 => bc::bc2(elem, i),
        DXT45 => bc::bc3(elem, i),
        DXN1 => [bc::bc4(elem, i, snorm), 0.0, 0.0, 1.0],
        DXN2 => [bc::bc4(elem, i, snorm), bc::bc4(&elem[8..], i, snorm), 0.0, 1.0],
        BC6H_UF16 => bc::bc6h(elem, i, false),
        BC6H_SF16 => bc::bc6h(elem, i, true),
        BC7U => bc::bc7(elem, i),
        ETC2_RGB => etc::etc2_rgb(elem, i, false),
        ETC2_RGB_PTA => etc::etc2_rgb(elem, i, true),
        ETC2_RGBA => {
            let mut c = etc::etc2_rgb(&elem[8..], i, false);
            c[3] = etc::etc2_alpha(elem, i);
            c
        }
        EAC => [etc::eac(elem, i, snorm), 0.0, 0.0, 1.0],
        EACX2 => [etc::eac(elem, i, snorm), etc::eac(&elem[8..], i, snorm), 0.0, 1.0],
        c if astc_footprint(c).is_some() => astc::decode(elem, e.block_w, e.block_h, i),
        _ => return packed::decode(desc, elem).map(|c| srgb(desc, c, |ch| desc.types[ch] == Some(NumType::Unorm))),
    };
    // Compressed formats are unorm unless they say otherwise; sRGB applies to the colour channels.
    let unorm = !matches!(desc.components, BC6H_UF16 | BC6H_SF16) && !snorm;
    Some(srgb(desc, c, |_| unorm))
}

fn srgb(desc: &FormatDesc, mut c: [f32; 4], unorm: impl Fn(usize) -> bool) -> [f32; 4] {
    if desc.srgb {
        for (ch, v) in c.iter_mut().enumerate().take(3) {
            if unorm(ch) {
                *v = srgb_to_linear(*v);
            }
        }
    }
    c
}

/// The sRGB EOTF (IEC 61966-2-1).
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// IEEE 754 binary16 to f32.
pub(crate) fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let man = (h & 0x3ff) as f32;
    sign * match exp {
        0 => man * (2.0f32).powi(-24),
        31 if man == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        e => (1.0 + man / 1024.0) * (2.0f32).powi(e - 15),
    }
}

/// Reads `n` bits (n <= 64) starting at bit `pos` of a little-endian byte string.
pub(crate) fn bits_at(data: &[u8], pos: u32, n: u32) -> u64 {
    let mut v = 0u64;
    for k in 0..n {
        let b = pos + k;
        v |= (((data[(b / 8) as usize] >> (b % 8)) & 1) as u64) << k;
    }
    v
}
