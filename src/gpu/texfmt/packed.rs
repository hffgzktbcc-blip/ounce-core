//! Plain, packed and depth/stencil formats: one texel per element.
//!
//! Each format is a list of fields from the least significant bit up. Every channel decodes with
//! the header's data type for that channel, except depth/stencil, whose types are fixed (depth is
//! unorm or float, stencil is uint).

use super::{half_to_f32, FormatDesc, NumType};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ch {
    R,
    G,
    B,
    A,
    /// Padding.
    X,
}

use Ch::*;

/// Fields from bit 0 up, as `(channel, bits)`.
fn fields(components: u32) -> Option<&'static [(Ch, u8)]> {
    Some(match components {
        0x01 => &[(R, 32), (G, 32), (B, 32), (A, 32)],
        0x03 => &[(R, 16), (G, 16), (B, 16), (A, 16)],
        0x04 => &[(R, 32), (G, 32)],
        0x07 => &[(R, 8), (G, 8), (B, 8), (X, 8)],
        0x08 => &[(R, 8), (G, 8), (B, 8), (A, 8)],
        0x09 => &[(R, 10), (G, 10), (B, 10), (A, 2)],
        0x0c => &[(R, 16), (G, 16)],
        0x0d => &[(R, 24), (G, 8)],
        0x0e => &[(R, 8), (G, 24)],
        0x0f => &[(R, 32)],
        0x12 => &[(R, 4), (G, 4), (B, 4), (A, 4)],
        0x13 => &[(R, 1), (G, 5), (B, 5), (A, 5)],
        0x14 => &[(R, 5), (G, 5), (B, 5), (A, 1)],
        0x15 => &[(R, 5), (G, 6), (B, 5)],
        0x16 => &[(R, 5), (G, 5), (B, 6)],
        0x18 => &[(R, 8), (G, 8)],
        0x1b => &[(R, 16)],
        0x1d => &[(R, 8)],
        0x1e => &[(R, 4), (G, 4)],
        _ => return None,
    })
}

/// Depth/stencil codes: (depth bits position, depth is float, stencil bit position) with depth in
/// R and stencil in G.
fn depth(components: u32) -> Option<(u32, bool, Option<u32>)> {
    Some(match components {
        0x29 => (8, false, Some(0)),   // Z24S8: S in 7:0, Z in 31:8
        0x2a => (0, false, None),      // X8Z24: Z in 23:0
        0x2b => (0, false, Some(24)),  // S8Z24: Z in 23:0, S in 31:24
        0x2f => (0, true, None),       // ZF32
        0x30 => (0, true, Some(32)),   // ZF32_X24S8: S in 39:32
        0x3a => (0, false, None),      // Z16
        _ => return None,
    })
}

/// Bytes per texel.
pub(super) fn bytes(components: u32) -> Option<u32> {
    Some(match components {
        0x20 | 0x21 => 4,
        0x29 | 0x2a | 0x2b | 0x2f => 4,
        0x30 => 8,
        0x3a => 2,
        c => fields(c)?.iter().map(|f| f.1 as u32).sum::<u32>() / 8,
    })
}

fn word(t: &[u8], at: u32, n: u32) -> u64 {
    super::bits_at(t, at, n)
}

pub(super) fn decode(desc: &FormatDesc, t: &[u8]) -> Option<[f32; 4]> {
    let v32 = || u32::from_le_bytes(t[0..4].try_into().unwrap());
    match desc.components {
        // E5B9G9R9_SHAREDEXP: 9-bit mantissas without an implicit one, bias 15.
        0x20 => {
            let v = v32();
            let scale = (2.0f32).powi((v >> 27) as i32 - 15 - 9);
            let m = |s: u32| ((v >> s) & 0x1ff) as f32 * scale;
            return Some([m(0), m(9), m(18), 1.0]);
        }
        // BF10GF11RF11: unsigned 11/11/10-bit floats.
        0x21 => {
            let v = v32();
            return Some([small_float(v & 0x7ff, 6), small_float((v >> 11) & 0x7ff, 6), small_float(v >> 22, 5), 1.0]);
        }
        _ => {}
    }
    if let Some((z_at, float, s_at)) = depth(desc.components) {
        let z = if float {
            f32::from_bits(word(t, 0, 32) as u32)
        } else if desc.components == 0x3a {
            word(t, 0, 16) as f32 / 65535.0
        } else {
            word(t, z_at, 24) as f32 / 16_777_215.0
        };
        let s = s_at.map_or(0, |at| word(t, at, 8) as u32);
        return Some([z, f32::from_bits(s), 0.0, 1.0]);
    }
    let fields = fields(desc.components)?;
    let int = desc.types[0].is_some_and(NumType::is_int);
    let one = if int { f32::from_bits(1) } else { 1.0 };
    let mut out = [0.0, 0.0, 0.0, one];
    let mut at = 0;
    for &(ch, n) in fields {
        let n = n as u32;
        if ch != X {
            let idx = ch as usize;
            out[idx] = channel(desc.types[idx]?, word(t, at, n), n)?;
        }
        at += n;
    }
    Some(out)
}

/// One channel of `n` bits.
fn channel(ty: NumType, v: u64, n: u32) -> Option<f32> {
    let max = ((1u64 << n) - 1) as f32;
    let signed = || ((v << (64 - n)) as i64) >> (64 - n);
    Some(match ty {
        NumType::Unorm => v as f32 / max,
        NumType::Snorm if n >= 2 => (signed() as f32 / ((1u64 << (n - 1)) - 1) as f32).max(-1.0),
        NumType::Uint => f32::from_bits(v as u32),
        NumType::Sint => f32::from_bits(signed() as i32 as u32),
        NumType::Float if n == 32 => f32::from_bits(v as u32),
        NumType::Float if n == 16 => half_to_f32(v as u16),
        _ => return None,
    })
}

/// Unsigned float with a 5-bit exponent (bias 15) and `man_bits` of mantissa.
pub(super) fn small_float(v: u32, man_bits: u32) -> f32 {
    let exp = (v >> man_bits) as i32;
    let man = (v & ((1 << man_bits) - 1)) as f32 / (1 << man_bits) as f32;
    match exp {
        0 => man * (2.0f32).powi(-14),
        31 if man == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        e => (1.0 + man) * (2.0f32).powi(e - 15),
    }
}
