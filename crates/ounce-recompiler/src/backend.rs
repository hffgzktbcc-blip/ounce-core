//! IR -> host machine code with Cranelift (design section 4). Safe code only: the bytes go into
//! an `exec::BlockCode`, which owns the executable memory.
//!
//! Generated function: `extern "C" fn(state: *mut GuestState, ctx: *mut CallCtx) -> u64`.
//! Guest registers are loaded from and stored to `state` at every `GetReg`/`SetReg`, so the state
//! in memory is always current when a memory callback runs and faults are precise (section 5).
//! The return value is `code | insns << 32`; the exit `pc` is stored in `state.pc`.

use crate::exec::{self, BlockCode, MEM_CODE_WRITTEN, MEM_FAULT};
use crate::ir::{BinOp, Block, ExitTo, FaultSite, Op, Reg, Terminator, Value, Width};
use crate::{CompileError, GuestState, EXIT_CODE_WRITTEN, EXIT_FAULT};
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{self as clif, types, AbiParam, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind};
use cranelift_codegen::isa::{OwnedTargetIsa, TargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use std::mem::offset_of;

pub(crate) fn host_isa() -> Result<OwnedTargetIsa, CompileError> {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").map_err(|e| CompileError::Backend(e.to_string()))?;
    let isa = cranelift_native::builder().map_err(|e| CompileError::Backend(e.to_string()))?;
    let isa = isa
        .finish(settings::Flags::new(flags))
        .map_err(|e| CompileError::Backend(e.to_string()))?;
    if isa.pointer_bits() != 64 {
        return Err(CompileError::Backend("64-bit host required".into()));
    }
    Ok(isa)
}

fn reg_offset(reg: Reg) -> i32 {
    (match reg {
        Reg::X(n) => offset_of!(GuestState, x) + 8 * n as usize,
        Reg::Sp => offset_of!(GuestState, sp),
        Reg::Nzcv => offset_of!(GuestState, nzcv),
    }) as i32
}

const PC_OFFSET: i32 = offset_of!(GuestState, pc) as i32;

pub(crate) fn compile(
    isa: &dyn TargetIsa,
    ctx: &mut Context,
    fctx: &mut FunctionBuilderContext,
    block: &Block,
) -> Result<BlockCode, CompileError> {
    block.validate().map_err(CompileError::InvalidIr)?;

    let call_conv = isa.default_call_conv();
    let mut sig = clif::Signature::new(call_conv);
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));

    ctx.clear();
    ctx.func.signature = sig;
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, fctx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let state = b.block_params(entry)[0];
        let call_ctx = b.block_params(entry)[1];

        let mut mem_sig = clif::Signature::new(call_conv);
        for t in [types::I64, types::I64, types::I32, types::I64] {
            mem_sig.params.push(AbiParam::new(t));
        }
        mem_sig.returns.push(AbiParam::new(types::I32));
        let mem_sig = b.import_signature(mem_sig);

        let slot = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let stop = b.declare_var(types::I64);
        let zero = b.ins().iconst(types::I64, 0);
        b.def_var(stop, zero);

        let mut lw = Lowering { b, state, call_ctx, mem_sig, slot, stop, vals: Vec::with_capacity(block.ops.len()) };
        for op in &block.ops {
            let v = lw.op(op);
            lw.vals.push(v);
        }
        match block.term {
            Terminator::Exit(e) => lw.exit(e),
            Terminator::Branch { cond, taken, not_taken } => {
                let c = lw.v(cond);
                let t = lw.b.create_block();
                let n = lw.b.create_block();
                lw.b.ins().brif(c, t, &[], n, &[]);
                lw.b.switch_to_block(t);
                lw.exit(taken);
                lw.b.switch_to_block(n);
                lw.exit(not_taken);
            }
        }
        lw.b.seal_all_blocks();
        lw.b.finalize(isa.frontend_config());
    }

    let compiled = ctx
        .compile(isa, &mut ControlPlane::default())
        .map_err(|e| CompileError::Backend(format!("{:?}", e.inner)))?;
    if !compiled.buffer.relocs().is_empty() {
        return Err(CompileError::Backend("generated code needs relocations".into()));
    }
    BlockCode::new(compiled.code_buffer()).map_err(CompileError::Exec)
}

struct Lowering<'a> {
    b: FunctionBuilder<'a>,
    state: clif::Value,
    call_ctx: clif::Value,
    mem_sig: clif::SigRef,
    slot: clif::StackSlot,
    stop: Variable,
    /// Cranelift value per IR op; `None` for ops without a result.
    vals: Vec<Option<clif::Value>>,
}

impl Lowering<'_> {
    fn v(&self, v: Value) -> clif::Value {
        self.vals[v.0 as usize].expect("validated IR")
    }

    fn flags() -> MemFlagsData {
        MemFlagsData::trusted()
    }

    fn narrow(&mut self, w: Width, v: clif::Value) -> clif::Value {
        match w {
            Width::W64 => v,
            Width::W32 => self.b.ins().ireduce(types::I32, v),
        }
    }

    fn widen(&mut self, w: Width, v: clif::Value) -> clif::Value {
        match w {
            Width::W64 => v,
            Width::W32 => self.b.ins().uextend(types::I64, v),
        }
    }

    /// 0/1 as i64 from an icmp result.
    fn bool64(&mut self, v: clif::Value) -> clif::Value {
        self.b.ins().uextend(types::I64, v)
    }

    fn pack_nzcv(&mut self, n: clif::Value, z: clif::Value, c: clif::Value, v: clif::Value) -> clif::Value {
        let mut acc = None;
        for (bit, flag) in [(31, n), (30, z), (29, c), (28, v)] {
            let f = self.bool64(flag);
            let f = self.b.ins().ishl_imm_u(f, bit);
            acc = Some(match acc {
                None => f,
                Some(a) => self.b.ins().bor(a, f),
            });
        }
        acc.unwrap()
    }

    fn nz(&mut self, r: clif::Value) -> (clif::Value, clif::Value) {
        let n = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, r, 0);
        let z = self.b.ins().icmp_imm_u(IntCC::Equal, r, 0);
        (n, z)
    }

    fn ret(&mut self, code: u32, insns: u32) {
        let word = self.b.ins().iconst(types::I64, (code as u64 | (insns as u64) << 32) as i64);
        self.b.ins().return_(&[word]);
    }

    fn exit(&mut self, e: ExitTo) {
        let pc = self.v(e.pc);
        self.b.ins().store(Self::flags(), pc, self.state, PC_OFFSET);
        self.ret(e.code, e.insns);
    }

    /// Branches to a cold block that stores the faulting `pc` and returns `EXIT_FAULT` when
    /// `status == MEM_FAULT`.
    fn fault_check(&mut self, status: clif::Value, site: FaultSite) {
        let is_fault = self.b.ins().icmp_imm_u(IntCC::Equal, status, MEM_FAULT as i64);
        let fault = self.b.create_block();
        let cont = self.b.create_block();
        self.b.set_cold_block(fault);
        self.b.ins().brif(is_fault, fault, &[], cont, &[]);
        self.b.switch_to_block(fault);
        let pc = self.b.ins().iconst(types::I64, site.pc as i64);
        self.b.ins().store(Self::flags(), pc, self.state, PC_OFFSET);
        self.ret(EXIT_FAULT, site.insns);
        self.b.switch_to_block(cont);
    }

    fn mem_call(&mut self, target: u64, va: clif::Value, size: u8, last: clif::Value) -> clif::Value {
        let callee = self.b.ins().iconst(types::I64, target as i64);
        let size = self.b.ins().iconst(types::I32, size as i64);
        let call = self.b.ins().call_indirect(self.mem_sig, callee, &[self.call_ctx, va, size, last]);
        self.b.inst_results(call)[0]
    }

    fn binop(&mut self, op: BinOp, w: Width, a: clif::Value, bv: clif::Value) -> clif::Value {
        let a = self.narrow(w, a);
        let bv = self.narrow(w, bv);
        let ty = match w {
            Width::W32 => types::I32,
            Width::W64 => types::I64,
        };
        let r = match op {
            BinOp::Add => self.b.ins().iadd(a, bv),
            BinOp::Sub => self.b.ins().isub(a, bv),
            BinOp::Mul => self.b.ins().imul(a, bv),
            BinOp::And => self.b.ins().band(a, bv),
            BinOp::Or => self.b.ins().bor(a, bv),
            BinOp::Xor => self.b.ins().bxor(a, bv),
            // Cranelift masks shift amounts to the type width, which is the IR rule.
            BinOp::Shl => self.b.ins().ishl(a, bv),
            BinOp::Lshr => self.b.ins().ushr(a, bv),
            BinOp::Ashr => self.b.ins().sshr(a, bv),
            BinOp::Rotr => self.b.ins().rotr(a, bv),
            BinOp::Udiv => {
                let is0 = self.b.ins().icmp_imm_u(IntCC::Equal, bv, 0);
                let one = self.b.ins().iconst(ty, 1);
                let zero = self.b.ins().iconst(ty, 0);
                let safe = self.b.ins().select(is0, one, bv);
                let q = self.b.ins().udiv(a, safe);
                self.b.ins().select(is0, zero, q)
            }
            BinOp::Sdiv => {
                let is0 = self.b.ins().icmp_imm_u(IntCC::Equal, bv, 0);
                let is_m1 = self.b.ins().icmp_imm_s(IntCC::Equal, bv, -1);
                let special = self.b.ins().bor(is0, is_m1);
                let one = self.b.ins().iconst(ty, 1);
                let zero = self.b.ins().iconst(ty, 0);
                let safe = self.b.ins().select(special, one, bv);
                let q = self.b.ins().sdiv(a, safe);
                let neg = self.b.ins().ineg(a);
                let q = self.b.ins().select(is_m1, neg, q);
                self.b.ins().select(is0, zero, q)
            }
        };
        self.widen(w, r)
    }

    fn cond(&mut self, cc: u8, nzcv: clif::Value) -> clif::Value {
        let bit = |l: &mut Self, sh: i64| {
            let s = l.b.ins().ushr_imm_u(nzcv, sh);
            l.b.ins().band_imm_u(s, 1)
        };
        let not = |l: &mut Self, v: clif::Value| l.b.ins().bxor_imm_u(v, 1);
        let r = match cc >> 1 {
            0 => bit(self, 30),                // EQ: Z
            1 => bit(self, 29),                // CS: C
            2 => bit(self, 31),                // MI: N
            3 => bit(self, 28),                // VS: V
            4 => {
                // HI: C && !Z
                let c = bit(self, 29);
                let z = bit(self, 30);
                let nz = not(self, z);
                self.b.ins().band(c, nz)
            }
            5 => {
                // GE: N == V
                let n = bit(self, 31);
                let v = bit(self, 28);
                let x = self.b.ins().bxor(n, v);
                not(self, x)
            }
            6 => {
                // GT: !Z && N == V
                let n = bit(self, 31);
                let v = bit(self, 28);
                let z = bit(self, 30);
                let x = self.b.ins().bxor(n, v);
                let nx = self.b.ins().bor(x, z);
                not(self, nx)
            }
            _ => return self.b.ins().iconst(types::I64, 1), // AL, NV
        };
        // Odd codes are the negation, except AL/NV handled above.
        if cc & 1 == 1 { not(self, r) } else { r }
    }

    fn op(&mut self, op: &Op) -> Option<clif::Value> {
        Some(match *op {
            Op::Const(c) => self.b.ins().iconst(types::I64, c as i64),
            Op::GetReg(r) => self.b.ins().load(types::I64, Self::flags(), self.state, reg_offset(r)),
            Op::SetReg(r, v) => {
                let v = self.v(v);
                self.b.ins().store(Self::flags(), v, self.state, reg_offset(r));
                return None;
            }
            Op::Bin(op, w, a, bv) => {
                let (a, bv) = (self.v(a), self.v(bv));
                self.binop(op, w, a, bv)
            }
            Op::Not(w, v) => {
                let v = self.v(v);
                let v = self.narrow(w, v);
                let r = self.b.ins().bnot(v);
                self.widen(w, r)
            }
            Op::Sext(bits, v) | Op::Zext(bits, v) => {
                let v = self.v(v);
                if bits == 64 {
                    v
                } else {
                    let ty = clif::Type::int(bits as u16).expect("validated width");
                    let n = self.b.ins().ireduce(ty, v);
                    if matches!(op, Op::Sext(..)) {
                        self.b.ins().sextend(types::I64, n)
                    } else {
                        self.b.ins().uextend(types::I64, n)
                    }
                }
            }
            Op::AddFlags(w, a, bv) | Op::SubFlags(w, a, bv) => {
                let a = self.v(a);
                let bv = self.v(bv);
                let a = self.narrow(w, a);
                let bv = self.narrow(w, bv);
                let (r, c, v) = if matches!(op, Op::AddFlags(..)) {
                    let r = self.b.ins().iadd(a, bv);
                    let c = self.b.ins().icmp(IntCC::UnsignedLessThan, r, a);
                    // Overflow: both inputs differ in sign from the result.
                    let x1 = self.b.ins().bxor(a, r);
                    let x2 = self.b.ins().bxor(bv, r);
                    let x = self.b.ins().band(x1, x2);
                    let v = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, x, 0);
                    (r, c, v)
                } else {
                    let r = self.b.ins().isub(a, bv);
                    let c = self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, a, bv);
                    let x1 = self.b.ins().bxor(a, bv);
                    let x2 = self.b.ins().bxor(a, r);
                    let x = self.b.ins().band(x1, x2);
                    let v = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, x, 0);
                    (r, c, v)
                };
                let (n, z) = self.nz(r);
                self.pack_nzcv(n, z, c, v)
            }
            Op::LogicFlags(w, r) => {
                let r = self.v(r);
                let r = self.narrow(w, r);
                let (n, z) = self.nz(r);
                let n = self.bool64(n);
                let z = self.bool64(z);
                let n = self.b.ins().ishl_imm_u(n, 31);
                let z = self.b.ins().ishl_imm_u(z, 30);
                self.b.ins().bor(n, z)
            }
            Op::Cond(cc, nzcv) => {
                let nzcv = self.v(nzcv);
                self.cond(cc, nzcv)
            }
            Op::IsZero(w, v) => {
                let v = self.v(v);
                let v = self.narrow(w, v);
                let z = self.b.ins().icmp_imm_u(IntCC::Equal, v, 0);
                self.bool64(z)
            }
            Op::Select(c, a, bv) => {
                let (c, a, bv) = (self.v(c), self.v(a), self.v(bv));
                self.b.ins().select(c, a, bv)
            }
            Op::Load(size, addr, site) => {
                let addr = self.v(addr);
                let out = self.b.ins().stack_addr(types::I64, self.slot, 0);
                let status = self.mem_call(exec::mem_read_addr(), addr, size, out);
                self.fault_check(status, site);
                self.b.ins().load(types::I64, Self::flags(), out, 0)
            }
            Op::Store(size, addr, value, site) => {
                let (addr, value) = (self.v(addr), self.v(value));
                let status = self.mem_call(exec::mem_write_addr(), addr, size, value);
                self.fault_check(status, site);
                let written = self.b.ins().icmp_imm_u(IntCC::Equal, status, MEM_CODE_WRITTEN as i64);
                let written = self.bool64(written);
                let stop = self.b.use_var(self.stop);
                let stop = self.b.ins().bor(stop, written);
                self.b.def_var(self.stop, stop);
                return None;
            }
            Op::EndInsn { next_pc, insns } => {
                let stop = self.b.use_var(self.stop);
                let out = self.b.create_block();
                let cont = self.b.create_block();
                self.b.set_cold_block(out);
                self.b.ins().brif(stop, out, &[], cont, &[]);
                self.b.switch_to_block(out);
                let pc = self.b.ins().iconst(types::I64, next_pc as i64);
                self.b.ins().store(Self::flags(), pc, self.state, PC_OFFSET);
                self.ret(EXIT_CODE_WRITTEN, insns);
                self.b.switch_to_block(cont);
                return None;
            }
        })
    }
}
