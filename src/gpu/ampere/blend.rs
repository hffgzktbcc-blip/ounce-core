//! Color blending for the software draw path.
//!
//! State comes from open-gpu-doc `clc797.h` (same offsets in `clb197.h`): `SET_BLEND(i)` enables
//! target `i`; `SET_BLEND_STATE_PER_TARGET` picks between the shared `SET_BLEND_COLOR_*` /
//! `SET_BLEND_ALPHA_*` registers and the per-target `SET_BLEND_PER_TARGET_*(j)` block. When
//! `SEPARATE_FOR_ALPHA` is off, alpha uses the color equation. Ops and coefficients take both the
//! OGL and the D3D encodings the header lists. Blend constants are `SET_BLEND_CONST_*` floats.
//!
//! Semantics follow the usual GL/D3D blend equation: unorm targets clamp the source and constant
//! to [0, 1] first, float targets do not. Dual-source (`SRC1`) and D3D `BOTH*` coefficients are
//! rejected rather than guessed.

use super::methods as m;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Factor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
    DstColor,
    OneMinusDstColor,
    SrcAlphaSaturate,
    ConstantColor,
    OneMinusConstantColor,
    ConstantAlpha,
    OneMinusConstantAlpha,
}

fn op(v: u32) -> Result<Op, &'static str> {
    Ok(match v {
        0x8006 | 1 => Op::Add,
        0x800a | 2 => Op::Subtract,
        0x800b | 3 => Op::ReverseSubtract,
        0x8007 | 4 => Op::Min,
        0x8008 | 5 => Op::Max,
        _ => return Err("blend op"),
    })
}

fn factor(v: u32) -> Result<Factor, &'static str> {
    use Factor::*;
    Ok(match v {
        0x4000 | 0x01 => Zero,
        0x4001 | 0x02 => One,
        0x4300 | 0x03 => SrcColor,
        0x4301 | 0x04 => OneMinusSrcColor,
        0x4302 | 0x05 => SrcAlpha,
        0x4303 | 0x06 => OneMinusSrcAlpha,
        0x4304 | 0x07 => DstAlpha,
        0x4305 | 0x08 => OneMinusDstAlpha,
        0x4306 | 0x09 => DstColor,
        0x4307 | 0x0a => OneMinusDstColor,
        0x4308 | 0x0b => SrcAlphaSaturate,
        0xc001 | 0x0e => ConstantColor,
        0xc002 | 0x0f => OneMinusConstantColor,
        0xc003 => ConstantAlpha,
        0xc004 => OneMinusConstantAlpha,
        0xc900..=0xc903 | 0x10..=0x13 => return Err("dual-source blending"),
        0x0c | 0x0d => return Err("D3D BOTHSRCALPHA blend coefficients"),
        _ => return Err("blend coefficient"),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Equation {
    op: Op,
    src: Factor,
    dst: Factor,
}

/// Blend state for one target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Blend {
    color: Equation,
    alpha: Equation,
    constant: [f32; 4],
}

impl Blend {
    /// State for `target`, `None` when its blending is off.
    pub(crate) fn for_target(regs: &[u32], target: u8) -> Result<Option<Self>, &'static str> {
        let r = |method: u32| regs[(method / 4) as usize];
        if r(m::SET_BLEND + 4 * target as u32) & 1 == 0 {
            return Ok(None);
        }
        // separate, color op/src/dst, alpha op/src/dst
        let raw: [u32; 7] = if r(m::SET_BLEND_STATE_PER_TARGET) & 1 == 1 {
            let base = m::SET_BLEND_PER_TARGET + target as u32 * m::BLEND_PER_TARGET_STRIDE;
            [0, 4, 8, 12, 16, 20, 24].map(|o| r(base + o))
        } else {
            [
                m::SET_BLEND_SEPARATE_FOR_ALPHA,
                m::SET_BLEND_COLOR_OP,
                m::SET_BLEND_COLOR_SOURCE_COEFF,
                m::SET_BLEND_COLOR_DEST_COEFF,
                m::SET_BLEND_ALPHA_OP,
                m::SET_BLEND_ALPHA_SOURCE_COEFF,
                m::SET_BLEND_ALPHA_DEST_COEFF,
            ]
            .map(r)
        };
        let color = Equation { op: op(raw[1])?, src: factor(raw[2])?, dst: factor(raw[3])? };
        let alpha = if raw[0] & 1 == 1 { Equation { op: op(raw[4])?, src: factor(raw[5])?, dst: factor(raw[6])? } } else { color };
        let constant = [m::SET_BLEND_CONST_RED, m::SET_BLEND_CONST_GREEN, m::SET_BLEND_CONST_BLUE, m::SET_BLEND_CONST_ALPHA]
            .map(|c| f32::from_bits(r(c)));
        Ok(Some(Blend { color, alpha, constant }))
    }

    /// Blends shader output `src` over the stored `dst`. `clamp` for unorm targets.
    pub(crate) fn apply(&self, src: [f32; 4], dst: [f32; 4], clamp: bool) -> [f32; 4] {
        let c = |v: f32| if clamp { v.clamp(0.0, 1.0) } else { v };
        let (s, k) = (src.map(c), self.constant.map(c));
        std::array::from_fn(|i| {
            let eq = if i == 3 { self.alpha } else { self.color };
            let f = |fac: Factor| -> f32 {
                use Factor::*;
                match fac {
                    Zero => 0.0,
                    One => 1.0,
                    SrcColor => s[i],
                    OneMinusSrcColor => 1.0 - s[i],
                    SrcAlpha => s[3],
                    OneMinusSrcAlpha => 1.0 - s[3],
                    DstAlpha => dst[3],
                    OneMinusDstAlpha => 1.0 - dst[3],
                    DstColor => dst[i],
                    OneMinusDstColor => 1.0 - dst[i],
                    SrcAlphaSaturate if i == 3 => 1.0,
                    SrcAlphaSaturate => s[3].min(1.0 - dst[3]),
                    ConstantColor => k[i],
                    OneMinusConstantColor => 1.0 - k[i],
                    ConstantAlpha => k[3],
                    OneMinusConstantAlpha => 1.0 - k[3],
                }
            };
            let (a, b) = (s[i] * f(eq.src), dst[i] * f(eq.dst));
            match eq.op {
                Op::Add => a + b,
                Op::Subtract => a - b,
                Op::ReverseSubtract => b - a,
                Op::Min => s[i].min(dst[i]),
                Op::Max => s[i].max(dst[i]),
            }
        })
    }
}
