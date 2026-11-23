#![allow(dead_code)]

use super::instructions::{
    BranchRegOp, Condition, DataProc1Op, FpOp2, Instruction, LogicalOp, MoveWideOp, PacModifier, PauthOp, ShiftType,
    SimdFpOp3, SimdImmOp, SimdOp2, SimdShiftOp,
};
use super::pauth::{self, PointerAuth};
use super::simd;
use super::softfp::{self, Env as FpEnv, Fmt as FpFmt};

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

    #[error("BRK #{imm:#x} breakpoint hit at PC: 0x{pc:016X}")]
    Breakpoint { imm: u16, pc: u64 },

    #[error("CPU halted: {0}")]
    Halted(String),

    /// Branch Target exception: an indirect branch into a guarded page did not land on a valid
    /// landing pad (`BTI`, `PACIASP`/`PACIBSP`, `BRK`, `HLT`) for `btype`.
    #[error("Branch target exception at PC: 0x{pc:016X} (BTYPE {btype:02b})")]
    BranchTarget { pc: u64, btype: u8 },
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
    /// Pointer authentication keys and EL0 controls (shared by the process's threads).
    pub pauth: PointerAuth,
    /// `CNTPCT_EL0`: the kernel moves it up to the system tick at every SVC, and each read advances it by
    /// [`COUNTER_READ_TICKS`] so a loop polling the counter always sees time pass. It depends on
    /// nothing the CPU engines differ in.
    pub counter_ticks: u64,
    /// Set once the program touches `FPSR` (or by a harness). Tier-4 then stops compiling FP arithmetic and
    /// compares, which run in the interpreter instead, because compiled code does not accumulate the FPSR
    /// exception flags. FP operations compiled before the first access leave no flags. See `docs/verify/fp-edge`.
    pub fp_flags_tracked: bool,
    /// Most recent fault or halt reason produced by [`Self::step`] or [`Self::check_landing_pad`],
    /// so block engines that collapse step errors into `BlockExit::Fault` can still report the exact
    /// underlying instruction or memory fault.
    pub last_error: Option<String>,
}

/// Counter ticks (19.2 MHz) one `CNTPCT_EL0` read takes: about 52 microseconds.
pub const COUNTER_READ_TICKS: u64 = 1000;

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
            counter_ticks: 0x0123_4560,
            exclusive: None,
            pauth: PointerAuth::default(),
            fp_flags_tracked: false,
            last_error: None,
        }
    }

    /// The BTI check for the instruction at PC after an indirect branch into a guarded page.
    /// Clears `PSTATE.BTYPE`. Block engines call it on entry; `step` calls it itself.
    pub fn check_landing_pad(&mut self, mmu: &Memory) -> Result<(), CpuError> {
        let btype = std::mem::take(&mut self.regs.pstate.btype);
        if btype == 0 {
            return Ok(());
        }
        let pc = self.regs.pc;
        let raw = mmu.read_u32(pc).map_err(|e| {
            let err = CpuError::Memory(e);
            self.last_error = Some(err.to_string());
            err
        })?;
        if pauth::bti_compatible(raw, btype, self.pauth.bt0) {
            Ok(())
        } else {
            let err = CpuError::BranchTarget { pc, btype };
            self.last_error = Some(err.to_string());
            Err(err)
        }
    }

    fn pac_modifier(&self, modifier: PacModifier) -> u64 {
        match modifier {
            PacModifier::Reg(r) => self.regs.read_x_or_sp(r),
            PacModifier::Zero => 0,
        }
    }

    /// Takes an indirect branch: sets PC, the link register for BLR, and `PSTATE.BTYPE`.
    fn branch_indirect(&mut self, mmu: &Memory, pc: u64, target: u64, rn: u8, op: BranchRegOp) {
        if op == BranchRegOp::Blr {
            self.regs.write_x(30, pc.wrapping_add(4));
        }
        self.regs.pstate.btype = pauth::next_btype(
            op == BranchRegOp::Blr,
            op == BranchRegOp::Ret,
            rn,
            mmu.is_guarded(pc),
            mmu.is_guarded(target),
        );
        self.regs.pc = target;
    }

    /// Fetches, basic-block JIT-compiles/looks up, and executes a single AArch64 instruction, then folds the
    /// floating-point exception flags it raised into `FPSR`.
    pub fn step(&mut self, mmu: &mut Memory) -> Result<StepResult, CpuError> {
        let res = self.step_inner(mmu);
        let raised = softfp::take_flags();
        if raised != 0 {
            self.regs.fpsr |= raised;
        }
        match &res {
            Err(err) => self.last_error = Some(err.to_string()),
            Ok(StepResult::Halt(msg)) => self.last_error = Some(format!("CPU halted: {msg}")),
            _ => {}
        }
        res
    }

    fn step_inner(&mut self, mmu: &mut Memory) -> Result<StepResult, CpuError> {
        let pc = self.regs.pc;
        let raw_inst = mmu.read_u32(pc)?;
        if self.regs.pstate.btype != 0 {
            let btype = std::mem::take(&mut self.regs.pstate.btype);
            if !pauth::bti_compatible(raw_inst, btype, self.pauth.bt0) {
                return Err(CpuError::BranchTarget { pc, btype });
            }
        }
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
                        let mask = !(0xFFFF_u64 << shift);
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
                // Immediate arithmetic always interprets Rn == 31 as SP, including CMP/CMN.
                let op1 = if rn == 31 { self.regs.sp } else { self.regs.read_x(rn) };

                let op2 = imm;

                if is_64 {
                    let (res, c, v) = if is_sub {
                        let (res, borrow) = op1.overflowing_sub(op2);
                        let v = ((op1 ^ op2) & (op1 ^ res) & (1 << 63)) != 0;
                        (res, !borrow, v)
                    } else {
                        let (res, carry) = op1.overflowing_add(op2);
                        let v = (!(op1 ^ op2) & (op1 ^ res) & (1 << 63)) != 0;
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
                        let v = (!(op1_32 ^ op2_32) & (op1_32 ^ res) & (1 << 31)) != 0;
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
                        0b000 => (raw_rm as u8) as u64,           // UXTB
                        0b001 => (raw_rm as u16) as u64,          // UXTH
                        0b010 => (raw_rm as u32) as u64,          // UXTW
                        0b011 => raw_rm,                          // UXTX
                        0b100 => ((raw_rm as i8) as i64) as u64,  // SXTB
                        0b101 => ((raw_rm as i16) as i64) as u64, // SXTH
                        0b110 => ((raw_rm as i32) as i64) as u64, // SXTW
                        _ => raw_rm,                              // SXTX
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
                        let v = (!(op1 ^ op2) & (op1 ^ res) & (1 << 63)) != 0;
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
                        let v = (!(op1_32 ^ op2_32) & (op1_32 ^ res) & (1 << 31)) != 0;
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

            Instruction::Extract { rd, rn, rm, lsb, is_64 } => {
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
                        0 => m,                 // CSEL
                        1 => m.wrapping_add(1), // CSINC
                        2 => !m,                // CSINV
                        _ => m.wrapping_neg(),  // CSNEG
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
                            let v = (!(op1 ^ op2) & (op1 ^ r) & (1 << 63)) != 0;
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
                            let v = (!(op1_32 ^ op2_32) & (op1_32 ^ r) & (1 << 31)) != 0;
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
                    0b010 => (raw_m as u32) as u64,          // UXTW
                    0b110 => ((raw_m as i32) as i64) as u64, // SXTW
                    0b111 => raw_m,                          // SXTX
                    _ => raw_m,                              // LSL / UXTX
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
                self.branch_indirect(mmu, pc, target, rn, op);
                Ok(StepResult::Continue)
            }

            Instruction::BranchRegAuth { rn, op, key, modifier } => {
                let modifier = self.pac_modifier(modifier);
                let target = self.pauth.auth(self.regs.read_x(rn), modifier, key);
                self.branch_indirect(mmu, pc, target, rn, op);
                Ok(StepResult::Continue)
            }

            Instruction::Pauth { rd, op, key, modifier } => {
                let modifier = self.pac_modifier(modifier);
                let ptr = self.regs.read_x(rd);
                let val = match op {
                    PauthOp::Sign => self.pauth.sign(ptr, modifier, key),
                    PauthOp::Auth => self.pauth.auth(ptr, modifier, key),
                    PauthOp::Strip => self.pauth.strip(ptr),
                };
                self.regs.write_x(rd, val);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Pacga { rd, rn, rm } => {
                let val = self.pauth.pacga(self.regs.read_x(rn), self.regs.read_x_or_sp(rm));
                self.regs.write_x(rd, val);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::LoadAuth {
                rt,
                rn,
                offset,
                key,
                writeback,
            } => {
                let base = self.pauth.auth(self.regs.read_x_or_sp(rn), 0, key);
                let addr = base.wrapping_add(offset as u64);
                let val = mmu.read_u64(addr)?;
                if writeback {
                    self.regs.write_x_or_sp(rn, addr);
                }
                self.regs.write_x(rt, val);
                self.regs.advance_pc();
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
                is_64_target,
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
                    if !is_64_target {
                        self.regs.write_w(rt, self.regs.read_w(rt));
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
                signed_word,
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
                        if signed_word {
                            self.regs.write_x(rt1, v1 as i32 as i64 as u64);
                            self.regs.write_x(rt2, v2 as i32 as i64 as u64);
                        } else {
                            self.regs.write_w(rt1, v1);
                            self.regs.write_w(rt2, v2);
                        }
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

            Instruction::SystemReg { rt, sysreg, is_read } => {
                // System registers:
                // 0x5E82 = TPIDR_EL0   (S3_3_C13_C0_2)
                // 0x5E83 = TPIDRRO_EL0 (S3_3_C13_C0_3) - Horizon TLS Pointer
                // 0x5A10 = NZCV        (S3_3_C4_C2_0)
                // 0x5F00 = CNTFRQ_EL0  (19,200,000 Hz Switch 2 hardware timer frequency)
                // 0x5F01 = CNTPCT_EL0  (Switch 2 physical timer counter)
                // 0x5807 = DCZID_EL0   (S3_3_C0_C0_7)
                // 0x5A20 = FPCR        (S3_3_C4_C4_0), stored only
                // 0x5A21 = FPSR        (S3_3_C4_C4_1), QC and the cumulative exception flags modelled
                if sysreg == 0x5A21 {
                    self.fp_flags_tracked = true;
                }
                if is_read {
                    let val = match sysreg {
                        0x5E82 => self.regs.tpidr_el0,
                        0x5E83 => self.regs.tpidrro_el0,
                        0x5A10 => self.regs.pstate.to_bits() as u64,
                        0x5A20 => self.regs.fpcr as u64,
                        0x5A21 => self.regs.fpsr as u64,
                        0x5F00 => 19_200_000,
                        // DCZID_EL0: DC ZVA permitted, block = 2^4 words = DC_ZVA_BLOCK bytes
                        0x5807 => DC_ZVA_BLOCK.trailing_zeros() as u64 - 2,
                        // ID_AA64ISAR1_EL1: API = 1, GPI = 1 (IMPLEMENTATION DEFINED PAC
                        // algorithm, see `pauth`); APA/GPA = 0 (not QARMA)
                        0x4031 => 1 << 8 | 1 << 28,
                        // ID_AA64PFR1_EL1: BT = 1 (FEAT_BTI)
                        0x4021 => 1,
                        0x5F01 => {
                            let now = self.counter_ticks;
                            self.counter_ticks = now.wrapping_add(COUNTER_READ_TICKS);
                            now
                        }
                        _ => 0,
                    };
                    self.regs.write_x(rt, val);
                } else {
                    let val = self.regs.read_x(rt);
                    match sysreg {
                        0x5E82 => self.regs.tpidr_el0 = val,
                        0x5E83 => self.regs.tpidrro_el0 = val,
                        0x5A10 => self.regs.pstate.set_bits(val as u32),
                        0x5A20 => self.regs.fpcr = val as u32,
                        0x5A21 => self.regs.fpsr = val as u32 & super::registers::FPSR_MASK,
                        _ => {}
                    }
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpData2 { rd, rn, rm, op, esize } => {
                if esize == 16 {
                    let f = softfp::F16;
                    let env = softfp::Env::from_fpcr_f16(self.regs.fpcr);
                    let a = (self.regs.v[rn as usize] & 0xFFFF) as u64;
                    let b = (self.regs.v[rm as usize] & 0xFFFF) as u64;
                    let r = match op {
                        FpOp2::Fadd => softfp::binop(f, env, softfp::BinOp::Add, a, b),
                        FpOp2::Fsub => softfp::binop(f, env, softfp::BinOp::Sub, a, b),
                        FpOp2::Fmul => softfp::binop(f, env, softfp::BinOp::Mul, a, b),
                        FpOp2::Fdiv => softfp::binop(f, env, softfp::BinOp::Div, a, b),
                        FpOp2::Fnmul => f.neg(softfp::binop(f, env, softfp::BinOp::Mul, a, b)),
                        _ => unreachable!("FMAX/FMIN H decoded as FpHalfMinMax"),
                    };
                    self.regs.v[rd as usize] = r as u128;
                    self.regs.advance_pc();
                    return Ok(StepResult::Continue);
                }
                let is_double = esize == 64;
                if matches!(op, FpOp2::Fadd | FpOp2::Fsub | FpOp2::Fmul | FpOp2::Fdiv | FpOp2::Fnmul) {
                    let (f, env) = self.fp_env(is_double);
                    let (a, b) = (self.fp_bits(rn, is_double), self.fp_bits(rm, is_double));
                    let r = match op {
                        FpOp2::Fadd => softfp::binop(f, env, softfp::BinOp::Add, a, b),
                        FpOp2::Fsub => softfp::binop(f, env, softfp::BinOp::Sub, a, b),
                        FpOp2::Fmul => softfp::binop(f, env, softfp::BinOp::Mul, a, b),
                        FpOp2::Fdiv => softfp::binop(f, env, softfp::BinOp::Div, a, b),
                        _ => f.neg(softfp::binop(f, env, softfp::BinOp::Mul, a, b)),
                    };
                    self.set_fp_bits(rd, is_double, r);
                    self.regs.advance_pc();
                    return Ok(StepResult::Continue);
                }
                // FMAX/FMIN/FMAXNM/FMINNM: only the NaN rules and FZ differ from the host, so only the flags are soft
                {
                    let (f, env) = self.fp_env(is_double);
                    softfp::raise_minmax(f, env, self.fp_bits(rn, is_double), self.fp_bits(rm, is_double));
                }
                if is_double {
                    let (mut n, mut m) = (self.regs.read_d(rn), self.regs.read_d(rm));
                    let minmax = matches!(op, FpOp2::Fmax | FpOp2::Fmin | FpOp2::Fmaxnm | FpOp2::Fminnm);
                    if minmax && self.regs.fpcr & (1 << 24) != 0 {
                        // FPCR.FZ: denormal inputs read as signed zero
                        if n != 0.0 && n.is_subnormal() {
                            n = 0.0f64.copysign(n);
                        }
                        if m != 0.0 && m.is_subnormal() {
                            m = 0.0f64.copysign(m);
                        }
                    }
                    let res = match op {
                        FpOp2::Fadd | FpOp2::Fsub | FpOp2::Fmul | FpOp2::Fdiv | FpOp2::Fnmul => {
                            unreachable!("arithmetic takes the soft-float path")
                        }
                        FpOp2::Fmax => {
                            if n.is_nan() || m.is_nan() {
                                FpNum::process_nans(&[n, m]).unwrap_or(n)
                            } else if n == 0.0 && m == 0.0 {
                                if n.is_sign_positive() || m.is_sign_positive() {
                                    0.0
                                } else {
                                    -0.0
                                }
                            } else {
                                n.max(m)
                            }
                        }
                        FpOp2::Fmin => {
                            if n.is_nan() || m.is_nan() {
                                FpNum::process_nans(&[n, m]).unwrap_or(n)
                            } else if n == 0.0 && m == 0.0 {
                                if n.is_sign_negative() || m.is_sign_negative() {
                                    -0.0
                                } else {
                                    0.0
                                }
                            } else {
                                n.min(m)
                            }
                        }
                        FpOp2::Fmaxnm => {
                            if let Some(r) = FpNum::nm_nan(n, m) {
                                r
                            } else if n == 0.0 && m == 0.0 {
                                if n.is_sign_positive() || m.is_sign_positive() {
                                    0.0
                                } else {
                                    -0.0
                                }
                            } else {
                                n.max(m)
                            }
                        }
                        FpOp2::Fminnm => {
                            if let Some(r) = FpNum::nm_nan(n, m) {
                                r
                            } else if n == 0.0 && m == 0.0 {
                                if n.is_sign_negative() || m.is_sign_negative() {
                                    -0.0
                                } else {
                                    0.0
                                }
                            } else {
                                n.min(m)
                            }
                        }
                    };
                    let res = if minmax && self.regs.fpcr & (1 << 25) != 0 && res.is_nan() {
                        f64::from_bits(0x7ff8_0000_0000_0000)
                    } else {
                        res
                    };
                    self.regs.write_d(rd, res);
                } else {
                    let (mut n, mut m) = (self.regs.read_s(rn), self.regs.read_s(rm));
                    let minmax = matches!(op, FpOp2::Fmax | FpOp2::Fmin | FpOp2::Fmaxnm | FpOp2::Fminnm);
                    if minmax && self.regs.fpcr & (1 << 24) != 0 {
                        if n != 0.0 && n.is_subnormal() {
                            n = 0.0f32.copysign(n);
                        }
                        if m != 0.0 && m.is_subnormal() {
                            m = 0.0f32.copysign(m);
                        }
                    }
                    let res = match op {
                        FpOp2::Fadd | FpOp2::Fsub | FpOp2::Fmul | FpOp2::Fdiv | FpOp2::Fnmul => {
                            unreachable!("arithmetic takes the soft-float path")
                        }
                        FpOp2::Fmax => {
                            if n.is_nan() || m.is_nan() {
                                FpNum::process_nans(&[n, m]).unwrap_or(n)
                            } else if n == 0.0 && m == 0.0 {
                                if n.is_sign_positive() || m.is_sign_positive() {
                                    0.0
                                } else {
                                    -0.0
                                }
                            } else {
                                n.max(m)
                            }
                        }
                        FpOp2::Fmin => {
                            if n.is_nan() || m.is_nan() {
                                FpNum::process_nans(&[n, m]).unwrap_or(n)
                            } else if n == 0.0 && m == 0.0 {
                                if n.is_sign_negative() || m.is_sign_negative() {
                                    -0.0
                                } else {
                                    0.0
                                }
                            } else {
                                n.min(m)
                            }
                        }
                        FpOp2::Fmaxnm => {
                            if let Some(r) = FpNum::nm_nan(n, m) {
                                r
                            } else if n == 0.0 && m == 0.0 {
                                if n.is_sign_positive() || m.is_sign_positive() {
                                    0.0
                                } else {
                                    -0.0
                                }
                            } else {
                                n.max(m)
                            }
                        }
                        FpOp2::Fminnm => {
                            if let Some(r) = FpNum::nm_nan(n, m) {
                                r
                            } else if n == 0.0 && m == 0.0 {
                                if n.is_sign_negative() || m.is_sign_negative() {
                                    -0.0
                                } else {
                                    0.0
                                }
                            } else {
                                n.min(m)
                            }
                        }
                    };
                    let res = if minmax && self.regs.fpcr & (1 << 25) != 0 && res.is_nan() {
                        f32::from_bits(0x7fc0_0000)
                    } else {
                        res
                    };
                    self.regs.write_s(rd, res);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpData1 { rd, rn, op, esize } => {
                if op == 3 {
                    if esize == 16 {
                        let env = softfp::Env::from_fpcr_f16(self.regs.fpcr);
                        let a = (self.regs.v[rn as usize] & 0xFFFF) as u64;
                        self.regs.v[rd as usize] = softfp::sqrt(softfp::F16, env, a) as u128;
                    } else {
                        let is_double = esize == 64;
                        let (f, env) = self.fp_env(is_double);
                        let a = self.fp_bits(rn, is_double);
                        self.set_fp_bits(rd, is_double, softfp::sqrt(f, env, a));
                    }
                    self.regs.advance_pc();
                    return Ok(StepResult::Continue);
                }
                if matches!(op, 4 | 5 | 7) {
                    let env = softfp::Env::from_fpcr(self.regs.fpcr);
                    let (src_fmt, src_bits) = match esize {
                        16 => (softfp::F16, (self.regs.v[rn as usize] & 0xFFFF) as u64),
                        32 => (softfp::F32, self.fp_bits(rn, false)),
                        _ => (softfp::F64, self.fp_bits(rn, true)),
                    };
                    match op {
                        4 => self.set_fp_bits(rd, false, softfp::convert(src_fmt, softfp::F32, env, src_bits)),
                        5 => self.set_fp_bits(rd, true, softfp::convert(src_fmt, softfp::F64, env, src_bits)),
                        _ => {
                            self.regs.v[rd as usize] = softfp::convert(src_fmt, softfp::F16, env, src_bits) as u128;
                        }
                    }
                    self.regs.advance_pc();
                    return Ok(StepResult::Continue);
                }
                match (op, esize) {
                    (0, 16) => self.regs.v[rd as usize] = self.regs.v[rn as usize] & 0xFFFF,
                    (1, 16) => self.regs.v[rd as usize] = self.regs.v[rn as usize] & 0x7FFF,
                    (2, 16) => self.regs.v[rd as usize] = (self.regs.v[rn as usize] & 0xFFFF) ^ 0x8000,
                    (0, 32) => self.regs.write_s(rd, self.regs.read_s(rn)),
                    (0, 64) => self.regs.write_d(rd, self.regs.read_d(rn)),
                    (1, 32) => self.regs.write_s(rd, self.regs.read_s(rn).abs()),
                    (1, 64) => self.regs.write_d(rd, self.regs.read_d(rn).abs()),
                    (2, 32) => self.regs.write_s(rd, -self.regs.read_s(rn)),
                    (2, 64) => self.regs.write_d(rd, -self.regs.read_d(rn)),
                    _ => {
                        return Err(CpuError::UnimplementedInstruction {
                            raw: 0,
                            pc: self.regs.pc,
                        });
                    }
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpData3 {
                rd,
                rn,
                rm,
                ra,
                o1,
                o0,
                esize,
            } => {
                if esize == 16 {
                    let f = softfp::F16;
                    let env = softfp::Env::from_fpcr_f16(self.regs.fpcr);
                    let n = (self.regs.v[rn as usize] & 0xFFFF) as u64;
                    let m = (self.regs.v[rm as usize] & 0xFFFF) as u64;
                    let a = (self.regs.v[ra as usize] & 0xFFFF) as u64;
                    let n = if o0 != o1 { f.neg(n) } else { n };
                    let a = if o1 { f.neg(a) } else { a };
                    self.regs.v[rd as usize] = softfp::fma(f, env, n, m, a) as u128;
                } else {
                    let is_double = esize == 64;
                    let (f, env) = self.fp_env(is_double);
                    let (n, m, a) = (
                        self.fp_bits(rn, is_double),
                        self.fp_bits(rm, is_double),
                        self.fp_bits(ra, is_double),
                    );
                    let n = if o0 != o1 { f.neg(n) } else { n };
                    let a = if o1 { f.neg(a) } else { a };
                    self.set_fp_bits(rd, is_double, softfp::fma(f, env, n, m, a));
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpRound { rd, rn, op, is_double } => {
                // FRINTX/FRINTI (op 6, 7) round with FPCR.RMode; with FPCR.FZ a denormal input reads as zero
                let (f, env) = self.fp_env(is_double);
                let inexact_flag = op == 6; // FRINTX raises IXC, FRINTI does not
                let op = if op >= 6 {
                    match env.round {
                        softfp::Round::Nearest => 0,
                        softfp::Round::Up => 1,
                        softfp::Round::Down => 2,
                        softfp::Round::Zero => 3,
                        softfp::Round::Odd => unreachable!("round-to-odd is not an FPCR mode"),
                    }
                } else {
                    op
                };
                let flushed = softfp::flush_input(f, env, self.fp_bits(rn, is_double));
                if is_double {
                    let v = f64::from_bits(flushed);
                    // Arm FPRoundInt: a NaN operand is returned quieted (a signalling NaN gets its quiet bit set)
                    let r = if v.is_nan() {
                        if env.dn {
                            FpNum::default_nan_if_nan(v)
                        } else {
                            v.quieted()
                        }
                    } else {
                        match op {
                            1 => v.ceil(),
                            2 => v.floor(),
                            3 => v.trunc(),
                            4 => v.round(),
                            _ => v.round_ties_even(),
                        }
                    };
                    softfp::raise_round_int(f, flushed, v, r, inexact_flag);
                    self.regs.write_d(rd, r);
                } else {
                    let v = f32::from_bits(flushed as u32);
                    // Arm FPRoundInt: a NaN operand is returned quieted (a signalling NaN gets its quiet bit set)
                    let r = if v.is_nan() {
                        if env.dn {
                            FpNum::default_nan_if_nan(v)
                        } else {
                            v.quieted()
                        }
                    } else {
                        match op {
                            1 => v.ceil(),
                            2 => v.floor(),
                            3 => v.trunc(),
                            4 => v.round(),
                            _ => v.round_ties_even(),
                        }
                    };
                    softfp::raise_round_int(f, flushed, v as f64, r as f64, inexact_flag);
                    self.regs.write_s(rd, r);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpIntConv {
                rd,
                rn,
                opcode,
                rmode,
                is_64,
                is_double,
            } => {
                if opcode == 2 || opcode == 3 {
                    // SCVTF / UCVTF: integer register to float
                    let signed = opcode == 2;
                    let x = if is_64 {
                        self.regs.read_x(rn)
                    } else {
                        self.regs.read_w(rn) as u64
                    };
                    let x = if signed && !is_64 {
                        x as u32 as i32 as i64 as u64
                    } else {
                        x
                    };
                    let (f, env) = self.fp_env(is_double);
                    let neg = signed && (x as i64) < 0;
                    let mag = if neg { (x as i64).unsigned_abs() } else { x };
                    self.set_fp_bits(rd, is_double, softfp::int_to_fp(f, env, neg, mag, 0));
                } else {
                    // float to integer with the rounding mode in rmode (opcodes 4/5 are ties-away)
                    let (f, env) = self.fp_env(is_double);
                    let src = softfp::flush_input(f, env, self.fp_bits(rn, is_double));
                    let v = if is_double {
                        f64::from_bits(src)
                    } else {
                        f32::from_bits(src as u32) as f64
                    };
                    let rounded = if opcode >= 4 {
                        v.round()
                    } else {
                        match rmode {
                            0 => v.round_ties_even(),
                            1 => v.ceil(),
                            2 => v.floor(),
                            _ => v.trunc(),
                        }
                    };
                    let signed = opcode == 0 || opcode == 4;
                    softfp::raise_to_int(f, src, v, rounded, signed, is_64);
                    let out: u64 = match (signed, is_64) {
                        (true, true) => rounded as i64 as u64,
                        (true, false) => rounded as i32 as u32 as u64,
                        (false, true) => rounded as u64,
                        (false, false) => rounded as u32 as u64,
                    };
                    self.regs.write_x(rd, out);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpFixedConv {
                rd,
                rn,
                opcode,
                fbits,
                is_64,
                is_double,
            } => {
                let scale = 2f64.powi(fbits as i32);
                if opcode >= 2 {
                    // SCVTF / UCVTF: fixed-point integer register to float
                    let x = if is_64 {
                        self.regs.read_x(rn)
                    } else {
                        self.regs.read_w(rn) as u64
                    };
                    let (f, env) = self.fp_env(is_double);
                    let sx = if is_64 { x as i64 } else { x as u32 as i32 as i64 };
                    let neg = opcode == 2 && sx < 0;
                    let mag = if opcode == 2 {
                        sx.unsigned_abs()
                    } else if is_64 {
                        x
                    } else {
                        x as u32 as u64
                    };
                    self.set_fp_bits(rd, is_double, softfp::int_to_fp(f, env, neg, mag, fbits as u32));
                } else {
                    // FCVTZS / FCVTZU: scale, then truncate toward zero with saturation
                    let (f, env) = self.fp_env(is_double);
                    let src = softfp::flush_input(f, env, self.fp_bits(rn, is_double));
                    let v = if is_double {
                        f64::from_bits(src)
                    } else {
                        f32::from_bits(src as u32) as f64
                    };
                    let t = (v * scale).trunc();
                    softfp::raise_to_int(f, src, v * scale, t, opcode == 0, is_64);
                    let out: u64 = match (opcode == 0, is_64) {
                        (true, true) => t as i64 as u64,
                        (true, false) => t as i32 as u32 as u64,
                        (false, true) => t as u64,
                        (false, false) => t as u32 as u64,
                    };
                    self.regs.write_x(rd, out);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpCondSelect {
                rd,
                rn,
                rm,
                cond,
                esize,
            } => {
                let src = if self.check_condition(cond) { rn } else { rm };
                let mask: u128 = match esize {
                    16 => u16::MAX as u128,
                    32 => u32::MAX as u128,
                    _ => u64::MAX as u128,
                };
                self.regs.write_q(rd, self.regs.read_q(src) & mask);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpCompare {
                rn,
                rm,
                is_zero,
                esize,
                signalling,
            } => {
                let (n_f, z_f, c_f, v_f) = if esize == 16 {
                    let env = softfp::Env::from_fpcr_f16(self.regs.fpcr);
                    let a = softfp::flush_input(softfp::F16, env, (self.regs.v[rn as usize] & 0xFFFF) as u64);
                    let b = if is_zero {
                        0
                    } else {
                        softfp::flush_input(softfp::F16, env, (self.regs.v[rm as usize] & 0xFFFF) as u64)
                    };
                    softfp::raise_compare(softfp::F16, a, b, signalling);
                    softfp::compare_f16_nzcv(a, b)
                } else {
                    let is_double = esize == 64;
                    let (f, _) = self.fp_env(is_double);
                    let b_bits = if is_zero { 0 } else { self.fp_bits(rm, is_double) };
                    softfp::raise_compare(f, self.fp_bits(rn, is_double), b_bits, signalling);
                    let (n, m) = if is_zero {
                        (self.fp_cmp_operand(rn, is_double), 0.0)
                    } else {
                        (self.fp_cmp_operand(rn, is_double), self.fp_cmp_operand(rm, is_double))
                    };
                    if n.is_nan() || m.is_nan() {
                        (false, false, true, true)
                    } else if n == m {
                        (false, true, true, false)
                    } else if n < m {
                        (true, false, false, false)
                    } else {
                        (false, false, true, false)
                    }
                };
                self.regs.pstate.n = n_f;
                self.regs.pstate.z = z_f;
                self.regs.pstate.c = c_f;
                self.regs.pstate.v = v_f;

                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::AddSubCarry {
                rd,
                rn,
                rm,
                is_sub,
                set_flags,
                is_64,
            } => {
                let bits = if is_64 { 64 } else { 32 };
                let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
                let a = self.regs.read_x(rn) & mask;
                let b = self.regs.read_x(rm) & mask;
                let b = if is_sub { !b & mask } else { b };
                let carry_in = self.regs.pstate.c as u128;
                let wide = a as u128 + b as u128 + carry_in;
                let res = (wide as u64) & mask;
                if set_flags {
                    let sign = 1u64 << (bits - 1);
                    self.regs.pstate.n = res & sign != 0;
                    self.regs.pstate.z = res == 0;
                    self.regs.pstate.c = wide >> bits != 0;
                    self.regs.pstate.v = (!(a ^ b) & (a ^ res) & sign) != 0;
                }
                self.regs.write_x(rd, res);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpCondCompare {
                rn,
                rm,
                nzcv,
                cond,
                esize,
                signalling,
            } => {
                let (n, z, c, v) = if self.check_condition(cond) {
                    if esize == 16 {
                        let env = softfp::Env::from_fpcr_f16(self.regs.fpcr);
                        let a = softfp::flush_input(softfp::F16, env, (self.regs.v[rn as usize] & 0xFFFF) as u64);
                        let b = softfp::flush_input(softfp::F16, env, (self.regs.v[rm as usize] & 0xFFFF) as u64);
                        softfp::raise_compare(softfp::F16, a, b, signalling);
                        softfp::compare_f16_nzcv(a, b)
                    } else {
                        let is_double = esize == 64;
                        let (f, _) = self.fp_env(is_double);
                        softfp::raise_compare(f, self.fp_bits(rn, is_double), self.fp_bits(rm, is_double), signalling);
                        let (a, b) = (self.fp_cmp_operand(rn, is_double), self.fp_cmp_operand(rm, is_double));
                        if a.is_nan() || b.is_nan() {
                            (false, false, true, true)
                        } else if a == b {
                            (false, true, true, false)
                        } else if a < b {
                            (true, false, false, false)
                        } else {
                            (false, false, true, false)
                        }
                    }
                } else {
                    (nzcv & 8 != 0, nzcv & 4 != 0, nzcv & 2 != 0, nzcv & 1 != 0)
                };
                self.regs.pstate.n = n;
                self.regs.pstate.z = z;
                self.regs.pstate.c = c;
                self.regs.pstate.v = v;
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpMovGen { rd, rn, to_fp, is_64 } => {
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

            Instruction::FpMovGenHalf { rd, rn, to_fp, is_64 } => {
                // a bit-pattern move of the low 16 bits; everything above them in the destination is zero
                if to_fp {
                    let bits = self.regs.read_x(rn) & 0xFFFF;
                    self.regs.write_q(rd, bits as u128);
                } else {
                    let bits = (self.regs.read_q(rn) & 0xFFFF) as u64;
                    if is_64 {
                        self.regs.write_x(rd, bits);
                    } else {
                        self.regs.write_w(rd, bits as u32);
                    }
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpRoundHalf { rd, rn, op } => {
                // FRINTX/FRINTI (op 6, 7) round with FPCR.RMode; with FPCR.FZ16 a denormal input reads as zero
                let f = softfp::F16;
                let env = softfp::Env::from_fpcr_f16(self.regs.fpcr);
                let inexact_flag = op == 6; // FRINTX raises IXC, FRINTI does not
                let mode = if op >= 6 {
                    match env.round {
                        softfp::Round::Nearest => 0,
                        softfp::Round::Up => 1,
                        softfp::Round::Down => 2,
                        softfp::Round::Zero => 3,
                        softfp::Round::Odd => unreachable!("round-to-odd is not an FPCR mode"),
                    }
                } else {
                    op
                };
                let src = softfp::flush_input(f, env, self.regs.read_q(rn) as u64 & 0xFFFF);
                if f.is_nan(src) {
                    // FPRoundInt: a NaN operand comes back quieted (the quiet bit of a half is bit 9); DN gives the default
                    softfp::raise_round_int(f, src, 0.0, 0.0, false);
                    let nan = if env.dn { f.default_nan() } else { src | 0x200 };
                    self.regs.write_q(rd, nan as u128);
                } else {
                    // a half is exact in f64, and an integral value of a half is again exact as a half
                    let widen = softfp::Env { fz: false, ..env };
                    let v = f64::from_bits(softfp::convert(f, softfp::F64, widen, src));
                    let r = match mode {
                        1 => v.ceil(),
                        2 => v.floor(),
                        3 => v.trunc(),
                        4 => v.round(),
                        _ => v.round_ties_even(),
                    };
                    softfp::raise_round_int(f, src, v, r, inexact_flag);
                    let bits = softfp::convert(softfp::F64, f, softfp::Env::DEFAULT, r.to_bits());
                    self.regs.write_q(rd, bits as u128);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpIntConvHalf {
                rd,
                rn,
                opcode,
                rmode,
                is_64,
            } => {
                let f = softfp::F16;
                let env = softfp::Env::from_fpcr_f16(self.regs.fpcr);
                if opcode == 2 || opcode == 3 {
                    // SCVTF / UCVTF: integer register to half, rounded under FPCR.RMode
                    let signed = opcode == 2;
                    let x = if is_64 {
                        self.regs.read_x(rn)
                    } else {
                        self.regs.read_w(rn) as u64
                    };
                    let x = if signed && !is_64 {
                        x as u32 as i32 as i64 as u64
                    } else {
                        x
                    };
                    let neg = signed && (x as i64) < 0;
                    let mag = if neg { (x as i64).unsigned_abs() } else { x };
                    self.regs.write_q(rd, softfp::int_to_fp(f, env, neg, mag, 0) as u128);
                } else {
                    // half to integer with the rounding mode in rmode (opcodes 4/5 are ties-away)
                    let src = softfp::flush_input(f, env, self.regs.read_q(rn) as u64 & 0xFFFF);
                    let widen = softfp::Env { fz: false, ..env };
                    let v = f64::from_bits(softfp::convert(f, softfp::F64, widen, src));
                    let rounded = if opcode >= 4 {
                        v.round()
                    } else {
                        match rmode {
                            0 => v.round_ties_even(),
                            1 => v.ceil(),
                            2 => v.floor(),
                            _ => v.trunc(),
                        }
                    };
                    let signed = opcode == 0 || opcode == 4;
                    softfp::raise_to_int(f, src, v, rounded, signed, is_64);
                    let out: u64 = match (signed, is_64) {
                        (true, true) => rounded as i64 as u64,
                        (true, false) => rounded as i32 as u32 as u64,
                        (false, true) => rounded as u64,
                        (false, false) => rounded as u32 as u64,
                    };
                    self.regs.write_x(rd, out);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpFixedConvHalf {
                rd,
                rn,
                opcode,
                fbits,
                is_64,
            } => {
                let f = softfp::F16;
                let env = softfp::Env::from_fpcr_f16(self.regs.fpcr);
                if opcode >= 2 {
                    // SCVTF / UCVTF: fixed-point integer register to half
                    let x = if is_64 {
                        self.regs.read_x(rn)
                    } else {
                        self.regs.read_w(rn) as u64
                    };
                    let sx = if is_64 { x as i64 } else { x as u32 as i32 as i64 };
                    let neg = opcode == 2 && sx < 0;
                    let mag = if opcode == 2 {
                        sx.unsigned_abs()
                    } else if is_64 {
                        x
                    } else {
                        x as u32 as u64
                    };
                    self.regs
                        .write_q(rd, softfp::int_to_fp(f, env, neg, mag, fbits as u32) as u128);
                } else {
                    // FCVTZS / FCVTZU: scale, then truncate toward zero with saturation
                    let src = softfp::flush_input(f, env, self.regs.read_q(rn) as u64 & 0xFFFF);
                    let widen = softfp::Env { fz: false, ..env };
                    let v = f64::from_bits(softfp::convert(f, softfp::F64, widen, src));
                    let scaled = v * 2f64.powi(fbits as i32);
                    let t = scaled.trunc();
                    softfp::raise_to_int(f, src, scaled, t, opcode == 0, is_64);
                    let out: u64 = match (opcode == 0, is_64) {
                        (true, true) => t as i64 as u64,
                        (true, false) => t as i32 as u32 as u64,
                        (false, true) => t as u64,
                        (false, false) => t as u32 as u64,
                    };
                    self.regs.write_x(rd, out);
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
                    let shift = self.regs.read_w(rm) & 31;
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

            Instruction::SimdVec2D { rd, rn, rm, op } => {
                let a = self.regs.read_q(rn);
                let b = self.regs.read_q(rm);
                // FADD/FSUB/FMUL/FDIV share the lane code with the generic FP three-same arm so NaN selection
                // (signalling first, then operand order) and the default NaN match Arm instead of the host.
                let shared = match op {
                    FpOp2::Fadd => Some(crate::cpu::instructions::SimdFpOp3::Fadd),
                    FpOp2::Fsub => Some(crate::cpu::instructions::SimdFpOp3::Fsub),
                    FpOp2::Fmul => Some(crate::cpu::instructions::SimdFpOp3::Fmul),
                    FpOp2::Fdiv => Some(crate::cpu::instructions::SimdFpOp3::Fdiv),
                    _ => None,
                };
                let mut out = 0u128;
                if let Some(s) = shared {
                    out = simd::fp_three_same_fpcr(s, 0, a, b, true, true, self.regs.fpcr);
                } else {
                    for lane in 0..2 {
                        let n = f64::from_bits((a >> (lane * 64)) as u64);
                        let m = f64::from_bits((b >> (lane * 64)) as u64);
                        let value = match op {
                            FpOp2::Fnmul => -(n * m),
                            FpOp2::Fmax => {
                                if n.is_nan() || m.is_nan() {
                                    n + m
                                } else {
                                    n.max(m)
                                }
                            }
                            FpOp2::Fmin => {
                                if n.is_nan() || m.is_nan() {
                                    n + m
                                } else {
                                    n.min(m)
                                }
                            }
                            FpOp2::Fmaxnm => {
                                if let Some(r) = FpNum::nm_nan(n, m) {
                                    r
                                } else if n == 0.0 && m == 0.0 {
                                    if n.is_sign_positive() || m.is_sign_positive() {
                                        0.0
                                    } else {
                                        -0.0
                                    }
                                } else {
                                    n.max(m)
                                }
                            }
                            _ => {
                                if let Some(r) = FpNum::nm_nan(n, m) {
                                    r
                                } else if n == 0.0 && m == 0.0 {
                                    if n.is_sign_negative() || m.is_sign_negative() {
                                        -0.0
                                    } else {
                                        0.0
                                    }
                                } else {
                                    n.min(m)
                                }
                            }
                        };
                        out |= (value.to_bits() as u128) << (lane * 64);
                    }
                }
                self.regs.write_q(rd, out);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdVec4S { rd, rn, rm, op } => {
                let shared = match op {
                    FpOp2::Fadd => Some(crate::cpu::instructions::SimdFpOp3::Fadd),
                    FpOp2::Fsub => Some(crate::cpu::instructions::SimdFpOp3::Fsub),
                    FpOp2::Fmul => Some(crate::cpu::instructions::SimdFpOp3::Fmul),
                    FpOp2::Fdiv => Some(crate::cpu::instructions::SimdFpOp3::Fdiv),
                    _ => None,
                };
                if let Some(s) = shared {
                    let (a, b) = (self.regs.read_q(rn), self.regs.read_q(rm));
                    self.regs
                        .write_q(rd, simd::fp_three_same_fpcr(s, 0, a, b, false, true, self.regs.fpcr));
                    self.regs.advance_pc();
                    return Ok(StepResult::Continue);
                }
                let a = self.regs.read_v4s(rn);
                let b = self.regs.read_v4s(rm);
                let mut out = [0.0f32; 4];
                for i in 0..4 {
                    out[i] = match op {
                        FpOp2::Fnmul => -(a[i] * b[i]),
                        FpOp2::Fmax | FpOp2::Fmaxnm => a[i].max(b[i]),
                        _ => a[i].min(b[i]),
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

            Instruction::SimdLoadStorePair {
                rt1,
                rt2,
                rn,
                offset,
                size_bytes,
                is_load,
                writeback,
                post_index,
            } => {
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
                let pattern = if q {
                    imm as u128 | (imm as u128) << 64
                } else {
                    imm as u128
                };
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

            Instruction::SimdDupScalar { rd, rn, esize, index } => {
                let v = simd::lane(self.regs.read_q(rn), esize as u32, index as usize);
                self.regs.write_q(rd, v as u128);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdDupElement {
                rd,
                rn,
                esize,
                index,
                q,
            } => {
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

            Instruction::SimdInsElement {
                rd,
                rn,
                esize,
                dst_index,
                src_index,
            } => {
                let v = simd::lane(self.regs.read_q(rn), esize as u32, src_index as usize);
                let mut d = self.regs.read_q(rd);
                simd::set_lane(&mut d, esize as u32, dst_index as usize, v);
                self.regs.write_q(rd, d);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLoadStoreSingle {
                rt,
                rn,
                rm,
                esize,
                index,
                replicate,
                q,
                is_load,
                post_index,
                nregs,
            } => {
                let base = self.regs.read_x_or_sp(rn);
                let n = esize as usize / 8;
                // The elements of one structure are consecutive in memory; register (rt + i) mod 32 takes element i.
                // Load and store all of them before writing any register or the base, so a fault changes nothing.
                let mut vals = [0u64; 4];
                if is_load {
                    for (i, val) in vals.iter_mut().enumerate().take(nregs as usize) {
                        let mut buf = [0u8; 8];
                        mmu.read_bytes(base.wrapping_add((i * n) as u64), &mut buf[..n])?;
                        *val = u64::from_le_bytes(buf);
                    }
                    for (i, &v) in vals.iter().enumerate().take(nregs as usize) {
                        let r = ((rt as usize + i) % 32) as u8;
                        let res = if replicate {
                            simd::dup(v, esize as u32, q)
                        } else {
                            let mut d = self.regs.read_q(r);
                            simd::set_lane(&mut d, esize as u32, index as usize, v);
                            d
                        };
                        self.regs.write_q(r, res);
                    }
                } else {
                    for (i, val) in vals.iter_mut().enumerate().take(nregs as usize) {
                        let r = ((rt as usize + i) % 32) as u8;
                        *val = simd::lane(self.regs.read_q(r), esize as u32, index as usize);
                    }
                    for (i, v) in vals.iter().enumerate().take(nregs as usize) {
                        mmu.write_bytes(base.wrapping_add((i * n) as u64), &v.to_le_bytes()[..n])?;
                    }
                }
                if post_index {
                    let step = if rm == 31 {
                        (n * nregs as usize) as u64
                    } else {
                        self.regs.read_x(rm)
                    };
                    self.regs.write_x_or_sp(rn, base.wrapping_add(step));
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdPermute {
                rd,
                rn,
                rm,
                op,
                esize,
                q,
            } => {
                let (n, m) = (self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs.write_q(rd, simd::permute(op, n, m, esize as u32, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Fcadd {
                rd,
                rn,
                rm,
                esize,
                q,
                rot90,
            } => {
                let (n, m) = (self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs
                    .write_q(rd, simd::fcadd(n, m, esize as u32, q, rot90, self.regs.fpcr));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdFpThreeSame {
                rd,
                rn,
                rm,
                op,
                double,
                q,
            } => {
                let (d, n, m) = (self.regs.read_q(rd), self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs
                    .write_q(rd, simd::fp_three_same_fpcr(op, d, n, m, double, q, self.regs.fpcr));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdFpTwoMisc {
                rd,
                rn,
                op,
                double,
                q,
                scalar,
            } => {
                let (d, n) = (self.regs.read_q(rd), self.regs.read_q(rn));
                let res = if scalar {
                    simd::fcvtxn_f32(simd::lane(n, 64, 0), self.regs.fpcr) as u128
                } else {
                    simd::fp_two_misc_fpcr(op, d, n, double, q, self.regs.fpcr)
                };
                self.regs.write_q(rd, res);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdFpByElement {
                rd,
                rn,
                rm,
                index,
                op,
                double,
                q,
                scalar,
            } => {
                let (mut d, mut n, m) = (self.regs.read_q(rd), self.regs.read_q(rn), self.regs.read_q(rm));
                if scalar {
                    // a scalar op reads lane 0 only: replicate it so the unread lanes cannot raise exception flags
                    let e = if double { 64 } else { 32 };
                    n = simd::dup(simd::lane(n, e, 0), e, true);
                    d = simd::dup(simd::lane(d, e, 0), e, true);
                }
                let mut res = simd::fp_by_element_fpcr(op, d, n, m, index as usize, double, q, self.regs.fpcr);
                if scalar {
                    res &= simd::lane_mask(if double { 64 } else { 32 });
                }
                self.regs.write_q(rd, res);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdByElement {
                rd,
                rn,
                rm,
                index,
                op,
                esize,
                q,
                scalar,
            } => {
                let (d, n, m) = (self.regs.read_q(rd), self.regs.read_q(rn), self.regs.read_q(rm));
                let (r, qc) = simd::by_element(op, d, n, m, index, esize as u32, q, scalar);
                if qc {
                    self.regs.fpsr |= super::registers::FPSR_QC;
                }
                self.regs.write_q(rd, r);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdExt { rd, rn, rm, index, q } => {
                let (n, m) = (self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs.write_q(rd, simd::extract(n, m, index as u32, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdTbl {
                rd,
                rn,
                rm,
                regs,
                is_tbx,
                q,
            } => {
                let mut table = [0u128; 4];
                for (i, t) in table.iter_mut().take(regs as usize).enumerate() {
                    *t = self.regs.read_q(rn.wrapping_add(i as u8) & 0x1F);
                }
                let res = simd::table_lookup(
                    &table[..regs as usize],
                    self.regs.read_q(rm),
                    self.regs.read_q(rd),
                    is_tbx,
                    q,
                );
                self.regs.write_q(rd, res);
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

            Instruction::Crc32 {
                rd,
                rn,
                rm,
                size,
                castagnoli,
            } => {
                let poly: u32 = if castagnoli { 0x82F6_3B78 } else { 0xEDB8_8320 };
                let mut crc = self.regs.read_w(rn);
                let data = self.regs.read_x(rm);
                for i in 0..(1u32 << size) {
                    crc ^= ((data >> (8 * i)) & 0xFF) as u32;
                    for _ in 0..8 {
                        crc = if crc & 1 != 0 { (crc >> 1) ^ poly } else { crc >> 1 };
                    }
                }
                self.regs.write_w(rd, crc);
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

            Instruction::MultiplyLong {
                rd,
                rn,
                rm,
                ra,
                is_signed,
                is_sub,
            } => {
                let (n, m) = (self.regs.read_w(rn), self.regs.read_w(rm));
                let prod = if is_signed {
                    (n as i32 as i64).wrapping_mul(m as i32 as i64) as u64
                } else {
                    (n as u64).wrapping_mul(m as u64)
                };
                let a = self.regs.read_x(ra);
                self.regs.write_x(
                    rd,
                    if is_sub {
                        a.wrapping_sub(prod)
                    } else {
                        a.wrapping_add(prod)
                    },
                );
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLoadStore {
                rt,
                rn,
                offset,
                size_bytes,
                is_load,
                writeback,
                post_index,
            } => {
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

            Instruction::SimdLoadStoreReg {
                rt,
                rn,
                rm,
                option,
                shift,
                size_bytes,
                is_load,
            } => {
                let raw_m = self.regs.read_x(rm);
                let index: u64 = match option {
                    0b010 => (raw_m as u32) as u64,          // UXTW
                    0b110 => ((raw_m as i32) as i64) as u64, // SXTW
                    _ => raw_m,                              // LSL / SXTX
                };
                let addr = self
                    .regs
                    .read_x_or_sp(rn)
                    .wrapping_add(index.wrapping_shl(shift as u32));
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

            Instruction::SimdLoadStoreMulti {
                rt,
                rn,
                rm,
                regs,
                q,
                is_load,
                post_index,
            } => {
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
                    let step = if rm == 31 {
                        regs as u64 * per_reg as u64
                    } else {
                        self.regs.read_x(rm)
                    };
                    self.regs.write_x_or_sp(rn, base.wrapping_add(step));
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLoadStoreStruct {
                rt,
                rn,
                rm,
                selem,
                esize,
                q,
                is_load,
                post_index,
            } => {
                let base = self.regs.read_x_or_sp(rn);
                let ebytes = esize as usize / 8;
                let elements = if q { 16 } else { 8 } / ebytes;
                let regs: Vec<u8> = (0..selem).map(|s| (rt + s) % 32).collect();
                let mut vals: Vec<u128> = if is_load {
                    vec![0; regs.len()]
                } else {
                    regs.iter().map(|&r| self.regs.read_q(r)).collect()
                };
                for e in 0..elements {
                    for (s, val) in vals.iter_mut().enumerate() {
                        let addr = base.wrapping_add(((e * selem as usize + s) * ebytes) as u64);
                        if is_load {
                            let mut buf = [0u8; 8];
                            mmu.read_bytes(addr, &mut buf[..ebytes])?;
                            simd::set_lane(val, esize as u32, e, u64::from_le_bytes(buf));
                        } else {
                            let v = simd::lane(*val, esize as u32, e);
                            mmu.write_bytes(addr, &v.to_le_bytes()[..ebytes])?;
                        }
                    }
                }
                if is_load {
                    for (&r, &v) in regs.iter().zip(&vals) {
                        self.regs.write_q(r, v);
                    }
                }
                if post_index {
                    let step = if rm == 31 {
                        (selem as usize * elements * ebytes) as u64
                    } else {
                        self.regs.read_x(rm)
                    };
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

            Instruction::SimdSmov {
                rd,
                rn,
                esize,
                index,
                x,
            } => {
                let val = simd::lane(self.regs.read_q(rn), esize as u32, index as usize);
                let ext = (((val << (64 - esize as u32)) as i64) >> (64 - esize as u32)) as u64;
                if x {
                    self.regs.write_x(rd, ext);
                } else {
                    self.regs.write_w(rd, ext as u32);
                }
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Sha3 { rd, rn, rm, ra, op } => {
                use super::instructions::Sha3Op;
                let (n, m, a) = (self.regs.read_q(rn), self.regs.read_q(rm), self.regs.read_q(ra));
                let r = match op {
                    Sha3Op::Eor3 => n ^ m ^ a,
                    Sha3Op::Bcax => n ^ (m & !a),
                    Sha3Op::Rax1 | Sha3Op::Xar(_) => {
                        let mut r = 0u128;
                        for shift in [0, 64] {
                            let (n, m) = ((n >> shift) as u64, (m >> shift) as u64);
                            let lane = match op {
                                Sha3Op::Rax1 => n ^ m.rotate_left(1),
                                Sha3Op::Xar(imm) => (n ^ m).rotate_right(u32::from(imm)),
                                _ => unreachable!(),
                            };
                            r |= u128::from(lane) << shift;
                        }
                        r
                    }
                };
                self.regs.write_q(rd, r);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdThreeSame {
                rd,
                rn,
                rm,
                op,
                esize,
                q,
            } => {
                let (d, n, m) = (self.regs.read_q(rd), self.regs.read_q(rn), self.regs.read_q(rm));
                let (r, qc) = simd::three_same_qc(op, d, n, m, esize as u32, q);
                if qc {
                    self.regs.fpsr |= super::registers::FPSR_QC;
                }
                self.regs.write_q(rd, r);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdScalarThreeSame { rd, rn, rm, op, esize } => {
                let (d, n, m) = (self.regs.read_q(rd), self.regs.read_q(rn), self.regs.read_q(rm));
                let (r, qc) = simd::scalar_three_same(op, d, n, m, esize as u32);
                if qc {
                    self.regs.fpsr |= super::registers::FPSR_QC;
                }
                self.regs.write_q(rd, r);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdNarrow {
                rd,
                rn,
                rm,
                op,
                esize,
                q,
            } => {
                let (d, n, m) = (self.regs.read_q(rd), self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs.write_q(rd, simd::narrow_high(op, d, n, m, esize as u32, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdSatAddSub {
                rd,
                rn,
                rm,
                op,
                esize,
                q,
                scalar,
            } => {
                let (n, m) = (self.regs.read_q(rn), self.regs.read_q(rm));
                let (r, sat) = simd::sat_add_sub(op, n, m, esize as u32, q, scalar);
                if sat {
                    self.regs.fpsr |= super::registers::FPSR_QC;
                }
                self.regs.write_q(rd, r);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }
            Instruction::SimdSatLong {
                rd,
                rn,
                rm,
                op,
                esize,
                q,
            } => {
                let (d, n, m) = (self.regs.read_q(rd), self.regs.read_q(rn), self.regs.read_q(rm));
                let (r, sat) = simd::sat_doubling_mul_long(op, d, n, m, esize as u32, q);
                if sat {
                    self.regs.fpsr |= super::registers::FPSR_QC;
                }
                self.regs.write_q(rd, r);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdLong {
                rd,
                rn,
                rm,
                op,
                signed,
                esize,
                q,
            } => {
                let (d, n, m) = (self.regs.read_q(rd), self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs
                    .write_q(rd, simd::mul_long(op, d, n, m, esize as u32, q, signed));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Pmull { rd, rn, rm, wide, q } => {
                let (n, m) = (self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs.write_q(rd, simd::pmull(n, m, wide, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdFpThreeSameHalf { rd, rn, rm, op, q } => {
                let (n, m) = (self.regs.read_q(rn), self.regs.read_q(rm));
                self.regs
                    .write_q(rd, simd::fp16_three_same_fpcr(op, n, m, q, self.regs.fpcr));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdFpAbsNegHalf { rd, rn, neg, q } => {
                // pure sign-bit operations, including on NaNs: no exception flags and no FPCR dependence
                let sign: u128 = 0x8000_8000_8000_8000_8000_8000_8000_8000;
                let n = self.regs.read_q(rn);
                let r = if neg { n ^ sign } else { n & !sign };
                self.regs.write_q(rd, if q { r } else { r & u64::MAX as u128 });
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpHalfMinMax { rd, rn, rm, op } => {
                let (n, m) = (
                    self.regs.read_q(rn) as u64 & 0xffff,
                    self.regs.read_q(rm) as u64 & 0xffff,
                );
                let lane_op = match op {
                    FpOp2::Fmax => crate::cpu::instructions::SimdFpOp3::Fmax,
                    FpOp2::Fmin => crate::cpu::instructions::SimdFpOp3::Fmin,
                    FpOp2::Fmaxnm => crate::cpu::instructions::SimdFpOp3::Fmaxnm,
                    _ => crate::cpu::instructions::SimdFpOp3::Fminnm,
                };
                let r = simd::fp_reduce_fpcr(lane_op, (n | m << 16) as u128, 16, 2, self.regs.fpcr);
                self.regs.write_q(rd, r as u128);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdFpReduce {
                rd,
                rn,
                op,
                esize,
                lanes,
            } => {
                let r = simd::fp_reduce_fpcr(op, self.regs.read_q(rn), esize as u32, lanes as usize, self.regs.fpcr);
                self.regs.write_q(rd, r as u128);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdAcross { rd, rn, op, esize, q } => {
                self.regs
                    .write_q(rd, simd::across(op, self.regs.read_q(rn), esize as u32, q));
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpAbd { rd, rn, rm, is_double } => {
                // always the soft path: it raises the exception flags (IXC, IOC, ...) the host does at default FPCR too
                let (f, env) = self.fp_env(is_double);
                let (a, b) = (self.fp_bits(rn, is_double), self.fp_bits(rm, is_double));
                let r = f.abs(softfp::binop(f, env, softfp::BinOp::Sub, a, b));
                self.regs.write_q(rd, r as u128);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpMulx { rd, rn, rm, is_double } => {
                let esize = if is_double { 64 } else { 32 };
                let keep = simd::lane_mask(esize);
                // `q == false` still evaluates two 32-bit lanes. Mask the sources so leftover
                // upper bits cannot raise flags, then keep only lane 0.
                let raw = simd::fp_three_same_fpcr(
                    SimdFpOp3::Fmulx,
                    0,
                    self.regs.read_q(rn) & keep,
                    self.regs.read_q(rm) & keep,
                    is_double,
                    false,
                    self.regs.fpcr,
                );
                self.regs.write_q(rd, raw & keep);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpScalarThreeSame {
                rd,
                rn,
                rm,
                op,
                is_double,
            } => {
                let e = if is_double { 64 } else { 32 };
                let keep = simd::lane_mask(e);
                // a scalar op reads lane 0 only: replicate it so the unread lane cannot raise exception flags
                let n = simd::dup(simd::lane(self.regs.read_q(rn), e, 0), e, true);
                let m = simd::dup(simd::lane(self.regs.read_q(rm), e, 0), e, true);
                let raw = simd::fp_three_same_fpcr(op, 0, n, m, is_double, false, self.regs.fpcr);
                self.regs.write_q(rd, raw & keep);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::FpScalarTwoMisc { rd, rn, op, is_double } => {
                let e = if is_double { 64 } else { 32 };
                let keep = simd::lane_mask(e);
                // replicate lane 0 so the unread lane (zero would be a divide-by-zero for FRECPE) raises no flags
                let n = simd::dup(simd::lane(self.regs.read_q(rn), e, 0), e, true);
                let raw = simd::fp_two_misc_fpcr(op, 0, n, is_double, false, self.regs.fpcr);
                self.regs.write_q(rd, raw & keep);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdScalarTwoMisc { rd, rn, op, esize } => {
                let e = esize as u32;
                // only lane 0 of the sources is read, so upper lanes cannot set QC
                let keep = simd::lane_mask(e);
                if matches!(op, SimdOp2::Scvtf | SimdOp2::Ucvtf | SimdOp2::Fcvtzs | SimdOp2::Fcvtzu) {
                    // SCVTF/UCVTF round under FPCR.RMode, FCVTZ* flush inputs under FZ (lane 0 only)
                    let r = simd::int_fp_convert_fpcr(op, self.regs.read_q(rn) & keep, e, false, self.regs.fpcr);
                    self.regs.write_q(rd, simd::lane(r, e, 0) as u128);
                    self.regs.advance_pc();
                    return Ok(StepResult::Continue);
                }
                let (r, qc) = simd::two_misc_sat(op, self.regs.read_q(rd) & keep, self.regs.read_q(rn) & keep, e, true);
                if qc {
                    self.regs.fpsr |= super::registers::FPSR_QC;
                }
                let lane0 = simd::lane(r, e, 0);
                self.regs.write_q(rd, lane0 as u128);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdTwoMisc { rd, rn, op, esize, q } => {
                if matches!(
                    op,
                    SimdOp2::Scvtf
                        | SimdOp2::Ucvtf
                        | SimdOp2::Fcvtzs
                        | SimdOp2::Fcvtzu
                        | SimdOp2::Fcvtns
                        | SimdOp2::Fcvtnu
                        | SimdOp2::Fcvtms
                        | SimdOp2::Fcvtmu
                        | SimdOp2::Fcvtas
                        | SimdOp2::Fcvtau
                        | SimdOp2::Fcvtps
                        | SimdOp2::Fcvtpu
                ) {
                    let r = simd::int_fp_convert_fpcr(op, self.regs.read_q(rn), esize as u32, q, self.regs.fpcr);
                    self.regs.write_q(rd, r);
                    self.regs.advance_pc();
                    return Ok(StepResult::Continue);
                }
                let (r, qc) = simd::two_misc_sat(op, self.regs.read_q(rd), self.regs.read_q(rn), esize as u32, q);
                if qc {
                    self.regs.fpsr |= super::registers::FPSR_QC;
                }
                self.regs.write_q(rd, r);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdShiftImm {
                rd,
                rn,
                op,
                esize,
                shift,
                q,
            } => {
                let (n, e, sh) = (self.regs.read_q(rn), esize as u32, shift as u32);
                let res = match op {
                    SimdShiftOp::Sshr => simd::shift_imm(n, e, sh, q, true, false),
                    SimdShiftOp::Ushr => simd::shift_imm(n, e, sh, q, false, false),
                    SimdShiftOp::Shl => simd::shift_imm(n, e, sh, q, false, true),
                    SimdShiftOp::Shrn => simd::shrn(self.regs.read_q(rd), n, e, sh, q),
                    SimdShiftOp::Sshll => simd::shll(n, e, sh, q, true),
                    SimdShiftOp::Ushll => simd::shll(n, e, sh, q, false),
                };
                self.regs.write_q(rd, res);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Brk { imm } => Err(CpuError::Breakpoint { imm, pc }),

            Instruction::SimdShiftExt {
                rd,
                rn,
                op,
                esize,
                shift,
                q,
                scalar,
            } => {
                let (n, e) = (self.regs.read_q(rn), esize as u32);
                // a scalar form only reads lane 0 (twice the width for narrowing), so mask the rest out of QC
                let src = if scalar { n & simd::scalar_source_mask(op, e) } else { n };
                let (res, sat) = simd::shift_ext_flag(op, self.regs.read_q(rd), src, e, shift as u32, q && !scalar);
                if sat {
                    self.regs.fpsr |= super::registers::FPSR_QC;
                }
                self.regs
                    .write_q(rd, if scalar { simd::lane(res, e, 0) as u128 } else { res });
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdPairwiseLong {
                rd,
                rn,
                signed,
                acc,
                esize,
                q,
            } => {
                let res = simd::pairwise_long(self.regs.read_q(rd), self.regs.read_q(rn), esize as u32, q, signed, acc);
                self.regs.write_q(rd, res);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdQNarrow {
                rd,
                rn,
                op,
                esize,
                q,
                scalar,
            } => {
                let e = esize as u32;
                let n = self.regs.read_q(rn);
                let src = if scalar { n & simd::scalar_source_mask(op, e) } else { n }; // scalar: only the source lane counts
                let (res, sat) = simd::shift_ext_flag(op, self.regs.read_q(rd), src, e, 0, q && !scalar);
                if sat {
                    self.regs.fpsr |= super::registers::FPSR_QC;
                }
                self.regs
                    .write_q(rd, if scalar { simd::lane(res, e, 0) as u128 } else { res });
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::SimdFixedConv {
                rd,
                rn,
                op,
                esize,
                fbits,
                q,
            } => {
                let r =
                    simd::fixed_convert_fpcr(op, self.regs.read_q(rn), esize as u32, q, fbits as u32, self.regs.fpcr);
                self.regs.write_q(rd, r);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::ScalarFixedConv {
                rd,
                rn,
                op,
                esize,
                fbits,
            } => {
                // One lane: convert it with the vector arithmetic (the other lane is zero and converts to zero with no flags)
                // and keep only lane 0, as the scalar form zeroes the rest of Vd.
                let e = esize as u32;
                let mask = (1u128 << e) - 1;
                let src = self.regs.read_q(rn) & mask;
                let r = simd::fixed_convert_fpcr(op, src, e, e == 64, fbits as u32, self.regs.fpcr);
                self.regs.write_q(rd, r & mask);
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Fjcvtzs { rd, rn } => {
                let (res, z) = softfp::double_to_int32_js(self.fp_env(true).1, self.fp_bits(rn, true));
                self.regs.write_x(rd, res as u64);
                // NZCV = 0 Z 0 0
                self.regs.pstate.n = false;
                self.regs.pstate.z = z;
                self.regs.pstate.c = false;
                self.regs.pstate.v = false;
                self.regs.advance_pc();
                Ok(StepResult::Continue)
            }

            Instruction::Unimplemented { raw } => Err(CpuError::UnimplementedInstruction { raw, pc }),
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
            assert!(
                matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)),
                "step failed at 0x{:X}",
                cpu.regs.pc
            );
        }
        (cpu, mmu)
    }

    #[test]
    fn libnx_ops_decode() {
        let table: &[(u32, &str)] = &[
            (0xdac00043, "DataProc1"),           // rbit x3, x2
            (0xdac00c84, "DataProc1"),           // rev x4, x4
            (0x5ac008a4, "DataProc1"),           // rev w4, w5
            (0x5ac00441, "DataProc1"),           // rev16 w1, w2
            (0xdac00841, "DataProc1"),           // rev32 x1, x2
            (0xdac01063, "DataProc1"),           // clz x3, x3
            (0xdac014c5, "DataProc1"),           // cls x5, x6
            (0x9bc17c00, "MultiplyHigh"),        // umulh x0, x0, x1
            (0x9b447c62, "MultiplyHigh"),        // smulh x2, x3, x4
            (0x9ba20c20, "MultiplyLong"),        // umaddl x0, w1, w2, x3
            (0x9b228c20, "MultiplyLong"),        // smsubl x0, w1, w2, x3
            (0x3cc0827f, "SimdLoadStore"),       // ldur q31, [x19, #8]
            (0x3c9f0080, "SimdLoadStore"),       // stur q0, [x4, #-16]
            (0xfc408441, "SimdLoadStore"),       // ldr d1, [x2], #8
            (0xbc004c83, "SimdLoadStore"),       // str s3, [x4, #4]!
            (0x3ce1681f, "SimdLoadStoreReg"),    // ldr q31, [x0, x1]
            (0xfc24d862, "SimdLoadStoreReg"),    // str d2, [x3, w4, sxtw #3]
            (0x4c40a021, "SimdLoadStoreMulti"),  // ld1 {v1.16b-v2.16b}, [x1]
            (0x4cdf7020, "SimdLoadStoreMulti"),  // ld1 {v0.16b}, [x1], #16
            (0x0c836044, "SimdLoadStoreMulti"),  // st1 {v4.8b-v6.8b}, [x2], x3
            (0x4e010c20, "SimdDupGeneral"),      // dup v0.16b, w1
            (0x4e080c83, "SimdDupGeneral"),      // dup v3.2d, x4
            (0x4e083c01, "SimdMovToGeneral"),    // mov x1, v0.d[0]
            (0x0e0b3c22, "SimdMovToGeneral"),    // umov w2, v1.b[5]
            (0x6e22ac20, "SimdThreeSame"),       // uminp v0.16b, v1.16b, v2.16b
            (0x4e22bc20, "SimdThreeSame"),       // addp v0.16b, v1.16b, v2.16b
            (0x4e201c21, "SimdThreeSame"),       // and v1.16b, v1.16b, v0.16b
            (0x4ea41c83, "SimdThreeSame"),       // mov v3.16b, v4.16b
            (0x6e621c20, "SimdThreeSame"),       // bsl v0.16b, v1.16b, v2.16b
            (0x4ea28420, "SimdThreeSame"),       // add v0.4s, v1.4s, v2.4s
            (0x6e623420, "SimdThreeSame"),       // cmhi v0.8h, v1.8h, v2.8h
            (0x0e209800, "SimdTwoMisc"),         // cmeq v0.8b, v0.8b, #0
            (0x0e31b800, "SimdAcross"),          // addv b0, v0.8b
            (0x0e205820, "SimdTwoMisc"),         // cnt v0.8b, v1.8b
            (0x6e205820, "SimdTwoMisc"),         // mvn v0.16b, v1.16b
            (0x0f0c8400, "SimdShiftImm"),        // shrn v0.8b, v0.8h, #4
            (0x4f088420, "SimdShiftImm"),        // shrn2 v0.16b, v1.8h, #8
            (0x6f3d0420, "SimdShiftImm"),        // ushr v0.4s, v1.4s, #3
            (0x7f6007fe, "SimdShiftImm"),        // ushr d30, d31, #32
            (0x4f4c5420, "SimdShiftImm"),        // shl v0.2d, v1.2d, #12
            (0xad3e0080, "SimdLoadStorePair"),   // stp q0, q0, [x4, #-64]
            (0x6cc107e0, "SimdLoadStorePair"),   // ldp d0, d1, [sp], #16
            (0xac0008a1, "SimdLoadStorePair"),   // stnp q1, q2, [x5]
            (0xd50b7423, "DcZva"),               // dc zva, x3
            (0x4f00e41f, "SimdModImm"),          // movi v31.16b, #0
            (0x1a010001, "AddSubCarry"),         // adc w1, w0, w1
            (0x1e6105e1, "FpCondCompare"),       // fccmp d15, d1, #1, eq
            (0x1e756bff, "FpData2"),             // fmaxnm d31, d31, d21
            (0x4f9f939f, "SimdFpByElement"),     // fmul v31.4s, v28.4s, v31.s[0]
            (0x6e1f43ff, "SimdExt"),             // ext v31.16b, v31.16b, v31.16b, #8
            (0x4e1d3bde, "SimdPermute"),         // zip1 v30.16b, v30.16b, v29.16b
            (0x0e095907, "SimdPermute"),         // uzp2 v7.8b, v8.8b, v9.8b
            (0x4e1f0549, "SimdDupElement"),      // dup v9.16b, v10.b[15]
            (0x6e1c2441, "SimdInsElement"),      // mov v1.s[3], v2.s[1]
            (0x4e181cc5, "SimdInsGeneral"),      // mov v5.d[1], x6
            (0x4d0048a4, "SimdLoadStoreSingle"), // st1 {v4.h}[5], [x5]
            (0x3d4a001d, "SimdLoadStore"),       // ldr b29, [x0, #640]
            (0x1e2e101f, "SimdModImm"),          // fmov s31, #1.0
            (0x1e22c042, "FpData1"),             // fcvt d2, s2
            (0x1e19e027, "FpFixedConv"),         // fcvtzu w7, s1, #8
            (0x1f5effbf, "FpData3"),             // fmsub d31, d29, d30, d31
            (0x4ebc9f5a, "SimdThreeSame"),       // mul v26.4s, v26.4s, v28.4s
            (0x0e21db9c, "SimdTwoMisc"),         // scvtf v28.2s, v28.2s
            (0x7efcd7de, "FpAbd"),               // fabd d30, d30, d28
            (0x5e21dc03, "FpMulx"),              // fmulx s3, s0, s1
            (0x5e61dc03, "FpMulx"),              // fmulx d3, d0, d1
            (0x7e21dbee, "SimdScalarTwoMisc"),   // ucvtf s14, s31
            (0x6e22e420, "SimdFpThreeSame"),     // fcmge v0.4s, v1.4s, v2.4s
            (0x4e62e420, "SimdFpThreeSame"),     // fcmeq v0.2d, v1.2d, v2.2d
            (0x6ea2e420, "SimdFpThreeSame"),     // fcmgt v0.4s, v1.4s, v2.4s
            (0x6e22dc20, "SimdVec4S"),           // fmul v0.4s, v1.4s, v2.4s (older dedicated arm)
            (0x6e62dc20, "SimdFpThreeSame"),     // fmul v0.2d, v1.2d, v2.2d
            (0x4e22cc20, "SimdFpThreeSame"),     // fmla v0.4s, v1.4s, v2.4s
            (0x6e62fc20, "SimdFpThreeSame"),     // fdiv v0.2d, v1.2d, v2.2d
            (0x4ea0f820, "SimdFpTwoMisc"),       // fabs v0.4s, v1.4s
            (0x4ea0c820, "SimdFpTwoMisc"),       // fcmgt v0.4s, v1.4s, #0.0
            (0x0e616820, "SimdFpTwoMisc"),       // fcvtn v0.2s, v1.2d
            (0x4ea1b820, "SimdTwoMisc"),         // fcvtzs v0.4s, v1.4s
            (0x7ea1b800, "SimdScalarTwoMisc"),   // fcvtzu s0, s0
            (0x0e612800, "SimdShiftImm"),        // xtn v0.4h, v0.4s
            (0x2f10a400, "SimdShiftImm"),        // uxtl v0.4s, v0.4h
            (0x4c400844, "SimdLoadStoreStruct"), // ld4 {v4.4s-v7.4s}, [x2]
            (0x4c9f4836, "SimdLoadStoreStruct"), // st3 {v22.4s-v24.4s}, [x1], #48
        ];
        for &(raw, name) in table {
            let got = format!("{:?}", super::super::decoder::decode(raw));
            assert!(got.starts_with(name), "0x{raw:08X}: expected {name}, got {got}");
        }
    }

    #[test]
    fn fabd_scalar_absolute_difference() {
        // fabd d30, d30, d28 ; fabd s0, s1, s2 (found by the cJSON differential test)
        let (cpu, _) = run(&[0x7efcd7de, 0x7ea2d420], |c, _| {
            c.regs.write_q(30, u128::MAX);
            c.regs.write_d(30, 1.0);
            c.regs.write_d(28, 3.5);
            c.regs.write_q(0, u128::MAX);
            c.regs.write_s(1, -2.0);
            c.regs.write_s(2, 5.0);
        });
        assert_eq!(cpu.regs.read_q(30), 2.5f64.to_bits() as u128); // |1.0 - 3.5|, upper bits zeroed
        assert_eq!(cpu.regs.read_q(0), 7.0f32.to_bits() as u128); // |-2 - 5|
        let (cpu, _) = run(&[0x7efcd7de], |c, _| {
            c.regs.write_d(30, f64::NAN);
            c.regs.write_d(28, 1.0);
        });
        assert!(cpu.regs.read_d(30).is_nan());
        let (cpu, _) = run(&[0x7efcd7de], |c, _| {
            c.regs.write_d(30, f64::INFINITY);
            c.regs.write_d(28, f64::INFINITY);
        });
        assert!(cpu.regs.read_d(30).is_nan()); // inf - inf
    }

    #[test]
    fn tbl_tbx_instructions() {
        // tbl v0.16b, {v0.16b}, v3.16b ; tbl v4.16b, {v30.16b, v31.16b}, v3.16b ; tbx v5.8b, {v1.16b}, v3.8b
        let words = [0x4E03_0000u32, 0x4E03_23C4, 0x0E03_1025];
        let names = ["SimdTbl"; 3];
        for (w, n) in words.iter().zip(names) {
            assert!(
                format!("{:?}", super::super::decoder::decode(*w)).starts_with(n),
                "0x{w:08X}"
            );
        }
        let (cpu, _) = run(&words, |c, _| {
            // v0 = bytes 0x00..0x0F reversed index source, v3 = indices 15..0
            c.regs
                .write_q(0, (0..16u128).fold(0, |a, i| a | ((0x40 + i) << (8 * i))));
            c.regs.write_q(3, (0..16u128).fold(0, |a, i| a | ((15 - i) << (8 * i))));
            c.regs
                .write_q(30, (0..16u128).fold(0, |a, i| a | ((0x50 + i) << (8 * i))));
            c.regs
                .write_q(31, (0..16u128).fold(0, |a, i| a | ((0x60 + i) << (8 * i))));
            c.regs
                .write_q(1, (0..16u128).fold(0, |a, i| a | ((0x70 + i) << (8 * i))));
            c.regs.write_q(5, u128::MAX);
        });
        // tbl reverses the byte order of v0
        assert_eq!(
            cpu.regs.read_q(0),
            (0..16u128).fold(0, |a, i| a | ((0x40 + 15 - i) << (8 * i)))
        );
        // two-register table: indices 0..15 only touch v30
        assert_eq!(
            cpu.regs.read_q(4),
            (0..16u128).fold(0, |a, i| a | ((0x50 + 15 - i) << (8 * i)))
        );
        // tbx 64-bit: low 8 bytes looked up from v1 with indices 15..8, upper half zeroed
        assert_eq!(
            cpu.regs.read_q(5),
            (0..8u128).fold(0, |a, i| a | ((0x70 + 15 - i) << (8 * i)))
        );
    }

    #[test]
    fn crc32_instructions() {
        // crc32b w1,w1,w0 ; crc32cb w3,w3,w0 ; crc32x w5,w5,x0 ; crc32w w6,w6,w0 ; crc32h w7,w7,w0 ; crc32cx w8,w8,x0
        let words = [
            0x1AC0_4021u32,
            0x1AC0_5063,
            0x9AC0_4CA5,
            0x1AC0_48C6,
            0x1AC0_44E7,
            0x9AC0_5D08,
        ];
        for w in words {
            assert!(
                format!("{:?}", super::super::decoder::decode(w)).starts_with("Crc32"),
                "0x{w:08X}"
            );
        }
        // sf=1 with a byte size is not a valid encoding
        assert!(format!("{:?}", super::super::decoder::decode(0x9AC0_4021)).starts_with("Unimplemented"));
        let (cpu, _) = run(&words, |c, _| {
            c.regs.write_x(0, 0x3132_3334_3536_3738);
            for r in [1, 3, 5, 6, 7, 8] {
                c.regs.write_x(r, 0xFFFF_FFFF);
            }
        });
        // reference values from a bitwise software CRC (no pre/post inversion, reflected)
        let crc = |init: u32, data: u64, bytes: u32, poly: u32| {
            let mut c = init;
            for i in 0..bytes {
                c ^= ((data >> (8 * i)) & 0xFF) as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { (c >> 1) ^ poly } else { c >> 1 };
                }
            }
            c
        };
        let d = 0x3132_3334_3536_3738u64;
        assert_eq!(cpu.regs.read_w(1), crc(0xFFFF_FFFF, d, 1, 0xEDB8_8320));
        assert_eq!(cpu.regs.read_w(3), crc(0xFFFF_FFFF, d, 1, 0x82F6_3B78));
        assert_eq!(cpu.regs.read_w(5), crc(0xFFFF_FFFF, d, 8, 0xEDB8_8320));
        assert_eq!(cpu.regs.read_w(6), crc(0xFFFF_FFFF, d, 4, 0xEDB8_8320));
        assert_eq!(cpu.regs.read_w(7), crc(0xFFFF_FFFF, d, 2, 0xEDB8_8320));
        assert_eq!(cpu.regs.read_w(8), crc(0xFFFF_FFFF, d, 8, 0x82F6_3B78));
        // known answer: CRC-32 of "123456789" is 0xCBF43926 with init/xorout 0xFFFFFFFF
        let mut c = 0xFFFF_FFFFu32;
        for b in b"123456789" {
            c = crc(c, *b as u64, 1, 0xEDB8_8320);
        }
        assert_eq!(!c, 0xCBF4_3926);
    }

    #[test]
    fn corpus_fp_and_simd_forms() {
        // Forms first met in the step 3 corpus programs (sixaxis, deko-console, nx-hbmenu).
        let (cpu, _) = run(&[0x1e22c042, 0x1e19e027, 0x1f5effbf], |c, _| {
            c.regs.write_s(2, 1.5); // fcvt d2, s2
            c.regs.write_s(1, -1.5); // fcvtzu w7, s1, #8 saturates negatives to 0
            c.regs.write_d(29, 2.0);
            c.regs.write_d(30, 3.0);
            c.regs.write_d(31, 10.0);
        });
        assert_eq!(cpu.regs.read_d(2), 1.5);
        assert_eq!(cpu.regs.read_x(7), 0);
        assert_eq!(cpu.regs.read_d(31), 4.0); // fmsub: 10 - 2 * 3
        // all four fused 3-source forms, d29 = 2, d30 = 3, d31 = 10 (regression: FNMADD/FNMSUB had the product sign wrong)
        for (word, want) in [
            (0x1F5E7FBFu32, 16.0),
            (0x1F5EFFBF, 4.0),
            (0x1F7E7FBF, -16.0),
            (0x1F7EFFBF, -4.0),
        ] {
            let (cpu, _) = run(&[word], |c, _| {
                c.regs.write_d(29, 2.0);
                c.regs.write_d(30, 3.0);
                c.regs.write_d(31, 10.0);
            });
            assert_eq!(cpu.regs.read_d(31), want, "{word:#x}"); // fmadd, fmsub, fnmadd, fnmsub
        }
        let (cpu, _) = run(&[0x1e19e027, 0x1f020020], |c, _| {
            c.regs.write_s(1, 1.5);
            c.regs.write_s(2, 3.0);
            c.regs.write_s(0, 0.5);
        });
        assert_eq!(cpu.regs.read_x(7), 384);
        assert_eq!(cpu.regs.read_s(0), 5.0); // fmadd s0, s1, s2, s0

        let lanes = |v: [u32; 4]| v.iter().rev().fold(0u128, |a, &x| a << 32 | x as u128);
        let (cpu, _) = run(&[0x4ebc9f5a, 0x0e612800, 0x2f10a40a, 0x7e21dbee], |c, _| {
            c.regs.write_q(26, lanes([2, 3, 0xFFFF_FFFF, 7]));
            c.regs.write_q(28, lanes([5, 6, 2, 0]));
            c.regs.write_q(0, lanes([0x1_0002, 0x3_0004, 0x5_0006, 0x7_0008]));
            c.regs.write_q(31, 9);
        });
        assert_eq!(cpu.regs.read_q(26), lanes([10, 18, 0xFFFF_FFFE, 0]));
        assert_eq!(cpu.regs.read_q(0), 0x0008_0006_0004_0002); // xtn v0.4h, v0.4s
        assert_eq!(cpu.regs.read_q(10), lanes([2, 4, 6, 8])); // uxtl v10.4s, v0.4h
        assert_eq!(cpu.regs.read_q(14), 9.0f32.to_bits() as u128); // ucvtf s14, s31

        // st3 {v22.4s-v24.4s}, [x1], #48 then ld4 {v4.4s-v7.4s}, [x2] reads the same words back 4 apart
        let (cpu, mmu) = run(&[0x4c9f4836, 0x4c400844], |c, _| {
            c.regs.write_x(1, DATA);
            c.regs.write_x(2, DATA);
            c.regs.write_q(22, lanes([0, 3, 6, 9]));
            c.regs.write_q(23, lanes([1, 4, 7, 10]));
            c.regs.write_q(24, lanes([2, 5, 8, 11]));
        });
        assert_eq!(cpu.regs.read_x(1), DATA + 48);
        assert_eq!(mmu.read_u32(DATA + 4 * 5).unwrap(), 5);
        assert_eq!(cpu.regs.read_q(4), lanes([0, 4, 8, 0]));
        assert_eq!(cpu.regs.read_q(7), lanes([3, 7, 11, 0]));
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
        assert_eq!(
            cpu.regs.read_q(31),
            u128::from_le_bytes(core::array::from_fn(|i| i as u8 + 8))
        );
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
    fn scalar_fp_by_element() {
        // fmul d2, d0, v1.d[1]; fmul s3, s4, v1.s[3] (found by running QuickJS as a Linux ELF)
        let (cpu, _) = run(&[0x5FC19802, 0x5FA19883], |c, _| {
            c.regs.write_q(0, (3.0f64.to_bits() as u128) | (u128::MAX << 64));
            c.regs
                .write_q(1, (1.0f64.to_bits() as u128) | ((0.5f64.to_bits() as u128) << 64));
            c.regs.write_q(2, u128::MAX);
            c.regs.write_q(3, u128::MAX);
            c.regs.write_q(4, 2.0f32.to_bits() as u128 | (u128::MAX << 32));
        });
        assert_eq!(cpu.regs.read_q(2), 1.5f64.to_bits() as u128);
        // v1.s[3] is the high word of 0.5f64 (0x3FE00000 = 1.75f32): 2.0 * 1.75
        assert_eq!(cpu.regs.read_q(3), 3.5f32.to_bits() as u128);
    }

    #[test]
    fn scalar_faddp_sums_two_lanes() {
        // faddp d0, v1.2d and faddp s2, v3.2s (found by running Lua as a Linux ELF)
        let (cpu, _) = run(&[0x7E70D820, 0x7E30D862], |c, _| {
            c.regs
                .write_q(1, (2.5f64.to_bits() as u128) | ((4.0f64.to_bits() as u128) << 64));
            c.regs.write_q(
                3,
                (1.5f32.to_bits() as u128) | ((0.25f32.to_bits() as u128) << 32) | (u128::MAX << 64),
            );
        });
        assert_eq!(cpu.regs.read_q(0), 6.5f64.to_bits() as u128);
        assert_eq!(cpu.regs.read_q(2), 1.75f32.to_bits() as u128);
    }

    #[test]
    fn ldpsw_sign_extends_both_words() {
        // ldpsw x0, x1, [x2]  (found by running Monocypher X25519 as a Linux ELF against qemu-user)
        let (cpu, _) = run(&[0x69400440], |c, m| {
            c.regs.write_x(2, DATA);
            m.write_u32(DATA, 0xFFFF_FFFB).unwrap(); // -5
            m.write_u32(DATA + 4, 0x7FFF_FFFF).unwrap();
        });
        assert_eq!(cpu.regs.read_x(0), (-5i64) as u64);
        assert_eq!(cpu.regs.read_x(1), 0x7FFF_FFFF);
        // ldpsw x0, x1, [x2, #8]!: pre-index writeback, negative high word
        let (cpu, _) = run(&[0x69c10440], |c, m| {
            c.regs.write_x(2, DATA);
            m.write_u32(DATA + 8, 0x8000_0000).unwrap();
            m.write_u32(DATA + 12, 1).unwrap();
        });
        assert_eq!(cpu.regs.read_x(0), 0xFFFF_FFFF_8000_0000);
        assert_eq!(cpu.regs.read_x(2), DATA + 8);
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
    fn saturating_narrows_and_shifts_set_qc() {
        use super::super::decoder::decode;
        // sqxtun v0.8b, v1.8h: a negative lane saturates to 0 and sets QC; in-range lanes do not
        assert!(
            matches!(decode(0x2E212820), Instruction::SimdQNarrow { scalar: false, .. }),
            "decode"
        );
        let (cpu, _) = run(&[0x2E212820], |c, _| c.regs.write_q(1, 0xFFFF));
        assert_ne!(cpu.regs.fpsr & super::super::registers::FPSR_QC, 0);
        let (cpu, _) = run(&[0x2E212820], |c, _| c.regs.write_q(1, 5));
        assert_eq!(cpu.regs.fpsr & super::super::registers::FPSR_QC, 0);
        // scalar sqxtun b0, h1 reads only lane 0: a saturating value in lane 1 must not set QC
        assert!(
            matches!(decode(0x7E212820), Instruction::SimdQNarrow { scalar: true, .. }),
            "decode scalar"
        );
        let (cpu, _) = run(&[0x7E212820], |c, _| c.regs.write_q(1, 0xFFFF_0005));
        assert_eq!(cpu.regs.fpsr & super::super::registers::FPSR_QC, 0);
        assert_eq!(cpu.regs.read_q(0), 5);
        let (cpu, _) = run(&[0x7E212820], |c, _| c.regs.write_q(1, 0x0005_FFFF));
        assert_ne!(cpu.regs.fpsr & super::super::registers::FPSR_QC, 0);
    }

    #[test]
    fn fpsr_exception_flags_accumulate_and_clear() {
        const IOC: u64 = 1;
        const DZC: u64 = 2;
        const IXC: u64 = 0x10;
        let init = |c: &mut Cpu, _: &mut Memory| {
            c.regs.write_q(0, 0x3f80_0000); // 1.0f
            c.regs.write_q(1, 0); // 0.0f
            c.regs.write_q(2, 0x7f80_0000); // +inf
            c.regs.write_q(3, 0xff80_0000); // -inf
            c.regs.write_q(4, 0x3380_0000); // 2^-24
            c.regs.write_q(5, 0x7fa0_0000); // signalling NaN
            c.regs.write_q(6, 0x7fc0_0000); // quiet NaN
        };
        // msr fpsr, xzr; fdiv s8, s0, s1; mrs x0, fpsr
        let (cpu, _) = run(&[0xd51b443f, 0x1e211808, 0xd53b4420], init);
        assert_eq!(cpu.regs.read_x(0), DZC);
        assert!(
            cpu.fp_flags_tracked,
            "touching FPSR switches Tier-4 to the interpreter's FP"
        );
        // fadd s8, s2, s3 (inf + -inf); fadd s9, s0, s4 (inexact); mrs x0; msr fpsr, xzr; mrs x1: sticky, then cleared
        let (cpu, _) = run(&[0x1e232848, 0x1e242809, 0xd53b4420, 0xd51b443f, 0xd53b4421], init);
        assert_eq!(cpu.regs.read_x(0), IOC | IXC);
        assert_eq!(cpu.regs.read_x(1), 0);
        // fcmp s5, s0 (signalling NaN: IOC); fcmp s6, s0 (quiet NaN: nothing); fcmpe s6, s0 (IOC)
        let (cpu, _) = run(&[0xd51b443f, 0x1e2020a0, 0xd53b4420], init); // fcmp s5, s0
        assert_eq!(cpu.regs.read_x(0), IOC);
        let (cpu, _) = run(&[0xd51b443f, 0x1e2020c0, 0xd53b4420], init); // fcmp s6, s0
        assert_eq!(cpu.regs.read_x(0), 0);
        let (cpu, _) = run(&[0xd51b443f, 0x1e2020d0, 0xd53b4420], init); // fcmpe s6, s0
        assert_eq!(cpu.regs.read_x(0), IOC);
        // fcvtzs w1, s0 is exact; fcvtzs w1, s4 (2^-24) is inexact; fcvtzs w1, s2 (inf) is invalid
        let (cpu, _) = run(&[0xd51b443f, 0x1e380001, 0xd53b4420], init);
        assert_eq!(cpu.regs.read_x(0), 0);
        let (cpu, _) = run(&[0xd51b443f, 0x1e380081, 0xd53b4420], init);
        assert_eq!(cpu.regs.read_x(0), IXC);
        let (cpu, _) = run(&[0xd51b443f, 0x1e380041, 0xd53b4420], init);
        assert_eq!(cpu.regs.read_x(0), IOC);
        assert_eq!(cpu.regs.read_x(1), 0x7fff_ffff);
    }

    #[test]
    fn fpsr_qc_sticky_and_fpcr_round_trip() {
        const QC: u64 = 1 << 27;
        // v0 = v1 = {0x8000, 0x0002, ..}: SQDMULL lane 0 is min * min (saturates), lane 1 is 2*2*2 = 8.
        let sat_ops = |c: &mut Cpu, _: &mut Memory| {
            c.regs.write_q(0, 0x0002_8000);
            c.regs.write_q(1, 0x0002_8000);
        };
        // msr fpsr, xzr; sqdmull v2.4s, v0.4h, v1.4h; mrs x0, fpsr
        let (cpu, _) = run(&[0xd51b443f, 0x0e61d002, 0xd53b4420], sat_ops);
        assert_eq!(cpu.regs.read_x(0), QC);
        assert_eq!(cpu.regs.read_q(2) & 0xFFFF_FFFF, 0x7FFF_FFFF);
        // No lane saturates: QC stays clear. 0x4000 * 0x4000 * 2 = 0x2000_0000.
        let (cpu, _) = run(&[0xd51b443f, 0x0e61d002, 0xd53b4420], |c, _| {
            c.regs.write_q(0, 0x4000);
            c.regs.write_q(1, 0x4000);
        });
        assert_eq!(cpu.regs.read_x(0), 0);
        // SQDMLAL accumulate saturation alone sets QC: acc 0x7FFFFFF0 + 2*3*5.
        let (cpu, _) = run(&[0xd51b443f, 0x0e619002, 0xd53b4420], |c, _| {
            c.regs.write_q(2, 0x7FFF_FFF0);
            c.regs.write_q(0, 3);
            c.regs.write_q(1, 5);
        });
        assert_eq!(cpu.regs.read_x(0), QC);
        // Sticky: a later non-saturating op leaves QC set; msr fpsr, xzr clears it.
        // sqdmull (sat); sqdmull v3.4s, v4.4h, v5.4h (zeros, no sat); mrs x0; msr fpsr, xzr; mrs x1
        let (cpu, _) = run(&[0x0e61d002, 0x0e65d083, 0xd53b4420, 0xd51b443f, 0xd53b4421], sat_ops);
        assert_eq!(cpu.regs.read_x(0), QC);
        assert_eq!(cpu.regs.read_x(1), 0);
        // mrs/msr round trip. FPSR keeps only defined bits; FPCR is stored verbatim.
        let (cpu, _) = run(&[0xd51b4421, 0xd53b4420, 0xd51b4422, 0xd53b4423], |c, _| {
            c.regs.write_x(1, 0xFFFF_FFFF);
            c.regs.write_x(2, 0x0140_0000); // no defined bits
        });
        assert_eq!(cpu.regs.read_x(0), 0x0800_009F);
        assert_eq!(cpu.regs.read_x(3), 0);
        // msr fpcr, x1; mrs x0, fpcr; msr fpsr, x2; mrs x3, fpsr
        let (cpu, _) = run(&[0xd51b4401, 0xd53b4400, 0xd51b4422, 0xd53b4423], |c, _| {
            c.regs.write_x(1, 0x0300_0000);
            c.regs.write_x(2, 0);
        });
        assert_eq!(cpu.regs.read_x(0), 0x0300_0000);
        assert_eq!(cpu.regs.fpcr, 0x0300_0000);
        assert_eq!(cpu.regs.read_x(3), 0); // fpsr untouched by fpcr writes
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
                c.regs
                    .write_q(8, 0x0000_0002_0000_0001u128 << 64 | 0xDEAD_BEEF_0000_0000);
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

    /// libpng's NEON row filters (`vst4_lane_u32`, `vld3_lane_u32`, ...) hit `LD2`-`LD4`/`ST2`-`ST4` single-structure
    /// and `LD2R`-`LD4R` words, which were undecoded. Element `i` of a structure goes to register `rt + i` (mod 32)
    /// and sits at `base + i * element size`; an immediate post-index is `registers * element size`.
    /// Expected values are worked out by hand from that definition.
    #[test]
    fn libpng_neon_ld2_ld3_ld4_st2_st3_st4_single_structure_and_replicate() {
        // st4 {v0.s-v3.s}[0], [x1], #16: only lane 0 of each register is stored, four words in a row
        let (cpu, mmu) = run(&[0x0dbf_a020], |c, _| {
            c.regs.write_q(0, 0x1111_1111);
            c.regs.write_q(1, 0xFFFF_FFFFu128 << 32 | 0x2222_2222);
            c.regs.write_q(2, 0x3333_3333);
            c.regs.write_q(3, 0x4444_4444);
            c.regs.write_x(1, DATA + 0x40);
        });
        for (i, want) in [0x1111_1111u32, 0x2222_2222, 0x3333_3333, 0x4444_4444, 0]
            .into_iter()
            .enumerate()
        {
            assert_eq!(mmu.read_u32(DATA + 0x40 + 4 * i as u64).unwrap(), want, "st4 word {i}");
        }
        assert_eq!(cpu.regs.read_x(1), DATA + 0x50);
        // ld3 {v0.s-v2.s}[1], [x1]: lane 1 replaced, other lanes kept, no write-back
        let (cpu, _) = run(&[0x0d40_b020], |c, m| {
            m.write_u32(DATA + 0x20, 0xA0A1_A2A3).unwrap();
            m.write_u32(DATA + 0x24, 0xB0B1_B2B3).unwrap();
            m.write_u32(DATA + 0x28, 0xC0C1_C2C3).unwrap();
            for r in 0..3 {
                c.regs.write_q(r, u128::MAX);
            }
            c.regs.write_x(1, DATA + 0x20);
        });
        let with_lane1 = |w: u128| u128::MAX & !(0xFFFF_FFFFu128 << 32) | w << 32;
        assert_eq!(cpu.regs.read_q(0), with_lane1(0xA0A1_A2A3));
        assert_eq!(cpu.regs.read_q(1), with_lane1(0xB0B1_B2B3));
        assert_eq!(cpu.regs.read_q(2), with_lane1(0xC0C1_C2C3));
        assert_eq!(cpu.regs.read_x(1), DATA + 0x20);
        // ld2 {v4.h, v5.h}[5], [x1], x2: halfword lane 5 is bits 80..95; the base moves by the register
        let (cpu, _) = run(&[0x4de2_4824], |c, m| {
            m.write_u16(DATA + 0x30, 0x1234).unwrap();
            m.write_u16(DATA + 0x32, 0x5678).unwrap();
            c.regs.write_x(1, DATA + 0x30);
            c.regs.write_x(2, 0x100);
        });
        assert_eq!(cpu.regs.read_q(4), 0x1234u128 << 80);
        assert_eq!(cpu.regs.read_q(5), 0x5678u128 << 80);
        assert_eq!(cpu.regs.read_x(1), DATA + 0x130);
        // st2 {v6.d, v7.d}[1], [x1]
        let (_, mmu) = run(&[0x4d20_8426], |c, _| {
            c.regs.write_q(6, 0x0102_0304_0506_0708u128 << 64 | 0xFFFF);
            c.regs.write_q(7, 0x1112_1314_1516_1718u128 << 64);
            c.regs.write_x(1, DATA + 0x80);
        });
        assert_eq!(mmu.read_u64(DATA + 0x80).unwrap(), 0x0102_0304_0506_0708);
        assert_eq!(mmu.read_u64(DATA + 0x88).unwrap(), 0x1112_1314_1516_1718);
        // ld4r {v8.4s-v11.4s}, [x1]: each register gets its own word in every lane
        let words = [0x1020_3040u32, 0x5060_7080, 0x90A0_B0C0, 0xD0E0_F000];
        let (cpu, _) = run(&[0x4d60_e828], |c, m| {
            for (i, w) in words.iter().enumerate() {
                m.write_u32(DATA + 0x40 + 4 * i as u64, *w).unwrap();
            }
            c.regs.write_x(1, DATA + 0x40);
        });
        for (i, w) in words.iter().enumerate() {
            assert_eq!(
                cpu.regs.read_q(8 + i as u8),
                super::simd::dup(*w as u64, 32, true),
                "ld4r v{}",
                8 + i
            );
        }
        // ld2r {v12.8b, v13.8b}, [x1], #2: Q = 0 leaves the upper half zero
        let (cpu, _) = run(&[0x0dff_c02c], |c, m| {
            m.write_u8(DATA + 0x60, 0xAB).unwrap();
            m.write_u8(DATA + 0x61, 0xCD).unwrap();
            c.regs.write_x(1, DATA + 0x60);
            c.regs.write_q(12, u128::MAX);
            c.regs.write_q(13, u128::MAX);
        });
        assert_eq!(cpu.regs.read_q(12), 0xABAB_ABAB_ABAB_ABAB);
        assert_eq!(cpu.regs.read_q(13), 0xCDCD_CDCD_CDCD_CDCD);
        assert_eq!(cpu.regs.read_x(1), DATA + 0x62);
        // st3 {v30.b, v31.b, v0.b}[3], [x1]: the register list wraps from v31 to v0
        let (_, mmu) = run(&[0x0d00_2c3e], |c, _| {
            c.regs.write_q(30, 0x31u128 << 24);
            c.regs.write_q(31, 0x32u128 << 24);
            c.regs.write_q(0, 0x33u128 << 24);
            c.regs.write_x(1, DATA + 0x90);
        });
        assert_eq!(
            (0..4)
                .map(|i| mmu.read_u8(DATA + 0x90 + i).unwrap())
                .collect::<Vec<_>>(),
            [0x31, 0x32, 0x33, 0]
        );
        // ld3r {v16.2d-v18.2d}, [x1], #24
        let (cpu, _) = run(&[0x4ddf_ec30], |c, m| {
            for (i, w) in [0x1111_2222_3333_4444u64, 0x5555_6666_7777_8888, 0x9999_AAAA_BBBB_CCCC]
                .iter()
                .enumerate()
            {
                m.write_u64(DATA + 0xA0 + 8 * i as u64, *w).unwrap();
            }
            c.regs.write_x(1, DATA + 0xA0);
        });
        assert_eq!(cpu.regs.read_q(16), super::simd::dup(0x1111_2222_3333_4444, 64, true));
        assert_eq!(cpu.regs.read_q(17), super::simd::dup(0x5555_6666_7777_8888, 64, true));
        assert_eq!(cpu.regs.read_q(18), super::simd::dup(0x9999_AAAA_BBBB_CCCC, 64, true));
        assert_eq!(cpu.regs.read_x(1), DATA + 0xB8);
        // ld4 {v20.b-v23.b}[15], [x1], #4: byte lane 15 is the top byte
        let (cpu, _) = run(&[0x4dff_3c34], |c, m| {
            for i in 0..4u64 {
                m.write_u8(DATA + 0xC0 + i, 0x41 + i as u8).unwrap();
            }
            c.regs.write_x(1, DATA + 0xC0);
        });
        for i in 0..4u8 {
            assert_eq!(
                cpu.regs.read_q(20 + i),
                (0x41u128 + i as u128) << 120,
                "ld4 v{}",
                20 + i
            );
        }
        assert_eq!(cpu.regs.read_x(1), DATA + 0xC4);
        // a replicate form cannot store: the L = 0 twin of ld1r still has no decode
        assert!(format!("{:?}", super::super::decoder::decode(0x4d00_c949)).starts_with("Unimplemented"));
    }

    /// Scalar half precision (FEAT_FP16): `FMOV` between a half register and a general register, `FMOV #imm`,
    /// `FRINT*` and the integer / fixed-point conversions. Compilers emit `FMOV Hd, Wn` for every `_Float16` that
    /// arrives in a general register, and TestFloat's half-precision groups stopped on the others. Expected values
    /// are worked out by hand from the Arm ARM definitions (VFPExpandImm, FPRoundInt, FPToFixed, FixedToFP).
    #[test]
    fn scalar_fp16_fmov_frint_and_integer_conversions() {
        const IOC: u32 = 1;
        const OFC: u32 = 1 << 2;
        const IXC: u32 = 1 << 4;
        const FZ16: u32 = 1 << 19;
        const DN: u32 = 1 << 25;
        const RP: u32 = 1 << 22;
        const RM: u32 = 2 << 22;
        const RZ: u32 = 3 << 22;
        // one instruction with v1 = h, x1 = x, FPCR = fpcr; returns the CPU to look at
        let one = |word: u32, h: u16, x: u64, fpcr: u32| {
            let (cpu, _) = run(&[word], |c, _| {
                c.regs.write_q(0, u128::MAX); // a dirty destination shows any bits left unwritten
                c.regs.write_q(1, (u128::MAX << 16) | h as u128);
                c.regs.write_x(0, u64::MAX);
                c.regs.write_x(1, x);
                c.regs.fpcr = fpcr;
            });
            cpu
        };
        // FMOV Hd, Wn/Xn: the low 16 bits, everything above them zero. FMOV Wd/Xd, Hn: zero-extended.
        assert_eq!(one(0x1ee7_0020, 0, 0xDEAD_3C00, 0).regs.read_q(0), 0x3C00); // fmov h0, w1
        assert_eq!(one(0x9ee7_0020, 0, 0x1234_5678_9ABC_DEF0, 0).regs.read_q(0), 0xDEF0); // fmov h0, x1
        let c = one(0x1ee6_0020, 0xABCD, 0, 0); // fmov w0, h1: the high half of x0 is cleared
        assert_eq!((c.regs.read_x(0), c.regs.fpsr), (0xABCD, 0));
        assert_eq!(one(0x9ee6_0020, 0x7C01, 0, 0).regs.read_x(0), 0x7C01); // fmov x0, h1: a signalling NaN moves untouched
        assert_eq!(one(0x9ee6_0020, 0x7C01, 0, 0).regs.fpsr, 0);
        // FMOV Hd, #imm: 1.0, -2.0, 0.5, 31.0, -0.125
        for (word, want) in [
            (0x1eee_1000u32, 0x3C00u128),
            (0x1ef0_1000, 0xC000),
            (0x1eec_1000, 0x3800),
            (0x1ee7_f000, 0x4FC0),
            (0x1ef8_1000, 0xB000),
        ] {
            assert_eq!(one(word, 0, 0, 0).regs.read_q(0), want, "{word:#010x}");
        }
        // FRINT*: (word, input, FPCR, result, FPSR)
        let cases: &[(u32, u16, u32, u16, u32)] = &[
            (0x1ee4_4020, 0x4100, 0, 0x4000, 0),    // frintn 2.5 -> 2.0 (ties to even)
            (0x1ee4_4020, 0x4300, 0, 0x4400, 0),    // frintn 3.5 -> 4.0
            (0x1ee6_4020, 0x4100, 0, 0x4200, 0),    // frinta 2.5 -> 3.0 (ties away)
            (0x1ee4_c020, 0xB800, 0, 0x8000, 0),    // frintp -0.5 -> -0.0
            (0x1ee5_4020, 0xB800, 0, 0xBC00, 0),    // frintm -0.5 -> -1.0
            (0x1ee5_c020, 0x41CD, 0, 0x4000, 0),    // frintz 2.8984375 -> 2.0
            (0x1ee7_4020, 0x4100, 0, 0x4000, IXC),  // frintx 2.5 -> 2.0, inexact
            (0x1ee7_4020, 0x4000, 0, 0x4000, 0),    // frintx 2.0 is exact
            (0x1ee7_c020, 0x4100, RP, 0x4200, 0),   // frinti follows RMode: toward +inf, no inexact
            (0x1ee7_c020, 0x41CD, RZ, 0x4000, 0),   // frinti toward zero
            (0x1ee7_4020, 0xB800, RM, 0xBC00, IXC), // frintx toward -inf: -0.5 -> -1.0, inexact
            (0x1ee4_4020, 0x7C01, 0, 0x7E01, IOC),  // a signalling NaN comes back quieted, invalid
            (0x1ee4_4020, 0x7E05, 0, 0x7E05, 0),    // a quiet NaN keeps its payload
            (0x1ee4_4020, 0x7C01, DN, 0x7E00, IOC), // DN: the default NaN
            (0x1ee4_4020, 0x7C00, 0, 0x7C00, 0),    // +inf
            (0x1ee4_4020, 0xFC00, 0, 0xFC00, 0),    // -inf
            (0x1ee4_4020, 0x7BFF, 0, 0x7BFF, 0),    // the largest finite half is already integral
        ];
        for &(word, h, fpcr, want, flags) in cases {
            let c = one(word, h, 0, fpcr);
            assert_eq!(
                (c.regs.read_q(0), c.regs.fpsr),
                (want as u128, flags),
                "{word:#010x} in={h:#06x} fpcr={fpcr:#x}"
            );
        }
        // FZ16 reads a denormal input as zero: ceil(2^-24) is 1.0 without it, 0.0 with it
        assert_eq!(one(0x1ee4_c020, 0x0001, 0, 0).regs.read_q(0), 0x3C00);
        assert_eq!(one(0x1ee4_c020, 0x0001, 0, FZ16).regs.read_q(0), 0);
        // SCVTF / UCVTF to half: (word, x, FPCR, result, FPSR)
        let cases: &[(u32, u64, u32, u16, u32)] = &[
            (0x1ee2_0020, 0xFFFF_FFFB, 0, 0xC500, 0),         // scvtf h0, w1: -5 -> -5.0
            (0x9ee2_0020, 65519, 0, 0x7BFF, IXC),             // scvtf h0, x1: rounds to the largest finite half
            (0x9ee2_0020, 65520, 0, 0x7C00, OFC | IXC),       // the first value that rounds to infinity
            (0x9ee2_0020, 65520, RZ, 0x7BFF, IXC),            // toward zero it stays at the largest finite half
            (0x1ee3_0020, 0xFFFF_FFFF, 0, 0x7C00, OFC | IXC), // ucvtf h0, w1: 4294967295 overflows
            // toward zero it still overflows before rounding: FPRound raises OFC and returns the largest finite half
            (0x1ee3_0020, 0xFFFF_FFFF, RZ, 0x7BFF, OFC | IXC),
        ];
        for &(word, x, fpcr, want, flags) in cases {
            let c = one(word, 0, x, fpcr);
            assert_eq!(
                (c.regs.read_q(0), c.regs.fpsr),
                (want as u128, flags),
                "{word:#010x} x={x:#x} fpcr={fpcr:#x}"
            );
        }
        // half to integer: (word, input, result, FPSR)
        let cases: &[(u32, u16, u64, u32)] = &[
            (0x1ef8_0020, 0xC500, 0xFFFF_FFFB, 0),   // fcvtzs w0, h1: -5.0
            (0x1ef8_0020, 0x41CD, 2, IXC),           // 2.8984375 -> 2, inexact
            (0x1ef8_0020, 0x7E00, 0, IOC),           // NaN -> 0, invalid
            (0x1ef8_0020, 0x7C00, 0x7FFF_FFFF, IOC), // +inf saturates
            (0x1ef8_0020, 0xFC00, 0x8000_0000, IOC), // -inf saturates
            (0x1ef9_0020, 0xBC00, 0, IOC),           // fcvtzu w0, h1: -1.0 is out of range
            (0x1ef9_0020, 0x7BFF, 65504, 0),         // 65504 is exact
            (0x1ee0_0020, 0x4100, 2, IXC),           // fcvtns: 2.5 -> 2 (ties to even)
            (0x1ee0_0020, 0x4300, 4, IXC),           // fcvtns: 3.5 -> 4
            (0x1ee4_0020, 0x4100, 3, IXC),           // fcvtas: 2.5 -> 3 (ties away)
            (0x1ee8_0020, 0x41CD, 3, IXC),           // fcvtps: 2.8984375 -> 3
            (0x1ef0_0020, 0xB800, 0xFFFF_FFFF, IXC), // fcvtms: -0.5 -> -1
            (0x9ef8_0020, 0x7BFF, 65504, 0),         // fcvtzs x0, h1
        ];
        for &(word, h, want, flags) in cases {
            let c = one(word, h, 0, 0);
            assert_eq!(
                (c.regs.read_x(0), c.regs.fpsr),
                (want, flags),
                "{word:#010x} in={h:#06x}"
            );
        }
        // fixed point: scvtf h0, w1, #8 and fcvtzs w0, h1, #4
        assert_eq!(one(0x1ec2_e020, 0, 384, 0).regs.read_q(0), 0x3E00); // 384 / 256 = 1.5
        assert_eq!(one(0x1ec2_e020, 0, (-384i32) as u32 as u64, 0).regs.read_q(0), 0xBE00);
        let c = one(0x1ed8_f020, 0x3E00, 0, 0); // 1.5 * 16 = 24
        assert_eq!((c.regs.read_x(0), c.regs.fpsr), (24, 0));
        let c = one(0x1ed8_f020, 0x41CD, 0, 0); // 2.8984375 * 16 = 46.375 -> 46, inexact
        assert_eq!((c.regs.read_x(0), c.regs.fpsr), (46, IXC));
        // reserved encodings stay undecoded: FCVTAS with rmode 1
        assert!(format!("{:?}", super::super::decoder::decode(0x1eec_0020)).starts_with("Unimplemented"));
    }

    #[test]
    fn simd_modified_immediate() {
        let words = [
            0x4f00e41f, 0x4f052561, 0x6f008642, 0x6f05e4a3, 0x2f00e424, 0x4f005425, 0x6f0797e6, 0x4f03f607, 0x6f04f488,
            0x4f02c749,
        ];
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
        let (cpu, _) = run(&[0x0f0c8400], |c, _| {
            c.regs.write_q(0, super::simd::dup(0x0AB0, 16, true))
        });
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

#[cfg(test)]
mod intvec_tests {
    //! Integer vector narrowing/shift forms and BRK. Encodings from the devkitA64 assembler; expected values
    //! computed by hand from the architecture pseudocode.
    use super::*;
    use crate::mmu::Permission;

    const CODE: u64 = 0x1000;

    fn lanes(esize: u32, vals: &[u64]) -> u128 {
        let mut v = 0u128;
        for (i, x) in vals.iter().enumerate() {
            simd::set_lane(&mut v, esize, i, *x);
        }
        v
    }

    /// Runs one word with v0 = `d` and v1 = `n`; returns v0.
    fn one(word: u32, d: u128, n: u128) -> u128 {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        mmu.map_range(CODE, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.write_u32(CODE, word).unwrap();
        cpu.regs.pc = CODE;
        cpu.regs.write_q(0, d);
        cpu.regs.write_q(1, n);
        assert!(matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)), "{word:#x}");
        cpu.regs.read_q(0)
    }

    #[test]
    fn saturating_narrow() {
        let n16 = lanes(16, &[0x0100, 0xFFFF, 5, 0x8000, 0x7FFF, 0xFF, 0, 1]);
        // sqxtun v0.8b, v1.8h: negatives to 0, above 255 to 255
        assert_eq!(one(0x2e212820, u128::MAX, n16), 0x0100_ffff_0005_00ff);
        // sqxtun2 v0.16b, v1.8h keeps the low half of v0
        assert_eq!(
            one(0x6e212820, 0x1111_2222_3333_4444, n16),
            0x0100_ffff_0005_00ffu128 << 64 | 0x1111_2222_3333_4444
        );
        // sqxtn v0.4h, v1.4s
        let n32 = lanes(32, &[0x8000_0000, 0x7FFF_FFFF, 0xFFFF_FFFE, 0x0001_0000]);
        assert_eq!(one(0x0e614820, u128::MAX, n32), 0x7FFF_FFFE_7FFF_8000);
        // uqxtn v0.8b, v1.8h: unsigned source, so 0xFFFF saturates to 0xFF
        let n16 = lanes(16, &[0x100, 0xFFFF, 5, 0, 0xFE, 0xFF, 1, 2]);
        assert_eq!(one(0x2e214820, u128::MAX, n16), 0x0201_fffe_0005_ffff);
        // sqxtn b0, h1: scalar, upper bits zero
        assert_eq!(one(0x5e214820, u128::MAX, 0x0100), 0x7F);
    }

    #[test]
    fn narrowing_shifts() {
        // rshrn v0.8b, v1.8h, #4: (0xAB8 + 8) >> 4 = 0xAC; (0xFFFF + 8) >> 4 = 0x1000 -> low byte 0
        let n = lanes(16, &[0xFFFF, 0xAB8, 0xAB8, 0xAB8, 0xAB8, 0xAB8, 0xAB8, 0xAB8]);
        assert_eq!(one(0x0f0c8c20, 0, n), 0xACAC_ACAC_ACAC_AC00);
        // rshrn2 v0.8h, v1.4s, #3 keeps the low half: (0x1_0004 + 4) >> 3 = 0x2001
        let n = lanes(32, &[0x1_0004, 0, 0xF, 0xC]);
        assert_eq!(one(0x4f1d8c20, 0xAA, n), lanes(16, &[0x2001, 0, 2, 2]) << 64 | 0xAA);
        // sqshrn v0.4h, v1.4s, #5
        let n = lanes(32, &[0x7FFF_FFFF, 0xFFFF_FFFF, 0x8000_0000, 0x640]);
        assert_eq!(one(0x0f1b9420, 0, n), 0x0032_8000_FFFF_7FFF);
        // uqrshrn v0.8b, v1.8h, #3: (0xFFFF + 4) >> 3 saturates; (0x14 + 4) >> 3 = 3
        let n = lanes(16, &[0xFFFF, 0x14, 0, 0, 0, 0, 0, 0]);
        assert_eq!(one(0x2f0d9c20, 0, n), 0x0000_0000_0000_03ff);
        // sqshrun v0.2s, v1.2d, #7: -5000 >> 7 < 0 -> 0; huge -> 0xFFFFFFFF
        let n = lanes(64, &[(-5000i64) as u64, i64::MAX as u64]);
        assert_eq!(one(0x2f398420, 0, n), 0xFFFF_FFFF_0000_0000);
    }

    #[test]
    fn accumulate_round_insert() {
        let dup = |e, v| simd::dup(v, e, true);
        // usra v0.4s, v1.4s, #3: 10 + (0x80000000 >> 3)
        assert_eq!(one(0x6f3d1420, dup(32, 10), dup(32, 0x8000_0000)), dup(32, 0x1000_000A));
        // ssra v0.8h, v1.8h, #2: 1 + (-32768 >> 2) = 0xE001
        assert_eq!(one(0x4f1e1420, dup(16, 1), dup(16, 0x8000)), dup(16, 0xE001));
        // srshr v0.4s, v1.4s, #4: -24 -> (-24 + 8) >> 4 = -1; 24 -> 2; 7 -> 0; -8 -> 0
        let n = lanes(32, &[(-24i32) as u32 as u64, 24, 7, (-8i32) as u32 as u64]);
        assert_eq!(one(0x4f3c2420, 0, n), lanes(32, &[0xFFFF_FFFF, 2, 0, 0]));
        // urshr v0.4s, v1.4s, #1: 0xFFFFFFFF -> 0x80000000 (no 32-bit overflow); 1 -> 1; 3 -> 2
        let n = lanes(32, &[0xFFFF_FFFF, 1, 0, 3]);
        assert_eq!(one(0x6f3f2420, 0, n), lanes(32, &[0x8000_0000, 1, 0, 2]));
        // srsra v0.8h, v1.8h, #3: 5 + ((-9 + 4) >> 3) = 5 + -1 = 4
        assert_eq!(one(0x4f1d3420, dup(16, 5), dup(16, (-9i16) as u16 as u64)), dup(16, 4));
        // ursra v0.2d, v1.2d, #5: 1 + ((0xFFFF_FFFF_FFFF_FFFF + 16) >> 5) = 1 + 0x0800_0000_0000_0000
        assert_eq!(
            one(0x6f7b3420, dup(64, 1), dup(64, u64::MAX)),
            dup(64, 0x0800_0000_0000_0001)
        );
        // usra d0, d1, #7: scalar, 5 + (1 << 63 >> 7), upper half zeroed
        assert_eq!(one(0x7f791420, 5, 1u128 << 63 | 1u128 << 100), 0x0100_0000_0000_0005);
        // sri v0.4s, v1.4s, #8: top byte of d kept, n >> 8 below
        assert_eq!(
            one(0x6f384420, dup(32, 0xAABB_CCDD), dup(32, 0x1122_3344)),
            dup(32, 0xAA11_2233)
        );
        // sli v0.4s, v1.4s, #8: low byte of d kept, n << 8 above
        assert_eq!(
            one(0x6f285420, dup(32, 0xAABB_CCDD), dup(32, 0x1122_3344)),
            dup(32, 0x2233_44DD)
        );
    }

    #[test]
    fn saturating_shift_left() {
        let n = lanes(32, &[0x1000_0000, 0xF000_0000, 0xE000_0000, 5]);
        // sqshl v0.4s, v1.4s, #3: 2^31 overflows, -2^31 fits exactly, below saturates to INT_MIN
        assert_eq!(
            one(0x4f237420, 0, n),
            lanes(32, &[0x7FFF_FFFF, 0x8000_0000, 0x8000_0000, 40])
        );
        // uqshl v0.16b, v1.16b, #2: bytes 40 3f 01 80 -> ff fc 04 ff
        assert_eq!(
            one(0x6f0a7420, 0, simd::dup(0x8001_3F40, 32, true)),
            simd::dup(0xFF04_FCFF, 32, true)
        );
        // sqshlu v0.8h, v1.8h, #4: signed source, unsigned result
        let n = lanes(16, &[0xFFFF, 0x07FF, 0x0800, 0x1000, 0, 0, 0, 0]);
        assert_eq!(
            one(0x6f146420, 0, n),
            lanes(16, &[0, 0x7FF0, 0x8000, 0xFFFF, 0, 0, 0, 0])
        );
    }

    #[test]
    fn pairwise_long() {
        // uaddlp v0.1d, v1.2s (0x2ea02820): 0xFFFFFFFF + 0xFFFFFFFF = 0x1_FFFF_FFFE
        assert_eq!(
            one(0x2ea02820, u128::MAX, lanes(32, &[0xFFFF_FFFF, 0xFFFF_FFFF])),
            0x1_FFFF_FFFE
        );
        // saddlp v0.8h, v1.16b (0x4e202820): -1 + -1 = -2 ; 127 + 127 = 254
        let n = lanes(8, &[0xFF, 0xFF, 127, 127, 0x80, 0x80, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(one(0x4e202820, 0, n), lanes(16, &[0xFFFE, 254, 0xFF00, 3, 0, 0, 0, 0]));
        // uadalp v0.4s, v1.8h (0x6e606820): 10 + (0xFFFF + 1)
        let n = lanes(16, &[0xFFFF, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            one(0x6e606820, lanes(32, &[10, 20, 30, 40]), n),
            lanes(32, &[10 + 0x10000, 20, 30, 40])
        );
    }

    #[test]
    fn addp_scalar_d() {
        // addp d0, v1.2d (0x5ef1b820): 1 + 2^64-1 wraps to 0; upper half zeroed
        assert_eq!(one(0x5ef1b820, u128::MAX, 1u128 | (u64::MAX as u128) << 64), 0);
        assert_eq!(one(0x5ef1b820, u128::MAX, 7u128 | 9u128 << 64), 16);
    }

    #[test]
    fn saturate_helper_reports_saturation() {
        assert_eq!(simd::saturate(300, 8, false), (255, true));
        assert_eq!(simd::saturate(-1, 8, false), (0, true));
        assert_eq!(simd::saturate(-129, 8, true), (0x80, true));
        assert_eq!(simd::saturate(127, 8, true), (127, false));
    }

    #[test]
    fn brk_stops_the_guest() {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        mmu.map_range(CODE, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.write_u32(CODE, 0xd4207d00).unwrap(); // brk #0x3e8
        cpu.regs.pc = CODE;
        let err = cpu.step(&mut mmu).unwrap_err();
        assert!(matches!(err, CpuError::Breakpoint { imm: 0x3e8, pc: CODE }), "{err:?}");
        assert_eq!(err.to_string(), "BRK #0x3e8 breakpoint hit at PC: 0x0000000000001000");
        assert_eq!(cpu.regs.pc, CODE);
    }
}

impl Cpu {
    /// FPCR selects a non-default RMode, FZ or DN: arithmetic goes through the soft-float path.
    fn soft_fp(&self) -> bool {
        self.regs.fpcr & softfp::FPCR_ARITH_MASK != 0
    }
    fn fp_env(&self, double: bool) -> (FpFmt, FpEnv) {
        (
            if double { softfp::F64 } else { softfp::F32 },
            FpEnv::from_fpcr(self.regs.fpcr),
        )
    }
    fn fp_bits(&self, r: u8, double: bool) -> u64 {
        let v = self.regs.read_q(r);
        if double { v as u64 } else { v as u32 as u64 }
    }
    /// Scalar result write: the upper bits of the register are zeroed.
    fn set_fp_bits(&mut self, r: u8, double: bool, bits: u64) {
        let _ = double;
        self.regs.write_q(r, bits as u128);
    }
    /// A compare operand widened to f64; with FPCR.FZ a denormal reads as zero.
    fn fp_cmp_operand(&self, r: u8, double: bool) -> f64 {
        let (f, env) = self.fp_env(double);
        let b = softfp::flush_input(f, env, self.fp_bits(r, double));
        if double {
            f64::from_bits(b)
        } else {
            f32::from_bits(b as u32) as f64
        }
    }
}

/// Arm `FPMulAdd` for `n * m + a`: a quiet-NaN addend with `0 * inf` gives the default NaN, otherwise the NaN
/// of the addend, then `n`, then `m` propagates (signalling first); else the fused result.
fn fp_fused<T: FpNum + FpFused>(n: T, m: T, a: T) -> T {
    if a.is_qnan() && n.zero_times_inf(m) {
        return T::default_nan();
    }
    FpNum::process_nans(&[a, n, m]).unwrap_or_else(|| n.fused(m, a).default_nan_if_nan())
}
trait FpFused: Sized {
    fn zero_times_inf(self, m: Self) -> bool;
    fn default_nan() -> Self;
    fn fused(self, m: Self, a: Self) -> Self;
}
impl FpFused for f32 {
    fn zero_times_inf(self, m: Self) -> bool {
        (self == 0.0 && m.is_infinite()) || (self.is_infinite() && m == 0.0)
    }
    fn default_nan() -> Self {
        f32::NAN
    }
    fn fused(self, m: Self, a: Self) -> Self {
        self.mul_add(m, a)
    }
}
impl FpFused for f64 {
    fn zero_times_inf(self, m: Self) -> bool {
        (self == 0.0 && m.is_infinite()) || (self.is_infinite() && m == 0.0)
    }
    fn default_nan() -> Self {
        f64::NAN
    }
    fn fused(self, m: Self, a: Self) -> Self {
        self.mul_add(m, a)
    }
}

/// NaN handling shared by scalar `FMAXNM`/`FMINNM`: a quiet NaN against a number returns the number;
/// any other NaN combination (a signalling NaN involved, or two NaNs) propagates like `FADD`
/// (signalling first, quieted), which is what Arm hardware returns.
trait FpNum: Copy + std::ops::Add<Output = Self> + std::ops::Neg<Output = Self> {
    fn is_qnan(self) -> bool;
    fn is_any_nan(self) -> bool;
    /// `self` with its quiet bit set.
    fn quieted(self) -> Self;
    /// Arm `FPDefaultNaN()` when `self` is a NaN made by an invalid operation (no NaN input): always the positive
    /// quiet NaN. x86 hosts return the negative one, so the result must not depend on the host CPU.
    fn default_nan_if_nan(self) -> Self;
    /// Arm `FPProcessNaNs`: the first signalling NaN (quieted), else the first quiet NaN, else `None`.
    /// Spelled out because `a + b` / `a * b` may be commuted by the compiler, picking the other NaN.
    fn process_nans(ops: &[Self]) -> Option<Self> {
        if let Some(x) = ops.iter().find(|x| x.is_any_nan() && !x.is_qnan()) {
            return Some(x.quieted());
        }
        ops.iter().find(|x| x.is_any_nan()).copied()
    }
    fn nm_nan(n: Self, m: Self) -> Option<Self> {
        if n.is_any_nan() || m.is_any_nan() {
            Some(if n.is_qnan() && !m.is_any_nan() {
                m
            } else if m.is_qnan() && !n.is_any_nan() {
                n
            } else {
                Self::process_nans(&[n, m]).unwrap_or(n + m)
            })
        } else {
            None
        }
    }
}
impl FpNum for f32 {
    fn quieted(self) -> Self {
        f32::from_bits(self.to_bits() | 1 << 22)
    }
    fn default_nan_if_nan(self) -> Self {
        if self.is_nan() {
            f32::from_bits(0x7fc0_0000)
        } else {
            self
        }
    }
    fn is_qnan(self) -> bool {
        self.is_nan() && self.to_bits() & (1 << 22) != 0
    }
    fn is_any_nan(self) -> bool {
        self.is_nan()
    }
}
impl FpNum for f64 {
    fn quieted(self) -> Self {
        f64::from_bits(self.to_bits() | 1 << 51)
    }
    fn default_nan_if_nan(self) -> Self {
        if self.is_nan() {
            f64::from_bits(0x7ff8_0000_0000_0000)
        } else {
            self
        }
    }
    fn is_qnan(self) -> bool {
        self.is_nan() && self.to_bits() & (1 << 51) != 0
    }
    fn is_any_nan(self) -> bool {
        self.is_nan()
    }
}

#[cfg(test)]
mod fuzz_scalar_nan {
    use super::*;
    use crate::mmu::Permission;

    fn run1(word: u32, set: &[(usize, u128)]) -> Cpu {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        mmu.map_range(0x1000, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.write_u32(0x1000, word).unwrap();
        cpu.regs.pc = 0x1000;
        for &(r, v) in set {
            cpu.regs.v[r] = v;
        }
        assert!(matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)));
        cpu
    }

    /// Found by the instruction fuzzer; the expected values are what Apple Silicon returned for these
    /// exact inputs: the first operand's NaN propagates (signalling first, quieted), then FNMUL negates.
    #[test]
    fn fnmul_propagates_first_nan_then_negates() {
        // fnmul s28, s1, s19: both quiet, n first
        let c = run1(0x1e33_883c, &[(1, 0xffff_ffff), (19, 0xffc0_0000)]);
        assert_eq!(c.regs.v[28], 0x7fff_ffff);
        // fnmul s22, s5, s12
        let c = run1(0x1e2c_88b6, &[(5, 0x7fff_ffff), (12, 0xffff_ffff)]);
        assert_eq!(c.regs.v[22], 0xffff_ffff);
        // fnmul d10, d22, d28: both signalling, n first; quieted then negated
        let c = run1(0x1e7c_8aca, &[(22, 0xfff4_0000_0000_0000), (28, 0x7ff0_0000_0000_0001)]);
        assert_eq!(c.regs.v[10], 0x7ffc_0000_0000_0000);
    }

    #[test]
    fn scalar_fadd_fmul_nan_order_and_fmadd_addend_first() {
        // fadd s0, s1, s2 and fmul s0, s1, s2: n's NaN wins when both are quiet
        for w in [0x1e22_2820u32, 0x1e22_0820] {
            let c = run1(w, &[(1, 0x7fc0_0001), (2, 0x7fc0_0002)]);
            assert_eq!(c.regs.v[0], 0x7fc0_0001);
        }
        // a signalling NaN in m beats a quiet NaN in n
        let c = run1(0x1e22_2820, &[(1, 0x7fc0_0001), (2, 0x7f80_0002)]);
        assert_eq!(c.regs.v[0], 0x7fc0_0002);
        // fmadd s0, s1, s2, s3: the addend's NaN first
        let c = run1(0x1f02_0c20, &[(1, 0x7fc0_0001), (2, 0x7fc0_0002), (3, 0x7fc0_0003)]);
        assert_eq!(c.regs.v[0], 0x7fc0_0003);
        // a quiet-NaN addend with 0 * inf is the default NaN
        let c = run1(0x1f02_0c20, &[(1, 0), (2, 0x7f80_0000), (3, 0x7fc0_0003)]);
        assert_eq!(c.regs.v[0], 0x7fc0_0000);
    }

    fn run_fpcr(word: u32, fpcr: u32, set: &[(usize, u128)]) -> Cpu {
        let mut mmu = Memory::new();
        let mut cpu = Cpu::new();
        mmu.map_range(0x1000, 0x1000, Permission::READ_WRITE_EXECUTE);
        mmu.write_u32(0x1000, word).unwrap();
        cpu.regs.pc = 0x1000;
        cpu.regs.fpcr = fpcr;
        for &(r, v) in set {
            cpu.regs.v[r] = v;
        }
        assert!(matches!(cpu.step(&mut mmu), Ok(StepResult::Continue)));
        cpu
    }

    /// The three repros of #363, each taken from a corpus mismatch against an AArch64 reference.
    #[test]
    fn fpcr_fz_dn_and_rounding_mode_apply_to_arithmetic() {
        // FMUL S3,S0,S1 with FZ: 1.0 * denormal reads the denormal as zero
        assert_eq!(
            run_fpcr(0x1e21_0803, 0x100_0000, &[(0, 0x3f80_0000), (1, 1)]).regs.v[3],
            0
        );
        assert_eq!(run_fpcr(0x1e21_0803, 0, &[(0, 0x3f80_0000), (1, 1)]).regs.v[3], 1);
        // FADD S3,S0,S1 with DN: a quiet NaN with payload becomes the default NaN
        assert_eq!(
            run_fpcr(0x1e21_2803, 0x200_0000, &[(0, 0), (1, 0xffc0_0001)]).regs.v[3],
            0x7fc0_0000
        );
        assert_eq!(
            run_fpcr(0x1e21_2803, 0, &[(0, 0), (1, 0xffc0_0001)]).regs.v[3],
            0xffc0_0001
        );
        // FADD S3,S0,S1 with RMode: 1.0 + 2^-149 rounds up for RP, stays for RN/RM/RZ
        for (rmode, want) in [
            (0u32, 0x3f80_0000u128),
            (1, 0x3f80_0001),
            (2, 0x3f80_0000),
            (3, 0x3f80_0000),
        ] {
            assert_eq!(
                run_fpcr(0x1e21_2803, rmode << 22, &[(0, 0x3f80_0000), (1, 1)]).regs.v[3],
                want,
                "rmode {rmode}"
            );
        }
        // FMADD D3,D0,D1,D2 (fused) under RM: 1 + 2^-60 * 2^-60 rounds down to 1.0; RP gives the next double up
        let (a, b) = (0x3c30_0000_0000_0000u128, 0x3c30_0000_0000_0000u128); // 2^-60
        assert_eq!(
            run_fpcr(0x1f41_0803, 2 << 22, &[(0, a), (1, b), (2, 0x3ff0_0000_0000_0000)])
                .regs
                .v[3],
            0x3ff0_0000_0000_0000
        );
        assert_eq!(
            run_fpcr(0x1f41_0803, 1 << 22, &[(0, a), (1, b), (2, 0x3ff0_0000_0000_0000)])
                .regs
                .v[3],
            0x3ff0_0000_0000_0001
        );
        // scalar FADDP S0, V1.2S under RP: 1.0 + 2^-149 rounds up
        assert_eq!(
            run_fpcr(0x7e30_d820, 1 << 22, &[(1, 0x3f80_0000 | 1u128 << 32)]).regs.v[0],
            0x3f80_0001
        );
        assert_eq!(
            run_fpcr(0x7e30_d820, 0, &[(1, 0x3f80_0000 | 1u128 << 32)]).regs.v[0],
            0x3f80_0000
        );
        // vector FADD V.4S under FZ+DN: lane 0 denormal + denormal flushes to +0
        let c = run_fpcr(0x4e22_d420, 0x300_0000, &[(1, 0x0000_0001), (2, 0x0000_0002)]);
        assert_eq!(c.regs.v[0] as u32, 0);
    }

    /// Found by the FP edge-case corpus (docs/verify/fp-edge): results that must not depend on the host CPU.
    /// The expected values follow the Arm pseudocode (FPDefaultNaN, FPProcessNaNs, FPRoundInt, FPSqrt) and agree
    /// with what an AArch64 reference returned for these inputs; x86 hosts used to give the negative default NaN.
    #[test]
    fn invalid_operations_return_positive_default_nan_and_unary_ops_quiet_snan() {
        let (inf, ninf) = (0x7f80_0000u128, 0xff80_0000u128);
        // fadd s0,s1,s2: inf + -inf; fdiv s0,s1,s2: 0 / 0
        assert_eq!(run1(0x1e22_2820, &[(1, inf), (2, ninf)]).regs.v[0], 0x7fc0_0000);
        assert_eq!(run1(0x1e22_1820, &[(1, 0), (2, 0)]).regs.v[0], 0x7fc0_0000);
        // fnmul s0,s1,s2: 0 * inf is the default NaN, then negated
        assert_eq!(run1(0x1e22_8820, &[(1, 0), (2, inf)]).regs.v[0], 0xffc0_0000);
        // fmadd s0,s1,s2,s3: 0 * inf + 1.0
        assert_eq!(
            run1(0x1f02_0c20, &[(1, 0), (2, inf), (3, 0x3f80_0000)]).regs.v[0],
            0x7fc0_0000
        );
        // fsqrt s0,s1: sqrt(-1) is the default NaN, a signalling NaN comes back quieted
        assert_eq!(run1(0x1e21_c020, &[(1, 0xbf80_0000)]).regs.v[0], 0x7fc0_0000);
        assert_eq!(run1(0x1e21_c020, &[(1, 0x7f80_0001)]).regs.v[0], 0x7fc0_0001);
        // frintn s0,s1 / frintn d0,d1 on a signalling NaN
        assert_eq!(run1(0x1e24_4020, &[(1, 0x7f80_0001)]).regs.v[0], 0x7fc0_0001);
        assert_eq!(
            run1(0x1e64_4020, &[(1, 0x7ff0_0000_0000_0001)]).regs.v[0],
            0x7ff8_0000_0000_0001
        );
        // fadd v0.4s,v1.4s,v2.4s: lane 0 quiet NaN + signalling NaN returns the signalling one quieted;
        // lane 1 is inf + -inf (default NaN); fdiv v0.2d: lane 0 is 0 / 0
        let c = run1(
            0x4e22_d420,
            &[(1, 0x7fc0_0000 | inf << 32), (2, 0x7f80_0001 | ninf << 32)],
        );
        assert_eq!(c.regs.v[0] & 0xffff_ffff_ffff_ffff, 0x7fc0_0000_7fc0_0001);
        let c = run1(0x6e62_fc20, &[(1, 0), (2, 0)]);
        assert_eq!(c.regs.v[0], 0x7ff8_0000_0000_0000_7ff8_0000_0000_0000);
    }

    #[test]
    fn scalar_fp16_arithmetic_unary_sqrt_and_fcvt() {
        let fz16 = 1 << 19;
        let fz = 1 << 24;
        // 1.5 (0x3e00) + 2.0 (0x4000) = 3.5 (0x4300); - 2.0 = -0.5 (0xb800); * 2.0 = 3.0 (0x4200)
        assert_eq!(run1(0x1ee2_2820, &[(1, 0x3e00), (2, 0x4000)]).regs.v[0], 0x4300);
        assert_eq!(run1(0x1ee2_3820, &[(1, 0x3e00), (2, 0x4000)]).regs.v[0], 0xb800);
        assert_eq!(run1(0x1ee2_0820, &[(1, 0x3e00), (2, 0x4000)]).regs.v[0], 0x4200);
        // 3.0 (0x4200) / 2.0 (0x4000) = 1.5 (0x3e00); fnmul(1.5, 2.0) = -3.0 (0xc200)
        assert_eq!(run1(0x1ee2_1820, &[(1, 0x4200), (2, 0x4000)]).regs.v[0], 0x3e00);
        assert_eq!(run1(0x1ee2_8820, &[(1, 0x3e00), (2, 0x4000)]).regs.v[0], 0xc200);
        // fnmul h0, h1, h2 on qNaN flips the sign bit even without touching payload
        assert_eq!(run1(0x1ee2_8820, &[(1, 0x7e01), (2, 0x3c00)]).regs.v[0], 0xfe01);
        // FADD H honours FZ16 (flushes denormal without IDC), ignores FZ
        let c = run_fpcr(0x1ee2_2820, fz16, &[(1, 0x0001), (2, 0x0000)]);
        assert_eq!((c.regs.v[0], c.regs.fpsr), (0, 0));
        let c = run_fpcr(0x1ee2_2820, fz, &[(1, 0x0001), (2, 0x0000)]);
        assert_eq!((c.regs.v[0], c.regs.fpsr), (0x0001, 0));
        // FMOV / FABS / FNEG H zero upper 112 bits and leave sNaN unquieted without IOC
        let dirty_snan = (0xdead_beefu128 << 64) | 0xfd01;
        let c = run1(0x1ee0_4020, &[(1, dirty_snan)]);
        assert_eq!((c.regs.v[0], c.regs.fpsr), (0xfd01, 0));
        let c = run1(0x1ee0_c020, &[(1, dirty_snan)]);
        assert_eq!((c.regs.v[0], c.regs.fpsr), (0x7d01, 0));
        let c = run1(0x1ee1_4020, &[(1, dirty_snan)]);
        assert_eq!((c.regs.v[0], c.regs.fpsr), (0x7d01, 0));
        // FSQRT H: sqrt(4.0 = 0x4400) = 2.0 (0x4000)
        assert_eq!(run1(0x1ee1_c020, &[(1, 0x4400)]).regs.v[0], 0x4000);
        // FCVT S<-H (0x1ee2_4020), D<-H (0x1ee2_c020), H<-S (0x1e23_c020), H<-D (0x1e63_c020)
        assert_eq!(run1(0x1ee2_4020, &[(1, 0x3e00)]).regs.v[0], 0x3fc0_0000);
        assert_eq!(run1(0x1ee2_c020, &[(1, 0x3e00)]).regs.v[0], 0x3ff8_0000_0000_0000);
        assert_eq!(run1(0x1e23_c020, &[(1, 0x3fc0_0000)]).regs.v[0], 0x3e00);
        assert_eq!(run1(0x1e63_c020, &[(1, 0x3ff8_0000_0000_0000)]).regs.v[0], 0x3e00);
    }
}
