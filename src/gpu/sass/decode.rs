//! Ampere (SM 8.6 / 8.7) SASS instruction decoder for the step-1 subset.
//!
//! NVIDIA does not publish the Volta+ SASS encoding. This is NOT a hardware-accurate
//! decoder: it decodes only the fields listed here, from community sources, and rejects
//! anything else. Every instruction is 128 bits, stored as two little-endian u64 (low
//! half first). Bits 105..128 hold scheduling control (stall, yield, barriers, reuse)
//! and are ignored.
//!
//! Sources, per field group:
//! - [CuAsm] CuAssembler `CuAsm/InsAsmRepos/DefaultInsAsmRepos.sm_86.txt`
//!   (github.com/cloudcores/CuAssembler): real `nvdisasm` encodings for sm_86. Verified
//!   against it (see the `cuasm_*` tests): opcode bits 0..12 with the ALU operand form in
//!   bits 9..12, guard predicate 12..15 (+ negate bit 15), Rd 16..24, Ra 24..32,
//!   Rb/imm32/cbuf at 32..64, Rc at 64..72; cbuf byte offset 38..54 and bank 54..59;
//!   neg/abs bits (Ra 72/73, slot-32 63/62, slot-64 75/74); `.SAT` 77, rounding 78..80,
//!   `.FTZ` 80; `MOV` lane mask 72..76; `MUFU` function 74..78; `LDC` size 73..76;
//!   opcodes `MOV` 0x002, `FMUL` 0x020, `FADD` 0x021, `FFMA` 0x023, `MUFU` 0x108,
//!   `LDC` 0xb82, `EXIT` 0x94d, `NOP` 0x918.
//! - [turingas] github.com/daadaada/turingas `turingas/grammar.py`: same opcodes for
//!   sm_75 (`FFMA` 0x223, `FADD` 0x221, `FMUL` 0x220, `MUFU` 0x308, `LDC` 0xb82).
//! - [NAK, UNVERIFIED] graphics-only ops never appear in CUDA output, so `ALD` (0x321),
//!   `AST` (0x322) and `IPA` (0x326) and their field positions are recalled from Mesa's
//!   NAK SM70 encoder (`src/nouveau/compiler/nak/sm70_encode.rs`), which could not be
//!   fetched from this environment. Treat them as unverified until checked against an
//!   Ampere graphics shader compiled by NAK (`uam`/deko3d emits Maxwell SASS, so it
//!   cannot confirm these).
//! - [inferred] `LDC` 32-bit size value 4 (CuAsm shows U8=0, S8=1, U16=2 only).
//! - Textures, bound-handle `TEX` (0xb60) and `TLD` (0xb66), from [CuAsm] `TEX.SCR.LL` /
//!   `TLD.SCR.LZ` words (see the `cuasm_tex_*` tests): Rd 16..24 and a second destination
//!   Rd2 64..72, coordinates from Ra 24..32, extra operand Rb 32..40, handle word index
//!   40..54 in constant buffer bank 54..59, `.SCR` 60 (meaning unknown, ignored), dimension
//!   61..63 (1D 0, 2D 1, CUBE 3), array 63, write mask 72..76, `.AOFFI` 76, `.NDV` 77, `.DC`
//!   78, LOD mode 87..90 (auto 0, `.LZ` 1, `.LL` 3). The bindless forms (0x361, 0x367) are
//!   rejected. UNVERIFIED (from NAK's `OpTex` operand order, not checked against a graphics
//!   shader): the written components are packed densely, the first two into Rd, Rd+1 and the
//!   rest into Rd2, Rd2+1; 2D coordinates are Ra, Ra+1; `.LL` takes the level from Rb.

use std::fmt;

pub const RZ: u8 = 255;
pub const PT: u8 = 7;

fn bits(w: u128, lo: u32, hi: u32) -> u32 {
    ((w >> lo) & ((1u128 << (hi - lo)) - 1)) as u32
}

fn bit(w: u128, b: u32) -> bool {
    (w >> b) & 1 != 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Src {
    Reg(u8),
    /// Raw 32-bit immediate.
    Imm(u32),
    /// `c[bank][offset]`, byte offset.
    Cbuf { bank: u8, offset: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FSrc {
    pub src: Src,
    pub neg: bool,
    pub abs: bool,
}

/// `MUFU` function, values from [CuAsm].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MufuOp {
    Cos,
    Sin,
    Ex2,
    Lg2,
    Rcp,
    Rsq,
    Sqrt,
    Tanh,
}

/// `IPA` sampling frequency / mode [NAK, UNVERIFIED].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpaFreq {
    /// Value as interpolated.
    Pass,
    /// Interpolated value times register `mul` (perspective correction).
    PassMulW,
    /// Flat (provoking vertex).
    Constant,
}

/// Where a texture instruction finds its handle word (TIC index in bits 0..20, TSC index in
/// bits 20..32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TexHandle {
    /// Ampere: the word at `c[bank][offset]` (byte offset).
    Cbuf { bank: u8, offset: u16 },
    /// Maxwell bound form: word `index` of the constant buffer that the 3D class method
    /// `SET_BINDLESS_TEXTURE` selects.
    Bound { index: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexOp {
    /// `TEX`/`TEXS`: filtered sample at normalized coordinates.
    Sample,
    /// `TLD`/`TLDS`: one texel at integer coordinates, no sampler.
    Fetch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexLod {
    /// Implicit (derivatives in a fragment shader).
    Auto,
    /// `.LZ`: level 0.
    Zero,
    /// `.LL`: level from a register.
    Level(u8),
}

/// A 2D texture read, normalized across encodings: coordinates `x`, `y` and the destination
/// register of each RGBA component (`RZ` = not written).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tex {
    pub op: TexOp,
    pub handle: TexHandle,
    pub lod: TexLod,
    pub x: u8,
    pub y: u8,
    pub dst: [u8; 4],
}

/// Destinations for the components in `comps` (in order), packed two per register pair:
/// the first two into `a`, `a + 1`, the rest into `b`, `b + 1`.
pub(crate) fn pack_pairs(comps: &[usize], a: u8, b: u8) -> [u8; 4] {
    let mut dst = [RZ; 4];
    for (i, &c) in comps.iter().enumerate() {
        let base = if i < 2 { a } else { b };
        dst[c] = if base == RZ { RZ } else { base.wrapping_add((i % 2) as u8) };
    }
    dst
}

/// Components enabled in a 4-bit write mask, in RGBA order.
pub(crate) fn mask_comps(mask: u32) -> Vec<usize> {
    (0..4).filter(|c| mask & (1 << c) != 0).collect()
}

/// Second coordinate register after `ra` (`RZ` stays `RZ`).
pub(crate) fn next_reg(ra: u8) -> u8 {
    if ra == RZ { RZ } else { ra.wrapping_add(1) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instr {
    Nop,
    Exit,
    Mov { dst: u8, src: Src },
    FAdd { dst: u8, a: FSrc, b: FSrc, sat: bool, ftz: bool },
    FMul { dst: u8, a: FSrc, b: FSrc, sat: bool, ftz: bool },
    FFma { dst: u8, a: FSrc, b: FSrc, c: FSrc, sat: bool, ftz: bool },
    Mufu { dst: u8, op: MufuOp, src: FSrc },
    /// `LDC.32 Rd, c[bank][offset]` (register offset must be `RZ`).
    Ldc { dst: u8, bank: u8, offset: u16 },
    /// `ALD Rd, a[addr]`, `comps` consecutive words into `Rd..`.
    Ald { dst: u8, addr: u16, comps: u8 },
    /// `AST a[addr], Rs`, `comps` consecutive words from `Rs..`.
    Ast { src: u8, addr: u16, comps: u8 },
    Ipa { dst: u8, addr: u16, freq: IpaFreq, mul: u8 },
    Tex(Tex),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoded {
    pub pred: u8,
    pub pred_neg: bool,
    pub instr: Instr,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("unknown opcode {opcode:#05x}")]
    UnknownOpcode { opcode: u16 },
    #[error("opcode {opcode:#05x}: unsupported {what}")]
    Unsupported { opcode: u16, what: &'static str },
}

fn reg_slot(w: u128, lo: u32, neg: u32, abs: u32) -> FSrc {
    FSrc { src: Src::Reg(bits(w, lo, lo + 8) as u8), neg: bit(w, neg), abs: bit(w, abs) }
}

fn cbuf(w: u128) -> Src {
    Src::Cbuf { bank: bits(w, 54, 59) as u8, offset: bits(w, 38, 54) as u16 }
}

fn plain(src: Src) -> FSrc {
    FSrc { src, neg: false, abs: false }
}

/// Operand in the bit-32 slot. `imm`/`cb` are the form numbers (bits 9..12) this opcode
/// uses for an immediate / constant buffer there; form 1 is always a register. They
/// differ per opcode in [CuAsm]: `MOV`/`FMUL` use 4/5, `FADD` uses 2/3.
fn slot32(w: u128, form: u32, imm: u32, cb: u32, opcode: u16) -> Result<FSrc, DecodeError> {
    if form == 1 {
        Ok(reg_slot(w, 32, 63, 62))
    } else if form == imm {
        Ok(plain(Src::Imm(bits(w, 32, 64))))
    } else if form == cb {
        Ok(FSrc { src: cbuf(w), neg: bit(w, 63), abs: bit(w, 62) })
    } else {
        Err(DecodeError::Unsupported { opcode, what: "operand form" })
    }
}

fn fp_ctl(w: u128, opcode: u16) -> Result<(bool, bool), DecodeError> {
    if bits(w, 78, 80) != 0 {
        return Err(DecodeError::Unsupported { opcode, what: "rounding mode other than RN" });
    }
    Ok((bit(w, 77), bit(w, 80)))
}

pub fn decode(w: u128) -> Result<Decoded, DecodeError> {
    let full = bits(w, 0, 12) as u16;
    let base = full & 0x1ff;
    let form = bits(w, 9, 12);
    let pred = bits(w, 12, 15) as u8;
    let pred_neg = bit(w, 15);
    let dst = bits(w, 16, 24) as u8;
    let ra = reg_slot(w, 24, 72, 73);
    let unsup = |what| DecodeError::Unsupported { opcode: full, what };

    let instr = match full {
        0x94d => Instr::Exit,
        0x918 => Instr::Nop,
        0xb82 => {
            if bits(w, 24, 32) as u8 != RZ {
                return Err(unsup("LDC register offset"));
            }
            if bits(w, 73, 76) != 4 {
                return Err(unsup("LDC size other than 32-bit"));
            }
            if bits(w, 78, 80) != 0 {
                return Err(unsup("LDC addressing mode"));
            }
            match cbuf(w) {
                Src::Cbuf { bank, offset } if offset % 4 == 0 => Instr::Ldc { dst, bank, offset },
                _ => return Err(unsup("unaligned LDC")),
            }
        }
        0x321 => {
            if bits(w, 24, 32) as u8 != RZ || bits(w, 32, 40) as u8 != RZ {
                return Err(unsup("ALD vertex/offset register"));
            }
            if bit(w, 76) || bit(w, 77) || bit(w, 79) {
                return Err(unsup("ALD .P/.PHYS/.O"));
            }
            Instr::Ald { dst, addr: bits(w, 40, 50) as u16, comps: bits(w, 74, 76) as u8 + 1 }
        }
        0x322 => {
            if bits(w, 24, 32) as u8 != RZ || bits(w, 64, 72) as u8 != RZ {
                return Err(unsup("AST vertex/offset register"));
            }
            if bit(w, 76) || bit(w, 77) {
                return Err(unsup("AST .P/.PHYS"));
            }
            Instr::Ast { src: bits(w, 32, 40) as u8, addr: bits(w, 40, 50) as u16, comps: bits(w, 74, 76) as u8 + 1 }
        }
        0x326 => {
            if bits(w, 76, 78) != 0 {
                return Err(unsup("IPA centroid/offset/sample location"));
            }
            if bits(w, 24, 32) as u8 != RZ {
                return Err(unsup("IPA offset register"));
            }
            let freq = match bits(w, 78, 80) {
                0 => IpaFreq::Pass,
                1 => IpaFreq::PassMulW,
                2 => IpaFreq::Constant,
                _ => return Err(unsup("IPA.STATE")),
            };
            Instr::Ipa { dst, addr: (bits(w, 64, 72) << 2) as u16, freq, mul: bits(w, 32, 40) as u8 }
        }
        0xb60 | 0xb66 => {
            let op = if full == 0xb60 { TexOp::Sample } else { TexOp::Fetch };
            if bits(w, 61, 63) != 1 || bit(w, 63) {
                return Err(unsup("texture dimension other than 2D"));
            }
            if bit(w, 76) || bit(w, 78) {
                return Err(unsup("texture .AOFFI/.DC"));
            }
            let rb = bits(w, 32, 40) as u8;
            let lod = match (bits(w, 87, 90), op) {
                (0, TexOp::Sample) => TexLod::Auto,
                (1, _) => TexLod::Zero,
                (3, _) => TexLod::Level(rb),
                _ => return Err(unsup("texture LOD mode")),
            };
            if !matches!(lod, TexLod::Level(_)) && rb != RZ {
                return Err(unsup("texture extra operand register"));
            }
            let ra = bits(w, 24, 32) as u8;
            Instr::Tex(Tex {
                op,
                handle: TexHandle::Cbuf { bank: bits(w, 54, 59) as u8, offset: (bits(w, 40, 54) * 4) as u16 },
                lod,
                x: ra,
                y: next_reg(ra),
                dst: pack_pairs(&mask_comps(bits(w, 72, 76)), dst, bits(w, 64, 72) as u8),
            })
        }
        0x361 | 0x367 => return Err(unsup("bindless texture handle")),
        _ => match base {
            0x002 => {
                if bits(w, 72, 76) != 0xf {
                    return Err(unsup("MOV lane mask"));
                }
                let s = slot32(w, form, 4, 5, full)?;
                // MOV has no source modifiers; a set bit 62/63 outside the immediate form
                // means we misread the word.
                if s.neg || s.abs {
                    return Err(unsup("MOV source modifiers"));
                }
                Instr::Mov { dst, src: s.src }
            }
            0x020 | 0x021 => {
                let b = if base == 0x020 { slot32(w, form, 4, 5, full)? } else { slot32(w, form, 2, 3, full)? };
                let (sat, ftz) = fp_ctl(w, full)?;
                if base == 0x020 {
                    // CuAsm sets 4 in bits 84..87 on every FMUL without .D2/.M2-style scaling.
                    if bits(w, 84, 87) != 4 {
                        return Err(unsup("FMUL scale"));
                    }
                    Instr::FMul { dst, a: ra, b, sat, ftz }
                } else {
                    Instr::FAdd { dst, a: ra, b, sat, ftz }
                }
            }
            0x023 => {
                let r64 = reg_slot(w, 64, 75, 74);
                let (b, c) = match form {
                    1 => (reg_slot(w, 32, 63, 62), r64),
                    2 => (r64, plain(Src::Imm(bits(w, 32, 64)))),
                    3 => (r64, FSrc { src: cbuf(w), neg: bit(w, 63), abs: bit(w, 62) }),
                    4 => (plain(Src::Imm(bits(w, 32, 64))), r64),
                    5 => (FSrc { src: cbuf(w), neg: bit(w, 63), abs: bit(w, 62) }, r64),
                    _ => return Err(unsup("operand form")),
                };
                let (sat, ftz) = fp_ctl(w, full)?;
                Instr::FFma { dst, a: ra, b, c, sat, ftz }
            }
            0x108 => {
                // Only the register form appears in [CuAsm]; immediate/cbuf forms unverified.
                let src = slot32(w, form, u32::MAX, u32::MAX, full)?;
                let op = match bits(w, 74, 78) {
                    0 => MufuOp::Cos,
                    1 => MufuOp::Sin,
                    2 => MufuOp::Ex2,
                    3 => MufuOp::Lg2,
                    4 => MufuOp::Rcp,
                    5 => MufuOp::Rsq,
                    8 => MufuOp::Sqrt,
                    9 => MufuOp::Tanh,
                    _ => return Err(unsup("MUFU function (64H forms)")),
                };
                Instr::Mufu { dst, op, src }
            }
            _ => return Err(DecodeError::UnknownOpcode { opcode: full }),
        },
    };
    Ok(Decoded { pred, pred_neg, instr })
}

/// Reads instruction `i` (128-bit, little endian) from `code`.
pub fn word_at(code: &[u8], i: usize) -> Option<u128> {
    let b = code.get(i * 16..i * 16 + 16)?;
    let mut a = [0u8; 16];
    a.copy_from_slice(b);
    Some(u128::from_le_bytes(a))
}

fn reg(r: u8) -> String {
    if r == RZ { "RZ".into() } else { format!("R{r}") }
}

impl fmt::Display for Src {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Src::Reg(r) => write!(f, "{}", reg(*r)),
            Src::Imm(v) => write!(f, "{v:#x}"),
            Src::Cbuf { bank, offset } => write!(f, "c[{bank:#x}][{offset:#x}]"),
        }
    }
}

impl fmt::Display for FSrc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = if self.neg { "-" } else { "" };
        if self.abs { write!(f, "{n}|{}|", self.src) } else { write!(f, "{n}{}", self.src) }
    }
}

impl fmt::Display for Decoded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pred != PT || self.pred_neg {
            let p = if self.pred == PT { "PT".to_string() } else { format!("P{}", self.pred) };
            write!(f, "@{}{p} ", if self.pred_neg { "!" } else { "" })?;
        }
        let m = |sat: bool, ftz: bool| format!("{}{}", if ftz { ".FTZ" } else { "" }, if sat { ".SAT" } else { "" });
        match self.instr {
            Instr::Nop => write!(f, "NOP"),
            Instr::Exit => write!(f, "EXIT"),
            Instr::Mov { dst, src } => write!(f, "MOV {}, {src}", reg(dst)),
            Instr::FAdd { dst, a, b, sat, ftz } => write!(f, "FADD{} {}, {a}, {b}", m(sat, ftz), reg(dst)),
            Instr::FMul { dst, a, b, sat, ftz } => write!(f, "FMUL{} {}, {a}, {b}", m(sat, ftz), reg(dst)),
            Instr::FFma { dst, a, b, c, sat, ftz } => write!(f, "FFMA{} {}, {a}, {b}, {c}", m(sat, ftz), reg(dst)),
            Instr::Mufu { dst, op, src } => write!(f, "MUFU.{} {}, {src}", format!("{op:?}").to_uppercase(), reg(dst)),
            Instr::Ldc { dst, bank, offset } => write!(f, "LDC {}, c[{bank:#x}][{offset:#x}]", reg(dst)),
            Instr::Ald { dst, addr, comps } => write!(f, "ALD.{comps} {}, a[{addr:#x}]", reg(dst)),
            Instr::Ast { src, addr, comps } => write!(f, "AST.{comps} a[{addr:#x}], {}", reg(src)),
            Instr::Ipa { dst, addr, freq, mul } => match freq {
                IpaFreq::Pass => write!(f, "IPA.PASS {}, a[{addr:#x}]", reg(dst)),
                IpaFreq::PassMulW => write!(f, "IPA {}, a[{addr:#x}], {}", reg(dst), reg(mul)),
                IpaFreq::Constant => write!(f, "IPA.CONSTANT {}, a[{addr:#x}]", reg(dst)),
            },
            Instr::Tex(t) => {
                let name = if t.op == TexOp::Sample { "TEX" } else { "TLD" };
                let lod = match t.lod {
                    TexLod::Auto => String::new(),
                    TexLod::Zero => ".LZ".into(),
                    TexLod::Level(r) => format!(".LL({})", reg(r)),
                };
                let h = match t.handle {
                    TexHandle::Cbuf { bank, offset } => format!("c[{bank:#x}][{offset:#x}]"),
                    TexHandle::Bound { index } => format!("{index:#x}"),
                };
                let d: Vec<String> = t.dst.iter().map(|&r| reg(r)).collect();
                write!(f, "{name}.2D{lod} {{{}}}, {}, {}, {h}", d.join(", "), reg(t.x), reg(t.y))
            }
        }
    }
}
