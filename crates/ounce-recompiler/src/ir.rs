//! Block IR: a straight-line list of operations ending in one exit or one two-way exit.
//!
//! The IR knows nothing about AArch64 decoding. `ounce-core` expresses guest semantics with these
//! operations (design section 3), so the recompiler never sees `ounce-core` types.
//!
//! Every value is a 64-bit integer. `Width::W32` operations read the low 32 bits of their inputs
//! and zero-extend their result, which is what AArch64 `W` registers do.

/// A value defined by an earlier operation of the same block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value(pub(crate) u32);

/// A guest register slot in [`crate::GuestState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg {
    /// `X0`..`X30`.
    X(u8),
    Sp,
    /// Packed flags, N/Z/C/V in bits 31..28.
    Nzcv,
    /// Low 64 bits of `V0`..`V31` (D register, or zero-extended S register).
    VLo(u8),
    /// High 64 bits of `V0`..`V31`.
    VHi(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    W32,
    W64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpWidth {
    F32,
    F64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpBinOp {
    Fadd,
    Fsub,
    Fmul,
    Fdiv,
    Fmax,
    Fmin,
}

/// Two-operand integer operations.
///
/// Shift and rotate amounts are taken modulo the width. `Udiv`/`Sdiv` by zero give 0, and
/// `Sdiv` of the most negative value by -1 gives the most negative value (no traps).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Shl,
    Lshr,
    Ashr,
    Rotr,
    Udiv,
    Sdiv,
}

/// Where a memory operation sits in the guest block, for precise faults: on a fault the block
/// exits with `pc` and `insns` guest instructions completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaultSite {
    pub pc: u64,
    pub insns: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Const(u64),
    GetReg(Reg),
    /// No result.
    SetReg(Reg, Value),
    Bin(BinOp, Width, Value, Value),
    Not(Width, Value),
    /// Sign-extends the low `from_bits` (8, 16, 32 or 64) bits.
    Sext(u8, Value),
    /// Zero-extends the low `from_bits` (8, 16, 32 or 64) bits.
    Zext(u8, Value),
    /// NZCV (bits 31..28) of `a + b`.
    AddFlags(Width, Value, Value),
    /// NZCV (bits 31..28) of `a - b`, with C = no borrow.
    SubFlags(Width, Value, Value),
    /// N and Z of a logical result; C and V clear.
    LogicFlags(Width, Value),
    /// 1 if the AArch64 condition code (0..=15) holds for `nzcv`, else 0.
    Cond(u8, Value),
    /// 1 if the value is zero at this width, else 0.
    IsZero(Width, Value),
    /// `if c != 0 { a } else { b }`.
    Select(Value, Value, Value),
    /// Binary floating-point operation.
    FpBin(FpBinOp, FpWidth, Value, Value),
    /// NZCV (bits 31..28) of float compare `a vs b`.
    FpCompare(FpWidth, Value, Value),
    /// Reads `size` (1, 2, 4, 8) bytes through the memory bus, zero-extended.
    Load(u8, Value, FaultSite),
    /// Writes the low `size` bytes of the value. No result.
    Store(u8, Value, Value, FaultSite),
    /// Instruction boundary after a store: if a store in this block wrote cached code, exit with
    /// `pc = next_pc` and `insns` completed. No result.
    EndInsn { next_pc: u64, insns: u32 },
}

impl Op {
    fn has_result(&self) -> bool {
        !matches!(self, Op::SetReg(..) | Op::Store(..) | Op::EndInsn { .. })
    }

    fn operands(&self) -> Vec<Value> {
        match *self {
            Op::Const(_) | Op::GetReg(_) | Op::EndInsn { .. } => vec![],
            Op::SetReg(_, v) | Op::Not(_, v) | Op::Sext(_, v) | Op::Zext(_, v) | Op::LogicFlags(_, v) => vec![v],
            Op::Cond(_, v) | Op::IsZero(_, v) | Op::Load(_, v, _) => vec![v],
            Op::Bin(_, _, a, b)
            | Op::AddFlags(_, a, b)
            | Op::SubFlags(_, a, b)
            | Op::Store(_, a, b, _)
            | Op::FpBin(_, _, a, b)
            | Op::FpCompare(_, a, b) => vec![a, b],
            Op::Select(c, a, b) => vec![c, a, b],
        }
    }
}

/// One way out of a block: the guest `pc` to continue at, a caller-defined `code`, and how many
/// guest instructions ran. Codes at or above [`RESERVED_CODES`] are used by the recompiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitTo {
    pub code: u32,
    pub pc: Value,
    pub insns: u32,
}

/// Exit codes from this value up are reserved (fault, code written).
pub const RESERVED_CODES: u32 = 0xFFFF_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminator {
    Exit(ExitTo),
    /// `taken` if `cond != 0`, else `not_taken`.
    Branch { cond: Value, taken: ExitTo, not_taken: ExitTo },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub(crate) ops: Vec<Op>,
    pub(crate) term: Terminator,
}

impl Block {
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    pub fn terminator(&self) -> &Terminator {
        &self.term
    }

    /// Checks that every operand is the result of an earlier operation and every immediate is in
    /// range.
    pub fn validate(&self) -> Result<(), String> {
        let defined = |v: Value, at: usize| -> Result<(), String> {
            match self.ops.get(v.0 as usize) {
                Some(op) if (v.0 as usize) < at && op.has_result() => Ok(()),
                _ => Err(format!("op {at} uses undefined value {}", v.0)),
            }
        };
        for (i, op) in self.ops.iter().enumerate() {
            for v in op.operands() {
                defined(v, i)?;
            }
            match *op {
                Op::GetReg(Reg::X(n)) | Op::SetReg(Reg::X(n), _) if n > 30 => {
                    return Err(format!("op {i}: X{n} out of range"));
                }
                Op::GetReg(Reg::VLo(n) | Reg::VHi(n)) | Op::SetReg(Reg::VLo(n) | Reg::VHi(n), _) if n > 31 => {
                    return Err(format!("op {i}: V{n} out of range"));
                }
                Op::Sext(bits, _) | Op::Zext(bits, _) if !matches!(bits, 8 | 16 | 32 | 64) => {
                    return Err(format!("op {i}: bad extend width {bits}"));
                }
                Op::Load(size, ..) | Op::Store(size, ..) if !matches!(size, 1 | 2 | 4 | 8) => {
                    return Err(format!("op {i}: bad access size {size}"));
                }
                Op::Cond(cc, _) if cc > 15 => return Err(format!("op {i}: bad condition {cc}")),
                _ => {}
            }
        }
        let end = self.ops.len();
        let exit_ok = |e: &ExitTo| -> Result<(), String> {
            if e.code >= RESERVED_CODES {
                return Err(format!("exit code {:#x} is reserved", e.code));
            }
            defined(e.pc, end)
        };
        match &self.term {
            Terminator::Exit(e) => exit_ok(e),
            Terminator::Branch { cond, taken, not_taken } => {
                defined(*cond, end)?;
                exit_ok(taken)?;
                exit_ok(not_taken)
            }
        }
    }
}

/// Appends operations and hands out their result values.
#[derive(Debug, Default)]
pub struct BlockBuilder {
    ops: Vec<Op>,
}

impl BlockBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of operations so far.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn push(&mut self, op: Op) -> Value {
        self.ops.push(op);
        Value((self.ops.len() - 1) as u32)
    }

    pub fn konst(&mut self, v: u64) -> Value {
        self.push(Op::Const(v))
    }

    pub fn get(&mut self, reg: Reg) -> Value {
        self.push(Op::GetReg(reg))
    }

    pub fn set(&mut self, reg: Reg, v: Value) {
        self.push(Op::SetReg(reg, v));
    }

    pub fn bin(&mut self, op: BinOp, w: Width, a: Value, b: Value) -> Value {
        self.push(Op::Bin(op, w, a, b))
    }

    /// `bin` with a constant right-hand side.
    pub fn bin_imm(&mut self, op: BinOp, w: Width, a: Value, imm: u64) -> Value {
        let b = self.konst(imm);
        self.bin(op, w, a, b)
    }

    pub fn finish(self, term: Terminator) -> Block {
        Block { ops: self.ops, term }
    }
}
