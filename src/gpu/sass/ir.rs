//! Small SSA IR for straight-line shaders, and the lowering from decoded SASS.
//!
//! Step 1 has no control flow, so SSA is plain renaming: every instruction result is a
//! fresh [`Value`], a register map tracks the latest value per register, and `MOV`
//! just aliases. All ops are pure except attribute stores.
//!
//! Attribute addresses follow the public Maxwell+ layout used by Mesa/nouveau
//! (position at 0x70..0x7c, generic vector `i` at 0x80 + 16 * i).

use super::decode::{decode, word_at, FSrc, Instr, IpaFreq, MufuOp, Src, PT, RZ};
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
}

impl Op {
    pub fn ty(&self) -> Ty {
        match self {
            Op::Const(_) | Op::Cbuf { .. } => Ty::U32,
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
    /// Decoded program text, one line per instruction, for logs and tests.
    pub listing: Vec<String>,
}

impl Function {
    pub fn op(&self, v: Value) -> Op {
        self.values[v.0 as usize]
    }
}

struct Builder {
    values: Vec<Op>,
    regs: Vec<Option<Value>>,
    zero: Option<Value>,
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

    fn write(&mut self, r: u8, v: Value) {
        if r != RZ {
            self.regs[r as usize] = Some(v);
        }
    }

    fn src(&mut self, s: Src) -> Value {
        match s {
            Src::Reg(r) => self.read(r),
            Src::Imm(i) => self.push(Op::Const(i)),
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
}

/// Decodes and lowers the code that follows the header. `code` starts at the first
/// instruction; decoding stops at the first unconditional `EXIT`.
pub fn lower(stage: ShaderStage, code: &[u8]) -> Result<Function, XlateError> {
    let mut b = Builder { values: Vec::new(), regs: vec![None; 255], zero: None };
    let mut stores = Vec::new();
    let mut listing = Vec::new();
    for pc in 0..MAX_INSTRUCTIONS {
        let at = pc * 16;
        let w = word_at(code, pc).ok_or(XlateError::MissingExit)?;
        let d = decode(w).map_err(|e| XlateError::Decode { at, word: w, err: e })?;
        listing.push(d.to_string());
        let unsup = |what: &'static str| XlateError::Unsupported { at, what };
        if d.pred != PT || d.pred_neg {
            if d.instr == Instr::Nop {
                continue;
            }
            return Err(unsup("predicated instruction (no control flow in step 1)"));
        }
        match d.instr {
            Instr::Nop => {}
            Instr::Exit => {
                return Ok(Function { stage, values: b.values, stores, exit_regs: b.regs, listing });
            }
            Instr::Mov { dst, src } => {
                let v = b.src(src);
                b.write(dst, v);
            }
            Instr::FAdd { dst, a, b: bb, sat, .. } => {
                let (x, y) = (b.fsrc(a), b.fsrc(bb));
                let v = b.push(Op::FAdd(x, y));
                let v = b.sat(v, sat);
                b.write(dst, v);
            }
            Instr::FMul { dst, a, b: bb, sat, .. } => {
                let (x, y) = (b.fsrc(a), b.fsrc(bb));
                let v = b.push(Op::FMul(x, y));
                let v = b.sat(v, sat);
                b.write(dst, v);
            }
            Instr::FFma { dst, a, b: bb, c, sat, .. } => {
                let (x, y, z) = (b.fsrc(a), b.fsrc(bb), b.fsrc(c));
                let v = b.push(Op::FFma(x, y, z));
                let v = b.sat(v, sat);
                b.write(dst, v);
            }
            Instr::Mufu { dst, op, src } => {
                let x = b.fsrc(src);
                let v = b.push(Op::Mufu(op, x));
                b.write(dst, v);
            }
            Instr::Ldc { dst, bank, offset } => {
                let v = b.push(Op::Cbuf { bank, offset });
                b.write(dst, v);
            }
            Instr::Ald { dst, addr, comps } => {
                if stage != ShaderStage::Vertex {
                    return Err(unsup("ALD outside a vertex shader"));
                }
                for i in 0..comps as u16 {
                    let a = addr + 4 * i;
                    if !matches!(attr_slot(a), Some(AttrSlot::Generic { .. })) {
                        return Err(unsup("ALD from a non-generic attribute"));
                    }
                    let v = b.push(Op::Attr { addr: a });
                    b.write(dst.wrapping_add(i as u8), v);
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
                    stores.push((a, v));
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
                b.write(dst, v);
            }
        }
    }
    Err(XlateError::MissingExit)
}
