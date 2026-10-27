#![allow(dead_code)]

use super::instructions::{
    BranchRegOp, Condition, DataProc1Op, FpOp2, Instruction, LogicalOp, MoveWideOp, ShiftType, SimdImmOp, SimdShiftOp,
};
use super::simd;

/// Bytes zeroed by `DC ZVA`, as reported by `DCZID_EL0`.
pub const DC_ZVA_BLOCK: u64 = 64;
use super::jit_cache::JitBlockCache;
use super::registers::CpuRegisters;
use crate::mmu::{Memory, MemoryError};

#[derive(Debug, thiserror::Error)]
pub enum CpuError {
    #[error("Memory access violation: {0}")]
    Memory(#[from] MemoryError),

    #[error("Unimplemented instruction 0x{raw:08X} at PC: 0x{pc:016X}")]
    UnimplementedInstruction { raw: u32, pc: u64 },

    #[error("CPU halted: {0}")]
    Halted(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepResult {
    /// Normal instruction completed
    Continue,
    /// Horizon OS Supervisor Call triggered (SVC #imm)
    Svc(u16),
    /// Process exited or CPU halted
    Halt(String),
}

/// AArch64 Execution Engine with Basic-Block JIT Instruction Cache.
pub struct Cpu {
    pub regs: CpuRegisters,
    pub jit: JitBlockCache,
    /// Exclusive monitor: the address the last load-exclusive tagged. A store-exclusive succeeds only
    /// while it is still set for that address; switching threads clears it, so an interrupted
    /// LDXR/STXR sequence retries as on hardware.
    pub exclusive: Option<u64>,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            regs: CpuRegisters::new(),
            jit: JitBlockCache::new(),
            exclusive: None,
        }
    }

    /// Fetches, basic-block JIT-compiles/looks up, and executes a single AArch64 instruction.
    pub fn step(&mut self, mmu: &mut Memory) -> Result<StepResult, CpuError> {
        let pc = self.regs.pc;
        let raw_inst = mmu.read_u32(pc)?;
        let inst = self.jit.fetch_or_compile(pc, raw_inst, mmu);

        match inst {
            Instruction::Nop => {
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Svc { imm } => {
                // Return control to Horizon kernel dispatcher
                self.regs.advance_pc();
                Ok(StepResult::Svc(imm))
            }

            Instruction::Adr { rd, imm, is_page } => {
                let base = if is_page { pc & !0xFFF } else { pc };
                let target = (base as i64).wrapping_add(imm) as u64;
                self.regs.write_x(rd, target);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::MoveWide {
                rd,
                imm,
                shift,
                op,
                is_64,
            } => {
                let shifted = (imm as u64) << shift;
                let val = match op {
                    MoveWideOp::Movz => shifted,
                    MoveWideOp::Movn => {
                        let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
                        (!shifted) & mask
                    }
                    MoveWideOp::Movk => {
                        let cur = if is_64 {
                            self.regs.read_x(rd)
                        } else {
                            self.regs.read_w(rd) as u64
                        };
                        let mask = !((0xFFFF as u64) << shift);
                        (cur & mask) | shifted
                    }
                };

                if is_64 {
                    self.regs.write_x(rd, val);
                } else {
                    self.regs.write_w(rd, val as u32);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::AddSubImm {
                rd,
                rn,
                imm,
                is_sub,
                set_flags,
                is_64,
            } => {
                // If setting flags or rd != 31, read Rn (with SP support if not setting flags)
                let op1 = if !set_flags && rn == 31 {
                    self.regs.sp
                } else {
                    self.regs.read_x(rn)
                };

                let op2 = imm;

                if is_64 {
                    let (res, c, v) = if is_sub {
                        let (res, borrow) = op1.overflowing_sub(op2);
                        let v = ((op1 ^ op2) & (op1 ^ res) & (1 << 63)) != 0;
                        (res, !borrow, v)
                    } else {
                        let (res, carry) = op1.overflowing_add(op2);
                        let v = (!((op1 ^ op2)) & (op1 ^ res) & (1 << 63)) != 0;
                        (res, carry, v)
                    };

                    if set_flags {
                        self.regs.pstate.n = (res as i64) < 0;
                        self.regs.pstate.z = res == 0;
                        self.regs.pstate.c = c;
                        self.regs.pstate.v = v;
                        self.regs.write_x(rd, res);
                    } else {
                        self.regs.write_x_or_sp(rd, res);
                    }
                } else {
                    let op1_32 = op1 as u32;
                    let op2_32 = op2 as u32;

                    let (res, c, v) = if is_sub {
                        let (res, borrow) = op1_32.overflowing_sub(op2_32);
                        let v = ((op1_32 ^ op2_32) & (op1_32 ^ res) & (1 << 31)) != 0;
                        (res, !borrow, v)
                    } else {
                        let (res, carry) = op1_32.overflowing_add(op2_32);
                        let v = (!((op1_32 ^ op2_32)) & (op1_32 ^ res) & (1 << 31)) != 0;
                        (res, carry, v)
                    };

                    if set_flags {
                        self.regs.pstate.n = (res as i32) < 0;
                        self.regs.pstate.z = res == 0;
                        self.regs.pstate.c = c;
                        self.regs.pstate.v = v;
                        self.regs.write_w(rd, res);
                    } else {
                        self.regs.write_x_or_sp(rd, res as u64);
                    }
                }

                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::AddSubReg {
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
                let op1 = if is_extended && rn == 31 {
                    self.regs.sp
                } else {
                    self.regs.read_x(rn)
                };
                let raw_rm = self.regs.read_x(rm);
                let op2 = if is_extended {
                    let ext: u64 = match extend_option {
                        0b000 => (raw_rm as u8) as u64,                  // UXTB
                        0b001 => (raw_rm as u16) as u64,                 // UXTH
                        0b010 => (raw_rm as u32) as u64,                 // UXTW
                        0b011 => raw_rm,                                 // UXTX
                        0b100 => ((raw_rm as i8) as i64) as u64,         // SXTB
                        0b101 => ((raw_rm as i16) as i64) as u64,        // SXTH
                        0b110 => ((raw_rm as i32) as i64) as u64,        // SXTW
                        _ => raw_rm,                                     // SXTX
                    };
                    ext.wrapping_shl((shift_amount & 7) as u32)
                } else if shift_amount == 0 {
                    raw_rm
                } else if is_64 {
                    let sh = (shift_amount & 63) as u32;
                    match shift_type {
                        ShiftType::Lsl => raw_rm.wrapping_shl(sh),
                        ShiftType::Lsr => raw_rm.wrapping_shr(sh),
                        ShiftType::Asr => ((raw_rm as i64).wrapping_shr(sh)) as u64,
                        ShiftType::Ror => raw_rm.rotate_right(sh),
                    }
                } else {
                    let sh = (shift_amount & 31) as u32;
                    let rm32 = raw_rm as u32;
                    let shifted32 = match shift_type {
                        ShiftType::Lsl => rm32.wrapping_shl(sh),
                        ShiftType::Lsr => rm32.wrapping_shr(sh),
                        ShiftType::Asr => ((rm32 as i32).wrapping_shr(sh)) as u32,
                        ShiftType::Ror => rm32.rotate_right(sh),
                    };
                    shifted32 as u64
                };

                if is_64 {
                    let (res, c, v) = if is_sub {
                        let (res, borrow) = op1.overflowing_sub(op2);
                        let v = ((op1 ^ op2) & (op1 ^ res) & (1 << 63)) != 0;
                        (res, !borrow, v)
                    } else {
                        let (res, carry) = op1.overflowing_add(op2);
                        let v = (!((op1 ^ op2)) & (op1 ^ res) & (1 << 63)) != 0;
                        (res, carry, v)
                    };

                    if set_flags {
                        self.regs.pstate.n = (res as i64) < 0;
                        self.regs.pstate.z = res == 0;
                        self.regs.pstate.c = c;
                        self.regs.pstate.v = v;
                    }
                    // Extended-register ADD/SUB (not ADDS/SUBS) writes SP for Rd = 31, e.g. `sub sp, sp, x12`.
                    if is_extended && !set_flags && rd == 31 {
                        self.regs.sp = res;
                    } else {
                        self.regs.write_x(rd, res);
                    }
                } else {
                    let op1_32 = op1 as u32;
                    let op2_32 = op2 as u32;

                    let (res, c, v) = if is_sub {
                        let (res, borrow) = op1_32.overflowing_sub(op2_32);
                        let v = ((op1_32 ^ op2_32) & (op1_32 ^ res) & (1 << 31)) != 0;
                        (res, !borrow, v)
                    } else {
                        let (res, carry) = op1_32.overflowing_add(op2_32);
                        let v = (!((op1_32 ^ op2_32)) & (op1_32 ^ res) & (1 << 31)) != 0;
                        (res, carry, v)
                    };

                    if set_flags {
                        self.regs.pstate.n = (res as i32) < 0;
                        self.regs.pstate.z = res == 0;
                        self.regs.pstate.c = c;
                        self.regs.pstate.v = v;
                    }
                    // WSP for Rd = 31 in the extended form, zero-extended like any W write.
                    if is_extended && !set_flags && rd == 31 {
                        self.regs.sp = res as u64;
                    } else {
                        self.regs.write_w(rd, res);
                    }
                }

                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::LogicalReg {
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
                let op1 = self.regs.read_x(rn);
                let raw_rm = self.regs.read_x(rm);
                let shifted = if shift_amount == 0 {
                    raw_rm
                } else if is_64 {
                    let sh = (shift_amount & 63) as u32;
                    match shift_type {
                        ShiftType::Lsl => raw_rm.wrapping_shl(sh),
                        ShiftType::Lsr => raw_rm.wrapping_shr(sh),
                        ShiftType::Asr => ((raw_rm as i64).wrapping_shr(sh)) as u64,
                        ShiftType::Ror => raw_rm.rotate_right(sh),
                    }
                } else {
                    let sh = (shift_amount & 31) as u32;
                    let rm32 = raw_rm as u32;
                    let shifted32 = match shift_type {
                        ShiftType::Lsl => rm32.wrapping_shl(sh),
                        ShiftType::Lsr => rm32.wrapping_shr(sh),
                        ShiftType::Asr => ((rm32 as i32).wrapping_shr(sh)) as u32,
                        ShiftType::Ror => rm32.rotate_right(sh),
                    };
                    shifted32 as u64
                };
                let op2 = if invert_rm { !shifted } else { shifted };

                let res = match op {
                    LogicalOp::And | LogicalOp::Ands => op1 & op2,
                    LogicalOp::Orr => op1 | op2,
                    LogicalOp::Eor => op1 ^ op2,
                };

                if is_64 {
                    self.regs.write_x(rd, res);
                    if set_flags {
                        self.regs.pstate.n = (res as i64) < 0;
                        self.regs.pstate.z = res == 0;
                        self.regs.pstate.c = false;
                        self.regs.pstate.v = false;
                    }
                } else {
                    let res32 = res as u32;
                    self.regs.write_w(rd, res32);
                    if set_flags {
                        self.regs.pstate.n = (res32 as i32) < 0;
                        self.regs.pstate.z = res32 == 0;
                        self.regs.pstate.c = false;
                        self.regs.pstate.v = false;
                    }
                }

                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::LogicalImm {
                rd,
                rn,
                imm,
                op,
                set_flags,
                is_64,
            } => {
                let op1 = self.regs.read_x(rn);
                let res = match op {
                    LogicalOp::And | LogicalOp::Ands => op1 & imm,
                    LogicalOp::Orr => op1 | imm,
                    LogicalOp::Eor => op1 ^ imm,
                };
                if is_64 {
                    if set_flags {
                        self.regs.pstate.n = (res as i64) < 0;
                        self.regs.pstate.z = res == 0;
                        self.regs.pstate.c = false;
                        self.regs.pstate.v = false;
                        self.regs.write_x(rd, res);
                    } else {
                        self.regs.write_x_or_sp(rd, res);
                    }
                } else {
                    let res32 = res as u32;
                    if set_flags {
                        self.regs.pstate.n = (res32 as i32) < 0;
                        self.regs.pstate.z = res32 == 0;
                        self.regs.pstate.c = false;
                        self.regs.pstate.v = false;
                        self.regs.write_w(rd, res32);
                    } else {
                        self.regs.write_x_or_sp(rd, res32 as u64);
                    }
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Bitfield {
                rd,
                rn,
                immr,
                imms,
                opc,
                is_64,
            } => {
                let src = if is_64 {
                    self.regs.read_x(rn)
                } else {
                    self.regs.read_w(rn) as u64
                };
                let dst = if is_64 {
                    self.regs.read_x(rd)
                } else {
                    self.regs.read_w(rd) as u64
                };
                let bits: u8 = if is_64 { 64 } else { 32 };
                let r = immr % bits;
                let s = imms % bits;

                let res = if s >= r {
                    // Extract bits [s:r] -> [s-r:0]
                    let width = (s - r + 1) as u32;
                    let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
                    let extracted = (src >> r) & mask;
                    match opc {
                        0 => {
                            // SBFM: sign-extend from bit (width - 1)
                            let shift = 64 - width;
                            (((extracted << shift) as i64) >> shift) as u64
                        }
                        1 => {
                            // BFM: bitfield extract and insert into low bits of Rd
                            (dst & !mask) | extracted
                        }
                        _ => extracted, // UBFM
                    }
                } else {
                    // Left shift / insert: shift left by (bits - r), width = s + 1
                    let lshift = (bits - r) as u32;
                    let width = (s + 1) as u32;
                    let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
                    let extracted = src & mask;
                    let positioned = extracted.wrapping_shl(lshift);
                    match opc {
                        0 => {
                            // SBFIZ
                            let sign_bit = (extracted >> s) & 1;
                            if sign_bit != 0 {
                               let top_shift = lshift + width;
                               let top_mask = if top_shift >= 64 { 0 } else { u64::MAX << top_shift };
                               positioned | top_mask
                            } else {
                               positioned
                            }
                        }
                        1 => {
                            // BFI: insert into dst at [lshift + width - 1 : lshift]
                            let dst_mask = mask.wrapping_shl(lshift);
                            (dst & !dst_mask) | positioned
                        }
                        _ => positioned, // UBFIZ / LSL #imm
                    }
                };

                if is_64 {
                    self.regs.write_x(rd, res);
                } else {
                    self.regs.write_w(rd, res as u32);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Extract {
                rd,
                rn,
                rm,
                lsb,
                is_64,
            } => {
                if is_64 {
                    let hi = self.regs.read_x(rn) as u128;
                    let lo = self.regs.read_x(rm) as u128;
                    let combined = (hi << 64) | lo;
                    let res = ((combined >> (lsb & 63)) & 0xFFFF_FFFF_FFFF_FFFF) as u64;
                    self.regs.write_x(rd, res);
                } else {
                    let hi = (self.regs.read_w(rn) as u64) << 32;
                    let lo = self.regs.read_w(rm) as u64;
                    let res = ((hi | lo) >> (lsb & 31)) as u32;
                    self.regs.write_w(rd, res);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::CondSelect {
                rd,
                rn,
                rm,
                cond,
                op,
                is_64,
            } => {
                let res = if self.check_condition(cond) {
                    self.regs.read_x(rn)
                } else {
                    let m = self.regs.read_x(rm);
                    match op {
                        0 => m,                   // CSEL
                        1 => m.wrapping_add(1),   // CSINC
                        2 => !m,                  // CSINV
                        _ => m.wrapping_neg(),    // CSNEG
                    }
                };
                if is_64 {
                    self.regs.write_x(rd, res);
                } else {
                    self.regs.write_w(rd, res as u32);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::CondCompare {
                rn,
                rm_or_imm,
                nzcv,
                cond,
                is_imm,
                is_sub,
                is_64,
            } => {
                if self.check_condition(cond) {
                    let op1 = self.regs.read_x(rn);
                    let op2 = if is_imm {
                        rm_or_imm as u64
                    } else {
                        self.regs.read_x(rm_or_imm)
                    };
                    if is_64 {
                        let (res, c, v) = if is_sub {
                            let (r, borrow) = op1.overflowing_sub(op2);
                            let v = ((op1 ^ op2) & (op1 ^ r) & (1 << 63)) != 0;
                            (r, !borrow, v)
                        } else {
                            let (r, carry) = op1.overflowing_add(op2);
                            let v = (!((op1 ^ op2)) & (op1 ^ r) & (1 << 63)) != 0;
                            (r, carry, v)
                        };
                        self.regs.pstate.n = (res as i64) < 0;
                        self.regs.pstate.z = res == 0;
                        self.regs.pstate.c = c;
                        self.regs.pstate.v = v;
                    } else {
                        let op1_32 = op1 as u32;
                        let op2_32 = op2 as u32;
                        let (res, c, v) = if is_sub {
                            let (r, borrow) = op1_32.overflowing_sub(op2_32);
                            let v = ((op1_32 ^ op2_32) & (op1_32 ^ r) & (1 << 31)) != 0;
                            (r, !borrow, v)
                        } else {
                            let (r, carry) = op1_32.overflowing_add(op2_32);
                            let v = (!((op1_32 ^ op2_32)) & (op1_32 ^ r) & (1 << 31)) != 0;
                            (r, carry, v)
                        };
                        self.regs.pstate.n = (res as i32) < 0;
                        self.regs.pstate.z = res == 0;
                        self.regs.pstate.c = c;
                        self.regs.pstate.v = v;
                    }
                } else {
                    self.regs.pstate.n = (nzcv & 0b1000) != 0;
                    self.regs.pstate.z = (nzcv & 0b0100) != 0;
                    self.regs.pstate.c = (nzcv & 0b0010) != 0;
                    self.regs.pstate.v = (nzcv & 0b0001) != 0;
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::TestBranch {
                rt,
                bit,
                offset,
                is_tbnz,
            } => {
                let val = self.regs.read_x(rt);
                let bit_set = ((val >> (bit & 63)) & 1) != 0;
                if bit_set == is_tbnz {
                    self.regs.pc = (pc as i64).wrapping_add(offset) as u64;
                } else {
                    self.regs.advance_pc();
                }
                Ok(StepResult::Continue)
            }

            Instruction::LoadStoreReg {
                rt,
                rn,
                rm,
                option,
                shift,
                size_bytes,
                is_load,
                is_signed,
                is_64_target,
            } => {
                let base = self.regs.read_x_or_sp(rn);
                let raw_m = self.regs.read_x(rm);
                let index: u64 = match option {
                    0b010 => (raw_m as u32) as u64,               // UXTW
                    0b110 => ((raw_m as i32) as i64) as u64,      // SXTW
                    0b111 => raw_m,                               // SXTX
                    _ => raw_m,                                   // LSL / UXTX
                };
                let eff_addr = base.wrapping_add(index.wrapping_shl(shift as u32));

                if is_load {
                    let val = match (size_bytes, is_signed) {
                        (1, false) => mmu.read_u8(eff_addr)? as u64,
                        (1, true) => ((mmu.read_u8(eff_addr)? as i8) as i64) as u64,
                        (2, false) => mmu.read_u16(eff_addr)? as u64,
                        (2, true) => ((mmu.read_u16(eff_addr)? as i16) as i64) as u64,
                        (4, false) => mmu.read_u32(eff_addr)? as u64,
                        (4, true) => ((mmu.read_u32(eff_addr)? as i32) as i64) as u64,
                        (8, _) => mmu.read_u64(eff_addr)?,
                        _ => unreachable!(),
                    };
                    if is_64_target || size_bytes == 8 {
                        self.regs.write_x(rt, val);
                    } else {
                        self.regs.write_w(rt, val as u32);
                    }
                } else {
                    match size_bytes {
                        1 => mmu.write_u8(eff_addr, self.regs.read_w(rt) as u8)?,
                        2 => mmu.write_u16(eff_addr, self.regs.read_w(rt) as u16)?,
                        4 => mmu.write_u32(eff_addr, self.regs.read_w(rt))?,
                        8 => mmu.write_u64(eff_addr, self.regs.read_x(rt))?,
                        _ => unreachable!(),
                    }
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::BranchImm { offset, link } => {
                if link {
                    self.regs.write_x(30, pc.wrapping_add(4));
                }
                self.regs.pc = (pc as i64).wrapping_add(offset) as u64;
                Ok(StepResult::Continue)
            }

            Instruction::BranchCond { offset, cond } => {
                if self.check_condition(cond) {
                    self.regs.pc = (pc as i64).wrapping_add(offset) as u64;
                } else {
                    self.regs.advance_pc();
                }
                Ok(StepResult::Continue)
            }

            Instruction::BranchReg { rn, op } => {
                let target = self.regs.read_x(rn);
                match op {
                    BranchRegOp::Br => {
                        self.regs.pc = target;
                    }
                    BranchRegOp::Blr => {
                        self.regs.write_x(30, pc.wrapping_add(4));
                        self.regs.pc = target;
                    }
                    BranchRegOp::Ret => {
                        self.regs.pc = target;
                    }
                }
                Ok(StepResult::Continue)
            }

            Instruction::CompareBranch {
                rt,
                offset,
                is_cbnz,
                is_64,
            } => {
                let val = if is_64 {
                    self.regs.read_x(rt)
                } else {
                    self.regs.read_w(rt) as u64
                };

                let condition_met = if is_cbnz { val != 0 } else { val == 0 };

                if condition_met {
                    self.regs.pc = (pc as i64).wrapping_add(offset) as u64;
                } else {
                    self.regs.advance_pc();
                }
                Ok(StepResult::Continue)
            }

            Instruction::LoadStoreImm {
                rt,
                rn,
                offset,
                size_bytes,
                is_load,
                is_signed,
                writeback,
                post_index,
            } => {
                let base = self.regs.read_x_or_sp(rn);
                let eff_addr = if post_index {
                    base
                } else {
                    (base as i64).wrapping_add(offset) as u64
                };

                if is_load {
                    match (size_bytes, is_signed) {
                        (1, false) => self.regs.write_x(rt, mmu.read_u8(eff_addr)? as u64),
                        (1, true) => self.regs.write_x(rt, (mmu.read_u8(eff_addr)? as i8 as i64) as u64),
                        (2, false) => self.regs.write_x(rt, mmu.read_u16(eff_addr)? as u64),
                        (2, true) => self.regs.write_x(rt, (mmu.read_u16(eff_addr)? as i16 as i64) as u64),
                        (4, false) => self.regs.write_w(rt, mmu.read_u32(eff_addr)?),
                        (4, true) => self.regs.write_x(rt, (mmu.read_u32(eff_addr)? as i32 as i64) as u64),
                        (8, _) => self.regs.write_x(rt, mmu.read_u64(eff_addr)?),
                        _ => unreachable!(),
                    }
                } else {
                    match size_bytes {
                        1 => mmu.write_u8(eff_addr, self.regs.read_w(rt) as u8)?,
                        2 => mmu.write_u16(eff_addr, self.regs.read_w(rt) as u16)?,
                        4 => mmu.write_u32(eff_addr, self.regs.read_w(rt))?,
                        8 => mmu.write_u64(eff_addr, self.regs.read_x(rt))?,
                        _ => unreachable!(),
                    }
                }

                if writeback || post_index {
                    let updated = (base as i64).wrapping_add(offset) as u64;
                    self.regs.write_x_or_sp(rn, updated);
                }

                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::LoadStorePair {
                rt1,
                rt2,
                rn,
                offset,
                is_load,
                is_64,
                writeback,
                post_index,
            } => {
                let base = self.regs.read_x_or_sp(rn);
                let stride = if is_64 { 8 } else { 4 };
                let eff_addr = if post_index {
                    base
                } else {
                    (base as i64).wrapping_add(offset) as u64
                };

                if is_load {
                    if is_64 {
                        let v1 = mmu.read_u64(eff_addr)?;
                        let v2 = mmu.read_u64(eff_addr.wrapping_add(stride))?;
                        self.regs.write_x(rt1, v1);
                        self.regs.write_x(rt2, v2);
                    } else {
                        let v1 = mmu.read_u32(eff_addr)?;
                        let v2 = mmu.read_u32(eff_addr.wrapping_add(stride))?;
                        self.regs.write_w(rt1, v1);
                        self.regs.write_w(rt2, v2);
                    }
                } else if is_64 {
                    mmu.write_u64(eff_addr, self.regs.read_x(rt1))?;
                    mmu.write_u64(eff_addr.wrapping_add(stride), self.regs.read_x(rt2))?;
                } else {
                    mmu.write_u32(eff_addr, self.regs.read_w(rt1))?;
                    mmu.write_u32(eff_addr.wrapping_add(stride), self.regs.read_w(rt2))?;
                }

                if writeback || post_index {
                    let updated = (base as i64).wrapping_add(offset) as u64;
                    self.regs.write_x_or_sp(rn, updated);
                }

                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SystemReg {
                rt,
                sysreg,
                is_read,
            } => {
                // System registers:
                // 0x5E82 = TPIDR_EL0   (S3_3_C13_C0_2)
                // 0x5E83 = TPIDRRO_EL0 (S3_3_C13_C0_3) - Horizon TLS Pointer
                // 0x5A10 = NZCV        (S3_3_C4_C2_0)
                // 0x5F00 = CNTFRQ_EL0  (19,200,000 Hz Switch 2 hardware timer frequency)
                // 0x5F01 = CNTPCT_EL0  (Switch 2 physical timer counter)
                // 0x5807 = DCZID_EL0   (S3_3_C0_C0_7)
                if is_read {
                    let val = match sysreg {
                        0x5E82 => self.regs.tpidr_el0,
                        0x5E83 => self.regs.tpidrro_el0,
                        0x5A10 => self.regs.pstate.to_bits() as u64,
                        0x5F00 => 19_200_000,
                        // DCZID_EL0: DC ZVA permitted, block = 2^4 words = DC_ZVA_BLOCK bytes
                        0x5807 => DC_ZVA_BLOCK.trailing_zeros() as u64 - 2,
                        0x5F01 => self.jit.cache_hits.wrapping_add(self.jit.cache_misses).wrapping_mul(16).wrapping_add(0x0123_4560),
                        _ => 0,
                    };
                    self.regs.write_x(rt, val);
                } else {
                    let val = self.regs.read_x(rt);
                    match sysreg {
                        0x5E82 => self.regs.tpidr_el0 = val,
                        0x5E83 => self.regs.tpidrro_el0 = val,
                        0x5A10 => self.regs.pstate.from_bits(val as u32),
                        _ => {}
                    }
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpData2 {
                rd,
                rn,
                rm,
                op,
                is_double,
            } => {
                if is_double {
                    let n = self.regs.read_d(rn);
                    let m = self.regs.read_d(rm);
                    let res = match op {
                        FpOp2::Fadd => n + m,
                        FpOp2::Fsub => n - m,
                        FpOp2::Fmul => n * m,
                        FpOp2::Fdiv => n / m,
                        FpOp2::Fmax => n.max(m),
                        FpOp2::Fmin => n.min(m),
                    };
                    self.regs.write_d(rd, res);
                } else {
                    let n = self.regs.read_s(rn);
                    let m = self.regs.read_s(rm);
                    let res = match op {
                        FpOp2::Fadd => n + m,
                        FpOp2::Fsub => n - m,
                        FpOp2::Fmul => n * m,
                        FpOp2::Fdiv => n / m,
                        FpOp2::Fmax => n.max(m),
                        FpOp2::Fmin => n.min(m),
                    };
                    self.regs.write_s(rd, res);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpCompare {
                rn,
                rm,
                is_zero: _,
                is_double,
            } => {
                let (n, m) = if is_double {
                    (self.regs.read_d(rn), self.regs.read_d(rm))
                } else {
                    (self.regs.read_s(rn) as f64, self.regs.read_s(rm) as f64)
                };

                if n.is_nan() || m.is_nan() {
                    self.regs.pstate.n = false;
                    self.regs.pstate.z = false;
                    self.regs.pstate.c = true;
                    self.regs.pstate.v = true;
                } else if n == m {
                    self.regs.pstate.n = false;
                    self.regs.pstate.z = true;
                    self.regs.pstate.c = true;
                    self.regs.pstate.v = false;
                } else if n < m {
                    self.regs.pstate.n = true;
                    self.regs.pstate.z = false;
                    self.regs.pstate.c = false;
                    self.regs.pstate.v = false;
                } else {
                    // n > m
                    self.regs.pstate.n = false;
                    self.regs.pstate.z = false;
                    self.regs.pstate.c = true;
                    self.regs.pstate.v = false;
                }

                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpMovGen {
                rd,
                rn,
                to_fp,
                is_64,
            } => {
                if to_fp {
                    if is_64 {
                        self.regs.write_d(rd, f64::from_bits(self.regs.read_x(rn)));
                    } else {
                        self.regs.write_s(rd, f32::from_bits(self.regs.read_w(rn)));
                    }
                } else if is_64 {
                    self.regs.write_x(rd, self.regs.read_d(rn).to_bits());
                } else {
                    self.regs.write_w(rd, self.regs.read_s(rn).to_bits());
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpLoadStoreImm {
                rt,
                rn,
                offset,
                is_load,
                is_double,
            } => {
                let base = self.regs.read_x_or_sp(rn);
                let eff_addr = (base as i64).wrapping_add(offset) as u64;
                if is_load {
                    if is_double {
                        let bits = mmu.read_u64(eff_addr)?;
                        self.regs.write_d(rt, f64::from_bits(bits));
                    } else {
                        let bits = mmu.read_u32(eff_addr)?;
                        self.regs.write_s(rt, f32::from_bits(bits));
                    }
                } else if is_double {
                    mmu.write_u64(eff_addr, self.regs.read_d(rt).to_bits())?;
                } else {
                    mmu.write_u32(eff_addr, self.regs.read_s(rt).to_bits())?;
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::MultiplyAddSub {
                rd,
                rn,
                rm,
                ra,
                is_sub,
                is_64,
            } => {
                if is_64 {
                    let n = self.regs.read_x(rn);
                    let m = self.regs.read_x(rm);
                    let a = self.regs.read_x(ra);
                    let prod = n.wrapping_mul(m);
                    let res = if is_sub {
                        a.wrapping_sub(prod)
                    } else {
                        a.wrapping_add(prod)
                    };
                    self.regs.write_x(rd, res);
                } else {
                    let n = self.regs.read_w(rn);
                    let m = self.regs.read_w(rm);
                    let a = self.regs.read_w(ra);
                    let prod = n.wrapping_mul(m);
                    let res = if is_sub {
                        a.wrapping_sub(prod)
                    } else {
                        a.wrapping_add(prod)
                    };
                    self.regs.write_w(rd, res);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Divide {
                rd,
                rn,
                rm,
                is_signed,
                is_64,
            } => {
                if is_64 {
                    let n = self.regs.read_x(rn);
                    let m = self.regs.read_x(rm);
                    let res = if m == 0 {
                        0
                    } else if is_signed {
                        (n as i64).wrapping_div(m as i64) as u64
                    } else {
                        n / m
                    };
                    self.regs.write_x(rd, res);
                } else {
                    let n = self.regs.read_w(rn);
                    let m = self.regs.read_w(rm);
                    let res = if m == 0 {
                        0
                    } else if is_signed {
                        (n as i32).wrapping_div(m as i32) as u32
                    } else {
                        n / m
                    };
                    self.regs.write_w(rd, res);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::ShiftReg {
                rd,
                rn,
                rm,
                shift_type,
                is_64,
            } => {
                if is_64 {
                    let n = self.regs.read_x(rn);
                    let shift = (self.regs.read_x(rm) & 63) as u32;
                    let res = match shift_type {
                        ShiftType::Lsl => n.wrapping_shl(shift),
                        ShiftType::Lsr => n.wrapping_shr(shift),
                        ShiftType::Asr => ((n as i64).wrapping_shr(shift)) as u64,
                        ShiftType::Ror => n.rotate_right(shift),
                    };
                    self.regs.write_x(rd, res);
                } else {
                    let n = self.regs.read_w(rn);
                    let shift = (self.regs.read_w(rm) & 31) as u32;
                    let res = match shift_type {
                        ShiftType::Lsl => n.wrapping_shl(shift),
                        ShiftType::Lsr => n.wrapping_shr(shift),
                        ShiftType::Asr => ((n as i32).wrapping_shr(shift)) as u32,
                        ShiftType::Ror => n.rotate_right(shift),
                    };
                    self.regs.write_w(rd, res);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLoadStoreQ {
                rt,
                rn,
                offset,
                is_load,
            } => {
                let base = self.regs.read_x_or_sp(rn);
                let addr = (base as i64).wrapping_add(offset) as u64;
                if is_load {
                    let lo = mmu.read_u64(addr)? as u128;
                    let hi = mmu.read_u64(addr + 8)? as u128;
                    self.regs.write_q(rt, lo | (hi << 64));
                } else {
                    let q = self.regs.read_q(rt);
                    mmu.write_u64(addr, (q & 0xFFFF_FFFF_FFFF_FFFF) as u64)?;
                    mmu.write_u64(addr + 8, (q >> 64) as u64)?;
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdVec4S { rd, rn, rm, op } => {
                let a = self.regs.read_v4s(rn);
                let b = self.regs.read_v4s(rm);
                let mut out = [0.0f32; 4];
                for i in 0..4 {
                    out[i] = match op {
                        FpOp2::Fadd => a[i] + b[i],
                        FpOp2::Fsub => a[i] - b[i],
                        FpOp2::Fmul => a[i] * b[i],
                        FpOp2::Fdiv => a[i] / b[i],
                        FpOp2::Fmax => a[i].max(b[i]),
                        FpOp2::Fmin => a[i].min(b[i]),
                    };
                }
                self.regs.write_v4s(rd, out);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::LoadStoreExclusive {
                rt,
                rn,
                rs,
                size_bytes,
                is_load,
                is_exclusive,
            } => {
                let addr = self.regs.read_x_or_sp(rn);
                if !is_load && is_exclusive && self.exclusive.take() != Some(addr) {
                    // The monitor was lost (another thread ran since the load-exclusive): fail, no store.
                    self.regs.write_w(rs, 1);
                    self.regs.advance_pc();
                    return Ok(StepResult::Continue);
                }
                if is_load && is_exclusive {
                    self.exclusive = Some(addr);
                }
                if is_load {
                    let val = match size_bytes {
                        1 => mmu.read_u8(addr)? as u64,
                        2 => mmu.read_u16(addr)? as u64,
                        4 => mmu.read_u32(addr)? as u64,
                        8 => mmu.read_u64(addr)?,
                        _ => unreachable!(),
                    };
                    if size_bytes == 8 {
                        self.regs.write_x(rt, val);
                    } else {
                        self.regs.write_w(rt, val as u32);
                    }
                } else {
                    match size_bytes {
                        1 => mmu.write_u8(addr, self.regs.read_w(rt) as u8)?,
                        2 => mmu.write_u16(addr, self.regs.read_w(rt) as u16)?,
                        4 => mmu.write_u32(addr, self.regs.read_w(rt))?,
                        8 => mmu.write_u64(addr, self.regs.read_x(rt))?,
                        _ => unreachable!(),
                    }
                    if is_exclusive {
                        // Store-exclusive succeeded -> status register Ws = 0
                        self.regs.write_w(rs, 0);
                    }
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLoadStorePair { rt1, rt2, rn, offset, size_bytes, is_load, writeback, post_index } => {
                let base = self.regs.read_x_or_sp(rn);
                let updated = (base as i64).wrapping_add(offset) as u64;
                let addr = if post_index { base } else { updated };
                let n = size_bytes as usize;
                for (i, rt) in [rt1, rt2].into_iter().enumerate() {
                    let at = addr.wrapping_add((i * n) as u64);
                    if is_load {
                        let mut buf = [0u8; 16];
                        mmu.read_bytes(at, &mut buf[..n])?;
                        self.regs.write_q(rt, u128::from_le_bytes(buf));
                    } else {
                        mmu.write_bytes(at, &self.regs.read_q(rt).to_le_bytes()[..n])?;
                    }
                }
                if writeback {
                    self.regs.write_x_or_sp(rn, updated);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::DcZva { rt } => {
                let addr = self.regs.read_x(rt) & !(DC_ZVA_BLOCK - 1);
                mmu.write_bytes(addr, &[0u8; DC_ZVA_BLOCK as usize])?;
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdModImm { rd, imm, op, q } => {
                let pattern = if q { imm as u128 | (imm as u128) << 64 } else { imm as u128 };
                let width = if q { u128::MAX } else { u64::MAX as u128 };
                let d = self.regs.read_q(rd);
                let res = match op {
                    SimdImmOp::Mov => pattern,
                    SimdImmOp::Orr => (d | pattern) & width,
                    SimdImmOp::Bic => (d & !pattern) & width,
                };
                self.regs.write_q(rd, res);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdDupElement { rd, rn, esize, index, q } => {
                let v = simd::lane(self.regs.read_q(rn), esize as u32, index as usize);
                self.regs.write_q(rd, simd::dup(v, esize as u32, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdInsGeneral { rd, rn, esize, index } => {
                let mut d = self.regs.read_q(rd);
                simd::set_lane(&mut d, esize as u32, index as usize, self.regs.read_x(rn));
                self.regs.write_q(rd, d);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdInsElement { rd, rn, esize, dst_index, src_index } => {
                let v = simd::lane(self.regs.read_q(rn), esize as u32, src_index as usize);
                let mut d = self.regs.read_q(rd);
                simd::set_lane(&mut d, esize as u32, dst_index as usize, v);
                self.regs.write_q(rd, d);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLoadStoreSingle { rt, rn, rm, esize, index, replicate, q, is_load, post_index } => {
                let base = self.regs.read_x_or_sp(rn);
                let n = esize as usize / 8;
                if is_load {
                    let mut buf = [0u8; 8];
                    mmu.read_bytes(base, &mut buf[..n])?;
                    let v = u64::from_le_bytes(buf);
                    let res = if replicate {
                        simd::dup(v, esize as u32, q)
                    } else {
                        let mut d = self.regs.read_q(rt);
                        simd::set_lane(&mut d, esize as u32, index as usize, v);
                        d
                    };
                    self.regs.write_q(rt, res);
                } else {
                    let v = simd::lane(self.regs.read_q(rt), esize as u32, index as usize);
                    mmu.write_bytes(base, &v.to_le_bytes()[..n])?;
                }
                if post_index {
                    let step = if rm == 31 { n as u64 } else { self.regs.read_x(rm) };
                    self.regs.write_x_or_sp(rn, base.wrapping_add(step));
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdPermute { rd, rn, rm, op, esize, q } => {
                let (n, m) = (self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs.write_q(rd, simd::permute(op, n, m, esize as u32, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::DataProc1 { rd, rn, op, is_64 } => {
                if is_64 {
                    let x = self.regs.read_x(rn);
                    let res = match op {
                        DataProc1Op::Rbit => x.reverse_bits(),
                        DataProc1Op::Rev16 => ((x & 0x00FF_00FF_00FF_00FF) << 8) | ((x >> 8) & 0x00FF_00FF_00FF_00FF),
                        DataProc1Op::Rev32 => {
                            (((x >> 32) as u32).swap_bytes() as u64) << 32 | (x as u32).swap_bytes() as u64
                        }
                        DataProc1Op::Rev64 => x.swap_bytes(),
                        DataProc1Op::Clz => x.leading_zeros() as u64,
                        DataProc1Op::Cls => ((x as i64 ^ ((x as i64) >> 1)) as u64).leading_zeros() as u64 - 1,
                    };
                    self.regs.write_x(rd, res);
                } else {
                    let w = self.regs.read_w(rn);
                    let res = match op {
                        DataProc1Op::Rbit => w.reverse_bits(),
                        DataProc1Op::Rev16 => ((w & 0x00FF_00FF) << 8) | ((w >> 8) & 0x00FF_00FF),
                        DataProc1Op::Rev32 | DataProc1Op::Rev64 => w.swap_bytes(),
                        DataProc1Op::Clz => w.leading_zeros(),
                        DataProc1Op::Cls => ((w as i32 ^ ((w as i32) >> 1)) as u32).leading_zeros() - 1,
                    };
                    self.regs.write_w(rd, res);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::MultiplyHigh { rd, rn, rm, is_signed } => {
                let (n, m) = (self.regs.read_x(rn), self.regs.read_x(rm));
                let hi = if is_signed {
                    ((n as i64 as i128 * m as i64 as i128) >> 64) as u64
                } else {
                    ((n as u128 * m as u128) >> 64) as u64
                };
                self.regs.write_x(rd, hi);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::MultiplyLong { rd, rn, rm, ra, is_signed, is_sub } => {
                let (n, m) = (self.regs.read_w(rn), self.regs.read_w(rm));
                let prod = if is_signed {
                    (n as i32 as i64).wrapping_mul(m as i32 as i64) as u64
                } else {
                    (n as u64).wrapping_mul(m as u64)
                };
                let a = self.regs.read_x(ra);
                self.regs.write_x(rd, if is_sub { a.wrapping_sub(prod) } else { a.wrapping_add(prod) });
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLoadStore { rt, rn, offset, size_bytes, is_load, writeback, post_index } => {
                let base = self.regs.read_x_or_sp(rn);
                let updated = (base as i64).wrapping_add(offset) as u64;
                let addr = if post_index { base } else { updated };
                let n = size_bytes as usize;
                if is_load {
                    let mut buf = [0u8; 16];
                    mmu.read_bytes(addr, &mut buf[..n])?;
                    // Loads into B/H/S/D/Q zero the rest of the vector register.
                    self.regs.write_q(rt, u128::from_le_bytes(buf));
                } else {
                    mmu.write_bytes(addr, &self.regs.read_q(rt).to_le_bytes()[..n])?;
                }
                if writeback {
                    self.regs.write_x_or_sp(rn, updated);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLoadStoreReg { rt, rn, rm, option, shift, size_bytes, is_load } => {
                let raw_m = self.regs.read_x(rm);
                let index: u64 = match option {
                    0b010 => (raw_m as u32) as u64,          // UXTW
                    0b110 => ((raw_m as i32) as i64) as u64, // SXTW
                    _ => raw_m,                              // LSL / SXTX
                };
                let addr = self.regs.read_x_or_sp(rn).wrapping_add(index.wrapping_shl(shift as u32));
                let n = size_bytes as usize;
                if is_load {
                    let mut buf = [0u8; 16];
                    mmu.read_bytes(addr, &mut buf[..n])?;
                    self.regs.write_q(rt, u128::from_le_bytes(buf));
                } else {
                    mmu.write_bytes(addr, &self.regs.read_q(rt).to_le_bytes()[..n])?;
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLoadStoreMulti { rt, rn, rm, regs, q, is_load, post_index } => {
                let base = self.regs.read_x_or_sp(rn);
                let per_reg = if q { 16 } else { 8 };
                for r in 0..regs as u64 {
                    let addr = base.wrapping_add(r * per_reg as u64);
                    let reg = (rt + r as u8) % 32;
                    if is_load {
                        let mut buf = [0u8; 16];
                        mmu.read_bytes(addr, &mut buf[..per_reg])?;
                        self.regs.write_q(reg, u128::from_le_bytes(buf));
                    } else {
                        mmu.write_bytes(addr, &self.regs.read_q(reg).to_le_bytes()[..per_reg])?;
                    }
                }
                if post_index {
                    let step = if rm == 31 { regs as u64 * per_reg as u64 } else { self.regs.read_x(rm) };
                    self.regs.write_x_or_sp(rn, base.wrapping_add(step));
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdDupGeneral { rd, rn, esize, q } => {
                self.regs.write_q(rd, simd::dup(self.regs.read_x(rn), esize as u32, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdMovToGeneral { rd, rn, esize, index } => {
                let val = simd::lane(self.regs.read_q(rn), esize as u32, index as usize);
                if esize == 64 {
                    self.regs.write_x(rd, val);
                } else {
                    self.regs.write_w(rd, val as u32);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdThreeSame { rd, rn, rm, op, esize, q } => {
                let (d, n, m) = (self.regs.read_q(rd), self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs.write_q(rd, simd::three_same(op, d, n, m, esize as u32, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdTwoMisc { rd, rn, op, esize, q } => {
                self.regs.write_q(rd, simd::two_misc(op, self.regs.read_q(rn), esize as u32, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdShiftImm { rd, rn, op, esize, shift, q } => {
                let (n, e, sh) = (self.regs.read_q(rn), esize as u32, shift as u32);
                let res = match op {
                    SimdShiftOp::Sshr => simd::shift_imm(n, e, sh, q, true, false),
                    SimdShiftOp::Ushr => simd::shift_imm(n, e, sh, q, false, false),
                    SimdShiftOp::Shl => simd::shift_imm(n, e, sh, q, false, true),
                    SimdShiftOp::Shrn => simd::shrn(self.regs.read_q(rd), n, e, sh, q),
                };
                self.regs.write_q(rd, res);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Unimplemented { raw } => {
                Err(CpuError::UnimplementedInstruction { raw, pc })
            }
        }
    }

    /// Evaluates AArch64 condition codes against PSTATE.
    pub fn check_condition(&self, cond: Condition) -> bool {
        let pstate = &self.regs.pstate;
        match cond {
            Condition::Eq => pstate.z,
            Condition::Ne => !pstate.z,
            Condition::Cs => pstate.c,
            Condition::Cc => !pstate.c,
            Condition::Mi => pstate.n,
            Condition::Pl => !pstate.n,
            Condition::Vs => pstate.v,
            Condition::Vc => !pstate.v,
            Condition::Hi => pstate.c && !pstate.z,
            Condition::Ls => !pstate.c || pstate.z,
            Condition::Ge => pstate.n == pstate.v,
            Condition::Lt => pstate.n != pstate.v,
            Condition::Gt => !pstate.z && (pstate.n == pstate.v),
            Condition::Le => pstate.z || (pstate.n != pstate.v),
            Condition::Al | Condition::Nv => true,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Encodings below were produced by the devkitA64 assembler from the mnemonic in each comment.
    use super::*;
    use crate::mmu::Permission;

    const CODE: u64 = 0x1000;
    const DATA: u64 = 0x8000;

    /// Runs `words` from `CODE` with `setup` applied first. `DATA..DATA+0x1000` is mapped read/write.
    fn run(words: &[u32], setup: impl FnOnce(&mut Cpu, &mut Memory)) -> (Cpu, Memory) {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        mmu.map_range(CODE, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.map_range(DATA, 0x1000, Permission::READ_WRITE);
        for (i, w) in words.iter().enumerate() {
            mmu.write_u32(CODE + i as u64 * 4, *w).unwrap();
        }
        cpu.regs.pc = CODE;
        setup(&mut cpu, &mut mmu);
        for _ in words {
            assert!(matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)), "step failed at 0x{:X}", cpu.regs.pc);
        }
        (cpu, mmu)
    }

    #[test]
    fn libnx_ops_decode() {
        let table: &[(u32, &str)] = &[
            (0xdac00043, "DataProc1"),          // rbit x3, x2
            (0xdac00c84, "DataProc1"),          // rev x4, x4
            (0x5ac008a4, "DataProc1"),          // rev w4, w5
            (0x5ac00441, "DataProc1"),          // rev16 w1, w2
            (0xdac00841, "DataProc1"),          // rev32 x1, x2
            (0xdac01063, "DataProc1"),          // clz x3, x3
            (0xdac014c5, "DataProc1"),          // cls x5, x6
            (0x9bc17c00, "MultiplyHigh"),       // umulh x0, x0, x1
            (0x9b447c62, "MultiplyHigh"),       // smulh x2, x3, x4
            (0x9ba20c20, "MultiplyLong"),       // umaddl x0, w1, w2, x3
            (0x9b228c20, "MultiplyLong"),       // smsubl x0, w1, w2, x3
            (0x3cc0827f, "SimdLoadStore"),      // ldur q31, [x19, #8]
            (0x3c9f0080, "SimdLoadStore"),      // stur q0, [x4, #-16]
            (0xfc408441, "SimdLoadStore"),      // ldr d1, [x2], #8
            (0xbc004c83, "SimdLoadStore"),      // str s3, [x4, #4]!
            (0x3ce1681f, "SimdLoadStoreReg"),   // ldr q31, [x0, x1]
            (0xfc24d862, "SimdLoadStoreReg"),   // str d2, [x3, w4, sxtw #3]
            (0x4c40a021, "SimdLoadStoreMulti"), // ld1 {v1.16b-v2.16b}, [x1]
            (0x4cdf7020, "SimdLoadStoreMulti"), // ld1 {v0.16b}, [x1], #16
            (0x0c836044, "SimdLoadStoreMulti"), // st1 {v4.8b-v6.8b}, [x2], x3
            (0x4e010c20, "SimdDupGeneral"),     // dup v0.16b, w1
            (0x4e080c83, "SimdDupGeneral"),     // dup v3.2d, x4
            (0x4e083c01, "SimdMovToGeneral"),   // mov x1, v0.d[0]
            (0x0e0b3c22, "SimdMovToGeneral"),   // umov w2, v1.b[5]
            (0x6e22ac20, "SimdThreeSame"),      // uminp v0.16b, v1.16b, v2.16b
            (0x4e22bc20, "SimdThreeSame"),      // addp v0.16b, v1.16b, v2.16b
            (0x4e201c21, "SimdThreeSame"),      // and v1.16b, v1.16b, v0.16b
            (0x4ea41c83, "SimdThreeSame"),      // mov v3.16b, v4.16b
            (0x6e621c20, "SimdThreeSame"),      // bsl v0.16b, v1.16b, v2.16b
            (0x4ea28420, "SimdThreeSame"),      // add v0.4s, v1.4s, v2.4s
            (0x6e623420, "SimdThreeSame"),      // cmhi v0.8h, v1.8h, v2.8h
            (0x0e209800, "SimdTwoMisc"),        // cmeq v0.8b, v0.8b, #0
            (0x0e205820, "SimdTwoMisc"),        // cnt v0.8b, v1.8b
            (0x6e205820, "SimdTwoMisc"),        // mvn v0.16b, v1.16b
            (0x0f0c8400, "SimdShiftImm"),       // shrn v0.8b, v0.8h, #4
            (0x4f088420, "SimdShiftImm"),       // shrn2 v0.16b, v1.8h, #8
            (0x6f3d0420, "SimdShiftImm"),       // ushr v0.4s, v1.4s, #3
            (0x4f4c5420, "SimdShiftImm"),       // shl v0.2d, v1.2d, #12
            (0xad3e0080, "SimdLoadStorePair"),  // stp q0, q0, [x4, #-64]
            (0x6cc107e0, "SimdLoadStorePair"),  // ldp d0, d1, [sp], #16
            (0xac0008a1, "SimdLoadStorePair"),  // stnp q1, q2, [x5]
            (0xd50b7423, "DcZva"),              // dc zva, x3
            (0x4f00e41f, "SimdModImm"),         // movi v31.16b, #0
            (0x4e1d3bde, "SimdPermute"),        // zip1 v30.16b, v30.16b, v29.16b
            (0x0e095907, "SimdPermute"),        // uzp2 v7.8b, v8.8b, v9.8b
            (0x4e1f0549, "SimdDupElement"),     // dup v9.16b, v10.b[15]
            (0x6e1c2441, "SimdInsElement"),     // mov v1.s[3], v2.s[1]
            (0x4e181cc5, "SimdInsGeneral"),     // mov v5.d[1], x6
            (0x4d0048a4, "SimdLoadStoreSingle"), // st1 {v4.h}[5], [x5]
            (0x3d4a001d, "SimdLoadStore"),      // ldr b29, [x0, #640]
            (0x1e2e101f, "SimdModImm"),         // fmov s31, #1.0
        ];
        for &(raw, name) in table {
            let got = format!("{:?}", super::super::decoder::decode(raw));
            assert!(got.starts_with(name), "0x{raw:08X}: expected {name}, got {got}");
        }
    }

    #[test]
    fn extended_register_sub_writes_sp() {
        // mov x12, #0x40b0; sub sp, sp, x12 (large stack frames in GCC output)
        let (cpu, _) = run(&[0xd288160c, 0xcb2c63ff], |c, _| c.regs.sp = 0x10_0000);
        assert_eq!(cpu.regs.sp, 0x10_0000 - 0x40b0);
    }

    #[test]
    fn scalar_bit_ops() {
        let (cpu, _) = run(&[0xdac00043, 0xdac01063, 0xdac014c5, 0x5ac00441, 0xdac00841], |c, _| {
            c.regs.write_x(2, 0x10); // rbit -> bit 59 set, clz -> 4
            c.regs.write_x(6, 0xFFFF_0000_0000_0000); // cls -> 15
        });
        assert_eq!(cpu.regs.read_x(3), 4); // clz(rbit(0x10)) = clz(1 << 59)
        assert_eq!(cpu.regs.read_x(5), 15);
        // rev16 w1, w2 then rev32 x1, x2 overwrites x1: rev32 of 0x10 = 0x1000_0000
        assert_eq!(cpu.regs.read_x(1), 0x1000_0000);
        let (cpu, _) = run(&[0xdac00c84, 0x5ac008a4], |c, _| {
            c.regs.write_x(4, 0x0102_0304_0506_0708);
            c.regs.write_x(5, 0x1122_3344);
        });
        assert_eq!(cpu.regs.read_x(4), 0x4433_2211); // rev w4, w5 runs last
        let (cpu, _) = run(&[0x5ac00441], |c, _| c.regs.write_x(2, 0xAABB_CCDD));
        assert_eq!(cpu.regs.read_x(1), 0xBBAA_DDCC);
    }

    #[test]
    fn multiply_high_and_long() {
        let (cpu, _) = run(&[0x9bc17c00, 0x9b447c62], |c, _| {
            c.regs.write_x(0, u64::MAX);
            c.regs.write_x(1, 3);
            c.regs.write_x(3, (-2i64) as u64);
            c.regs.write_x(4, 5);
        });
        assert_eq!(cpu.regs.read_x(0), 2); // (2^64 - 1) * 3 >> 64
        assert_eq!(cpu.regs.read_x(2), u64::MAX); // -10 >> 64 = -1
        let (cpu, _) = run(&[0x9ba20c20], |c, _| {
            c.regs.write_x(1, 0xFFFF_FFFF);
            c.regs.write_x(2, 2);
            c.regs.write_x(3, 1);
        });
        assert_eq!(cpu.regs.read_x(0), 0x1_FFFF_FFFF);
        let (cpu, _) = run(&[0x9b228c20], |c, _| {
            c.regs.write_x(1, 0xFFFF_FFFF); // -1 as a signed word
            c.regs.write_x(2, 2);
            c.regs.write_x(3, 1);
        });
        assert_eq!(cpu.regs.read_x(0), 3); // 1 - (-1 * 2)
    }

    #[test]
    fn simd_loads_and_stores() {
        let (cpu, mmu) = run(&[0x3cc0827f, 0x3c9f0080, 0xfc408441], |c, m| {
            for i in 0..32u64 {
                m.write_u8(DATA + i, i as u8).unwrap();
            }
            c.regs.write_x(19, DATA);
            c.regs.write_q(0, 0xA5A5_A5A5_A5A5_A5A5_A5A5_A5A5_A5A5_A5A5);
            c.regs.write_x(4, DATA + 0x100);
            c.regs.write_x(2, DATA);
        });
        assert_eq!(cpu.regs.read_q(31), u128::from_le_bytes(core::array::from_fn(|i| i as u8 + 8)));
        assert_eq!(mmu.read_u64(DATA + 0xF0).unwrap(), 0xA5A5_A5A5_A5A5_A5A5);
        assert_eq!(cpu.regs.read_q(1), 0x0706_0504_0302_0100); // D load zeroes the upper bits
        assert_eq!(cpu.regs.read_x(2), DATA + 8); // post-index writeback
        assert_eq!(cpu.regs.read_x(4), DATA + 0x100); // stur does not write back

        let (cpu, mmu) = run(&[0x4c40a021, 0x0c836044], |c, m| {
            for i in 0..32u64 {
                m.write_u8(DATA + i, 0x40 + i as u8).unwrap();
            }
            c.regs.write_x(1, DATA);
            c.regs.write_x(2, DATA + 0x200);
            c.regs.write_x(3, 0x30);
            c.regs.write_q(4, 0x1111);
            c.regs.write_q(5, 0x2222);
            c.regs.write_q(6, 0x3333);
        });
        assert_eq!(cpu.regs.read_q(2) & 0xFF, 0x50);
        assert_eq!(mmu.read_u64(DATA + 0x208).unwrap(), 0x2222);
        assert_eq!(mmu.read_u64(DATA + 0x210).unwrap(), 0x3333);
        assert_eq!(cpu.regs.read_x(2), DATA + 0x230);

        let (cpu, _) = run(&[0x3ce1681f], |c, m| {
            m.write_u64(DATA + 0x40, 0xDEAD).unwrap();
            c.regs.write_x(0, DATA);
            c.regs.write_x(1, 0x40);
        });
        assert_eq!(cpu.regs.read_q(31), 0xDEAD);
    }

    #[test]
    fn simd_pairs_and_dc_zva() {
        // stp q0, q0, [x3, #64]!
        let (cpu, mmu) = run(&[0xad820060], |c, _| {
            c.regs.write_x(3, DATA);
            c.regs.write_q(0, u128::MAX);
        });
        assert_eq!(cpu.regs.read_x(3), DATA + 64); // pre-index writeback
        assert_eq!(mmu.read_u64(DATA + 64).unwrap(), u64::MAX);
        assert_eq!(mmu.read_u64(DATA + 64 + 24).unwrap(), u64::MAX);
        // dc zva, x3 zeroes the aligned 64-byte block containing x3.
        let (_, mmu2) = run(&[0xd50b7423], |c, m| {
            for i in 0..0x100u64 {
                m.write_u8(DATA + i, 0xEE).unwrap();
            }
            c.regs.write_x(3, DATA + 0x47);
        });
        assert_eq!(mmu2.read_u64(DATA + 0x40).unwrap(), 0);
        assert_eq!(mmu2.read_u64(DATA + 0x78).unwrap(), 0);
        assert_eq!(mmu2.read_u64(DATA + 0x80).unwrap(), 0xEEEE_EEEE_EEEE_EEEE);
        assert_eq!(mmu2.read_u64(DATA + 0x38).unwrap(), 0xEEEE_EEEE_EEEE_EEEE);

        let (cpu, _) = run(&[0xd53b00e0], |_, _| {});
        assert_eq!(1u64 << (cpu.regs.read_x(0) + 2), DC_ZVA_BLOCK); // mrs x0, dczid_el0
    }

    #[test]
    fn simd_lane_moves_and_single_structure() {
        let (cpu, mmu) = run(
            &[
                0x7d400444, // ldr h4, [x2, #2]
                0x6e0307be, // mov v30.b[1], v29.b[0]
                0x4e0c1c83, // mov v3.s[1], w4
                0x4e140507, // dup v7.4s, v8.s[2]
                0x0d40183d, // ld1 {v29.b}[6], [x1]
                0x4ddf9062, // ld1 {v2.s}[3], [x3], #4
                0x4d8884e6, // st1 {v6.d}[1], [x7], x8
                0x4d40c949, // ld1r {v9.4s}, [x10]
                0x3d000065, // str b5, [x3]
            ],
            |c, m| {
                m.write_u64(DATA, 0x1122_3344_5566_7788).unwrap();
                c.regs.write_x(2, DATA);
                c.regs.write_q(29, 0xAB);
                c.regs.write_x(4, 0xCAFE_F00D);
                c.regs.write_q(8, 0x0000_0002_0000_0001u128 << 64 | 0xDEAD_BEEF_0000_0000);
                c.regs.write_x(1, DATA + 1);
                c.regs.write_x(3, DATA + 4);
                c.regs.write_q(6, 0x0102_0304_0506_0708u128 << 64);
                c.regs.write_x(7, DATA + 0x100);
                c.regs.write_x(8, 0x20);
                c.regs.write_x(10, DATA);
                c.regs.write_q(5, 0x99);
            },
        );
        assert_eq!(cpu.regs.read_q(4), 0x5566);
        assert_eq!(cpu.regs.read_q(30) >> 8 & 0xFF, 0xAB);
        assert_eq!(cpu.regs.read_q(3) >> 32 & 0xFFFF_FFFF, 0xCAFE_F00D);
        assert_eq!(cpu.regs.read_q(7), super::simd::dup(1, 32, true));
        assert_eq!(cpu.regs.read_q(29) >> 48 & 0xFF, 0x77);
        assert_eq!(cpu.regs.read_q(2) >> 96, 0x1122_3344);
        assert_eq!(cpu.regs.read_x(3), DATA + 8);
        assert_eq!(mmu.read_u64(DATA + 0x100).unwrap(), 0x0102_0304_0506_0708);
        assert_eq!(cpu.regs.read_x(7), DATA + 0x120);
        assert_eq!(cpu.regs.read_q(9), super::simd::dup(0x5566_7788, 32, true));
        assert_eq!(mmu.read_u8(DATA + 8).unwrap(), 0x99); // x3 was advanced to DATA + 8
    }

    #[test]
    fn simd_modified_immediate() {
        let words = [0x4f00e41f, 0x4f052561, 0x6f008642, 0x6f05e4a3, 0x2f00e424, 0x4f005425, 0x6f0797e6, 0x4f03f607, 0x6f04f488, 0x4f02c749];
        let (cpu, _) = run(&words, |c, _| {
            c.regs.write_q(31, u128::MAX);
            c.regs.write_q(4, u128::MAX);
            c.regs.write_q(5, 0x10);
            c.regs.write_q(6, u128::MAX);
        });
        let rep = |v: u64| v as u128 | (v as u128) << 64;
        assert_eq!(cpu.regs.read_q(31), 0); // movi v31.16b, #0
        assert_eq!(cpu.regs.read_q(1), rep(0x0000_AB00_0000_AB00)); // movi v1.4s, #0xab, lsl #8
        assert_eq!(cpu.regs.read_q(2), rep(0xFFED_FFED_FFED_FFED)); // mvni v2.8h, #0x12
        assert_eq!(cpu.regs.read_q(3), rep(0xFF00_FF00_00FF_00FF)); // movi v3.2d
        assert_eq!(cpu.regs.read_q(4), 0xFF); // movi d4, #0xff clears the upper half
        assert_eq!(cpu.regs.read_q(5), rep(0x0001_0000_0001_0000) | 0x10); // orr v5.4s, #1, lsl #16
        assert_eq!(cpu.regs.read_q(6), rep(0xFF00_FF00_FF00_FF00)); // bic v6.8h, #0xff
        assert_eq!(cpu.regs.read_q(7), rep(0x3F80_0000_3F80_0000)); // fmov v7.4s, #1.0
        assert_eq!(cpu.regs.read_q(8), rep((-2.5f64).to_bits())); // fmov v8.2d, #-2.5
        assert_eq!(cpu.regs.read_q(9), rep(0x0000_5AFF_0000_5AFF)); // movi v9.4s, #0x5a, msl #8

        // fmov s31, #1.0 / fmov d0, #-2.5 (scalar): low bits only, rest zeroed.
        let (cpu, _) = run(&[0x1e2e101f, 0x1e709000], |c, _| {
            c.regs.write_q(31, u128::MAX);
            c.regs.write_q(0, u128::MAX);
        });
        assert_eq!(cpu.regs.read_q(31), 1.0f32.to_bits() as u128);
        assert_eq!(cpu.regs.read_q(0), (-2.5f64).to_bits() as u128);
    }

    #[test]
    fn simd_vector_ops() {
        // dup v0.16b, w1; cmeq v0.16b, v1.16b, #0; mov x1, v0.d[0]
        let (cpu, _) = run(&[0x4e010c20, 0x4e209820, 0x4e083c01], |c, _| {
            c.regs.write_x(1, 0x1AB);
            c.regs.write_q(1, 0xFF00);
        });
        assert_eq!(cpu.regs.read_x(1), 0xFFFF_FFFF_FFFF_00FF);
        // shrn v0.8b, v0.8h, #4 on 0x0AB0 halfwords
        let (cpu, _) = run(&[0x0f0c8400], |c, _| c.regs.write_q(0, super::simd::dup(0x0AB0, 16, true)));
        assert_eq!(cpu.regs.read_q(0), 0xABAB_ABAB_ABAB_ABAB);
        // umov w2, v1.b[5]
        let (cpu, _) = run(&[0x0e0b3c22], |c, _| c.regs.write_q(1, 0x7700_0000_0000));
        assert_eq!(cpu.regs.read_x(2), 0x77);
        // mov v3.16b, v4.16b
        let (cpu, _) = run(&[0x4ea41c83], |c, _| c.regs.write_q(4, u128::MAX - 5));
        assert_eq!(cpu.regs.read_q(3), u128::MAX - 5);
        // add v0.4s, v1.4s, v2.4s wraps per lane
        let (cpu, _) = run(&[0x4ea28420], |c, _| {
            c.regs.write_q(1, super::simd::dup(0xFFFF_FFFF, 32, true));
            c.regs.write_q(2, super::simd::dup(2, 32, true));
        });
        assert_eq!(cpu.regs.read_q(0), super::simd::dup(1, 32, true));
    }
}
