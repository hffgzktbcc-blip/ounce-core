//! Entering guest code and getting back out.
//!
//! [`enter`] saves the host's callee-saved registers on the host stack, loads the guest register
//! file from a [`crate::Context`] and branches to a trampoline in the patch area (a resume stub or the
//! launch slot, see [`crate::patch`]). Guest code then runs on the host CPU until it reaches a
//! rewritten instruction. That trampoline stores the guest registers into the same `Context` and
//! jumps to the host return path, which restores the host stack and returns from [`enter`].
//!
//! A guest fault (bad access, undefined instruction, `brk`) raises a signal. While guest code runs
//! on this thread the handler records the guest registers and the fault in the `Context` and
//! resumes the host at the same return path, so [`enter`] returns [`Exit::Fault`] instead of the
//! process dying. Other signals go to the handler that was installed before.
//!
//! Preemption: while guest code runs, a watchdog thread signals the running thread (`SIGUSR2`) once
//! per time slice. If the thread is in guest code (not in a stub or host code), the handler saves
//! the full register file and returns [`Exit::Preempted`], so a guest thread that spins without
//! SVCs still yields to the scheduler and to the host's deadlines.
//!
//! Guest code shares the host's address space and privileges: native execution is for homebrew the
//! user would run natively anyway, not a sandbox.

#[cfg_attr(not(target_arch = "aarch64"), allow(unused_imports))] // `Context` is used by the aarch64 signal handlers
use crate::{Arena, Context, NceError};

/// Why [`enter`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// A trampoline with this exit id (`Context::exit_id`) left guest code.
    Trampoline(u64),
    /// Guest code raised `signal` at `pc` (fault address `addr` where the signal has one).
    Fault { signal: i32, pc: u64, addr: u64 },
    /// The time slice ran out in guest code; the context holds the complete guest state at `pc`.
    Preempted { pc: u64 },
}

/// Guest-code ranges the preemption signal must not interrupt: the patch areas, whose stubs
/// expect x16/x17 below `sp` as they left them. At most this many.
pub const MAX_NO_PREEMPT: usize = 8;

#[cfg(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux")))]
mod imp {
    use super::*;
    use crate::context::off;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    thread_local! {
        /// Address of the `Context` of the guest code running on this thread, 0 when none runs.
        static ACTIVE: Cell<u64> = const { Cell::new(0) };
        static ALT_STACK: Cell<bool> = const { Cell::new(false) };
        /// The arena window and the ranges inside it not to preempt (see [`MAX_NO_PREEMPT`]).
        static WINDOW: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
        static NO_PREEMPT: Cell<[(u64, u64); MAX_NO_PREEMPT]> = const { Cell::new([(0, 0); MAX_NO_PREEMPT]) };
        static SLOT: Cell<Option<&'static Slot>> = const { Cell::new(None) };
    }

    const PREEMPT_SIGNAL: libc::c_int = libc::SIGUSR2;

    /// A thread the watchdog may preempt.
    struct Slot {
        thread: libc::pthread_t,
        /// Bumped on every entry into guest code.
        generation: AtomicU64,
        active: AtomicBool,
        slice_us: AtomicU64,
    }

    // SAFETY: pthread_t is an opaque thread id; the rest are atomics.
    unsafe impl Sync for Slot {}

    static SLOTS: Mutex<Vec<&'static Slot>> = Mutex::new(Vec::new());

    /// One watchdog thread for the process: every slice, signals threads still in the same guest
    /// entry as at its previous look.
    fn watchdog() {
        let mut seen: Vec<(usize, u64)> = Vec::new();
        loop {
            let slots: Vec<&'static Slot> = SLOTS.lock().unwrap().clone();
            let slice = slots.iter().map(|s| s.slice_us.load(Ordering::Relaxed)).filter(|&v| v > 0).min().unwrap_or(1000);
            std::thread::sleep(std::time::Duration::from_micros(slice));
            for (i, slot) in slots.iter().enumerate() {
                let generation = slot.generation.load(Ordering::Acquire);
                let active = slot.active.load(Ordering::Acquire);
                let last = seen.iter().find(|(j, _)| *j == i).map(|&(_, g)| g);
                if active && slot.slice_us.load(Ordering::Relaxed) > 0 && last == Some(generation) {
                    // SAFETY: the thread registered itself and its slot lives forever; a thread
                    // that has exited is never active again, so it is never signalled.
                    unsafe { libc::pthread_kill(slot.thread, PREEMPT_SIGNAL) };
                }
                match seen.iter_mut().find(|(j, _)| *j == i) {
                    Some(entry) => entry.1 = generation,
                    None => seen.push((i, generation)),
                }
            }
        }
    }

    fn slot() -> &'static Slot {
        if let Some(slot) = SLOT.with(|s| s.get()) {
            return slot;
        }
        // SAFETY: pthread_self has no preconditions.
        let slot: &'static Slot = Box::leak(Box::new(Slot { thread: unsafe { libc::pthread_self() },
            generation: AtomicU64::new(0), active: AtomicBool::new(false), slice_us: AtomicU64::new(0) }));
        let mut slots = SLOTS.lock().unwrap();
        if slots.is_empty() {
            std::thread::Builder::new().name("ounce-nce-watchdog".into()).spawn(watchdog).expect("spawn watchdog");
        }
        slots.push(slot);
        SLOT.with(|s| s.set(Some(slot)));
        slot
    }

    const SIGNALS: [libc::c_int; 6] = [libc::SIGSEGV, libc::SIGBUS, libc::SIGILL, libc::SIGTRAP, libc::SIGFPE, libc::SIGUSR2];
    static PREVIOUS: OnceLock<Vec<(libc::c_int, libc::sigaction)>> = OnceLock::new();

    /// Host frame: x19-x30, d8-d15 and FPCR/FPSR, 176 bytes, kept 16-byte aligned.
    macro_rules! host_epilogue {
        () => {
            concat!(
                "ldr x0, [x17, #{host_sp}]\n",
                "mov sp, x0\n",
                "ldp x19, x20, [sp, #0]\n",
                "ldp x21, x22, [sp, #16]\n",
                "ldp x23, x24, [sp, #32]\n",
                "ldp x25, x26, [sp, #48]\n",
                "ldp x27, x28, [sp, #64]\n",
                "ldp x29, x30, [sp, #80]\n",
                "ldp d8, d9, [sp, #96]\n",
                "ldp d10, d11, [sp, #112]\n",
                "ldp d12, d13, [sp, #128]\n",
                "ldp d14, d15, [sp, #144]\n",
                "ldp x0, x1, [sp, #160]\n",
                "msr fpcr, x0\n",
                "msr fpsr, x1\n",
                "add sp, sp, #176\n",
            )
        };
    }

    /// `nce_enter(ctx, target)`: run guest code from `target` with the registers in `ctx`.
    /// Returns 0 when a trampoline left guest code, 1 after a fault (see `fault_return`).
    #[unsafe(naked)]
    unsafe extern "C" fn nce_enter(ctx: u64, target: u64) -> u64 {
        core::arch::naked_asm!(
            "sub sp, sp, #176",
            "stp x19, x20, [sp, #0]",
            "stp x21, x22, [sp, #16]",
            "stp x23, x24, [sp, #32]",
            "stp x25, x26, [sp, #48]",
            "stp x27, x28, [sp, #64]",
            "stp x29, x30, [sp, #80]",
            "stp d8, d9, [sp, #96]",
            "stp d10, d11, [sp, #112]",
            "stp d12, d13, [sp, #128]",
            "stp d14, d15, [sp, #144]",
            "mrs x2, fpcr",
            "mrs x3, fpsr",
            "stp x2, x3, [sp, #160]",
            "mov x2, sp",
            "str x2, [x0, #{host_sp}]",
            "adr x2, 2f",
            "str x2, [x0, #{host_ret}]",
            // Guest SIMD/FP state.
            "add x2, x0, #{v}",
            "ldp q0, q1, [x2, #0]",
            "ldp q2, q3, [x2, #32]",
            "ldp q4, q5, [x2, #64]",
            "ldp q6, q7, [x2, #96]",
            "ldp q8, q9, [x2, #128]",
            "ldp q10, q11, [x2, #160]",
            "ldp q12, q13, [x2, #192]",
            "ldp q14, q15, [x2, #224]",
            "ldp q16, q17, [x2, #256]",
            "ldp q18, q19, [x2, #288]",
            "ldp q20, q21, [x2, #320]",
            "ldp q22, q23, [x2, #352]",
            "ldp q24, q25, [x2, #384]",
            "ldp q26, q27, [x2, #416]",
            "ldp q28, q29, [x2, #448]",
            "ldp q30, q31, [x2, #480]",
            "ldr x2, [x0, #{fpcr}]",
            "msr fpcr, x2",
            "ldr x2, [x0, #{fpsr}]",
            "msr fpsr, x2",
            "ldr x2, [x0, #{nzcv}]",
            "msr nzcv, x2",
            // Guest x16/x17 go just below the guest sp, where every trampoline reloads them from.
            "ldr x2, [x0, #{sp}]",
            "ldp x3, x4, [x0, #128]",
            "stp x3, x4, [x2, #-16]",
            "mov sp, x2",
            "mov x16, x1",
            "ldp x2, x3, [x0, #16]",
            "ldp x4, x5, [x0, #32]",
            "ldp x6, x7, [x0, #48]",
            "ldp x8, x9, [x0, #64]",
            "ldp x10, x11, [x0, #80]",
            "ldp x12, x13, [x0, #96]",
            "ldp x14, x15, [x0, #112]",
            // x18 stays in the context (the rewritten code keeps it there).
            "ldr x19, [x0, #152]",
            "ldp x20, x21, [x0, #160]",
            "ldp x22, x23, [x0, #176]",
            "ldp x24, x25, [x0, #192]",
            "ldp x26, x27, [x0, #208]",
            "ldp x28, x29, [x0, #224]",
            "ldr x30, [x0, #240]",
            "ldp x0, x1, [x0, #0]",
            "br x16",
            // Host return path: the trampoline exit jumps here with x17 = ctx.
            "2:",
            host_epilogue!(),
            "mov x0, #0",
            "ret",
            host_sp = const off::HOST_SP,
            host_ret = const off::HOST_RET,
            v = const off::V,
            fpcr = const off::FPCR,
            fpsr = const off::FPSR,
            nzcv = const off::NZCV,
            sp = const off::SP,
        )
    }

    /// The signal handler resumes here with x17 = ctx after recording a guest fault.
    #[unsafe(naked)]
    unsafe extern "C" fn nce_fault_return() {
        core::arch::naked_asm!(
            host_epilogue!(),
            "mov x0, #1",
            "ret",
            host_sp = const off::HOST_SP,
        )
    }

    #[cfg(target_os = "macos")]
    mod uctx {
        //! Darwin arm64 `ucontext_t` / `mcontext64` (`<sys/_types/_ucontext.h>`, `<mach/arm/_structs.h>`).
        #[repr(C)]
        pub struct ThreadState { pub x: [u64; 29], pub fp: u64, pub lr: u64, pub sp: u64, pub pc: u64, pub cpsr: u32, pub pad: u32 }
        #[repr(C)]
        pub struct NeonState { pub v: [u128; 32], pub fpsr: u32, pub fpcr: u32 }
        #[repr(C)]
        pub struct MContext { pub far: u64, pub esr: u32, pub exception: u32, pub ss: ThreadState, pub ns: NeonState }
        #[repr(C)]
        pub struct UContext { pub onstack: i32, pub sigmask: u32, pub stack: libc::stack_t, pub link: *mut UContext, pub mcsize: usize, pub mcontext: *mut MContext }

        /// (x0..x28, fp, lr, sp, pc, pstate) of the interrupted code.
        pub unsafe fn regs<'a>(uc: *mut libc::c_void) -> &'a mut ThreadState {
            // SAFETY: the kernel passes a valid ucontext whose mcontext pointer is valid for the handler.
            unsafe { &mut (*(*(uc as *mut UContext)).mcontext).ss }
        }

        /// SIMD/FP state of the interrupted code.
        pub unsafe fn neon<'a>(uc: *mut libc::c_void) -> &'a NeonState {
            // SAFETY: as for `regs`; the full 64-bit mcontext includes the NEON state.
            unsafe { &(*(*(uc as *mut UContext)).mcontext).ns }
        }
    }

    /// The interrupted pc, from the signal context.
    fn interrupted_pc(uc: *mut libc::c_void) -> u64 {
        #[cfg(target_os = "macos")]
        // SAFETY: see `uctx::regs`.
        return unsafe { uctx::regs(uc) }.pc;
        #[cfg(target_os = "linux")]
        // SAFETY: the kernel passes a valid ucontext_t.
        return unsafe { (*(uc as *mut libc::ucontext_t)).uc_mcontext.pc };
    }

    /// Copies the SIMD/FP state of the interrupted code into `ctx` (preemption keeps all state).
    fn save_fp(ctx: &mut Context, uc: *mut libc::c_void) {
        #[cfg(target_os = "macos")]
        {
            // SAFETY: see `uctx::neon`.
            let ns = unsafe { uctx::neon(uc) };
            ctx.v = ns.v;
            ctx.fpsr = ns.fpsr as u64;
            ctx.fpcr = ns.fpcr as u64;
        }
        #[cfg(target_os = "linux")]
        {
            // The first record in __reserved (16-byte aligned, right after pstate) is the FPSIMD
            // context (magic 0x46508001). libc keeps the field private, so find it by offset.
            let at = (std::mem::offset_of!(libc::mcontext_t, pstate) + 8).next_multiple_of(16);
            // SAFETY: the kernel passes a valid ucontext_t; `at` lies inside its mcontext.
            let reserved = unsafe { (&raw const (*(uc as *mut libc::ucontext_t)).uc_mcontext as *const u8).add(at) };
            // SAFETY: reading the record header and body the kernel wrote.
            unsafe {
                if *(reserved as *const u32) == 0x4650_8001 {
                    ctx.fpsr = *(reserved.add(8) as *const u32) as u64;
                    ctx.fpcr = *(reserved.add(12) as *const u32) as u64;
                    std::ptr::copy_nonoverlapping(reserved.add(16) as *const u128, ctx.v.as_mut_ptr(), 32);
                }
            }
        }
    }

    unsafe extern "C" fn handler(sig: libc::c_int, info: *mut libc::siginfo_t, uc: *mut libc::c_void) {
        let ctx_va = ACTIVE.with(|a| a.get());
        if sig == PREEMPT_SIGNAL {
            // Only preempt guest code itself: never stubs (their x16/x17 live below sp) or host code.
            let pc = interrupted_pc(uc);
            let (lo, hi) = WINDOW.with(|w| w.get());
            let stub = NO_PREEMPT.with(|n| n.get()).iter().any(|&(a, b)| pc >= a && pc < b);
            if ctx_va == 0 || pc < lo || pc >= hi || stub {
                return;
            }
            // SAFETY: as below for faults.
            let ctx = unsafe { &mut *(ctx_va as *mut Context) };
            save_fp(ctx, uc);
        } else if ctx_va == 0 {
            // SAFETY: forwarding the arguments the kernel gave us.
            unsafe { chain(sig, info, uc) };
            return;
        }
        // SAFETY: while ACTIVE is set the context is committed, writable arena memory, and the host
        // is suspended inside `nce_enter`, so nothing else references it.
        let ctx = unsafe { &mut *(ctx_va as *mut Context) };
        if sig != PREEMPT_SIGNAL {
            // A fault leaves guest code mid-run (a branch into an interpreted page, for one): the
            // SIMD/FP registers it holds carry on from here, so they must reach the context too.
            save_fp(ctx, uc);
        }
        // SAFETY: `info` is valid for the handler's duration.
        ctx.fault_addr = unsafe { (*info).si_addr() } as u64;
        ctx.fault_signal = sig as u64;
        #[cfg(target_os = "macos")]
        {
            // SAFETY: see `uctx::regs`.
            let ss = unsafe { uctx::regs(uc) };
            for i in (0..29).filter(|&i| i != 18) {
                ctx.x[i] = ss.x[i];
            }
            ctx.x[29] = ss.fp;
            ctx.x[30] = ss.lr;
            ctx.sp = ss.sp;
            ctx.pc = ss.pc;
            ctx.nzcv = (ss.cpsr & 0xF000_0000) as u64;
            ctx.fault_pc = ss.pc;
            ss.x[17] = ctx_va;
            ss.pc = nce_fault_return as *const () as usize as u64;
        }
        #[cfg(target_os = "linux")]
        {
            // SAFETY: the kernel passes a valid ucontext_t.
            let mc = unsafe { &mut (*(uc as *mut libc::ucontext_t)).uc_mcontext };
            for i in (0..31).filter(|&i| i != 18) {
                ctx.x[i] = mc.regs[i];
            }
            ctx.sp = mc.sp;
            ctx.pc = mc.pc;
            ctx.nzcv = mc.pstate & 0xF000_0000;
            ctx.fault_pc = mc.pc;
            mc.regs[17] = ctx_va;
            mc.pc = nce_fault_return as *const () as usize as u64;
        }
    }

    /// Hands a signal that is not a guest fault to the handler installed before ours.
    unsafe fn chain(sig: libc::c_int, info: *mut libc::siginfo_t, uc: *mut libc::c_void) {
        let Some(old) = PREVIOUS.get().and_then(|v| v.iter().find(|(s, _)| *s == sig)).map(|(_, a)| *a) else { return };
        let h = old.sa_sigaction;
        if old.sa_flags & libc::SA_SIGINFO != 0 && h != libc::SIG_DFL && h != libc::SIG_IGN {
            // SAFETY: the previous owner registered this as an SA_SIGINFO handler.
            let f: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) = unsafe { std::mem::transmute(h) };
            f(sig, info, uc);
        } else if h != libc::SIG_DFL && h != libc::SIG_IGN {
            // SAFETY: a plain handler registered by the previous owner.
            let f: extern "C" fn(libc::c_int) = unsafe { std::mem::transmute(h) };
            f(sig);
        } else {
            // Restore the previous disposition; returning re-raises the fault under it.
            // SAFETY: reinstalling a sigaction the kernel gave us.
            unsafe { libc::sigaction(sig, &old, std::ptr::null_mut()) };
        }
    }

    fn install() -> Result<(), NceError> {
        let mut err = None;
        slot();
        PREVIOUS.get_or_init(|| {
            let mut saved = Vec::new();
            for &sig in &SIGNALS {
                // SAFETY: zeroed sigaction is a valid starting value; fields are set before use.
                let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
                sa.sa_sigaction = handler as *const () as usize;
                sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK | libc::SA_RESTART;
                // SAFETY: sigemptyset on a local.
                unsafe { libc::sigemptyset(&mut sa.sa_mask) };
                // SAFETY: zeroed out-parameter for the previous action.
                let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
                // SAFETY: installing a handler with valid structs.
                if unsafe { libc::sigaction(sig, &sa, &mut old) } != 0 {
                    err = Some(NceError::Os("sigaction", std::io::Error::last_os_error().raw_os_error().unwrap_or(0)));
                }
                saved.push((sig, old));
            }
            saved
        });
        err.map_or(Ok(()), Err)?;
        // Guest faults may hit with a bad guest sp: give this thread an alternate signal stack.
        if !ALT_STACK.with(|a| a.get()) {
            const SIZE: usize = 128 * 1024;
            let stack: &'static mut [u8] = Box::leak(vec![0u8; SIZE].into_boxed_slice());
            let ss = libc::stack_t { ss_sp: stack.as_mut_ptr() as *mut libc::c_void, ss_flags: 0, ss_size: SIZE };
            // SAFETY: the stack is leaked, so it outlives the thread.
            if unsafe { libc::sigaltstack(&ss, std::ptr::null_mut()) } != 0 {
                return Err(NceError::Os("sigaltstack", std::io::Error::last_os_error().raw_os_error().unwrap_or(0)));
            }
            ALT_STACK.with(|a| a.set(true));
        }
        Ok(())
    }

    pub fn set_preemption(slice: Option<std::time::Duration>, no_preempt: &[(u64, u64)]) {
        let slot = slot();
        slot.slice_us.store(slice.map_or(0, |d| (d.as_micros() as u64).max(100)), Ordering::Relaxed);
        let mut ranges = [(0, 0); MAX_NO_PREEMPT];
        for (r, &n) in ranges.iter_mut().zip(no_preempt) {
            *r = n;
        }
        NO_PREEMPT.with(|n| n.set(ranges));
    }

    pub fn enter(arena: &mut Arena, ctx_va: u64, target: u64) -> Result<Exit, NceError> {
        install()?;
        WINDOW.with(|w| w.set((arena.base(), arena.base() + arena.size())));
        if !arena.is_sealed(target) {
            return Err(NceError::Range { va: target, len: 4 });
        }
        let ctx = arena.context_mut(ctx_va)?;
        ctx.fault_signal = 0;
        if !ctx.sp.is_multiple_of(16) {
            return Err(NceError::Range { va: ctx.sp, len: 16 });
        }
        let slot = slot();
        slot.generation.fetch_add(1, Ordering::AcqRel);
        ACTIVE.with(|a| a.set(ctx_va));
        slot.active.store(true, Ordering::Release);
        // SAFETY: `ctx_va` is a committed, writable, aligned Context inside the arena and `target`
        // is sealed guest code; `&mut arena` keeps the host from touching guest memory meanwhile.
        // What the guest code itself does is outside the host's control (see the module doc).
        let faulted = unsafe { nce_enter(ctx_va, target) };
        slot.active.store(false, Ordering::Release);
        ACTIVE.with(|a| a.set(0));
        let ctx = arena.context_mut(ctx_va)?;
        Ok(if faulted != 0 && ctx.fault_signal == PREEMPT_SIGNAL as u64 {
            Exit::Preempted { pc: ctx.pc }
        } else if faulted != 0 {
            Exit::Fault { signal: ctx.fault_signal as i32, pc: ctx.fault_pc, addr: ctx.fault_addr }
        } else {
            Exit::Trampoline(ctx.exit_id)
        })
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::patch::Patcher;

        fn fp_state() -> (u64, u64) {
            let (fpcr, fpsr);
            // SAFETY: reading the current thread's user-accessible FP registers.
            unsafe { core::arch::asm!("mrs {0}, fpcr", "mrs {1}, fpsr", out(reg) fpcr, out(reg) fpsr, options(nostack)); }
            (fpcr, fpsr)
        }

        struct RestoreFp(u64, u64);
        impl Drop for RestoreFp {
            fn drop(&mut self) {
                // SAFETY: restoring values read from this thread, including on test failure.
                unsafe { core::arch::asm!("msr fpcr, {0}", "msr fpsr, {1}", in(reg) self.0, in(reg) self.1, options(nostack)); }
            }
        }

        #[test]
        fn host_fp_state_survives_trampoline_and_fault() {
            for word in [0xD400_00E1u32, 0xF940_001F] {
                let mut arena = Arena::reserve(1 << 32).unwrap();
                let base = arena.base();
                let (ctx, code, stack) = (base + 0x10_0000, base + 0x100_0000, base + 0x200_0000);
                let mut patcher = Patcher::new(ctx, 0x1_0000).unwrap();
                let mut words = [word];
                patcher.rewrite(code, &mut words).unwrap();
                arena.commit(ctx, 0x3_0000).unwrap();
                arena.commit(code, 0x1_0000).unwrap();
                arena.commit(stack, 0x1_0000).unwrap();
                let bytes = |w: &[u32]| w.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<_>>();
                arena.write(code, &bytes(&words)).unwrap();
                let slot = patcher.launch_slot(code).unwrap();
                arena.write(patcher.code_base(), &bytes(&patcher.areas()[0].code)).unwrap();
                arena.write(slot, &bytes(&Patcher::launch_words(slot, code).unwrap())).unwrap();
                arena.seal_exec(code, 0x1_0000).unwrap();
                arena.seal_exec(patcher.code_base(), 0x1_0000).unwrap();
                let host = fp_state();
                let restore = RestoreFp(host.0, host.1);
                let guest = (host.0 ^ (1 << 22), host.1 ^ 1);
                let context = arena.context_mut(ctx).unwrap();
                context.sp = stack + 0x1_0000;
                context.fpcr = guest.0;
                context.fpsr = guest.1;
                let exit = enter(&mut arena, ctx, slot).unwrap();
                let after = fp_state();
                drop(restore);
                assert_eq!(after, host, "host FP state leaked after {exit:?}");
                let context = arena.context_mut(ctx).unwrap();
                assert_eq!((context.fpcr, context.fpsr), guest);
            }
        }
    }

}

#[cfg(not(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux"))))]
mod imp {
    use super::*;
    pub fn set_preemption(_slice: Option<std::time::Duration>, _no_preempt: &[(u64, u64)]) {}
    pub fn enter(_arena: &mut Arena, _ctx_va: u64, _target: u64) -> Result<Exit, NceError> {
        Err(NceError::Unsupported)
    }
}

/// Sets this thread's preemption time slice (`None` = never preempt) and the ranges (stub areas)
/// the preemption signal must leave alone. At most [`MAX_NO_PREEMPT`] ranges are kept.
pub fn set_preemption(slice: Option<std::time::Duration>, no_preempt: &[(u64, u64)]) {
    imp::set_preemption(slice, no_preempt)
}

/// Runs guest code from `target` (a sealed trampoline) with the registers in the [`crate::Context`] at
/// `ctx_va` until a trampoline or a fault returns control. See the module documentation.
pub fn enter(arena: &mut Arena, ctx_va: u64, target: u64) -> Result<Exit, NceError> {
    imp::enter(arena, ctx_va, target)
}
