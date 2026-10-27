//! Tier-4: hot guest blocks translated to host code by `ounce-recompiler` (M4 step 2,
//! docs/design/m4-native-jit.md). Safe code only; executable memory and raw calls stay in the
//! recompiler crate.
//!
//! Lowering mirrors `Cpu::step` instruction by instruction (the interpreter is the reference), so
//! the same guest state comes out of every tier. Integer ALU, moves, logical, shifts, bitfield,
//! extract, conditional select/compare, multiply/divide, `ADR`, loads/stores/pairs, branches and
//! `SVC` are translated. A block stops before anything else; cold code and those instructions run
//! on Tier-3.
//!
//! Memory goes through [`MemoryBus`] callbacks (plan A). Guest registers live in memory while a
//! block runs, so a fault leaves the state exactly as the interpreter would at the faulting
//! instruction. A store into translated code ends the block after that instruction and the stale
//! translations are dropped through `Memory::code_written_since`, like Tier-2/3.

use super::decoder::decode;
use super::instructions::{BranchRegOp, Condition, Instruction, LogicalOp, MoveWideOp, ShiftType};
use super::interpreter::Cpu;
use super::tier2_jit::BlockExit;
use super::tier3_jit::Tier3JitEngine;
use crate::mmu::Memory;
use ounce_recompiler::ir::{BinOp, BlockBuilder, ExitTo, FaultSite, Op, Reg, Terminator, Value, Width};
use ounce_recompiler::{BlockId, ExitKind, GuestState, MemFault, MemoryBus, PageMapping, Recompiler, WriteEffect};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Most guest instructions in one translated block.
const MAX_BLOCK_INSNS: usize = 64;

// Exit codes of translated blocks (low 16 bits; `SVC` puts its immediate in bits 16..32).
const EXIT_BRANCH: u32 = 0;
const EXIT_COND_TAKEN: u32 = 1;
const EXIT_FALLTHROUGH: u32 = 2;
const EXIT_RETURN: u32 = 3;
const EXIT_SVC: u32 = 4;
/// The next instruction is not translated; continue on Tier-3.
const EXIT_UNSUPPORTED: u32 = 5;

/// Guest memory for generated code. Reports writes that bumped the code-write generation.
struct MmuBus<'a> {
    mmu: &'a mut Memory,
}

impl MemoryBus for MmuBus<'_> {
    fn read(&mut self, va: u64, size: u8) -> Result<u64, MemFault> {
        let r = match size {
            1 => self.mmu.read_u8(va).map(u64::from),
            2 => self.mmu.read_u16(va).map(u64::from),
            4 => self.mmu.read_u32(va).map(u64::from),
            _ => self.mmu.read_u64(va),
        };
        r.map_err(|_| MemFault)
    }

    fn write(&mut self, va: u64, size: u8, value: u64) -> Result<WriteEffect, MemFault> {
        let gen_before = self.mmu.code_write_gen();
        let r = match size {
            1 => self.mmu.write_u8(va, value as u8),
            2 => self.mmu.write_u16(va, value as u16),
            4 => self.mmu.write_u32(va, value as u32),
            _ => self.mmu.write_u64(va, value),
        };
        r.map_err(|_| MemFault)?;
        Ok(if self.mmu.code_write_gen() != gen_before { WriteEffect::CodeWritten } else { WriteEffect::Done })
    }

    fn query_page(&mut self, page_num: u64) -> Option<PageMapping> {
        let page = self.mmu.get_page(page_num)?;
        Some(PageMapping {
            page_num,
            host_base: page.data.as_ptr() as u64,
            read: page.perm.read,
            write: page.perm.write,
            holds_code: page.holds_code,
        })
    }
}

/// Lowers decoded guest instructions to recompiler IR with the interpreter's semantics.
struct Lowerer {
    b: BlockBuilder,
}

impl Lowerer {
    fn k(&mut self, v: u64) -> Value {
        self.b.konst(v)
    }

    /// `read_x`: register 31 reads as zero.
    fn x(&mut self, r: u8) -> Value {
        if r < 31 { self.b.get(Reg::X(r)) } else { self.k(0) }
    }

    /// `read_x_or_sp`.
    fn x_or_sp(&mut self, r: u8) -> Value {
        if r < 31 { self.b.get(Reg::X(r)) } else { self.b.get(Reg::Sp) }
    }

    /// `write_x`: writes to register 31 are dropped.
    fn set_x(&mut self, r: u8, v: Value) {
        if r < 31 {
            self.b.set(Reg::X(r), v);
        }
    }

    /// `write_x_or_sp`.
    fn set_x_or_sp(&mut self, r: u8, v: Value) {
        if r < 31 { self.b.set(Reg::X(r), v) } else { self.b.set(Reg::Sp, v) }
    }

    /// `write_w`: zero-extends.
    fn set_w(&mut self, r: u8, v: Value) {
        let z = self.zext32(v);
        self.set_x(r, z);
    }

    fn zext32(&mut self, v: Value) -> Value {
        self.b.push(Op::Zext(32, v))
    }

    fn width(is_64: bool) -> Width {
        if is_64 { Width::W64 } else { Width::W32 }
    }

    fn nzcv(&mut self) -> Value {
        self.b.get(Reg::Nzcv)
    }

    fn cond(&mut self, cond: Condition) -> Value {
        let f = self.nzcv();
        self.b.push(Op::Cond(cond as u8, f))
    }

    /// The interpreter's register shift for `AddSubReg`/`LogicalReg` (constant amount, non-zero).
    fn shifted(&mut self, raw: Value, shift_type: ShiftType, amount: u8, is_64: bool) -> Value {
        if amount == 0 {
            return raw;
        }
        let op = match shift_type {
            ShiftType::Lsl => BinOp::Shl,
            ShiftType::Lsr => BinOp::Lshr,
            ShiftType::Asr => BinOp::Ashr,
            ShiftType::Ror => BinOp::Rotr,
        };
        let w = Self::width(is_64);
        let amt = if is_64 { amount & 63 } else { amount & 31 };
        self.b.bin_imm(op, w, raw, amt as u64)
    }

    fn add_sub(&mut self, a: Value, b: Value, is_sub: bool, set_flags: bool, is_64: bool) -> Value {
        let w = Self::width(is_64);
        let res = self.b.bin(if is_sub { BinOp::Sub } else { BinOp::Add }, w, a, b);
        if set_flags {
            let f = self.b.push(if is_sub { Op::SubFlags(w, a, b) } else { Op::AddFlags(w, a, b) });
            self.b.set(Reg::Nzcv, f);
        }
        res
    }

    fn logic(&mut self, op: LogicalOp, a: Value, b: Value) -> Value {
        let bop = match op {
            LogicalOp::And | LogicalOp::Ands => BinOp::And,
            LogicalOp::Orr => BinOp::Or,
            LogicalOp::Eor => BinOp::Xor,
        };
        self.b.bin(bop, Width::W64, a, b)
    }

    fn logic_flags(&mut self, res: Value, is_64: bool) {
        let f = self.b.push(Op::LogicFlags(Self::width(is_64), res));
        self.b.set(Reg::Nzcv, f);
    }

    fn exit(&mut self, code: u32, pc: u64, insns: u32) -> ExitTo {
        let pc = self.k(pc);
        ExitTo { code, pc, insns }
    }

    fn load(&mut self, size: u8, addr: Value, site: FaultSite) -> Value {
        self.b.push(Op::Load(size, addr, site))
    }

    fn store(&mut self, size: u8, addr: Value, val: Value, site: FaultSite) {
        self.b.push(Op::Store(size, addr, val, site));
    }

    /// Lowers one instruction. `Ok(None)`: done, continue the block. `Ok(Some(t))`: the block ends
    /// here with `t`. `Err(())`: not translated, the block must end before it.
    fn insn(&mut self, pc: u64, idx: u32, inst: Instruction) -> Result<Option<Terminator>, ()> {
        let site = FaultSite { pc, insns: idx };
        let done = idx + 1;
        match inst {
            Instruction::Nop => {}

            Instruction::Svc { imm } => {
                let e = self.exit(EXIT_SVC | (imm as u32) << 16, pc.wrapping_add(4), done);
                return Ok(Some(Terminator::Exit(e)));
            }

            Instruction::Adr { rd, imm, is_page } => {
                let base = if is_page { pc & !0xFFF } else { pc };
                let v = self.k((base as i64).wrapping_add(imm) as u64);
                self.set_x(rd, v);
            }

            Instruction::MoveWide { rd, imm, shift, op, is_64 } => {
                let shifted = (imm as u64) << shift;
                let v = match op {
                    MoveWideOp::Movz => self.k(if is_64 { shifted } else { shifted as u32 as u64 }),
                    MoveWideOp::Movn => {
                        let mask = if is_64 { u64::MAX } else { u32::MAX as u64 };
                        self.k((!shifted) & mask)
                    }
                    MoveWideOp::Movk => {
                        let cur = self.x(rd);
                        let cur = if is_64 { cur } else { self.zext32(cur) };
                        let kept = self.b.bin_imm(BinOp::And, Width::W64, cur, !(0xFFFFu64 << shift));
                        let r = self.b.bin_imm(BinOp::Or, Width::W64, kept, shifted);
                        if is_64 { r } else { self.zext32(r) }
                    }
                };
                self.set_x(rd, v);
            }

            Instruction::AddSubImm { rd, rn, imm, is_sub, set_flags, is_64 } => {
                let a = if !set_flags && rn == 31 { self.b.get(Reg::Sp) } else { self.x(rn) };
                let b = self.k(imm);
                let res = self.add_sub(a, b, is_sub, set_flags, is_64);
                if set_flags { self.set_x(rd, res) } else { self.set_x_or_sp(rd, res) }
            }

            Instruction::AddSubReg {
                rd, rn, rm, shift_type, shift_amount, is_extended, extend_option, is_sub, set_flags, is_64,
            } => {
                let a = if is_extended && rn == 31 { self.b.get(Reg::Sp) } else { self.x(rn) };
                let raw = self.x(rm);
                let b = if is_extended {
                    let ext = match extend_option {
                        0b000 => self.b.push(Op::Zext(8, raw)),
                        0b001 => self.b.push(Op::Zext(16, raw)),
                        0b010 => self.b.push(Op::Zext(32, raw)),
                        0b100 => self.b.push(Op::Sext(8, raw)),
                        0b101 => self.b.push(Op::Sext(16, raw)),
                        0b110 => self.b.push(Op::Sext(32, raw)),
                        _ => raw,
                    };
                    match shift_amount & 7 {
                        0 => ext,
                        sh => self.b.bin_imm(BinOp::Shl, Width::W64, ext, sh as u64),
                    }
                } else {
                    self.shifted(raw, shift_type, shift_amount, is_64)
                };
                let res = self.add_sub(a, b, is_sub, set_flags, is_64);
                // Extended-register ADD/SUB (not ADDS/SUBS) writes SP for Rd = 31, e.g. `sub sp, sp, x12`.
                // W32 results are already zero-extended.
                if is_extended && !set_flags { self.set_x_or_sp(rd, res) } else { self.set_x(rd, res) }
            }

            Instruction::LogicalReg { rd, rn, rm, shift_type, shift_amount, invert_rm, op, set_flags, is_64 } => {
                let a = self.x(rn);
                let raw = self.x(rm);
                let sh = self.shifted(raw, shift_type, shift_amount, is_64);
                let b = if invert_rm { self.b.push(Op::Not(Width::W64, sh)) } else { sh };
                let res = self.logic(op, a, b);
                let res = if is_64 { res } else { self.zext32(res) };
                self.set_x(rd, res);
                if set_flags {
                    self.logic_flags(res, is_64);
                }
            }

            Instruction::LogicalImm { rd, rn, imm, op, set_flags, is_64 } => {
                let a = self.x(rn);
                let b = self.k(imm);
                let res = self.logic(op, a, b);
                let res = if is_64 { res } else { self.zext32(res) };
                if set_flags {
                    self.logic_flags(res, is_64);
                    self.set_x(rd, res);
                } else {
                    self.set_x_or_sp(rd, res);
                }
            }

            Instruction::Bitfield { rd, rn, immr, imms, opc, is_64 } => {
                let w64 = Width::W64;
                let src = self.x(rn);
                let src = if is_64 { src } else { self.zext32(src) };
                let bits: u8 = if is_64 { 64 } else { 32 };
                let (r, s) = (immr % bits, imms % bits);
                let mask_of = |width: u32| if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
                let res = if s >= r {
                    let width = (s - r + 1) as u32;
                    let mask = mask_of(width);
                    let sh = self.b.bin_imm(BinOp::Lshr, w64, src, r as u64);
                    let extracted = self.b.bin_imm(BinOp::And, w64, sh, mask);
                    match opc {
                        0 => {
                            let shift = (64 - width) as u64;
                            let up = self.b.bin_imm(BinOp::Shl, w64, extracted, shift);
                            self.b.bin_imm(BinOp::Ashr, w64, up, shift)
                        }
                        1 => {
                            let dst = self.x(rd);
                            let dst = if is_64 { dst } else { self.zext32(dst) };
                            let kept = self.b.bin_imm(BinOp::And, w64, dst, !mask);
                            self.b.bin(BinOp::Or, w64, kept, extracted)
                        }
                        _ => extracted,
                    }
                } else {
                    let lshift = (bits - r) as u32;
                    let width = (s + 1) as u32;
                    let mask = mask_of(width);
                    let extracted = self.b.bin_imm(BinOp::And, w64, src, mask);
                    let positioned = self.b.bin_imm(BinOp::Shl, w64, extracted, lshift as u64);
                    match opc {
                        0 => {
                            let top_shift = lshift + width;
                            let top_mask = if top_shift >= 64 { 0 } else { u64::MAX << top_shift };
                            let sign = self.b.bin_imm(BinOp::Lshr, w64, extracted, s as u64);
                            let sign = self.b.bin_imm(BinOp::And, w64, sign, 1);
                            let filled = self.b.bin_imm(BinOp::Or, w64, positioned, top_mask);
                            self.b.push(Op::Select(sign, filled, positioned))
                        }
                        1 => {
                            let dst = self.x(rd);
                            let dst = if is_64 { dst } else { self.zext32(dst) };
                            let kept = self.b.bin_imm(BinOp::And, w64, dst, !mask.wrapping_shl(lshift));
                            self.b.bin(BinOp::Or, w64, kept, positioned)
                        }
                        _ => positioned,
                    }
                };
                if is_64 { self.set_x(rd, res) } else { self.set_w(rd, res) }
            }

            Instruction::Extract { rd, rn, rm, lsb, is_64 } => {
                let w64 = Width::W64;
                if is_64 {
                    let l = (lsb & 63) as u64;
                    let lo = self.x(rm);
                    let res = if l == 0 {
                        lo
                    } else {
                        let hi = self.x(rn);
                        let a = self.b.bin_imm(BinOp::Lshr, w64, lo, l);
                        let b = self.b.bin_imm(BinOp::Shl, w64, hi, 64 - l);
                        self.b.bin(BinOp::Or, w64, a, b)
                    };
                    self.set_x(rd, res);
                } else {
                    let hi = self.x(rn);
                    let hi = self.b.bin_imm(BinOp::Shl, w64, hi, 32);
                    let lo = self.x(rm);
                    let lo = self.zext32(lo);
                    let c = self.b.bin(BinOp::Or, w64, hi, lo);
                    let res = self.b.bin_imm(BinOp::Lshr, w64, c, (lsb & 31) as u64);
                    self.set_w(rd, res);
                }
            }

            Instruction::CondSelect { rd, rn, rm, cond, op, is_64 } => {
                let c = self.cond(cond);
                let n = self.x(rn);
                let m = self.x(rm);
                let alt = match op {
                    0 => m,
                    1 => self.b.bin_imm(BinOp::Add, Width::W64, m, 1),
                    2 => self.b.push(Op::Not(Width::W64, m)),
                    _ => {
                        let z = self.k(0);
                        self.b.bin(BinOp::Sub, Width::W64, z, m)
                    }
                };
                let res = self.b.push(Op::Select(c, n, alt));
                if is_64 { self.set_x(rd, res) } else { self.set_w(rd, res) }
            }

            Instruction::CondCompare { rn, rm_or_imm, nzcv, cond, is_imm, is_sub, is_64 } => {
                let c = self.cond(cond);
                let a = self.x(rn);
                let b = if is_imm { self.k(rm_or_imm as u64) } else { self.x(rm_or_imm) };
                let w = Self::width(is_64);
                let f = self.b.push(if is_sub { Op::SubFlags(w, a, b) } else { Op::AddFlags(w, a, b) });
                let alt = self.k(((nzcv & 0xF) as u64) << 28);
                let res = self.b.push(Op::Select(c, f, alt));
                self.b.set(Reg::Nzcv, res);
            }

            Instruction::MultiplyAddSub { rd, rn, rm, ra, is_sub, is_64 } => {
                let w = Self::width(is_64);
                let n = self.x(rn);
                let m = self.x(rm);
                let a = self.x(ra);
                let p = self.b.bin(BinOp::Mul, w, n, m);
                let res = self.b.bin(if is_sub { BinOp::Sub } else { BinOp::Add }, w, a, p);
                self.set_x(rd, res);
            }

            Instruction::Divide { rd, rn, rm, is_signed, is_64 } => {
                let n = self.x(rn);
                let m = self.x(rm);
                let res = self.b.bin(if is_signed { BinOp::Sdiv } else { BinOp::Udiv }, Self::width(is_64), n, m);
                self.set_x(rd, res);
            }

            Instruction::ShiftReg { rd, rn, rm, shift_type, is_64 } => {
                let op = match shift_type {
                    ShiftType::Lsl => BinOp::Shl,
                    ShiftType::Lsr => BinOp::Lshr,
                    ShiftType::Asr => BinOp::Ashr,
                    ShiftType::Ror => BinOp::Rotr,
                };
                let n = self.x(rn);
                let m = self.x(rm); // IR shifts take the amount modulo the width, as the interpreter's mask
                let res = self.b.bin(op, Self::width(is_64), n, m);
                self.set_x(rd, res);
            }

            Instruction::LoadStoreReg { rt, rn, rm, option, shift, size_bytes, is_load, is_signed, is_64_target } => {
                if !matches!(size_bytes, 1 | 2 | 4 | 8) {
                    return Err(());
                }
                let base = self.x_or_sp(rn);
                let raw = self.x(rm);
                let index = match option {
                    0b010 => self.b.push(Op::Zext(32, raw)),
                    0b110 => self.b.push(Op::Sext(32, raw)),
                    _ => raw,
                };
                let index = self.b.bin_imm(BinOp::Shl, Width::W64, index, shift as u64);
                let addr = self.b.bin(BinOp::Add, Width::W64, base, index);
                if is_load {
                    let v = self.load(size_bytes, addr, site);
                    let v = if is_signed && size_bytes < 8 { self.b.push(Op::Sext(size_bytes * 8, v)) } else { v };
                    if is_64_target || size_bytes == 8 { self.set_x(rt, v) } else { self.set_w(rt, v) }
                } else {
                    let v = self.x(rt);
                    self.store(size_bytes, addr, v, site);
                    self.b.push(Op::EndInsn { next_pc: pc.wrapping_add(4), insns: done });
                }
            }

            Instruction::LoadStoreImm { rt, rn, offset, size_bytes, is_load, is_signed, writeback, post_index } => {
                if !matches!(size_bytes, 1 | 2 | 4 | 8) {
                    return Err(());
                }
                let base = self.x_or_sp(rn);
                let updated = self.b.bin_imm(BinOp::Add, Width::W64, base, offset as u64);
                let addr = if post_index { base } else { updated };
                if is_load {
                    let v = self.load(size_bytes, addr, site);
                    let v = if is_signed && size_bytes < 8 { self.b.push(Op::Sext(size_bytes * 8, v)) } else { v };
                    self.set_x(rt, v);
                } else {
                    let v = self.x(rt);
                    self.store(size_bytes, addr, v, site);
                }
                if writeback || post_index {
                    self.set_x_or_sp(rn, updated);
                }
                if !is_load {
                    self.b.push(Op::EndInsn { next_pc: pc.wrapping_add(4), insns: done });
                }
            }

            Instruction::LoadStorePair { rt1, rt2, rn, offset, is_load, is_64, writeback, post_index } => {
                let base = self.x_or_sp(rn);
                let size: u8 = if is_64 { 8 } else { 4 };
                let updated = self.b.bin_imm(BinOp::Add, Width::W64, base, offset as u64);
                let addr = if post_index { base } else { updated };
                let addr2 = self.b.bin_imm(BinOp::Add, Width::W64, addr, size as u64);
                if is_load {
                    let v1 = self.load(size, addr, site);
                    let v2 = self.load(size, addr2, site);
                    self.set_x(rt1, v1);
                    self.set_x(rt2, v2);
                } else {
                    let v1 = self.x(rt1);
                    let v2 = self.x(rt2);
                    self.store(size, addr, v1, site);
                    self.store(size, addr2, v2, site);
                }
                if writeback || post_index {
                    self.set_x_or_sp(rn, updated);
                }
                if !is_load {
                    self.b.push(Op::EndInsn { next_pc: pc.wrapping_add(4), insns: done });
                }
            }

            Instruction::BranchImm { offset, link } => {
                if link {
                    let ret = self.k(pc.wrapping_add(4));
                    self.set_x(30, ret);
                }
                let e = self.exit(EXIT_BRANCH, (pc as i64).wrapping_add(offset) as u64, done);
                return Ok(Some(Terminator::Exit(e)));
            }

            Instruction::BranchCond { offset, cond } => {
                let c = self.cond(cond);
                return Ok(Some(self.two_way(c, pc, offset, done)));
            }

            Instruction::CompareBranch { rt, offset, is_cbnz, is_64 } => {
                let v = self.x(rt);
                let z = self.b.push(Op::IsZero(Self::width(is_64), v));
                let c = if is_cbnz { self.b.bin_imm(BinOp::Xor, Width::W64, z, 1) } else { z };
                return Ok(Some(self.two_way(c, pc, offset, done)));
            }

            Instruction::TestBranch { rt, bit, offset, is_tbnz } => {
                let v = self.x(rt);
                let sh = self.b.bin_imm(BinOp::Lshr, Width::W64, v, (bit & 63) as u64);
                let set = self.b.bin_imm(BinOp::And, Width::W64, sh, 1);
                let c = if is_tbnz { set } else { self.b.bin_imm(BinOp::Xor, Width::W64, set, 1) };
                return Ok(Some(self.two_way(c, pc, offset, done)));
            }

            Instruction::BranchReg { rn, op } => {
                let target = self.x(rn);
                if op == BranchRegOp::Blr {
                    let ret = self.k(pc.wrapping_add(4));
                    self.set_x(30, ret);
                }
                let code = if op == BranchRegOp::Ret { EXIT_RETURN } else { EXIT_BRANCH };
                return Ok(Some(Terminator::Exit(ExitTo { code, pc: target, insns: done })));
            }

            // System registers, FP/SIMD, exclusives and unknown encodings stay on Tier-3.
            _ => return Err(()),
        }
        Ok(None)
    }

    fn two_way(&mut self, cond: Value, pc: u64, offset: i64, insns: u32) -> Terminator {
        let taken = self.exit(EXIT_COND_TAKEN, (pc as i64).wrapping_add(offset) as u64, insns);
        let not_taken = self.exit(EXIT_FALLTHROUGH, pc.wrapping_add(4), insns);
        Terminator::Branch { cond, taken, not_taken }
    }
}

/// A translated guest block: IR plus the guest range `[start, end)` it was read from.
pub struct LoweredBlock {
    pub ir: ounce_recompiler::ir::Block,
    pub start: u64,
    pub end: u64,
    /// Guest instructions in the block.
    pub insns: usize,
}

/// Decodes from `pc` and lowers up to the first terminator, untranslated instruction or
/// [`MAX_BLOCK_INSNS`]. `None` if the first instruction cannot be read or translated.
pub fn lower_block(pc: u64, mmu: &Memory) -> Option<LoweredBlock> {
    let mut l = Lowerer { b: BlockBuilder::new() };
    let mut cur = pc;
    let mut count = 0u32;
    let term = loop {
        if count as usize == MAX_BLOCK_INSNS {
            break Terminator::Exit(l.exit(EXIT_FALLTHROUGH, cur, count));
        }
        let Ok(raw) = mmu.read_u32(cur) else {
            if count == 0 {
                return None;
            }
            break Terminator::Exit(l.exit(EXIT_UNSUPPORTED, cur, count));
        };
        let before = l.b.len();
        match l.insn(cur, count, decode(raw)) {
            Ok(t) => {
                count += 1;
                cur = cur.wrapping_add(4);
                if let Some(t) = t {
                    break t;
                }
            }
            Err(()) => {
                // `insn` rejects before emitting anything.
                debug_assert_eq!(l.b.len(), before);
                if count == 0 {
                    return None;
                }
                break Terminator::Exit(l.exit(EXIT_UNSUPPORTED, cur, count));
            }
        }
    };
    Some(LoweredBlock { ir: l.b.finish(term), start: pc, end: cur, insns: count as usize })
}

/// Guest PCs are well-distributed 4-byte-aligned keys; a multiply hash beats SipHash on the
/// per-block lookup.
#[derive(Default, Clone, Copy)]
struct PcHasher(u64);

impl std::hash::Hasher for PcHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100_0000_01B3);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (v >> 2).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

type PcMap<V> = HashMap<u64, V, std::hash::BuildHasherDefault<PcHasher>>;

struct Translation {
    id: BlockId,
    start: u64,
    end: u64,
}

/// Tier-4 engine: translated blocks for hot code, Tier-3 for the rest.
pub struct Tier4JitEngine {
    rec: Option<Recompiler>,
    blocks: PcMap<Translation>,
    hot: PcMap<u32>,
    no_compile: HashSet<u64>,
    threshold: u32,
    seen_code_gen: u64,
    /// Runs cold and untranslatable code.
    pub tier3: Tier3JitEngine,
    pub native_block_runs: u64,
    pub native_insns: u64,
    pub chained_jumps: u64,
    pub fallback_insns: u64,
    pub blocks_compiled: u64,
    /// Guest instructions covered by all translations made so far.
    pub translated_insns: u64,
    pub blocks_invalidated: u64,
    pub compile_failures: u64,
    pub mips_rate: f64,
    /// Why native code is off, if it is (no recompiler for this host).
    pub unavailable: Option<String>,
    fast_cache: Box<[(u64, BlockId); 1024]>,
}

impl Default for Tier4JitEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Tier4JitEngine {
    /// Block entries before a block is translated (design section 7).
    pub const HOT_THRESHOLD: u32 = 64;

    pub fn new() -> Self {
        Self::with_threshold(Self::HOT_THRESHOLD)
    }

    pub fn with_threshold(threshold: u32) -> Self {
        let (rec, unavailable) = match Recompiler::new() {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Self {
            rec,
            blocks: PcMap::default(),
            hot: PcMap::default(),
            no_compile: HashSet::new(),
            threshold: threshold.max(1),
            seen_code_gen: 0,
            tier3: Tier3JitEngine::new(),
            native_block_runs: 0,
            native_insns: 0,
            chained_jumps: 0,
            fallback_insns: 0,
            blocks_compiled: 0,
            translated_insns: 0,
            blocks_invalidated: 0,
            compile_failures: 0,
            mips_rate: 0.0,
            unavailable,
            fast_cache: Box::new([(u64::MAX, BlockId::default()); 1024]),
        }
    }

    /// Drops translations whose guest pages were written since the last sweep.
    pub fn sync_code_writes(&mut self, mmu: &Memory) {
        let code_gen = mmu.code_write_gen();
        if code_gen == self.seen_code_gen {
            return;
        }
        let since = self.seen_code_gen;
        let stale: Vec<u64> = self
            .blocks
            .iter()
            .filter(|(_, t)| mmu.code_written_since(t.start, t.end, since))
            .map(|(&pc, _)| pc)
            .collect();
        for pc in stale {
            if let (Some(t), Some(rec)) = (self.blocks.remove(&pc), self.rec.as_mut()) {
                rec.remove(t.id);
            }
            self.hot.remove(&pc);
            self.blocks_invalidated += 1;
        }
        // Code that changed may be translatable now.
        self.no_compile.clear();
        self.fast_cache.fill((u64::MAX, BlockId::default()));
        if let Some(rec) = self.rec.as_mut() {
            rec.flush_tlb();
        }
        self.seen_code_gen = code_gen;
    }

    #[inline]
    fn lookup_or_compile_fast(&mut self, pc: u64, mmu: &mut Memory) -> Option<BlockId> {
        let idx = ((pc >> 2) as usize) & 1023;
        if self.fast_cache[idx].0 == pc {
            return Some(self.fast_cache[idx].1);
        }
        let id = self.lookup_or_compile(pc, mmu)?;
        self.fast_cache[idx] = (pc, id);
        Some(id)
    }

    fn lookup_or_compile(&mut self, pc: u64, mmu: &mut Memory) -> Option<BlockId> {
        let rec = self.rec.as_mut()?;
        if let Some(t) = self.blocks.get(&pc) {
            return Some(t.id);
        }
        if self.no_compile.contains(&pc) {
            return None;
        }
        let n = self.hot.entry(pc).or_insert(0);
        *n += 1;
        if *n < self.threshold {
            return None;
        }
        let Some(lowered) = lower_block(pc, mmu) else {
            self.no_compile.insert(pc);
            return None;
        };
        match rec.compile(&lowered.ir) {
            Ok(id) => {
                mmu.mark_code_range(lowered.start, lowered.end);
                self.blocks.insert(pc, Translation { id, start: lowered.start, end: lowered.end });
                self.blocks_compiled += 1;
                self.translated_insns += lowered.insns as u64;
                Some(id)
            }
            Err(_) => {
                self.compile_failures += 1;
                self.no_compile.insert(pc);
                None
            }
        }
    }

    /// Runs translated blocks back to back, keeping guest state in a [`GuestState`] until a block
    /// exits with anything but a plain branch, the next `pc` has no translation, or `budget` is
    /// used up. `None` if `cpu.regs.pc` has no translation (yet).
    /// Returns (instructions, exit to report, continue on Tier-3).
    fn run_native(&mut self, cpu: &mut Cpu, mmu: &mut Memory, budget: usize) -> Option<(usize, Option<BlockExit>, bool)> {
        let mut id = self.lookup_or_compile_fast(cpu.regs.pc, mmu)?;
        let r = &mut cpu.regs;
        let mut state = GuestState { x: r.x, sp: r.sp, pc: r.pc, nzcv: r.pstate.to_bits() as u64 };
        let mut executed = 0usize;
        let outcome = loop {
            let exit = self.rec.as_mut()?.run(id, &mut state, &mut MmuBus { mmu });
            self.native_block_runs += 1;
            self.native_insns += exit.insns as u64;
            executed += exit.insns as usize;
            let pc = state.pc;
            match exit.kind {
                ExitKind::Fault => break (Some(BlockExit::Fault { pc }), false),
                ExitKind::CodeWritten => break (None, false),
                ExitKind::Code(code) => match code & 0xFFFF {
                    EXIT_RETURN => break (Some(BlockExit::Return { target: pc }), false),
                    EXIT_SVC => break (Some(BlockExit::Svc { svc_id: (code >> 16) as u16 }), false),
                    EXIT_UNSUPPORTED => break (None, true),
                    _ => {
                        self.chained_jumps += 1;
                    }
                },
            }
            if executed >= budget {
                break (None, false);
            }
            // Guest code pages only change through a store, which exits with `CodeWritten`, so the
            // translations looked up here are current.
            match self.lookup_or_compile_fast(pc, mmu) {
                Some(next) => id = next,
                None => break (None, false),
            }
        };
        let r = &mut cpu.regs;
        r.x = state.x;
        r.sp = state.sp;
        r.pc = state.pc;
        r.pstate.from_bits(state.nzcv as u32);
        Some((executed, outcome.0, outcome.1))
    }

    /// Runs one Tier-3 step (one super-block or one interpreted instruction).
    fn run_fallback(&mut self, cpu: &mut Cpu, mmu: &mut Memory) -> (usize, Option<BlockExit>) {
        let (n, exit) = self.tier3.run_slice(cpu, mmu, 1);
        self.fallback_insns += n as u64;
        (n, if exit == BlockExit::BudgetExceeded { None } else { Some(exit) })
    }

    /// Same contract as [`Tier3JitEngine::run_slice`]: runs until `max_instructions`, or returns
    /// early on `SVC`, `RET` or a fault.
    pub fn run_slice(&mut self, cpu: &mut Cpu, mmu: &mut Memory, max_instructions: usize) -> (usize, BlockExit) {
        let start = Instant::now();
        let mut executed = 0usize;
        let mut result = BlockExit::BudgetExceeded;
        while executed < max_instructions {
            self.sync_code_writes(mmu);
            let (n, exit) = match self.run_native(cpu, mmu, max_instructions - executed) {
                Some((n, exit, false)) => (n, exit),
                Some((n, None, true)) => {
                    let (m, exit) = self.run_fallback(cpu, mmu);
                    (n + m, exit)
                }
                Some((n, exit, true)) => (n, exit),
                None => self.run_fallback(cpu, mmu),
            };
            executed += n;
            if let Some(exit) = exit {
                result = exit;
                break;
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.mips_rate = (executed as f64 / elapsed) / 1_000_000.0;
        }
        (executed, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::interpreter::StepResult;
    use crate::mmu::Permission;

    const CODE: u64 = 0x8000_0000;
    const DATA: u64 = 0x9000_0000;

    fn setup(words: &[u32]) -> (Cpu, Memory) {
        let mut mmu = Memory::new();
        mmu.map_range(CODE, 0x4000, Permission::READ_WRITE_EXECUTE);
        mmu.map_range(DATA, 0x1000, Permission::READ_WRITE);
        for (i, &w) in words.iter().enumerate() {
            mmu.write_u32(CODE + 4 * i as u64, w).unwrap();
        }
        let mut cpu = Cpu::new();
        cpu.regs.pc = CODE;
        cpu.regs.sp = DATA + 0x800;
        (cpu, mmu)
    }

    #[derive(Debug, PartialEq)]
    struct Outcome {
        x: [u64; 31],
        sp: u64,
        pc: u64,
        nzcv: u32,
        exit: BlockExit,
        data: Vec<u8>,
    }

    fn outcome(cpu: &Cpu, mmu: &Memory, exit: BlockExit) -> Outcome {
        let mut data = vec![0u8; 0x1000];
        mmu.read_bytes(DATA, &mut data).unwrap();
        Outcome { x: cpu.regs.x, sp: cpu.regs.sp, pc: cpu.regs.pc, nzcv: cpu.regs.pstate.to_bits(), exit, data }
    }

    /// Reference: the interpreter, one instruction at a time, until `SVC` or a fault.
    fn run_interp(mut cpu: Cpu, mut mmu: Memory, limit: usize) -> Outcome {
        for _ in 0..limit {
            let pc = cpu.regs.pc;
            match cpu.step(&mut mmu) {
                Ok(StepResult::Continue) => {}
                Ok(StepResult::Svc(imm)) => return outcome(&cpu, &mmu, BlockExit::Svc { svc_id: imm }),
                _ => return outcome(&cpu, &mmu, BlockExit::Fault { pc }),
            }
        }
        outcome(&cpu, &mmu, BlockExit::BudgetExceeded)
    }

    /// Runs slices until `SVC`, a fault or `limit`; `RET` exits are stepped over like the harness.
    fn run_engine(mut slice: impl FnMut(&mut Cpu, &mut Memory, usize) -> (usize, BlockExit), mut cpu: Cpu, mut mmu: Memory, limit: usize) -> Outcome {
        let mut total = 0;
        while total < limit {
            let (n, exit) = slice(&mut cpu, &mut mmu, limit - total);
            total += n;
            match exit {
                BlockExit::Return { .. } | BlockExit::BudgetExceeded if total < limit => {}
                BlockExit::Svc { .. } | BlockExit::Fault { .. } => return outcome(&cpu, &mmu, exit),
                _ => break,
            }
        }
        outcome(&cpu, &mmu, BlockExit::BudgetExceeded)
    }

    fn run_tier4(cpu: Cpu, mmu: Memory, limit: usize) -> (Outcome, Tier4JitEngine) {
        let mut e = Tier4JitEngine::with_threshold(1);
        let o = run_engine(|c, m, n| e.run_slice(c, m, n), cpu, mmu, limit);
        (o, e)
    }

    fn run_tier3(cpu: Cpu, mmu: Memory, limit: usize) -> Outcome {
        let mut e = Tier3JitEngine::new();
        run_engine(|c, m, n| e.run_slice(c, m, n), cpu, mmu, limit)
    }

    fn clone(cpu: &Cpu, mmu: &Memory) -> (Cpu, Memory) {
        let mut c = Cpu::new();
        c.regs = cpu.regs.clone();
        let mut m = Memory::new();
        m.map_range(CODE, 0x4000, Permission::READ_WRITE_EXECUTE);
        m.map_range(DATA, 0x1000, Permission::READ_WRITE);
        let mut buf = vec![0u8; 0x4000];
        mmu.read_bytes(CODE, &mut buf).unwrap();
        m.write_bytes(CODE, &buf).unwrap();
        let mut buf = vec![0u8; 0x1000];
        mmu.read_bytes(DATA, &mut buf).unwrap();
        m.write_bytes(DATA, &buf).unwrap();
        (c, m)
    }

    /// Interpreter, Tier-3 and Tier-4 must agree.
    fn check_all_tiers(words: &[u32], init: impl Fn(&mut Cpu)) -> Tier4JitEngine {
        let (mut cpu, mmu) = setup(words);
        init(&mut cpu);
        let (c, m) = clone(&cpu, &mmu);
        let reference = run_interp(c, m, 100_000);
        assert!(matches!(reference.exit, BlockExit::Svc { .. }), "guest did not reach SVC: {:?}", reference.exit);
        let (c, m) = clone(&cpu, &mmu);
        assert_eq!(run_tier3(c, m, 100_000), reference, "Tier-3 differs from the interpreter");
        let (t4, engine) = run_tier4(cpu, mmu, 100_000);
        assert_eq!(t4, reference, "Tier-4 differs from the interpreter");
        assert!(engine.native_insns > 0, "nothing ran natively");
        engine
    }

    #[test]
    fn fibonacci_loop_matches_all_tiers() {
        #[rustfmt::skip]
        let e = check_all_tiers(&[
            0xD2800141, // movz x1, #10
            0xD2800002, // movz x2, #0
            0xD2800023, // movz x3, #1
            0xB40000C1, // cbz  x1, done
            0x8B030044, // add  x4, x2, x3
            0x91000062, // add  x2, x3, #0
            0x91000083, // add  x3, x4, #0
            0xF1000421, // subs x1, x1, #1
            0x17FFFFFB, // b    loop
            0x91000040, // add  x0, x2, #0
            0xD40000E1, // svc  #7
        ], |_| {});
        assert!(e.blocks_compiled >= 3);
    }

    #[test]
    fn fibonacci_loop_block_chaining() {
        let (mut cpu, mut mmu) = setup(&[
            0xD2800141, // movz x1, #10
            0xD2800002, // movz x2, #0
            0xD2800023, // movz x3, #1
            // loop:
            0xB40000C1, // cbz  x1, done
            0x8B030044, // add  x4, x2, x3
            0x91000062, // add  x2, x3, #0
            0x91000083, // add  x3, x4, #0
            0xF1000421, // subs x1, x1, #1
            0x17FFFFFB, // b    loop
            // done:
            0x91000040, // add  x0, x2, #0
            0xD40000E1, // svc  #7
        ]);
        let mut e = Tier4JitEngine::with_threshold(1);
        let (_n, exit) = e.run_slice(&mut cpu, &mut mmu, 10_000);
        assert_eq!(exit, BlockExit::Svc { svc_id: 7 });
        assert_eq!(cpu.regs.read_x(0), 55);
        assert!(e.native_insns >= 50);
        assert!(e.chained_jumps >= 9, "loop must chain consecutive blocks directly (got {})", e.chained_jumps);
    }

    #[test]
    fn stage20_nested_loop_matches_all_tiers() {
        #[rustfmt::skip]
        check_all_tiers(&[
            0xD2800C81, 0xD2800002, 0xD2800643, 0x8B030042,
            0xF1000463, 0x54FFFFC1, 0xF1000421, 0x54FFFF41,
            0xD2815540, 0xD40000E1,
        ], |_| {});
    }

    #[test]
    fn memory_pairs_writeback_and_calls_match_all_tiers() {
        #[rustfmt::skip]
        check_all_tiers(&[
            0xA9BF7BFD, // 00: stp  x29, x30, [sp, #-16]!
            0xD2824681, // 04: movz x1, #0x1234
            0xF81F8FE1, // 08: str  x1, [sp, #-8]!
            0x39400FE2, // 0C: ldrb w2, [sp, #3]
            0x79C003E3, // 10: ldrsh x3, [sp]
            0x94000004, // 14: bl   0x24
            0xF84087E4, // 18: ldr  x4, [sp], #8
            0xA8C17BFD, // 1C: ldp  x29, x30, [sp], #16
            0xD4000001, // 20: svc  #0
            0xB8256BE1, // 24: str  w1, [sp, x5]
            0xD65F03C0, // 28: ret
        ], |cpu| {
            cpu.regs.write_x(29, 0x29);
            cpu.regs.write_x(5, 8);
        });
    }

    #[test]
    fn alu_mix_matches_interpreter_and_tier3() {
        #[rustfmt::skip]
        check_all_tiers(&[
            0x92800000, // movn x0, #0
            0x12800001, // movn w1, #0
            0xF2A00020, // movk x0, #1, lsl #16
            0xD3441C02, // ubfx x2, x0, #4, #4
            0x93407C23, // sxtw x3, w1
            0xD37CEC24, // lsl  x4, x1, #4
            0xB3410C05, // bfxil x5, x0, #1, #3
            0x93C02025, // extr x5, x1, x0, #8
            0xAB010006, // adds x6, x0, x1
            0x9A8103E7, // csel x7, xzr, x1, eq
            0xFA410804, // ccmp x0, #1, #4, eq
            0x9B017C08, // mul  x8, x0, x1
            0x9AC10C09, // sdiv x9, x0, x1
            0x9ADF080A, // udiv x10, x0, xzr
            0x9AC1200B, // lsl  x11, x0, x1
            0x8A20000C, // bic  x12, x0, x0
            0xEA01001F, // tst  x0, x1
            0x1A9F17ED, // cset w13, eq
            0x10000020, // adr  x0, #4
            0xD4000001, // svc  #0
        ], |_| {});
    }

    #[test]
    fn precise_fault_matches_interpreter() {
        // The second load of the pair faults: neither register may change.
        #[rustfmt::skip]
        let words = [
            0xD2800041, // movz x1, #2
            0xA9400C02, // ldp  x2, x3, [x0]
            0xD4000001, // svc  #0
        ];
        let (mut cpu, mmu) = setup(&words);
        cpu.regs.write_x(0, DATA + 0x1000 - 8);
        cpu.regs.write_x(2, 0x22);
        cpu.regs.write_x(3, 0x33);
        let (c, m) = clone(&cpu, &mmu);
        let reference = run_interp(c, m, 100);
        assert_eq!(reference.exit, BlockExit::Fault { pc: CODE + 4 });
        let (t4, _) = run_tier4(cpu, mmu, 100);
        assert_eq!(t4, reference);
        assert_eq!((t4.x[1], t4.x[2], t4.x[3]), (2, 0x22, 0x33));
    }

    #[test]
    fn store_into_translated_code_is_seen_next_run() {
        #[rustfmt::skip]
        let words = [
            0xD2800020, // 00: movz x0, #1
            0xB9000041, // 04: str  w1, [x2]      ; patches 0x00 after the first round
            0xD4000001, // 08: svc  #0
        ];
        let (mut cpu, mut mmu) = setup(&words);
        let mut e = Tier4JitEngine::with_threshold(1);
        cpu.regs.write_x(1, 0xD280_0040); // movz x0, #2
        cpu.regs.write_x(2, CODE);
        assert_eq!(e.run_slice(&mut cpu, &mut mmu, 100).1, BlockExit::Svc { svc_id: 0 });
        assert_eq!(cpu.regs.read_x(0), 1);
        cpu.regs.pc = CODE;
        assert_eq!(e.run_slice(&mut cpu, &mut mmu, 100).1, BlockExit::Svc { svc_id: 0 });
        assert_eq!(cpu.regs.read_x(0), 2, "ran stale translation");
        assert!(e.blocks_invalidated >= 1);
    }

    #[test]
    fn untranslated_instruction_falls_back_to_tier3() {
        #[rustfmt::skip]
        let e = check_all_tiers(&[
            0xD2800540, // movz x0, #42
            0xD53BD041, // mrs  x1, tpidr_el0      ; not translated
            0x91000421, // add  x1, x1, #1
            0xD4000001, // svc  #0
        ], |cpu| cpu.regs.tpidr_el0 = 0x77);
        assert!(e.fallback_insns >= 1);
    }

    #[test]
    fn lowering_stops_before_untranslated_instruction() {
        let (_, mmu) = setup(&[0xD2800540, 0xD53BD041, 0xD4000001]); // movz, mrs, svc
        let b = lower_block(CODE, &mmu).unwrap();
        assert_eq!((b.insns, b.start, b.end), (1, CODE, CODE + 4));
        assert!(lower_block(CODE + 4, &mmu).is_none(), "a block cannot start with MRS");
    }

    #[test]
    fn hot_threshold_delays_translation() {
        let (mut cpu, mut mmu) = setup(&[0xD2800020, 0xD4000001]);
        let mut e = Tier4JitEngine::with_threshold(3);
        for _ in 0..2 {
            cpu.regs.pc = CODE;
            e.run_slice(&mut cpu, &mut mmu, 10);
        }
        assert_eq!(e.blocks_compiled, 0);
        cpu.regs.pc = CODE;
        e.run_slice(&mut cpu, &mut mmu, 10);
        assert_eq!(e.blocks_compiled, 1);
    }

    /// Small seeded generator (xorshift64*), so every failure reproduces from its seed.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
    }

    fn translatable_straight_line(inst: Instruction) -> bool {
        matches!(
            inst,
            Instruction::AddSubImm { .. }
                | Instruction::AddSubReg { .. }
                | Instruction::LogicalReg { .. }
                | Instruction::LogicalImm { .. }
                | Instruction::Bitfield { .. }
                | Instruction::Extract { .. }
                | Instruction::CondSelect { .. }
                | Instruction::CondCompare { .. }
                | Instruction::MoveWide { .. }
                | Instruction::MultiplyAddSub { .. }
                | Instruction::Divide { .. }
                | Instruction::ShiftReg { .. }
                | Instruction::Adr { .. }
                | Instruction::LoadStoreImm { .. }
                | Instruction::LoadStoreReg { .. }
                | Instruction::LoadStorePair { .. }
        )
    }

    /// Random straight-line blocks of translatable instructions: Tier-3 and Tier-4 must match the
    /// interpreter bit for bit (registers, SP, PC, NZCV, exit, data page).
    #[test]
    fn random_blocks_match_interpreter_and_tier3() {
        let mut rng = Rng(0x0BAD_5EED_1234_5678);
        // Opcode classes the decoder translates: data processing (imm and reg) and loads/stores.
        let classes: [(u32, u32); 4] = [
            (0x1000_0000, 0x1C00_0000), // data processing, immediate
            (0x0A00_0000, 0x0E00_0000), // data processing, register
            (0x0800_0000, 0x0A00_0000), // loads and stores
            (0x1A00_0000, 0x1E00_0000), // data processing, register (cont.)
        ];
        let mut cases = 0;
        let (mut native, mut faults, mut kinds) = (0u64, 0u32, HashSet::new());
        while cases < 10_000 {
            let len = 1 + (rng.next() % 12) as usize;
            let mut words = Vec::with_capacity(len + 1);
            while words.len() < len {
                let (val, mask) = classes[(rng.next() % 4) as usize];
                let raw = ((rng.next() as u32) & !mask) | val;
                if translatable_straight_line(decode(raw)) {
                    words.push(raw);
                }
            }
            words.push(0xD4000001); // svc #0
            let (mut cpu, mmu) = setup(&words);
            for r in 0..31u8 {
                let v = match rng.next() % 6 {
                    0 => rng.next(),
                    1 => rng.next() % 64,
                    2 => DATA + 0xFF8 + (rng.next() % 16), // near page boundary
                    3 => DATA + 0x1000 + (rng.next() % 0x100), // unmapped page
                    _ => DATA + (rng.next() % 0x800) & !7,
                };
                cpu.regs.write_x(r, v);
            }
            cpu.regs.pstate.from_bits((rng.next() as u32) & 0xF000_0000);
            let (c, m) = clone(&cpu, &mmu);
            let reference = run_interp(c, m, 1_000);
            let (c, m) = clone(&cpu, &mmu);
            assert_eq!(run_tier3(c, m, 1_000), reference, "Tier-3, case {cases}: {words:08X?}");
            let (t4, e) = run_tier4(cpu, mmu, 1_000);
            assert_eq!(t4, reference, "case {cases}: {words:08X?}");
            native += e.native_insns;
            faults += matches!(reference.exit, BlockExit::Fault { .. }) as u32;
            for w in &words {
                kinds.insert(std::mem::discriminant(&decode(*w)));
            }
            cases += 1;
        }
        eprintln!("random: {native} native insns, {faults}/10000 faulted, {} instruction kinds", kinds.len());
        assert!(native > 25_000 && faults < 7_500 && kinds.len() >= 14);
    }
}
