#![allow(dead_code)]

use super::ipc::{IpcManager, ServiceType};
use super::scheduler::{ThreadScheduler, WaitReason, MUTEX_HAS_LISTENERS, RESULT_INVALID_HANDLE, RESULT_TIMED_OUT};
use super::sync::SyncManager;
use crate::audio::AudioOutServer;
use crate::cpu::Cpu;
use crate::fs::VirtualFileSystem;
use crate::gpu::nvgpu::NvgpuChannel;
use crate::mmu::{Memory, Permission};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvcResult {
    /// Kernel handled the SVC successfully; continue execution
    Success,
    /// Process requested termination via svcExitProcess
    ExitProcess,
    /// Frame presentation requested (vsync / buffer flip)
    PresentFrame,
    /// Hit a panic / break via svcBreak
    Break { reason: u64, info1: u64, info2: u64 },
    /// Unknown or unhandled SVC ID
    Unhandled(u16),
}

/// ounce-core's own research SVCs (GPU, RomFS, audio, save data, NVN and SDK fast paths).
/// Horizon's SVC numbers stop below `0x80`, so this range cannot collide with a real Horizon call
/// that homebrew makes (for example `0x29` is `svcGetInfo`, which libnx `crt0` calls).
pub mod ext {
    pub const PRESENT_FRAME: u16 = 0x100;
    pub const SUBMIT_GPU_PUSHBUFFER: u16 = 0x101;
    pub const READ_ROMFS_FILE: u16 = 0x102;
    pub const APPEND_AUDIO_BUFFER: u16 = 0x103;
    pub const WRITE_SAVE_DATA_FILE: u16 = 0x104;
    pub const NVN_SUBMIT_COMMAND_BUFFER: u16 = 0x105;
    pub const NNSDK_DISPATCH: u16 = 0x106;

    /// Encodes `svc #imm`.
    pub const fn svc(imm: u16) -> u32 {
        0xD400_0001 | (imm as u32) << 5
    }
}

/// Horizon `svcGetInfo` result for an info type it does not know (`InvalidEnumValue`).
pub const RESULT_INVALID_ENUM_VALUE: u64 = 0xF001;

/// Horizon OS Microkernel Compatibility Layer.
pub struct HorizonKernel {
    pub heap_base: u64,
    pub heap_size: usize,
    pub start_time: Instant,
    pub ipc: IpcManager,
    pub nvgpu: NvgpuChannel,
    pub vfs: VirtualFileSystem,
    pub audio: AudioOutServer,
    pub audren: crate::audio::AudioRenderer,
    pub scheduler: ThreadScheduler,
    pub sync: SyncManager,
    pub nvn: crate::gpu::nvn::NvnDriver,
    pub nnsdk: super::nnsdk::NnSdkRuntime,
    pub reply_receive_step: u32,
}

impl Default for HorizonKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl HorizonKernel {
    pub fn new() -> Self {
        Self {
            heap_base: 0x100000000, // 4GB mark
            heap_size: 0,
            start_time: Instant::now(),
            ipc: IpcManager::new(),
            nvgpu: NvgpuChannel::new(),
            vfs: VirtualFileSystem::new(),
            audio: AudioOutServer::new(),
            audren: crate::audio::AudioRenderer::new(Default::default()),
            scheduler: ThreadScheduler::new(),
            sync: SyncManager::new(),
            nvn: crate::gpu::nvn::NvnDriver::new(),
            nnsdk: super::nnsdk::NnSdkRuntime::new(),
            reply_receive_step: 0,
        }
    }

    /// System tick at 19.2 MHz since the kernel was created (at least one millisecond's worth).
    pub fn system_tick(&self) -> u64 {
        let elapsed = self.start_time.elapsed().as_nanos();
        (((elapsed * 19_200_000) / 1_000_000_000) as u64).max(19_200)
    }

    /// `svcGetInfo` values. `None` for info types this model does not provide.
    /// The regions do not overlap: ASLR covers the 39-bit space that holds code (0x8000_0000), the heap,
    /// alias and stack regions sit inside it, and the stack region holds the main thread's stack.
    pub fn info(&self, info_type: u32, sub_type: u64) -> Option<u64> {
        Some(match info_type {
            0 => 0xFF,                     // CoreMask: 8x ARM Cortex-A78C cores (bits 0..7)
            1 => 0xFFF,                    // PriorityMask
            2 => 0x0000_0002_0000_0000,    // AliasRegionAddress
            3 => 0x0000_0010_0000_0000,    // AliasRegionSize (64 GB)
            4 => self.heap_base,           // HeapRegionAddress (0x1_0000_0000)
            5 => 0x0000_0001_0000_0000,    // HeapRegionSize (4 GB)
            6 => 12 * 1024 * 1024 * 1024,  // TotalMemorySize: 12 GB LPDDR5
            7 => (self.heap_size as u64) + 0x10_0000, // UsedMemorySize
            8 => 0,                        // DebuggerAttached
            11 if sub_type < 4 => 0x9E37_79B9_7F4A_7C15u64.rotate_left(sub_type as u32 * 16) ^ self.system_tick(), // RandomEntropy
            12 => 0x0000_0000_0800_0000,   // AslrRegionAddress
            13 => 0x0000_007F_F800_0000,   // AslrRegionSize
            14 => 0x0000_007F_0000_0000,   // StackRegionAddress
            15 => 0x0000_0000_FFFF_F000,   // StackRegionSize (ends at the top of the main stack)
            18 => 0x0400_0000_0000_0000,   // ProgramId (synthetic value)
            20 | 21 => 0,                  // SystemResourceSizeTotal / Used
            _ => return None,
        })
    }

    /// Handles for svcWaitSynchronization when it is a real wait: 1..=64 handles, all of them threads
    /// or events this kernel knows.
    fn wait_handles(&self, cpu: &Cpu, mmu: &Memory) -> Option<Vec<u32>> {
        let count = cpu.regs.read_x(2);
        if !(1..=64).contains(&count) {
            return None;
        }
        let ptr = cpu.regs.read_x(1);
        let handles: Vec<u32> = (0..count).map(|i| mmu.read_u32(ptr + i * 4)).collect::<Result<_, _>>().ok()?;
        handles
            .iter()
            .all(|h| {
                self.scheduler.thread_signaled(*h).is_some() || self.sync.events.contains_key(h) || self.ipc.is_audio_renderer_event(*h)
            })
            .then_some(handles)
    }

    /// Renders one audio renderer frame and queues it for the host with the other audio output.
    pub fn render_audio_frame(&mut self, mmu: &Memory) {
        if let Some(r) = self.ipc.audio_renderer.as_mut() {
            r.render_frame(mmu);
            let pcm = r.drain();
            self.audren.output_ring_buffer.extend_from_slice(&pcm);
        }
    }

    /// Every thread waits and none can be woken: stop the guest with a break that says so.
    fn deadlock(&mut self, cpu: &mut Cpu, svc: &str) -> SvcResult {
        eprintln!("[Scheduler] deadlock in {svc}: every thread is waiting and none can wake the others");
        cpu.regs.write_x(0, RESULT_INVALID_HANDLE);
        SvcResult::Break { reason: 0xDEAD_10C4, info1: self.scheduler.current as u64, info2: 0 }
    }

    /// Dispatches a Horizon OS Supervisor Call (SVC).
    pub fn dispatch_svc(&mut self, svc_id: u16, cpu: &mut Cpu, mmu: &mut Memory) -> SvcResult {
        // Guests poll HID shared memory between SVCs, so publish the latest input before each one.
        self.ipc.hid.sync(mmu);
        self.scheduler.finish_timeouts(mmu);
        match svc_id {
            // 0x01: svcSetHeapSize(&out_addr, size)
            0x01 => {
                let requested_size = cpu.regs.read_x(1) as usize;
                let heap_addr = self.heap_base;

                if requested_size > 0 {
                    mmu.map_range(heap_addr, requested_size, Permission::READ_WRITE);
                }
                self.heap_size = requested_size;

                // X0 = 0 (ResultSuccess), X1 = Heap base virtual address
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, heap_addr);
                SvcResult::Success
            }

            // 0x02: svcSetMemoryPermission(addr, size, perm) / 0x03: svcSetMemoryAttribute(addr, size, mask, attr)
            // Permissions and attributes are not modelled; accept the request.
            0x02 | 0x03 => {
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x04: svcMapMemory(dst, src, size) / 0x05: svcUnmapMemory(dst, src, size).
            // Horizon makes the source inaccessible while it is mapped at dst, so the pages move (and back).
            0x04 | 0x05 => {
                let (dst, src, size) = (cpu.regs.read_x(0), cpu.regs.read_x(1), cpu.regs.read_x(2) as usize);
                if svc_id == 0x04 {
                    mmu.move_range(src, dst, size);
                } else {
                    mmu.move_range(dst, src, size);
                }
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x06: svcQueryMemory(out_mem_info, &out_page_info, query_addr)
            0x06 => {
                let mem_info_ptr = cpu.regs.read_x(0);
                let query_addr = cpu.regs.read_x(2);
                let aligned_base = query_addr & !0xFFF;

                // MemoryInfo for the whole run of mapped (Normal/RW) or unmapped (Free, no access) pages
                // around the address. libnx virtmem needs the full extent of a free run to place mappings.
                let (start, end, mapped) = mmu.region_at(aligned_base);
                let (state, perm) = if mapped { (0x03, 0x03) } else { (0x00, 0x00) };
                let _ = mmu.write_u64(mem_info_ptr, start);
                let _ = mmu.write_u64(mem_info_ptr + 8, end - start);
                let _ = mmu.write_u32(mem_info_ptr + 16, state);
                let _ = mmu.write_u32(mem_info_ptr + 20, 0);
                let _ = mmu.write_u32(mem_info_ptr + 24, perm);

                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, 0);
                SvcResult::Success
            }

            // 0x07: svcExitProcess()
            0x07 => SvcResult::ExitProcess,

            // 0x08: svcCreateThread(&out_handle, entry_pc, arg, stack_top, prio, processor_id)
            0x08 => {
                let entry_pc = cpu.regs.read_x(1);
                let arg = cpu.regs.read_x(2);
                let stack_top = cpu.regs.read_x(3);
                let prio = cpu.regs.read_w(4) as i32;
                let core_id = cpu.regs.read_w(5);

                let handle = self
                    .scheduler
                    .create_thread(entry_pc, arg, stack_top, prio, core_id);
                self.scheduler.setup_thread_tls(handle, mmu);
                self.scheduler.prepare_thread(handle, mmu);
                cpu.regs.write_x(0, 0); // ResultSuccess
                cpu.regs.write_x(1, handle as u64);
                SvcResult::Success
            }

            // 0x09: svcStartThread(thread_handle). The new thread runs first; the caller resumes when it
            // waits or exits.
            0x09 => {
                let handle = cpu.regs.read_w(0);
                if let Err(code) = self.scheduler.start_thread(cpu, handle, 0) {
                    cpu.regs.write_x(0, code);
                }
                SvcResult::Success
            }

            // 0x0A: svcExitThread(). The process ends when no other thread can run.
            0x0A => match self.scheduler.exit_current(cpu) {
                Some(()) => SvcResult::Success,
                None => SvcResult::ExitProcess,
            },

            // 0x0B: svcSleepThread(nanoseconds). Time is not modelled: the thread yields to the others.
            0x0B => {
                cpu.regs.write_x(0, 0);
                self.scheduler.yield_current(cpu);
                SvcResult::Success
            }

            // 0x0C: svcGetThreadPriority(&out_prio, thread_handle)
            0x0C => {
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, 16); // Default Horizon user thread priority
                SvcResult::Success
            }

            // 0x0D: svcSetThreadPriority(thread_handle, prio)
            0x0D => {
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x0E: svcGetThreadCoreMask(&out_core_id, &out_affinity_mask, thread_handle)
            0x0E => {
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, 0);    // Preferred core 0
                cpu.regs.write_x(2, 0xFF); // 8-core Switch 2 Cortex-A78C affinity mask
                SvcResult::Success
            }

            // 0x0F: svcSetThreadCoreMask(thread_handle, core_id, affinity_mask). Affinity is not modelled.
            0x0F => {
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x10: svcGetCurrentProcessorNumber()
            0x10 => {
                cpu.regs.write_x(0, 0); // Running on Core 0 of 8x Cortex-A78C
                SvcResult::Success
            }

            // 0x11: svcSignalEvent(event_handle)
            0x11 => {
                let handle = cpu.regs.read_w(0);
                let ok = self.sync.signal_event(handle);
                cpu.regs.write_x(0, if ok { 0 } else { 0xE401 });
                if ok {
                    self.scheduler.wake_object_waiters(handle);
                }
                SvcResult::Success
            }

            // 0x12: svcClearEvent(event_handle)
            0x12 => {
                let handle = cpu.regs.read_w(0);
                let ok = self.sync.clear_event(handle);
                cpu.regs.write_x(0, if ok { 0 } else { 0xE401 });
                SvcResult::Success
            }

            // 0x13: svcMapSharedMemory(handle, addr, size, perm)
            0x13 => {
                let handle = cpu.regs.read_w(0);
                let addr = cpu.regs.read_x(1);
                let size = cpu.regs.read_x(2) as usize;
                if addr != 0 && size > 0 {
                    mmu.map_range(addr, size, Permission::READ_WRITE);
                    self.ipc.shared_memory_mapped(handle, addr, mmu);
                }
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x14: svcUnmapSharedMemory(handle, addr, size)
            0x14 => {
                let (addr, size) = (cpu.regs.read_x(1), cpu.regs.read_x(2) as usize);
                mmu.unmap_range(addr, size);
                self.ipc.shared_memory_unmapped(addr);
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x15: svcCreateTransferMemory(&out_handle, addr, size, perm)
            0x15 => {
                let (addr, size) = (cpu.regs.read_x(1), cpu.regs.read_x(2));
                let handle = self.ipc.allocate_handle(ServiceType::TransferMemory { addr, size });
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, handle as u64);
                SvcResult::Success
            }

            // 0x16: svcCloseHandle(handle)
            0x16 => {
                let handle = cpu.regs.read_w(0);
                self.ipc.close_handle(handle);
                if self.scheduler.thread_signaled(handle) == Some(true) {
                    self.scheduler.threads.remove(&handle);
                }
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x17: svcResetSignal(handle): clear an event (the audio renderer event re-arms per frame).
            0x17 => {
                let handle = cpu.regs.read_w(0);
                let ok = self.sync.clear_event(handle) || self.ipc.is_audio_renderer_event(handle);
                cpu.regs.write_x(0, if ok { 0 } else { 0xE401 });
                SvcResult::Success
            }

            // 0x18: svcWaitSynchronization(&out_index, handles, count, timeout_ns) on thread and event handles.
            // Called without 1..=64 known handles it keeps meaning VSync for the demo programs.
            0x18 if self.wait_handles(cpu, mmu).is_some() => {
                let handles = self.wait_handles(cpu, mmu).unwrap();
                let timeout = cpu.regs.read_x(3) as i64;
                // Waiting for the audio renderer's frame event is where a 5 ms frame passes: render it.
                let audio_event = handles.iter().position(|&h| self.ipc.is_audio_renderer_event(h));
                if audio_event.is_some() {
                    self.render_audio_frame(mmu);
                }
                let signaled = audio_event.or_else(|| {
                    handles.iter().position(|&h| match self.scheduler.thread_signaled(h) {
                        Some(exited) => exited,
                        None => self.sync.events.get(&h).is_some_and(|e| e.signaled),
                    })
                });
                if let Some(i) = signaled {
                    if let Some(e) = self.sync.events.get_mut(&handles[i]) {
                        if e.auto_clear {
                            e.signaled = false;
                        }
                    }
                    cpu.regs.write_x(0, 0);
                    cpu.regs.write_x(1, i as u64);
                } else if timeout == 0 {
                    cpu.regs.write_x(0, RESULT_TIMED_OUT);
                } else if !self.scheduler.block_current(cpu, WaitReason::Objects { handles, timed: timeout > 0 }) {
                    if timeout > 0 {
                        cpu.regs.write_x(0, RESULT_TIMED_OUT);
                    } else {
                        return self.deadlock(cpu, "svcWaitSynchronization");
                    }
                }
                SvcResult::Success
            }

            // 0x18 (VSync, see above) / ext::PRESENT_FRAME: direct buffer flip request
            0x18 | ext::PRESENT_FRAME => {
                cpu.regs.write_x(0, 0); // ResultSuccess
                SvcResult::PresentFrame
            }

            // 0x1A: svcArbitrateLock(owner_tag, mutex_addr, requester_tag). Waits for the owner to hand the
            // mutex over, unless the word no longer says "owner, with waiters" (then the caller retries).
            0x1A => {
                let owner = cpu.regs.read_w(0);
                let addr = cpu.regs.read_x(1);
                let tag = cpu.regs.read_w(2);
                self.sync.total_locks_arbitrated += 1;
                let word = mmu.read_u32(addr).unwrap_or(0);
                cpu.regs.write_x(0, 0);
                if word == 0 {
                    // Free by the time the kernel looks: take it (callers that skipped the user-space fast path).
                    let _ = mmu.write_u32(addr, tag);
                } else if word == owner | MUTEX_HAS_LISTENERS
                    && !self.scheduler.block_current(cpu, WaitReason::Mutex { addr, tag })
                {
                    return self.deadlock(cpu, "svcArbitrateLock");
                }
                SvcResult::Success
            }

            // 0x1B: svcArbitrateUnlock(mutex_addr): hand the mutex to its earliest waiter, or free it.
            0x1B => {
                let addr = cpu.regs.read_x(0);
                self.sync.total_locks_arbitrated += 1;
                self.scheduler.release_mutex(mmu, addr);
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x1C: svcWaitProcessWideKeyAtomic(mutex_addr, condvar_addr, thread_tag, timeout_ns): release the
            // mutex and wait for a signal; the thread owns the mutex again when this returns.
            0x1C => {
                let mutex = cpu.regs.read_x(0);
                let cvar = cpu.regs.read_x(1);
                let tag = cpu.regs.read_w(2);
                let timeout = cpu.regs.read_x(3) as i64;
                self.scheduler.release_mutex(mmu, mutex);
                let _ = mmu.write_u32(cvar, 1);
                if timeout == 0 {
                    // Times out at once, then takes the mutex back like any other waiter.
                    let word = mmu.read_u32(mutex).unwrap_or(0);
                    cpu.regs.write_x(0, RESULT_TIMED_OUT);
                    if word == 0 {
                        let _ = mmu.write_u32(mutex, tag);
                    } else {
                        let _ = mmu.write_u32(mutex, word | MUTEX_HAS_LISTENERS);
                        let cur = self.scheduler.current;
                        self.scheduler.pending_relock.push(cur);
                        if !self.scheduler.block_current(cpu, WaitReason::Mutex { addr: mutex, tag }) {
                            return self.deadlock(cpu, "svcWaitProcessWideKeyAtomic");
                        }
                    }
                } else {
                    cpu.regs.write_x(0, 0);
                    let reason = WaitReason::CondVar { cvar, mutex, tag, timed: timeout > 0 };
                    if !self.scheduler.block_current(cpu, reason) {
                        if timeout > 0 {
                            cpu.regs.write_x(0, RESULT_TIMED_OUT);
                            let _ = mmu.write_u32(mutex, tag);
                        } else {
                            return self.deadlock(cpu, "svcWaitProcessWideKeyAtomic");
                        }
                    }
                }
                SvcResult::Success
            }

            // 0x1D: svcSignalProcessWideKey(condvar_addr, count): wake up to `count` waiters (all when
            // count <= 0); each then takes its mutex back, or waits for it.
            0x1D => {
                let cvar = cpu.regs.read_x(0);
                let count = cpu.regs.read_w(1) as i32;
                self.sync.total_condvar_signals += 1;
                let waiters = self.scheduler.waiters(|w| matches!(w, WaitReason::CondVar { cvar: c, .. } if *c == cvar));
                let n = if count <= 0 { waiters.len() } else { (count as usize).min(waiters.len()) };
                for &key in &waiters[..n] {
                    if let Some(WaitReason::CondVar { mutex, tag, .. }) = self.scheduler.threads[&key].wait.clone() {
                        self.scheduler.relock_after_wake(mmu, key, mutex, tag, 0);
                    }
                }
                let _ = mmu.write_u32(cvar, (waiters.len() > n) as u32);
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x1E: svcGetSystemTick() -> X0. The counter runs at 19.2 MHz.
            0x1E => {
                cpu.regs.write_x(0, self.system_tick());
                SvcResult::Success
            }

            // 0x29: svcGetInfo(&out_info, info_type, handle, info_sub_type) -> X0 = result, X1 = value
            0x29 => {
                let info_type = cpu.regs.read_w(1);
                let sub_type = cpu.regs.read_x(3);
                match self.info(info_type, sub_type) {
                    Some(value) => {
                        cpu.regs.write_x(0, 0);
                        cpu.regs.write_x(1, value);
                    }
                    None => {
                        cpu.regs.write_x(0, RESULT_INVALID_ENUM_VALUE);
                        cpu.regs.write_x(1, 0);
                    }
                }
                SvcResult::Success
            }

            // 0x41: svcAcceptSession(&out_session_handle, port_handle)
            0x41 => {
                let port_handle = cpu.regs.read_w(1);
                let session_handle = self
                    .ipc
                    .allocate_handle(ServiceType::Generic(format!("session:{port_handle}")));
                cpu.regs.write_x(0, 0); // ResultSuccess
                cpu.regs.write_x(1, session_handle as u64);
                SvcResult::Success
            }

            // 0x43: svcReplyAndReceive(&out_index, handles_ptr, handle_count, reply_target, timeout_ns)
            0x43 => {
                let count = cpu.regs.read_w(2);
                let reply_target = cpu.regs.read_w(3);
                let tls_addr = cpu.regs.tpidrro_el0;

                if reply_target != 0 && count == 0 {
                    // Server finished writing its SFCO reply to TLS and sent it via svcReplyAndReceive!
                    cpu.regs.write_x(0, 0);
                    cpu.regs.write_x(1, 0);
                    return SvcResult::ExitProcess;
                }

                self.reply_receive_step += 1;
                if self.reply_receive_step == 1 {
                    // Step 1: Signal port 0 ("ro:1") has an incoming client connection
                    cpu.regs.write_x(0, 0);
                    cpu.regs.write_x(1, 0);
                } else {
                    // Step 2: Deliver the `0x524f0001` ("RO\0\x01") challenge request on session slot (index 2)
                    let _ = mmu.write_u32(tls_addr, 4);               // HIPC Request Type 4
                    let _ = mmu.write_u32(tls_addr + 4, 10);          // 10 raw words
                    let _ = mmu.write_u32(tls_addr + 16, 0x4943_4653); // "SFCI"
                    let _ = mmu.write_u32(tls_addr + 20, 0);
                    let _ = mmu.write_u32(tls_addr + 24, 0x524F_0001); // Command 0x524f0001
                    let _ = mmu.write_u32(tls_addr + 28, 0);
                    let _ = mmu.write_u64(tls_addr + 32, 0x1122_3344_5566_7788); // Challenge nonce
                    cpu.regs.write_x(0, 0);
                    cpu.regs.write_x(1, 2); // Index 2 = accepted session handle
                }
                SvcResult::Success
            }

            // 0x45: svcCreateEvent(&out_write_handle, &out_read_handle)
            0x45 => {
                let handle = self.sync.create_event(false);
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, handle as u64);
                cpu.regs.write_x(2, handle as u64);
                SvcResult::Success
            }

            // 0x1F: svcConnectToNamedPort(&out_session_handle, port_name_ptr)
            0x1F => {
                let name_ptr = cpu.regs.read_x(1);
                let mut name_buf = [0u8; 16];
                let _ = mmu.read_bytes(name_ptr, &mut name_buf);
                let len = name_buf.iter().position(|&b| b == 0).unwrap_or(16);
                let port_name = String::from_utf8_lossy(&name_buf[..len]).to_string();

                let service = if port_name == "sm:" {
                    ServiceType::Sm
                } else {
                    ServiceType::Generic(port_name)
                };

                let handle = self.ipc.allocate_handle(service);
                cpu.regs.write_x(0, 0); // ResultSuccess
                cpu.regs.write_x(1, handle as u64);
                SvcResult::Success
            }

            // 0x21: svcSendSyncRequest(session_handle)
            0x21 => {
                let session_handle = cpu.regs.read_w(0);
                let tls_addr = cpu.regs.tpidrro_el0;
                let res = self.ipc.handle_sync_request(
                    session_handle,
                    tls_addr,
                    mmu,
                    &mut self.nvgpu,
                    &mut self.audio,
                    &mut self.vfs,
                );
                cpu.regs.write_x(0, res);
                SvcResult::Success
            }

            // 0x24: svcGetProcessId(&out_process_id, process_handle)
            // Used with handle 0xFFFF8001 = SELF / CurrentProcess
            0x24 => {
                cpu.regs.write_x(0, 0);    // ResultSuccess
                cpu.regs.write_x(1, 0x52); // Switch 2 Native FS Process ID
                SvcResult::Success
            }

            // 0x25: svcGetThreadId(&out_thread_id, handle)
            0x25 => {
                let handle = cpu.regs.read_w(1);
                let id = match self.scheduler.threads.get(&handle) {
                    Some(t) if handle != super::scheduler::MAIN_THREAD_KEY => t.thread_id,
                    // The CurrentThread pseudo-handle (and the main thread's own handle) name the caller.
                    _ => self.scheduler.current_thread_id(),
                };
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, id);
                SvcResult::Success
            }

            // 0x26: svcBreak(reason, info1, info2)
            0x26 => {
                let reason = cpu.regs.read_x(0);
                let info1 = cpu.regs.read_x(1);
                let info2 = cpu.regs.read_x(2);
                SvcResult::Break {
                    reason,
                    info1,
                    info2,
                }
            }

            // 0x27: svcOutputDebugString(str_addr, str_len)
            0x27 => {
                let str_addr = cpu.regs.read_x(0);
                let str_len = cpu.regs.read_x(1) as usize;

                let mut buf = vec![0u8; str_len];
                if mmu.read_bytes(str_addr, &mut buf).is_ok() {
                    let text = String::from_utf8_lossy(&buf);
                    print!("{text}");
                }

                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x28: svcReturnFromException(result). Exceptions are not delivered to the guest, so a
            // return from one means the guest's handler gave up: stop like svcBreak.
            0x28 => SvcResult::Break {
                reason: cpu.regs.read_x(0),
                info1: 0,
                info2: 0,
            },

            // 0x2A: svcFlushEntireDataCache() / 0x2B: svcFlushDataCache(addr, size). Memory is coherent here.
            0x2A | 0x2B => {
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x2C: svcMapPhysicalMemory(addr, size) / 0x2D: svcUnmapPhysicalMemory(addr, size)
            0x2C => {
                let (addr, size) = (cpu.regs.read_x(0), cpu.regs.read_x(1) as usize);
                mmu.map_range(addr, size, Permission::READ_WRITE);
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }
            0x2D => {
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // ext::SUBMIT_GPU_PUSHBUFFER(pb_addr, word_count) - Fast-path NVGPU GPFIFO submission
            ext::SUBMIT_GPU_PUSHBUFFER => {
                let pb_addr = cpu.regs.read_x(0);
                let word_count = cpu.regs.read_x(1) as usize;
                if pb_addr != 0 && word_count > 0 {
                    self.nvgpu.process_pushbuffer(mmu, pb_addr, word_count);
                }
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // ext::READ_ROMFS_FILE(path_ptr, path_len, dst_addr, max_len)
            ext::READ_ROMFS_FILE => {
                let path_ptr = cpu.regs.read_x(0);
                let path_len = cpu.regs.read_x(1) as usize;
                let dst_addr = cpu.regs.read_x(2);
                let max_len = cpu.regs.read_x(3) as usize;

                let mut path_buf = vec![0u8; path_len];
                let mut bytes_read = 0;
                if mmu.read_bytes(path_ptr, &mut path_buf).is_ok() {
                    let path_str = String::from_utf8_lossy(&path_buf).to_string();
                    if let Ok(n) = self
                        .vfs
                        .read_file_to_guest(&path_str, 0, dst_addr, max_len, mmu)
                    {
                        bytes_read = n;
                    }
                }
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, bytes_read as u64);
                SvcResult::Success
            }

            // ext::APPEND_AUDIO_BUFFER(pcm_addr, sample_count)
            ext::APPEND_AUDIO_BUFFER => {
                let pcm_addr = cpu.regs.read_x(0);
                let sample_count = cpu.regs.read_x(1) as usize;
                self.audio.append_pcm_buffer(mmu, pcm_addr, sample_count);
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, self.audio.peak_amplitude as u64);
                SvcResult::Success
            }

            // ext::WRITE_SAVE_DATA_FILE(path_ptr, path_len, src_addr, data_len)
            ext::WRITE_SAVE_DATA_FILE => {
                let path_ptr = cpu.regs.read_x(0);
                let path_len = cpu.regs.read_x(1) as usize;
                let src_addr = cpu.regs.read_x(2);
                let data_len = cpu.regs.read_x(3) as usize;

                let mut path_buf = vec![0u8; path_len];
                let mut bytes_written = 0;
                if mmu.read_bytes(path_ptr, &mut path_buf).is_ok() {
                    let path_str = String::from_utf8_lossy(&path_buf).to_string();
                    if let Ok(n) = self
                        .vfs
                        .write_file_from_guest(&path_str, src_addr, data_len, mmu)
                    {
                        bytes_written = n;
                    }
                }
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, bytes_written as u64);
                SvcResult::Success
            }

            // ext::NVN_SUBMIT_COMMAND_BUFFER(cmd_stream_addr, max_words) - Fast-path NVN Command Stream Submission
            ext::NVN_SUBMIT_COMMAND_BUFFER => {
                let cmd_stream_addr = cpu.regs.read_x(0);
                let max_words = cpu.regs.read_x(1) as usize;
                let count = self.nvn.submit_guest_command_stream(
                    mmu,
                    &mut self.nvgpu,
                    cmd_stream_addr,
                    max_words,
                );
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, count as u64);
                SvcResult::Success
            }

            // ext::NNSDK_DISPATCH(func_id, arg0, arg1, arg2, arg3) - Nintendo SDK C++ HLE Dispatch
            ext::NNSDK_DISPATCH => {
                let func_id = cpu.regs.read_w(0);
                let arg0 = cpu.regs.read_x(1);
                let arg1 = cpu.regs.read_x(2);
                let arg2 = cpu.regs.read_x(3);
                let arg3 = cpu.regs.read_x(4);
                let (res, out_val) = self.nnsdk.dispatch_call(
                    func_id,
                    arg0,
                    arg1,
                    arg2,
                    arg3,
                    &mut self.vfs,
                    mmu,
                );
                cpu.regs.write_x(0, res as u64);
                cpu.regs.write_x(1, out_val);
                SvcResult::Success
            }

            // 0x34: svcWaitForAddress(address, arb_type, value, timeout_ns)
            0x34 => {
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x35: svcSignalToAddress(address, signal_type, value, count)
            0x35 => {
                let addr = cpu.regs.read_x(0);
                let signal_type = cpu.regs.read_w(1);
                let val = cpu.regs.read_w(2);
                if signal_type == 1 {
                    // SignalAndIncrementIfEqual
                    let cur = mmu.read_u32(addr).unwrap_or(0);
                    if cur == val {
                        let _ = mmu.write_u32(addr, cur.wrapping_add(1));
                    }
                } else if signal_type == 2 {
                    // SignalAndModifyByWaitingCountIfEqual
                    let _ = mmu.write_u32(addr, val);
                }
                cpu.regs.write_x(0, 0);
                SvcResult::Success
            }

            // 0x73..=0x78: RO/FS cross-process memory management SVCs
            // 0x73: svcSetProcessMemoryPermission(handle, addr, size, perm)
            // 0x74: svcMapProcessMemory(dst_addr, handle, src_addr, size)
            // 0x75: svcUnmapProcessMemory(dst_addr, handle, src_addr, size)
            // 0x76: svcQueryProcessMemory(out_mem_info, &out_page_info, handle, addr)
            // 0x77: svcMapProcessCodeMemory(handle, dst_addr, src_addr, size)
            // 0x78: svcUnmapProcessCodeMemory(handle, dst_addr, src_addr, size)
            0x73 | 0x74 | 0x75 | 0x77 | 0x78 => {
                if svc_id == 0x74 || svc_id == 0x77 {
                    let dst_addr = if svc_id == 0x74 {
                        cpu.regs.read_x(0)
                    } else {
                        cpu.regs.read_x(1)
                    };
                    let size = (cpu.regs.read_x(3) as usize).max(0x1000);
                    if dst_addr != 0 {
                        mmu.map_range(dst_addr, size, Permission::READ_WRITE_EXECUTE);
                    }
                }
                cpu.regs.write_x(0, 0); // ResultSuccess
                SvcResult::Success
            }

            0x76 => {
                let mem_info_ptr = cpu.regs.read_x(0);
                let query_addr = cpu.regs.read_x(2);
                let aligned_base = query_addr & !0xFFF;
                let _ = mmu.write_u64(mem_info_ptr, aligned_base);
                let _ = mmu.write_u64(mem_info_ptr + 8, 0x1000);
                let _ = mmu.write_u32(mem_info_ptr + 16, 0x03); // MemoryType::CodeStatic (3)
                let _ = mmu.write_u32(mem_info_ptr + 20, 0);    // attributes = 0
                let _ = mmu.write_u32(mem_info_ptr + 24, 0x04); // permission = Execute-Only (4) on Switch 2 FS
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, 0);
                SvcResult::Success
            }

            other => {
                eprintln!("[Horizon SVC] Unhandled SVC #0x{other:02X}");
                SvcResult::Unhandled(other)
            }
        }
    }
}

/// Clean-room AArch64 FS query program, written for this project (see Stage 10 in `main.rs`).
/// Queries svcGetProcessId / svcGetThreadId / svcGetInfo(SystemTick), stores the results at [x9 + 0..=32]
/// and writes the challenge response `x10 ^ (x10 << 1)` at [x9 + 40].
///
/// Source (assembled with `llvm-mc --triple=aarch64 -mattr=+v8.5a`):
///   bti c
///   str  x30, [sp, #-16]!
///   mov  x9, x0
///   mov  x10, x1
///   movn w1, #0x7ffe        // CurrentProcess pseudo-handle (0xFFFF8001)
///   svc  #0x24              // svcGetProcessId
///   str  x0, [x9]
///   str  x1, [x9, #8]
///   movn w1, #0x7fff        // CurrentThread pseudo-handle (0xFFFF8000)
///   svc  #0x25              // svcGetThreadId
///   str  x0, [x9, #16]
///   str  x1, [x9, #24]
///   svc  #0x1e              // svcGetSystemTick
///   str  x0, [x9, #32]
///   eor  x0, x10, x10, lsl #1
///   str  x0, [x9, #40]
///   ldr  x30, [sp], #16
///   ret
pub const FS_QUERY_PAYLOAD: [u32; 18] = [
    0xD503245F, // bti c
    0xF81F0FFE, // str x30, [sp, #-16]!
    0xAA0003E9, // mov x9, x0
    0xAA0103EA, // mov x10, x1
    0x128FFFC1, // movn w1, #0x7ffe
    0xD4000481, // svc #0x24
    0xF9000120, // str x0, [x9]
    0xF9000521, // str x1, [x9, #8]
    0x128FFFE1, // movn w1, #0x7fff
    0xD40004A1, // svc #0x25
    0xF9000920, // str x0, [x9, #16]
    0xF9000D21, // str x1, [x9, #24]
    0xD40003C1, // svc #0x1e
    0xF9001120, // str x0, [x9, #32]
    0xCA0A0540, // eor x0, x10, x10, lsl #1
    0xF9001520, // str x0, [x9, #40]
    0xF84107FE, // ldr x30, [sp], #16
    0xD65F03C0, // ret
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::StepResult;

    /// Runs `FS_QUERY_PAYLOAD` and checks the challenge response it writes to the output buffer.
    #[test]
    fn test_fs_query_payload_challenge_response() {
        let mut cpu = Cpu::new();
        let mut mmu = Memory::new();
        let mut kernel = HorizonKernel::new();

        let code_base = 0x8000_0000u64;
        let out_buf = 0x8002_0000u64;
        let stack_base = 0x8100_0000u64;
        let challenge: u64 = 0x0000_0000_0020_2600;

        mmu.map_range(code_base, 4096, Permission::READ_WRITE_EXECUTE);
        mmu.map_range(out_buf, 4096, Permission::READ_WRITE);
        mmu.map_range(stack_base, 4096, Permission::READ_WRITE);
        for (i, &w) in FS_QUERY_PAYLOAD.iter().enumerate() {
            mmu.write_u32(code_base + (i * 4) as u64, w).unwrap();
        }
        mmu.write_u32(code_base + 0x100, 0xD400_00E1).unwrap(); // svcExitProcess

        cpu.regs.pc = code_base;
        cpu.regs.write_x(0, out_buf);
        cpu.regs.write_x(1, challenge);
        cpu.regs.sp = stack_base + 0x800;
        cpu.regs.write_x(30, code_base + 0x100);

        loop {
            match cpu.step(&mut mmu).unwrap() {
                StepResult::Continue => {}
                StepResult::Svc(id) => {
                    if kernel.dispatch_svc(id, &mut cpu, &mut mmu) == SvcResult::ExitProcess {
                        break;
                    }
                }
                StepResult::Halt(_) => break,
            }
        }

        assert_eq!(mmu.read_u64(out_buf + 40).unwrap(), challenge ^ (challenge << 1));
    }

    #[test]
    fn wait_synchronization_on_threads() {
        let mut cpu = Cpu::new();
        let mut mmu = Memory::new();
        let mut kernel = HorizonKernel::new();
        mmu.map_range(0x9000, 0x1000, Permission::READ_WRITE);

        // A created, never started thread: not signaled.
        cpu.regs.write_x(1, 0x5000);
        cpu.regs.write_x(3, 0x6000);
        kernel.dispatch_svc(0x08, &mut cpu, &mut mmu);
        let thread = cpu.regs.read_x(1) as u32;
        mmu.write_u32(0x9000, thread).unwrap();
        let wait = |cpu: &mut Cpu, timeout: i64| {
            cpu.regs.write_x(1, 0x9000);
            cpu.regs.write_x(2, 1);
            cpu.regs.write_x(3, timeout as u64);
        };

        // Polling times out; a timed wait with nothing else runnable times out; an endless one is a deadlock.
        wait(&mut cpu, 0);
        assert_eq!(kernel.dispatch_svc(0x18, &mut cpu, &mut mmu), SvcResult::Success);
        assert_eq!(cpu.regs.read_x(0), RESULT_TIMED_OUT);
        wait(&mut cpu, 1_000_000);
        kernel.dispatch_svc(0x18, &mut cpu, &mut mmu);
        assert_eq!(cpu.regs.read_x(0), RESULT_TIMED_OUT);
        wait(&mut cpu, -1);
        assert!(matches!(kernel.dispatch_svc(0x18, &mut cpu, &mut mmu), SvcResult::Break { reason: 0xDEAD_10C4, .. }));

        // Without handles, 0x18 is still the demos' VSync.
        cpu.regs.write_x(2, 0);
        assert_eq!(kernel.dispatch_svc(0x18, &mut cpu, &mut mmu), SvcResult::PresentFrame);
    }

    #[test]
    fn horizon_numbers_and_research_range() {
        let mut cpu = Cpu::new();
        let mut mmu = Memory::new();
        let mut kernel = HorizonKernel::new();

        // svcGetInfo(CoreMask) and an unknown info type.
        cpu.regs.write_x(1, 0);
        assert_eq!(kernel.dispatch_svc(0x29, &mut cpu, &mut mmu), SvcResult::Success);
        assert_eq!((cpu.regs.read_x(0), cpu.regs.read_x(1)), (0, 0xFF));
        cpu.regs.write_x(1, 0xDEAD);
        kernel.dispatch_svc(0x29, &mut cpu, &mut mmu);
        assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_ENUM_VALUE);

        // svcGetSystemTick returns the tick in X0.
        kernel.dispatch_svc(0x1E, &mut cpu, &mut mmu);
        assert!(cpu.regs.read_x(0) >= 19_200);

        // Flush/attribute calls succeed without side effects.
        for svc in [0x02, 0x03, 0x0F, 0x2A, 0x2B] {
            cpu.regs.write_x(0, 0x1234);
            assert_eq!(kernel.dispatch_svc(svc, &mut cpu, &mut mmu), SvcResult::Success);
            assert_eq!(cpu.regs.read_x(0), 0, "svc 0x{svc:X}");
        }

        // svcQueryMemory reports whole runs: free up to the next mapped page, mapped across contiguous pages.
        mmu.map_range(0x9000, 0x1000, Permission::READ_WRITE);
        mmu.map_range(0x20_0000, 0x3000, Permission::READ_WRITE);
        cpu.regs.write_x(0, 0x9000);
        cpu.regs.write_x(2, 0x10_0000);
        kernel.dispatch_svc(0x06, &mut cpu, &mut mmu);
        assert_eq!(mmu.read_u32(0x9000 + 16).unwrap(), 0);
        assert_eq!((mmu.read_u64(0x9000).unwrap(), mmu.read_u64(0x9008).unwrap()), (0xA000, 0x20_0000 - 0xA000));
        cpu.regs.write_x(0, 0x9000);
        cpu.regs.write_x(2, 0x20_1234);
        kernel.dispatch_svc(0x06, &mut cpu, &mut mmu);
        assert_eq!((mmu.read_u64(0x9000).unwrap(), mmu.read_u64(0x9008).unwrap()), (0x20_0000, 0x3000));

        // The research SVCs live above Horizon's range.
        assert_eq!(ext::svc(ext::SUBMIT_GPU_PUSHBUFFER), 0xD400_2021);
        cpu.regs.write_x(0, 0);
        assert_eq!(kernel.dispatch_svc(ext::SUBMIT_GPU_PUSHBUFFER, &mut cpu, &mut mmu), SvcResult::Success);
        assert_eq!(kernel.dispatch_svc(ext::PRESENT_FRAME, &mut cpu, &mut mmu), SvcResult::PresentFrame);
        assert!(matches!(kernel.dispatch_svc(0x28, &mut cpu, &mut mmu), SvcResult::Break { .. }));
    }
}
