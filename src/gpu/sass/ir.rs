//! Small SSA IR for shaders, and the lowering from decoded SASS.
//!
//! The IR is straight-line: every instruction result is a fresh [`Value`], a register map
//! tracks the latest value per register, and `MOV` just aliases. All ops are pure except
//! attribute stores.
//!
//! Control flow is if-converted rather than kept: lowering walks the program in address
//! order and tracks, per instruction, the condition under which a thread reaches it (its
//! *path condition*). Predicated instructions and instructions on a conditional path still
//! compute their result, and every register, predicate and attribute write becomes
//! `select(guard, new, old)`. This is exact for forward branches (`BRA`, `SSY`/`SYNC`,
//! `PBK`/`BRK`, predicated `EXIT`/`KIL`), since a thread's path through forward-only code
//! visits addresses in increasing order. Backward branches (loops) are rejected. Code with
//! no branches or predicates lowers to the same values as before (no guard is ever built).
//! A side benefit: the emitted WGSL has no control flow, so texture sampling never sits in
//! non-uniform control flow.
//!
//! Attribute addresses follow the public Maxwell+ layout used by Mesa/nouveau
//! (position at 0x70..0x7c, generic vector `i` at 0x80 + 16 * i).

use std::collections::BTreeMap;

use super::decode::{
    decode, word_at, BoolOp, Compare, Decoded, FCmp, FSrc, ICmp, Instr, IpaFreq, MufuOp, PSrc, Round, Src, TexHandle, TexLod,
    TexOp, XmadMode, LUT_A, LUT_B, LUT_C, PT, RZ,
};
use super::{ShaderStage, XlateError};

/// Upper bound on decoded instructions, so a missing `EXIT` cannot run away.
pub const MAX_INSTRUCTIONS: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Value(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    /// Raw 32-bit register bits.
    U32,
    F32,
    /// RGBA result of a texture read; only [`Op::Comp`] reads it.
    Vec4,
    /// Predicate / path condition.
    Bool,
}

/// Level of detail of a texture read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lod {
    Auto,
    Zero,
    Level(Value),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Const(u32),
    Cbuf { bank: u8, offset: u16 },
    /// Vertex input word.
    Attr { addr: u16 },
    /// Fragment input word; `mul` is the `PassMulW` multiplier.
    Interp { addr: u16, freq: IpaFreq, mul: Option<Value> },
    FNeg(Value),
    FAbs(Value),
    FSat(Value),
    FAdd(Value, Value),
    FMul(Value, Value),
    FFma(Value, Value, Value),
    Mufu(MufuOp, Value),
    /// 2D texture read at (`x`, `y`): floats for [`TexOp::Sample`], integer texel coordinates
    /// for [`TexOp::Fetch`].
    Tex { op: TexOp, handle: TexHandle, lod: Lod, x: Value, y: Value },
    /// Component `.1` (0..4) of a [`Op::Tex`] result.
    Comp(Value, u8),
    Bool(bool),
    FCmp(FCmp, Value, Value),
    /// Integer comparison; `.1` = signed.
    ICmp(ICmp, bool, Value, Value),
    BAnd(Value, Value),
    BOr(Value, Value),
    BXor(Value, Value),
    BNot(Value),
    /// `c ? a : b` on predicates.
    BSel(Value, Value, Value),
    /// `c ? a : b` on raw register bits.
    Sel(Value, Value, Value),
    IAdd(Value, Value),
    INeg(Value),
    /// Low 32 bits of the product.
    IMul(Value, Value),
    /// Shifts use the low 5 bits of the amount (WGSL semantics); lowering adds the clamp.
    Shl(Value, Value),
    ShrU(Value, Value),
    ShrS(Value, Value),
    And(Value, Value),
    Or(Value, Value),
    Xor(Value, Value),
    Not(Value),
    /// Bitwise three-input truth table (see [`LUT_A`]).
    Lop3(u8, Value, Value, Value),
    /// IEEE minNum/maxNum: a NaN operand yields the other one.
    FMin(Value, Value),
    FMax(Value, Value),
    /// Integer min/max; `.0` = signed.
    IMin(bool, Value, Value),
    IMax(bool, Value, Value),
    /// Integer to float; `.0` = signed.
    I2F(bool, Value),
    /// Float to integer, truncating, saturating, NaN to 0; `.0` = signed.
    F2I(bool, Value),
    /// Round to an integral float.
    FRound(Round, Value),
}

impl Op {
    pub fn ty(&self) -> Ty {
        match self {
            Op::Const(_) | Op::Cbuf { .. } => Ty::U32,
            Op::Tex { .. } => Ty::Vec4,
            Op::Bool(_)
            | Op::FCmp(..)
            | Op::ICmp(..)
            | Op::BAnd(..)
            | Op::BOr(..)
            | Op::BXor(..)
            | Op::BNot(_)
            | Op::BSel(..) => Ty::Bool,
            Op::Sel(..)
            | Op::IAdd(..)
            | Op::INeg(_)
            | Op::IMul(..)
            | Op::Shl(..)
            | Op::ShrU(..)
            | Op::ShrS(..)
            | Op::And(..)
            | Op::Or(..)
            | Op::Xor(..)
            | Op::Not(_)
            | Op::Lop3(..)
            | Op::IMin(..)
            | Op::IMax(..)
            | Op::F2I(..) => Ty::U32,
            _ => Ty::F32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrSlot {
    Position(u8),
    Generic { loc: u32, comp: u8 },
}

pub fn attr_slot(addr: u16) -> Option<AttrSlot> {
    if addr % 4 != 0 {
        return None;
    }
    match addr {
        0x70..=0x7c => Some(AttrSlot::Position(((addr - 0x70) / 4) as u8)),
        0x80..=0x27c => Some(AttrSlot::Generic { loc: ((addr - 0x80) / 16) as u32, comp: ((addr / 4) % 4) as u8 }),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct Function {
    pub stage: ShaderStage,
    /// `values[i]` defines `Value(i)`, in program order.
    pub values: Vec<Op>,
    /// Attribute stores in program order (later stores win).
    pub stores: Vec<(u16, Value)>,
    /// Register file at `EXIT` (index = register number; `None` = never written).
    pub exit_regs: Vec<Option<Value>>,
    /// Fragment stage: a [`Ty::Bool`] value, true when the shader discards the fragment.
    pub discard: Option<Value>,
    /// Decoded program text, one line per instruction, for logs and tests.
    pub listing: Vec<String>,
}

impl Function {
    pub fn op(&self, v: Value) -> Op {
        self.values[v.0 as usize]
    }
}

/// Condition under which a thread reaches an instruction, or executes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cond {
    True,
    V(Value),
}

struct Builder {
    values: Vec<Op>,
    regs: Vec<Option<Value>>,
    /// P0..P6; `None` = never written (reads as false).
    preds: [Option<Value>; 7],
    zero: Option<Value>,
    bools: [Option<Value>; 2],
    stores: Vec<(u16, Value)>,
    discard: Option<Value>,
}

impl Builder {
    fn push(&mut self, op: Op) -> Value {
        self.values.push(op);
        Value(self.values.len() as u32 - 1)
    }

    fn zero(&mut self) -> Value {
        if let Some(z) = self.zero {
            return z;
        }
        let z = self.push(Op::Const(0));
        self.zero = Some(z);
        z
    }

    fn konst(&mut self, bits: u32) -> Value {
        if bits == 0 { self.zero() } else { self.push(Op::Const(bits)) }
    }

    fn boolean(&mut self, b: bool) -> Value {
        if let Some(v) = self.bools[b as usize] {
            return v;
        }
        let v = self.push(Op::Bool(b));
        self.bools[b as usize] = Some(v);
        v
    }

    /// Registers never written read as zero (undefined on hardware).
    fn read(&mut self, r: u8) -> Value {
        if r == RZ {
            return self.zero();
        }
        match self.regs[r as usize] {
            Some(v) => v,
            None => self.zero(),
        }
    }

    fn write(&mut self, r: u8, v: Value, g: Cond) {
        if r == RZ {
            return;
        }
        let v = match g {
            Cond::True => v,
            Cond::V(c) => {
                let old = self.read(r);
                if old == v { v } else { self.push(Op::Sel(c, v, old)) }
            }
        };
        self.regs[r as usize] = Some(v);
    }

    fn pred_raw(&mut self, p: u8) -> Value {
        if p >= PT {
            return self.boolean(true);
        }
        match self.preds[p as usize] {
            Some(v) => v,
            None => self.boolean(false),
        }
    }

    fn pred(&mut self, p: PSrc) -> Value {
        let v = self.pred_raw(p.p);
        if p.neg { self.not(v) } else { v }
    }

    fn pwrite(&mut self, p: u8, v: Value, g: Cond) {
        if p >= PT {
            return;
        }
        let v = match g {
            Cond::True => v,
            Cond::V(c) => {
                let old = self.pred_raw(p);
                if old == v { v } else { self.push(Op::BSel(c, v, old)) }
            }
        };
        self.preds[p as usize] = Some(v);
    }

    fn store(&mut self, addr: u16, v: Value, g: Cond) {
        let v = match g {
            Cond::True => v,
            Cond::V(c) => {
                let old = match self.stores.iter().rev().find(|(a, _)| *a == addr) {
                    Some(&(_, o)) => o,
                    None => self.zero(),
                };
                self.push(Op::Sel(c, v, old))
            }
        };
        self.stores.push((addr, v));
    }

    fn not(&mut self, v: Value) -> Value {
        match self.values[v.0 as usize] {
            Op::Bool(b) => self.boolean(!b),
            Op::BNot(x) => x,
            _ => self.push(Op::BNot(v)),
        }
    }

    fn bop(&mut self, op: BoolOp, a: Value, b: Value) -> Value {
        let (ka, kb) = (self.bool_const(a), self.bool_const(b));
        match (op, ka, kb) {
            (BoolOp::And, Some(true), _) | (BoolOp::Or, Some(false), _) | (BoolOp::Xor, Some(false), _) => b,
            (BoolOp::And, _, Some(true)) | (BoolOp::Or, _, Some(false)) | (BoolOp::Xor, _, Some(false)) => a,
            (BoolOp::And, Some(false), _) | (BoolOp::And, _, Some(false)) => self.boolean(false),
            (BoolOp::Or, Some(true), _) | (BoolOp::Or, _, Some(true)) => self.boolean(true),
            (BoolOp::Xor, Some(true), _) => self.not(b),
            (BoolOp::Xor, _, Some(true)) => self.not(a),
            (BoolOp::And, ..) => self.push(Op::BAnd(a, b)),
            (BoolOp::Or, ..) => self.push(Op::BOr(a, b)),
            (BoolOp::Xor, ..) => self.push(Op::BXor(a, b)),
        }
    }

    fn bool_const(&self, v: Value) -> Option<bool> {
        match self.values[v.0 as usize] {
            Op::Bool(b) => Some(b),
            _ => None,
        }
    }

    fn src(&mut self, s: Src) -> Value {
        match s {
            Src::Reg(r) => self.read(r),
            Src::Imm(i) => self.konst(i),
            Src::Cbuf { bank, offset } => self.push(Op::Cbuf { bank, offset }),
        }
    }

    fn fsrc(&mut self, s: FSrc) -> Value {
        let mut v = self.src(s.src);
        if s.abs {
            v = self.push(Op::FAbs(v));
        }
        if s.neg {
            v = self.push(Op::FNeg(v));
        }
        v
    }

    fn sat(&mut self, v: Value, sat: bool) -> Value {
        if sat { self.push(Op::FSat(v)) } else { v }
    }

    fn int_const(&self, v: Value) -> Option<u32> {
        match self.values[v.0 as usize] {
            Op::Const(b) => Some(b),
            _ => None,
        }
    }

    fn iadd(&mut self, a: Value, b: Value) -> Value {
        match (self.int_const(a), self.int_const(b)) {
            (Some(0), _) => b,
            (_, Some(0)) => a,
            _ => self.push(Op::IAdd(a, b)),
        }
    }

    fn ineg(&mut self, a: Value, neg: bool) -> Value {
        if !neg {
            return a;
        }
        match self.int_const(a) {
            Some(k) => self.konst(k.wrapping_neg()),
            None => self.push(Op::INeg(a)),
        }
    }

    fn and_k(&mut self, a: Value, k: u32) -> Value {
        let k = self.konst(k);
        self.push(Op::And(a, k))
    }

    fn shl_k(&mut self, a: Value, k: u32) -> Value {
        if k == 0 {
            return a;
        }
        let k = self.konst(k);
        self.push(Op::Shl(a, k))
    }

    fn shr_k(&mut self, a: Value, k: u32, signed: bool) -> Value {
        let k = self.konst(k);
        self.push(if signed { Op::ShrS(a, k) } else { Op::ShrU(a, k) })
    }

    /// `c ? a : b` on bits, folding a constant `c`.
    fn sel(&mut self, c: Value, a: Value, b: Value) -> Value {
        match self.bool_const(c) {
            Some(true) => a,
            Some(false) => b,
            None if a == b => a,
            None => self.push(Op::Sel(c, a, b)),
        }
    }

    fn compare(&mut self, cmp: Compare) -> Value {
        match cmp {
            Compare::F { cmp, a, b } => match cmp {
                FCmp::F => self.boolean(false),
                FCmp::T => self.boolean(true),
                _ => {
                    let (x, y) = (self.fsrc(a), self.fsrc(b));
                    self.push(Op::FCmp(cmp, x, y))
                }
            },
            Compare::I { cmp, signed, a, b } => match cmp {
                ICmp::F => self.boolean(false),
                ICmp::T => self.boolean(true),
                _ => {
                    let (x, y) = (self.src(a), self.src(b));
                    self.push(Op::ICmp(cmp, signed, x, y))
                }
            },
            Compare::P { op, a, b } => {
                let (x, y) = (self.pred(a), self.pred(b));
                self.bop(op, x, y)
            }
        }
    }

    fn lop3(&mut self, lut: u8, a: Src, b: Src, c: Src) -> Value {
        let (x, y, z) = (self.src(a), self.src(b), self.src(c));
        match lut {
            0x00 => self.zero(),
            0xff => self.konst(u32::MAX),
            LUT_A => x,
            LUT_B => y,
            LUT_C => z,
            0x0f => self.push(Op::Not(x)),
            0x33 => self.push(Op::Not(y)),
            0x55 => self.push(Op::Not(z)),
            0xc0 => self.push(Op::And(x, y)),
            0xa0 => self.push(Op::And(x, z)),
            0x88 => self.push(Op::And(y, z)),
            0xfc => self.push(Op::Or(x, y)),
            0xfa => self.push(Op::Or(x, z)),
            0xee => self.push(Op::Or(y, z)),
            0x3c => self.push(Op::Xor(x, y)),
            0x5a => self.push(Op::Xor(x, z)),
            0x66 => self.push(Op::Xor(y, z)),
            _ => self.push(Op::Lop3(lut, x, y, z)),
        }
    }

    fn cond(&mut self, c: Cond) -> Value {
        match c {
            Cond::True => self.boolean(true),
            Cond::V(v) => v,
        }
    }

    fn and_cond(&mut self, c: Cond, v: Value) -> Option<Cond> {
        match (c, self.bool_const(v)) {
            (_, Some(false)) => None,
            (c, Some(true)) => Some(c),
            (Cond::True, None) => Some(Cond::V(v)),
            (Cond::V(x), None) => Some(Cond::V(self.push(Op::BAnd(x, v)))),
        }
    }

    fn or_cond(&mut self, a: Option<Cond>, b: Option<Cond>) -> Option<Cond> {
        match (a, b) {
            (None, x) | (x, None) => x,
            (Some(Cond::True), _) | (_, Some(Cond::True)) => Some(Cond::True),
            (Some(Cond::V(x)), Some(Cond::V(y))) => Some(Cond::V(self.push(Op::BOr(x, y)))),
        }
    }
}

/// Decodes and lowers the code that follows the header. `code` starts at the first
/// instruction; lowering ends where no thread can run any further.
pub fn lower(stage: ShaderStage, code: &[u8]) -> Result<Function, XlateError> {
    lower_with(
        stage,
        |pc| {
            let at = pc * 16;
            let w = word_at(code, pc).ok_or(XlateError::MissingExit)?;
            let d = decode(w).map_err(|e| XlateError::Decode { at, word: w, err: e })?;
            Ok((at, d))
        },
        |byte| (byte % 16 == 0).then_some(byte / 16),
    )
}

/// ISA-independent lowering: `fetch(pc)` returns the byte offset and decoded form of
/// instruction `pc` (Ampere via [`lower`], Maxwell via [`super::maxwell::lower`]), and
/// `pc_of(byte)` maps a branch target's byte offset back to an instruction index.
pub fn lower_with(
    stage: ShaderStage,
    mut fetch: impl FnMut(usize) -> Result<(usize, Decoded), XlateError>,
    pc_of: impl Fn(usize) -> Option<usize>,
) -> Result<Function, XlateError> {
    let mut b = Builder {
        values: Vec::new(),
        regs: vec![None; 255],
        preds: [None; 7],
        zero: None,
        bools: [None; 2],
        stores: Vec::new(),
        discard: None,
    };
    let mut listing = Vec::new();
    // Fallthrough into the current instruction, and branch conditions into later ones.
    let mut flow = Some(Cond::True);
    let mut pending: BTreeMap<usize, Cond> = BTreeMap::new();
    // Maxwell SSY / PBK reconvergence targets, innermost last.
    let (mut ssy, mut pbk): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    for pc in 0..MAX_INSTRUCTIONS {
        while ssy.last() == Some(&pc) {
            ssy.pop();
        }
        while pbk.last() == Some(&pc) {
            pbk.pop();
        }
        let incoming = pending.remove(&pc);
        let Some(path) = b.or_cond(flow, incoming) else {
            if pending.is_empty() {
                return Ok(Function {
                    stage,
                    values: b.values,
                    stores: b.stores,
                    exit_regs: b.regs,
                    discard: b.discard,
                    listing,
                });
            }
            // Unreachable instruction between a jump and its target.
            flow = None;
            continue;
        };
        let (at, d) = fetch(pc)?;
        listing.push(d.to_string());
        let unsup = |what: &'static str| XlateError::Unsupported { at, what };
        flow = Some(path);
        let p = PSrc { p: d.pred, neg: d.pred_neg };
        if p.p == PT && p.neg {
            // `@!PT`: never runs.
            continue;
        }
        // Guard for writes, and the threads on `path` that do not run the instruction.
        let (g, skip) = if p == PSrc::TRUE {
            (path, None)
        } else {
            let pv = b.pred(p);
            let npv = b.not(pv);
            match b.and_cond(path, pv) {
                Some(g) => (g, b.and_cond(path, npv)),
                None => continue,
            }
        };
        let target = |rel: i32| -> Result<usize, XlateError> {
            let t = at as i64 + rel as i64;
            let t = usize::try_from(t).ok().and_then(&pc_of).ok_or(unsup("branch target is not an instruction"))?;
            if t <= pc {
                return Err(unsup("backward branch (loop)"));
            }
            Ok(t)
        };
        let jump = |b: &mut Builder, pending: &mut BTreeMap<usize, Cond>, t: usize| {
            let merged = b.or_cond(pending.get(&t).copied(), Some(g));
            pending.insert(t, merged.unwrap_or(g));
        };
        match d.instr {
            Instr::Nop | Instr::Hint(_) => {}
            Instr::Exit => flow = skip,
            Instr::Kill => {
                if stage != ShaderStage::Fragment {
                    return Err(unsup("KIL outside a fragment shader"));
                }
                let gv = b.cond(g);
                b.discard = b.or_cond(b.discard.map(Cond::V), Some(Cond::V(gv))).map(|c| b.cond(c));
                flow = skip;
            }
            Instr::Bra { rel } => {
                let t = target(rel)?;
                jump(&mut b, &mut pending, t);
                flow = skip;
            }
            Instr::Sync | Instr::Brk => {
                let stack = if d.instr == Instr::Sync { &ssy } else { &pbk };
                let t = *stack.last().ok_or(unsup("SYNC/BRK without SSY/PBK"))?;
                jump(&mut b, &mut pending, t);
                flow = skip;
            }
            Instr::Ssy { rel } => ssy.push(target(rel)?),
            Instr::Pbk { rel } => pbk.push(target(rel)?),
            Instr::Mov { dst, src } => {
                let v = b.src(src);
                b.write(dst, v, g);
            }
            Instr::FAdd { dst, a, b: bb, sat, .. } => {
                let (x, y) = (b.fsrc(a), b.fsrc(bb));
                let v = b.push(Op::FAdd(x, y));
                let v = b.sat(v, sat);
                b.write(dst, v, g);
            }
            Instr::FMul { dst, a, b: bb, sat, .. } => {
                let (x, y) = (b.fsrc(a), b.fsrc(bb));
                let v = b.push(Op::FMul(x, y));
                let v = b.sat(v, sat);
                b.write(dst, v, g);
            }
            Instr::FFma { dst, a, b: bb, c, sat, .. } => {
                let (x, y, z) = (b.fsrc(a), b.fsrc(bb), b.fsrc(c));
                let v = b.push(Op::FFma(x, y, z));
                let v = b.sat(v, sat);
                b.write(dst, v, g);
            }
            Instr::Mufu { dst, op, src } => {
                let x = b.fsrc(src);
                let v = b.push(Op::Mufu(op, x));
                b.write(dst, v, g);
            }
            Instr::Ldc { dst, bank, offset } => {
                let v = b.push(Op::Cbuf { bank, offset });
                b.write(dst, v, g);
            }
            Instr::Ald { dst, addr, comps } => {
                if stage != ShaderStage::Vertex {
                    return Err(unsup("ALD outside a vertex shader"));
                }
                let mut vals = Vec::new();
                for i in 0..comps as u16 {
                    let a = addr + 4 * i;
                    if !matches!(attr_slot(a), Some(AttrSlot::Generic { .. })) {
                        return Err(unsup("ALD from a non-generic attribute"));
                    }
                    vals.push(b.push(Op::Attr { addr: a }));
                }
                for (i, v) in vals.into_iter().enumerate() {
                    b.write(dst.wrapping_add(i as u8), v, g);
                }
            }
            Instr::Ast { src, addr, comps } => {
                if stage != ShaderStage::Vertex {
                    return Err(unsup("AST outside a vertex shader"));
                }
                for i in 0..comps as u16 {
                    let a = addr + 4 * i;
                    if attr_slot(a).is_none() {
                        return Err(unsup("AST to an attribute other than position/generic"));
                    }
                    let v = b.read(src.wrapping_add(i as u8));
                    b.store(a, v, g);
                }
            }
            Instr::Ipa { dst, addr, freq, mul } => {
                if stage != ShaderStage::Fragment {
                    return Err(unsup("IPA outside a fragment shader"));
                }
                if attr_slot(addr).is_none() {
                    return Err(unsup("IPA from an attribute other than position/generic"));
                }
                let m = if freq == IpaFreq::PassMulW { Some(b.read(mul)) } else { None };
                let v = b.push(Op::Interp { addr, freq, mul: m });
                b.write(dst, v, g);
            }
            Instr::Tex(t) => {
                let (x, y) = (b.read(t.x), b.read(t.y));
                let lod = match t.lod {
                    TexLod::Auto => Lod::Auto,
                    TexLod::Zero => Lod::Zero,
                    TexLod::Level(r) => Lod::Level(b.read(r)),
                };
                let tex = b.push(Op::Tex { op: t.op, handle: t.handle, lod, x, y });
                // Read every source before writing: destinations may overlap the coordinates.
                for (c, &r) in t.dst.iter().enumerate() {
                    if r != RZ {
                        let v = b.push(Op::Comp(tex, c as u8));
                        b.write(r, v, g);
                    }
                }
            }
            Instr::Setp { pd, pd2, cmp, bop, c } => {
                let r = b.compare(cmp);
                let cv = b.pred(c);
                let v1 = b.bop(bop, r, cv);
                let v2 = if pd2 < PT {
                    let nr = b.not(r);
                    Some(b.bop(bop, nr, cv))
                } else {
                    None
                };
                b.pwrite(pd, v1, g);
                if let Some(v2) = v2 {
                    b.pwrite(pd2, v2, g);
                }
            }
            Instr::PLop3 { pd, lut, a, b: bb, c } => {
                // Through the bitwise table: each predicate as all-ones / zero.
                let (ones, zero) = (b.konst(u32::MAX), b.zero());
                let mut ins = [zero; 3];
                for (k, ps) in [a, bb, c].into_iter().enumerate() {
                    let pv = b.pred(ps);
                    ins[k] = b.sel(pv, ones, zero);
                }
                let r = b.push(Op::Lop3(lut, ins[0], ins[1], ins[2]));
                let v = b.push(Op::ICmp(ICmp::Ne, false, r, zero));
                b.pwrite(pd, v, g);
            }
            Instr::Set { dst, cmp, bop, c, bf } => {
                let r = b.compare(cmp);
                let cv = b.pred(c);
                let r = b.bop(bop, r, cv);
                let (t, f) = (b.konst(if bf { 1.0f32.to_bits() } else { u32::MAX }), b.zero());
                let v = b.sel(r, t, f);
                b.write(dst, v, g);
            }
            Instr::Sel { dst, a, b: bb, c } => {
                let (x, y) = (b.src(a), b.src(bb));
                let cv = b.pred(c);
                let v = b.sel(cv, x, y);
                b.write(dst, v, g);
            }
            Instr::FMnmx { dst, a, b: bb, c } => {
                let (x, y) = (b.fsrc(a), b.fsrc(bb));
                let cv = b.pred(c);
                let v = match b.bool_const(cv) {
                    Some(true) => b.push(Op::FMin(x, y)),
                    Some(false) => b.push(Op::FMax(x, y)),
                    None => {
                        let (lo, hi) = (b.push(Op::FMin(x, y)), b.push(Op::FMax(x, y)));
                        b.sel(cv, lo, hi)
                    }
                };
                b.write(dst, v, g);
            }
            Instr::IMnmx { dst, a, b: bb, signed, c } => {
                let (x, y) = (b.src(a), b.src(bb));
                let cv = b.pred(c);
                let v = match b.bool_const(cv) {
                    Some(true) => b.push(Op::IMin(signed, x, y)),
                    Some(false) => b.push(Op::IMax(signed, x, y)),
                    None => {
                        let (lo, hi) = (b.push(Op::IMin(signed, x, y)), b.push(Op::IMax(signed, x, y)));
                        b.sel(cv, lo, hi)
                    }
                };
                b.write(dst, v, g);
            }
            Instr::FCmpSel { dst, cmp, a, b: bb, c } => {
                let (x, y) = (b.src(a), b.src(bb));
                let z = FSrc { src: c, neg: false, abs: false };
                let zero = FSrc { src: Src::Imm(0), neg: false, abs: false };
                let r = b.compare(Compare::F { cmp, a: z, b: zero });
                let v = b.sel(r, x, y);
                b.write(dst, v, g);
            }
            Instr::IAdd { dst, a, b: bb, c, neg } => {
                let mut sum = b.zero();
                for (s, n) in [a, bb, c].into_iter().zip(neg) {
                    let v = b.src(s);
                    let v = b.ineg(v, n);
                    sum = b.iadd(sum, v);
                }
                b.write(dst, sum, g);
            }
            Instr::IScAdd { dst, a, b: bb, shift, neg } => {
                let (x, y) = (b.src(a), b.src(bb));
                let x = b.ineg(x, neg[0]);
                let y = b.ineg(y, neg[1]);
                let x = b.shl_k(x, shift as u32);
                let v = b.iadd(x, y);
                b.write(dst, v, g);
            }
            Instr::IMad { dst, a, b: bb, c } => {
                let (x, y, z) = (b.src(a), b.src(bb), b.src(c));
                let m = match (b.int_const(x), b.int_const(y)) {
                    (Some(0), _) | (_, Some(0)) => b.zero(),
                    (Some(1), _) => y,
                    (_, Some(1)) => x,
                    _ => b.push(Op::IMul(x, y)),
                };
                let v = b.iadd(m, z);
                b.write(dst, v, g);
            }
            Instr::Xmad { dst, a, b: bb, c, a_signed, b_signed, a_hi, b_hi, psl, mrg, mode } => {
                let (x, y, z) = (b.src(a), b.src(bb), b.src(c));
                let half = |b: &mut Builder, v: Value, hi: bool, signed: bool| {
                    if hi {
                        b.shr_k(v, 16, signed)
                    } else if signed {
                        let s = b.shl_k(v, 16);
                        b.shr_k(s, 16, true)
                    } else {
                        b.and_k(v, 0xffff)
                    }
                };
                let x16 = half(&mut b, x, a_hi, a_signed);
                let y16 = half(&mut b, y, b_hi, b_signed);
                let mut prod = b.push(Op::IMul(x16, y16));
                if psl {
                    prod = b.shl_k(prod, 16);
                }
                let addend = match mode {
                    XmadMode::None => z,
                    XmadMode::Clo => b.and_k(z, 0xffff),
                    XmadMode::Chi => b.shr_k(z, 16, false),
                    XmadMode::Cbcc => {
                        let s = b.shl_k(y, 16);
                        b.iadd(z, s)
                    }
                };
                let mut v = b.iadd(prod, addend);
                if mrg {
                    let lo = b.and_k(v, 0xffff);
                    let hi = b.shl_k(y, 16);
                    v = b.push(Op::Or(lo, hi));
                }
                b.write(dst, v, g);
            }
            Instr::Lop3 { dst, lut, a, b: bb, c } => {
                let v = b.lop3(lut, a, bb, c);
                b.write(dst, v, g);
            }
            Instr::Shl { dst, a, b: bb, wrap } => {
                let (x, y) = (b.src(a), b.src(bb));
                let v = match b.int_const(y) {
                    Some(k) if wrap || k < 32 => b.shl_k(x, k & 31),
                    Some(_) => b.zero(),
                    None => {
                        let s = b.push(Op::Shl(x, y));
                        if wrap {
                            s
                        } else {
                            let k32 = b.konst(32);
                            let ok = b.push(Op::ICmp(ICmp::Lt, false, y, k32));
                            let z = b.zero();
                            b.sel(ok, s, z)
                        }
                    }
                };
                b.write(dst, v, g);
            }
            Instr::Shr { dst, a, b: bb, signed, wrap } => {
                let (x, y) = (b.src(a), b.src(bb));
                let v = match b.int_const(y) {
                    Some(k) if wrap || k < 32 => {
                        if k & 31 == 0 { x } else { b.shr_k(x, k & 31, signed) }
                    }
                    Some(_) if signed => b.shr_k(x, 31, true),
                    Some(_) => b.zero(),
                    None if wrap => b.push(if signed { Op::ShrS(x, y) } else { Op::ShrU(x, y) }),
                    None if signed => {
                        let k31 = b.konst(31);
                        let amt = b.push(Op::IMin(false, y, k31));
                        b.push(Op::ShrS(x, amt))
                    }
                    None => {
                        let s = b.push(Op::ShrU(x, y));
                        let k32 = b.konst(32);
                        let ok = b.push(Op::ICmp(ICmp::Lt, false, y, k32));
                        let z = b.zero();
                        b.sel(ok, s, z)
                    }
                };
                b.write(dst, v, g);
            }
            Instr::I2F { dst, src, signed } => {
                let x = b.src(src);
                let v = b.push(Op::I2F(signed, x));
                b.write(dst, v, g);
            }
            Instr::F2I { dst, src, signed, round } => {
                let mut x = b.fsrc(src);
                if round != Round::Trunc {
                    x = b.push(Op::FRound(round, x));
                }
                let v = b.push(Op::F2I(signed, x));
                b.write(dst, v, g);
            }
            Instr::F2F { dst, src, round, sat } => {
                let mut x = b.fsrc(src);
                if let Some(r) = round {
                    x = b.push(Op::FRound(r, x));
                }
                let v = b.sat(x, sat);
                b.write(dst, v, g);
            }
        }
    }
    Err(XlateError::MissingExit)
}
