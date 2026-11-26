//! Synchronous Linux AArch64 SIGSEGV, SIGBUS and SIGILL delivery. Frames follow the Linux UAPI
//! rt_sigframe/ucontext layout; guest handlers may return or siglongjmp away.
use super::syscalls::{EFAULT, EINVAL, ESRCH, Outcome, PID, Process};
use crate::cpu::{Cpu, CpuError};
use crate::mmu::{Memory, MemoryError, Permission};

/// What became of a fault the engine reported.
pub(super) enum SyncFault {
    /// A guest handler now runs.
    Delivered,
    /// Not a guest fault, or `SIGSEGV` with no usable handler: the runner reports it as a CPU stop.
    NotDelivered,
    /// `SIGILL` or `SIGBUS` that no handler takes: the process dies of that signal, status `128 + signal`.
    Fatal(i32),
}

const FRAME_SIZE: u64 = 4688;
const MC: u64 = 128 + 176;
const FP: u64 = MC + 288;
const TRAMPOLINE: u64 = super::elf::MMAP_BASE - super::elf::PAGE;

impl Process {
    pub(super) fn sys_sigaltstack(&mut self, a: [u64; 6], mmu: &mut Memory) -> i64 {
        if a[1] != 0 {
            for i in 0..3 {
                if mmu.write_u64(a[1] + i * 8, self.alt_stack[i as usize]).is_err() {
                    return -EFAULT;
                }
            }
        }
        if a[0] != 0 {
            let (Ok(sp), Ok(flags), Ok(size)) = (mmu.read_u64(a[0]), mmu.read_u32(a[0] + 8), mmu.read_u64(a[0] + 16))
            else {
                return -EFAULT;
            };
            if flags != 0 && flags != 2 {
                return -EINVAL;
            }
            if flags == 0 && size < 2048 {
                return -12;
            }
            self.alt_stack = [sp, flags as u64, size];
        }
        0
    }

    /// `tgkill(tgid, tid, sig)` and `tkill(tid, sig)` aimed at the guest's only thread, as `raise` and `abort` do
    /// (musl uses `tkill`, glibc `tgkill`). A signal that is blocked stays pending until `rt_sigprocmask` unblocks it,
    /// as in the kernel; `musl`'s `abort` relies on that order.
    pub(super) fn sys_tgkill(&mut self, a: [u64; 6]) -> Outcome {
        if (a[0] as i64) <= 0 {
            return Outcome::Return(-EINVAL);
        }
        if a[0] != PID {
            return Outcome::Return(-ESRCH);
        }
        self.sys_tkill([a[1], a[2], 0, 0, 0, 0])
    }

    pub(super) fn sys_tkill(&mut self, a: [u64; 6]) -> Outcome {
        if (a[0] as i64) <= 0 || a[1] > 64 {
            return Outcome::Return(-EINVAL);
        }
        if a[0] != PID {
            return Outcome::Return(-ESRCH);
        }
        if a[1] == 0 {
            return Outcome::Return(0);
        }
        self.signal_pending |= 1 << (a[1] - 1);
        self.deliver_pending().unwrap_or(Outcome::Return(0))
    }

    /// Delivers the lowest pending, unblocked signal that has an effect. A default action that terminates the
    /// process ends the guest with status `128 + signal`, as a shell reports it. Returns `None` when nothing
    /// ends or stops the run. A guest handler for a raised signal, and the stop actions, are not implemented.
    pub(super) fn deliver_pending(&mut self) -> Option<Outcome> {
        const UNBLOCKABLE: u64 = 1 << 8 | 1 << 18; // SIGKILL, SIGSTOP
        loop {
            let ready = self.signal_pending & (!self.signal_mask | UNBLOCKABLE);
            if ready == 0 {
                return None;
            }
            let bit = ready.trailing_zeros() as u64;
            let sig = bit + 1;
            self.signal_pending &= !(1 << bit);
            let handler = u64::from_le_bytes(self.sigactions[sig as usize][0..8].try_into().unwrap());
            match handler {
                1 => {} // SIG_IGN
                0 => match sig {
                    17 | 18 | 23 | 28 => {} // SIGCHLD, SIGCONT, SIGURG, SIGWINCH: ignored by default
                    19..=22 => {
                        return Some(Outcome::Unimplemented(format!(
                            "default stop action of signal {sig} (tgkill)"
                        )));
                    }
                    _ => return Some(Outcome::Exit(128 + sig as i32)),
                },
                _ => {
                    return Some(Outcome::Unimplemented(format!(
                        "delivery of signal {sig} to a guest handler (tgkill)"
                    )));
                }
            }
        }
    }

    /// Turns the fault the engine reported at `pc` into the signal Linux raises for it, using the same
    /// frame for all three:
    /// - an access to unmapped or protected memory: `SIGSEGV` (`SEGV_MAPERR`/`SEGV_ACCERR`, `si_addr` = address);
    /// - a misaligned exclusive: `SIGBUS` with `BUS_ADRALN` and `si_addr` = address (`arch/arm64/mm/fault.c`,
    ///   `do_alignment_fault`);
    /// - an UNDEFINED encoding or an EL0 access to a system register Linux does not expose: `SIGILL` with
    ///   `ILL_ILLOPC` and `si_addr` = the faulting PC (`arch/arm64/kernel/traps.c`, `do_el0_undef`; `do_el0_sys`
    ///   falls back to it).
    ///
    /// `SIGILL` and `SIGBUS` are forced signals: with no handler, or with the signal ignored or blocked, the
    /// kernel resets the action to default and the process dies, so those return [`SyncFault::Fatal`].
    /// `SIGSEGV` without a handler stays [`SyncFault::NotDelivered`] (reported as a memory fault).
    pub(super) fn deliver_fault(&mut self, cpu: &mut Cpu, mmu: &mut Memory, pc: u64) -> SyncFault {
        // Re-step a clone to learn why the instruction failed. Only guest faults become signals, never
        // unimplemented instructions or an engine disagreement. Preserve fault state.
        let mut probe = Cpu::new();
        probe.regs = cpu.regs.clone();
        probe.regs.pc = pc;
        probe.exclusive = cpu.exclusive;
        probe.linux_user = cpu.linux_user;
        let Some(mut snapshot) = mmu.fault_snapshot() else {
            return SyncFault::NotDelivered;
        };
        let (sig, addr, code) = match probe.step(&mut snapshot) {
            Err(CpuError::Memory(MemoryError::Unmapped { addr })) => (
                11u64,
                addr,
                if self.covered(addr, addr.saturating_add(1), mmu) {
                    2u32
                } else {
                    1u32
                },
            ),
            Err(CpuError::Memory(MemoryError::PermissionDenied { addr, .. })) => (11, addr, 2u32),
            Err(CpuError::Memory(MemoryError::Misaligned { addr, .. })) => (7, addr, 1), // BUS_ADRALN
            Err(CpuError::Undefined { .. } | CpuError::IllegalSystemRegister { .. }) => (4, pc, 1), // ILL_ILLOPC
            _ => return SyncFault::NotDelivered,
        };
        let action = self.sigactions[sig as usize];
        let handler = u64::from_le_bytes(action[0..8].try_into().unwrap());
        let bit = 1u64 << (sig - 1);
        if handler <= 1 || self.signal_mask & bit != 0 {
            return if sig == 11 {
                SyncFault::NotDelivered
            } else {
                SyncFault::Fatal(sig as i32)
            };
        }
        let flags = u64::from_le_bytes(action[8..16].try_into().unwrap());
        let alt = self.alt_stack;
        let on_alt = alt[1] != 2 && cpu.regs.sp >= alt[0] && cpu.regs.sp < alt[0].saturating_add(alt[2]);
        let top = if flags & 0x08000000 != 0 && alt[1] != 2 && !on_alt {
            alt[0].saturating_add(alt[2])
        } else {
            cpu.regs.sp
        };
        let Some(frame) = top.checked_sub(FRAME_SIZE).map(|v| v & !15) else {
            return SyncFault::NotDelivered;
        };
        let mut data = vec![0u8; FRAME_SIZE as usize];
        let put =
            |data: &mut [u8], off: u64, v: u64| data[off as usize..off as usize + 8].copy_from_slice(&v.to_le_bytes());
        put(&mut data, 0, sig);
        data[8..12].copy_from_slice(&code.to_le_bytes());
        put(&mut data, 16, addr);
        put(&mut data, 128 + 16, alt[0]);
        put(&mut data, 128 + 24, if on_alt { 1 } else { alt[1] });
        put(&mut data, 128 + 32, alt[2]);
        put(&mut data, 128 + 40, self.signal_mask);
        // fault_address: the faulting address, but 0 for SIGILL (`arm64_notify_die` clears it).
        put(&mut data, MC, if sig == 4 { 0 } else { addr });
        for i in 0..31 {
            put(&mut data, MC + 8 + i * 8, cpu.regs.x[i as usize]);
        }
        put(&mut data, MC + 256, cpu.regs.sp);
        put(&mut data, MC + 264, pc);
        put(&mut data, MC + 272, cpu.regs.pstate.to_bits() as u64);
        put(&mut data, FP, 0x210_46508001); // fpsimd_context header: size=528
        put(&mut data, FP + 8, cpu.regs.fpsr as u64 | (cpu.regs.fpcr as u64) << 32);
        for i in 0..32 {
            data[(FP + 16) as usize + i * 16..(FP + 16) as usize + (i + 1) * 16]
                .copy_from_slice(&cpu.regs.v[i].to_le_bytes());
        }
        if mmu.write_bytes(frame, &data).is_err() {
            return SyncFault::NotDelivered;
        }
        if !mmu.is_mapped(TRAMPOLINE) {
            mmu.map_range(TRAMPOLINE, 4096, Permission::READ_WRITE_EXECUTE);
            mmu.write_u32(TRAMPOLINE, 0xd2801168).unwrap(); // mov x8, #139
            mmu.write_u32(TRAMPOLINE + 4, 0xd4000001).unwrap(); // svc #0
        }
        self.signal_mask |= u64::from_le_bytes(action[24..32].try_into().unwrap());
        if flags & 0x40000000 == 0 {
            self.signal_mask |= bit;
        }
        if flags & 0x80000000 != 0 {
            self.sigactions[sig as usize] = [0; 32];
        }
        cpu.regs.pstate.btype = 0;
        cpu.regs.sp = frame;
        cpu.regs.pc = handler;
        cpu.regs.x[0] = sig;
        cpu.regs.x[1] = frame;
        cpu.regs.x[2] = frame + 128;
        cpu.regs.x[30] = if flags & 0x04000000 != 0 {
            u64::from_le_bytes(action[16..24].try_into().unwrap())
        } else {
            TRAMPOLINE
        };
        SyncFault::Delivered
    }

    pub(super) fn signal_return(&mut self, cpu: &mut Cpu, mmu: &Memory) -> Result<(), MemoryError> {
        let frame = cpu.regs.sp;
        if !frame.is_multiple_of(16) || frame.checked_add(FRAME_SIZE).is_none() {
            return Err(MemoryError::Unmapped { addr: frame });
        }
        let mut regs = cpu.regs.clone();
        for i in 0..31 {
            regs.x[i] = mmu.read_u64(frame + MC + 8 + i as u64 * 8)?;
        }
        regs.sp = mmu.read_u64(frame + MC + 256)?;
        regs.pc = mmu.read_u64(frame + MC + 264)?;
        regs.pstate.set_bits(mmu.read_u64(frame + MC + 272)? as u32);
        regs.fpsr = mmu.read_u32(frame + FP + 8)?;
        regs.fpcr = mmu.read_u32(frame + FP + 12)?;
        for i in 0..32 {
            let mut v = [0; 16];
            mmu.read_bytes(frame + FP + 16 + i as u64 * 16, &mut v)?;
            regs.v[i] = u128::from_le_bytes(v);
        }
        self.signal_mask = mmu.read_u64(frame + 128 + 40)? & !(1 << 8 | 1 << 18);
        cpu.regs = regs;
        Ok(())
    }
}
