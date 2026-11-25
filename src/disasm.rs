//! Diagnostic AArch64 assembly spelling for instructions decoded by ounce-core.
//!
//! This is deliberately a formatter, not a second decoder: every operand comes from
//! [`Instruction`]. Unsupported variants retain their decoder `Debug` form.

use crate::cpu::instructions::{
    BranchRegOp, Condition, DataProc1Op, FpOp2, Instruction, LogicalOp, MoveWideOp, ShiftType,
};

fn gp(reg: u8, is_64: bool, sp: bool) -> String {
    if reg == 31 {
        if sp {
            "sp".into()
        } else if is_64 {
            "xzr".into()
        } else {
            "wzr".into()
        }
    } else {
        format!("{}{}", if is_64 { 'x' } else { 'w' }, reg)
    }
}

fn fp(reg: u8, bits: u8) -> String {
    format!(
        "{}{}",
        match bits {
            8 => 'b',
            16 => 'h',
            32 => 's',
            64 => 'd',
            _ => 'q',
        },
        reg
    )
}

fn imm(value: i64) -> String {
    if value < 0 {
        format!("#-0x{:x}", value.unsigned_abs())
    } else {
        format!("#0x{value:x}")
    }
}

fn cond(c: Condition) -> &'static str {
    match c {
        Condition::Eq => "eq",
        Condition::Ne => "ne",
        Condition::Cs => "hs",
        Condition::Cc => "lo",
        Condition::Mi => "mi",
        Condition::Pl => "pl",
        Condition::Vs => "vs",
        Condition::Vc => "vc",
        Condition::Hi => "hi",
        Condition::Ls => "ls",
        Condition::Ge => "ge",
        Condition::Lt => "lt",
        Condition::Gt => "gt",
        Condition::Le => "le",
        Condition::Al => "al",
        Condition::Nv => "nv",
    }
}

fn shift(s: ShiftType) -> &'static str {
    match s {
        ShiftType::Lsl => "lsl",
        ShiftType::Lsr => "lsr",
        ShiftType::Asr => "asr",
        ShiftType::Ror => "ror",
    }
}

fn mem(base: u8, offset: i64, writeback: bool, post_index: bool) -> String {
    let base = gp(base, true, true);
    if post_index {
        format!("[{base}], {}", imm(offset))
    } else if offset == 0 {
        format!("[{base}]{}", if writeback { "!" } else { "" })
    } else {
        format!("[{base}, {}]{}", imm(offset), if writeback { "!" } else { "" })
    }
}

fn fp_op(op: FpOp2) -> &'static str {
    match op {
        FpOp2::Fnmul => "fnmul",
        FpOp2::Fmul => "fmul",
        FpOp2::Fdiv => "fdiv",
        FpOp2::Fadd => "fadd",
        FpOp2::Fsub => "fsub",
        FpOp2::Fmax => "fmax",
        FpOp2::Fmin => "fmin",
        FpOp2::Fmaxnm => "fmaxnm",
        FpOp2::Fminnm => "fminnm",
    }
}

/// Formats the decoder result using conventional GNU/LLVM AArch64 syntax.
///
/// The caller prints `raw` separately. An unsupported instruction returns its `Debug`
/// representation, so the complete readout still contains both the word and decoded form.
pub(crate) fn format_instruction(raw: u32, insn: Instruction) -> String {
    use Instruction::*;
    match insn {
        Adr { rd, imm: n, is_page } => format!(
            "{} {}, {}",
            if is_page { "adrp" } else { "adr" },
            gp(rd, true, false),
            imm(n)
        ),
        AddSubImm {
            rd,
            rn,
            imm: n,
            is_sub,
            set_flags,
            is_64,
        } => {
            if set_flags && rd == 31 {
                format!(
                    "{} {}, {}",
                    if is_sub { "cmp" } else { "cmn" },
                    gp(rn, is_64, true),
                    imm(n as i64)
                )
            } else {
                let m = match (is_sub, set_flags) {
                    (false, false) => "add",
                    (false, true) => "adds",
                    (true, false) => "sub",
                    (true, true) => "subs",
                };
                format!(
                    "{m} {}, {}, {}",
                    gp(rd, is_64, !set_flags),
                    gp(rn, is_64, true),
                    imm(n as i64)
                )
            }
        }
        AddSubReg {
            rd,
            rn,
            rm,
            shift_type,
            shift_amount,
            is_extended,
            extend_option,
            is_sub,
            set_flags,
            is_64,
        } => {
            let m = match (is_sub, set_flags) {
                (false, false) => "add",
                (false, true) => "adds",
                (true, false) => "sub",
                (true, true) => "subs",
            };
            let mut tail = if is_extended {
                let ext = ["uxtb", "uxth", "uxtw", "uxtx", "sxtb", "sxth", "sxtw", "sxtx"][extend_option as usize & 7];
                format!("{}, {ext}", gp(rm, extend_option & 3 == 3, false))
            } else {
                gp(rm, is_64, false)
            };
            if shift_amount != 0 {
                if is_extended {
                    tail.push_str(&format!(" #{shift_amount}"));
                } else {
                    tail.push_str(&format!(", {} #{shift_amount}", shift(shift_type)));
                }
            }
            if set_flags && rd == 31 {
                format!("{} {}, {tail}", if is_sub { "cmp" } else { "cmn" }, gp(rn, is_64, true))
            } else {
                format!("{m} {}, {}, {tail}", gp(rd, is_64, !set_flags), gp(rn, is_64, true))
            }
        }
        LogicalReg {
            rd,
            rn,
            rm,
            shift_type,
            shift_amount,
            invert_rm,
            op,
            set_flags,
            is_64,
        } => {
            if op == LogicalOp::Orr && !invert_rm && rn == 31 && shift_amount == 0 {
                return format!("mov {}, {}", gp(rd, is_64, false), gp(rm, is_64, false));
            }
            let m = match (op, invert_rm, set_flags) {
                (LogicalOp::And, false, false) => "and",
                (LogicalOp::And, true, false) => "bic",
                (LogicalOp::Orr, false, false) => "orr",
                (LogicalOp::Orr, true, false) => "orn",
                (LogicalOp::Eor, false, false) => "eor",
                (LogicalOp::Eor, true, false) => "eon",
                (LogicalOp::Ands, false, _) | (LogicalOp::And, false, true) => "ands",
                _ => "bics",
            };
            let mut rhs = gp(rm, is_64, false);
            if shift_amount != 0 {
                rhs.push_str(&format!(", {} #{shift_amount}", shift(shift_type)));
            }
            if (m == "ands" || m == "bics") && rd == 31 {
                format!("tst {}, {rhs}", gp(rn, is_64, false))
            } else {
                format!("{m} {}, {}, {rhs}", gp(rd, is_64, false), gp(rn, is_64, false))
            }
        }
        LogicalImm {
            rd,
            rn,
            imm: n,
            op,
            set_flags,
            is_64,
        } => {
            let m = if set_flags || op == LogicalOp::Ands {
                "ands"
            } else {
                match op {
                    LogicalOp::And => "and",
                    LogicalOp::Orr => "orr",
                    LogicalOp::Eor => "eor",
                    LogicalOp::Ands => "ands",
                }
            };
            if m == "ands" && rd == 31 {
                format!("tst {}, #0x{n:x}", gp(rn, is_64, false))
            } else {
                format!("{m} {}, {}, #0x{n:x}", gp(rd, is_64, false), gp(rn, is_64, false))
            }
        }
        Bitfield {
            rd,
            rn,
            immr,
            imms,
            opc,
            is_64,
        } => {
            let width = if is_64 { 64 } else { 32 };
            if opc == 2 && imms == width - 1 {
                format!("lsr {}, {}, #{immr}", gp(rd, is_64, false), gp(rn, is_64, false))
            } else if opc == 0 && imms == width - 1 {
                format!("asr {}, {}, #{immr}", gp(rd, is_64, false), gp(rn, is_64, false))
            } else if opc == 2 && imms < immr {
                format!(
                    "lsl {}, {}, #{}",
                    gp(rd, is_64, false),
                    gp(rn, is_64, false),
                    width - immr
                )
            } else {
                format!(
                    "{} {}, {}, #0x{immr:x}, #0x{imms:x}",
                    ["sbfm", "bfm", "ubfm"][opc.min(2) as usize],
                    gp(rd, is_64, false),
                    gp(rn, is_64, false)
                )
            }
        }
        Extract { rd, rn, rm, lsb, is_64 } if rn == rm => {
            format!("ror {}, {}, #0x{lsb:x}", gp(rd, is_64, false), gp(rn, is_64, false))
        }
        Extract { rd, rn, rm, lsb, is_64 } => format!(
            "extr {}, {}, {}, #0x{lsb:x}",
            gp(rd, is_64, false),
            gp(rn, is_64, false),
            gp(rm, is_64, false)
        ),
        CondSelect {
            rd,
            rn,
            rm,
            cond: c,
            op,
            is_64,
        } => format!(
            "{} {}, {}, {}, {}",
            ["csel", "csinc", "csinv", "csneg"][op.min(3) as usize],
            gp(rd, is_64, false),
            gp(rn, is_64, false),
            gp(rm, is_64, false),
            cond(c)
        ),
        CondCompare {
            rn,
            rm_or_imm,
            nzcv,
            cond: c,
            is_imm,
            is_sub,
            is_64,
        } => format!(
            "{} {}, {}, #0x{nzcv:x}, {}",
            if is_sub { "ccmp" } else { "ccmn" },
            gp(rn, is_64, false),
            if is_imm {
                imm(rm_or_imm as i64)
            } else {
                gp(rm_or_imm, is_64, false)
            },
            cond(c)
        ),
        TestBranch {
            rt,
            bit,
            offset,
            is_tbnz,
        } => format!(
            "{} {}, #0x{bit:x}, {}",
            if is_tbnz { "tbnz" } else { "tbz" },
            gp(rt, bit >= 32, false),
            imm(offset)
        ),
        MoveWide {
            rd,
            imm: n,
            shift: s,
            op,
            is_64,
        } => {
            let m = match op {
                MoveWideOp::Movn => "movn",
                MoveWideOp::Movz => "movz",
                MoveWideOp::Movk => "movk",
            };
            let tail = if s == 0 { String::new() } else { format!(", lsl #{s}") };
            format!("{m} {}, #0x{n:x}{tail}", gp(rd, is_64, false))
        }
        BranchImm { offset, link } => format!("{} {}", if link { "bl" } else { "b" }, imm(offset)),
        BranchCond { offset, cond: c } => format!("b.{} {}", cond(c), imm(offset)),
        BranchReg { rn, op } => match op {
            BranchRegOp::Ret if rn == 30 => "ret".into(),
            BranchRegOp::Ret => format!("ret {}", gp(rn, true, false)),
            BranchRegOp::Br => format!("br {}", gp(rn, true, false)),
            BranchRegOp::Blr => format!("blr {}", gp(rn, true, false)),
        },
        CompareBranch {
            rt,
            offset,
            is_cbnz,
            is_64,
        } => format!(
            "{} {}, {}",
            if is_cbnz { "cbnz" } else { "cbz" },
            gp(rt, is_64, false),
            imm(offset)
        ),
        LoadStoreImm {
            rt,
            rn,
            offset,
            size_bytes,
            is_load,
            is_signed,
            is_64_target,
            writeback,
            post_index,
        } => {
            let m = match (is_load, is_signed, size_bytes) {
                (false, _, 1) => "strb",
                (false, _, 2) => "strh",
                (false, _, _) => "str",
                (true, false, 1) => "ldrb",
                (true, false, 2) => "ldrh",
                (true, false, _) => "ldr",
                (true, true, 1) => "ldrsb",
                (true, true, 2) => "ldrsh",
                _ => "ldrsw",
            };
            format!(
                "{m} {}, {}",
                gp(rt, is_64_target || size_bytes == 8, false),
                mem(rn, offset, writeback, post_index)
            )
        }
        LoadStorePair {
            rt1,
            rt2,
            rn,
            offset,
            is_load,
            is_64,
            signed_word,
            writeback,
            post_index,
        } => format!(
            "{} {}, {}, {}",
            if signed_word {
                "ldpsw"
            } else if is_load {
                "ldp"
            } else {
                "stp"
            },
            gp(rt1, is_64 || signed_word, false),
            gp(rt2, is_64 || signed_word, false),
            mem(rn, offset, writeback, post_index)
        ),
        Svc { imm: n } => format!("svc #0x{n:x}"),
        Nop => "nop".into(),
        FpData2 { rd, rn, rm, op, esize } => {
            format!("{} {}, {}, {}", fp_op(op), fp(rd, esize), fp(rn, esize), fp(rm, esize))
        }
        FpData1 { rd, rn, op, esize } if op < 4 => format!(
            "{} {}, {}",
            ["fmov", "fabs", "fneg", "fsqrt"][op as usize],
            fp(rd, esize),
            fp(rn, esize)
        ),
        FpData3 {
            rd,
            rn,
            rm,
            ra,
            o1,
            o0,
            esize,
        } => format!(
            "{} {}, {}, {}, {}",
            match (o1, o0) {
                (false, false) => "fmadd",
                (false, true) => "fmsub",
                (true, false) => "fnmadd",
                (true, true) => "fnmsub",
            },
            fp(rd, esize),
            fp(rn, esize),
            fp(rm, esize),
            fp(ra, esize)
        ),
        FpCondSelect {
            rd,
            rn,
            rm,
            cond: c,
            esize,
        } => format!(
            "fcsel {}, {}, {}, {}",
            fp(rd, esize),
            fp(rn, esize),
            fp(rm, esize),
            cond(c)
        ),
        FpCompare {
            rn,
            rm,
            is_zero,
            esize,
            signalling,
        } => format!(
            "{} {}, {}",
            if signalling { "fcmpe" } else { "fcmp" },
            fp(rn, esize),
            if is_zero { "#0.0".into() } else { fp(rm, esize) }
        ),
        FpLoadStoreImm {
            rt,
            rn,
            offset,
            is_load,
            is_double,
        } => format!(
            "{} {}, {}",
            if is_load { "ldr" } else { "str" },
            fp(rt, if is_double { 64 } else { 32 }),
            mem(rn, offset, false, false)
        ),
        MultiplyAddSub {
            rd,
            rn,
            rm,
            ra: 31,
            is_sub,
            is_64,
        } => format!(
            "{} {}, {}, {}",
            if is_sub { "mneg" } else { "mul" },
            gp(rd, is_64, false),
            gp(rn, is_64, false),
            gp(rm, is_64, false)
        ),
        MultiplyAddSub {
            rd,
            rn,
            rm,
            ra,
            is_sub,
            is_64,
        } => format!(
            "{} {}, {}, {}, {}",
            if is_sub { "msub" } else { "madd" },
            gp(rd, is_64, false),
            gp(rn, is_64, false),
            gp(rm, is_64, false),
            gp(ra, is_64, false)
        ),
        Divide {
            rd,
            rn,
            rm,
            is_signed,
            is_64,
        } => format!(
            "{} {}, {}, {}",
            if is_signed { "sdiv" } else { "udiv" },
            gp(rd, is_64, false),
            gp(rn, is_64, false),
            gp(rm, is_64, false)
        ),
        ShiftReg {
            rd,
            rn,
            rm,
            shift_type,
            is_64,
        } => format!(
            "{} {}, {}, {}",
            shift(shift_type),
            gp(rd, is_64, false),
            gp(rn, is_64, false),
            gp(rm, is_64, false)
        ),
        SimdLoadStoreQ {
            rt,
            rn,
            offset,
            is_load,
        } => format!(
            "{} q{}, {}",
            if is_load { "ldr" } else { "str" },
            rt,
            mem(rn, offset, false, false)
        ),
        DataProc1 { rd, rn, op, is_64 } => format!(
            "{} {}, {}",
            match op {
                DataProc1Op::Rbit => "rbit",
                DataProc1Op::Rev16 => "rev16",
                DataProc1Op::Rev32 if is_64 => "rev32",
                DataProc1Op::Rev32 | DataProc1Op::Rev64 => "rev",
                DataProc1Op::Clz => "clz",
                DataProc1Op::Cls => "cls",
            },
            gp(rd, is_64, false),
            gp(rn, is_64, false)
        ),
        MultiplyHigh { rd, rn, rm, is_signed } => format!(
            "{} {}, {}, {}",
            if is_signed { "smulh" } else { "umulh" },
            gp(rd, true, false),
            gp(rn, true, false),
            gp(rm, true, false)
        ),
        Brk { imm: n } => format!("brk #0x{n:x}"),
        Unimplemented { .. } => format!("{insn:?}"),
        _ => {
            let _ = raw;
            format!("{insn:?}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Golden spellings from Apple LLVM objdump 21.0.0 (`objdump -d --no-show-raw-insn`).
    // Branch targets are absent because Instruction records an offset, not the instruction PC.
    #[test]
    fn llvm_objdump_integer_fp_and_memory_golden() {
        let cases = [
            (0xf2d579a2, "movk x2, #0xabcd, lsl #32"),
            (0x910083e3, "add x3, sp, #0x20"),
            (0x71001c9f, "cmp w4, #0x7"),
            (0x310024bf, "cmn w5, #0x9"),
            (0xd10043ff, "sub sp, sp, #0x10"),
            (0x8b080ce6, "add x6, x7, x8, lsl #3"),
            (0xeb8a113f, "cmp x9, x10, asr #4"),
            (0xaa0c03eb, "mov x11, x12"),
            (0x6a4e15bf, "tst w13, w14, lsr #5"),
            (0xcaf11e0f, "eon x15, x16, x17, ror #7"),
            (0x12001e72, "and w18, w19, #0xff"),
            (0xf2089e9f, "tst x20, #0xff00ff00ff00ff00"),
            (0x531d72d5, "lsl w21, w22, #3"),
            (0xd349ff17, "lsr x23, x24, #9"),
            (0x13077f59, "asr w25, w26, #7"),
            (0x93dc379b, "ror x27, x28, #0xd"),
            (0x13822020, "extr w0, w1, w2, #0x8"),
            (0x9a850083, "csel x3, x4, x5, eq"),
            (0x1a8814e6, "csinc w6, w7, w8, ne"),
            (0xfa4aa123, "ccmp x9, x10, #0x3, ge"),
            (0x3a4cb964, "ccmn w11, #0xc, #0x4, lt"),
            (0xd61f0200, "br x16"),
            (0xd63f0220, "blr x17"),
            (0xd65f03c0, "ret"),
            (0xf9400be0, "ldr x0, [sp, #0x10]"),
            (0xb81fcc41, "str w1, [x2, #-0x4]!"),
            (0x38401483, "ldrb w3, [x4], #0x1"),
            (0x798004c5, "ldrsh x5, [x6, #0x2]"),
            (0xa9bf23e7, "stp x7, x8, [sp, #-0x10]!"),
            (0x28c12be9, "ldp w9, w10, [sp], #0x8"),
            (0xd4001001, "svc #0x80"),
            (0xd503201f, "nop"),
            (0x1e622820, "fadd d0, d1, d2"),
            (0x1e214083, "fneg s3, s4"),
            (0x1f4720c5, "fmadd d5, d6, d7, d8"),
            (0x1e2bcd49, "fcsel s9, s10, s11, gt"),
            (0x1e602188, "fcmp d12, #0.0"),
            (0x3dc009cd, "ldr q13, [x14, #0x20]"),
            (0x9b117e0f, "mul x15, x16, x17"),
            (0x1b14d672, "msub w18, w19, w20, w21"),
            (0x9ad80ef6, "sdiv x22, x23, x24"),
            (0x1adb2359, "lsl w25, w26, w27"),
            (0xdac013bc, "clz x28, x29"),
            (0x5ac00820, "rev w0, w1"),
            (0x9bc47c62, "umulh x2, x3, x4"),
            (0xd4202460, "brk #0x123"),
        ];
        for (raw, expected) in cases {
            let decoded = crate::cpu::decoder::decode(raw);
            assert_eq!(format_instruction(raw, decoded), expected, "{raw:08x}: {decoded:?}");
        }
    }

    #[test]
    fn move_wide_keeps_the_decoded_operation_visible() {
        let raw = 0x52800020;
        assert_eq!(
            format_instruction(raw, crate::cpu::decoder::decode(raw)),
            "movz w0, #0x1"
        );
    }
}
