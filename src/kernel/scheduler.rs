#![allow(dead_code)]

use crate::cpu::{Cpu, CpuRegisters, StepResult};
use crate::mmu::{Memory, Permission};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Created,
    Runnable,
    Running,
    /// Waiting; `KThread::wait` says for what.
    Blocked,
    Exited,
}

/// What a blocked thread waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitReason {
    /// svcArbitrateLock: ownership of the mutex word at `addr`, then the word becomes `tag`.
    Mutex { addr: u64, tag: u32 },
    /// svcWaitProcessWideKeyAtomic: a signal on `cvar`, then the mutex at `mutex` (as `tag`).
    CondVar { cvar: u64, mutex: u64, tag: u32, timed: bool },
    /// svcWaitSynchronization: any of `handles` becoming signaled.
    Objects { handles: Vec<u32>, timed: bool },
}

/// Key of the process's first thread, which exists before any svcCreateThread and has no handle there.
pub const MAIN_THREAD_KEY: u32 = 0;
/// Page holding `svc #0x0A`; new threads return into it, so a thread that returns from its entry exits.
pub const THREAD_EXIT_STUB: u64 = 0x7F_FBFF_F000;
/// Pool of per-thread `TPIDRRO_EL0` pages (the Horizon thread-local region: IPC buffer and, for libnx,
/// `ThreadVars` at +0x1E0). One zeroed page per thread.
pub const THREAD_TLR_POOL_BASE: u64 = 0x7F_FB00_0000;
pub const THREAD_TLR_POOL_PAGES: u64 = 0x800;
/// Horizon results.
pub const RESULT_TIMED_OUT: u64 = 0xEA01;
pub const RESULT_INVALID_HANDLE: u64 = 0xE401;
pub const RESULT_INVALID_STATE: u64 = 0xFA01;
/// Mutex word flag: other threads wait for this mutex.
pub const MUTEX_HAS_LISTENERS: u32 = 0x4000_0000;

/// Base of the pool that per-thread static TLS areas are carved from.
pub const THREAD_TLS_POOL_BASE: u64 = 0x7F_FC00_0000;
/// Size of the per-thread TLS pool.
pub const THREAD_TLS_POOL_SIZE: u64 = 0x100_0000;
/// Sentinel return address loaded into the Link Register (X30) of created threads.
/// When a thread function terminates via standard `RET` instead of calling `svcExitThread`,
/// PC reaches this sentinel and the scheduler transitions the thread to `ThreadState::Exited`.
pub const THREAD_RETURN_SENTINEL: u64 = 0xFFFF_0000_0000_E717;

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
    /// What the thread waits for while `Blocked`.
    pub wait: Option<WaitReason>,
    /// Order in which threads started waiting; the earliest waiter is served first.
    pub wait_seq: u64,
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
    /// Thread whose registers are live in the CPU.
    pub current: u32,
    /// Threads ready to run, in order.
    pub ready: VecDeque<u32>,
    next_tlr_page: u64,
    wait_counter: u64,
    /// Condvar waiters that timed out and still have to reacquire their mutex.
    pub(crate) pending_relock: Vec<u32>,
    /// Context switches performed, for diagnostics.
    pub switches: u64,
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
            current: MAIN_THREAD_KEY,
            ready: VecDeque::new(),
            next_tlr_page: 0,
            wait_counter: 0,
            pending_relock: Vec::new(),
            switches: 0,
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
        cpu_context.regs.write_x(30, THREAD_RETURN_SENTINEL);

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
            wait: None,
            wait_seq: 0,
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
            if thread.cpu_context.regs.pc == THREAD_RETURN_SENTINEL {
                thread.state = ThreadState::Exited;
                self.total_threads_completed += 1;
                return true;
            }
            match thread.cpu_context.step(mmu) {
                Ok(StepResult::Svc(0x0A)) | Ok(StepResult::Svc(0x07)) => {
                    // 0x0A = svcExitThread, 0x07 = svcExitProcess
                    thread.state = ThreadState::Exited;
                    self.total_threads_completed += 1;
                    return true;
                }
                Ok(_) => {
                    if thread.cpu_context.regs.pc == THREAD_RETURN_SENTINEL {
                        thread.state = ThreadState::Exited;
                        self.total_threads_completed += 1;
                        return true;
                    }
                }
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

/// Cooperative scheduling of guest threads on the one CPU. Switches happen only inside SVCs: the
/// kernel saves the live registers into the current thread and loads the next thread's.
impl ThreadScheduler {
    /// The main thread's entry, created the first time the scheduler needs to save it.
    fn ensure_main(&mut self) {
        self.threads.entry(MAIN_THREAD_KEY).or_insert_with(|| KThread {
            handle: MAIN_THREAD_KEY,
            thread_id: 1,
            entry_pc: 0,
            arg_x0: 0,
            stack_top: 0,
            priority: 44,
            core_id: 0,
            state: ThreadState::Running,
            cpu_context: Cpu::new(),
            tls_area: None,
            wait: None,
            wait_seq: 0,
        });
    }

    /// Gives a created thread its own zeroed thread-local region and points its `TPIDRRO_EL0` at it, and
    /// makes it return into the exit stub. Returns the region.
    pub fn prepare_thread(&mut self, handle: u32, mmu: &mut Memory) -> Option<u64> {
        if !mmu.is_mapped(THREAD_EXIT_STUB) {
            mmu.map_range(THREAD_EXIT_STUB, 0x1000, Permission::READ_WRITE_EXECUTE);
            mmu.write_u32(THREAD_EXIT_STUB, 0xD400_0141).ok()?; // svc #0x0A
        }
        if self.next_tlr_page >= THREAD_TLR_POOL_PAGES {
            return None;
        }
        let page = THREAD_TLR_POOL_BASE + self.next_tlr_page * 0x1000;
        self.next_tlr_page += 1;
        mmu.map_range(page, 0x1000, Permission::READ_WRITE);
        let thread = self.threads.get_mut(&handle)?;
        thread.cpu_context.regs.tpidrro_el0 = page;
        thread.cpu_context.regs.write_x(30, THREAD_EXIT_STUB);
        Some(page)
    }

    /// Thread ID of the running thread.
    pub fn current_thread_id(&self) -> u64 {
        self.threads.get(&self.current).map_or(1, |t| t.thread_id)
    }

    fn switch_to(&mut self, cpu: &mut Cpu, next: u32) {
        self.ensure_main();
        if let Some(cur) = self.threads.get_mut(&self.current) {
            cur.cpu_context.regs = cpu.regs.clone();
        }
        let thread = self.threads.get_mut(&next).expect("switching to a known thread");
        thread.state = ThreadState::Running;
        cpu.regs = thread.cpu_context.regs.clone();
        self.current = next;
        self.switches += 1;
    }

    /// svcStartThread: the new thread runs now; the caller resumes as soon as the new thread waits or exits.
    /// `caller_result` is what the caller sees in X0 when it resumes.
    pub fn start_thread(&mut self, cpu: &mut Cpu, handle: u32, caller_result: u64) -> Result<(), u64> {
        match self.threads.get(&handle).map(|t| t.state) {
            Some(ThreadState::Created) => {}
            Some(_) => return Err(RESULT_INVALID_STATE),
            None => return Err(RESULT_INVALID_HANDLE),
        }
        cpu.regs.write_x(0, caller_result);
        self.ensure_main();
        let cur = self.current;
        self.threads.get_mut(&cur).unwrap().state = ThreadState::Runnable;
        self.ready.push_front(cur);
        self.switch_to(cpu, handle);
        Ok(())
    }

    /// Puts the running thread to sleep on `reason` and switches to the next ready thread. When no thread
    /// is ready, a timed waiter is woken with a timeout; if nothing can run at all, returns `false`
    /// without blocking (a deadlock the caller reports).
    pub fn block_current(&mut self, cpu: &mut Cpu, reason: WaitReason) -> bool {
        self.ensure_main();
        let cur = self.current;
        self.wait_counter += 1;
        let seq = self.wait_counter;
        {
            let t = self.threads.get_mut(&cur).unwrap();
            t.state = ThreadState::Blocked;
            t.wait = Some(reason);
            t.wait_seq = seq;
        }
        match self.next_ready_or_timeout(cpu) {
            Some(next) if next == cur => {
                // The only thing that could run was this thread's own timeout: carry on with its result.
                let t = self.threads.get_mut(&cur).unwrap();
                t.state = ThreadState::Running;
                cpu.regs = t.cpu_context.regs.clone();
                true
            }
            Some(next) => {
                self.switch_to(cpu, next);
                true
            }
            None => {
                let t = self.threads.get_mut(&cur).unwrap();
                t.state = ThreadState::Running;
                t.wait = None;
                false
            }
        }
    }

    /// The next thread to run. With none ready, the earliest timed waiter times out. The current thread's
    /// registers are saved first so a timeout can be written into them.
    fn next_ready_or_timeout(&mut self, cpu: &mut Cpu) -> Option<u32> {
        if let Some(next) = self.ready.pop_front() {
            return Some(next);
        }
        let cur = self.current;
        self.threads.get_mut(&cur).unwrap().cpu_context.regs = cpu.regs.clone();
        let timed = self
            .threads
            .values()
            .filter(|t| t.state == ThreadState::Blocked)
            .filter(|t| matches!(t.wait, Some(WaitReason::Objects { timed: true, .. }) | Some(WaitReason::CondVar { timed: true, .. })))
            .min_by_key(|t| t.wait_seq)
            .map(|t| t.handle)?;
        self.time_out(timed);
        self.ready.pop_front()
    }

    /// A timed wait ends with a timeout. A condvar waiter then still has to get its mutex back.
    fn time_out(&mut self, key: u32) {
        let reason = self.threads.get(&key).and_then(|t| t.wait.clone());
        match reason {
            Some(WaitReason::CondVar { mutex, tag, .. }) => {
                let t = self.threads.get_mut(&key).unwrap();
                t.cpu_context.regs.write_x(0, RESULT_TIMED_OUT);
                // Marked as a mutex waiter; `relock_after_wake` is applied by the kernel, which has memory.
                t.wait = Some(WaitReason::Mutex { addr: mutex, tag });
                t.state = ThreadState::Blocked;
                self.pending_relock.push(key);
            }
            _ => self.wake(key, RESULT_TIMED_OUT, None),
        }
    }

    /// Makes a blocked thread ready with `result` in X0 (and `x1` in X1 when given).
    pub fn wake(&mut self, key: u32, result: u64, x1: Option<u64>) {
        if let Some(t) = self.threads.get_mut(&key) {
            t.cpu_context.regs.write_x(0, result);
            if let Some(v) = x1 {
                t.cpu_context.regs.write_x(1, v);
            }
            t.state = ThreadState::Runnable;
            t.wait = None;
            self.ready.push_back(key);
        }
    }

    /// svcSleepThread / yield: the running thread goes to the back of the ready queue.
    pub fn yield_current(&mut self, cpu: &mut Cpu) {
        if let Some(next) = self.ready.pop_front() {
            self.ensure_main();
            let cur = self.current;
            self.threads.get_mut(&cur).unwrap().state = ThreadState::Runnable;
            self.ready.push_back(cur);
            self.switch_to(cpu, next);
        }
    }

    /// svcExitThread. Returns the threads that were waiting for this one to exit (to be woken by the
    /// caller), or `None` when no other thread can run and the process should end.
    pub fn exit_current(&mut self, cpu: &mut Cpu) -> Option<()> {
        self.ensure_main();
        let cur = self.current;
        let t = self.threads.get_mut(&cur).unwrap();
        t.state = ThreadState::Exited;
        t.cpu_context.regs = cpu.regs.clone();
        self.total_threads_completed += 1;
        self.wake_object_waiters(cur);
        let next = self.next_ready_or_timeout(cpu)?;
        let thread = self.threads.get_mut(&next).unwrap();
        thread.state = ThreadState::Running;
        cpu.regs = thread.cpu_context.regs.clone();
        self.current = next;
        self.switches += 1;
        Some(())
    }

    /// Wakes threads in svcWaitSynchronization on `handle`, with the index of `handle` in X1.
    pub fn wake_object_waiters(&mut self, handle: u32) {
        let mut woken: Vec<(u64, u32, usize)> = self
            .threads
            .values()
            .filter(|t| t.state == ThreadState::Blocked)
            .filter_map(|t| match &t.wait {
                Some(WaitReason::Objects { handles, .. }) => handles.iter().position(|&h| h == handle).map(|i| (t.wait_seq, t.handle, i)),
                _ => None,
            })
            .collect();
        woken.sort();
        for (_, key, index) in woken {
            self.wake(key, 0, Some(index as u64));
        }
    }

    /// Whether `handle` names a thread that has exited (signaled for svcWaitSynchronization).
    pub fn thread_signaled(&self, handle: u32) -> Option<bool> {
        self.threads.get(&handle).filter(|_| handle != MAIN_THREAD_KEY).map(|t| t.state == ThreadState::Exited)
    }

    /// Blocked threads waiting for `pred`, earliest first.
    pub fn waiters(&self, pred: impl Fn(&WaitReason) -> bool) -> Vec<u32> {
        let mut v: Vec<(u64, u32)> = self
            .threads
            .values()
            .filter(|t| t.state == ThreadState::Blocked && t.wait.as_ref().is_some_and(&pred))
            .map(|t| (t.wait_seq, t.handle))
            .collect();
        v.sort();
        v.into_iter().map(|(_, h)| h).collect()
    }

    /// Hands the mutex word at `addr` to its earliest waiter (waking it), or clears it.
    pub fn release_mutex(&mut self, mmu: &mut Memory, addr: u64) {
        let waiters = self.waiters(|w| matches!(w, WaitReason::Mutex { addr: a, .. } if *a == addr));
        match waiters.first() {
            Some(&next) => {
                let tag = match self.threads[&next].wait {
                    Some(WaitReason::Mutex { tag, .. }) => tag,
                    _ => unreachable!(),
                };
                let more = if waiters.len() > 1 { MUTEX_HAS_LISTENERS } else { 0 };
                let _ = mmu.write_u32(addr, tag | more);
                // A condvar waiter that timed out keeps its timeout result.
                let result = self.threads[&next].cpu_context.regs.read_x(0);
                let keep = self.pending_relock.contains(&next);
                self.pending_relock.retain(|&k| k != next);
                self.wake(next, if keep { result } else { 0 }, None);
            }
            None => {
                let _ = mmu.write_u32(addr, 0);
            }
        }
    }

    /// A thread leaving a condvar wait takes its mutex back: at once if free, else as a mutex waiter.
    pub fn relock_after_wake(&mut self, mmu: &mut Memory, key: u32, mutex: u64, tag: u32, result: u64) {
        let word = mmu.read_u32(mutex).unwrap_or(0);
        if word == 0 {
            let _ = mmu.write_u32(mutex, tag);
            self.wake(key, result, None);
        } else {
            let _ = mmu.write_u32(mutex, word | MUTEX_HAS_LISTENERS);
            let t = self.threads.get_mut(&key).unwrap();
            t.cpu_context.regs.write_x(0, result);
            t.wait = Some(WaitReason::Mutex { addr: mutex, tag });
            if result != 0 {
                self.pending_relock.push(key);
            }
        }
    }

    /// Applies the mutex step for condvar waiters that timed out while the scheduler was picking a thread.
    pub fn finish_timeouts(&mut self, mmu: &mut Memory) {
        let pending: Vec<u32> = self.pending_relock.clone();
        for key in pending {
            let Some(t) = self.threads.get(&key) else { continue };
            if t.state != ThreadState::Blocked {
                continue;
            }
            if let Some(WaitReason::Mutex { addr, tag }) = t.wait.clone() {
                if mmu.read_u32(addr).unwrap_or(0) == 0 {
                    self.pending_relock.retain(|&k| k != key);
                    let _ = mmu.write_u32(addr, tag);
                    self.wake(key, RESULT_TIMED_OUT, None);
                } else {
                    let word = mmu.read_u32(addr).unwrap_or(0);
                    let _ = mmu.write_u32(addr, word | MUTEX_HAS_LISTENERS);
                }
            }
        }
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

    #[test]
    fn test_thread_exit_via_ret_sentinel() {
        let mut mmu = Memory::new();
        let mut sched = ThreadScheduler::new();
        let code_addr = 0x80010000u64;
        let stack_top = 0x70002000u64;
        let shared_addr = 0x80020000u64;
        mmu.map_range(code_addr, 4096, Permission::READ_WRITE_EXECUTE);
        mmu.map_range(0x70000000, 0x4000, Permission::READ_WRITE);
        mmu.map_range(shared_addr, 4096, Permission::READ_WRITE);

        // Worker writes 0x4242 to [X0] and then simply returns via `RET` (X30 sentinel).
        let worker_code: [u32; 3] = [
            0xD2884841, // MOVZ X1, #0x4242
            0xF9000001, // STR  X1, [X0]
            0xD65F03C0, // RET  (branches to X30 = THREAD_RETURN_SENTINEL)
        ];
        for (i, &insn) in worker_code.iter().enumerate() {
            mmu.write_u32(code_addr + (i * 4) as u64, insn).unwrap();
        }

        let handle = sched.create_thread(code_addr, shared_addr, stack_top, 44, 1);
        assert_eq!(sched.threads[&handle].cpu_context.regs.read_x(30), THREAD_RETURN_SENTINEL);
        assert!(sched.start_and_run_thread(handle, &mut mmu, 100));
        assert_eq!(sched.threads[&handle].state, ThreadState::Exited);
        assert_eq!(sched.total_threads_completed, 1);
        assert_eq!(mmu.read_u64(shared_addr).unwrap(), 0x4242);
    }
}
