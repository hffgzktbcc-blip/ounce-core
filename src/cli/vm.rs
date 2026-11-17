//! VM setup, the stage gate and the core `run_vm_internal` loop.

use crate::*;

pub(crate) fn setup_vm() -> (Cpu, Memory, HorizonKernel) {
    setup_vm_in(Memory::new(), 0, 0x10_0000_0000)
}

/// [`setup_vm`] into `mmu`, with the low layout moved up by `base` and the alias region `alias_size`
/// long (native execution, see `HorizonKernel::set_addr_base`).
pub(crate) fn setup_vm_in(mut mmu: Memory, base: u64, alias_size: u64) -> (Cpu, Memory, HorizonKernel) {
    let mut cpu = Cpu::new();
    let mut kernel = HorizonKernel::new();
    kernel.set_addr_base(base, alias_size);

    // Map stack space: 0x7FFFF00000 .. 0x7FFFFFF000
    let stack_base = STACK_TOP - STACK_SIZE as u64;
    mmu.map_range(stack_base, STACK_SIZE, Permission::READ_WRITE);

    // Map Thread-Local Storage (TLS) page for Horizon HIPC/CMIF buffers
    mmu.map_range(DEFAULT_TLS_ADDR, TLS_SIZE, Permission::READ_WRITE);

    // Initialize stack pointer, TLS pointers, and PC
    cpu.regs.sp = STACK_TOP;
    cpu.regs.tpidr_el0 = DEFAULT_TLS_ADDR;
    cpu.regs.tpidrro_el0 = DEFAULT_TLS_ADDR;
    cpu.regs.pc = base + DEFAULT_BASE_ADDR;

    (cpu, mmu, kernel)
}

/// Why a guest run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmStop {
    /// The guest called `svcExitProcess`.
    ExitProcess,
    /// The guest called `svcBreak`.
    Break,
    /// The guest called an SVC the kernel model does not handle.
    UnhandledSvc,
    /// The CPU halted on an undecodable or unsupported instruction.
    Halt,
    /// The CPU or MMU returned an error.
    Fault,
    /// The instruction budget ran out before the guest exited.
    Budget,
    /// The host window was closed.
    WindowClosed,
    /// The requested number of frames was presented.
    FrameLimit,
}

/// Final guest state of a run, used by the verification harness.
#[derive(Debug, Clone)]
pub(crate) struct VmOutcome {
    pub(crate) stop: VmStop,
    pub(crate) x0: u64,
    pub(crate) d0: f64,
    pub(crate) report: run_report::RunReport,
}

pub(crate) fn run_vm(
    cpu: Cpu,
    mmu: Memory,
    kernel: HorizonKernel,
    max_instructions: usize,
) -> VmOutcome {
    run_vm_internal(cpu, mmu, kernel, None, None, max_instructions, 0)
}

/// Pass/fail bookkeeping for the integration harness. A stage passes only
/// when its guest exits through `svcExitProcess` with the expected `X0`.
#[derive(Debug, Default)]
pub(crate) struct StageGate {
    pub(crate) passed: usize,
    pub(crate) failures: Vec<String>,
}

/// Guard synthetic service programs before they can overwrite a failed result with a success code.
/// A zero-offset B.NE in the input is a comparison guard targeting the same failure exit.
pub(crate) fn checked_service_program(words: &[u32], svc_id: u16, cmif: bool) -> Vec<u32> {
    let mut code = Vec::new();
    let mut guards = Vec::new();
    for &word in words {
        if word == 0x5400_0001 { guards.push(code.len()); }
        code.push(word);
        if word == kernel::svc::ext::svc(svc_id) {
            guards.push(code.len());
            code.push(0xB500_0000); // cbnz x0, failure
            if cmif {
                code.extend([0xD53B_D076, 0xB940_1AD7]); // mrs x22,tpidrro_el0; ldr w23,[x22,#0x18]
                guards.push(code.len());
                code.push(0x3500_0017); // cbnz w23, failure
            }
        }
    }
    let failure = code.len();
    code.extend([0xD281_75A0, 0xD400_00E1]); // movz x0,#0xBAD; svcExitProcess
    for index in guards { code[index] |= ((failure - index) as u32) << 5; }
    code
}

impl StageGate {
    pub(crate) fn expect_surface(&mut self, stage: u32, pixels: &[u32], count: usize, color: u32) -> bool {
        self.expect(stage, "decoded constant-color surface", pixels.len() == count && pixels.iter().all(|&pixel| pixel == color))
    }
    /// Checks a guest run and prints the verdict. Returns `true` on pass.
    pub(crate) fn expect_exit(&mut self, stage: u32, outcome: &VmOutcome, expected_x0: u64) -> bool {
        if outcome.stop == VmStop::ExitProcess && outcome.x0 == expected_x0 {
            self.passed += 1;
            println!("  [Gate] Stage {:02} PASS (X0 = 0x{:X})", stage, outcome.x0);
            true
        } else {
            let msg = format!(
                "Stage {:02}: expected svcExitProcess with X0 = 0x{:X}, got {:?} with X0 = 0x{:X}",
                stage, expected_x0, outcome.stop, outcome.x0
            );
            println!("  [Gate] FAIL {}", msg);
            self.failures.push(msg);
            false
        }
    }

    /// Records a host-side check that has no guest exit code. Returns `ok`.
    pub(crate) fn expect(&mut self, stage: u32, what: &str, ok: bool) -> bool {
        if ok {
            self.passed += 1;
            println!("  [Gate] Stage {:02} PASS ({})", stage, what);
        } else {
            let msg = format!("Stage {:02}: {} failed", stage, what);
            println!("  [Gate] FAIL {}", msg);
            self.failures.push(msg);
        }
        ok
    }

    /// Prints the summary and exits the process with status 1 on any failure.
    pub(crate) fn finish(self) {
        if self.failures.is_empty() {
            println!("[Harness] All {} checks passed across 25 stages.", self.passed);
        } else {
            eprintln!(
                "[Harness] {} check(s) failed, {} passed:",
                self.failures.len(),
                self.passed
            );
            for f in &self.failures {
                eprintln!("  - {}", f);
            }
            std::process::exit(1);
        }
    }
}

pub(crate) fn queued_vi_frame(kernel: &HorizonKernel, memory: &Memory) -> Result<gpu::present::Frame, String> {
    let submitted = kernel.ipc.submitted_vi_surface().ok_or("queued VI buffer cannot be resolved")?;
    let surface = submitted.tiling_surface().ok_or("unsupported queued VI surface descriptor")?;
    let frame = gpu::present::surface::frame_from_surface(memory, submitted.guest_addr, &surface)
        .map_err(|error| format!("cannot decode queued VI surface: {error:?}"))?;
    Ok(match kernel.ipc.submitted_vi_crop() {
        Some((left, top, right, bottom)) => frame.cropped((left as usize, top as usize, right as usize, bottom as usize)),
        None => frame,
    })
}

pub(crate) fn run_vm_internal(
    mut cpu: Cpu,
    mut mmu: Memory,
    mut kernel: HorizonKernel,
    mut display: Option<&mut DisplayServer>,
    mut hid: Option<&mut HidServer>,
    max_instructions: usize,
    max_frames: usize,
) -> VmOutcome {
    let is_gpu = display.as_ref().is_some_and(|d| d.is_gpu_accelerated());
    kernel.nvgpu.set_hardware_acceleration(is_gpu);

    println!("[VM] Starting execution at PC: 0x{:016X}", cpu.regs.pc);
    println!("[VM] Stack Pointer: 0x{:016X}", cpu.regs.sp);

    let start = Instant::now();
    let mut instructions_executed = 0;
    let mut frames_presented = 0;
    let max_frames = if max_frames == 0 { loop_controls::max_frames() } else { max_frames };
    let injected_presses = loop_controls::presses();
    let mut gamepad_mgr = HostGamepadManager::new();
    if let Some(pad_name) = gamepad_mgr.active_gamepad_name() {
        println!("[HID] Physical gamepad attached: {}", pad_name);
    }
    let host_audio = host_audio_for(gpu::BackendKind::from_env());
    let mut stop = VmStop::Budget;
    let mut stop_pc: Option<u64> = None;
    let mut stop_opcode: Option<u32> = None;
    let mut stop_fault_addr: Option<u64> = None;
    let mut stop_svc_id: Option<u32> = None;
    let mut stop_break_result: Option<u32> = None;
    let mut stop_message: Option<String> = None;
    let mut last_input_poll = Instant::now();
    let mut last_queued_frame = 0;
    let mut engine = cpu::CpuTier::explicit()
        .filter(|tier| *tier != cpu::CpuTier::Native)
        .map(cpu::CpuEngine::new);
    if let Some(e) = &engine {
        println!("[CPU] Block engine: {}", e.name());
    }
    // `OUNCE_TRACE=<file>` records register-only instructions for `ounce-core diverge`. With a block engine
    // selected (`--cpu=interpreter|tier3|tier4`) it runs one-instruction slices so each recorded instruction is
    // executed by that engine's own translation (single-instruction blocks, not multi-instruction fusion).
    let mut tracer = trace::Tracer::from_env();

    loop {
        if max_instructions > 0 && instructions_executed >= max_instructions {
            println!("\n[VM] Execution budget reached ({} instructions).", max_instructions);
            break;
        }

        // With an explicit `--cpu=interpreter|tier3|tier4` the block engine runs the guest in slices that end at the next
        // preemption point, a budget limit or an SVC; otherwise one instruction at a time (the default).
        let before = instructions_executed;
        let (stepped, executed) = match engine.as_mut() {
            Some(e) => {
                let mut slice = kernel::scheduler::PREEMPT_INTERVAL - before % kernel::scheduler::PREEMPT_INTERVAL;
                if max_instructions > 0 {
                    slice = slice.min(max_instructions - before);
                }
                if tracer.is_some() {
                    slice = 1;
                }
                let pc = cpu.regs.pc;
                let traced = match tracer.as_mut() {
                    Some(t) => mmu.read_u32(pc).ok().and_then(|raw| t.pre(&cpu, raw).map(|pre| (raw, pre))),
                    None => None,
                };
                let (count, exit) = e.run_slice(&mut cpu, &mut mmu, slice.max(1));
                if let (Some(t), Some((raw, pre))) = (tracer.as_mut(), traced) {
                    let stopped = matches!(exit, cpu::tier2_jit::BlockExit::Fault { .. } | cpu::tier2_jit::BlockExit::Svc { .. });
                    if count == 1 && !stopped {
                        t.post(&cpu, raw, pc, pre);
                    } else if !stopped {
                        t.skip(raw);
                    }
                }
                match exit {
                    cpu::tier2_jit::BlockExit::Fault { pc: fault_pc } => {
                        let detail = cpu.last_error.take().or_else(|| {
                            cpu.regs.pc = fault_pc;
                            cpu.step(&mut mmu).err().map(|err| err.to_string())
                        });
                        let msg = match detail {
                            Some(reason) => format!("{} fault at {fault_pc:#x} (slice began at {pc:#x}): {reason}", e.name()),
                            None => format!("{} fault at {fault_pc:#x} (slice began at {pc:#x})", e.name()),
                        };
                        (Err(msg), count)
                    }
                    cpu::tier2_jit::BlockExit::Svc { svc_id } => (Ok(StepResult::Svc(svc_id)), count),
                    _ if count == 0 => (Err(format!("{} made no progress at {pc:#x}", e.name())), 0),
                    _ => (Ok(StepResult::Continue), count),
                }
            }
            None => {
                let traced = match tracer.as_mut() {
                    Some(t) => {
                        let pc = cpu.regs.pc;
                        mmu.read_u32(pc).ok().and_then(|raw| t.pre(&cpu, raw).map(|pre| (raw, pc, pre)))
                    }
                    None => None,
                };
                let step_result = cpu.step(&mut mmu);
                if let (Some(t), Some((raw, pc, pre)), Ok(StepResult::Continue)) = (tracer.as_mut(), traced, &step_result) {
                    t.post(&cpu, raw, pc, pre);
                }
                (step_result.map_err(|e| e.to_string()), 1)
            }
        };
        match stepped {
            Ok(StepResult::Continue) => {
                instructions_executed += executed;
                // Time slice: a guest thread that spins without SVCs must not starve the others.
                if instructions_executed / kernel::scheduler::PREEMPT_INTERVAL != before / kernel::scheduler::PREEMPT_INTERVAL {
                    kernel.scheduler.preempt(&mut cpu);
                }
            }
            Ok(StepResult::Svc(svc_id)) => {
                instructions_executed += executed;
                let result = kernel.dispatch_svc(svc_id, &mut cpu, &mut mmu);
                if let Some(ref mut disp) = display {
                    let queued = kernel.ipc.vi_frames_queued();
                    if queued != last_queued_frame {
                        let frame = match queued_vi_frame(&kernel, &mmu) {
                            Ok(frame) => frame,
                            Err(error) => {
                                eprintln!("[Display Error] {error}");
                                stop = VmStop::Fault;
                                break;
                            }
                        };
                        if last_queued_frame == 0 {
                            println!("[Display] Guest VI frame: {}x{}", frame.width, frame.height);
                        }
                        last_queued_frame = queued;
                        if !disp.present_guest_frame(frame) {
                            stop = VmStop::WindowClosed;
                            break;
                        }
                        frames_presented += 1;
                        if max_frames > 0 && frames_presented >= max_frames {
                            stop = VmStop::FrameLimit;
                            break;
                        }
                    }
                }
                match result {
                    SvcResult::Success => {
                        // libnx homebrew reads HID shared memory between SVCs rather than per presented
                        // frame: hand it the host controller every few milliseconds.
                        // Scripted --press runs poll after every SVC: a wall-clock interval would let a fast guest read the pad first.
                        if kernel.ipc.hid.addr.is_some() && (!injected_presses.is_empty() || last_input_poll.elapsed() >= INPUT_POLL_INTERVAL) {
                            last_input_poll = Instant::now();
                            if let Some(ref mut h) = hid {
                                if let Some(ref mut disp) = display {
                                    if !disp.pump_events() {
                                        println!("\n[Display] Window closed by user. Shutting down.");
                                        stop = VmStop::WindowClosed;
                                        break;
                                    }
                                    h.poll_keyboard(&disp.get_active_keys());
                                }
                                gamepad_mgr.poll(h);
                                h.state.buttons |= loop_controls::held_mask(injected_presses, frames_presented);
                                kernel.ipc.hid.input = kernel::hid::NpadInput::from_controller(&h.state);
                            }
                        }
                        // libnx audio renderer output plays as it is mixed (homebrew may never present a
                        // frame), and the host device's playback clock paces the guest.
                        if kernel.ipc.audio_renderer.is_some() {
                            stream_audio_to_host(&mut kernel, &host_audio);
                        }
                    }
                    SvcResult::PresentFrame => {
                        if last_queued_frame == 0 { frames_presented += 1; }

                        // 1. Poll host keyboard and physical gamepad input
                        if let Some(ref mut h) = hid {
                            if let Some(ref mut disp) = display {
                                let active_keys = disp.get_active_keys();
                                h.poll_keyboard(&active_keys);
                            }
                            gamepad_mgr.poll(h);
                            h.state.buttons |= loop_controls::held_mask(injected_presses, frames_presented);
                            let _ = h.update_shared_memory(&mut mmu);
                        }

                        // 2. Stream audio to host output sink (48 kHz stereo)
                        let audio_samples = kernel.audio.drain_samples();
                        if !audio_samples.is_empty() {
                            host_audio.push_samples(&audio_samples);
                        }
                        let audren_samples = kernel.audren.drain_samples();
                        if !audren_samples.is_empty() {
                            host_audio.push_samples(&audren_samples);
                        }

                        // 3. Present video frame to host window
                        let draw_ops = kernel.nvgpu.drain_draw_ops();
                        if let Some(ref mut disp) = display {
                            if last_queued_frame == 0 && !disp.present_frame_3d(&mmu, &draw_ops) {
                                println!("\n[Display] Window closed by user. Shutting down.");
                                stop = VmStop::WindowClosed;
                                break;
                            }
                        } else {
                            println!("[VM] VSync / Frame {} presented.", frames_presented);
                        }

                        if max_frames > 0 && frames_presented >= max_frames {
                            println!("\n[Display] Rendered {} benchmark frames. Exiting.", frames_presented);
                            stop = VmStop::FrameLimit;
                            break;
                        }
                    }
                    SvcResult::ExitProcess => {
                        println!("\n[VM] Process exited gracefully (svcExitProcess).");
                        if kernel.ipc.audio_renderer.is_some() {
                            stream_audio_to_host(&mut kernel, &host_audio);
                            // Let the last queued audio play out (bounded) before the process ends.
                            let deadline = Instant::now() + std::time::Duration::from_secs(3);
                            while host_audio.is_enabled && host_audio.queued_sample_count() > 0 && Instant::now() < deadline {
                                std::thread::sleep(std::time::Duration::from_millis(5));
                            }
                        }
                        stop = VmStop::ExitProcess;
                        break;
                    }
                    SvcResult::Break { reason, info1, info2 } => {
                        println!(
                            "\n[VM] Hit svcBreak: reason=0x{:X}, info1=0x{:X}, info2=0x{:X}",
                            reason, info1, info2
                        );
                        println!("[VM] Break PC: 0x{:016X}", cpu.regs.pc);
                        if info2 == 4 {
                            match mmu.read_u32(info1) {
                                Ok(result) => {
                                    stop_break_result = Some(result);
                                    println!("[VM] Break result: 0x{:08X} (module={}, description={})", result, result & 0x1FF, (result >> 9) & 0x1FFF);
                                }
                                Err(error) => println!("[VM] Break result unreadable: {}", error),
                            }
                        }
                        stop_pc = Some(cpu.regs.pc);
                        stop = VmStop::Break;
                        break;
                    }
                    SvcResult::Unhandled(id) => {
                        println!("\n[VM] Stopping on unhandled SVC #0x{:02X}.", id);
                        stop_svc_id = Some(id as u32);
                        stop_pc = Some(cpu.regs.pc);
                        stop = VmStop::UnhandledSvc;
                        break;
                    }
                }
            }
            Ok(StepResult::Halt(msg)) => {
                println!("\n[VM] CPU Halted: {}", msg);
                stop_pc = Some(cpu.regs.pc);
                stop_message = Some(msg);
                stop = VmStop::Halt;
                break;
            }
            Err(e) => {
                eprintln!("\n[VM Error] {}", e);
                eprintln!("  Final PC: 0x{:016X}", cpu.regs.pc);
                stop_pc = Some(cpu.regs.pc);
                stop_message = Some(e.clone());
                if let Some(rest) = e.strip_prefix("Unimplemented instruction 0x") {
                    if let Some(hex_word) = rest.split_whitespace().next() {
                        stop_opcode = u32::from_str_radix(hex_word, 16).ok();
                    }
                    if let Some(pc_hex) = rest.split("at PC: 0x").nth(1) {
                        if let Some(parsed) = u64::from_str_radix(pc_hex.trim(), 16).ok() {
                            stop_pc = Some(parsed);
                        }
                    }
                } else if e.contains("Memory access violation:") {
                    if let Some(addr_hex) = e.split("0x").nth(1) {
                        let digits: String = addr_hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
                        stop_fault_addr = u64::from_str_radix(&digits, 16).ok();
                    }
                    if stop_fault_addr.is_none() {
                        stop_fault_addr = Some(0);
                    }
                }
                stop = VmStop::Fault;
                break;
            }
        }
    }
    if let Some(t) = tracer.as_mut() {
        t.finish();
    }

    let elapsed = start.elapsed();
    let ips = if elapsed.as_secs_f64() > 0.0 {
        instructions_executed as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    println!(
        "[VM Summary] Executed {} instructions in {:.4?} ({:.2} MIPS) | Frames: {}",
        instructions_executed,
        elapsed,
        ips / 1_000_000.0,
        frames_presented
    );
    if kernel.nvgpu.state.total_commands_processed > 0 || kernel.ipc.total_ipc_requests > 0 {
        println!(
            "  -> GPU Commands: {} | 3D Draw Calls: {} | Triangles Rasterized: {} | HIPC Calls: {}",
            kernel.nvgpu.state.total_commands_processed,
            kernel.nvgpu.state.total_draw_calls,
            kernel.nvgpu.state.total_triangles_rendered,
            kernel.ipc.total_ipc_requests
        );
    }
    if kernel.vfs.total_bytes_read > 0 || kernel.audio.total_samples_processed > 0 {
        println!(
            "  -> RomFS Bytes Read: {} | Audio Buffers: {} | PCM Samples: {} (Peak Amp: {})",
            kernel.vfs.total_bytes_read,
            kernel.audio.buffers_queued,
            kernel.audio.total_samples_processed,
            kernel.audio.peak_amplitude
        );
    }
    if kernel.scheduler.total_threads_spawned > 0 {
        println!(
            "  -> Multi-Core Scheduler: {} KThreads Spawned | {} KThreads Completed",
            kernel.scheduler.total_threads_spawned,
            kernel.scheduler.total_threads_completed
        );
    }
    if let Some(ref glsl) = kernel.nvgpu.state.last_decompiled_glsl {
        let line_count = glsl.lines().count();
        println!(
            "  -> Ampere SM86 Decompiler: GLSL 450 ({} lines) + Binary SPIR-V 1.5 ({} words, magic 0x07230203)",
            line_count,
            kernel.nvgpu.state.last_decompiled_spirv_words
        );
    }
    if kernel.nvgpu.state.total_texels_sampled > 0
        || kernel.sync.total_locks_arbitrated > 0
        || kernel.sync.total_condvar_signals > 0
    {
        println!(
            "  -> 2D Texture Sampler: {} Texels | Futex/Mutex Locks: {} | CondVar Signals: {}",
            kernel.nvgpu.state.total_texels_sampled,
            kernel.sync.total_locks_arbitrated,
            kernel.sync.total_condvar_signals
        );
    }
    if kernel.nvgpu.state.total_depth_tests_passed > 0
        || kernel.nvgpu.state.total_depth_tests_rejected > 0
    {
        println!(
            "  -> 3D Z-Buffer Depth Test: {} Passed | {} Occluded (Rejected)",
            kernel.nvgpu.state.total_depth_tests_passed,
            kernel.nvgpu.state.total_depth_tests_rejected
        );
    }
    if kernel.nvgpu.upscaler.frames_upscaled > 0 || kernel.vfs.total_bytes_written > 0 {
        println!(
            "  -> 2x Upscaler Model: {} Frames ({} Output Pixels) | SaveData Written: {} Bytes",
            kernel.nvgpu.upscaler.frames_upscaled,
            kernel.nvgpu.upscaler.output_pixels_generated,
            kernel.vfs.total_bytes_written
        );
    }
    println!(
        "  -> Basic-Block JIT Cache: {} Blocks ({} Insns) | Hits: {} | Misses: {}",
        cpu.jit.blocks_compiled,
        cpu.jit.instructions_compiled,
        cpu.jit.cache_hits,
        cpu.jit.cache_misses
    );
    let mem_stats = mmu.bucket_stats();
    println!(
        "  -> Memory Buckets ({} KB): Code={} KB | VRAM={} KB | Stack/TLS={} KB | Data={} KB",
        mem_stats.total_bytes / 1024,
        mem_stats.code_bytes / 1024,
        mem_stats.vram_bytes / 1024,
        mem_stats.stack_tls_bytes / 1024,
        mem_stats.heap_data_bytes / 1024
    );
    println!(
        "  -> Final X0: {} (0x{:016X}) | D0: {:.2}",
        cpu.regs.read_x(0),
        cpu.regs.read_x(0),
        cpu.regs.read_d(0)
    );

    let tier_str = match cpu::CpuTier::from_env() {
        cpu::CpuTier::Interpreter => "interpreter",
        cpu::CpuTier::Tier3 => "tier3",
        cpu::CpuTier::Tier4 => "tier4",
        cpu::CpuTier::Native => "nce",
    };
    let (stop_kind, exit_code) = match stop {
        VmStop::ExitProcess => ("clean_exit", 0),
        VmStop::WindowClosed => ("window_closed", 0),
        VmStop::FrameLimit => ("frame_limit", 0),
        VmStop::Break => ("svc_break", 13),
        VmStop::UnhandledSvc => ("unhandled_svc", 14),
        VmStop::Halt => ("cpu_halted", 15),
        VmStop::Budget => ("budget_exhausted", 16),
        VmStop::Fault => {
            if stop_opcode.is_some() {
                ("unimplemented_insn", 10)
            } else if stop_fault_addr.is_some() {
                ("memory_fault", 11)
            } else {
                ("cpu_error", 12)
            }
        }
    };
    let report = run_report::RunReport {
        tier: tier_str,
        silent_fallback: false,
        stop_kind,
        exit_code,
        pc: stop_pc,
        opcode: stop_opcode,
        fault_addr: stop_fault_addr,
        svc_id: stop_svc_id,
        break_result: stop_break_result,
        insns_executed: instructions_executed as u64,
        frames_presented: frames_presented as u64,
        framebuffer_hash: display.as_ref().and_then(|d| d.last_frame_hash()).map(str::to_owned),
        message: stop_message,
    };

    VmOutcome {
        stop,
        x0: cpu.regs.read_x(0),
        d0: cpu.regs.read_d(0),
        report,
    }
}

#[cfg(test)]
mod harness_gate_tests {
    use super::*;

    #[test]
    fn wrong_surface_output_fails_the_harness_gate() {
        let mut pixels = [0xFF00_88FF; 16];
        let mut gate = StageGate::default();
        assert!(gate.expect_surface(22, &pixels, 16, 0xFF00_88FF));
        pixels[15] ^= 1;
        assert!(!gate.expect_surface(22, &pixels, 16, 0xFF00_88FF));
        assert_eq!(gate.failures.len(), 1);
    }

    #[test]
    fn failed_guest_service_cannot_be_hidden_by_a_success_constant() {
        for result in [0, 1] {
            let code = checked_service_program(&[0xD400_20C1, 0xD281_3320, 0xD400_00E1], 0x106, false);
            let mut memory = Memory::new();
            memory.map_range(0x8000, 4096, Permission::READ_WRITE_EXECUTE);
            for (i, word) in code.iter().enumerate() { memory.write_u32(0x8000 + 4 * i as u64, *word).unwrap(); }
            let mut cpu = Cpu::new();
            cpu.regs.pc = 0x8000;
            let mut exited = false;
            for _ in 0..12 {
                match cpu.step(&mut memory).unwrap() {
                    cpu::StepResult::Svc(0x106) => cpu.regs.write_x(0, result),
                    cpu::StepResult::Svc(7) => { exited = true; break; }
                    _ => {}
                }
            }
            assert!(exited);
            assert_eq!(cpu.regs.read_x(0), if result == 0 { 0x999 } else { 0xBAD });
        }
    }

    /// Runs a tiny program that fills the framebuffer and calls the present SVC, on a windowless display.
    fn headless_present_report(fill: u32, max_frames: usize) -> run_report::RunReport {
        let program: [u32; 6] = [
            0xD2800004, // MOVZ X4, #0
            0xF2B20004, // MOVK X4, #0x9000, LSL #16 (framebuffer base 0x90000000)
            0xD2800000 | (fill << 5) | 2, // MOVZ X2, #fill
            0xB9000082, // STR  W2, [X4]
            0xD4000301, // SVC  #0x18 (present)
            0x17FFFFFE, // B    . (present again)
        ];
        let (mut cpu, mut mmu, kernel) = setup_vm();
        mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, SCREEN_WIDTH * SCREEN_HEIGHT * 4, Permission::READ_WRITE);
        mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
        let code: Vec<u8> = program.iter().flat_map(|i| i.to_le_bytes()).collect();
        mmu.write_bytes(DEFAULT_BASE_ADDR, &code).unwrap();
        cpu.regs.pc = DEFAULT_BASE_ADDR;
        let mut display = DisplayServer::new(
            DEFAULT_FRAMEBUFFER_ADDR,
            DisplayConfig { backend: gpu::BackendKind::Headless, ..Default::default() },
        );
        run_vm_internal(cpu, mmu, kernel, Some(&mut display), None, 0, max_frames).report
    }

    #[test]
    fn report_carries_frame_count_and_hash_that_follow_the_pixels() {
        let a = headless_present_report(0x1234, 2);
        assert_eq!(a.stop_kind, "frame_limit");
        assert_eq!(a.frames_presented, 2);
        let hash = a.framebuffer_hash.clone().expect("a presented frame has a hash");
        assert!(hash.starts_with("fnv1a64:") && hash.len() == "fnv1a64:".len() + 16, "{hash}");
        // Same program, same hash; different guest pixels, different hash (the HUD does not mask the change).
        assert_eq!(headless_present_report(0x1234, 2).framebuffer_hash.as_deref(), Some(hash.as_str()));
        assert_ne!(headless_present_report(0x4321, 2).framebuffer_hash.as_deref(), Some(hash.as_str()));
    }

    fn outcome(stop: VmStop, x0: u64) -> VmOutcome {
        VmOutcome {
            stop,
            x0,
            d0: 0.0,
            report: run_report::RunReport {
                tier: "interpreter",
                silent_fallback: false,
                stop_kind: "clean_exit",
                exit_code: 0,
                pc: None,
                opcode: None,
                fault_addr: None,
                svc_id: None,
                break_result: None,
                insns_executed: 0,
                frames_presented: 0,
                framebuffer_hash: None,
                message: None,
            },
        }
    }

    #[test]
    fn test_stage_gate_requires_exit_process_and_expected_x0() {
        let mut gate = StageGate::default();
        assert!(gate.expect_exit(1, &outcome(VmStop::ExitProcess, 0xEEE), 0xEEE));
        // Right exit path, wrong value.
        assert!(!gate.expect_exit(2, &outcome(VmStop::ExitProcess, 0xBAD), 0xEEE));
        // Right value, but the guest never reached svcExitProcess.
        assert!(!gate.expect_exit(3, &outcome(VmStop::Budget, 0xEEE), 0xEEE));
        assert!(!gate.expect_exit(4, &outcome(VmStop::Halt, 0xEEE), 0xEEE));
        assert!(gate.expect(5, "host check", true));
        assert!(!gate.expect(6, "host check", false));
        assert_eq!(gate.passed, 2);
        assert_eq!(gate.failures.len(), 4);
    }
}
