//! Synchronous Linux AArch64 SIGSEGV delivery. Frames follow the Linux UAPI
//! rt_sigframe/ucontext layout; guest handlers may return or siglongjmp away.
use super::syscalls::{Process, EFAULT, EINVAL};
use crate::cpu::{Cpu, CpuError};
use crate::mmu::{Memory, MemoryError, Permission};

const FRAME_SIZE: u64 = 4688;
const MC: u64 = 128 + 176;
const FP: u64 = MC + 288;
const TRAMPOLINE: u64 = super::elf::MMAP_BASE - super::elf::PAGE;

impl Process {
    pub(super) fn sys_sigaltstack(&mut self, a: [u64; 6], mmu: &mut Memory) -> i64 {
        if a[1] != 0 {
            for i in 0..3 {
                if mmu.write_u64(a[1] + i * 8, self.alt_stack[i as usize]).is_err() { return -EFAULT; }
            }
        }
        if a[0] != 0 {
            let (Ok(sp), Ok(flags), Ok(size)) = (mmu.read_u64(a[0]), mmu.read_u32(a[0] + 8), mmu.read_u64(a[0] + 16)) else { return -EFAULT; };
            if flags != 0 && flags != 2 { return -EINVAL; }
            if flags == 0 && size < 2048 { return -12; }
            self.alt_stack = [sp, flags as u64, size];
        }
        0
    }

    pub(super) fn deliver_segv(&mut self, cpu: &mut Cpu, mmu: &mut Memory, pc: u64) -> bool {
        let action = self.sigactions[11];
        let handler = u64::from_le_bytes(action[0..8].try_into().unwrap());
        if handler <= 1 || self.signal_mask & (1 << 10) != 0 { return false; }
        // Re-step a clone: only actual data/access faults become SIGSEGV, never
        // unsupported instructions or an engine disagreement. Preserve fault state.
        let mut probe = Cpu::new();
        probe.regs = cpu.regs.clone();
        probe.regs.pc = pc;
        let Some(mut snapshot) = mmu.fault_snapshot() else { return false; };
        let (addr, code) = match probe.step(&mut snapshot) {
            Err(CpuError::Memory(MemoryError::Unmapped { addr })) => (addr, if self.covered(addr, addr.saturating_add(1), mmu) { 2u32 } else { 1u32 }),
            Err(CpuError::Memory(MemoryError::PermissionDenied { addr, .. })) => (addr, 2u32),
            _ => return false,
        };
        let flags = u64::from_le_bytes(action[8..16].try_into().unwrap());
        let alt = self.alt_stack;
        let on_alt = alt[1] != 2 && cpu.regs.sp >= alt[0] && cpu.regs.sp < alt[0].saturating_add(alt[2]);
        let top = if flags & 0x08000000 != 0 && alt[1] != 2 && !on_alt { alt[0].saturating_add(alt[2]) } else { cpu.regs.sp };
        let Some(frame) = top.checked_sub(FRAME_SIZE).map(|v| v & !15) else { return false; };
        let mut data = vec![0u8; FRAME_SIZE as usize];
        let put = |data: &mut [u8], off: u64, v: u64| data[off as usize..off as usize + 8].copy_from_slice(&v.to_le_bytes());
        put(&mut data, 0, 11);
        data[8..12].copy_from_slice(&code.to_le_bytes());
        put(&mut data, 16, addr);
        put(&mut data, 128 + 16, alt[0]);
        put(&mut data, 128 + 24, if on_alt { 1 } else { alt[1] });
        put(&mut data, 128 + 32, alt[2]);
        put(&mut data, 128 + 40, self.signal_mask);
        put(&mut data, MC, addr);
        for i in 0..31 { put(&mut data, MC + 8 + i * 8, cpu.regs.x[i as usize]); }
        put(&mut data, MC + 256, cpu.regs.sp);
        put(&mut data, MC + 264, pc);
        put(&mut data, MC + 272, cpu.regs.pstate.to_bits() as u64);
        put(&mut data, FP, 0x210_46508001); // fpsimd_context header: size=528
        put(&mut data, FP + 8, cpu.regs.fpsr as u64 | (cpu.regs.fpcr as u64) << 32);
        for i in 0..32 { data[(FP + 16) as usize + i * 16..(FP + 16) as usize + (i + 1) * 16].copy_from_slice(&cpu.regs.v[i].to_le_bytes()); }
        if mmu.write_bytes(frame, &data).is_err() { return false; }
        if !mmu.is_mapped(TRAMPOLINE) {
            mmu.map_range(TRAMPOLINE, 4096, Permission::READ_WRITE_EXECUTE);
            mmu.write_u32(TRAMPOLINE, 0xd2801168).unwrap(); // mov x8, #139
            mmu.write_u32(TRAMPOLINE + 4, 0xd4000001).unwrap(); // svc #0
        }
        self.signal_mask |= u64::from_le_bytes(action[24..32].try_into().unwrap());
        if flags & 0x40000000 == 0 { self.signal_mask |= 1 << 10; }
        if flags & 0x80000000 != 0 { self.sigactions[11] = [0; 32]; }
        cpu.regs.pstate.btype = 0;
        cpu.regs.sp = frame;
        cpu.regs.pc = handler;
        cpu.regs.x[0] = 11;
        cpu.regs.x[1] = frame;
        cpu.regs.x[2] = frame + 128;
        cpu.regs.x[30] = if flags & 0x04000000 != 0 { u64::from_le_bytes(action[16..24].try_into().unwrap()) } else { TRAMPOLINE };
        true
    }

    pub(super) fn signal_return(&mut self, cpu: &mut Cpu, mmu: &Memory) -> Result<(), MemoryError> {
        let frame = cpu.regs.sp;
        if frame % 16 != 0 || frame.checked_add(FRAME_SIZE).is_none() { return Err(MemoryError::Unmapped { addr: frame }); }
        let mut regs = cpu.regs.clone();
        for i in 0..31 { regs.x[i] = mmu.read_u64(frame + MC + 8 + i as u64 * 8)?; }
        regs.sp = mmu.read_u64(frame + MC + 256)?;
        regs.pc = mmu.read_u64(frame + MC + 264)?;
        regs.pstate.from_bits(mmu.read_u64(frame + MC + 272)? as u32);
        regs.fpsr = mmu.read_u32(frame + FP + 8)?;
        regs.fpcr = mmu.read_u32(frame + FP + 12)?;
        for i in 0..32 {
            let mut v = [0; 16]; mmu.read_bytes(frame + FP + 16 + i as u64 * 16, &mut v)?;
            regs.v[i] = u128::from_le_bytes(v);
        }
        self.signal_mask = mmu.read_u64(frame + 128 + 40)? & !(1 << 8 | 1 << 18);
        cpu.regs = regs;
        Ok(())
    }
}
