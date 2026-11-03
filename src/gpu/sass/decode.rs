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

/// Float comparison; the discriminant is the 4-bit code both ISAs use ([envytools]
/// `tab5bb0_0`, matched by [CuAsm] `FSETP` words). `U` forms are also true when either side
/// is NaN; the plain forms are false then. `Num`/`Nan`: both ordered / either unordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FCmp {
    F,
    Lt,
    Eq,
    Le,
    Gt,
    Ne,
    Ge,
    Num,
    Nan,
    Ltu,
    Equ,
    Leu,
    Gtu,
    Neu,
    Geu,
    T,
}

impl FCmp {
    pub fn from_code(c: u32) -> Self {
        use FCmp::*;
        [F, Lt, Eq, Le, Gt, Ne, Ge, Num, Nan, Ltu, Equ, Leu, Gtu, Neu, Geu, T][(c & 0xf) as usize]
    }
}

/// Integer comparison, 3-bit code ([envytools] `tab5b60_0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ICmp {
    F,
    Lt,
    Eq,
    Le,
    Gt,
    Ne,
    Ge,
    T,
}

impl ICmp {
    pub fn from_code(c: u32) -> Self {
        use ICmp::*;
        [F, Lt, Eq, Le, Gt, Ne, Ge, T][(c & 7) as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    And,
    Or,
    Xor,
}

impl BoolOp {
    pub fn from_code(c: u32) -> Option<Self> {
        match c {
            0 => Some(BoolOp::And),
            1 => Some(BoolOp::Or),
            2 => Some(BoolOp::Xor),
            _ => None,
        }
    }
}

/// Float-to-integral rounding (`F2I`, `F2F`, `FRND`). `Nearest` is ties-to-even.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Round {
    Nearest,
    Floor,
    Ceil,
    Trunc,
}

/// Predicate operand: `p` (`PT` = 7 is always true), optionally negated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PSrc {
    pub p: u8,
    pub neg: bool,
}

impl PSrc {
    pub const TRUE: PSrc = PSrc { p: PT, neg: false };
}

/// The comparison inside a `*SETP`/`*SET` instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    F { cmp: FCmp, a: FSrc, b: FSrc },
    I { cmp: ICmp, signed: bool, a: Src, b: Src },
    /// `PSETP`: `a op b` on predicates.
    P { op: BoolOp, a: PSrc, b: PSrc },
}

/// `XMAD` addend mode ([envytools] `tab5b00_1`). `Csfu` is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmadMode {
    /// `c` as is.
    None,
    /// `c & 0xffff`.
    Clo,
    /// `c >> 16`.
    Chi,
    /// `c + (b << 16)` (b as encoded, all 32 bits).
    Cbcc,
}

/// `LOP3` truth-table inputs: the table for `a` alone is 0xf0, `b` 0xcc, `c` 0xaa.
pub const LUT_A: u8 = 0xf0;
pub const LUT_B: u8 = 0xcc;
pub const LUT_C: u8 = 0xaa;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instr {
    Nop,
    Exit,
    Mov { dst: u8, src: Src },
    FAdd { dst: u8, a: FSrc, b: FSrc, sat: bool, ftz: bool },
    FMul { dst: u8, a: FSrc, b: FSrc, sat: bool, ftz: bool },
    FFma { dst: u8, a: FSrc, b: FSrc, c: FSrc, sat: bool, ftz: bool },
    Mufu { dst: u8, op: MufuOp, src: FSrc },
    /// `LDC.32 Rd, c[bank][offset + reg]` (`reg == RZ` for unindexed).
    Ldc { dst: u8, bank: u8, offset: u16, reg: u8 },
    /// `ALD Rd, a[addr]`, `comps` consecutive words into `Rd..`.
    Ald { dst: u8, addr: u16, comps: u8 },
    /// `AST a[addr], Rs`, `comps` consecutive words from `Rs..`.
    Ast { src: u8, addr: u16, comps: u8 },
    Ipa { dst: u8, addr: u16, freq: IpaFreq, mul: u8 },
    Tex(Tex),
    /// `pd = cmp bop c`, `pd2 = !cmp bop c` (`PT` destinations are discarded).
    Setp { pd: u8, pd2: u8, cmp: Compare, bop: BoolOp, c: PSrc },
    /// As [`Instr::Setp`] into a register: 1.0 (`bf`) or 0xffffffff when true, 0 when false.
    Set { dst: u8, cmp: Compare, bop: BoolOp, c: PSrc, bf: bool },
    /// `dst = c ? a : b`.
    Sel { dst: u8, a: Src, b: Src, c: PSrc },
    /// `FMNMX`: `c ? min(a, b) : max(a, b)`.
    FMnmx { dst: u8, a: FSrc, b: FSrc, c: PSrc },
    /// `IMNMX`: `c ? min(a, b) : max(a, b)`.
    IMnmx { dst: u8, a: Src, b: Src, signed: bool, c: PSrc },
    /// `FCMP`: `dst = (c cmp 0.0) ? a : b`.
    FCmpSel { dst: u8, cmp: FCmp, a: Src, b: Src, c: Src },
    /// `dst = (+/-a) + (+/-b) + (+/-c)`, 32-bit wrapping (`IADD`, `IADD32I`, `IADD3`).
    IAdd { dst: u8, a: Src, b: Src, c: Src, neg: [bool; 3] },
    /// `ISCADD`: `dst = ((+/-a) << shift) + (+/-b)`.
    IScAdd { dst: u8, a: Src, b: Src, shift: u8, neg: [bool; 2] },
    /// `IMAD` (low 32 bits): `dst = a * b + c`.
    IMad { dst: u8, a: Src, b: Src, c: Src },
    /// Maxwell 16x16-bit multiply-add, see [`XmadMode`].
    Xmad { dst: u8, a: Src, b: Src, c: Src, a_signed: bool, b_signed: bool, a_hi: bool, b_hi: bool, psl: bool, mrg: bool, mode: XmadMode },
    /// `dst = LUT(a, b, c)` bitwise (`LOP`/`LOP32I` are mapped onto a table).
    Lop3 { dst: u8, lut: u8, a: Src, b: Src, c: Src },
    /// `dst = a << b`; `wrap`: shift by `b & 31`, else amounts >= 32 give 0.
    Shl { dst: u8, a: Src, b: Src, wrap: bool },
    /// `dst = a >> b` (arithmetic when `signed`); `wrap` as for [`Instr::Shl`], else
    /// amounts >= 32 act as 32.
    Shr { dst: u8, a: Src, b: Src, signed: bool, wrap: bool },
    /// 32-bit integer to f32 (round to nearest).
    I2F { dst: u8, src: Src, signed: bool },
    /// f32 to 32-bit integer after `round`, saturating; NaN gives 0.
    F2I { dst: u8, src: FSrc, signed: bool, round: Round },
    /// f32 to f32: optional rounding to an integral value (`F2F.FLOOR`, `FRND`), then `.SAT`.
    F2F { dst: u8, src: FSrc, round: Option<Round>, sat: bool },
    /// Branch to the instruction `rel` bytes from this one's start.
    Bra { rel: i32 },
    /// Maxwell `SSY`: push a reconvergence point `rel` bytes from this instruction.
    Ssy { rel: i32 },
    /// Maxwell `SYNC`: jump to the innermost `SSY` point.
    Sync,
    /// Maxwell `PBK`: push a break point.
    Pbk { rel: i32 },
    /// Maxwell `BRK`: jump to the innermost `PBK` point.
    Brk,
    /// Fragment discard (`KIL`, `KILL`).
    Kill,
    /// `PLOP3`: `pd = LUT(a, b, c)` on predicates.
    PLop3 { pd: u8, lut: u8, a: PSrc, b: PSrc, c: PSrc },
    /// Warp-level hint with no effect on one thread's result (`BSSY`, `BSYNC`, `WARPSYNC`).
    Hint(&'static str),
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
            let r_off = bits(w, 24, 32) as u8;
            if bits(w, 73, 76) != 4 {
                return Err(unsup("LDC size other than 32-bit"));
            }
            if bits(w, 78, 80) != 0 {
                return Err(unsup("LDC addressing mode"));
            }
            match cbuf(w) {
                Src::Cbuf { bank, offset } if offset % 4 == 0 => Instr::Ldc { dst, bank, offset, reg: r_off },
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
            _ => match decode_ext(w, full)? {
                Some(i) => i,
                None => return Err(DecodeError::UnknownOpcode { opcode: full }),
            },
        },
    };
    Ok(Decoded { pred, pred_neg, instr })
}

fn psrc(w: u128, lo: u32) -> PSrc {
    PSrc { p: bits(w, lo, lo + 3) as u8, neg: bit(w, lo + 3) }
}

/// Integer operand in the bit-32 slot: form 1 register, 4 immediate, 5 constant buffer.
/// Register and constant-buffer forms carry a negate bit at 63.
fn islot32(w: u128, form: u32, opcode: u16) -> Result<(Src, bool), DecodeError> {
    match form {
        1 => Ok((Src::Reg(bits(w, 32, 40) as u8), bit(w, 63))),
        4 => Ok((Src::Imm(bits(w, 32, 64)), false)),
        5 => Ok((cbuf(w), bit(w, 63))),
        _ => Err(DecodeError::Unsupported { opcode, what: "operand form (uniform registers?)" }),
    }
}

/// Predicates, compares, integer ops, conversions and branches. Field positions read off
/// [CuAsm] sm_86 words (see the `cuasm_ctl_*` tests); `Ok(None)` = not one of these.
fn decode_ext(w: u128, full: u16) -> Result<Option<Instr>, DecodeError> {
    let base = full & 0x1ff;
    let form = bits(w, 9, 12);
    let dst = bits(w, 16, 24) as u8;
    let ra = Src::Reg(bits(w, 24, 32) as u8);
    let rc = Src::Reg(bits(w, 64, 72) as u8);
    let unsup = |what| DecodeError::Unsupported { opcode: full, what };
    let bop = || BoolOp::from_code(bits(w, 74, 76)).ok_or(unsup("predicate combine op"));
    let pc87 = psrc(w, 87);
    // Predicate destinations at 81..84 / 84..87.
    let (pd, pd2) = (bits(w, 81, 84) as u8, bits(w, 84, 87) as u8);
    let no_pout = || if pd != PT || pd2 != PT { Err(unsup("carry / predicate output")) } else { Ok(()) };
    let fslot = |opcode| -> Result<FSrc, DecodeError> {
        match form {
            1 => Ok(reg_slot(w, 32, 63, 62)),
            4 => Ok(plain(Src::Imm(bits(w, 32, 64)))),
            5 => Ok(FSrc { src: cbuf(w), neg: bit(w, 63), abs: bit(w, 62) }),
            _ => Err(DecodeError::Unsupported { opcode, what: "operand form (uniform registers?)" }),
        }
    };
    let rnd = [Round::Nearest, Round::Floor, Round::Ceil, Round::Trunc];
    let i = match base {
        0x00c => {
            if bit(w, 72) {
                return Err(unsup("ISETP.EX"));
            }
            let (b, neg) = islot32(w, form, full)?;
            if neg {
                return Err(unsup("ISETP negated operand"));
            }
            let cmp = Compare::I { cmp: ICmp::from_code(bits(w, 76, 79)), signed: bit(w, 73), a: ra, b };
            Instr::Setp { pd, pd2, cmp, bop: bop()?, c: pc87 }
        }
        0x00b => {
            let a = reg_slot(w, 24, 72, 73);
            let cmp = Compare::F { cmp: FCmp::from_code(bits(w, 76, 80)), a, b: fslot(full)? };
            Instr::Setp { pd, pd2, cmp, bop: bop()?, c: pc87 }
        }
        0x01c if form == 4 => {
            if bit(w, 71) || bit(w, 80) || bit(w, 90) || pd2 != PT {
                return Err(unsup("PLOP3 negated input / second output"));
            }
            let lut = ((bits(w, 72, 77) << 3) | bits(w, 64, 67)) as u8;
            Instr::PLop3 { pd, lut, a: psrc(w, 87), b: psrc(w, 77), c: psrc(w, 68) }
        }
        0x007 | 0x008 => {
            let (b, neg) = islot32(w, form, full)?;
            if neg || bit(w, 72) {
                return Err(unsup("SEL/FSEL operand modifiers"));
            }
            Instr::Sel { dst, a: ra, b, c: pc87 }
        }
        0x009 => {
            if bit(w, 81) {
                return Err(unsup("FMNMX.NAN"));
            }
            Instr::FMnmx { dst, a: reg_slot(w, 24, 72, 73), b: fslot(full)?, c: pc87 }
        }
        0x017 => {
            let (b, neg) = islot32(w, form, full)?;
            if neg {
                return Err(unsup("IMNMX negated operand"));
            }
            Instr::IMnmx { dst, a: ra, b, signed: bit(w, 73), c: pc87 }
        }
        0x010 => {
            if bit(w, 74) {
                return Err(unsup("IADD3.X"));
            }
            no_pout()?;
            let (b, nb) = islot32(w, form, full)?;
            Instr::IAdd { dst, a: ra, b, c: rc, neg: [bit(w, 72), nb, bit(w, 75)] }
        }
        0x024 => {
            if bit(w, 74) || bit(w, 72) {
                return Err(unsup("IMAD.X / negated product"));
            }
            let (b, c, nb, nc) = match form {
                1 => (Src::Reg(bits(w, 32, 40) as u8), rc, bit(w, 63), bit(w, 75)),
                2 => (rc, Src::Imm(bits(w, 32, 64)), false, false),
                3 => (rc, cbuf(w), false, bit(w, 63)),
                4 => (Src::Imm(bits(w, 32, 64)), rc, false, bit(w, 75)),
                5 => (cbuf(w), rc, bit(w, 63), bit(w, 75)),
                _ => return Err(unsup("operand form (uniform registers?)")),
            };
            if nb {
                return Err(unsup("IMAD negated operand"));
            }
            if nc {
                // a * b - c: no IR op subtracts, so route through IADD3 when the product is trivial.
                match (ra, b) {
                    (Src::Reg(RZ), _) | (_, Src::Reg(RZ)) | (_, Src::Imm(0)) => {
                        Instr::IAdd { dst, a: Src::Reg(RZ), b: Src::Reg(RZ), c, neg: [false, false, true] }
                    }
                    (a, Src::Imm(1)) => Instr::IAdd { dst, a, b: Src::Reg(RZ), c, neg: [false, false, true] },
                    _ => return Err(unsup("IMAD with a negated addend and a real product")),
                }
            } else {
                Instr::IMad { dst, a: ra, b, c }
            }
        }
        0x012 => {
            if bits(w, 87, 91) != 0xf || bit(w, 80) {
                return Err(unsup("LOP3 predicate input / .PAND"));
            }
            if pd != PT {
                return Err(unsup("LOP3 predicate output"));
            }
            let (b, neg) = islot32(w, form, full)?;
            if neg {
                return Err(unsup("LOP3 negated operand"));
            }
            Instr::Lop3 { dst, lut: bits(w, 72, 80) as u8, a: ra, b, c: rc }
        }
        0x019 => {
            let (b, neg) = islot32(w, form, full)?;
            if neg {
                return Err(unsup("SHF negated operand"));
            }
            let ty = bits(w, 73, 75);
            if ty < 2 {
                return Err(unsup("64-bit SHF"));
            }
            let (right, hi, wrap) = (bit(w, 76), bit(w, 80), bit(w, 75));
            match (right, hi, ra, rc) {
                (false, false, a, Src::Reg(RZ)) => Instr::Shl { dst, a, b, wrap },
                (true, true, Src::Reg(RZ), c) => Instr::Shr { dst, a: c, b, signed: ty == 2, wrap },
                _ => return Err(unsup("SHF funnel shift across two registers")),
            }
        }
        0x106 | 0x045 => {
            if bits(w, 75, 77) != 2 || bits(w, 84, 87) != 2 {
                return Err(unsup("I2F format other than F32 <- 32-bit"));
            }
            if bits(w, 78, 80) != 0 {
                return Err(unsup("I2F rounding mode"));
            }
            let (src, neg) = islot32(w, form, full)?;
            if neg || bit(w, 72) {
                return Err(unsup("I2F modifiers / byte select"));
            }
            Instr::I2F { dst, src, signed: bit(w, 74) }
        }
        0x105 => {
            if bits(w, 75, 77) != 2 || bits(w, 84, 87) != 2 {
                return Err(unsup("F2I format other than 32-bit <- F32"));
            }
            Instr::F2I { dst, src: fslot(full)?, signed: bit(w, 72), round: rnd[bits(w, 78, 80) as usize] }
        }
        0x107 => {
            if bits(w, 75, 77) != 2 || bits(w, 84, 87) != 2 {
                return Err(unsup("FRND format other than F32"));
            }
            Instr::F2F { dst, src: fslot(full)?, round: Some(rnd[bits(w, 78, 80) as usize]), sat: false }
        }
        _ => match full {
            0x947 => {
                if bits(w, 32, 34) >= 2 || bits(w, 87, 91) != PT as u32 {
                    return Err(unsup("BRA.DIV/.CONV or a second predicate"));
                }
                // Byte offset bits 34..82 (word aligned), relative to the next instruction.
                let off = ((bits(w, 34, 64) as u64 | ((bits(w, 64, 82) as u64) << 30)) << 16) as i64 >> 14;
                Instr::Bra { rel: i32::try_from(off + 16).map_err(|_| unsup("branch offset"))? }
            }
            0x945 => Instr::Hint("BSSY"),
            0x941 => Instr::Hint("BSYNC"),
            0x948 | 0x348 => Instr::Hint("WARPSYNC"),
            _ => return Ok(None),
        },
    };
    Ok(Some(i))
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

fn pr(p: u8) -> String {
    if p >= PT { "PT".into() } else { format!("P{p}") }
}

fn sgn(signed: bool) -> &'static str {
    if signed { ".S32" } else { ".U32" }
}

impl fmt::Display for PSrc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", if self.neg { "!" } else { "" }, pr(self.p))
    }
}

fn cmp_kind(c: &Compare) -> &'static str {
    match c {
        Compare::F { .. } => "F",
        Compare::I { .. } => "I",
        Compare::P { .. } => "P",
    }
}

fn cmp_name(c: &Compare) -> String {
    match c {
        Compare::F { cmp, .. } => format!(".{}", format!("{cmp:?}").to_uppercase()),
        Compare::I { cmp, signed, .. } => format!(".{}{}", format!("{cmp:?}").to_uppercase(), sgn(*signed)),
        Compare::P { op, .. } => format!(".{}", format!("{op:?}").to_uppercase()),
    }
}

fn cmp_args(c: &Compare) -> String {
    match c {
        Compare::F { a, b, .. } => format!("{a}, {b}"),
        Compare::I { a, b, .. } => format!("{a}, {b}"),
        Compare::P { a, b, .. } => format!("{a}, {b}"),
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
            Instr::Ldc { dst, bank, offset, reg: r_off } => {
                if r_off == RZ {
                    write!(f, "LDC {}, c[{bank:#x}][{offset:#x}]", reg(dst))
                } else {
                    write!(f, "LDC {}, c[{bank:#x}][{} + {offset:#x}]", reg(dst), reg(r_off))
                }
            }
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
            Instr::Setp { pd, pd2, cmp, bop, c } => {
                write!(f, "{}SETP{}.{bop:?} {}, {}, {}, {c}", cmp_kind(&cmp), cmp_name(&cmp), pr(pd), pr(pd2), cmp_args(&cmp))
            }
            Instr::Set { dst, cmp, bop, c, bf } => {
                let bf = if bf { ".BF" } else { "" };
                write!(f, "{}SET{bf}{}.{bop:?} {}, {}, {c}", cmp_kind(&cmp), cmp_name(&cmp), reg(dst), cmp_args(&cmp))
            }
            Instr::Sel { dst, a, b, c } => write!(f, "SEL {}, {a}, {b}, {c}", reg(dst)),
            Instr::FMnmx { dst, a, b, c } => write!(f, "FMNMX {}, {a}, {b}, {c}", reg(dst)),
            Instr::IMnmx { dst, a, b, signed, c } => write!(f, "IMNMX{} {}, {a}, {b}, {c}", sgn(signed), reg(dst)),
            Instr::FCmpSel { dst, cmp, a, b, c } => write!(f, "FCMP.{cmp:?} {}, {a}, {b}, {c}", reg(dst)),
            Instr::IAdd { dst, a, b, c, neg } => {
                let n = |i: usize| if neg[i] { "-" } else { "" };
                write!(f, "IADD3 {}, {}{a}, {}{b}, {}{c}", reg(dst), n(0), n(1), n(2))
            }
            Instr::IScAdd { dst, a, b, shift, neg } => {
                let n = |i: usize| if neg[i] { "-" } else { "" };
                write!(f, "ISCADD {}, {}{a}, {}{b}, {shift:#x}", reg(dst), n(0), n(1))
            }
            Instr::IMad { dst, a, b, c } => write!(f, "IMAD {}, {a}, {b}, {c}", reg(dst)),
            Instr::Xmad { dst, a, b, c, a_signed, b_signed, a_hi, b_hi, psl, mrg, mode } => {
                let mode = if mode == XmadMode::None { String::new() } else { format!(".{}", format!("{mode:?}").to_uppercase()) };
                let s = |signed: bool| if signed { "S16" } else { "U16" };
                let h = |hi: bool| if hi { ".H1" } else { "" };
                write!(
                    f,
                    "XMAD.{}.{}{}{}{mode} {}, {a}{}, {b}{}, {c}",
                    s(a_signed),
                    s(b_signed),
                    if psl { ".PSL" } else { "" },
                    if mrg { ".MRG" } else { "" },
                    reg(dst),
                    h(a_hi),
                    h(b_hi)
                )
            }
            Instr::Lop3 { dst, lut, a, b, c } => write!(f, "LOP3.LUT {}, {a}, {b}, {c}, {lut:#04x}", reg(dst)),
            Instr::Shl { dst, a, b, wrap } => write!(f, "SHL{} {}, {a}, {b}", if wrap { ".W" } else { "" }, reg(dst)),
            Instr::Shr { dst, a, b, signed, wrap } => {
                write!(f, "SHR{}{} {}, {a}, {b}", sgn(signed), if wrap { ".W" } else { "" }, reg(dst))
            }
            Instr::I2F { dst, src, signed } => write!(f, "I2F.F32{} {}, {src}", sgn(signed), reg(dst)),
            Instr::F2I { dst, src, signed, round } => {
                write!(f, "F2I{}.F32.{} {}, {src}", sgn(signed), format!("{round:?}").to_uppercase(), reg(dst))
            }
            Instr::F2F { dst, src, round, sat } => {
                let r = round.map_or(String::new(), |r| format!(".{}", format!("{r:?}").to_uppercase()));
                write!(f, "F2F.F32.F32{r}{} {}, {src}", if sat { ".SAT" } else { "" }, reg(dst))
            }
            Instr::Bra { rel } => write!(f, "BRA {rel:+#x}"),
            Instr::Ssy { rel } => write!(f, "SSY {rel:+#x}"),
            Instr::Sync => write!(f, "SYNC"),
            Instr::Pbk { rel } => write!(f, "PBK {rel:+#x}"),
            Instr::Brk => write!(f, "BRK"),
            Instr::Kill => write!(f, "KIL"),
            Instr::PLop3 { pd, lut, a, b, c } => write!(f, "PLOP3.LUT {}, {a}, {b}, {c}, {lut:#04x}", pr(pd)),
            Instr::Hint(name) => write!(f, "{name}"),
        }
    }
}
