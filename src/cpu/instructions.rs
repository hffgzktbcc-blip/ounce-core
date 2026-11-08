#![allow(dead_code)]

pub use super::pauth::{PacKey, PacModifier};

/// AArch64 Condition Codes for conditional branching (`B.cond`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Eq = 0x0, // Equal (Z == 1)
    Ne = 0x1, // Not Equal (Z == 0)
    Cs = 0x2, // Carry Set / Unsigned Higher or Same (C == 1)
    Cc = 0x3, // Carry Clear / Unsigned Lower (C == 0)
    Mi = 0x4, // Minus / Negative (N == 1)
    Pl = 0x5, // Plus / Positive or Zero (N == 0)
    Vs = 0x6, // Overflow (V == 1)
    Vc = 0x7, // No Overflow (V == 0)
    Hi = 0x8, // Unsigned Higher (C == 1 && Z == 0)
    Ls = 0x9, // Unsigned Lower or Same (C == 0 || Z == 1)
    Ge = 0xA, // Signed Greater Than or Equal (N == V)
    Lt = 0xB, // Signed Less Than (N != V)
    Gt = 0xC, // Signed Greater Than (Z == 0 && N == V)
    Le = 0xD, // Signed Less Than or Equal (Z == 1 || N != V)
    Al = 0xE, // Always
    Nv = 0xF, // Always (never tested in practice)
}

impl Condition {
    pub fn from_u8(val: u8) -> Self {
        match val & 0xF {
            0x0 => Condition::Eq,
            0x1 => Condition::Ne,
            0x2 => Condition::Cs,
            0x3 => Condition::Cc,
            0x4 => Condition::Mi,
            0x5 => Condition::Pl,
            0x6 => Condition::Vs,
            0x7 => Condition::Vc,
            0x8 => Condition::Hi,
            0x9 => Condition::Ls,
            0xA => Condition::Ge,
            0xB => Condition::Lt,
            0xC => Condition::Gt,
            0xD => Condition::Le,
            0xE => Condition::Al,
            _ => Condition::Nv,
        }
    }
}

/// Decoded AArch64 Instructions supported by the Phase 1 core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    /// PC-relative address calculation: `ADR Xd, imm` or `ADRP Xd, imm`
    Adr {
        rd: u8,
        imm: i64,
        is_page: bool, // true if ADRP
    },

    /// Add / Subtract (Immediate): `ADD/SUB{S} Xd|Wd, Xn|Wn|SP, #imm{, LSL #12}`
    AddSubImm {
        rd: u8,
        rn: u8,
        imm: u64,
        is_sub: bool,
        set_flags: bool,
        is_64: bool,
    },

    /// Add / Subtract (Shifted or Extended Register): `ADD/SUB{S} Xd, Xn, Xm{, shift/extend}`
    AddSubReg {
        rd: u8,
        rn: u8,
        rm: u8,
        shift_type: ShiftType,
        shift_amount: u8,
        is_extended: bool,
        extend_option: u8,
        is_sub: bool,
        set_flags: bool,
        is_64: bool,
    },

    /// Logical (Shifted Register): `AND/BIC/ORR/ORN/EOR/EON/ANDS/BICS Xd, Xn, Xm{, shift #imm}`
    LogicalReg {
        rd: u8,
        rn: u8,
        rm: u8,
        shift_type: ShiftType,
        shift_amount: u8,
        invert_rm: bool,
        op: LogicalOp,
        set_flags: bool,
        is_64: bool,
    },

    /// Logical (Immediate): `AND/ORR/EOR/ANDS Xd, Xn, #bitmask_imm`
    LogicalImm {
        rd: u8,
        rn: u8,
        imm: u64,
        op: LogicalOp,
        set_flags: bool,
        is_64: bool,
    },

    /// Bitfield Move: `SBFM/BFM/UBFM Xd, Xn, #immr, #imms` (includes `LSL/LSR/ASR/UXTB/UXTH/SXTW/UBFX/BFI`)
    Bitfield {
        rd: u8,
        rn: u8,
        immr: u8,
        imms: u8,
        opc: u8,
        is_64: bool,
    },

    /// Extract Register: `EXTR/ROR Xd, Xn, Xm, #lsb`
    Extract {
        rd: u8,
        rn: u8,
        rm: u8,
        lsb: u8,
        is_64: bool,
    },

    /// Conditional Select: `CSEL/CSINC/CSINV/CSNEG Xd, Xn, Xm, cond`
    CondSelect {
        rd: u8,
        rn: u8,
        rm: u8,
        cond: Condition,
        op: u8, // 0=CSEL, 1=CSINC, 2=CSINV, 3=CSNEG
        is_64: bool,
    },

    /// Conditional Compare: `CCMP/CCMN Xn, Xm|#imm, #nzcv, cond`
    CondCompare {
        rn: u8,
        rm_or_imm: u8,
        nzcv: u8,
        cond: Condition,
        is_imm: bool,
        is_sub: bool,
        is_64: bool,
    },

    /// Test Bit and Branch: `TBZ/TBNZ Rt, #bit, label`
    TestBranch {
        rt: u8,
        bit: u8,
        offset: i64,
        is_tbnz: bool,
    },

    /// Single Load / Store (Register Offset): `LDR/STR Rt, [Xn, Xm, LSL/SXTW #shift]`
    LoadStoreReg {
        rt: u8,
        rn: u8,
        rm: u8,
        option: u8,
        shift: u8,
        size_bytes: u8,
        is_load: bool,
        is_signed: bool,
        is_64_target: bool,
    },

    /// Move Wide: `MOVZ/MOVN/MOVK Xd, #imm{, LSL #shift}`
    MoveWide {
        rd: u8,
        imm: u16,
        shift: u8,
        op: MoveWideOp,
        is_64: bool,
    },

    /// Unconditional Branch (Immediate): `B label` or `BL label`
    BranchImm {
        offset: i64,
        link: bool, // true if BL (saves return address in X30)
    },

    /// Conditional Branch: `B.cond label`
    BranchCond {
        offset: i64,
        cond: Condition,
    },

    /// Branch to Register: `BR/BLR/RET Xn`
    BranchReg {
        rn: u8,
        op: BranchRegOp,
    },

    /// Authenticated branch to register: `BRAA/BRAB/BLRAA/BLRAB Xn, Xm|SP`, the `*Z` forms
    /// (modifier 0) and `RETAA/RETAB` (`rn` = 30, modifier SP).
    BranchRegAuth {
        rn: u8,
        op: BranchRegOp,
        key: PacKey,
        modifier: PacModifier,
    },

    /// Pointer authentication on `rd`: `PAC*`/`AUT*`/`XPAC*`, including the HINT forms
    /// (`PACIASP`, `AUTIA1716`, `XPACLRI`, ...). `key` is IA/DA for `XPACI`/`XPACD`.
    Pauth {
        rd: u8,
        op: PauthOp,
        key: PacKey,
        modifier: PacModifier,
    },

    /// `PACGA Xd, Xn, Xm|SP`
    Pacga {
        rd: u8,
        rn: u8,
        rm: u8,
    },

    /// `LDRAA/LDRAB Xt, [Xn|SP, #simm]{!}`: authenticates `Xn` with the DA/DB key and modifier 0.
    LoadAuth {
        rt: u8,
        rn: u8,
        offset: i64,
        key: PacKey,
        writeback: bool,
    },

    /// Compare and Branch: `CBZ/CBNZ Xn|Wn, label`
    CompareBranch {
        rt: u8,
        offset: i64,
        is_cbnz: bool,
        is_64: bool,
    },

    /// Single Load / Store (Immediate / Unscaled / Pre/Post Index): `LDR/STR Rt, [Xn, #offset]`
    LoadStoreImm {
        rt: u8,
        rn: u8,
        offset: i64,
        size_bytes: u8, // 1 (byte), 2 (half), 4 (word), 8 (double)
        is_load: bool,
        is_signed: bool,
        is_64_target: bool,
        writeback: bool,
        post_index: bool,
    },

    /// Load / Store Pair: `LDP/STP Rt1, Rt2, [Xn, #offset]`
    LoadStorePair {
        rt1: u8,
        rt2: u8,
        rn: u8,
        offset: i64,
        is_load: bool,
        is_64: bool,
        writeback: bool,
        post_index: bool,
    },

    /// System Call / Supervisor Call: `SVC #imm16`
    Svc {
        imm: u16,
    },

    /// System Register Read / Write: `MRS/MSR`
    SystemReg {
        rt: u8,
        sysreg: u16,
        is_read: bool,
    },

    /// No Operation / Hint
    Nop,

    /// Floating-Point 2-Source Arithmetic: `FADD/FSUB/FMUL/FDIV Dd/Sd, Dn/Sn, Dm/Sm`
    FpData2 {
        rd: u8,
        rn: u8,
        rm: u8,
        op: FpOp2,
        is_double: bool,
    },

    /// Floating-Point 1-Source: `FMOV`/`FABS`/`FNEG`/`FSQRT Dd/Sd, Dn/Sn` (op 0..3) and
    /// `FCVT` (op = bits 17..15: 4 to single from double, 5 to double from single)
    FpData1 {
        rd: u8,
        rn: u8,
        op: u8,
        is_double: bool,
    },

    /// Floating-Point 3-Source: `FMADD` (o1 0, o0 0), `FMSUB` (0, 1), `FNMADD` (1, 0), `FNMSUB` (1, 1),
    /// fused (one rounding): `Dd = (o1 ? -Da : Da) + (o0 ? -Dn : Dn) * Dm`
    FpData3 {
        rd: u8,
        rn: u8,
        rm: u8,
        ra: u8,
        o1: bool,
        o0: bool,
        is_double: bool,
    },

    /// Floating-Point round to integral: `FRINT{N,P,M,Z,A,X,I}` (op = bits 17..15 of the encoding)
    FpRound {
        rd: u8,
        rn: u8,
        op: u8,
        is_double: bool,
    },

    /// Float <-> integer register conversion: `FCVT{N,P,M,Z,A}{S,U}` (to integer), `SCVTF`, `UCVTF`.
    /// `opcode` is bits 18..16 (0 S, 1 U, 2 SCVTF, 3 UCVTF, 4 FCVTAS, 5 FCVTAU), `rmode` bits 20..19.
    FpIntConv {
        rd: u8,
        rn: u8,
        opcode: u8,
        rmode: u8,
        is_64: bool,
        is_double: bool,
    },

    /// Float <-> fixed-point general register conversion: `FCVTZS`/`FCVTZU` (opcode 0/1, to integer) and
    /// `SCVTF`/`UCVTF` (opcode 2/3, to float), scaled by 2^`fbits`.
    FpFixedConv {
        rd: u8,
        rn: u8,
        opcode: u8,
        fbits: u8,
        is_64: bool,
        is_double: bool,
    },

    /// Floating-Point Conditional Select: `FCSEL Dd/Sd, Dn/Sn, Dm/Sm, cond`
    FpCondSelect {
        rd: u8,
        rn: u8,
        rm: u8,
        cond: Condition,
        is_double: bool,
    },

    /// Floating-Point Compare: `FCMP Dn/Sn, Dm/Sm` or `FCMP Dn/Sn, #0.0`
    FpCompare {
        rn: u8,
        rm: u8,
        is_zero: bool,
        is_double: bool,
    },

    /// Floating-Point / GP Register Transfer: `FMOV Dd, Xn`, `FMOV Xd, Dn`, etc.
    FpMovGen {
        rd: u8,
        rn: u8,
        to_fp: bool,
        is_64: bool,
    },

    /// Floating-Point Single Load / Store (Immediate unsigned offset): `LDR/STR St/Dt, [Xn, #offset]`
    FpLoadStoreImm {
        rt: u8,
        rn: u8,
        offset: i64,
        is_load: bool,
        is_double: bool,
    },

    /// Integer Multiply-Add / Multiply-Subtract (`MADD`, `MSUB`, `MUL`): `Rd = Ra +/- Rn * Rm`
    MultiplyAddSub {
        rd: u8,
        rn: u8,
        rm: u8,
        ra: u8,
        is_sub: bool,
        is_64: bool,
    },

    /// Integer Divide (`UDIV` / `SDIV`)
    Divide {
        rd: u8,
        rn: u8,
        rm: u8,
        is_signed: bool,
        is_64: bool,
    },

    /// Variable Shift (`LSLV`, `LSRV`, `ASRV`, `RORV`)
    ShiftReg {
        rd: u8,
        rn: u8,
        rm: u8,
        shift_type: ShiftType,
        is_64: bool,
    },

    /// 128-bit NEON Vector Load / Store (`LDR Qt, [Xn, #offset]` / `STR Qt, [Xn, #offset]`)
    SimdLoadStoreQ {
        rt: u8,
        rn: u8,
        offset: i64,
        is_load: bool,
    },

    /// Two double-precision lanes; never interpreted as four single-precision values.
    SimdVec2D { rd: u8, rn: u8, rm: u8, op: FpOp2 },

    /// 128-bit NEON vector arithmetic on four single-precision lanes.
    SimdVec4S {
        rd: u8,
        rn: u8,
        rm: u8,
        op: FpOp2,
    },

    /// Exclusive & Acquire/Release Load / Store (`LDXR`, `LDAXR`, `LDAR`, `STXR`, `STLXR`, `STLR`)
    LoadStoreExclusive {
        rt: u8,
        rn: u8,
        rs: u8,
        size_bytes: u8,
        is_load: bool,
        is_exclusive: bool,
    },

    /// SIMD&FP Load / Store Pair: `LDP/STP St/Dt/Qt, St2/Dt2/Qt2, [Xn, #offset]` (also `LDNP/STNP`)
    SimdLoadStorePair {
        rt1: u8,
        rt2: u8,
        rn: u8,
        offset: i64,
        size_bytes: u8, // 4, 8 or 16
        is_load: bool,
        writeback: bool,
        post_index: bool,
    },

    /// `DC ZVA, Xt`: zero the `DCZID_EL0`-sized block containing `Xt`
    DcZva {
        rt: u8,
    },

    /// Data-Processing (1-Source): `RBIT`, `REV16`, `REV32`, `REV`, `CLZ`, `CLS`
    DataProc1 {
        rd: u8,
        rn: u8,
        op: DataProc1Op,
        is_64: bool,
    },

    /// Multiply High: `UMULH` / `SMULH Xd, Xn, Xm` (upper 64 bits of the 128-bit product)
    MultiplyHigh {
        rd: u8,
        rn: u8,
        rm: u8,
        is_signed: bool,
    },

    /// Multiply Long: `UMADDL`/`SMADDL`/`UMSUBL`/`SMSUBL Xd, Wn, Wm, Xa`
    MultiplyLong {
        rd: u8,
        rn: u8,
        rm: u8,
        ra: u8,
        is_signed: bool,
        is_sub: bool,
    },

    /// SIMD&FP Load / Store (Unscaled, Pre/Post-Index): `LDUR/STUR Bt/Ht/St/Dt/Qt, [Xn, #imm9]`
    SimdLoadStore {
        rt: u8,
        rn: u8,
        offset: i64,
        size_bytes: u8, // 1, 2, 4, 8 or 16
        is_load: bool,
        writeback: bool,
        post_index: bool,
    },

    /// SIMD&FP Load / Store (Register Offset): `LDR/STR Bt..Qt, [Xn, Xm, LSL/UXTW/SXTW/SXTX #shift]`
    SimdLoadStoreReg {
        rt: u8,
        rn: u8,
        rm: u8,
        option: u8,
        shift: u8,
        size_bytes: u8,
        is_load: bool,
    },

    /// `LD1`/`ST1` (multiple structures, 1 to 4 consecutive registers), optionally post-indexed
    SimdLoadStoreMulti {
        rt: u8,
        rn: u8,
        /// Post-index register; 31 means "advance by the bytes transferred".
        rm: u8,
        regs: u8,
        q: bool,
        is_load: bool,
        post_index: bool,
    },

    /// `LD2`-`LD4`/`ST2`-`ST4` (multiple structures): `selem` registers, `esize`-bit elements interleaved
    /// in memory, optionally post-indexed
    SimdLoadStoreStruct {
        rt: u8,
        rn: u8,
        /// Post-index register; 31 means "advance by the bytes transferred".
        rm: u8,
        selem: u8,
        esize: u8,
        q: bool,
        is_load: bool,
        post_index: bool,
    },

    /// AdvSIMD scalar two-register miscellaneous (`SCVTF`/`UCVTF`/`FCVTZS`/`FCVTZU Sd/Dd, Sn/Dn`): lane 0 only,
    /// the rest of `Vd` is zeroed
    SimdScalarTwoMisc {
        rd: u8,
        rn: u8,
        op: SimdOp2,
        esize: u8,
    },

    /// `DUP Vd.<T>, Wn/Xn`
    SimdDupGeneral {
        rd: u8,
        rn: u8,
        esize: u8, // element bits
        q: bool,
    },

    /// `UMOV Wd/Xd, Vn.<T>[index]` (`MOV` alias for 32/64-bit lanes)
    SimdMovToGeneral {
        rd: u8,
        rn: u8,
        esize: u8,
        index: u8,
    },

    /// `ADDHN{2}`/`SUBHN{2}`/`RADDHN{2}`/`RSUBHN{2}`: `esize` is the narrow (result) lane size; `q` writes the high half
    SimdNarrow {
        rd: u8,
        rn: u8,
        rm: u8,
        op: SimdNarrowOp,
        esize: u8,
        q: bool,
    },

    /// `SQDMULL{2}`/`SQDMLAL{2}`/`SQDMLSL{2}` (vector): `esize` is the source lane size (16 or 32)
    SimdSatLong {
        rd: u8,
        rn: u8,
        rm: u8,
        op: SimdSatLongOp,
        esize: u8,
        q: bool,
    },

    /// AdvSIMD three registers of different type: widening multiply (`S/UMULL`, `S/UMLAL`, `S/UMLSL`
    /// and their `2` forms). `esize` is the source lane size; `q` selects the high source half.
    SimdLong {
        rd: u8,
        rn: u8,
        rm: u8,
        op: SimdLongOp,
        signed: bool,
        esize: u8,
        q: bool,
    },

    /// `PMULL` / `PMULL2`: polynomial (carry-less) multiply long. `wide` selects the 64-bit form
    /// (`Vd.1Q` from `Vn.1D`, `Vm.1D`), otherwise 8-bit lanes widen to 16 bits (`Vd.8H` from `Vn.8B`).
    /// `q` selects the high source half.
    Pmull {
        rd: u8,
        rn: u8,
        rm: u8,
        wide: bool,
        q: bool,
    },

    /// AdvSIMD three registers of the same type (integer and bitwise)
    SimdThreeSame {
        rd: u8,
        rn: u8,
        rm: u8,
        op: SimdOp3,
        esize: u8,
        q: bool,
    },

    /// AdvSIMD two-register miscellaneous (compare against zero, `CNT`, `NOT`, `ABS`, `NEG`)
    /// AdvSIMD across lanes: reduces every lane of `Vn` into scalar `Vd`.
    SimdAcross {
        rd: u8,
        rn: u8,
        op: SimdAcrossOp,
        esize: u8,
        q: bool,
    },

    SimdTwoMisc {
        rd: u8,
        rn: u8,
        op: SimdOp2,
        esize: u8,
        q: bool,
    },

    /// AdvSIMD shift by immediate: `SSHR`, `USHR`, `SHL`, `SHRN`/`SHRN2`.
    /// `esize` is the element size of the result (for `SHRN`, half the source element).
    SimdShiftImm {
        rd: u8,
        rn: u8,
        op: SimdShiftOp,
        esize: u8,
        shift: u8,
        q: bool,
    },

    /// AdvSIMD modified immediate: `MOVI`/`MVNI`/`FMOV` (vector) and `ORR`/`BIC` (vector, immediate).
    /// `imm` is the expanded 64-bit pattern (already inverted for `MVNI`), repeated in both halves when `q`.
    SimdModImm {
        rd: u8,
        imm: u64,
        op: SimdImmOp,
        q: bool,
    },

    /// `DUP Bd/Hd/Sd/Dd, Vn.<Ts>[index]` (scalar; `MOV Sd, Vn.S[i]` alias): lane `index` into lane 0, rest zero
    SimdDupScalar {
        rd: u8,
        rn: u8,
        esize: u8,
        index: u8,
    },

    /// `DUP Vd.<T>, Vn.<Ts>[index]`
    SimdDupElement {
        rd: u8,
        rn: u8,
        esize: u8,
        index: u8,
        q: bool,
    },

    /// `INS Vd.<Ts>[index], Rn` (`MOV` alias)
    SimdInsGeneral {
        rd: u8,
        rn: u8,
        esize: u8,
        index: u8,
    },

    /// `INS Vd.<Ts>[dst], Vn.<Ts>[src]` (`MOV` alias)
    SimdInsElement {
        rd: u8,
        rn: u8,
        esize: u8,
        dst_index: u8,
        src_index: u8,
    },

    /// `LD1`/`ST1` single structure (one lane) and `LD1R` (load one element into every lane)
    SimdLoadStoreSingle {
        rt: u8,
        rn: u8,
        /// Post-index register; 31 means "advance by the element size".
        rm: u8,
        esize: u8,
        index: u8,
        replicate: bool,
        q: bool,
        is_load: bool,
        post_index: bool,
    },

    /// `UZP1/UZP2/TRN1/TRN2/ZIP1/ZIP2`
    SimdPermute {
        rd: u8,
        rn: u8,
        rm: u8,
        op: SimdPermuteOp,
        esize: u8,
        q: bool,
    },

    /// `FMUL/FMLA/FMLS Vd.<T>, Vn.<T>, Vm.<Ts>[index]`: `double` selects 64-bit lanes, else 32-bit
    SimdFpByElement {
        rd: u8,
        rn: u8,
        rm: u8,
        index: u8,
        op: SimdFpElementOp,
        double: bool,
        q: bool,
    },

    /// `FCCMP/FCCMPE Sn/Dn, Sm/Dm, #nzcv, cond`: compare when `cond` holds, else set the flags to `nzcv`
    FpCondCompare {
        rn: u8,
        rm: u8,
        nzcv: u8,
        cond: Condition,
        is_double: bool,
    },

    /// `ADC/ADCS/SBC/SBCS Rd, Rn, Rm`
    AddSubCarry {
        rd: u8,
        rn: u8,
        rm: u8,
        is_sub: bool,
        set_flags: bool,
        is_64: bool,
    },

    /// `EXT Vd.<T>, Vn.<T>, Vm.<T>, #index`: bytes `index..` of `Vm:Vn`
    SimdExt {
        rd: u8,
        rn: u8,
        rm: u8,
        index: u8,
        q: bool,
    },

    /// Unimplemented or Unknown instruction (preserves raw 32-bit opcode for debugging)
    Unimplemented {
        raw: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftType {
    Lsl = 0,
    Lsr = 1,
    Asr = 2,
    Ror = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Orr,
    Eor,
    Ands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveWideOp {
    Movn = 0b00,
    Movz = 0b10,
    Movk = 0b11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauthOp {
    Sign,
    Auth,
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchRegOp {
    Br,
    Blr,
    Ret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpOp2 {
    Fnmul,
    Fmul,
    Fdiv,
    Fadd,
    Fsub,
    Fmax,
    Fmin,
    /// `FMAXNM`/`FMINNM`: a quiet NaN operand yields the other operand
    Fmaxnm,
    Fminnm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataProc1Op {
    Rbit,
    Rev16,
    /// Reverse bytes in each 32-bit word (`REV` on W registers, `REV32` on X registers).
    Rev32,
    Rev64,
    Clz,
    Cls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdOp3 {
    And,
    Bic,
    Orr,
    Orn,
    Eor,
    Bsl,
    Bit,
    Bif,
    Add,
    Sub,
    Cmeq,
    Cmtst,
    Cmhi,
    Cmhs,
    Cmgt,
    Cmge,
    Umax,
    Umin,
    Smax,
    Smin,
    Addp,
    Umaxp,
    Uminp,
    Smaxp,
    Sminp,
    Mul,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdFpElementOp {
    Fmul,
    Fmla,
    Fmls,
}

/// Widening multiply flavours of AdvSIMD three-different.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLongOp {
    Mull,
    Mlal,
    Mlsl,
    /// `S/UADDL`, `S/UADDW`, `S/USUBL`, `S/USUBW`, `S/UABDL`, `S/UABAL`
    Addl,
    Addw,
    Subl,
    Subw,
    Abdl,
    Abal,
}

/// `ADDHN`, `SUBHN`, `RADDHN`, `RSUBHN`: wide operands, narrow result (the high half of each sum or difference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdNarrowOp {
    Addhn,
    Subhn,
    Raddhn,
    Rsubhn,
}

/// `SQDMULL`, `SQDMLAL`, `SQDMLSL`: saturating doubling multiply long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdSatLongOp {
    Sqdmull,
    Sqdmlal,
    Sqdmlsl,
}

/// AdvSIMD across-lanes reductions (`ADDV`, `S/UADDLV`, `S/UMAXV`, `S/UMINV`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdAcrossOp {
    Addv,
    Saddlv,
    Uaddlv,
    Smaxv,
    Umaxv,
    Sminv,
    Uminv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdOp2 {
    Cmeq0,
    Cmgt0,
    Cmge0,
    Cmle0,
    Cmlt0,
    Cnt,
    Not,
    Abs,
    Neg,
    /// `REV64`/`REV32`/`REV16` (vector): reverse the `esize` elements within each 64/32/16-bit container
    Rev64,
    Rev32,
    Rev16,
    /// `SCVTF`/`UCVTF` (vector, integer): lanes are 32- or 64-bit floats
    Scvtf,
    Ucvtf,
    /// `FCVTZS`/`FCVTZU` (vector or scalar, integer): float lanes to integer, rounding toward zero
    Fcvtzs,
    Fcvtzu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdShiftOp {
    Sshr,
    Ushr,
    Shl,
    Shrn,
    /// `SSHLL`/`USHLL` (`SXTL`/`UXTL` with a zero shift): `esize` is the source lane size
    Sshll,
    Ushll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdImmOp {
    Mov,
    Orr,
    Bic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdPermuteOp {
    Uzp1,
    Uzp2,
    Trn1,
    Trn2,
    Zip1,
    Zip2,
}
