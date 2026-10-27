#![allow(dead_code)]

use super::instructions::{
    BranchRegOp, Condition, DataProc1Op, FpOp2, Instruction, LogicalOp, MoveWideOp, PacKey, PacModifier, PauthOp, ShiftType,
    SimdImmOp, SimdOp2, SimdOp3, SimdPermuteOp, SimdShiftOp,
};

/// Decodes a 32-bit little-endian ARMv8.2-A AArch64 opcode.
pub fn decode(raw: u32) -> Instruction {
    // 1. NOP / HINT / Barrier / Cache Maintenance check:
    //    0xD503201F (NOP), 0xD503245F (BTI c, checked by `Cpu::step` from the raw word), DSB/DMB/ISB (0xD5033xxx), DC/IC (0xD5087xxx / 0xD50B7xxx)
    // DC ZVA, Xt zeroes a cache-line-sized block, so it is not a no-op like the other cache operations.
    if (raw & 0xFFFF_FFE0) == 0xD50B_7420 {
        return Instruction::DcZva { rt: (raw & 0x1F) as u8 };
    }
    if let Some(inst) = decode_pauth(raw) {
        return inst;
    }
    if (raw & 0xFFFF_F01F) == 0xD503_201F
        || (raw & 0xFFFF_F000) == 0xD503_3000
        || (raw & 0xFFF0_0000) == 0xD500_0000
    {
        return Instruction::Nop;
    }

    // 2. Supervisor Call: SVC #imm16
    // Pattern: 1101 0100 000 imm16 00001
    if (raw & 0xFFE0001F) == 0xD4000001 {
        let imm = ((raw >> 5) & 0xFFFF) as u16;
        return Instruction::Svc { imm };
    }

    // 3. PC-Relative: ADR and ADRP
    // Pattern: [31] 00 10000 [23:5] [4:0]
    if (raw & 0x1F000000) == 0x10000000 {
        let is_page = (raw & (1 << 31)) != 0;
        let immlo = ((raw >> 29) & 0x3) as u64;
        let immhi = ((raw >> 5) & 0x7FFFF) as u64;
        let raw_imm = (immhi << 2) | immlo;
        let signed_imm = sign_extend(raw_imm, 21);
        let rd = (raw & 0x1F) as u8;

        let imm = if is_page {
            signed_imm << 12
        } else {
            signed_imm
        };

        return Instruction::Adr { rd, imm, is_page };
    }

    // 4. Move Wide (Immediate): MOVZ, MOVN, MOVK
    // Pattern: sf 10 100101 hw imm16 Rd
    if (raw & 0x1F800000) == 0x12800000 || (raw & 0x1F800000) == 0x52800000 || (raw & 0x1F800000) == 0x72800000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let opc = (raw >> 29) & 0x3;
        let hw = ((raw >> 21) & 0x3) as u8;
        let imm16 = ((raw >> 5) & 0xFFFF) as u16;
        let rd = (raw & 0x1F) as u8;
        let shift = hw * 16;

        let op = match opc {
            0b00 => MoveWideOp::Movn,
            0b10 => MoveWideOp::Movz,
            0b11 => MoveWideOp::Movk,
            _ => return Instruction::Unimplemented { raw },
        };

        return Instruction::MoveWide {
            rd,
            imm: imm16,
            shift,
            op,
            is_64,
        };
    }

    // 5. Add / Subtract (Immediate): ADD, ADDS, SUB, SUBS
    // Pattern: sf op S 100010 shift imm12 Rn Rd
    if (raw & 0x1F000000) == 0x11000000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let is_sub = (raw & (1 << 30)) != 0;
        let set_flags = (raw & (1 << 29)) != 0;
        let shift = (raw >> 22) & 0x3;
        let imm12 = ((raw >> 10) & 0xFFF) as u64;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        let imm = match shift {
            0 => imm12,
            1 => imm12 << 12,
            _ => return Instruction::Unimplemented { raw },
        };

        return Instruction::AddSubImm {
            rd,
            rn,
            imm,
            is_sub,
            set_flags,
            is_64,
        };
    }

    // 6. Add / Subtract (Shifted or Extended Register)
    // Pattern: sf op S 01011 shift/opt is_ext Rm imm6/opt+imm3 Rn Rd
    if (raw & 0x1F000000) == 0x0B000000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let is_sub = (raw & (1 << 30)) != 0;
        let set_flags = (raw & (1 << 29)) != 0;
        let is_extended = (raw & (1 << 21)) != 0;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        let (shift_type, shift_amount, extend_option) = if is_extended {
            let opt = ((raw >> 13) & 0x7) as u8;
            let imm3 = ((raw >> 10) & 0x7) as u8;
            (ShiftType::Lsl, imm3, opt)
        } else {
            let sh_type = match (raw >> 22) & 0x3 {
                0 => ShiftType::Lsl,
                1 => ShiftType::Lsr,
                2 => ShiftType::Asr,
                _ => ShiftType::Ror,
            };
            let imm6 = ((raw >> 10) & 0x3F) as u8;
            (sh_type, imm6, 0)
        };

        return Instruction::AddSubReg {
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
        };
    }

    // 7. Logical (Shifted Register): AND, BIC, ORR, ORN, EOR, EON, ANDS, BICS
    // Pattern: sf opc 01010 shift N Rm imm6 Rn Rd
    if (raw & 0x1F000000) == 0x0A000000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let opc = (raw >> 29) & 0x3;
        let shift_bits = (raw >> 22) & 0x3;
        let invert_rm = (raw & (1 << 21)) != 0;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let shift_amount = ((raw >> 10) & 0x3F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        let shift_type = match shift_bits {
            0b00 => ShiftType::Lsl,
            0b01 => ShiftType::Lsr,
            0b10 => ShiftType::Asr,
            _ => ShiftType::Ror,
        };

        let (op, set_flags) = match opc {
            0b00 => (LogicalOp::And, false),
            0b01 => (LogicalOp::Orr, false),
            0b10 => (LogicalOp::Eor, false),
            0b11 => (LogicalOp::Ands, true),
            _ => unreachable!(),
        };

        return Instruction::LogicalReg {
            rd,
            rn,
            rm,
            shift_type,
            shift_amount,
            invert_rm,
            op,
            set_flags,
            is_64,
        };
    }

    // 7b. Logical (Immediate): AND, ORR, EOR, ANDS Xd, Xn, #bitmask_imm
    // Pattern: sf opc 100100 N immr imms Rn Rd
    if (raw & 0x1F800000) == 0x12000000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let opc = (raw >> 29) & 0x3;
        let n = ((raw >> 22) & 1) as u8;
        let immr = ((raw >> 16) & 0x3F) as u8;
        let imms = ((raw >> 10) & 0x3F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        if let Some(imm) = decode_bit_mask(is_64, n, imms, immr) {
            let (op, set_flags) = match opc {
                0b00 => (LogicalOp::And, false),
                0b01 => (LogicalOp::Orr, false),
                0b10 => (LogicalOp::Eor, false),
                0b11 => (LogicalOp::Ands, true),
                _ => unreachable!(),
            };
            return Instruction::LogicalImm {
                rd,
                rn,
                imm,
                op,
                set_flags,
                is_64,
            };
        }
    }

    // 7c. Bitfield: SBFM (00), BFM (01), UBFM (10)
    // Pattern: sf opc 100110 N immr imms Rn Rd
    if (raw & 0x1F800000) == 0x13000000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let opc = ((raw >> 29) & 0x3) as u8;
        let immr = ((raw >> 16) & 0x3F) as u8;
        let imms = ((raw >> 10) & 0x3F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        if opc <= 2 {
            return Instruction::Bitfield {
                rd,
                rn,
                immr,
                imms,
                opc,
                is_64,
            };
        }
    }

    // 7d. Extract: EXTR Xd, Xn, Xm, #lsb
    // Pattern: sf 00 100111 N 0 Rm imms Rn Rd
    if (raw & 0x1FA00000) == 0x13800000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let lsb = ((raw >> 10) & 0x3F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        return Instruction::Extract {
            rd,
            rn,
            rm,
            lsb,
            is_64,
        };
    }

    // 7e. Conditional Select: CSEL, CSINC, CSINV, CSNEG
    // Pattern: sf op 0 11010100 Rm cond 0 o2 Rn Rd
    if (raw & 0x1FE00800) == 0x1A800000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let op_hi = ((raw >> 30) & 1) as u8;
        let op_lo = ((raw >> 10) & 1) as u8;
        let op = (op_hi << 1) | op_lo;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let cond = Condition::from_u8(((raw >> 12) & 0xF) as u8);
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        return Instruction::CondSelect {
            rd,
            rn,
            rm,
            cond,
            op,
            is_64,
        };
    }

    // 7f. Conditional Compare: CCMP / CCMN (Register or Immediate)
    // Pattern: sf op 1 11010010 Rm_or_imm5 cond is_imm 0 Rn 0 nzcv
    if (raw & 0x1FE00410) == 0x1A400000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let is_sub = (raw & (1 << 30)) != 0;
        let rm_or_imm = ((raw >> 16) & 0x1F) as u8;
        let cond = Condition::from_u8(((raw >> 12) & 0xF) as u8);
        let is_imm = (raw & (1 << 11)) != 0;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let nzcv = (raw & 0xF) as u8;
        return Instruction::CondCompare {
            rn,
            rm_or_imm,
            nzcv,
            cond,
            is_imm,
            is_sub,
            is_64,
        };
    }

    // 10b. Test and Branch (TBZ / TBNZ)
    // Pattern: b5 011011 op b40 imm14 Rt
    if (raw & 0x7E000000) == 0x36000000 {
        let b5 = ((raw >> 31) & 1) as u8;
        let b40 = ((raw >> 19) & 0x1F) as u8;
        let bit = (b5 << 5) | b40;
        let is_tbnz = (raw & (1 << 24)) != 0;
        let imm14 = ((raw >> 5) & 0x3FFF) as u64;
        let offset = sign_extend(imm14, 14) * 4;
        let rt = (raw & 0x1F) as u8;
        return Instruction::TestBranch {
            rt,
            bit,
            offset,
            is_tbnz,
        };
    }

    // 8. Unconditional Branch (Immediate): B / BL
    // Pattern: op 00101 imm26
    if (raw & 0x7C000000) == 0x14000000 {
        let link = (raw & (1 << 31)) != 0;
        let imm26 = (raw & 0x03FFFFFF) as u64;
        let signed_offset = sign_extend(imm26, 26) * 4;
        return Instruction::BranchImm {
            offset: signed_offset,
            link,
        };
    }

    // 9. Conditional Branch (B.cond)
    // Pattern: 0101010 0 imm19 0 cond
    if (raw & 0xFF000010) == 0x54000000 {
        let imm19 = ((raw >> 5) & 0x7FFFF) as u64;
        let signed_offset = sign_extend(imm19, 19) * 4;
        let cond_val = (raw & 0xF) as u8;
        return Instruction::BranchCond {
            offset: signed_offset,
            cond: Condition::from_u8(cond_val),
        };
    }

    // 10. Compare and Branch (CBZ / CBNZ)
    // Pattern: sf 011010 op imm19 Rt
    if (raw & 0x7E000000) == 0x34000000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let is_cbnz = (raw & (1 << 24)) != 0;
        let imm19 = ((raw >> 5) & 0x7FFFF) as u64;
        let offset = sign_extend(imm19, 19) * 4;
        let rt = (raw & 0x1F) as u8;
        return Instruction::CompareBranch {
            rt,
            offset,
            is_cbnz,
            is_64,
        };
    }

    // 11. Branch to Register: BR, BLR, RET
    // Pattern: 1101011 0000 11111 000000 Rn 00000
    if (raw & 0xFE1FFC1F) == 0xD61F0000 {
        let opc = (raw >> 21) & 0x3;
        let rn = ((raw >> 5) & 0x1F) as u8;

        let op = match opc {
            0 => BranchRegOp::Br,
            1 => BranchRegOp::Blr,
            2 => BranchRegOp::Ret,
            _ => return Instruction::Unimplemented { raw },
        };

        return Instruction::BranchReg { rn, op };
    }

    // 12. Load / Store Pair (LDP / STP)
    // Pattern: opc 101 V 0 mode L imm7 Rt2 Rn Rt1
    if (raw & 0x3A000000) == 0x28000000 {
        let opc = (raw >> 30) & 0x3;
        let is_load = (raw & (1 << 22)) != 0;
        let is_64 = opc == 2;
        let scale = if is_64 { 8 } else { 4 };
        let imm7 = ((raw >> 15) & 0x7F) as u64;
        let offset = sign_extend(imm7, 7) * scale;
        let rt2 = ((raw >> 10) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rt1 = (raw & 0x1F) as u8;

        let mode = (raw >> 23) & 0x3;
        let (writeback, post_index) = match mode {
            0b00 => (false, false), // Non-temporal (LDNP/STNP): same access as signed offset
            0b01 => (true, true),  // Post-index
            0b10 => (false, false), // Signed offset
            _ => (true, false),    // Pre-index
        };

        // V == 1: SIMD&FP register pair (S, D or Q), not general registers.
        if (raw & (1 << 26)) != 0 {
            if opc == 3 {
                return Instruction::Unimplemented { raw };
            }
            let size_bytes = 4u8 << opc;
            return Instruction::SimdLoadStorePair {
                rt1,
                rt2,
                rn,
                offset: sign_extend(imm7, 7) * size_bytes as i64,
                size_bytes,
                is_load,
                writeback,
                post_index,
            };
        }

        return Instruction::LoadStorePair {
            rt1,
            rt2,
            rn,
            offset,
            is_load,
            is_64,
            writeback,
            post_index,
        };
    }

    // 13. Load / Store Single Register (Immediate unsigned offset, V == 0)
    if (raw & 0x3F000000) == 0x39000000 {
        let size = ((raw >> 30) & 0x3) as u8;
        let opc = ((raw >> 22) & 0x3) as u8;
        let is_load = opc != 0;
        let is_signed = opc == 2 || opc == 3;
        let imm12 = ((raw >> 10) & 0xFFF) as u64;
        let scale = 1 << size;
        let offset = (imm12 * scale) as i64;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rt = (raw & 0x1F) as u8;

        return Instruction::LoadStoreImm {
            rt,
            rn,
            offset,
            size_bytes: scale as u8,
            is_load,
            is_signed,
            writeback: false,
            post_index: false,
        };
    }

    // 13b. Load / Store Single Register (Unscaled Immediate LDUR/STUR, Post-Index, Pre-Index, & Register Offset, V == 0)
    if (raw & 0x3F000000) == 0x38000000 {
        let size = ((raw >> 30) & 0x3) as u8;
        let opc = ((raw >> 22) & 0x3) as u8;
        let is_load = opc != 0;
        let is_signed = opc == 2 || opc == 3;
        let scale = 1u8 << size;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rt = (raw & 0x1F) as u8;

        if (raw & (1 << 21)) != 0 && ((raw >> 10) & 0x3) == 0b10 {
            // Register offset: LDR/STR Rt, [Xn, Xm, option #shift]
            let rm = ((raw >> 16) & 0x1F) as u8;
            let option = ((raw >> 13) & 0x7) as u8;
            let s = ((raw >> 12) & 1) as u8;
            let shift = if s != 0 { size } else { 0 };
            let is_64_target = size == 3 || opc == 2;
            return Instruction::LoadStoreReg {
                rt,
                rn,
                rm,
                option,
                shift,
                size_bytes: scale,
                is_load,
                is_signed,
                is_64_target,
            };
        } else {
            let mode = (raw >> 10) & 0x3;
            let imm9 = ((raw >> 12) & 0x1FF) as u64;
            let offset = sign_extend(imm9, 9);
            let (writeback, post_index) = match mode {
                0b00 => (false, false), // Unscaled LDUR / STUR
                0b01 => (true, true),   // Post-index [Xn], #imm9
                0b11 => (true, false),  // Pre-index [Xn, #imm9]!
                _ => return Instruction::Unimplemented { raw },
            };
            return Instruction::LoadStoreImm {
                rt,
                rn,
                offset,
                size_bytes: scale,
                is_load,
                is_signed,
                writeback,
                post_index,
            };
        }
    }

    // 13c. Load/Store Exclusive & Load-Acquire/Store-Release (LDXR, LDAXR, LDAR, STXR, STLXR, STLR)
    // Pattern: size 001000 o2 L o1 Rs o0 Rt2 Rn Rt
    if (raw & 0x3F000000) == 0x08000000 {
        let size = ((raw >> 30) & 0x3) as u8;
        let is_exclusive = (raw & (1 << 23)) == 0;
        let is_load = (raw & (1 << 22)) != 0;
        let rs = ((raw >> 16) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rt = (raw & 0x1F) as u8;
        return Instruction::LoadStoreExclusive {
            rt,
            rn,
            rs,
            size_bytes: 1u8 << size,
            is_load,
            is_exclusive,
        };
    }

    // 14. Floating-Point 2-Source Arithmetic: FADD, FSUB, FMUL, FDIV, FMAX, FMIN
    // Pattern: 0 0 0 11110 ftype 1 Rm opcode 00 Rn Rd
    if (raw & 0x5F200C00) == 0x1E200800 {
        let ftype = (raw >> 22) & 0x3;
        if ftype == 0 || ftype == 1 {
            let is_double = ftype == 1;
            let rm = ((raw >> 16) & 0x1F) as u8;
            let opcode = (raw >> 12) & 0xF;
            let rn = ((raw >> 5) & 0x1F) as u8;
            let rd = (raw & 0x1F) as u8;

            let op = match opcode {
                0b0000 => FpOp2::Fmul,
                0b0001 => FpOp2::Fdiv,
                0b0010 => FpOp2::Fadd,
                0b0011 => FpOp2::Fsub,
                0b0100 => FpOp2::Fmax,
                0b0101 => FpOp2::Fmin,
                _ => return Instruction::Unimplemented { raw },
            };

            return Instruction::FpData2 {
                rd,
                rn,
                rm,
                op,
                is_double,
            };
        }
    }

    // 15. Floating-Point Compare: FCMP
    if (raw & 0x5F203C1F) == 0x1E202000 {
        let ftype = (raw >> 22) & 0x3;
        let is_double = ftype == 1;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        return Instruction::FpCompare {
            rn,
            rm,
            is_zero: false,
            is_double,
        };
    }

    // 16. System Register Read/Write: MRS Xt, <sysreg> / MSR <sysreg>, Xt
    // MRS: 1101 0101 0011 op1 CRn CRm op2 Rt (0xD5300000)
    // MSR: 1101 0101 0001 op1 CRn CRm op2 Rt (0xD5100000)
    if (raw & 0xFFF00000) == 0xD5300000 || (raw & 0xFFF00000) == 0xD5100000 {
        let is_read = (raw & (1 << 21)) != 0;
        let sysreg = ((raw >> 5) & 0x7FFF) as u16;
        let rt = (raw & 0x1F) as u8;
        return Instruction::SystemReg {
            rt,
            sysreg,
            is_read,
        };
    }

    // 17. Integer Multiply-Add / Multiply-Subtract: MADD, MSUB, MUL
    // Pattern: sf 00 11011 000 Rm o0 Ra Rn Rd
    if (raw & 0x7FE00000) == 0x1B000000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let is_sub = (raw & (1 << 15)) != 0;
        let ra = ((raw >> 10) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        return Instruction::MultiplyAddSub {
            rd,
            rn,
            rm,
            ra,
            is_sub,
            is_64,
        };
    }

    // 18. Data-Processing (2-Source): UDIV, SDIV, LSLV, LSRV, ASRV, RORV
    // Pattern: sf 00 11010110 Rm opcode Rn Rd
    if (raw & 0x7FE00000) == 0x1AC00000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let opcode = (raw >> 10) & 0x3F;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;

        match opcode {
            0b000010 | 0b000011 => {
                return Instruction::Divide {
                    rd,
                    rn,
                    rm,
                    is_signed: opcode == 0b000011,
                    is_64,
                };
            }
            0b001000..=0b001011 => {
                let shift_type = match opcode & 0x3 {
                    0 => ShiftType::Lsl,
                    1 => ShiftType::Lsr,
                    2 => ShiftType::Asr,
                    _ => ShiftType::Ror,
                };
                return Instruction::ShiftReg {
                    rd,
                    rn,
                    rm,
                    shift_type,
                    is_64,
                };
            }
            _ => {}
        }
    }

    // 19. Floating-Point / General Register Transfer: FMOV Dd, Xn / FMOV Xd, Dn / FMOV Sd, Wn / FMOV Wd, Sn
    if (raw & 0x7FBEFC00) == 0x1E260000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let to_fp = ((raw >> 16) & 1) != 0;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        return Instruction::FpMovGen {
            rd,
            rn,
            to_fp,
            is_64,
        };
    }

    // 20. Floating-Point Scalar & 128-bit NEON Q Load / Store (Immediate unsigned offset):
    //     LDR/STR St/Dt/Qt, [Xn, #offset]
    if (raw & 0x3F000000) == 0x3D000000 {
        let size = ((raw >> 30) & 0x3) as u8;
        let opc = ((raw >> 22) & 0x3) as u8;
        let imm12 = ((raw >> 10) & 0xFFF) as u64;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rt = (raw & 0x1F) as u8;

        // 128-bit Q register load/store: size == 0b00 && (opc == 0b10 [STR Q] || opc == 0b11 [LDR Q])
        if size == 0 && (opc == 0b10 || opc == 0b11) {
            let is_load = opc == 0b11;
            let offset = (imm12 * 16) as i64;
            return Instruction::SimdLoadStoreQ {
                rt,
                rn,
                offset,
                is_load,
            };
        }

        // B / H registers
        if size < 2 && opc < 2 {
            return Instruction::SimdLoadStore {
                rt,
                rn,
                offset: (imm12 << size) as i64,
                size_bytes: 1 << size,
                is_load: opc == 0b01,
                writeback: false,
                post_index: false,
            };
        }

        if (size == 2 || size == 3) && (opc == 0b00 || opc == 0b01) {
            let is_load = opc == 0b01;
            let is_double = size == 3;
            let scale = 1u64 << size;
            let offset = (imm12 * scale) as i64;
            return Instruction::FpLoadStoreImm {
                rt,
                rn,
                offset,
                is_load,
                is_double,
            };
        }
    }

    // 21. 128-bit NEON Vector 4xFloat32 SIMD:
    //     FADD Vd.4S, Vn.4S, Vm.4S (0x4E20D400)
    //     FSUB Vd.4S, Vn.4S, Vm.4S (0x4EA0D400)
    //     FMUL Vd.4S, Vn.4S, Vm.4S (0x6E20DC00)
    if (raw & 0xFF20FC00) == 0x4E20D400 || (raw & 0xFFE0FC00) == 0x6E20DC00 {
        let rm = ((raw >> 16) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        let op = if (raw & 0xFFE0FC00) == 0x6E20DC00 {
            FpOp2::Fmul
        } else if (raw & (1 << 23)) != 0 {
            FpOp2::Fsub
        } else {
            FpOp2::Fadd
        };
        return Instruction::SimdVec4S { rd, rn, rm, op };
    }

    decode_extra(raw).unwrap_or(Instruction::Unimplemented { raw })
}

/// FEAT_PAuth: the HINT-space forms, the data-processing (1-source) forms, `PACGA`, the
/// authenticated branches and `LDRAA`/`LDRAB`. Unallocated encodings in these groups return `None`.
fn decode_pauth(raw: u32) -> Option<Instruction> {
    use PacKey::*;
    use PacModifier::{Reg, Zero};
    use PauthOp::*;
    let rd = (raw & 0x1F) as u8;
    let rn = ((raw >> 5) & 0x1F) as u8;
    let pauth = |rd, op, key, modifier| Some(Instruction::Pauth { rd, op, key, modifier });

    // HINT #imm: 1101 0101 0000 0011 0010 CRm op2 11111
    if (raw & 0xFFFF_F01F) == 0xD503_201F {
        return match (raw >> 5) & 0x7F {
            7 => pauth(30, Strip, IA, Zero), // XPACLRI
            8 => pauth(17, Sign, IA, Reg(16)), // PACIA1716
            10 => pauth(17, Sign, IB, Reg(16)),
            12 => pauth(17, Auth, IA, Reg(16)), // AUTIA1716
            14 => pauth(17, Auth, IB, Reg(16)),
            24 => pauth(30, Sign, IA, Zero), // PACIAZ
            25 => pauth(30, Sign, IA, Reg(31)), // PACIASP
            26 => pauth(30, Sign, IB, Zero),
            27 => pauth(30, Sign, IB, Reg(31)),
            28 => pauth(30, Auth, IA, Zero), // AUTIAZ
            29 => pauth(30, Auth, IA, Reg(31)), // AUTIASP
            30 => pauth(30, Auth, IB, Zero),
            31 => pauth(30, Auth, IB, Reg(31)),
            _ => None,
        };
    }

    // Data-processing (1 source), opcode2 = 00001: 1 1 0 11010110 00001 opcode Rn Rd
    if (raw & 0xFFFF_8000) == 0xDAC1_0000 {
        let opcode = (raw >> 10) & 0x3F;
        let key = [IA, IB, DA, DB][(opcode & 3) as usize];
        return match opcode {
            0..=7 => pauth(rd, if opcode < 4 { Sign } else { Auth }, key, Reg(rn)),
            8..=15 if rn == 31 => pauth(rd, if opcode < 12 { Sign } else { Auth }, key, Zero),
            16 | 17 if rn == 31 => pauth(rd, Strip, if opcode == 16 { IA } else { DA }, Zero),
            _ => None,
        };
    }

    // PACGA: 1 0 0 11010110 Rm 001100 Rn Rd
    if (raw & 0xFFE0_FC00) == 0x9AC0_3000 {
        return Some(Instruction::Pacga { rd, rn, rm: ((raw >> 16) & 0x1F) as u8 });
    }

    // Branch (register) with pointer authentication: 1101011 Z 0 op(2) 11111 00001 M Rn Rm
    if (raw & 0xFE9F_F800) == 0xD61F_0800 {
        let z = (raw >> 24) & 1;
        let opc = (raw >> 21) & 3;
        let key = if (raw >> 10) & 1 == 0 { IA } else { IB };
        let rm = rd;
        let op = match opc {
            0 => BranchRegOp::Br,
            1 => BranchRegOp::Blr,
            2 => BranchRegOp::Ret,
            _ => return None,
        };
        let modifier = match (z, op) {
            (0, BranchRegOp::Ret) if rn == 31 && rm == 31 => return Some(Instruction::BranchRegAuth { rn: 30, op, key, modifier: Reg(31) }),
            (0, BranchRegOp::Br | BranchRegOp::Blr) if rm == 31 => Zero,
            (1, BranchRegOp::Br | BranchRegOp::Blr) => Reg(rm),
            _ => return None,
        };
        return Some(Instruction::BranchRegAuth { rn, op, key, modifier });
    }

    // LDRAA/LDRAB: 11 111 0 00 M S 1 imm9 W 1 Rn Rt
    if (raw & 0xFF20_0400) == 0xF820_0400 {
        let simm = (((raw >> 22) & 1) << 9 | (raw >> 12) & 0x1FF) as u64;
        return Some(Instruction::LoadAuth {
            rt: rd,
            rn,
            offset: sign_extend(simm, 10) << 3,
            key: if (raw >> 23) & 1 == 0 { DA } else { DB },
            writeback: (raw >> 11) & 1 != 0,
        });
    }
    None
}

/// Scalar and AdvSIMD encodings added for libnx-built homebrew (`memset`, `strlen`, `strcmp`,
/// `__nx_dynamic`). Field layouts follow the Arm A64 encoding tables.
fn decode_extra(raw: u32) -> Option<Instruction> {
    let rd = (raw & 0x1F) as u8;
    let rn = ((raw >> 5) & 0x1F) as u8;
    let rm = ((raw >> 16) & 0x1F) as u8;
    let q = (raw & (1 << 30)) != 0;
    let u = (raw & (1 << 29)) != 0;
    let size = (raw >> 22) & 0x3;

    // 21b. FMOV (scalar, immediate): 0 0 0 11110 ftype 1 imm8 100 00000 Rd. Writes the low 32/64 bits of
    //      Vd and zeroes the rest, which is SimdModImm without replication.
    if (raw & 0xFF20_1FE0) == 0x1E20_1000 {
        let imm8 = ((raw >> 13) & 0xFF) as u64;
        let (a, b, cd, efgh) = (imm8 >> 7, (imm8 >> 6) & 1, (imm8 >> 4) & 3, imm8 & 0xF);
        let imm = match (raw >> 22) & 0x3 {
            0 => a << 31 | (b ^ 1) << 30 | (if b != 0 { 0x1F } else { 0 }) << 25 | cd << 23 | efgh << 19,
            1 => a << 63 | (b ^ 1) << 62 | (if b != 0 { 0xFF } else { 0 }) << 54 | cd << 52 | efgh << 48,
            _ => return None, // half precision is not modelled
        };
        return Some(Instruction::SimdModImm { rd, imm, op: SimdImmOp::Mov, q: false });
    }

    // 22. Data-Processing (1-Source): sf 1 0 11010110 00000 opcode Rn Rd
    if (raw & 0x7FFF_E000) == 0x5AC0_0000 {
        let is_64 = (raw & (1 << 31)) != 0;
        let op = match ((raw >> 10) & 0x7, is_64) {
            (0, _) => DataProc1Op::Rbit,
            (1, _) => DataProc1Op::Rev16,
            (2, _) => DataProc1Op::Rev32,
            (3, true) => DataProc1Op::Rev64,
            (4, _) => DataProc1Op::Clz,
            (5, _) => DataProc1Op::Cls,
            _ => return None,
        };
        return Some(Instruction::DataProc1 { rd, rn, op, is_64 });
    }

    // 23. Multiply High: 1 00 11011 U 10 Rm 0 11111 Rn Rd
    if (raw & 0xFF60_FC00) == 0x9B40_7C00 {
        let is_signed = (raw & (1 << 23)) == 0;
        return Some(Instruction::MultiplyHigh { rd, rn, rm, is_signed });
    }

    // 24. Multiply Long: 1 00 11011 U 01 Rm o0 Ra Rn Rd
    if (raw & 0xFF60_0000) == 0x9B20_0000 {
        let is_signed = (raw & (1 << 23)) == 0;
        let is_sub = (raw & (1 << 15)) != 0;
        let ra = ((raw >> 10) & 0x1F) as u8;
        return Some(Instruction::MultiplyLong { rd, rn, rm, ra, is_signed, is_sub });
    }

    // 25. SIMD&FP Load/Store (unscaled / post-index / pre-index): size 111 1 00 opc 0 imm9 idx Rn Rt
    if (raw & 0x3F20_0000) == 0x3C00_0000 {
        let opc = (raw >> 22) & 0x3;
        let idx = (raw >> 10) & 0x3;
        let sz = (raw >> 30) & 0x3;
        let size_bytes = match (sz, opc & 0b10) {
            (0, 0b10) => 16,
            (s, 0) => 1u8 << s,
            _ => return None,
        };
        if idx == 0b10 {
            return None;
        }
        let offset = sign_extend(((raw >> 12) & 0x1FF) as u64, 9);
        return Some(Instruction::SimdLoadStore {
            rt: rd,
            rn,
            offset,
            size_bytes,
            is_load: (opc & 1) != 0,
            writeback: idx != 0,
            post_index: idx == 0b01,
        });
    }

    // 25b. SIMD&FP Load/Store (register offset): size 111 1 00 opc 1 Rm option S 10 Rn Rt
    if (raw & 0x3F20_0C00) == 0x3C20_0800 {
        let opc = (raw >> 22) & 0x3;
        let sz = (raw >> 30) & 0x3;
        let (size_bytes, log2) = match (sz, opc & 0b10) {
            (0, 0b10) => (16u8, 4u8),
            (s, 0) => (1u8 << s, s as u8),
            _ => return None,
        };
        let option = ((raw >> 13) & 0x7) as u8;
        if option & 0b010 == 0 {
            return None;
        }
        let shift = if (raw & (1 << 12)) != 0 { log2 } else { 0 };
        return Some(Instruction::SimdLoadStoreReg {
            rt: rd,
            rn,
            rm,
            option,
            shift,
            size_bytes,
            is_load: (opc & 1) != 0,
        });
    }

    // 26. LD1/ST1 (multiple structures): 0 Q 0011000 L 000000 opcode size Rn Rt
    //     post-index:                    0 Q 0011001 L 0 Rm    opcode size Rn Rt
    let no_offset = (raw & 0xBFBF_0000) == 0x0C00_0000;
    let post = (raw & 0xBFA0_0000) == 0x0C80_0000;
    if no_offset || post {
        let regs = match (raw >> 12) & 0xF {
            0b0111 => 1,
            0b1010 => 2,
            0b0110 => 3,
            0b0010 => 4,
            _ => return None, // LD2/LD3/LD4 (interleaved) are not implemented
        };
        return Some(Instruction::SimdLoadStoreMulti {
            rt: rd,
            rn,
            rm: if post { rm } else { 31 },
            regs,
            q,
            is_load: (raw & (1 << 22)) != 0,
            post_index: post,
        });
    }

    // 27. AdvSIMD copy: 0 Q op 01110000 imm5 0 imm4 1 Rn Rd
    //     DUP (element) imm4 0000, DUP (general) 0001, INS (general) 0011, UMOV 0111; op = 1: INS (element)
    if (raw & 0x9FE0_8400) == 0x0E00_0400 {
        let imm5 = (raw >> 16) & 0x1F;
        if imm5 & 0xF == 0 {
            return None;
        }
        let low = imm5.trailing_zeros();
        let esize = 8u8 << low;
        let index = (imm5 >> (low + 1)) as u8;
        if u {
            if !q {
                return None;
            }
            let src_index = (((raw >> 11) & 0xF) >> low) as u8;
            return Some(Instruction::SimdInsElement { rd, rn, esize, dst_index: index, src_index });
        }
        match (raw >> 11) & 0xF {
            0b0000 if !(esize == 64 && !q) => {
                return Some(Instruction::SimdDupElement { rd, rn, esize, index, q });
            }
            0b0011 if q => {
                return Some(Instruction::SimdInsGeneral { rd, rn, esize, index });
            }
            0b0001 if !(esize == 64 && !q) => {
                return Some(Instruction::SimdDupGeneral { rd, rn, esize, q });
            }
            0b0111 if (esize == 64) == q => {
                let index = (imm5 >> (low + 1)) as u8;
                return Some(Instruction::SimdMovToGeneral { rd, rn, esize, index });
            }
            _ => return None,
        }
    }

    // 26b. LD1/ST1 (single structure) and LD1R: 0 Q 0011010 L 0 00000 opcode S size Rn Rt,
    //      post-index 0 Q 0011011 L 0 Rm opcode S size Rn Rt
    let single = (raw & 0xBFBF_0000) == 0x0D00_0000;
    let single_post = (raw & 0xBFA0_0000) == 0x0D80_0000;
    if single || single_post {
        let is_load = (raw & (1 << 22)) != 0;
        let opcode = (raw >> 13) & 0x7;
        let s_bit = (raw >> 12) & 1;
        let sz = (raw >> 10) & 0x3;
        let qb = (raw >> 30) & 1;
        let (esize, index, replicate) = match opcode {
            0b000 => (8u8, (qb << 3 | s_bit << 2 | sz) as u8, false),
            0b010 if sz & 1 == 0 => (16, (qb << 2 | s_bit << 1 | sz >> 1) as u8, false),
            0b100 if sz == 0 => (32, (qb << 1 | s_bit) as u8, false),
            0b100 if sz == 1 && s_bit == 0 => (64, qb as u8, false),
            0b110 if is_load && s_bit == 0 => (8u8 << sz, 0, true),
            _ => return None, // LD2/LD3/LD4 single structure not implemented
        };
        return Some(Instruction::SimdLoadStoreSingle {
            rt: rd,
            rn,
            rm: if single_post { rm } else { 31 },
            esize,
            index,
            replicate,
            q,
            is_load,
            post_index: single_post,
        });
    }

    // 26c. AdvSIMD permute: 0 Q 001110 size 0 Rm 0 opc 10 Rn Rd (UZP1/TRN1/ZIP1/UZP2/TRN2/ZIP2)
    if (raw & 0xBF20_8C00) == 0x0E00_0800 {
        let op = match (raw >> 12) & 0x7 {
            0b001 => SimdPermuteOp::Uzp1,
            0b010 => SimdPermuteOp::Trn1,
            0b011 => SimdPermuteOp::Zip1,
            0b101 => SimdPermuteOp::Uzp2,
            0b110 => SimdPermuteOp::Trn2,
            0b111 => SimdPermuteOp::Zip2,
            _ => return None,
        };
        if size == 3 && !q {
            return None;
        }
        return Some(Instruction::SimdPermute { rd, rn, rm, op, esize: 8u8 << size, q });
    }

    // 28. AdvSIMD three same: 0 Q U 01110 size 1 Rm opcode 1 Rn Rd
    if (raw & 0x9F20_0400) == 0x0E20_0400 {
        let opcode = (raw >> 11) & 0x1F;
        let op = match (opcode, u) {
            (0b00011, false) => [SimdOp3::And, SimdOp3::Bic, SimdOp3::Orr, SimdOp3::Orn][size as usize],
            (0b00011, true) => [SimdOp3::Eor, SimdOp3::Bsl, SimdOp3::Bit, SimdOp3::Bif][size as usize],
            (0b10000, false) => SimdOp3::Add,
            (0b10000, true) => SimdOp3::Sub,
            (0b10001, false) => SimdOp3::Cmtst,
            (0b10001, true) => SimdOp3::Cmeq,
            (0b00110, false) => SimdOp3::Cmgt,
            (0b00110, true) => SimdOp3::Cmhi,
            (0b00111, false) => SimdOp3::Cmge,
            (0b00111, true) => SimdOp3::Cmhs,
            (0b01100, false) => SimdOp3::Smax,
            (0b01100, true) => SimdOp3::Umax,
            (0b01101, false) => SimdOp3::Smin,
            (0b01101, true) => SimdOp3::Umin,
            (0b10100, false) => SimdOp3::Smaxp,
            (0b10100, true) => SimdOp3::Umaxp,
            (0b10101, false) => SimdOp3::Sminp,
            (0b10101, true) => SimdOp3::Uminp,
            (0b10111, false) => SimdOp3::Addp,
            _ => return None,
        };
        let bitwise = opcode == 0b00011;
        // 64-bit lanes need Q, and min/max have no 64-bit form.
        if !bitwise && size == 3 && (!q || matches!(opcode, 0b01100 | 0b01101 | 0b10100 | 0b10101)) {
            return None;
        }
        let esize = if bitwise { 8 } else { 8u8 << size };
        return Some(Instruction::SimdThreeSame { rd, rn, rm, op, esize, q });
    }

    // 29. AdvSIMD two-register miscellaneous: 0 Q U 01110 size 10000 opcode 10 Rn Rd
    if (raw & 0x9F3E_0C00) == 0x0E20_0800 {
        let op = match ((raw >> 12) & 0x1F, u) {
            (0b01000, false) => SimdOp2::Cmgt0,
            (0b01000, true) => SimdOp2::Cmge0,
            (0b01001, false) => SimdOp2::Cmeq0,
            (0b01001, true) => SimdOp2::Cmle0,
            (0b01010, false) => SimdOp2::Cmlt0,
            (0b00101, false) if size == 0 => SimdOp2::Cnt,
            (0b00101, true) if size == 0 => SimdOp2::Not,
            (0b01011, false) => SimdOp2::Abs,
            (0b01011, true) => SimdOp2::Neg,
            _ => return None,
        };
        if size == 3 && !q {
            return None;
        }
        return Some(Instruction::SimdTwoMisc { rd, rn, op, esize: 8u8 << size, q });
    }

    // 31. AdvSIMD modified immediate: 0 Q op 0111100000 abc cmode o2 1 defgh Rd
    if (raw & 0x9FF8_0C00) == 0x0F00_0400 {
        let imm8 = (((raw >> 16) & 0x7) << 5 | ((raw >> 5) & 0x1F)) as u64;
        let cmode = (raw >> 12) & 0xF;
        let rep32 = |v: u64| v | v << 32;
        let rep16 = |v: u64| rep32(v | v << 16);
        let imm = match cmode >> 1 {
            0..=3 => rep32(imm8 << (8 * (cmode >> 1))),
            4 | 5 => rep16(imm8 << (8 * ((cmode >> 1) & 1))),
            6 if cmode & 1 == 0 => rep32(imm8 << 8 | 0xFF),
            6 => rep32(imm8 << 16 | 0xFFFF),
            _ => match (cmode & 1, u) {
                (0, false) => rep16(imm8 | imm8 << 8),
                (0, true) => (0..8).fold(0u64, |acc, i| acc | if imm8 >> i & 1 != 0 { 0xFF << (8 * i) } else { 0 }),
                (_, false) => {
                    // FMOV Vd.<2S|4S>, #imm
                    let (a, b, cd, efgh) = (imm8 >> 7, (imm8 >> 6) & 1, (imm8 >> 4) & 3, imm8 & 0xF);
                    rep32(a << 31 | (b ^ 1) << 30 | (if b != 0 { 0x1F } else { 0 }) << 25 | cd << 23 | efgh << 19)
                }
                (_, true) => {
                    if !q {
                        return None;
                    }
                    // FMOV Vd.2D, #imm
                    let (a, b, cd, efgh) = (imm8 >> 7, (imm8 >> 6) & 1, (imm8 >> 4) & 3, imm8 & 0xF);
                    a << 63 | (b ^ 1) << 62 | (if b != 0 { 0xFF } else { 0 }) << 54 | cd << 52 | efgh << 48
                }
            },
        };
        // cmode 0xx1 / 10x1 are ORR (op 0) / BIC (op 1); the rest move, MVNI being op 1 below cmode 1110.
        let op = if cmode & 1 == 1 && cmode < 12 {
            if u { SimdImmOp::Bic } else { SimdImmOp::Orr }
        } else {
            SimdImmOp::Mov
        };
        let imm = if op == SimdImmOp::Mov && u && cmode < 14 { !imm } else { imm };
        return Some(Instruction::SimdModImm { rd, imm, op, q });
    }

    // 30. AdvSIMD shift by immediate: 0 Q U 011110 immh immb opcode 1 Rn Rd (immh != 0)
    if (raw & 0x9F80_0400) == 0x0F00_0400 {
        let immh = (raw >> 19) & 0xF;
        if immh == 0 {
            return None;
        }
        let imm = (raw >> 16) & 0x7F; // immh:immb
        let esize = 8u32 << (31 - immh.leading_zeros());
        let (op, esize, shift) = match ((raw >> 11) & 0x1F, u) {
            (0b00000, false) => (SimdShiftOp::Sshr, esize, 2 * esize - imm),
            (0b00000, true) => (SimdShiftOp::Ushr, esize, 2 * esize - imm),
            (0b01010, false) => (SimdShiftOp::Shl, esize, imm - esize),
            (0b10000, false) if esize < 64 => (SimdShiftOp::Shrn, esize, 2 * esize - imm),
            _ => return None,
        };
        if esize == 64 && !q && op != SimdShiftOp::Shrn {
            return None;
        }
        return Some(Instruction::SimdShiftImm { rd, rn, op, esize: esize as u8, shift: shift as u8, q });
    }

    None
}

/// Sign-extends an `n`-bit unsigned integer to a 64-bit signed integer.
#[inline(always)]
fn sign_extend(val: u64, bits: u32) -> i64 {
    let shift = 64 - bits;
    ((val << shift) as i64) >> shift
}

/// Decodes an ARMv8 bitmask immediate (`N:imms:immr`) for `AND`, `ORR`, `EOR`, and `ANDS`.
fn decode_bit_mask(is_64: bool, n: u8, imms: u8, immr: u8) -> Option<u64> {
    if !is_64 && n != 0 {
        return None;
    }
    let combined = ((n as u32) << 6) | ((!imms as u32) & 0x3F);
    if combined == 0 {
        return None;
    }
    let len = 31 - combined.leading_zeros();
    if len < 1 {
        return None;
    }
    let esize = 1u32 << len;
    let levels = esize - 1;
    let s = (imms as u32) & levels;
    let r = (immr as u32) & levels;
    if s == levels {
        return None;
    }
    let welem: u64 = if s + 1 == 64 {
        u64::MAX
    } else {
        (1u64 << (s + 1)) - 1
    };
    let rotated = if r == 0 {
        welem
    } else if esize == 64 {
        welem.rotate_right(r)
    } else {
        let mask = (1u64 << esize) - 1;
        let elem = welem & mask;
        ((elem >> r) | (elem << (esize - r))) & mask
    };
    let mut result = 0u64;
    let mut shift = 0u32;
    while shift < 64 {
        result |= rotated << shift;
        shift += esize;
    }
    if !is_64 {
        result &= 0xFFFF_FFFF;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_nop() {
        assert_eq!(decode(0xD503201F), Instruction::Nop);
    }

    #[test]
    fn test_decode_svc() {
        assert_eq!(decode(0xD40004E1), Instruction::Svc { imm: 0x27 });
    }

    #[test]
    fn test_decode_ret() {
        assert_eq!(
            decode(0xD65F03C0),
            Instruction::BranchReg {
                rn: 30,
                op: BranchRegOp::Ret
            }
        );
    }

    #[test]
    fn test_decode_movz() {
        match decode(0xD2800540) {
            Instruction::MoveWide { rd, imm, shift, op, is_64 } => {
                assert_eq!(rd, 0);
                assert_eq!(imm, 42);
                assert_eq!(shift, 0);
                assert_eq!(op, MoveWideOp::Movz);
                assert!(is_64);
            }
            other => panic!("Unexpected decode: {:?}", other),
        }
    }

    #[test]
    fn test_decode_fadd() {
        assert_eq!(
            decode(0x1E622820),
            Instruction::FpData2 {
                rd: 0,
                rn: 1,
                rm: 2,
                op: FpOp2::Fadd,
                is_double: true,
            }
        );
    }
}
