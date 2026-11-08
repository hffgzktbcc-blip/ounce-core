//! The guest register file shared by the trampolines, [`crate::enter`] and the fault handler.
//! It lives in guest memory, in a page of the patch area, so generated stubs reach it with `ADRP`.
//! The offsets below are what the stubs and the entry code use; `#[repr(C)]` keeps them true.

/// Byte offsets into [`Context`].
pub mod off {
    pub const X: u32 = 0x000;
    pub const SP: u32 = 0x0F8;
    pub const PC: u32 = 0x100;
    pub const NZCV: u32 = 0x108;
    pub const FPCR: u32 = 0x110;
    pub const FPSR: u32 = 0x118;
    pub const TPIDR: u32 = 0x120;
    pub const TPIDRRO: u32 = 0x128;
    pub const EXIT_ID: u32 = 0x130;
    pub const HOST_SP: u32 = 0x138;
    pub const FAULT_SIGNAL: u32 = 0x140;
    pub const FAULT_PC: u32 = 0x148;
    pub const FAULT_ADDR: u32 = 0x150;
    pub const HOST_RET: u32 = 0x158;
    pub const V: u32 = 0x160;
    pub const SIZE: u32 = 0x360;
}

/// Guest state while guest code is not running. `x[18]` is the guest's `x18`: the host kernel may
/// change the hardware register at any time (macOS does), so rewritten code keeps it here instead.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Context {
    pub x: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    /// N/Z/C/V in bits 31..28.
    pub nzcv: u64,
    pub fpcr: u64,
    pub fpsr: u64,
    pub tpidr: u64,
    pub tpidrro: u64,
    /// Index of the trampoline that left guest code (see [`crate::patch::Site`]).
    pub exit_id: u64,
    pub host_sp: u64,
    /// Signal number of a guest fault, 0 if none.
    pub fault_signal: u64,
    pub fault_pc: u64,
    pub fault_addr: u64,
    pub host_ret: u64,
    pub v: [u128; 32],
}

const _: () = {
    assert!(std::mem::offset_of!(Context, sp) == off::SP as usize);
    assert!(std::mem::offset_of!(Context, pc) == off::PC as usize);
    assert!(std::mem::offset_of!(Context, nzcv) == off::NZCV as usize);
    assert!(std::mem::offset_of!(Context, fpcr) == off::FPCR as usize);
    assert!(std::mem::offset_of!(Context, fpsr) == off::FPSR as usize);
    assert!(std::mem::offset_of!(Context, tpidr) == off::TPIDR as usize);
    assert!(std::mem::offset_of!(Context, tpidrro) == off::TPIDRRO as usize);
    assert!(std::mem::offset_of!(Context, exit_id) == off::EXIT_ID as usize);
    assert!(std::mem::offset_of!(Context, host_sp) == off::HOST_SP as usize);
    assert!(std::mem::offset_of!(Context, fault_signal) == off::FAULT_SIGNAL as usize);
    assert!(std::mem::offset_of!(Context, fault_pc) == off::FAULT_PC as usize);
    assert!(std::mem::offset_of!(Context, fault_addr) == off::FAULT_ADDR as usize);
    assert!(std::mem::offset_of!(Context, host_ret) == off::HOST_RET as usize);
    assert!(std::mem::offset_of!(Context, v) == off::V as usize);
    assert!(std::mem::size_of::<Context>() == off::SIZE as usize);
};
