//! Tier-4 native recompiler for ounce-core (M4, `docs/design/m4-native-jit.md`).
//!
//! Step 1: executable-memory handling and a per-host self-test. Step 2: a block IR ([`ir`]), a
//! Cranelift backend, and [`Recompiler`], which compiles IR blocks and runs them against a
//! [`MemoryBus`] (memory plan A, callbacks). `unsafe` is denied crate-wide and allowed only in
//! `exec`.

#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod backend;
#[allow(unsafe_code)]
mod exec;
pub mod ir;

pub use exec::{ExecError, TlbEntry, TLB_MASK, TLB_SIZE};
use exec::run_return_constant;

/// Direct host memory mapping info for a 4096-byte guest page (VA >> 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageMapping {
    pub page_num: u64,
    /// Host base pointer to the page bytes.
    pub host_base: u64,
    pub read: bool,
    pub write: bool,
    pub holds_code: bool,
}

/// Guest integer state as generated code sees it. `#[repr(C)]`: the backend addresses fields by
/// offset.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GuestState {
    pub x: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    /// N/Z/C/V in bits 31..28.
    pub nzcv: u64,
}

/// A guest memory access that failed (unmapped or not permitted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemFault;

/// What a successful write did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteEffect {
    Done,
    /// The write hit guest code that a translation was made from. The block stops at the end of
    /// the current guest instruction so the caller can invalidate before running more.
    CodeWritten,
}

/// Guest memory as seen by generated code. `size` is 1, 2, 4 or 8; reads zero-extend.
pub trait MemoryBus {
    fn read(&mut self, va: u64, size: u8) -> Result<u64, MemFault>;
    fn write(&mut self, va: u64, size: u8, value: u64) -> Result<WriteEffect, MemFault>;
    /// Optional: provides direct host memory mapping for a guest page (VA >> 12) for software TLB acceleration.
    fn query_page(&mut self, _page_num: u64) -> Option<PageMapping> {
        None
    }
}

pub(crate) const EXIT_FAULT: u32 = u32::MAX;
pub(crate) const EXIT_CODE_WRITTEN: u32 = u32::MAX - 1;

/// Why a block stopped. `pc` in the [`GuestState`] is where the guest continues (for a fault: the
/// faulting instruction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitKind {
    /// A terminator of the block, with the caller's exit code.
    Code(u32),
    /// A memory access faulted; guest state is as before the faulting instruction (stores made
    /// earlier in that instruction, e.g. the first half of a pair, stay done).
    Fault,
    /// A store hit translated code; see [`WriteEffect::CodeWritten`].
    CodeWritten,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    pub kind: ExitKind,
    /// Guest instructions completed.
    pub insns: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    UnsupportedHost,
    InvalidIr(String),
    Backend(String),
    Exec(ExecError),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::UnsupportedHost => write!(f, "no native code generator for this host"),
            CompileError::InvalidIr(e) => write!(f, "invalid IR: {e}"),
            CompileError::Backend(e) => write!(f, "code generation failed: {e}"),
            CompileError::Exec(e) => write!(f, "executable memory: {e}"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Handle to a compiled block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockId(u32);

/// Compiles IR blocks to host code and runs them.
pub struct Recompiler {
    isa: cranelift_codegen::isa::OwnedTargetIsa,
    ctx: cranelift_codegen::Context,
    fctx: cranelift_frontend::FunctionBuilderContext,
    blocks: Vec<Option<exec::BlockCode>>,
    free: Vec<u32>,
    tlb: Box<[exec::TlbEntry; exec::TLB_SIZE]>,
}

impl Recompiler {
    pub fn new() -> Result<Self, CompileError> {
        if host_arch().is_none() {
            return Err(CompileError::UnsupportedHost);
        }
        Ok(Self {
            isa: backend::host_isa()?,
            ctx: cranelift_codegen::Context::new(),
            fctx: cranelift_frontend::FunctionBuilderContext::new(),
            blocks: Vec::new(),
            free: Vec::new(),
            tlb: Box::new([exec::TlbEntry::default(); exec::TLB_SIZE]),
        })
    }

    /// Flushes all entries in the software TLB (e.g. after code invalidation or memory remapping).
    pub fn flush_tlb(&mut self) {
        self.tlb.fill(exec::TlbEntry::default());
    }

    pub fn compile(&mut self, block: &ir::Block) -> Result<BlockId, CompileError> {
        let code = backend::compile(&*self.isa, &mut self.ctx, &mut self.fctx, block)?;
        Ok(match self.free.pop() {
            Some(i) => {
                self.blocks[i as usize] = Some(code);
                BlockId(i)
            }
            None => {
                self.blocks.push(Some(code));
                BlockId((self.blocks.len() - 1) as u32)
            }
        })
    }

    /// Runs a compiled block. Panics if `id` was removed. A panic inside `bus` propagates after
    /// the block has returned.
    pub fn run(&mut self, id: BlockId, state: &mut GuestState, bus: &mut dyn MemoryBus) -> Exit {
        let code = self.blocks[id.0 as usize].as_ref().expect("block was removed");
        let word = code.run(state, bus, &mut self.tlb);
        let insns = (word >> 32) as u32;
        let kind = match word as u32 {
            EXIT_FAULT => ExitKind::Fault,
            EXIT_CODE_WRITTEN => ExitKind::CodeWritten,
            c => ExitKind::Code(c),
        };
        Exit { kind, insns }
    }

    /// Frees a block's code (after guest code it came from changed).
    pub fn remove(&mut self, id: BlockId) {
        if let Some(slot) = self.blocks.get_mut(id.0 as usize)
            && slot.take().is_some()
        {
            self.free.push(id.0);
        }
    }

    /// Number of live compiled blocks.
    pub fn len(&self) -> usize {
        self.blocks.len() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The host architecture this build generates code for, or `None` if the recompiler cannot run here.
pub fn host_arch() -> Option<&'static str> {
    if cfg!(target_arch = "aarch64") {
        Some("aarch64")
    } else if cfg!(target_arch = "x86_64") {
        Some("x86_64")
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfTestError {
    /// No code generator for this host architecture.
    UnsupportedHost,
    /// Executable memory could not be set up.
    Exec(ExecError),
    /// The generated function ran but returned the wrong value.
    WrongResult { expected: u64, got: u64 },
}

impl std::fmt::Display for SelfTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelfTestError::UnsupportedHost => write!(f, "no native code generator for this host"),
            SelfTestError::Exec(e) => write!(f, "executable memory: {e}"),
            SelfTestError::WrongResult { expected, got } => write!(f, "returned {got}, expected {expected}"),
        }
    }
}

impl std::error::Error for SelfTestError {}

/// Generates, seals and runs a tiny native function that returns 42. Proves that this host can
/// allocate W^X memory, synchronise the instruction cache and call generated code.
pub fn host_selftest() -> Result<u64, SelfTestError> {
    run_return_constant(42, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use exec::ExecBuffer;

    #[test]
    fn selftest_returns_42() {
        assert_eq!(host_selftest(), Ok(42));
    }

    #[test]
    fn independent_buffers_return_their_own_constants() {
        assert_eq!(run_return_constant(0x1234, 0), Ok(0x1234));
        assert_eq!(run_return_constant(7, 0), Ok(7));
    }

    #[test]
    fn code_spanning_several_pages_runs() {
        // 64 KiB of no-ops before the return: crosses 4 KiB and 16 KiB page boundaries.
        assert_eq!(run_return_constant(99, 16 * 1024), Ok(99));
    }

    #[test]
    fn empty_code_is_rejected() {
        assert!(matches!(ExecBuffer::seal(&[]), Err(ExecError::Empty)));
    }

    use ir::{BinOp, BlockBuilder, ExitTo, FaultSite, Op, Reg, Terminator, Width};
    use std::collections::HashMap;

    /// Byte-addressed test memory; one 4 KiB page at 0x1000, page 0x2000 holds "code".
    #[derive(Default)]
    struct TestBus {
        bytes: HashMap<u64, u8>,
    }

    impl MemoryBus for TestBus {
        fn read(&mut self, va: u64, size: u8) -> Result<u64, MemFault> {
            let mut v = 0u64;
            for i in 0..size as u64 {
                let a = va.wrapping_add(i);
                if !(0x1000..0x3000).contains(&a) {
                    return Err(MemFault);
                }
                v |= (*self.bytes.get(&a).unwrap_or(&0) as u64) << (8 * i);
            }
            Ok(v)
        }

        fn write(&mut self, va: u64, size: u8, value: u64) -> Result<WriteEffect, MemFault> {
            if va == 0xDEAD {
                panic!("bus panic");
            }
            for i in 0..size as u64 {
                if !(0x1000..0x3000).contains(&va.wrapping_add(i)) {
                    return Err(MemFault);
                }
            }
            for i in 0..size as u64 {
                self.bytes.insert(va + i, (value >> (8 * i)) as u8);
            }
            Ok(if va >= 0x2000 { WriteEffect::CodeWritten } else { WriteEffect::Done })
        }
    }

    fn exit_to(b: &mut BlockBuilder, code: u32, pc: u64, insns: u32) -> ExitTo {
        let pc = b.konst(pc);
        ExitTo { code, pc, insns }
    }

    fn run(block: ir::Block, state: &mut GuestState, bus: &mut TestBus) -> Exit {
        let mut rec = Recompiler::new().unwrap();
        let id = rec.compile(&block).unwrap();
        rec.run(id, state, bus)
    }

    #[test]
    fn alu_flags_and_conditional_exit() {
        let mut b = BlockBuilder::new();
        let x0 = b.get(Reg::X(0));
        let x1 = b.get(Reg::X(1));
        let sum = b.bin(BinOp::Add, Width::W32, x0, x1);
        b.set(Reg::X(2), sum);
        let f = b.push(Op::AddFlags(Width::W32, x0, x1));
        b.set(Reg::Nzcv, f);
        let cs = b.push(Op::Cond(0x2, f)); // CS
        let taken = exit_to(&mut b, 1, 0x100, 3);
        let not_taken = exit_to(&mut b, 2, 0x200, 3);
        let block = b.finish(Terminator::Branch { cond: cs, taken, not_taken });

        let mut st = GuestState::default();
        st.x[0] = 0xFFFF_FFFF;
        st.x[1] = 1;
        let exit = run(block, &mut st, &mut TestBus::default());
        assert_eq!(st.x[2], 0);
        assert_eq!(st.nzcv, 0b0110 << 28, "Z and C set");
        assert_eq!(exit, Exit { kind: ExitKind::Code(1), insns: 3 });
        assert_eq!(st.pc, 0x100);
    }

    #[test]
    fn division_never_traps() {
        let mut b = BlockBuilder::new();
        let x0 = b.get(Reg::X(0));
        let x1 = b.get(Reg::X(1));
        let x2 = b.get(Reg::X(2));
        let q0 = b.bin(BinOp::Udiv, Width::W64, x0, x1); // x1 = 0
        b.set(Reg::X(3), q0);
        let q1 = b.bin(BinOp::Sdiv, Width::W64, x0, x2); // MIN / -1
        b.set(Reg::X(4), q1);
        let q2 = b.bin(BinOp::Sdiv, Width::W32, x0, x2);
        b.set(Reg::X(5), q2);
        let e = exit_to(&mut b, 0, 0, 1);
        let mut st = GuestState::default();
        st.x[0] = i64::MIN as u64;
        st.x[2] = u64::MAX;
        st.x[3] = 7;
        run(b.finish(Terminator::Exit(e)), &mut st, &mut TestBus::default());
        assert_eq!(st.x[3], 0);
        assert_eq!(st.x[4], i64::MIN as u64);
        assert_eq!(st.x[5], 0, "low word of MIN is 0, 0 / -1 = 0");
    }

    #[test]
    fn load_store_and_precise_fault() {
        let mut b = BlockBuilder::new();
        let addr = b.konst(0x1008);
        let val = b.konst(0x1122_3344_5566_7788);
        b.push(Op::Store(8, addr, val, FaultSite { pc: 0x40, insns: 0 }));
        let ld = b.push(Op::Load(2, addr, FaultSite { pc: 0x44, insns: 1 }));
        b.set(Reg::X(0), ld);
        let bad = b.konst(0x0FFF);
        let ld2 = b.push(Op::Load(8, bad, FaultSite { pc: 0x48, insns: 2 }));
        b.set(Reg::X(1), ld2);
        let e = exit_to(&mut b, 0, 0x4C, 3);
        let mut st = GuestState::default();
        st.x[1] = 5;
        let mut bus = TestBus::default();
        let exit = run(b.finish(Terminator::Exit(e)), &mut st, &mut bus);
        assert_eq!(exit, Exit { kind: ExitKind::Fault, insns: 2 });
        assert_eq!(st.pc, 0x48);
        assert_eq!(st.x[0], 0x7788);
        assert_eq!(st.x[1], 5, "faulting load must not write its register");
        assert_eq!(bus.read(0x1008, 8), Ok(0x1122_3344_5566_7788));
    }

    #[test]
    fn store_to_code_stops_at_instruction_end() {
        let mut b = BlockBuilder::new();
        let addr = b.konst(0x2000);
        let one = b.konst(1);
        b.push(Op::Store(4, addr, one, FaultSite { pc: 0x10, insns: 0 }));
        b.set(Reg::X(9), one); // writeback-like update in the same instruction
        b.push(Op::EndInsn { next_pc: 0x14, insns: 1 });
        b.set(Reg::X(0), one);
        let e = exit_to(&mut b, 0, 0x18, 2);
        let mut st = GuestState::default();
        let exit = run(b.finish(Terminator::Exit(e)), &mut st, &mut TestBus::default());
        assert_eq!(exit, Exit { kind: ExitKind::CodeWritten, insns: 1 });
        assert_eq!((st.pc, st.x[9], st.x[0]), (0x14, 1, 0));
    }

    #[test]
    fn bus_panic_propagates_after_block_returns() {
        let mut b = BlockBuilder::new();
        let addr = b.konst(0xDEAD);
        b.push(Op::Store(1, addr, addr, FaultSite { pc: 0, insns: 0 }));
        let e = exit_to(&mut b, 0, 0, 1);
        let block = b.finish(Terminator::Exit(e));
        let r = std::panic::catch_unwind(|| run(block, &mut GuestState::default(), &mut TestBus::default()));
        assert!(r.is_err());
    }

    #[test]
    fn invalid_ir_is_rejected() {
        let mut b = BlockBuilder::new();
        let v = b.konst(1);
        let e = ExitTo { code: ir::RESERVED_CODES, pc: v, insns: 0 };
        let mut rec = Recompiler::new().unwrap();
        assert!(matches!(rec.compile(&b.finish(Terminator::Exit(e))), Err(CompileError::InvalidIr(_))));
    }

    #[test]
    fn removed_slots_are_reused() {
        let mut rec = Recompiler::new().unwrap();
        let mk = || {
            let mut b = BlockBuilder::new();
            let e = exit_to(&mut b, 0, 0, 0);
            b.finish(Terminator::Exit(e))
        };
        let a = rec.compile(&mk()).unwrap();
        rec.remove(a);
        assert!(rec.is_empty());
        assert_eq!(rec.compile(&mk()).unwrap(), a);
    }

    #[test]
    fn host_is_supported_on_ci_targets() {
        assert!(host_arch().is_some());
    }
}
