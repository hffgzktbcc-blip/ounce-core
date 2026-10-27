//! Executable memory for generated host code. This is the only module in the workspace that may
//! contain `unsafe` (see AGENTS.md, "Unsafe Boundary", and docs/design/m4-native-jit.md section 6).
//!
//! W^X: a buffer is writable while code is copied in, then sealed read+execute and never made
//! writable again. Per host:
//! - macOS on Apple Silicon: `MAP_JIT` mapping; writes are enabled for the current thread only with
//!   `pthread_jit_write_protect_np(0)` and disabled again before the code runs. The kernel requires
//!   `MAP_JIT` regions to be mapped RWX; the per-thread toggle is what enforces W^X.
//! - Other Unix: `mmap` read+write, then `mprotect` read+execute.
//! - Windows: `VirtualAlloc` read+write, then `VirtualProtect` execute+read.
//!
//! The instruction cache is synchronised after every seal on hosts that need it.

use std::fmt;

use crate::{GuestState, MemFault, MemoryBus, WriteEffect};

pub const TLB_SIZE: usize = 512;
pub const TLB_MASK: u64 = (TLB_SIZE as u64) - 1;

/// A single direct-mapped Software TLB entry (Option B).
/// `#[repr(C)]` so Cranelift lowering can inspect fields by fixed byte offset.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlbEntry {
    /// Guest page number (VA >> 12). If invalid or unmapped, `u64::MAX`.
    pub page_tag: u64,
    /// Host pointer to start of page data (4096 bytes).
    pub host_base: u64,
    /// Bit 0: read permitted. Bit 1: write permitted (cleared if holds_code).
    pub flags: u32,
    pub _pad: u32,
}

impl Default for TlbEntry {
    fn default() -> Self {
        Self {
            page_tag: u64::MAX,
            host_base: 0,
            flags: 0,
            _pad: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecError {
    /// The code to seal was empty.
    Empty,
    /// An OS call failed. Holds the call name and the OS error code.
    Os(&'static str, i32),
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecError::Empty => write!(f, "no code to seal"),
            ExecError::Os(call, code) => write!(f, "{call} failed (OS error {code})"),
        }
    }
}

impl std::error::Error for ExecError {}

fn last_os_error(call: &'static str) -> ExecError {
    ExecError::Os(call, std::io::Error::last_os_error().raw_os_error().unwrap_or(0))
}

/// A sealed, read+execute copy of some host machine code.
pub struct ExecBuffer {
    ptr: *mut u8,
    /// Mapped size, a whole number of pages.
    mapped: usize,
}

impl ExecBuffer {
    /// Maps fresh memory, copies `code` into it and seals it read+execute.
    pub fn seal(code: &[u8]) -> Result<Self, ExecError> {
        if code.is_empty() {
            return Err(ExecError::Empty);
        }
        let page = os::page_size();
        let mapped = code.len().div_ceil(page) * page;
        let ptr = os::map(mapped)?;
        let buffer = ExecBuffer { ptr, mapped };
        os::write_and_seal(buffer.ptr, buffer.mapped, code)?;
        Ok(buffer)
    }

    /// Calls the code at the start of the buffer as `extern "C" fn() -> u64`.
    ///
    /// # Safety
    /// The buffer must hold a complete function for the host architecture that follows the host C
    /// calling convention, takes no arguments, returns a 64-bit integer, and touches no memory or
    /// registers it does not own under that convention.
    pub unsafe fn call_u64(&self) -> u64 {
        // SAFETY: `ptr` is the start of a live read+execute mapping that `seal` filled and whose
        // instruction cache was synchronised; the caller guarantees it holds a valid function of
        // this signature.
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(self.ptr) };
        f()
    }
}

impl Drop for ExecBuffer {
    fn drop(&mut self) {
        os::unmap(self.ptr, self.mapped);
    }
}

// --- Translated guest blocks (step 2). ---

/// What generated block code gets as its second argument: the caller's memory bus, plus room for
/// a panic raised inside it, which is carried across the generated frames and resumed afterwards.
pub(crate) struct CallCtx<'a> {
    pub(crate) bus: &'a mut dyn MemoryBus,
    pub(crate) panic: Option<Box<dyn std::any::Any + Send>>,
    pub(crate) tlb: &'a mut [TlbEntry; TLB_SIZE],
}

/// Status codes the memory trampolines return to generated code.
pub(crate) const MEM_OK: u32 = 0;
pub(crate) const MEM_FAULT: u32 = 1;
pub(crate) const MEM_CODE_WRITTEN: u32 = 2;

/// `extern "C" fn(ctx, va, size, out) -> status`: reads through the bus into `*out`.
extern "C" fn mem_read(ctx: *mut CallCtx<'_>, va: u64, size: u32, out: *mut u64) -> u32 {
    // SAFETY: generated code passes the `CallCtx` pointer that `run_block` handed it (live and
    // exclusively borrowed for the call) and a pointer to its own 8-byte, 8-aligned stack slot.
    let (ctx, out) = unsafe { (&mut *ctx, &mut *out) };
    if ctx.panic.is_some() {
        return MEM_FAULT;
    }
    let page_num = va >> 12;
    let idx = (page_num & TLB_MASK) as usize;
    let offset = (va & 0xFFF) as usize;

    let entry = ctx.tlb[idx];
    if entry.page_tag == page_num && (entry.flags & 1) != 0 && offset + size as usize <= 4096 {
        unsafe {
            let host_ptr = (entry.host_base + offset as u64) as *const u8;
            let val = match size {
                1 => *host_ptr as u64,
                2 => std::ptr::read_unaligned(host_ptr as *const u16) as u64,
                4 => std::ptr::read_unaligned(host_ptr as *const u32) as u64,
                8 => std::ptr::read_unaligned(host_ptr as *const u64),
                _ => return MEM_FAULT,
            };
            *out = val;
        }
        return MEM_OK;
    }

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ctx.bus.read(va, size as u8))) {
        Ok(Ok(value)) => {
            *out = value;
            if let Some(mapping) = ctx.bus.query_page(page_num) {
                let flags = (if mapping.read { 1 } else { 0 })
                    | (if mapping.write && !mapping.holds_code { 2 } else { 0 });
                ctx.tlb[idx] = TlbEntry {
                    page_tag: page_num,
                    host_base: mapping.host_base,
                    flags,
                    _pad: 0,
                };
            }
            MEM_OK
        }
        Ok(Err(MemFault)) => MEM_FAULT,
        Err(payload) => {
            ctx.panic = Some(payload);
            MEM_FAULT
        }
    }
}

/// `extern "C" fn(ctx, va, size, value) -> status`: writes through the bus.
extern "C" fn mem_write(ctx: *mut CallCtx<'_>, va: u64, size: u32, value: u64) -> u32 {
    // SAFETY: as in `mem_read`.
    let ctx = unsafe { &mut *ctx };
    if ctx.panic.is_some() {
        return MEM_FAULT;
    }
    let page_num = va >> 12;
    let idx = (page_num & TLB_MASK) as usize;
    let offset = (va & 0xFFF) as usize;

    let entry = ctx.tlb[idx];
    if entry.page_tag == page_num && (entry.flags & 2) != 0 && offset + size as usize <= 4096 {
        unsafe {
            let host_ptr = (entry.host_base + offset as u64) as *mut u8;
            match size {
                1 => *host_ptr = value as u8,
                2 => std::ptr::write_unaligned(host_ptr as *mut u16, value as u16),
                4 => std::ptr::write_unaligned(host_ptr as *mut u32, value as u32),
                8 => std::ptr::write_unaligned(host_ptr as *mut u64, value),
                _ => return MEM_FAULT,
            }
        }
        return MEM_OK;
    }

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ctx.bus.write(va, size as u8, value))) {
        Ok(Ok(WriteEffect::Done)) => {
            if let Some(mapping) = ctx.bus.query_page(page_num) {
                let flags = (if mapping.read { 1 } else { 0 })
                    | (if mapping.write && !mapping.holds_code { 2 } else { 0 });
                ctx.tlb[idx] = TlbEntry {
                    page_tag: page_num,
                    host_base: mapping.host_base,
                    flags,
                    _pad: 0,
                };
            }
            MEM_OK
        }
        Ok(Ok(WriteEffect::CodeWritten)) => MEM_CODE_WRITTEN,
        Ok(Err(MemFault)) => MEM_FAULT,
        Err(payload) => {
            ctx.panic = Some(payload);
            MEM_FAULT
        }
    }
}

/// Addresses of the memory trampolines, embedded as constants in generated code.
pub(crate) fn mem_read_addr() -> u64 {
    mem_read as extern "C" fn(*mut CallCtx<'_>, u64, u32, *mut u64) -> u32 as usize as u64
}

pub(crate) fn mem_write_addr() -> u64 {
    mem_write as extern "C" fn(*mut CallCtx<'_>, u64, u32, u64) -> u32 as usize as u64
}

/// Sealed host code for one guest block. Only `backend::compile` creates these.
pub(crate) struct BlockCode {
    buffer: ExecBuffer,
}

impl BlockCode {
    /// Wraps code produced by the backend.
    ///
    /// The code must be an `extern "C" fn(*mut GuestState, *mut CallCtx) -> u64` that only reads
    /// and writes the `GuestState` fields, calls `mem_read`/`mem_write` with its context pointer,
    /// and returns. `backend::compile` is the only caller and emits exactly that.
    pub(crate) fn new(code: &[u8]) -> Result<Self, ExecError> {
        Ok(BlockCode { buffer: ExecBuffer::seal(code)? })
    }

    /// Runs the block on `state` with `bus` and returns the packed exit word.
    pub(crate) fn run(&self, state: &mut GuestState, bus: &mut dyn MemoryBus, tlb: &mut [TlbEntry; TLB_SIZE]) -> u64 {
        let mut ctx = CallCtx { bus, panic: None, tlb };
        // SAFETY: `BlockCode::new` documents the function this buffer holds. Both pointers come
        // from live exclusive borrows that outlive the call; the trampolines never unwind.
        let exit = unsafe {
            let f: extern "C" fn(*mut GuestState, *mut CallCtx<'_>) -> u64 = std::mem::transmute(self.buffer.ptr);
            f(state, &mut ctx)
        };
        if let Some(payload) = ctx.panic {
            std::panic::resume_unwind(payload);
        }
        exit
    }
}

// --- Self-test code: the only generated code the crate runs in step 1. ---

/// Host machine code for `fn() -> u64 { value }`, followed by `pad_nops` no-op instructions placed
/// before it. `value` must fit in 16 bits (aarch64 `MOVZ`) / 32 bits (x86_64 `MOV r32`).
pub(crate) fn return_constant_code(value: u16, pad_nops: usize) -> Option<Vec<u8>> {
    let mut code = Vec::new();
    if cfg!(target_arch = "aarch64") {
        for _ in 0..pad_nops {
            code.extend_from_slice(&0xD503_201Fu32.to_le_bytes()); // nop
        }
        code.extend_from_slice(&(0xD280_0000u32 | (value as u32) << 5).to_le_bytes()); // movz x0, #value
        code.extend_from_slice(&0xD65F_03C0u32.to_le_bytes()); // ret
    } else if cfg!(target_arch = "x86_64") {
        code.extend(std::iter::repeat_n(0x90u8, pad_nops)); // nop
        code.push(0xB8); // mov eax, imm32 (zero-extends into rax)
        code.extend_from_slice(&(value as u32).to_le_bytes());
        code.push(0xC3); // ret
    } else {
        return None;
    }
    Some(code)
}

pub(crate) fn run_return_constant(value: u16, pad_nops: usize) -> Result<u64, crate::SelfTestError> {
    let code = return_constant_code(value, pad_nops).ok_or(crate::SelfTestError::UnsupportedHost)?;
    let buffer = ExecBuffer::seal(&code).map_err(crate::SelfTestError::Exec)?;
    // SAFETY: `code` is a complete `extern "C" fn() -> u64` for this host built just above: optional
    // NOPs, a move of a constant into the return register, and a return. It touches nothing else.
    let got = unsafe { buffer.call_u64() };
    if got == value as u64 { Ok(got) } else { Err(crate::SelfTestError::WrongResult { expected: value as u64, got }) }
}

#[cfg(unix)]
mod os {
    use super::{last_os_error, ExecError};

    pub fn page_size() -> usize {
        // SAFETY: sysconf has no preconditions.
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if size > 0 { size as usize } else { 4096 }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub fn map(len: usize) -> Result<*mut u8, ExecError> {
        // SAFETY: anonymous private mapping at a kernel-chosen address; no existing memory is touched.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_JIT,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED { Err(last_os_error("mmap(MAP_JIT)")) } else { Ok(ptr as *mut u8) }
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    pub fn map(len: usize) -> Result<*mut u8, ExecError> {
        // SAFETY: anonymous private mapping at a kernel-chosen address; no existing memory is touched.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED { Err(last_os_error("mmap")) } else { Ok(ptr as *mut u8) }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub fn write_and_seal(ptr: *mut u8, _mapped: usize, code: &[u8]) -> Result<(), ExecError> {
        unsafe extern "C" {
            fn sys_icache_invalidate(start: *mut libc::c_void, len: usize);
        }
        // SAFETY: `ptr` is a fresh MAP_JIT mapping of at least `code.len()` bytes owned by the
        // caller. Writes are enabled for this thread only, for the duration of the copy, and the
        // instruction cache is invalidated for exactly the written range before any execution.
        unsafe {
            libc::pthread_jit_write_protect_np(0);
            std::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
            libc::pthread_jit_write_protect_np(1);
            sys_icache_invalidate(ptr as *mut libc::c_void, code.len());
        }
        Ok(())
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    pub fn write_and_seal(ptr: *mut u8, mapped: usize, code: &[u8]) -> Result<(), ExecError> {
        // SAFETY: `ptr` is a fresh read+write mapping of `mapped >= code.len()` bytes owned by the
        // caller; after the copy it is switched to read+execute, so it is never writable and
        // executable at once.
        unsafe {
            std::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
            if libc::mprotect(ptr as *mut libc::c_void, mapped, libc::PROT_READ | libc::PROT_EXEC) != 0 {
                return Err(last_os_error("mprotect"));
            }
        }
        flush_icache(ptr, code.len());
        Ok(())
    }

    #[cfg(all(not(target_os = "macos"), target_arch = "aarch64"))]
    fn flush_icache(start: *mut u8, len: usize) {
        use std::arch::asm;
        let start = start as usize;
        let end = start + len;
        let ctr: u64;
        // SAFETY: CTR_EL0 is readable at EL0 on Linux and the BSDs; the maintenance instructions
        // only act on addresses inside the mapping we just wrote (Arm ARM, "Concurrent modification
        // and execution of instructions").
        unsafe {
            asm!("mrs {}, ctr_el0", out(reg) ctr, options(nomem, nostack));
            let dline = 4usize << ((ctr >> 16) & 0xF);
            let iline = 4usize << (ctr & 0xF);
            let mut addr = start & !(dline - 1);
            while addr < end {
                asm!("dc cvau, {}", in(reg) addr, options(nostack));
                addr += dline;
            }
            asm!("dsb ish", options(nostack));
            let mut addr = start & !(iline - 1);
            while addr < end {
                asm!("ic ivau, {}", in(reg) addr, options(nostack));
                addr += iline;
            }
            asm!("dsb ish", "isb", options(nostack));
        }
    }

    #[cfg(not(target_arch = "aarch64"))]
    fn flush_icache(_start: *mut u8, _len: usize) {
        // x86 keeps instruction fetch coherent with data writes.
    }

    pub fn unmap(ptr: *mut u8, len: usize) {
        // SAFETY: `ptr`/`len` describe a mapping created by `map` and not yet unmapped; the
        // ExecBuffer that owned it is being dropped, so no code in it can still be running.
        unsafe {
            libc::munmap(ptr as *mut libc::c_void, len);
        }
    }
}

#[cfg(windows)]
mod os {
    use super::{last_os_error, ExecError};
    use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
    use windows_sys::Win32::System::Memory::{
        VirtualAlloc, VirtualFree, VirtualProtect, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READ,
        PAGE_READWRITE,
    };
    use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    pub fn page_size() -> usize {
        // SAFETY: GetSystemInfo only writes the struct we pass it.
        let info = unsafe {
            let mut info: SYSTEM_INFO = std::mem::zeroed();
            GetSystemInfo(&mut info);
            info
        };
        if info.dwPageSize > 0 { info.dwPageSize as usize } else { 4096 }
    }

    pub fn map(len: usize) -> Result<*mut u8, ExecError> {
        // SAFETY: reserves and commits fresh pages at a system-chosen address.
        let ptr = unsafe { VirtualAlloc(std::ptr::null(), len, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
        if ptr.is_null() { Err(last_os_error("VirtualAlloc")) } else { Ok(ptr as *mut u8) }
    }

    pub fn write_and_seal(ptr: *mut u8, mapped: usize, code: &[u8]) -> Result<(), ExecError> {
        // SAFETY: `ptr` is a fresh read+write allocation of `mapped >= code.len()` bytes owned by the
        // caller; after the copy it becomes execute+read and the instruction cache is flushed.
        unsafe {
            std::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
            let mut old = 0u32;
            if VirtualProtect(ptr as *const _, mapped, PAGE_EXECUTE_READ, &mut old) == 0 {
                return Err(last_os_error("VirtualProtect"));
            }
            if FlushInstructionCache(GetCurrentProcess(), ptr as *const _, code.len()) == 0 {
                return Err(last_os_error("FlushInstructionCache"));
            }
        }
        Ok(())
    }

    pub fn unmap(ptr: *mut u8, _len: usize) {
        // SAFETY: `ptr` came from VirtualAlloc and the owning ExecBuffer is being dropped.
        unsafe {
            VirtualFree(ptr as *mut _, 0, MEM_RELEASE);
        }
    }
}
