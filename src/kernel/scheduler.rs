#![allow(dead_code)]

use crate::cpu::{Cpu, StepResult};
use crate::mmu::{Memory, Permission};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Created,
    Runnable,
    Running,
    Exited,
}

/// Base of the pool that per-thread static TLS areas are carved from.
pub const THREAD_TLS_POOL_BASE: u64 = 0x7F_FC00_0000;
/// Size of the per-thread TLS pool.
pub const THREAD_TLS_POOL_SIZE: u64 = 0x100_0000;

/// Initial contents of a thread's static TLS area, from the thread pointer (`TPIDR_EL0`) onward:
/// the TCB, then each module's `.tdata` followed by zeroed `.tbss`. Built by the loader.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ThreadTlsImage {
    pub bytes: Vec<u8>,
    /// Required alignment of the thread pointer.
    pub align: u64,
}

impl ThreadTlsImage {
    /// Bytes mapped per thread: the image rounded up to whole pages, and at least one page.
    pub fn mapped_size(&self) -> u64 {
        ((self.bytes.len() as u64).max(1) + 0xFFF) & !0xFFF
    }
}

pub struct KThread {
    pub handle: u32,
    pub thread_id: u64,
    pub entry_pc: u64,
    pub arg_x0: u64,
    pub stack_top: u64,
    pub priority: i32,
    pub core_id: u32,
    pub state: ThreadState,
    pub cpu_context: Cpu,
    /// This thread's static TLS area (its `TPIDR_EL0`), if the process uses static TLS.
    pub tls_area: Option<u64>,
}

/// Horizon OS 8-Core Cortex-A78C Thread Scheduler.
///
/// Manages guest `KThread` objects created via `svcCreateThread` (`0x08`),
/// started via `svcStartThread` (`0x09`), and terminated via `svcExitThread` (`0x0A`).
pub struct ThreadScheduler {
    next_handle: u32,
    next_thread_id: u64,
    pub threads: HashMap<u32, KThread>,
    pub total_threads_spawned: usize,
    pub total_threads_completed: usize,
    /// Static TLS image copied into every new thread's TLS area. `None` when no module uses TLS.
    pub tls_image: Option<ThreadTlsImage>,
    next_tls_area: u64,
}

impl Default for ThreadScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadScheduler {
    pub fn new() -> Self {
        Self {
            next_handle: 0x2000,
            next_thread_id: 2, // Thread ID 1 is the main application thread
            threads: HashMap::new(),
            total_threads_spawned: 0,
            total_threads_completed: 0,
            tls_image: None,
            next_tls_area: THREAD_TLS_POOL_BASE,
        }
    }

    /// Creates a new `KThread` object (`svcCreateThread`).
    pub fn create_thread(
        &mut self,
        entry_pc: u64,
        arg_x0: u64,
        stack_top: u64,
        priority: i32,
        core_id: u32,
    ) -> u32 {
        let handle = self.next_handle;
        self.next_handle += 1;
        let thread_id = self.next_thread_id;
        self.next_thread_id += 1;

        let mut cpu_context = Cpu::new();
        cpu_context.regs.pc = entry_pc;
        cpu_context.regs.sp = stack_top;
        cpu_context.regs.write_x(0, arg_x0);

        let kthread = KThread {
            handle,
            thread_id,
            entry_pc,
            arg_x0,
            stack_top,
            priority,
            core_id: core_id % 8, // 8 Cortex-A78C cores (0..7)
            state: ThreadState::Created,
            cpu_context,
            tls_area: None,
        };

        self.threads.insert(handle, kthread);
        self.total_threads_spawned += 1;
        handle
    }

    /// Gives a created thread its own static TLS area: maps it from the TLS pool, copies `tls_image`
    /// into it, and points the thread's `TPIDR_EL0` at it. Returns the area, or `None` when the process
    /// has no static TLS, the handle is unknown, or the pool is exhausted.
    pub fn setup_thread_tls(&mut self, handle: u32, mmu: &mut Memory) -> Option<u64> {
        let image = self.tls_image.as_ref()?;
        let thread = self.threads.get_mut(&handle)?;
        let align = image.align.max(0x1000);
        let area = (self.next_tls_area + align - 1) & !(align - 1);
        let size = image.mapped_size();
        if area + size > THREAD_TLS_POOL_BASE + THREAD_TLS_POOL_SIZE {
            return None;
        }
        mmu.map_range(area, size as usize, Permission::READ_WRITE);
        mmu.write_bytes(area, &vec![0u8; size as usize]).ok()?;
        mmu.write_bytes(area, &image.bytes).ok()?;
        self.next_tls_area = area + size;
        thread.cpu_context.regs.tpidr_el0 = area;
        thread.tls_area = Some(area);
        Some(area)
    }

    /// Starts a created `KThread` and executes its timeslice against shared guest `Memory` (`svcStartThread`).
    pub fn start_and_run_thread(&mut self, handle: u32, mmu: &mut Memory, max_cycles: usize) -> bool {
        let Some(thread) = self.threads.get_mut(&handle) else {
            return false;
        };
        thread.state = ThreadState::Running;

        for _ in 0..max_cycles {
            match thread.cpu_context.step(mmu) {
                Ok(StepResult::Svc(0x0A)) | Ok(StepResult::Svc(0x07)) => {
                    // 0x0A = svcExitThread, 0x07 = svcExitProcess
                    thread.state = ThreadState::Exited;
                    self.total_threads_completed += 1;
                    return true;
                }
                Ok(_) => {}
                Err(_) => {
                    thread.state = ThreadState::Exited;
                    return false;
                }
            }
        }

        thread.state = ThreadState::Runnable;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multicore_worker_thread_execution() {
        let mut mmu = Memory::new();
        let mut sched = ThreadScheduler::new();

        // Map worker code at 0x80050000, worker stack at 0x70050000, shared data at 0x80060000
        let code_addr = 0x80050000u64;
        let stack_top = 0x70051000u64;
        let shared_addr = 0x80060000u64;
        mmu.map_range(code_addr, 4096, Permission::READ_WRITE_EXECUTE);
        mmu.map_range(0x70050000, 4096, Permission::READ_WRITE);
        mmu.map_range(shared_addr, 4096, Permission::READ_WRITE);

        // Worker thread takes shared_addr in X0, writes 0xCAFE_BABE_2026_0002 to [X0], and calls SVC #0x0A (svcExitThread)
        let worker_code: [u32; 4] = [
            0xD2800041, // MOVZ X1, #2
            0xF9000001, // STR X1, [X0]
            0xD4000141, // SVC #0x0A (svcExitThread)
            0xD503201F, // NOP
        ];
        for (i, &insn) in worker_code.iter().enumerate() {
            mmu.write_u32(code_addr + (i * 4) as u64, insn).unwrap();
        }

        let handle = sched.create_thread(code_addr, shared_addr, stack_top, 44, 3);
        assert!(sched.start_and_run_thread(handle, &mut mmu, 100));
        assert_eq!(sched.total_threads_completed, 1);
        assert_eq!(mmu.read_u64(shared_addr).unwrap(), 2);
    }

    #[test]
    fn test_each_created_thread_gets_initialised_tls() {
        use crate::kernel::HorizonKernel;

        let mut mmu = Memory::new();
        let mut kernel = HorizonKernel::new();
        let code_addr = 0x80050000u64;
        let shared_a = 0x80060000u64;
        let shared_b = 0x80061000u64;
        mmu.map_range(code_addr, 4096, Permission::READ_WRITE_EXECUTE);
        mmu.map_range(0x70050000, 0x2000, Permission::READ_WRITE);
        mmu.map_range(shared_a, 0x2000, Permission::READ_WRITE);

        // TLS image: 16-byte TCB, then a block whose first word (.tdata) is 0xC0FFEE.
        let mut bytes = vec![0u8; 0x30];
        bytes[16..24].copy_from_slice(&0xC0FFEEu64.to_le_bytes());
        kernel.scheduler.tls_image = Some(ThreadTlsImage { bytes, align: 16 });

        // Reads TPIDR_EL0 and the TLS word, bumps the word, then writes and reads back TPIDR_EL0.
        let worker_code: [u32; 11] = [
            0xD53BD041, // MRS  X1, TPIDR_EL0
            0xF9400822, // LDR  X2, [X1, #16]
            0xF9000002, // STR  X2, [X0]
            0xF9000401, // STR  X1, [X0, #8]
            0x91000442, // ADD  X2, X2, #1
            0xF9000822, // STR  X2, [X1, #16]
            0xD2824683, // MOVZ X3, #0x1234
            0xD51BD043, // MSR  TPIDR_EL0, X3
            0xD53BD044, // MRS  X4, TPIDR_EL0
            0xF9000804, // STR  X4, [X0, #16]
            0xD4000141, // SVC  #0x0A (svcExitThread)
        ];
        for (i, &insn) in worker_code.iter().enumerate() {
            mmu.write_u32(code_addr + (i * 4) as u64, insn).unwrap();
        }

        // Create both threads through svcCreateThread so the SVC path sets up their TLS.
        let mut handles = Vec::new();
        for (arg, stack) in [(shared_a, 0x70051000u64), (shared_b, 0x70052000u64)] {
            let mut cpu = Cpu::new();
            cpu.regs.write_x(1, code_addr);
            cpu.regs.write_x(2, arg);
            cpu.regs.write_x(3, stack);
            cpu.regs.write_x(4, 44);
            kernel.dispatch_svc(0x08, &mut cpu, &mut mmu);
            assert_eq!(cpu.regs.read_x(0), 0);
            handles.push(cpu.regs.read_x(1) as u32);
        }
        let tls_a = kernel.scheduler.threads[&handles[0]].tls_area.unwrap();
        let tls_b = kernel.scheduler.threads[&handles[1]].tls_area.unwrap();
        assert_ne!(tls_a, tls_b);
        assert_eq!(kernel.scheduler.threads[&handles[0]].cpu_context.regs.tpidr_el0, tls_a);

        for &h in &handles {
            assert!(kernel.scheduler.start_and_run_thread(h, &mut mmu, 100));
        }
        for (shared, tls) in [(shared_a, tls_a), (shared_b, tls_b)] {
            assert_eq!(mmu.read_u64(shared).unwrap(), 0xC0FFEE); // each thread saw a fresh .tdata copy
            assert_eq!(mmu.read_u64(shared + 8).unwrap(), tls);
            assert_eq!(mmu.read_u64(shared + 16).unwrap(), 0x1234);
            assert_eq!(mmu.read_u64(tls + 16).unwrap(), 0xC0FFEF);
        }
    }

    #[test]
    fn test_no_tls_image_leaves_tpidr_untouched() {
        let mut mmu = Memory::new();
        let mut sched = ThreadScheduler::new();
        let handle = sched.create_thread(0x8000_0000, 0, 0x7000_1000, 44, 0);
        assert_eq!(sched.setup_thread_tls(handle, &mut mmu), None);
        assert_eq!(sched.threads[&handle].cpu_context.regs.tpidr_el0, 0);
    }
}
