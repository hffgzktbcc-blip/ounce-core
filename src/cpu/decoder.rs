#![allow(dead_code)]

use super::instructions::{
    SimdAcrossOp, SimdLongOp, SimdNarrowOp, SimdSatLongOp,
    SimdElemOp, BranchRegOp, Condition, DataProc1Op, FpOp2, Instruction, LogicalOp, MoveWideOp, PacKey, PacModifier, PauthOp, ShiftType,
    SimdFpElementOp, SimdFpOp2, SimdFpOp3, SimdImmOp, SimdOp2, SimdShiftExtOp, SimdOp3, SimdPermuteOp, SimdShiftOp,
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
    if (raw & 0x1F800000) == 0x12800000 {
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

        // opc 01 with L=1 is LDPSW; opc 01 with L=0 and opc 11 are unallocated.
        if opc == 3 || (opc == 1 && !is_load) {
            return Instruction::Unimplemented { raw };
        }
        return Instruction::LoadStorePair {
            rt1,
            rt2,
            rn,
            offset,
            is_load,
            is_64,
            signed_word: opc == 1,
            writeback,
            post_index,
        };
    }

    // 12z. PRFM (literal): prefetch hint, no architectural effect (must not be decoded as a load of Rt)
    if (raw & 0xFF00_0000) == 0xD800_0000 {
        return Instruction::Nop;
    }

    // 13. Load / Store Single Register (Immediate unsigned offset, V == 0)
    if (raw & 0x3F000000) == 0x39000000 {
        if (raw >> 30) == 3 && ((raw >> 22) & 0x3) == 2 {
            return Instruction::Nop; // PRFM (immediate)
        }
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
            is_64_target: size == 3 || opc == 2,
            writeback: false,
            post_index: false,
        };
    }

    // 13b. Load / Store Single Register (Unscaled Immediate LDUR/STUR, Post-Index, Pre-Index, & Register Offset, V == 0)
    if (raw & 0x3F000000) == 0x38000000 {
        let size = ((raw >> 30) & 0x3) as u8;
        let opc = ((raw >> 22) & 0x3) as u8;
        if size == 3 && opc == 2 {
            return Instruction::Nop; // PRFUM / PRFM (register): prefetch hints
        }
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
                is_64_target: size == 3 || opc == 2,
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
    if (raw & 0xFF200C00) == 0x1E200800 {
        let ftype = (raw >> 22) & 0x3;
        if ftype == 3 && (4..=7).contains(&((raw >> 12) & 0xF)) {
            // FMAX/FMIN/FMAXNM/FMINNM Hd, Hn, Hm (FEAT_FP16)
            let op = [FpOp2::Fmax, FpOp2::Fmin, FpOp2::Fmaxnm, FpOp2::Fminnm][(((raw >> 12) & 0xF) - 4) as usize];
            return Instruction::FpHalfMinMax { rd: (raw & 0x1F) as u8, rn: ((raw >> 5) & 0x1F) as u8, rm: ((raw >> 16) & 0x1F) as u8, op };
        }
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
                0b0110 => FpOp2::Fmaxnm,
                0b0111 => FpOp2::Fminnm,
                0b1000 => FpOp2::Fnmul,
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

    // 14a1. Floating-Point 3-Source: 0 0 0 11111 ftype o1 Rm o0 Ra Rn Rd
    if (raw & 0xFF00_0000) == 0x1F00_0000 && (raw >> 22) & 0x3 <= 1 {
        return Instruction::FpData3 {
            rd: (raw & 0x1F) as u8,
            rn: ((raw >> 5) & 0x1F) as u8,
            rm: ((raw >> 16) & 0x1F) as u8,
            ra: ((raw >> 10) & 0x1F) as u8,
            o1: raw & (1 << 21) != 0,
            o0: raw & (1 << 15) != 0,
            is_double: (raw >> 22) & 0x3 == 1,
        };
    }

    // 14a2. Floating-Point round to integral: FRINTx
    if (raw & 0xFF3C7C00) == 0x1E244000 && ((raw >> 15) & 0x7) != 5 && (raw >> 22) & 0x3 <= 1 {
        return Instruction::FpRound {
            rd: (raw & 0x1F) as u8,
            rn: ((raw >> 5) & 0x1F) as u8,
            op: ((raw >> 15) & 0x7) as u8,
            is_double: (raw >> 22) & 0x3 == 1,
        };
    }

    // 14a4. Float <-> fixed-point conversion: FCVTZS/FCVTZU (rmode 11, opcode 0/1), SCVTF/UCVTF (rmode 00,
    // opcode 2/3). Pattern: sf 0 0 11110 ftype 0 rmode opcode scale Rn Rd, fbits = 64 - scale
    if (raw & 0x7F200000) == 0x1E000000 && (raw >> 22) & 0x3 <= 1 {
        let rmode = ((raw >> 19) & 0x3) as u8;
        let opcode = ((raw >> 16) & 0x7) as u8;
        let scale = (raw >> 10) & 0x3F;
        let is_64 = raw & (1 << 31) != 0;
        let valid = matches!((rmode, opcode), (3, 0) | (3, 1) | (0, 2) | (0, 3)) && (is_64 || scale >= 32);
        if valid {
            return Instruction::FpFixedConv {
                rd: (raw & 0x1F) as u8,
                rn: ((raw >> 5) & 0x1F) as u8,
                opcode,
                fbits: (64 - scale) as u8,
                is_64,
                is_double: (raw >> 22) & 0x3 == 1,
            };
        }
    }

    // 14a3. Float <-> integer conversion (FCVTxS/U, SCVTF, UCVTF, FCVTAS/AU); FMOV (opcodes 6, 7) is elsewhere
    // Pattern: sf 0 0 11110 ftype 1 rmode opcode 000000 Rn Rd
    // SCVTF/UCVTF/FCVTAS/FCVTAU (opcodes 2 to 5) exist only with rmode 00; sf = 1 is allowed for both types
    if (raw & 0x7F20FC00) == 0x1E200000
        && ((raw >> 16) & 0x7) <= 5
        && ((raw >> 16) & 0x7 <= 1 || (raw >> 19) & 0x3 == 0)
        && (raw >> 22) & 0x3 <= 1
    {
        return Instruction::FpIntConv {
            rd: (raw & 0x1F) as u8,
            rn: ((raw >> 5) & 0x1F) as u8,
            opcode: ((raw >> 16) & 0x7) as u8,
            rmode: ((raw >> 19) & 0x3) as u8,
            is_64: raw & (1 << 31) != 0,
            is_double: (raw >> 22) & 0x3 == 1,
        };
    }

    // 14a. Floating-Point 1-Source: FMOV, FABS, FNEG, FSQRT (opcode 0000xx), FCVT single <-> double
    // (opcode 0001 opc, opc = destination type: 100 to single, 101 to double)
    // Pattern: 0 0 0 11110 ftype 1 opcode(6) 10000 Rn Rd
    let fp1_op = (raw >> 15) & 0x3F;
    if (raw & 0xFF207C00) == 0x1E204000 && (fp1_op <= 5) && (raw >> 22) & 0x3 <= 1 {
        return Instruction::FpData1 {
            rd: (raw & 0x1F) as u8,
            rn: ((raw >> 5) & 0x1F) as u8,
            op: ((raw >> 15) & 0x7) as u8,
            is_double: (raw >> 22) & 0x3 == 1,
        };
    }

    // 14b. Floating-Point Conditional Select: FCSEL
    // Pattern: 0 0 0 11110 ftype 1 Rm cond 11 Rn Rd
    if (raw & 0xFF200C00) == 0x1E200C00 && (raw >> 22) & 0x3 <= 1 {
        return Instruction::FpCondSelect {
            rd: (raw & 0x1F) as u8,
            rn: ((raw >> 5) & 0x1F) as u8,
            rm: ((raw >> 16) & 0x1F) as u8,
            cond: Condition::from_u8(((raw >> 12) & 0xF) as u8),
            is_double: (raw >> 22) & 0x3 == 1,
        };
    }

    // 15. Floating-Point Compare: FCMP
    // FCMP/FCMPE (bit 4 = signalling) with a register (bit 3 = 0) or #0.0 (bit 3 = 1) operand
    if (raw & 0xFF20FC07) == 0x1E202000 && (raw >> 22) & 0x3 <= 1 {
        let ftype = (raw >> 22) & 0x3;
        let is_double = ftype == 1;
        let rm = ((raw >> 16) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        return Instruction::FpCompare {
            rn,
            rm,
            is_zero: raw & 0x8 != 0,
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
            // CRC32{B,H,W,X} / CRC32C{B,H,W,X}: opcode 010 C sz, sf must equal (sz == 3)
            0b010000..=0b010111 => {
                let size = (opcode & 0x3) as u8;
                if is_64 == (size == 3) {
                    return Instruction::Crc32 { rd, rn, rm, size, castagnoli: (opcode & 0x4) != 0 };
                }
            }
            _ => {}
        }
    }

    // 19. Floating-Point / General Register Transfer: FMOV Dd, Xn / FMOV Xd, Dn / FMOV Sd, Wn / FMOV Wd, Sn
    // 32-bit forms are type 00 (S), 64-bit forms type 01 (D)
    if (raw & 0x7FBEFC00) == 0x1E260000 && (raw >> 22) & 1 == raw >> 31 {
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
    //     FDIV Vd.4S, Vn.4S, Vm.4S (0x6E20FC00)
    if (raw & 0xFF20FC00) == 0x4E20D400 || (raw & 0xFFE0FC00) == 0x6E20DC00 || (raw & 0xFFE0FC00) == 0x6E20FC00 {
        let rm = ((raw >> 16) & 0x1F) as u8;
        let rn = ((raw >> 5) & 0x1F) as u8;
        let rd = (raw & 0x1F) as u8;
        let op = if (raw & 0xFFE0FC00) == 0x6E20DC00 {
            FpOp2::Fmul
        } else if (raw & 0xFFE0FC00) == 0x6E20FC00 {
            FpOp2::Fdiv
        } else if (raw & (1 << 23)) != 0 {
            FpOp2::Fsub
        } else {
            FpOp2::Fadd
        };
        return if raw & (1 << 22) != 0 {
            Instruction::SimdVec2D { rd, rn, rm, op }
        } else {
            Instruction::SimdVec4S { rd, rn, rm, op }
        };
    }

    decode_extra(raw).or_else(|| decode_intvec(raw)).unwrap_or(Instruction::Unimplemented { raw })
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
        let size_field = (raw >> 10) & 0x3;
        let regs = match (raw >> 12) & 0xF {
            0b0111 => 1,
            0b1010 => 2,
            0b0110 => 3,
            0b0010 => 4,
            0b1000 | 0b0100 | 0b0000 => {
                // LD2/LD3/LD4, ST2/ST3/ST4: interleaved elements; 64-bit elements need Q
                if size_field == 3 && !q {
                    return None;
                }
                return Some(Instruction::SimdLoadStoreStruct {
                    rt: rd,
                    rn,
                    rm: if post { rm } else { 31 },
                    selem: [4, 3, 2][((raw >> 14) & 0x3) as usize],
                    esize: 8 << size_field,
                    q,
                    is_load: (raw & (1 << 22)) != 0,
                    post_index: post,
                });
            }
            _ => return None,
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

    // 27a. AdvSIMD scalar copy: 01 op=0 11110000 imm5 0 0000 1 Rn Rd (DUP scalar from element)
    if (raw & 0xFFE0_FC00) == 0x5E00_0400 {
        let imm5 = (raw >> 16) & 0x1F;
        if imm5 & 0xF == 0 {
            return None;
        }
        let low = imm5.trailing_zeros();
        return Some(Instruction::SimdDupScalar { rd, rn, esize: 8u8 << low, index: (imm5 >> (low + 1)) as u8 });
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
            0b0101 if esize <= 32 && (q || esize <= 16) => {
                return Some(Instruction::SimdSmov { rd, rn, esize, index, x: q });
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

    // 21b'. Add/subtract with carry: sf op S 11010000 Rm 000000 Rn Rd
    if (raw & 0x1FE0_FC00) == 0x1A00_0000 {
        return Some(Instruction::AddSubCarry {
            rd,
            rn,
            rm,
            is_sub: raw & (1 << 30) != 0,
            set_flags: raw & (1 << 29) != 0,
            is_64: raw & (1 << 31) != 0,
        });
    }

    // 21c. FCCMP/FCCMPE: 0 0 0 11110 ftype 1 Rm cond 01 Rn op nzcv (ftype 0 = single, 1 = double)
    if (raw & 0xFF20_0C00) == 0x1E20_0400 && (size == 0 || size == 1) {
        return Some(Instruction::FpCondCompare {
            rn,
            rm,
            nzcv: (raw & 0xF) as u8,
            cond: Condition::from_u8(((raw >> 12) & 0xF) as u8),
            is_double: size == 1,
        });
    }

    // 26a'. AdvSIMD vector x indexed element, floating point: 0 Q U 01111 1 sz L M Rm opcode H 0 Rn Rd
    //       (FMLA 0001, FMLS 0101, FMUL 1001, FMULX U=1 1001); other opcodes (SDOT, ...) fall through.
    //       Scalar form: 01 U 11111 1 sz L M Rm opcode H 0 Rn Rd (Sd/Dd, Sn/Dn, Vm.S/D[index]), sz 10 or 11 only.
    for (mask, want, scalar) in [(0x9F80_0400u32, 0x0F80_0000u32, false), (0xDF80_0400, 0x5F80_0000, true)] {
        if (raw & mask) != want {
            continue;
        }
        let op = match ((raw >> 12) & 0xF, u) {
            (0b0001, false) => Some(SimdFpElementOp::Fmla),
            (0b0101, false) => Some(SimdFpElementOp::Fmls),
            (0b1001, false) => Some(SimdFpElementOp::Fmul),
            (0b1001, true) => Some(SimdFpElementOp::Fmulx),
            _ => None,
        };
        if let Some(op) = op {
            let (h, l, m) = ((raw >> 11) & 1, (raw >> 21) & 1, (raw >> 20) & 1);
            let rm = ((m << 4) | ((raw >> 16) & 0xF)) as u8;
            let double = size == 3;
            if scalar && size < 2 {
                return None; // half precision scalar by element is FEAT_FP16, not decoded here
            }
            if double && (l == 1 || (!q && !scalar)) {
                return None;
            }
            let index = if double { h } else { (h << 1) | l } as u8;
            return Some(Instruction::SimdFpByElement { rd, rn, rm, index, op, double, q: q || scalar && double, scalar });
        }
    }

    // 26a''. AdvSIMD indexed element, integer: 0 Q U 01111 size L M Rm opcode H 0 Rn Rd
    //        MUL/MLA/MLS/SQDMULH/SQRDMULH, S/UMULL/MLAL/MLSL, SQDMULL/SQDMLAL/SQDMLSL (size 1 or 2), SDOT/UDOT (size 2)
    if (raw & 0x9F00_0400) == 0x0F00_0000 && (size == 1 || size == 2) {
        let opcode = (raw >> 12) & 0xF;
        let (h, l, m) = ((raw >> 11) & 1, (raw >> 21) & 1, (raw >> 20) & 1);
        // halfword elements: Rm is 4 bits and M extends the index; word elements: M extends Rm
        let (rm, index) = if size == 1 {
            (((raw >> 16) & 0xF) as u8, ((h << 2) | (l << 1) | m) as u8)
        } else {
            ((((m << 4) | ((raw >> 16) & 0xF)) as u8), ((h << 1) | l) as u8)
        };
        let esize = 8u8 << size;
        let op = match (opcode, u) {
            (0b1000, false) => SimdElemOp::Op3(SimdOp3::Mul),
            (0b0000, true) => SimdElemOp::Op3(SimdOp3::Mla),
            (0b0100, true) => SimdElemOp::Op3(SimdOp3::Mls),
            (0b1100, false) => SimdElemOp::Op3(SimdOp3::Sqdmulh),
            (0b1101, false) => SimdElemOp::Op3(SimdOp3::Sqrdmulh),
            (0b1010, _) => SimdElemOp::Long(SimdLongOp::Mull, !u),
            (0b0010, _) => SimdElemOp::Long(SimdLongOp::Mlal, !u),
            (0b0110, _) => SimdElemOp::Long(SimdLongOp::Mlsl, !u),
            (0b1011, false) => SimdElemOp::SatLong(SimdSatLongOp::Sqdmull),
            (0b0011, false) => SimdElemOp::SatLong(SimdSatLongOp::Sqdmlal),
            (0b0111, false) => SimdElemOp::SatLong(SimdSatLongOp::Sqdmlsl),
            (0b1110, _) if size == 2 => {
                return Some(Instruction::SimdByElement {
                    rd, rn, rm, index: ((h << 1) | l) as u8, op: SimdElemOp::Dot(!u), esize: 32, q, scalar: false,
                });
            }
            _ => return None,
        };
        return Some(Instruction::SimdByElement { rd, rn, rm, index, op, esize, q, scalar: false });
    }

    // 26a'''. AdvSIMD scalar x indexed element, integer: 01 U 11111 size L M Rm opcode H 0 Rn Rd
    //         SQDMULH, SQRDMULH, SQDMULL, SQDMLAL, SQDMLSL (size 1 or 2)
    if (raw & 0xDF00_0400) == 0x5F00_0000 && !u && (size == 1 || size == 2) {
        let (h, l, m) = ((raw >> 11) & 1, (raw >> 21) & 1, (raw >> 20) & 1);
        let (rm, index) = if size == 1 {
            (((raw >> 16) & 0xF) as u8, ((h << 2) | (l << 1) | m) as u8)
        } else {
            ((((m << 4) | ((raw >> 16) & 0xF)) as u8), ((h << 1) | l) as u8)
        };
        let op = match (raw >> 12) & 0xF {
            0b1100 => SimdElemOp::Op3(SimdOp3::Sqdmulh),
            0b1101 => SimdElemOp::Op3(SimdOp3::Sqrdmulh),
            0b1011 => SimdElemOp::SatLong(SimdSatLongOp::Sqdmull),
            0b0011 => SimdElemOp::SatLong(SimdSatLongOp::Sqdmlal),
            0b0111 => SimdElemOp::SatLong(SimdSatLongOp::Sqdmlsl),
            _ => return None,
        };
        return Some(Instruction::SimdByElement { rd, rn, rm, index, op, esize: 8u8 << size, q: false, scalar: true });
    }

    // 26a4. AdvSIMD vector SDOT/UDOT: 0 Q U 01110 10 0 Rm 10010 1 Rn Rd
    if (raw & 0x9FE0_FC00) == 0x0E80_9400 {
        return Some(Instruction::SimdByElement { rd, rn, rm, index: 0xFF, op: SimdElemOp::Dot(!u), esize: 32, q, scalar: false });
    }

    // 26b'. AdvSIMD extract: 0 Q 101110 00 0 Rm 0 imm4 0 Rn Rd (EXT)
    if (raw & 0xBFE0_8400) == 0x2E00_0000 {
        let index = ((raw >> 11) & 0xF) as u8;
        if !q && index >= 8 {
            return None;
        }
        return Some(Instruction::SimdExt { rd, rn, rm, index, q });
    }

    // 26b2. AdvSIMD table lookup: 0 Q 001110 00 0 Rm 0 len op 00 Rn Rd (TBL/TBX)
    if (raw & 0xBFE0_8C00) == 0x0E00_0000 {
        let regs = (((raw >> 13) & 0x3) + 1) as u8;
        return Some(Instruction::SimdTbl { rd, rn, rm, regs, is_tbx: (raw & (1 << 12)) != 0, q });
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

    // 27b. AdvSIMD three different (widening multiply): 0 Q U 01110 size 1 Rm opcode 00 Rn Rd
    if (raw & 0x9F20_0C00) == 0x0E20_0000 {
        let opcode = (raw >> 12) & 0xF;
        if matches!(opcode, 0b0100 | 0b0110) && size < 3 {
            let op = match (opcode, u) {
                (0b0100, false) => SimdNarrowOp::Addhn,
                (0b0100, true) => SimdNarrowOp::Raddhn,
                (0b0110, false) => SimdNarrowOp::Subhn,
                _ => SimdNarrowOp::Rsubhn,
            };
            return Some(Instruction::SimdNarrow { rd, rn, rm, op, esize: 8u8 << size, q });
        }
        if matches!(opcode, 0b1001 | 0b1011 | 0b1101) && !u && (size == 1 || size == 2) {
            let op = match opcode {
                0b1001 => SimdSatLongOp::Sqdmlal,
                0b1011 => SimdSatLongOp::Sqdmlsl,
                _ => SimdSatLongOp::Sqdmull,
            };
            return Some(Instruction::SimdSatLong { rd, rn, rm, op, esize: 8u8 << size, q });
        }
        // PMULL / PMULL2: opcode 1110, U == 0, size 0 (8H from 8B) or size 3 (1Q from 1D).
        if (raw >> 12) & 0xF == 0b1110 {
            if u || (size != 0 && size != 3) {
                return None;
            }
            return Some(Instruction::Pmull { rd, rn, rm, wide: size == 3, q });
        }
        let op = match (raw >> 12) & 0xF {
            0b0000 => SimdLongOp::Addl,
            0b0001 => SimdLongOp::Addw,
            0b0010 => SimdLongOp::Subl,
            0b0011 => SimdLongOp::Subw,
            0b0101 => SimdLongOp::Abal,
            0b0111 => SimdLongOp::Abdl,
            0b1000 => SimdLongOp::Mlal,
            0b1010 => SimdLongOp::Mlsl,
            0b1100 => SimdLongOp::Mull,
            _ => return None,
        };
        if size == 3 {
            return None; // PMULL / reserved
        }
        return Some(Instruction::SimdLong { rd, rn, rm, op, signed: !u, esize: 8u8 << size, q });
    }

    // 27b2. FABD (scalar): 01 1 11110 1 sz 1 Rm 11010 1 Rn Rd (bit 23 set, sz = bit 22)
    if (raw & 0xFFA0_FC00) == 0x7EA0_D400 {
        return Some(Instruction::FpAbd { rd, rn, rm, is_double: (raw >> 22) & 1 == 1 });
    }

    // 27c. AdvSIMD scalar three same: 01 U 11110 size 1 Rm opcode 1 Rn Rd.
    //      A one-lane op writes lane 0 and zeroes the rest, exactly the scalar semantics.
    if (raw & 0xDF20_0400) == 0x5E20_0400 && (raw >> 11) & 0x1F < 0b11000 {
        let op = match ((raw >> 11) & 0x1F, u) {
            (0b00001, false) => SimdOp3::Sqadd,
            (0b00001, true) => SimdOp3::Uqadd,
            (0b00101, false) => SimdOp3::Sqsub,
            (0b00101, true) => SimdOp3::Uqsub,
            (0b00110, false) => SimdOp3::Cmgt,
            (0b00110, true) => SimdOp3::Cmhi,
            (0b00111, false) => SimdOp3::Cmge,
            (0b00111, true) => SimdOp3::Cmhs,
            (0b01000, false) => SimdOp3::Sshl,
            (0b01000, true) => SimdOp3::Ushl,
            (0b01001, false) => SimdOp3::Sqshl,
            (0b01001, true) => SimdOp3::Uqshl,
            (0b01010, false) => SimdOp3::Srshl,
            (0b01010, true) => SimdOp3::Urshl,
            (0b01011, false) => SimdOp3::Sqrshl,
            (0b01011, true) => SimdOp3::Uqrshl,
            (0b10000, false) => SimdOp3::Add,
            (0b10000, true) => SimdOp3::Sub,
            (0b10001, false) => SimdOp3::Cmtst,
            (0b10001, true) => SimdOp3::Cmeq,
            (0b10110, false) => SimdOp3::Sqdmulh,
            (0b10110, true) => SimdOp3::Sqrdmulh,
            _ => return None,
        };
        // Only the saturating add/sub/shift forms have byte, half and word scalar sizes; SQDMULH has half and word.
        let sized = matches!(
            op,
            SimdOp3::Sqadd | SimdOp3::Uqadd | SimdOp3::Sqsub | SimdOp3::Uqsub | SimdOp3::Sqshl | SimdOp3::Uqshl
                | SimdOp3::Sqrshl | SimdOp3::Uqrshl
        );
        let ok = if sized { true } else if matches!(op, SimdOp3::Sqdmulh | SimdOp3::Sqrdmulh) { size == 1 || size == 2 } else { size == 3 };
        if !ok {
            return None;
        }
        return Some(Instruction::SimdScalarThreeSame { rd, rn, rm, op, esize: 8u8 << size });
    }

    // 27c2. AdvSIMD three same, half precision (FEAT_FP16): 0 Q U 01110 a 10 Rm 00 opcode 1 Rn Rd.
    //       Only the FMAX/FMIN family is decoded: opcode 000 (NM) and 110, U = 1 for the pairwise forms.
    if (raw & 0x9F60_C400) == 0x0E40_0400 && matches!((raw >> 11) & 7, 0b000 | 0b110) {
        let a = (raw >> 23) & 1 == 1;
        let op = match ((raw >> 11) & 7, u, a) {
            (0b000, false, false) => SimdFpOp3::Fmaxnm,
            (0b000, false, true) => SimdFpOp3::Fminnm,
            (0b110, false, false) => SimdFpOp3::Fmax,
            (0b110, false, true) => SimdFpOp3::Fmin,
            (0b000, true, false) => SimdFpOp3::Fmaxnmp,
            (0b000, true, true) => SimdFpOp3::Fminnmp,
            (0b110, true, false) => SimdFpOp3::Fmaxp,
            (_, true, true) => SimdFpOp3::Fminp,
            _ => return None,
        };
        return Some(Instruction::SimdFpThreeSameHalf { rd, rn, rm, op, q });
    }

    // 27d. AdvSIMD three same, floating point (opcode >= 11000): 0 Q U 01110 a sz 1 Rm opcode 1 Rn Rd.
    //      Half precision (the separate three-same-FP16 encoding) is not decoded.
    if (raw & 0x9F20_0400) == 0x0E20_0400 && (raw >> 11) & 0x1F >= 0b11000 {
        let a = (raw >> 23) & 1 == 1;
        let op = match ((raw >> 11) & 0x1F, u, a) {
            (0b11000, false, false) => SimdFpOp3::Fmaxnm,
            (0b11000, false, true) => SimdFpOp3::Fminnm,
            (0b11001, false, false) => SimdFpOp3::Fmla,
            (0b11001, false, true) => SimdFpOp3::Fmls,
            (0b11010, false, false) => SimdFpOp3::Fadd,
            (0b11010, false, true) => SimdFpOp3::Fsub,
            (0b11000, true, false) => SimdFpOp3::Fmaxnmp,
            (0b11000, true, true) => SimdFpOp3::Fminnmp,
            (0b11110, true, false) => SimdFpOp3::Fmaxp,
            (0b11110, true, true) => SimdFpOp3::Fminp,
            (0b11010, true, false) => SimdFpOp3::Faddp,
            (0b11010, true, true) => SimdFpOp3::Fabd,
            (0b11011, true, false) => SimdFpOp3::Fmul,
            (0b11011, false, false) => SimdFpOp3::Fmulx,
            (0b11111, false, false) => SimdFpOp3::Frecps,
            (0b11111, false, true) => SimdFpOp3::Frsqrts,
            (0b11100, false, false) => SimdFpOp3::Fcmeq,
            (0b11100, true, false) => SimdFpOp3::Fcmge,
            (0b11100, true, true) => SimdFpOp3::Fcmgt,
            (0b11101, true, false) => SimdFpOp3::Facge,
            (0b11101, true, true) => SimdFpOp3::Facgt,
            (0b11110, false, false) => SimdFpOp3::Fmax,
            (0b11110, false, true) => SimdFpOp3::Fmin,
            (0b11111, true, false) => SimdFpOp3::Fdiv,
            _ => return None,
        };
        let double = (raw >> 22) & 1 == 1;
        if double && !q {
            return None;
        }
        return Some(Instruction::SimdFpThreeSame { rd, rn, rm, op, double, q });
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
            (0b10011, false) => SimdOp3::Mul,
            (0b10011, true) if size == 0 => SimdOp3::Pmul,
            (0b01000, false) => SimdOp3::Sshl,
            (0b01000, true) => SimdOp3::Ushl,
            (0b00000, false) => SimdOp3::Shadd,
            (0b00000, true) => SimdOp3::Uhadd,
            (0b00001, false) => SimdOp3::Sqadd,
            (0b00001, true) => SimdOp3::Uqadd,
            (0b00010, false) => SimdOp3::Srhadd,
            (0b00010, true) => SimdOp3::Urhadd,
            (0b00100, false) => SimdOp3::Shsub,
            (0b00100, true) => SimdOp3::Uhsub,
            (0b00101, false) => SimdOp3::Sqsub,
            (0b00101, true) => SimdOp3::Uqsub,
            (0b01001, false) => SimdOp3::Sqshl,
            (0b01001, true) => SimdOp3::Uqshl,
            (0b01010, false) => SimdOp3::Srshl,
            (0b01010, true) => SimdOp3::Urshl,
            (0b01011, false) => SimdOp3::Sqrshl,
            (0b01011, true) => SimdOp3::Uqrshl,
            (0b01110, false) => SimdOp3::Sabd,
            (0b01110, true) => SimdOp3::Uabd,
            (0b01111, false) => SimdOp3::Saba,
            (0b01111, true) => SimdOp3::Uaba,
            (0b10010, false) => SimdOp3::Mla,
            (0b10010, true) => SimdOp3::Mls,
            (0b10110, false) if size == 1 || size == 2 => SimdOp3::Sqdmulh,
            (0b10110, true) if size == 1 || size == 2 => SimdOp3::Sqrdmulh,
            _ => return None,
        };
        let bitwise = opcode == 0b00011;
        // 64-bit lanes need Q. Only the saturating add/sub and the shifts have a 64-bit lane form.
        let has64 = matches!(
            op,
            SimdOp3::Add | SimdOp3::Addp | SimdOp3::Sub | SimdOp3::Cmeq | SimdOp3::Cmtst | SimdOp3::Cmhi | SimdOp3::Cmhs | SimdOp3::Cmgt
                | SimdOp3::Cmge | SimdOp3::Sshl | SimdOp3::Ushl | SimdOp3::Sqadd | SimdOp3::Uqadd | SimdOp3::Sqsub
                | SimdOp3::Uqsub | SimdOp3::Sqshl | SimdOp3::Uqshl | SimdOp3::Srshl | SimdOp3::Urshl | SimdOp3::Sqrshl
                | SimdOp3::Uqrshl
        );
        if !bitwise && size == 3 && (!q || !has64) {
            return None;
        }
        let esize = if bitwise { 8 } else { 8u8 << size };
        return Some(Instruction::SimdThreeSame { rd, rn, rm, op, esize, q });
    }

    // 28a. FMAXNMV/FMINNMV/FMAXV/FMINV: 0 Q U 01110 o1 sz 11000 opcode 10 Rn Rd, opcode 01100 / 01111.
    //      U = 1: Sd, Vn.4S (Q = 1, sz = 0). U = 0: half precision, Hd, Vn.4H/8H (sz = 0).
    // 28a2. Scalar pairwise FMAXNMP/FMINNMP/FMAXP/FMINP: 01 U 11110 o1 sz 11000 opcode 10 Rn Rd
    //      U = 1: Sd, Vn.2S / Dd, Vn.2D. U = 0: Hd, Vn.2H.
    for (mask, want, scalar) in [(0x9F7E_0C00u32, 0x0E30_0800u32, false), (0xDF3E_0C00, 0x5E30_0800, true)] {
        let opcode = (raw >> 12) & 0x1F;
        // Scalar FADDP Sd/Dd, Vn.2S/2D (opcode 01101, U = 1, bit 23 = 0): sum of the two lanes.
        if scalar && (raw & mask) == want && opcode == 0b01101 && u && (raw >> 23) & 1 == 0 {
            let esize = if (raw >> 22) & 1 == 1 { 64 } else { 32 };
            return Some(Instruction::SimdFpReduce { rd, rn, op: SimdFpOp3::Faddp, esize, lanes: 2 });
        }
        if (raw & mask) == want && matches!(opcode, 0b01100 | 0b01111) {
            let min = (raw >> 23) & 1 == 1;
            let op = match (opcode, min) {
                (0b01100, false) => SimdFpOp3::Fmaxnm,
                (0b01100, true) => SimdFpOp3::Fminnm,
                (_, false) => SimdFpOp3::Fmax,
                (_, true) => SimdFpOp3::Fmin,
            };
            let sz = (raw >> 22) & 1 == 1;
            let (esize, lanes) = match (scalar, u) {
                (false, false) => (16u8, if q { 8u8 } else { 4 }),
                (false, true) if q && !sz => (32, 4),
                (true, false) if !sz => (16, 2),
                (true, true) => (if sz { 64 } else { 32 }, 2),
                _ => return None,
            };
            return Some(Instruction::SimdFpReduce { rd, rn, op, esize, lanes });
        }
    }

    // 28b. AdvSIMD across lanes: 0 Q U 01110 size 11000 opcode 10 Rn Rd
    if (raw & 0x9F3E_0C00) == 0x0E30_0800 {
        let op = match ((raw >> 12) & 0x1F, u) {
            (0b11011, false) => SimdAcrossOp::Addv,
            (0b00011, false) => SimdAcrossOp::Saddlv,
            (0b00011, true) => SimdAcrossOp::Uaddlv,
            (0b01010, false) => SimdAcrossOp::Smaxv,
            (0b01010, true) => SimdAcrossOp::Umaxv,
            (0b11010, false) => SimdAcrossOp::Sminv,
            (0b11010, true) => SimdAcrossOp::Uminv,
            _ => return None,
        };
        // 64-bit lanes do not exist for these, and a 2S reduction needs Q.
        if size == 3 || (size == 2 && !q) {
            return None;
        }
        return Some(Instruction::SimdAcross { rd, rn, op, esize: 8u8 << size, q });
    }

    // 29a2. AdvSIMD scalar two-register miscellaneous, saturating forms: SUQADD/USQADD/SQABS/SQNEG (all sizes)
    if (raw & 0xDF3E_0C00) == 0x5E20_0800 {
        let op = match ((raw >> 12) & 0x1F, u) {
            (0b00011, false) => Some(SimdOp2::Suqadd),
            (0b00011, true) => Some(SimdOp2::Usqadd),
            (0b00111, false) => Some(SimdOp2::Sqabs),
            (0b00111, true) => Some(SimdOp2::Sqneg),
            _ => None,
        };
        if let Some(op) = op {
            return Some(Instruction::SimdScalarTwoMisc { rd, rn, op, esize: 8u8 << size });
        }
    }

    // 29a. AdvSIMD scalar two-register miscellaneous: 01 U 11110 size 10000 opcode 10 Rn Rd (conversions only)
    if (raw & 0xDF3E_0C00) == 0x5E20_0800 {
        let op = match ((raw >> 12) & 0x1F, u, size >> 1) {
            (0b11101, false, 0) => SimdOp2::Scvtf,
            (0b11101, true, 0) => SimdOp2::Ucvtf,
            (0b11011, false, 1) => SimdOp2::Fcvtzs,
            (0b11011, true, 1) => SimdOp2::Fcvtzu,
            _ => return None,
        };
        return Some(Instruction::SimdScalarTwoMisc { rd, rn, op, esize: 32u8 << (size & 1) });
    }

    // 29b. AdvSIMD two-register miscellaneous, floating point: 0 Q U 01110 a sz 10000 opcode 10 Rn Rd
    //      (half precision not decoded; FCVTN/FCVTL only in the double/single form)
    if (raw & 0x9F3E_0C00) == 0x0E20_0800 {
        let a = (raw >> 23) & 1 == 1;
        let double = (raw >> 22) & 1 == 1;
        let fp = match ((raw >> 12) & 0x1F, u, a) {
            (0b01100, false, true) => Some(SimdFpOp2::Fcmgt0),
            (0b01100, true, true) => Some(SimdFpOp2::Fcmge0),
            (0b01101, false, true) => Some(SimdFpOp2::Fcmeq0),
            (0b01101, true, true) => Some(SimdFpOp2::Fcmle0),
            (0b01110, false, true) => Some(SimdFpOp2::Fcmlt0),
            (0b01111, false, true) => Some(SimdFpOp2::Fabs),
            (0b01111, true, true) => Some(SimdFpOp2::Fneg),
            (0b11111, true, true) => Some(SimdFpOp2::Fsqrt),
            (0b11000, false, false) => Some(SimdFpOp2::Frintn),
            (0b11000, false, true) => Some(SimdFpOp2::Frintp),
            (0b11000, true, false) => Some(SimdFpOp2::Frinta),
            (0b11001, false, false) => Some(SimdFpOp2::Frintm),
            (0b11001, false, true) => Some(SimdFpOp2::Frintz),
            (0b11001, true, false) => Some(SimdFpOp2::Frintx),
            (0b11001, true, true) => Some(SimdFpOp2::Frinti),
            (0b11100, false, false) => None,
            (0b11101, false, true) => Some(SimdFpOp2::Frecpe),
            (0b11101, true, true) => Some(SimdFpOp2::Frsqrte),
            (0b11110, false, false) => Some(SimdFpOp2::Frint32z),
            (0b11110, true, false) => Some(SimdFpOp2::Frint32x),
            (0b11111, false, false) => Some(SimdFpOp2::Frint64z),
            (0b11111, true, false) => Some(SimdFpOp2::Frint64x),
            (0b10110, false, false) if double => Some(SimdFpOp2::Fcvtn),
            (0b10111, false, false) if double => Some(SimdFpOp2::Fcvtl),
            _ => None,
        };
        if let Some(op) = fp {
            // FCVTN/FCVTL use Q to pick the half of the wide/narrow register, not the lane count
            let wide_q = matches!(op, SimdFpOp2::Fcvtn | SimdFpOp2::Fcvtl);
            if double && !q && !wide_q {
                return None;
            }
            return Some(Instruction::SimdFpTwoMisc { rd, rn, op, double, q });
        }
        // FCVT{N,M,A,P}{S,U} and URECPE/URSQRTE: integer results
        let int_op = match ((raw >> 12) & 0x1F, u, a) {
            (0b11010, false, false) => Some(SimdOp2::Fcvtns),
            (0b11010, true, false) => Some(SimdOp2::Fcvtnu),
            (0b11010, false, true) => Some(SimdOp2::Fcvtps),
            (0b11010, true, true) => Some(SimdOp2::Fcvtpu),
            (0b11011, false, false) => Some(SimdOp2::Fcvtms),
            (0b11011, true, false) => Some(SimdOp2::Fcvtmu),
            (0b11100, false, false) => Some(SimdOp2::Fcvtas),
            (0b11100, true, false) => Some(SimdOp2::Fcvtau),
            (0b11100, false, true) if !double => Some(SimdOp2::Urecpe),
            (0b11100, true, true) if !double => Some(SimdOp2::Ursqrte),
            _ => None,
        };
        if let Some(op) = int_op {
            if double && !q {
                return None;
            }
            let esize = if double { 64 } else { 32 };
            return Some(Instruction::SimdTwoMisc { rd, rn, op, esize, q });
        }
        // FCVTZS/FCVTZU vector, integer result: a == 1, opcode 11011
        if (raw >> 12) & 0x1F == 0b11011 && a {
            if double && !q {
                return None;
            }
            let op = if u { SimdOp2::Fcvtzu } else { SimdOp2::Fcvtzs };
            return Some(Instruction::SimdTwoMisc { rd, rn, op, esize: if double { 64 } else { 32 }, q });
        }
    }

    // 29. AdvSIMD two-register miscellaneous: 0 Q U 01110 size 10000 opcode 10 Rn Rd
    if (raw & 0x9F3E_0C00) == 0x0E20_0800 {
        // XTN/XTN2 is SHRN/SHRN2 with a zero shift
        if (raw >> 12) & 0x1F == 0b10010 && !u && size < 3 {
            return Some(Instruction::SimdShiftImm { rd, rn, op: SimdShiftOp::Shrn, esize: 8 << size, shift: 0, q });
        }
        // SHLL/SHLL2: USHLL by the source lane size
        if (raw >> 12) & 0x1F == 0b10011 && u && size < 3 {
            return Some(Instruction::SimdShiftImm { rd, rn, op: SimdShiftOp::Ushll, esize: 8 << size, shift: 8 << size, q });
        }
        let op = match ((raw >> 12) & 0x1F, u) {
            (0b01000, false) => SimdOp2::Cmgt0,
            (0b01000, true) => SimdOp2::Cmge0,
            (0b01001, false) => SimdOp2::Cmeq0,
            (0b01001, true) => SimdOp2::Cmle0,
            (0b01010, false) => SimdOp2::Cmlt0,
            (0b00000, false) if size < 3 => SimdOp2::Rev64,
            (0b00000, true) if size < 2 => SimdOp2::Rev32,
            (0b00001, false) if size == 0 => SimdOp2::Rev16,
            (0b00101, false) if size == 0 => SimdOp2::Cnt,
            (0b00101, true) if size == 0 => SimdOp2::Not,
            (0b01011, false) => SimdOp2::Abs,
            (0b01011, true) => SimdOp2::Neg,
            (0b00011, false) => SimdOp2::Suqadd,
            (0b00011, true) => SimdOp2::Usqadd,
            (0b00100, false) if size < 3 => SimdOp2::Cls,
            (0b00100, true) if size < 3 => SimdOp2::Clz,
            (0b00101, true) if size == 1 => SimdOp2::Rbit,
            (0b00111, false) => SimdOp2::Sqabs,
            (0b00111, true) => SimdOp2::Sqneg,
            (0b11101, false) if size < 2 => SimdOp2::Scvtf,
            (0b11101, true) if size < 2 => SimdOp2::Ucvtf,
            _ => return None,
        };
        // The float forms use bit 22 (sz) alone: 32-bit lanes, or 64-bit lanes with Q. RBIT works on bytes.
        let esize = if matches!(op, SimdOp2::Scvtf | SimdOp2::Ucvtf) {
            32u8 << size
        } else if op == SimdOp2::Rbit {
            8
        } else {
            8u8 << size
        };
        if esize == 64 && !q {
            return None;
        }
        return Some(Instruction::SimdTwoMisc { rd, rn, op, esize, q });
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

    // 30a. AdvSIMD scalar shift by immediate (D registers only): 01 U 111110 1xxx immb opcode 1 Rn Rd
    //      (SSHR/USHR/SHL Dd, Dn, #shift). One 64-bit lane, the rest of Vd zeroed.
    if (raw & 0xDF80_0400) == 0x5F00_0400 && (raw >> 19) & 0xF >= 8 {
        let imm = (raw >> 16) & 0x7F;
        let (op, shift) = match ((raw >> 11) & 0x1F, u) {
            (0b00000, false) => (SimdShiftOp::Sshr, 128 - imm),
            (0b00000, true) => (SimdShiftOp::Ushr, 128 - imm),
            (0b01010, false) => (SimdShiftOp::Shl, imm - 64),
            _ => return None,
        };
        return Some(Instruction::SimdShiftImm { rd, rn, op, esize: 64, shift: shift as u8, q: false });
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
            (0b10100, false) if esize < 64 => (SimdShiftOp::Sshll, esize, imm - esize),
            (0b10100, true) if esize < 64 => (SimdShiftOp::Ushll, esize, imm - esize),
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
        // PRFM / PRFUM / PRFM (register) are prefetch hints, not loads into Rt (Rt is the prefetch operation, often 0)
        assert_eq!(decode(0xF980C160), Instruction::Nop); // prfm pldl1keep, [x11, #384]
        assert_eq!(decode(0xF8A06960), Instruction::Nop); // prfm pldl1keep, [x11, x0]
        assert_eq!(decode(0xF8800160), Instruction::Nop); // prfum pldl1keep, [x11]
        assert_eq!(decode(0xD8000000), Instruction::Nop); // prfm pldl1keep, <literal>
        assert!(matches!(decode(0x4E201BFF), Instruction::SimdTwoMisc { op: SimdOp2::Rev16, esize: 8, q: true, .. })); // rev16 v31.16b, v31.16b
        assert!(matches!(decode(0x6EB3479C), Instruction::SimdThreeSame { op: SimdOp3::Ushl, esize: 32, q: true, .. })); // ushl v28.4s, v28.4s, v19.4s
        assert!(matches!(decode(0x4EB34780), Instruction::SimdThreeSame { op: SimdOp3::Sshl, esize: 32, q: true, .. })); // sshl v0.4s, v28.4s, v19.4s
        assert!(matches!(decode(0x5EFE87FF), Instruction::SimdScalarThreeSame { op: SimdOp3::Add, esize: 64, .. })); // add d31, d31, d30
        assert!(matches!(decode(0x7EE18420), Instruction::SimdScalarThreeSame { op: SimdOp3::Sub, esize: 64, .. })); // sub d0, d1, d1
        assert!(matches!(decode(0x5E1C07EE), Instruction::SimdDupScalar { esize: 32, index: 3, .. })); // mov s14, v31.s[3]
        assert!(matches!(decode(0x0E224020), Instruction::SimdNarrow { op: SimdNarrowOp::Addhn, esize: 8, q: false, .. })); // addhn v0.8b, v1.8h, v2.8h
        assert!(matches!(decode(0x4E226020), Instruction::SimdNarrow { op: SimdNarrowOp::Subhn, esize: 8, q: true, .. })); // subhn2 v0.16b, v1.8h, v2.8h
        assert!(matches!(decode(0x2E624020), Instruction::SimdNarrow { op: SimdNarrowOp::Raddhn, esize: 16, q: false, .. })); // raddhn v0.4h, v1.4s, v2.4s
        assert!(matches!(decode(0x7E7F2FDF), Instruction::SimdScalarThreeSame { op: SimdOp3::Uqsub, esize: 16, .. })); // uqsub h31, h30, h31
        assert!(matches!(decode(0x4E220C20), Instruction::SimdThreeSame { op: SimdOp3::Sqadd, esize: 8, q: true, .. })); // sqadd v0.16b, v1.16b, v2.16b
        assert!(matches!(decode(0x4E012FE3), Instruction::SimdSmov { esize: 8, index: 0, x: true, .. })); // smov x3, v31.b[0]
        assert!(matches!(decode(0x0E62D020), Instruction::SimdSatLong { op: SimdSatLongOp::Sqdmull, esize: 16, q: false, .. })); // sqdmull v0.4s, v1.4h, v2.4h
        assert!(matches!(decode(0x4EA29020), Instruction::SimdSatLong { op: SimdSatLongOp::Sqdmlal, esize: 32, q: true, .. })); // sqdmlal2 v0.2d, v1.4s, v2.4s
        // UMLAL/UMULL/SMLAL family (three different)
        assert!(matches!(decode(0x2EBF837D), Instruction::SimdLong { op: SimdLongOp::Mlal, signed: false, esize: 32, q: false, .. })); // umlal v29.2d, v27.2s, v31.2s
        assert!(matches!(decode(0x6EBF837E), Instruction::SimdLong { op: SimdLongOp::Mlal, signed: false, esize: 32, q: true, .. })); // umlal2 v30.2d, v27.4s, v31.4s
    }

    #[test]
    fn test_decode_fcsel() {
        // fcsel s0, s0, s3, eq (seen in libdeko3d)
        assert_eq!(
            decode(0x1E230C00),
            Instruction::FpCondSelect { rd: 0, rn: 0, rm: 3, cond: Condition::from_u8(0), is_double: false }
        );
    }

    #[test]
    fn decode_fp_pairwise_and_across_minmax() {
        use SimdFpOp3::*;
        // vector pairwise (three-same)
        for (raw, want, double, q) in [
            (0x6E22_F420u32, Fmaxp, false, true),  // fmaxp v0.4s, v1.4s, v2.4s
            (0x6EA2_F420, Fminp, false, true),     // fminp v0.4s, v1.4s, v2.4s
            (0x6E22_C420, Fmaxnmp, false, true),   // fmaxnmp v0.4s, v1.4s, v2.4s
            (0x6EA2_C420, Fminnmp, false, true),   // fminnmp v0.4s, v1.4s, v2.4s
            (0x6E62_F420, Fmaxp, true, true),      // fmaxp v0.2d, v1.2d, v2.2d
            (0x2E22_F420, Fmaxp, false, false),    // fmaxp v0.2s, v1.2s, v2.2s
            (0x4E62_F420, Fmax, true, true),       // fmax v0.2d, v1.2d, v2.2d
        ] {
            match decode(raw) {
                Instruction::SimdFpThreeSame { op, double: d, q: qq, .. } => assert_eq!((op, d, qq), (want, double, q), "{raw:#010x}"),
                other => panic!("{raw:#010x}: {other:?}"),
            }
        }
        // across lanes: fmaxnmv/fminnmv/fmaxv/fminv s0, v1.4s and scalar pairwise Sd/Dd
        for (raw, want, double, lanes) in [
            (0x6E30_C820u32, Fmaxnm, false, 4u8),
            (0x6EB0_C820, Fminnm, false, 4),
            (0x6E30_F820, Fmax, false, 4),
            (0x6EB0_F820, Fmin, false, 4),
            (0x7E30_F820, Fmax, false, 2), // fmaxp s0, v1.2s
            (0x7E70_F820, Fmax, true, 2),  // fmaxp d0, v1.2d
            (0x7EF0_C820, Fminnm, true, 2), // fminnmp d0, v1.2d
            (0x7E30_D820, Faddp, false, 2), // faddp s0, v1.2s
            (0x7E70_D820, Faddp, true, 2),  // faddp d0, v1.2d
        ] {
            match decode(raw) {
                Instruction::SimdFpReduce { op, esize, lanes: l, rd: 0, rn: 1 } => assert_eq!((op, esize == 64, l), (want, double, lanes), "{raw:#010x}"),
                other => panic!("{raw:#010x}: {other:?}"),
            }
        }
        // reserved: .2D across lanes (sz = 1); scalar half pairwise with sz = 1; FP16 three-same with an undecoded opcode
        for raw in [0x6E70_F820u32, 0x5E70_F820, 0x4E42_1420, 0x4E42_3C20] {
            assert!(
                !matches!(decode(raw), Instruction::SimdFpReduce { .. } | Instruction::SimdFpThreeSame { .. } | Instruction::SimdFpThreeSameHalf { .. }),
                "{raw:#010x}"
            );
        }
        // half precision three-same, across lanes, scalar pairwise and scalar min/max
        for (raw, want, q) in [
            (0x4E42_3420u32, Fmax, true),    // fmax v0.8h, v1.8h, v2.8h
            (0x4EC2_3420, Fmin, true),       // fmin v0.8h, v1.8h, v2.8h
            (0x4E42_0420, Fmaxnm, true),     // fmaxnm v0.8h, v1.8h, v2.8h
            (0x4EC2_0420, Fminnm, true),     // fminnm v0.8h, v1.8h, v2.8h
            (0x6E42_3420, Fmaxp, true),      // fmaxp v0.8h, v1.8h, v2.8h
            (0x6EC2_3420, Fminp, true),      // fminp v0.8h, v1.8h, v2.8h
            (0x6E42_0420, Fmaxnmp, true),    // fmaxnmp v0.8h, v1.8h, v2.8h
            (0x6EC2_0420, Fminnmp, true),    // fminnmp v0.8h, v1.8h, v2.8h
            (0x0E42_3420, Fmax, false),      // fmax v0.4h, v1.4h, v2.4h
        ] {
            match decode(raw) {
                Instruction::SimdFpThreeSameHalf { op, q: qq, rd: 0, rn: 1, rm: 2 } => assert_eq!((op, qq), (want, q), "{raw:#010x}"),
                other => panic!("{raw:#010x}: {other:?}"),
            }
        }
        for (raw, want, lanes) in [
            (0x4E30_C820u32, Fmaxnm, 8u8), // fmaxnmv h0, v1.8h
            (0x0EB0_C820, Fminnm, 4),      // fminnmv h0, v1.4h
            (0x4E30_F820, Fmax, 8),        // fmaxv h0, v1.8h
            (0x0EB0_F820, Fmin, 4),        // fminv h0, v1.4h
            (0x5E30_F820, Fmax, 2),        // fmaxp h0, v1.2h
            (0x5EB0_C820, Fminnm, 2),      // fminnmp h0, v1.2h
        ] {
            match decode(raw) {
                Instruction::SimdFpReduce { op, esize: 16, lanes: l, rd: 0, rn: 1 } => assert_eq!((op, l), (want, lanes), "{raw:#010x}"),
                other => panic!("{raw:#010x}: {other:?}"),
            }
        }
        for (raw, want) in [(0x1EE2_4820u32, FpOp2::Fmax), (0x1EE2_5820, FpOp2::Fmin), (0x1EE2_6820, FpOp2::Fmaxnm), (0x1EE2_7820, FpOp2::Fminnm)] {
            match decode(raw) {
                Instruction::FpHalfMinMax { op, rd: 0, rn: 1, rm: 2 } => assert_eq!(op, want, "{raw:#010x}"),
                other => panic!("{raw:#010x}: {other:?}"),
            }
        }
    }

    #[test]
    fn test_decode_fp_scalar_forms_from_deko3d() {
        assert!(matches!(decode(0x1E204050), Instruction::FpData1 { rd: 16, rn: 2, op: 0, is_double: false })); // fmov s16, s2
        assert!(matches!(decode(0x1E3100A7), Instruction::FpIntConv { rd: 7, rn: 5, opcode: 1, rmode: 2, is_64: false, .. })); // fcvtmu w7, s5
        assert!(matches!(decode(0x1E24C021), Instruction::FpRound { rd: 1, rn: 1, op: 1, is_double: false })); // frintp s1, s1
        assert!(matches!(decode(0x1E2020F8), Instruction::FpCompare { rn: 7, is_zero: true, .. })); // fcmpe s7, #0.0
        assert!(matches!(decode(0x0E31B800), Instruction::SimdAcross { op: SimdAcrossOp::Addv, esize: 8, q: false, .. })); // addv b0, v0.8b
    }

    #[test]
    fn test_decode_fabd_scalar() {
        assert_eq!(
            decode(0x7EFCD7DE),
            Instruction::FpAbd { rd: 30, rn: 30, rm: 28, is_double: true }
        ); // fabd d30, d30, d28 (from cJSON_Compare)
        assert_eq!(
            decode(0x7EA2D420),
            Instruction::FpAbd { rd: 0, rn: 1, rm: 2, is_double: false }
        ); // fabd s0, s1, s2
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

    #[test]
    fn test_decode_pmull() {
        assert_eq!(decode(0x0E22E020), Instruction::Pmull { rd: 0, rn: 1, rm: 2, wide: false, q: false }); // pmull v0.8h, v1.8b, v2.8b
        assert_eq!(decode(0x4E22E020), Instruction::Pmull { rd: 0, rn: 1, rm: 2, wide: false, q: true }); // pmull2 v0.8h, v1.16b, v2.16b
        assert_eq!(decode(0x0EE2E020), Instruction::Pmull { rd: 0, rn: 1, rm: 2, wide: true, q: false }); // pmull v0.1q, v1.1d, v2.1d
        assert_eq!(decode(0x4EE2E020), Instruction::Pmull { rd: 0, rn: 1, rm: 2, wide: true, q: true }); // pmull2 v0.1q, v1.2d, v2.2d
        assert!(!matches!(decode(0x0E62E020), Instruction::Pmull { .. })); // size 1 reserved
        assert!(!matches!(decode(0x2E22E020), Instruction::Pmull { .. })); // U=1 is not PMULL
    }
}

/// Integer vector forms and BRK that the main decoder tables do not cover: `SQXTN`/`UQXTN`/`SQXTUN`,
/// `RSHRN`, `SSRA`/`USRA`, `SRSHR`/`URSHR`, `SRSRA`/`URSRA`, `SRI`/`SLI`, `SQSHL`/`UQSHL`/`SQSHLU` (immediate),
/// `SQSHRN` family. Called only after the main tables returned nothing, so it cannot shadow them.
fn decode_intvec(raw: u32) -> Option<Instruction> {
    let rd = (raw & 0x1F) as u8;
    let rn = ((raw >> 5) & 0x1F) as u8;
    let q = (raw & (1 << 30)) != 0;
    let u = (raw & (1 << 29)) != 0;

    // BRK #imm16: 1101 0100 001 imm16 00000
    if (raw & 0xFFE0_001F) == 0xD420_0000 {
        return Some(Instruction::Brk { imm: ((raw >> 5) & 0xFFFF) as u16 });
    }

    // ADDP Dd, Vn.2D (scalar pairwise add, what vaddvq_u64 compiles to): 01 0 11110 11 11000 11011 10 Rn Rd
    if (raw & 0xFFFF_FC00) == 0x5EF1_B800 {
        return Some(Instruction::SimdAcross { rd, rn, op: SimdAcrossOp::Addv, esize: 64, q: true });
    }

    let size = (raw >> 22) & 3;
    // SADDLP/UADDLP/SADALP/UADALP: 0 Q U 01110 size 10000 00010/00110 10 Rn Rd (size < 3)
    if (raw & 0x9F3F_BC00) == 0x0E20_2800 && size < 3 {
        let acc = (raw >> 14) & 1 != 0;
        return Some(Instruction::SimdPairwiseLong { rd, rn, signed: !u, acc, esize: 8u8 << size, q });
    }

    // Two-register misc narrowing: [0 Q | 01] U 01110 size 10000 opcode 10 Rn Rd (vector / scalar)
    for (mask, want, scalar) in [(0x9F3E_0C00u32, 0x0E20_0800u32, false), (0xDF3E_0C00, 0x5E20_0800, true)] {
        if (raw & mask) == want {
            let size = (raw >> 22) & 3;
            let op = match ((raw >> 12) & 0x1F, u) {
                (0b10010, true) => SimdShiftExtOp::Sqshrun,
                (0b10100, false) => SimdShiftExtOp::Sqshrn,
                (0b10100, true) => SimdShiftExtOp::Uqshrn,
                _ => return None,
            };
            if size == 3 {
                return None;
            }
            return Some(Instruction::SimdQNarrow { rd, rn, op, esize: 8u8 << size, q: q && !scalar, scalar });
        }
    }

    // Shift by immediate: [0 Q | 01] U 011110 / 111110 immh immb opcode 1 Rn Rd
    for (mask, want, scalar) in [(0x9F80_0400u32, 0x0F00_0400u32, false), (0xDF80_0400, 0x5F00_0400, true)] {
        if (raw & mask) != want {
            continue;
        }
        let immh = (raw >> 19) & 0xF;
        if immh == 0 {
            return None;
        }
        let imm = (raw >> 16) & 0x7F;
        let src = 8u32 << (31 - immh.leading_zeros());
        let right = 2 * src - imm;
        let left = imm - src;
        let opc = (raw >> 11) & 0x1F;
        // narrowing shifts: immh gives the RESULT lane size, source lanes are twice as wide
        let narrow = |op| (op, src, right);
        let (op, esize, shift) = match (opc, u) {
            (0b00010, false) => (SimdShiftExtOp::Ssra, src, right),
            (0b00010, true) => (SimdShiftExtOp::Usra, src, right),
            (0b00100, false) => (SimdShiftExtOp::Srshr, src, right),
            (0b00100, true) => (SimdShiftExtOp::Urshr, src, right),
            (0b00110, false) => (SimdShiftExtOp::Srsra, src, right),
            (0b00110, true) => (SimdShiftExtOp::Ursra, src, right),
            (0b01000, true) => (SimdShiftExtOp::Sri, src, right),
            (0b01010, true) => (SimdShiftExtOp::Sli, src, left),
            (0b01100, true) => (SimdShiftExtOp::Sqshlu, src, left),
            (0b01110, false) => (SimdShiftExtOp::Sqshl, src, left),
            (0b01110, true) => (SimdShiftExtOp::Uqshl, src, left),
            (0b10000, true) | (0b10001, _) | (0b10010, _) | (0b10011, _) if src == 64 => return None,
            (0b10000, true) => narrow(SimdShiftExtOp::Sqshrun),
            (0b10001, false) => narrow(SimdShiftExtOp::Rshrn),
            (0b10001, true) => narrow(SimdShiftExtOp::Sqrshrun),
            (0b10010, false) => narrow(SimdShiftExtOp::Sqshrn),
            (0b10010, true) => narrow(SimdShiftExtOp::Uqshrn),
            (0b10011, false) => narrow(SimdShiftExtOp::Sqrshrn),
            (0b10011, true) => narrow(SimdShiftExtOp::Uqrshrn),
            _ => return None,
        };
        let is_narrow = matches!(
            op,
            SimdShiftExtOp::Rshrn | SimdShiftExtOp::Sqshrn | SimdShiftExtOp::Uqshrn | SimdShiftExtOp::Sqshrun
                | SimdShiftExtOp::Sqrshrn | SimdShiftExtOp::Uqrshrn | SimdShiftExtOp::Sqrshrun
        );
        // Scalar forms: only SQSHL/UQSHL/SQSHLU and the narrowing shifts take B/H/S lanes; the rest are D only.
        // RSHRN has no scalar form. Vector 1D (esize 64 without Q) does not exist.
        let any_size = is_narrow || matches!(op, SimdShiftExtOp::Sqshl | SimdShiftExtOp::Uqshl | SimdShiftExtOp::Sqshlu);
        if scalar && ((!any_size && esize != 64) || op == SimdShiftExtOp::Rshrn) {
            return None;
        }
        if !scalar && esize == 64 && !q && !is_narrow {
            return None;
        }
        return Some(Instruction::SimdShiftExt { rd, rn, op, esize: esize as u8, shift: shift as u8, q: q && !scalar, scalar });
    }
    None
}

#[cfg(test)]
mod fuzz_half_precision {
    use super::*;

    /// Found by the instruction fuzzer: scalar FP words with ftype 3 (half precision) were decoded as
    /// single precision and gave wrong results. Half precision is not implemented, so they must be
    /// Unimplemented (every word below ran fine on Apple Silicon).
    #[test]
    fn half_precision_scalar_words_are_not_decoded_as_single() {
        for w in [
            0x1ef1_4f22u32, // fcsel h2, h25, h17, vs
            0x1ee2_03a6,    // fcvt s6, h29 (ftype 3 one-source)
            0x9ee0_00b4,    // fcvtns x20, h5
            0x1efd_21e8,    // fcmp h15, h29
            0x1ee4_c16a,    // fneg/fabs class, half
            0x1ef0_ef62,    // fcsel h2, h27, h16, al
        ] {
            assert!(matches!(decode(w), Instruction::Unimplemented { .. }), "{w:#010x} {:?}", decode(w));
        }
        // single and double still decode
        assert!(matches!(decode(0x1e22_0c20), Instruction::FpCondSelect { is_double: false, .. })); // fcsel s0, s1, s2, eq
        assert!(matches!(decode(0x1e62_0c20), Instruction::FpCondSelect { is_double: true, .. })); // fcsel d0, d1, d2, eq
        assert!(matches!(decode(0x1e22_2020), Instruction::FpCompare { is_double: false, .. })); // fcmp s1, s2
        assert!(matches!(decode(0x1e21_c020), Instruction::FpData1 { is_double: false, .. })); // fsqrt s0, s1
    }
}

#[cfg(test)]
mod fuzz_fixed_bits {
    use super::*;

    /// Found by the instruction fuzzer: the host raises SIGILL for each of these words but the decoder
    /// accepted them because it ignored M/S (bits 31, 29), FCMP's op field (15:14), the rmode of
    /// SCVTF/UCVTF/FCVTA*, and the sf/type pairing of FMOV between general and FP registers.
    #[test]
    fn undefined_scalar_fp_encodings_are_rejected() {
        for w in [
            0x9e2c_4b32u32, // fmax with sf = 1
            0x9e7a_1870,    // fdiv d with sf = 1
            0x1e38_a300,    // fcmp-like with op bits 10
            0x9e62_a240,    // fcmp-like, sf = 1
            0x1e6b_0368,    // ucvtf-like with rmode 1
            0x1e3a_0164,    // scvtf-like with rmode 3
            0x9e26_0004,    // fmov x4, s0 (sf = 1 needs type 01)
            0x1e66_0004,    // fmov w4, d0 (type 01 needs sf = 1)
        ] {
            assert!(matches!(decode(w), Instruction::Unimplemented { .. }), "{w:#010x} {:?}", decode(w));
        }
        assert!(matches!(decode(0x1e22_2820), Instruction::FpData2 { op: FpOp2::Fadd, .. })); // fadd s0, s1, s2
        assert!(matches!(decode(0x1e22_2020), Instruction::FpCompare { .. })); // fcmp s1, s2
        assert!(matches!(decode(0x1e22_2028), Instruction::FpCompare { is_zero: true, .. })); // fcmp s1, #0.0
        assert!(matches!(decode(0x9e62_0020), Instruction::FpIntConv { opcode: 2, .. })); // scvtf d0, x1
        assert!(matches!(decode(0x9e64_0020), Instruction::FpIntConv { opcode: 4, .. })); // fcvtas x0, d1
        assert!(matches!(decode(0x1e79_0020), Instruction::FpIntConv { rmode: 3, .. })); // fcvtzs w0, d1
        assert!(matches!(decode(0x9e66_0020), Instruction::FpMovGen { is_64: true, to_fp: false, .. })); // fmov x0, d1
        assert!(matches!(decode(0x1e26_0020), Instruction::FpMovGen { is_64: false, to_fp: false, .. })); // fmov w0, s1
        assert!(matches!(decode(0x9e67_0020), Instruction::FpMovGen { is_64: true, to_fp: true, .. })); // fmov d0, x1
    }
}
