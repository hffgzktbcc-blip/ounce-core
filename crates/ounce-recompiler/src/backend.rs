//! IR -> host machine code with Cranelift (design section 4). Safe code only: the bytes go into
//! an `exec::BlockCode`, which owns the executable memory.
//!
//! Generated function: `fn(state: *mut GuestState, ctx: *mut CallCtx) -> u64` with Cranelift's
//! `tail` convention; Rust enters through one `extern "C"` trampoline per `Recompiler`
//! (`compile_trampoline`). A linked exit (`ExitTo::link`) tail-calls the next block when the link
//! table has it and the budget allows (docs/design/m4-native-linking.md); otherwise it returns.
//! Guest registers live in Cranelift variables within a block: the ones the block uses are loaded
//! from `state` on entry, and the ones it sets are written back only on the way out (every return
//! and every linked tail call), with the values the IR has reached at that exit. The memory
//! callbacks never read `state`, so faults stay precise (section 5). A linked exit back to the
//! block's own `entry_pc` loops inside the function and keeps the registers in host registers.
//! The return value is `code | insns << 32`; the exit `pc` is stored in `state.pc`.

use crate::exec::{self, BlockCode, CallCtx, LinkEntry, Trampoline, LINK_MASK, MEM_CODE_WRITTEN, MEM_FAULT};
use crate::ir::{BinOp, Block, ExitTo, FaultSite, FpBinOp, FpWidth, Op, Reg, Terminator, Value, Width};
use crate::{CompileError, GuestState, EXIT_CODE_WRITTEN, EXIT_FAULT};
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{self as clif, types, AbiParam, Endianness, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind};
use cranelift_codegen::isa::{CallConv, OwnedTargetIsa, TargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use std::collections::{BTreeMap, BTreeSet};
use std::mem::offset_of;

pub(crate) fn host_isa() -> Result<OwnedTargetIsa, CompileError> {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").map_err(|e| CompileError::Backend(e.to_string()))?;
    // Cranelift's x86_64 `return_call_indirect` needs frame pointers (it panics while emitting
    // otherwise); linked exits use it.
    flags.set("preserve_frame_pointers", "true").map_err(|e| CompileError::Backend(e.to_string()))?;
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
        Reg::VLo(n) => offset_of!(GuestState, v) + 16 * n as usize,
        Reg::VHi(n) => offset_of!(GuestState, v) + 16 * n as usize + 8,
    }) as i32
}

const PC_OFFSET: i32 = offset_of!(GuestState, pc) as i32;

/// The signature every block has: `(state, ctx) -> exit word`, `tail` convention.
fn block_sig() -> clif::Signature {
    let mut sig = clif::Signature::new(CallConv::Tail);
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn finish(isa: &dyn TargetIsa, ctx: &mut Context) -> Result<Vec<u8>, CompileError> {
    let compiled = ctx
        .compile(isa, &mut ControlPlane::default())
        .map_err(|e| CompileError::Backend(format!("{:?}", e.inner)))?;
    if !compiled.buffer.relocs().is_empty() {
        return Err(CompileError::Backend("generated code needs relocations".into()));
    }
    Ok(compiled.code_buffer().to_vec())
}

/// `extern "C" fn(entry, state, ctx) -> u64` that calls the block at `entry` and returns its word.
pub(crate) fn compile_trampoline(
    isa: &dyn TargetIsa,
    ctx: &mut Context,
    fctx: &mut FunctionBuilderContext,
) -> Result<Trampoline, CompileError> {
    let mut sig = clif::Signature::new(isa.default_call_conv());
    for _ in 0..3 {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    ctx.clear();
    ctx.func.signature = sig;
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, fctx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let p = b.block_params(entry).to_vec();
        let block_sig = b.import_signature(block_sig());
        let call = b.ins().call_indirect(block_sig, p[0], &[p[1], p[2]]);
        let word = b.inst_results(call)[0];
        b.ins().return_(&[word]);
        b.seal_all_blocks();
        b.finalize(isa.frontend_config());
    }
    Trampoline::new(&finish(isa, ctx)?).map_err(CompileError::Exec)
}

pub(crate) fn compile(
    isa: &dyn TargetIsa,
    ctx: &mut Context,
    fctx: &mut FunctionBuilderContext,
    blocks: &[Block],
) -> Result<BlockCode, CompileError> {
    // A region: the first block is the entry; the others are guest blocks its direct links may
    // reach, each with its `entry_pc`, compiled into the same function (loops across blocks stay
    // in host registers).
    if blocks.is_empty() {
        return Err(CompileError::InvalidIr("empty region".into()));
    }
    for block in blocks {
        block.validate().map_err(CompileError::InvalidIr)?;
        if blocks.len() > 1 && block.entry_pc.is_none() {
            return Err(CompileError::InvalidIr("every block of a region needs an entry pc".into()));
        }
    }
    let pcs: BTreeSet<u64> = blocks.iter().filter_map(|bl| bl.entry_pc).collect();
    if blocks.len() > 1 && pcs.len() != blocks.len() {
        return Err(CompileError::InvalidIr("two blocks of a region share an entry pc".into()));
    }

    // The memory trampolines are plain `extern "C"` functions.
    let call_conv = isa.default_call_conv();

    ctx.clear();
    ctx.func.signature = block_sig();
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
        let tail_sig = b.import_signature(block_sig());

        let slot = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let stop = b.declare_var(types::I64);
        let zero = b.ins().iconst(types::I64, 0);
        b.def_var(stop, zero);

        // One variable per guest register the region touches, loaded on entry.
        let mut regs = BTreeMap::new();
        let mut written = BTreeSet::new();
        for op in blocks.iter().flat_map(|bl| &bl.ops) {
            let (Op::GetReg(r) | Op::SetReg(r, _)) = *op else { continue };
            let off = reg_offset(r);
            if matches!(op, Op::SetReg(..)) {
                written.insert(off);
            }
            regs.entry(off).or_insert_with(|| {
                let var = b.declare_var(types::I64);
                let v = b.ins().load(types::I64, MemFlagsData::trusted(), state, off);
                b.def_var(var, v);
                var
            });
        }
        // Link accounting lives in variables while the function runs and goes back to `ctx` at
        // every exit, so loops inside the function touch no memory for it.
        let chained = b.declare_var(types::I64);
        let v = b.ins().load(types::I64, MemFlagsData::trusted(), call_ctx, offset_of!(CallCtx, chained_insns) as i32);
        b.def_var(chained, v);
        let links = b.declare_var(types::I64);
        let v = b.ins().load(types::I64, MemFlagsData::trusted(), call_ctx, offset_of!(CallCtx, links_taken) as i32);
        b.def_var(links, v);
        let budget = b.ins().load(types::I64, MemFlagsData::trusted(), call_ctx, offset_of!(CallCtx, budget) as i32);

        // One Cranelift block per guest block with a known pc: linked exits to it jump there.
        let mut heads = BTreeMap::new();
        let mut order = Vec::with_capacity(blocks.len());
        for bl in blocks {
            let head = bl.entry_pc.map(|pc| *heads.entry(pc).or_insert_with(|| b.create_block()));
            order.push(head);
        }
        if let Some(first) = order[0] {
            b.ins().jump(first, &[]);
        }

        let mut lw = Lowering {
            b,
            state,
            call_ctx,
            mem_sig,
            tail_sig,
            slot,
            stop,
            vals: Vec::new(),
            regs,
            written,
            heads,
            chained,
            links,
            budget,
        };
        for (block, head) in blocks.iter().zip(order) {
            if let Some(head) = head {
                lw.b.switch_to_block(head);
            }
            // How a linked exit finds its table slot: fixed for a constant pc, computed otherwise.
            let link_pc = |e: &ExitTo| match block.ops[e.pc.0 as usize] {
                _ if !e.link => None,
                Op::Const(pc) => Some(LinkTarget::Const(pc)),
                _ => Some(LinkTarget::Dynamic),
            };
            lw.vals.clear();
            for op in &block.ops {
                let v = lw.op(op);
                lw.vals.push(v);
            }
            match block.term {
                Terminator::Exit(e) => lw.exit(e, link_pc(&e)),
                Terminator::Branch { cond, taken, not_taken } => {
                    let c = lw.v(cond);
                    let t = lw.b.create_block();
                    let n = lw.b.create_block();
                    lw.b.ins().brif(c, t, &[], n, &[]);
                    lw.b.switch_to_block(t);
                    lw.exit(taken, link_pc(&taken));
                    lw.b.switch_to_block(n);
                    lw.exit(not_taken, link_pc(&not_taken));
                }
            }
        }
        lw.b.seal_all_blocks();
        lw.b.finalize(isa.frontend_config());
    }

    BlockCode::new(&finish(isa, ctx)?).map_err(CompileError::Exec)
}

/// Where a linked exit goes.
#[derive(Clone, Copy)]
enum LinkTarget {
    /// Direct branch: the slot is a compile-time constant.
    Const(u64),
    /// Indirect branch or return: the slot is computed from the exit pc at run time.
    Dynamic,
}

struct Lowering<'a> {
    b: FunctionBuilder<'a>,
    state: clif::Value,
    call_ctx: clif::Value,
    mem_sig: clif::SigRef,
    /// Block signature, for linked tail calls.
    tail_sig: clif::SigRef,
    slot: clif::StackSlot,
    stop: Variable,
    /// Cranelift value per IR op; `None` for ops without a result.
    vals: Vec<Option<clif::Value>>,
    /// Variable per guest register the block touches, by `state` offset.
    regs: BTreeMap<i32, Variable>,
    /// Offsets of the registers the block sets; written back at every exit.
    written: BTreeSet<i32>,
    /// Guest pc -> Cranelift block, for every block of the region with a known pc.
    heads: BTreeMap<u64, clif::Block>,
    /// `CallCtx::chained_insns` and `CallCtx::links_taken`, written back at exits.
    chained: Variable,
    links: Variable,
    /// `CallCtx::budget`, read once on entry.
    budget: clif::Value,
}

impl Lowering<'_> {
    fn v(&self, v: Value) -> clif::Value {
        self.vals[v.0 as usize].expect("validated IR")
    }

    fn flags() -> MemFlagsData {
        MemFlagsData::trusted()
    }

    fn bitcast_flags() -> MemFlagsData {
        MemFlagsData::new().with_endianness(Endianness::Little)
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

    /// Writes back every register the block sets (unchanged ones store their entry value).
    fn flush_regs(&mut self) {
        let ctx = self.call_ctx;
        let chained = self.b.use_var(self.chained);
        self.b.ins().store(Self::flags(), chained, ctx, offset_of!(CallCtx, chained_insns) as i32);
        let links = self.b.use_var(self.links);
        self.b.ins().store(Self::flags(), links, ctx, offset_of!(CallCtx, links_taken) as i32);
        for off in self.written.clone() {
            let v = self.b.use_var(self.regs[&off]);
            self.b.ins().store(Self::flags(), v, self.state, off);
        }
    }

    fn ret(&mut self, code: u32, insns: u32) {
        self.flush_regs();
        let word = self.b.ins().iconst(types::I64, (code as u64 | (insns as u64) << 32) as i64);
        self.b.ins().return_(&[word]);
    }

    fn exit(&mut self, e: ExitTo, link: Option<LinkTarget>) {
        let pc = self.v(e.pc);
        if let Some(target) = link {
            self.try_link(target, pc, e.insns);
        }
        self.b.ins().store(Self::flags(), pc, self.state, PC_OFFSET);
        self.ret(e.code, e.insns);
    }

    /// Jumps straight to the block linked for guest `pc` when the link table has one, no store in
    /// this block hit code, and `chained_insns + insns < budget`. Falls through to the normal
    /// return otherwise. For a direct branch the table slot is a compile-time constant; for an
    /// indirect branch or return it is `(pc >> 2) & LINK_MASK` at run time (an indirect-target
    /// cache shared with direct links). A misaligned `pc` never matches: blocks start aligned.
    fn try_link(&mut self, target: LinkTarget, pc: clif::Value, insns: u32) {
        let ctx = self.call_ctx;
        let ret = self.b.create_block();
        let probe = self.b.create_block();
        let go = self.b.create_block();

        let chained = self.b.use_var(self.chained);
        let chained = self.b.ins().iadd_imm_u(chained, insns as i64);
        let in_budget = self.b.ins().icmp(IntCC::UnsignedLessThan, chained, self.budget);
        let stop = self.b.use_var(self.stop);
        let clean = self.b.ins().icmp_imm_u(IntCC::Equal, stop, 0);
        let ok = self.b.ins().band(in_budget, clean);
        if let LinkTarget::Const(t) = target
            && let Some(&head) = self.heads.get(&t)
        {
            // A block of this region: jump without leaving the function or touching `state`.
            let go = self.b.create_block();
            self.b.ins().brif(ok, go, &[], ret, &[]);
            self.b.switch_to_block(go);
            self.count_link(chained);
            self.b.ins().jump(head, &[]);
            self.b.switch_to_block(ret);
            return;
        }
        self.b.ins().brif(ok, probe, &[], ret, &[]);

        self.b.switch_to_block(probe);
        if let LinkTarget::Dynamic = target {
            // A return or indirect branch to a block of this region (typically the return site
            // of a call made inside it) stays in the function: compare against each head first.
            for (hpc, head) in self.heads.clone() {
                let hit = self.b.ins().icmp_imm_u(IntCC::Equal, pc, hpc as i64);
                let take = self.b.create_block();
                let next = self.b.create_block();
                self.b.ins().brif(hit, take, &[], next, &[]);
                self.b.switch_to_block(take);
                self.count_link(chained);
                self.b.ins().jump(head, &[]);
                self.b.switch_to_block(next);
            }
        }
        let table = self.b.ins().load(types::I64, Self::flags(), ctx, offset_of!(CallCtx, links) as i32);
        let (table, slot) = match target {
            LinkTarget::Const(t) => (table, (((t >> 2) & LINK_MASK) as usize * size_of::<LinkEntry>()) as i32),
            LinkTarget::Dynamic => {
                // (pc >> 2) & MASK, times 16 bytes per entry.
                const _: () = assert!(size_of::<LinkEntry>() == 16);
                let idx = self.b.ins().ushr_imm_u(pc, 2);
                let idx = self.b.ins().band_imm_u(idx, LINK_MASK as i64);
                let off = self.b.ins().ishl_imm_u(idx, 4);
                (self.b.ins().iadd(table, off), 0)
            }
        };
        let entry_pc = self.b.ins().load(types::I64, Self::flags(), table, slot + offset_of!(LinkEntry, guest_pc) as i32);
        let hit = self.b.ins().icmp(IntCC::Equal, entry_pc, pc);
        self.b.ins().brif(hit, go, &[], ret, &[]);

        self.b.switch_to_block(go);
        self.count_link(chained);
        let entry =
            self.b.ins().load(types::I64, Self::flags(), table, slot + offset_of!(LinkEntry, host_entry) as i32);
        self.flush_regs();
        self.b.ins().return_call_indirect(self.tail_sig, entry, &[self.state, ctx]);

        self.b.switch_to_block(ret);
    }

    /// Records a link: `chained_insns = chained`, `links_taken += 1` (in variables).
    fn count_link(&mut self, chained: clif::Value) {
        self.b.def_var(self.chained, chained);
        let taken = self.b.use_var(self.links);
        let taken = self.b.ins().iadd_imm_u(taken, 1);
        self.b.def_var(self.links, taken);
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
            Op::GetReg(r) => self.b.use_var(self.regs[&reg_offset(r)]),
            Op::SetReg(r, v) => {
                let v = self.v(v);
                self.b.def_var(self.regs[&reg_offset(r)], v);
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
            Op::FpBin(op, w, a, b) => {
                let va = self.v(a);
                let vb = self.v(b);
                match w {
                    FpWidth::F32 => {
                        let a32 = self.b.ins().ireduce(types::I32, va);
                        let b32 = self.b.ins().ireduce(types::I32, vb);
                        let fa = self.b.ins().bitcast(types::F32, Self::bitcast_flags(), a32);
                        let fb = self.b.ins().bitcast(types::F32, Self::bitcast_flags(), b32);
                        let fres = match op {
                            FpBinOp::Fadd => self.b.ins().fadd(fa, fb),
                            FpBinOp::Fsub => self.b.ins().fsub(fa, fb),
                            FpBinOp::Fmul => self.b.ins().fmul(fa, fb),
                            FpBinOp::Fdiv => self.b.ins().fdiv(fa, fb),
                            FpBinOp::Fmax => self.b.ins().fmax(fa, fb),
                            FpBinOp::Fmin => self.b.ins().fmin(fa, fb),
                        };
                        let ires32 = self.b.ins().bitcast(types::I32, Self::bitcast_flags(), fres);
                        self.b.ins().uextend(types::I64, ires32)
                    }
                    FpWidth::F64 => {
                        let fa = self.b.ins().bitcast(types::F64, Self::bitcast_flags(), va);
                        let fb = self.b.ins().bitcast(types::F64, Self::bitcast_flags(), vb);
                        let fres = match op {
                            FpBinOp::Fadd => self.b.ins().fadd(fa, fb),
                            FpBinOp::Fsub => self.b.ins().fsub(fa, fb),
                            FpBinOp::Fmul => self.b.ins().fmul(fa, fb),
                            FpBinOp::Fdiv => self.b.ins().fdiv(fa, fb),
                            FpBinOp::Fmax => self.b.ins().fmax(fa, fb),
                            FpBinOp::Fmin => self.b.ins().fmin(fa, fb),
                        };
                        self.b.ins().bitcast(types::I64, Self::bitcast_flags(), fres)
                    }
                }
            }
            Op::FpCompare(w, a, b) => {
                let va = self.v(a);
                let vb = self.v(b);
                let (is_un, is_eq, is_lt) = match w {
                    FpWidth::F32 => {
                        let a32 = self.b.ins().ireduce(types::I32, va);
                        let b32 = self.b.ins().ireduce(types::I32, vb);
                        let fa = self.b.ins().bitcast(types::F32, Self::bitcast_flags(), a32);
                        let fb = self.b.ins().bitcast(types::F32, Self::bitcast_flags(), b32);
                        (
                            self.b.ins().fcmp(FloatCC::Unordered, fa, fb),
                            self.b.ins().fcmp(FloatCC::Equal, fa, fb),
                            self.b.ins().fcmp(FloatCC::LessThan, fa, fb),
                        )
                    }
                    FpWidth::F64 => {
                        let fa = self.b.ins().bitcast(types::F64, Self::bitcast_flags(), va);
                        let fb = self.b.ins().bitcast(types::F64, Self::bitcast_flags(), vb);
                        (
                            self.b.ins().fcmp(FloatCC::Unordered, fa, fb),
                            self.b.ins().fcmp(FloatCC::Equal, fa, fb),
                            self.b.ins().fcmp(FloatCC::LessThan, fa, fb),
                        )
                    }
                };
                let c_gt = self.b.ins().iconst(types::I64, 0x2000_0000);
                let c_lt = self.b.ins().iconst(types::I64, 0x8000_0000);
                let c_eq = self.b.ins().iconst(types::I64, 0x6000_0000);
                let c_un = self.b.ins().iconst(types::I64, 0x3000_0000);

                let r1 = self.b.ins().select(is_lt, c_lt, c_gt);
                let r2 = self.b.ins().select(is_eq, c_eq, r1);
                self.b.ins().select(is_un, c_un, r2)
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
