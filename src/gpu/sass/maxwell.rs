//! Maxwell (SM 5.x, 64-bit) SASS front end for the step-1 subset.
//!
//! deko3d binds the `MAXWELL_B` 3D class (0xB197) and its shader compiler (`uam`) emits
//! Maxwell SASS, not Ampere SASS. This module decodes that encoding into the same
//! [`Decoded`] form as [`super::decode`], so lowering ([`super::ir::lower_with`]) and
//! WGSL emission are shared. It is not a hardware-accurate decoder: it decodes only the
//! fields listed here and rejects anything else.
//!
//! Layout: 64-bit little-endian words. Every 32-byte bundle starts with one scheduling
//! control word (stall counts, barriers, reuse) followed by three instructions; control
//! words are skipped. The header in front of the code is the 0x50-byte SPH described by
//! open-gpu-doc ([`MAXWELL_SPH_SIZE`]).
//!
//! Sources, per field group:
//! - [envytools] `envydis/gm107.c` (github.com/envytools/envytools), the nouveau
//!   project's reverse-engineered Maxwell disassembler: opcode masks, the bundle layout
//!   (`schedpos = 0x20`), guard predicate 16..19 (+ negate bit 19), Rd 0..8, Ra 8..16,
//!   Rb 20..28, Rc 39..47, cbuf bank 34..39 / word offset 20..34, 19-bit float immediate
//!   20..39 plus sign bit 56 (top bits of an f32), and every graphics op: `ALD` (0xefd8),
//!   `AST` (0xeff0), `IPA` (0xe0).
//! - [CuAsm] CuAssembler `CuAsm/InsAsmRepos/DefaultInsAsmRepos.sm_61.txt`
//!   (github.com/cloudcores/CuAssembler): real `nvdisasm` words for sm_61 (Pascal keeps
//!   the Maxwell encoding). The `maxwell_cuasm_*` tests check `MOV`, `MOV32I`, `FADD`,
//!   `FADD32I`, `FMUL`, `FMUL32I`, `FFMA`, `MUFU`, `LDC`, `NOP` and `EXIT` against them.
//!   CUDA never emits `ALD`/`AST`/`IPA`, so those rest on [envytools] alone and are
//!   UNVERIFIED until checked against a shader compiled by `uam`.
//! - Textures, bound forms only: [envytools] `TEX` (0xc038), `TLD` (0xdc38), `TEXS`
//!   (0xd0), `TLDS` (0xd2) masks and fields, each checked against [CuAsm] words in the
//!   `maxwell_cuasm_tex_*` tests: handle index 36..49 (a word of the constant buffer
//!   `SET_BINDLESS_TEXTURE` selects). `TEX`/`TLD`: Rd 0..8 written consecutively per the
//!   mask 31..35, coordinates from Ra 8..16, extra operands Rb 20..28, array 28, dimension
//!   29..31. `TEX` LOD 55..58, `.DC` 50, `.AOFFI` 54; `TLD` `.LL` 55, `.AOFFI` 35, `.MS` 50,
//!   `.CL` 54. `TEXS`/`TLDS`: Rd 0..8 and Rd2 28..36 take the components two per register,
//!   coordinates Ra and Rb, component set 50..53, target 53..57, bit 59 set for 32-bit
//!   results (all [CuAsm] words; clear means `.F16` on SM60+, rejected). UNVERIFIED
//!   (Ryujinx-style operand reading, not in [envytools]): 2D `TEXS`/`TLDS` take x from Ra
//!   and y from Rb, `TEX`/`TLD` from Ra, Ra+1.

use super::decode::{
    mask_comps, next_reg, pack_pairs, DecodeError, Decoded, FSrc, Instr, IpaFreq, MufuOp, Src, Tex, TexHandle, TexLod, TexOp,
    RZ,
};
use super::ir::{lower_with, Function};
use super::{ShaderStage, XlateError};

/// Bytes between the header start and the first bundle (20-word SPH, open-gpu-doc).
pub const MAXWELL_SPH_SIZE: usize = 0x50;

const PT: u8 = 7;

fn bits(w: u64, lo: u32, hi: u32) -> u32 {
    ((w >> lo) & ((1u64 << (hi - lo)) - 1)) as u32
}

fn bit(w: u64, b: u32) -> bool {
    (w >> b) & 1 != 0
}

fn reg(w: u64, lo: u32) -> u8 {
    bits(w, lo, lo + 8) as u8
}

fn r(w: u64, lo: u32, neg: Option<u32>, abs: Option<u32>) -> FSrc {
    FSrc { src: Src::Reg(reg(w, lo)), neg: neg.is_some_and(|b| bit(w, b)), abs: abs.is_some_and(|b| bit(w, b)) }
}

/// `c[bank][offset]` in the bit-20 slot: bank 34..39, word offset 20..34.
fn cbuf34(w: u64) -> Src {
    Src::Cbuf { bank: bits(w, 34, 39) as u8, offset: (bits(w, 20, 34) << 2) as u16 }
}

/// 19-bit float immediate at 20..39 with its sign at bit 56: the top 20 bits of an f32.
fn fimm20(w: u64) -> Src {
    Src::Imm((bits(w, 20, 39) | ((bit(w, 56) as u32) << 19)) << 12)
}

fn imm32(w: u64) -> Src {
    Src::Imm(bits(w, 20, 52))
}

fn with(src: Src, neg: Option<u32>, abs: Option<u32>, w: u64) -> FSrc {
    FSrc { src, neg: neg.is_some_and(|b| bit(w, b)), abs: abs.is_some_and(|b| bit(w, b)) }
}

/// Decodes one 64-bit instruction word (not a control word).
pub fn decode(w: u64) -> Result<Decoded, DecodeError> {
    let opcode = (w >> 48) as u16;
    let unsup = |what| DecodeError::Unsupported { opcode, what };
    let pred = bits(w, 16, 19) as u8;
    let pred_neg = bit(w, 19);
    let dst = reg(w, 0);
    let rnd = |lo: u32| if bits(w, lo, lo + 2) != 0 { Err(unsup("rounding mode other than RN")) } else { Ok(()) };
    // FTZ/FMZ field: 0 none, 1 FTZ, 2 FMZ (FMZ's 0 * x = 0 rule has no WGSL form).
    let ftz2 = |lo: u32| match bits(w, lo, lo + 2) {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(unsup("FMZ")),
    };
    let top = |mask: u64, val: u64| w & mask == val;

    let instr = if top(0xfff0 << 48, 0xe300 << 48) {
        if bits(w, 0, 5) != 0xf {
            return Err(unsup("EXIT condition code"));
        }
        Instr::Exit
    } else if top(0xfff8 << 48, 0x50b0 << 48) {
        Instr::Nop
    } else if top(0xfff0 << 48, 0x0100 << 48) {
        if bits(w, 12, 16) != 0xf {
            return Err(unsup("MOV32I lane mask"));
        }
        Instr::Mov { dst, src: imm32(w) }
    } else if top(0xfff8 << 48, 0x5c98 << 48) || top(0xfff8 << 48, 0x4c98 << 48) || top(0xfef8 << 48, 0x3898 << 48) {
        if bits(w, 39, 43) != 0xf {
            return Err(unsup("MOV lane mask"));
        }
        let src = match opcode & 0xff00 {
            0x5c00 => Src::Reg(reg(w, 20)),
            0x4c00 => cbuf34(w),
            // Integer immediate: 19 bits at 20 sign-extended from bit 56.
            _ => Src::Imm(((((bits(w, 20, 39) | ((bit(w, 56) as u32) << 19)) as i32) << 12) >> 12) as u32),
        };
        Instr::Mov { dst, src }
    } else if top(0xfff8 << 48, 0x5c58 << 48) || top(0xfff8 << 48, 0x4c58 << 48) || top(0xfef8 << 48, 0x3858 << 48) {
        rnd(39)?;
        if bit(w, 47) {
            return Err(unsup("FADD.CC"));
        }
        let b = match opcode & 0xff00 {
            0x5c00 => Src::Reg(reg(w, 20)),
            0x4c00 => cbuf34(w),
            _ => fimm20(w),
        };
        Instr::FAdd { dst, a: r(w, 8, Some(48), Some(46)), b: with(b, Some(45), Some(49), w), sat: bit(w, 50), ftz: bit(w, 44) }
    } else if top(0xfc00 << 48, 0x0800 << 48) {
        if bit(w, 52) {
            return Err(unsup("FADD32I.CC"));
        }
        Instr::FAdd { dst, a: r(w, 8, Some(56), Some(54)), b: with(imm32(w), Some(53), Some(57), w), sat: false, ftz: bit(w, 55) }
    } else if top(0xfff8 << 48, 0x5c68 << 48) || top(0xfff8 << 48, 0x4c68 << 48) || top(0xfef8 << 48, 0x3868 << 48) {
        rnd(39)?;
        if bits(w, 41, 44) != 0 {
            return Err(unsup("FMUL scale (.D2/.M2 ...)"));
        }
        if bit(w, 47) {
            return Err(unsup("FMUL.CC"));
        }
        let ftz = ftz2(44)?;
        let b = match opcode & 0xff00 {
            0x5c00 => Src::Reg(reg(w, 20)),
            0x4c00 => cbuf34(w),
            _ => fimm20(w),
        };
        Instr::FMul { dst, a: r(w, 8, None, None), b: with(b, Some(48), None, w), sat: bit(w, 50), ftz }
    } else if top(0xff00 << 48, 0x1e00 << 48) {
        if bit(w, 52) {
            return Err(unsup("FMUL32I.CC"));
        }
        Instr::FMul { dst, a: r(w, 8, None, None), b: with(imm32(w), None, None, w), sat: bit(w, 55), ftz: ftz2(53)? }
    } else if top(0xff80 << 48, 0x5980 << 48)
        || top(0xff80 << 48, 0x4980 << 48)
        || top(0xff80 << 48, 0x5180 << 48)
        || top(0xfe80 << 48, 0x3280 << 48)
    {
        rnd(51)?;
        if bit(w, 47) {
            return Err(unsup("FFMA.CC"));
        }
        let ftz = ftz2(53)?;
        let (b, c) = match opcode & 0xff80 {
            0x5980 => (Src::Reg(reg(w, 20)), Src::Reg(reg(w, 39))),
            0x4980 => (cbuf34(w), Src::Reg(reg(w, 39))),
            0x5180 => (Src::Reg(reg(w, 39)), cbuf34(w)),
            _ => (fimm20(w), Src::Reg(reg(w, 39))),
        };
        Instr::FFma { dst, a: r(w, 8, None, None), b: with(b, Some(48), None, w), c: with(c, Some(49), None, w), sat: bit(w, 50), ftz }
    } else if top(0xfc00 << 48, 0x0c00 << 48) {
        // FFMA32I: c is the destination register.
        if bit(w, 52) {
            return Err(unsup("FFMA32I.CC"));
        }
        Instr::FFma {
            dst,
            a: r(w, 8, Some(56), None),
            b: with(imm32(w), None, None, w),
            c: FSrc { src: Src::Reg(dst), neg: bit(w, 57), abs: false },
            sat: bit(w, 55),
            ftz: ftz2(53)?,
        }
    } else if top(0xfff8 << 48, 0x5080 << 48) {
        let op = match bits(w, 20, 24) {
            0 => MufuOp::Cos,
            1 => MufuOp::Sin,
            2 => MufuOp::Ex2,
            3 => MufuOp::Lg2,
            4 => MufuOp::Rcp,
            5 => MufuOp::Rsq,
            8 => MufuOp::Sqrt,
            _ => return Err(unsup("MUFU function (64H forms)")),
        };
        if bit(w, 50) {
            return Err(unsup("MUFU.SAT"));
        }
        Instr::Mufu { dst, op, src: r(w, 8, Some(48), Some(46)) }
    } else if top(0xfff8 << 48, 0xef90 << 48) {
        if reg(w, 8) != RZ {
            return Err(unsup("LDC register offset"));
        }
        if bits(w, 44, 46) != 0 {
            return Err(unsup("LDC addressing mode"));
        }
        if bits(w, 48, 51) != 4 {
            return Err(unsup("LDC size other than 32-bit"));
        }
        let offset = bits(w, 20, 36) as u16;
        if offset % 4 != 0 || offset & 0x8000 != 0 {
            return Err(unsup("unaligned or negative LDC offset"));
        }
        Instr::Ldc { dst, bank: bits(w, 36, 41) as u8, offset }
    } else if top(0xfff8 << 48, 0xefd8 << 48) {
        if reg(w, 8) != RZ || reg(w, 39) != RZ {
            return Err(unsup("ALD vertex/offset register"));
        }
        if bit(w, 31) || bit(w, 32) {
            return Err(unsup("ALD .P/.O"));
        }
        Instr::Ald { dst, addr: bits(w, 20, 30) as u16, comps: bits(w, 47, 49) as u8 + 1 }
    } else if top(0xfff8 << 48, 0xeff0 << 48) {
        if reg(w, 8) != RZ || reg(w, 39) != RZ {
            return Err(unsup("AST vertex/offset register"));
        }
        if bit(w, 31) {
            return Err(unsup("AST .P"));
        }
        Instr::Ast { src: reg(w, 0), addr: bits(w, 20, 30) as u16, comps: bits(w, 47, 49) as u8 + 1 }
    } else if top(0xff00 << 48, 0xe000 << 48) {
        if bit(w, 38) || reg(w, 8) != RZ {
            return Err(unsup("IPA indexed attribute"));
        }
        if bits(w, 52, 54) != 0 {
            return Err(unsup("IPA centroid/offset"));
        }
        if bit(w, 51) {
            return Err(unsup("IPA.SAT"));
        }
        if reg(w, 39) != RZ || bits(w, 47, 50) != PT as u32 || bit(w, 50) {
            return Err(unsup("IPA sample register / predicate output"));
        }
        let freq = match bits(w, 54, 56) {
            0 => IpaFreq::Pass,
            1 => IpaFreq::PassMulW,
            2 => IpaFreq::Constant,
            _ => return Err(unsup("IPA.SC")),
        };
        Instr::Ipa { dst, addr: bits(w, 28, 38) as u16, freq, mul: reg(w, 20) }
    } else if top(0xf6 << 56, 0xd0 << 56) || top(0xf6 << 56, 0xd2 << 56) {
        // TEXS / TLDS: the scalar-register forms.
        let fetch = bit(w, 57);
        if !bit(w, 59) {
            return Err(unsup("16-bit texture result"));
        }
        let lod = match (fetch, bits(w, 53, 57)) {
            (false, 1) => TexLod::Auto,
            (false, 2) | (true, 2) => TexLod::Zero,
            _ => return Err(unsup("texture target other than 2D (.LZ for TLDS)")),
        };
        let rd2 = reg(w, 28);
        let comps: &[usize] = match (rd2 == RZ, bits(w, 50, 53)) {
            (true, 0) => &[0],
            (true, 1) => &[1],
            (true, 2) => &[2],
            (true, 3) => &[3],
            (true, 4) => &[0, 1],
            (true, 5) => &[0, 3],
            (true, 6) => &[1, 3],
            (true, 7) => &[2, 3],
            (false, 0) => &[0, 1, 2],
            (false, 1) => &[0, 1, 3],
            (false, 2) => &[0, 2, 3],
            (false, 3) => &[1, 2, 3],
            (false, 4) => &[0, 1, 2, 3],
            _ => return Err(unsup("texture component set")),
        };
        Instr::Tex(Tex {
            op: if fetch { TexOp::Fetch } else { TexOp::Sample },
            handle: TexHandle::Bound { index: bits(w, 36, 49) as u16 },
            lod,
            x: reg(w, 8),
            y: reg(w, 20),
            dst: pack_pairs(comps, dst, rd2),
        })
    } else if top(0xfc38 << 48, 0xc038 << 48) || top(0xff38 << 48, 0xdc38 << 48) {
        let fetch = top(0xff38 << 48, 0xdc38 << 48);
        if bits(w, 29, 31) != 1 || bit(w, 28) {
            return Err(unsup("texture dimension other than 2D"));
        }
        let lod = if fetch {
            if bit(w, 35) || bit(w, 50) || bit(w, 54) {
                return Err(unsup("TLD .AOFFI/.MS/.CL"));
            }
            if bit(w, 55) {
                return Err(unsup("TLD.LL"));
            }
            TexLod::Zero
        } else {
            if bit(w, 50) || bit(w, 54) {
                return Err(unsup("TEX .DC/.AOFFI"));
            }
            match bits(w, 55, 58) {
                0 => TexLod::Auto,
                1 => TexLod::Zero,
                _ => return Err(unsup("TEX LOD mode")),
            }
        };
        if reg(w, 20) != RZ {
            return Err(unsup("texture extra operand register"));
        }
        let mut d = [RZ; 4];
        for (i, c) in mask_comps(bits(w, 31, 35)).into_iter().enumerate() {
            d[c] = if dst == RZ { RZ } else { dst.wrapping_add(i as u8) };
        }
        let ra = reg(w, 8);
        Instr::Tex(Tex {
            op: if fetch { TexOp::Fetch } else { TexOp::Sample },
            handle: TexHandle::Bound { index: bits(w, 36, 49) as u16 },
            lod,
            x: ra,
            y: next_reg(ra),
            dst: d,
        })
    } else if top(0xfff8 << 48, 0xdeb8 << 48) || top(0xff38 << 48, 0xdd38 << 48) {
        return Err(unsup("bindless texture handle"));
    } else {
        return Err(DecodeError::UnknownOpcode { opcode });
    };
    Ok(Decoded { pred, pred_neg, instr })
}

/// Byte offset (from the first bundle) of instruction `i`, skipping control words.
pub fn instr_offset(i: usize) -> usize {
    (i / 3) * 32 + 8 + (i % 3) * 8
}

/// Reads instruction `i` (64-bit, little endian) from `code`.
pub fn word_at(code: &[u8], i: usize) -> Option<u64> {
    let at = instr_offset(i);
    let b = code.get(at..at + 8)?;
    let mut a = [0u8; 8];
    a.copy_from_slice(b);
    Some(u64::from_le_bytes(a))
}

/// Decodes and lowers Maxwell code that follows the header, up to the first
/// unconditional `EXIT`.
pub fn lower(stage: ShaderStage, code: &[u8]) -> Result<Function, XlateError> {
    lower_with(stage, |pc| {
        let at = instr_offset(pc);
        let w = word_at(code, pc).ok_or(XlateError::MissingExit)?;
        let d = decode(w).map_err(|e| XlateError::Decode { at, word: w as u128, err: e })?;
        Ok((at, d))
    })
}
