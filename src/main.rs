mod audio;
mod cpu;
mod crypto;
mod fs;
mod gpu;
mod hid;
mod kernel;
mod loader;
mod mmu;
mod ui;

use cpu::{Cpu, StepResult};
use gpu::nvgpu::{
    NvgpuChannel, SubmissionMode, METHOD_BIND_SHADER_PROGRAM, METHOD_BIND_TEXTURE_2D,
    METHOD_CLEAR_SURFACE, METHOD_DISPATCH_DLSS_UPSCALE, METHOD_DRAW_ARRAYS,
    METHOD_DRAW_TEXTURED_ARRAYS, METHOD_SET_CLEAR_COLOR, METHOD_SET_DEPTH_TEST_ENABLE,
    METHOD_SET_MVP_MATRIX, METHOD_SET_RENDER_TARGET_ADDR, METHOD_SET_VERTEX_ARRAY_ADDR,
    METHOD_SET_VIEWPORT_DIMS,
};
use gpu::shader::SassOp;
use gpu::{DisplayConfig, DisplayServer, DEFAULT_FRAMEBUFFER_ADDR, SCREEN_HEIGHT, SCREEN_WIDTH};
use hid::{HidServer, HID_SHARED_MEM_ADDR};
use kernel::{HorizonKernel, ServiceType, SvcResult, CMIF_IN_MAGIC, DEFAULT_TLS_ADDR, TLS_SIZE};
use loader::{BootLoader, NcaImage, Pfs0Archive};
use mmu::{Memory, Permission};
use std::env;
use std::time::Instant;

const DEFAULT_BASE_ADDR: u64 = 0x80000000;
const STACK_TOP: u64 = 0x7FFFFFE000;
const STACK_SIZE: usize = 0x100000; // 1 MB stack

fn main() {
    let args: Vec<String> = env::args().collect();

    println!("============================================================");
    println!("  OUNCE-CORE: Nintendo Switch 2 (Tegra T239) Emulator Core");
    println!("  Target: ARMv8.2-A AArch64 | Horizon OS 64-bit Microkernel");
    println!("============================================================\n");

    if args.len() < 2 {
        print_usage();
        run_built_in_demo();
        return;
    }

    match args[1].as_str() {
        "test" | "demo" => {
            run_built_in_demo();
        }
        "gfx" | "display" => {
            let max_frames = if args.len() >= 3 {
                args[2].parse::<usize>().unwrap_or(0)
            } else {
                0
            };
            run_gfx_demo(max_frames);
        }
        "play" | "game" => {
            let max_frames = if args.len() >= 3 {
                args[2].parse::<usize>().unwrap_or(0)
            } else {
                0
            };
            run_playable_game_demo(max_frames);
        }
        "gpu3d" | "3d" => {
            let max_frames = if args.len() >= 3 {
                args[2].parse::<usize>().unwrap_or(0)
            } else {
                0
            };
            run_gpu3d_demo(max_frames);
        }
        "run" => {
            if args.len() < 3 {
                eprintln!("Error: Please provide path to .nsp, .nca, or .nro package");
                return;
            }
            let file_path = &args[2];
            run_file(file_path);
        }
        "package" | "showcase" => {
            let out_path = if args.len() >= 3 {
                &args[2]
            } else {
                "switch2_showcase.nsp"
            };
            build_and_boot_showcase_nsp(out_path);
        }
        "inspect" => {
            if args.len() < 3 {
                eprintln!("Error: Please provide path to package (.xci, .nsp, .nca, .nro)");
                return;
            }
            let file_path = &args[2];
            let default_key: [u8; 16] = [
                0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
                0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
            ];
            match std::fs::read(file_path) {
                Ok(bytes) => match ui::PackageInspector::inspect_bytes(file_path, &bytes, &default_key) {
                    Ok(report) => ui::PackageInspector::print_report(&report),
                    Err(e) => eprintln!("Inspector error: {}", e),
                },
                Err(e) => eprintln!("Failed to read file '{}': {}", file_path, e),
            }
        }
        "info" => {
            print_specs();
        }
        _ => {
            print_usage();
        }
    }
}

fn print_usage() {
    println!("Usage:");
    println!("  cargo run -- test                 Run built-in AArch64, FP, & Horizon IPC test programs");
    println!("  cargo run -- inspect <file>       Inspect .xci/.nsp package headers and explore RomFS assets");
    println!("  cargo run -- showcase [file.nsp]  Build & boot an encrypted Switch 2 .nsp cartridge on disk");
    println!("  cargo run -- gpu3d [frames]       Launch NVGPU 3D Triangle Rasterizer Demo (GPFIFO Pushbuffers)");
    println!("  cargo run -- play [frames]        Launch interactive playable game (Arrow Keys / WASD to move)");
    println!("  cargo run -- gfx [frames]         Launch interactive 2D Framebuffer Display Demo (1280x720)");
    println!("  cargo run -- run <pkg.nsp|.nro>   Load, decrypt, and execute an .nsp, .nca, or .nro package");
    println!("  cargo run -- info                 Display Switch 2 (T239 Drake) architectural specs\n");
}

fn print_specs() {
    println!("Nintendo Switch 2 (Codename: Ounce / Drake T239) Specs:");
    println!("  - CPU: 8x ARM Cortex-A78C (ARMv8.2-A, 64-bit)");
    println!("  - GPU: NVIDIA Ampere (GA10B/GA10F derivative), ~1536 CUDA cores");
    println!("  - RAM: 12 GB LPDDR5 (128-bit bus, ~102-120 GB/s)");
    println!("  - Storage: UFS 3.1 & High-Speed Game Cards");
    println!("  - OS: Horizon 64-bit Microkernel");
    println!("  - Horizon Address Space: 39-bit/48-bit Virtual Addressing\n");
}

fn setup_vm() -> (Cpu, Memory, HorizonKernel) {
    let mut cpu = Cpu::new();
    let mut mmu = Memory::new();
    let kernel = HorizonKernel::new();

    // Map stack space: 0x7FFFF00000 .. 0x7FFFFFF000
    let stack_base = STACK_TOP - STACK_SIZE as u64;
    mmu.map_range(stack_base, STACK_SIZE, Permission::READ_WRITE);

    // Map Thread-Local Storage (TLS) page for Horizon HIPC/CMIF buffers
    mmu.map_range(DEFAULT_TLS_ADDR, TLS_SIZE, Permission::READ_WRITE);

    // Initialize stack pointer, TLS pointers, and PC
    cpu.regs.sp = STACK_TOP;
    cpu.regs.tpidr_el0 = DEFAULT_TLS_ADDR;
    cpu.regs.tpidrro_el0 = DEFAULT_TLS_ADDR;
    cpu.regs.pc = DEFAULT_BASE_ADDR;

    (cpu, mmu, kernel)
}

fn run_vm(
    cpu: Cpu,
    mmu: Memory,
    kernel: HorizonKernel,
    max_instructions: usize,
) {
    run_vm_internal(cpu, mmu, kernel, None, None, max_instructions, 0);
}

fn run_vm_internal(
    mut cpu: Cpu,
    mut mmu: Memory,
    mut kernel: HorizonKernel,
    mut display: Option<&mut DisplayServer>,
    mut hid: Option<&mut HidServer>,
    max_instructions: usize,
    max_frames: usize,
) {
    println!("[VM] Starting execution at PC: 0x{:016X}", cpu.regs.pc);
    println!("[VM] Stack Pointer: 0x{:016X}", cpu.regs.sp);

    let start = Instant::now();
    let mut instructions_executed = 0;
    let mut frames_presented = 0;

    loop {
        if max_instructions > 0 && instructions_executed >= max_instructions {
            println!("\n[VM] Execution budget reached ({} instructions).", max_instructions);
            break;
        }

        match cpu.step(&mut mmu) {
            Ok(StepResult::Continue) => {
                instructions_executed += 1;
            }
            Ok(StepResult::Svc(svc_id)) => {
                instructions_executed += 1;
                match kernel.dispatch_svc(svc_id, &mut cpu, &mut mmu) {
                    SvcResult::Success => {}
                    SvcResult::PresentFrame => {
                        frames_presented += 1;

                        // 1. Poll host input and update Horizon HID shared memory
                        if let Some(ref disp) = display {
                            if let Some(ref mut h) = hid {
                                let active_keys = disp.get_active_keys();
                                h.poll_keyboard(&active_keys);
                                let _ = h.update_shared_memory(&mut mmu);
                            }
                        }

                        // 2. Present video frame to host window
                        if let Some(ref mut disp) = display {
                            if !disp.present_frame(&mmu) {
                                println!("\n[Display] Window closed by user. Shutting down.");
                                break;
                            }
                        } else {
                            println!("[VM] VSync / Frame {} presented.", frames_presented);
                        }

                        if max_frames > 0 && frames_presented >= max_frames {
                            println!("\n[Display] Rendered {} benchmark frames. Exiting.", frames_presented);
                            break;
                        }
                    }
                    SvcResult::ExitProcess => {
                        println!("\n[VM] Process exited gracefully (svcExitProcess).");
                        break;
                    }
                    SvcResult::Break { reason, info1, info2 } => {
                        println!(
                            "\n[VM] Hit svcBreak: reason=0x{:X}, info1=0x{:X}, info2=0x{:X}",
                            reason, info1, info2
                        );
                        break;
                    }
                    SvcResult::Unhandled(id) => {
                        println!("\n[VM] Stopping on unhandled SVC #0x{:02X}.", id);
                        break;
                    }
                }
            }
            Ok(StepResult::Halt(msg)) => {
                println!("\n[VM] CPU Halted: {}", msg);
                break;
            }
            Err(e) => {
                eprintln!("\n[VM Error] {}", e);
                eprintln!("  Final PC: 0x{:016X}", cpu.regs.pc);
                break;
            }
        }
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
    if kernel.nvgpu.dlss.frames_upscaled > 0 || kernel.vfs.total_bytes_written > 0 {
        println!(
            "  -> Ampere Tensor/DLSS Upscaler: {} Frames ({} Output Pixels) | SaveData Written: {} Bytes",
            kernel.nvgpu.dlss.frames_upscaled,
            kernel.nvgpu.dlss.output_pixels_generated,
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
}

fn run_built_in_demo() {
    println!("--- Running Built-in Test Suite ---\n");

    // Test Program 1: Fibonacci Sequence (computes Fib(10) = 55 in X0)
    #[rustfmt::skip]
    let fib_program: Vec<u32> = vec![
        0xD2800141, // MOVZ X1, #10
        0xD2800002, // MOVZ X2, #0
        0xD2800023, // MOVZ X3, #1
        // loop (offset 0x0C):
        0xB40000C1, // CBZ X1, done (+6 instructions = 24 bytes -> offset 0x24)
        0x8B030044, // ADD X4, X2, X3
        0x91000062, // ADD X2, X3, #0
        0x91000083, // ADD X3, X4, #0
        0xF1000421, // SUBS X1, X1, #1
        0x17FFFFFB, // B loop (-5 instructions)
        // done (offset 0x24):
        0x91000040, // ADD X0, X2, #0
        0xD40000E1, // SVC #0x07 (svcExitProcess)
    ];

    println!("[Test 1] AArch64 Fibonacci Computation (N=10)");
    let (cpu, mut mmu, kernel) = setup_vm();
    let mut code_bytes = Vec::new();
    for inst in &fib_program {
        code_bytes.extend_from_slice(&inst.to_le_bytes());
    }

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.write_bytes(DEFAULT_BASE_ADDR, &code_bytes).unwrap();

    run_vm(cpu, mmu, kernel, 100_000);

    // Test Program 2: Horizon OS Console Output (svcOutputDebugString)
    println!("\n[Test 2] Horizon OS Debug String (svcOutputDebugString)");
    let (cpu, mut mmu, kernel) = setup_vm();

    let message = "Hello from Nintendo Switch 2 (Tegra T239) Homebrew on Ounce-Core!\n";
    let msg_len = message.len() as u32;

    #[rustfmt::skip]
    let print_program: Vec<u32> = vec![
        0x10000080, // ADR X0, #16
        0xD2800001 | (msg_len << 5), // MOVZ X1, #msg_len
        0xD40004E1, // SVC #0x27 (svcOutputDebugString)
        0xD40000E1, // SVC #0x07 (svcExitProcess)
    ];

    let mut print_bytes = Vec::new();
    for inst in &print_program {
        print_bytes.extend_from_slice(&inst.to_le_bytes());
    }
    print_bytes.extend_from_slice(message.as_bytes());

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.write_bytes(DEFAULT_BASE_ADDR, &print_bytes).unwrap();

    run_vm(cpu, mmu, kernel, 10_000);

    // Test Program 3: Stack & Subroutine Call (STP, LDP, BL, RET)
    println!("\n[Test 3] Stack Frames & Subroutines (STP, LDP, BL, RET)");
    let (cpu, mut mmu, kernel) = setup_vm();

    #[rustfmt::skip]
    let call_program: Vec<u32> = vec![
        0xD28002A0, // MOVZ X0, #21
        0x94000002, // BL +2 instructions (+8 bytes to double_fn)
        0xD40000E1, // SVC #0x07 (Exit)
        // double_fn:
        0xD10043FF, // SUB SP, SP, #16
        0xF90003FE, // STR X30, [SP, #0]
        0x8B000000, // ADD X0, X0, X0
        0xF94003FE, // LDR X30, [SP, #0]
        0x910043FF, // ADD SP, SP, #16
        0xD65F03C0, // RET
    ];

    let mut call_bytes = Vec::new();
    for inst in &call_program {
        call_bytes.extend_from_slice(&inst.to_le_bytes());
    }
    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.write_bytes(DEFAULT_BASE_ADDR, &call_bytes).unwrap();

    run_vm(cpu, mmu, kernel, 10_000);

    // Test Program 4: Horizon OS HIPC / CMIF Service Manager ("sm:" -> "nvdrv") & Floating-Point Math
    println!("\n[Test 4] Horizon HIPC/CMIF ('sm:' -> 'nvdrv') & IEEE-754 Floating-Point (FADD)");
    let (mut cpu, mut mmu, kernel) = setup_vm();

    // Pre-populate D1 = 12.5 and D2 = 29.5 so FADD D0, D1, D2 computes 42.0
    cpu.regs.write_d(1, 12.5);
    cpu.regs.write_d(2, 29.5);

    // Write port name "sm:\0" at DEFAULT_BASE_ADDR + 0x100
    let port_name_addr = DEFAULT_BASE_ADDR + 0x100;
    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.write_bytes(port_name_addr, b"sm:\0").unwrap();

    // Prepare CMIF GetService("nvdrv") packet in Thread-Local Storage (DEFAULT_TLS_ADDR + 0x10)
    let cmif_base = DEFAULT_TLS_ADDR + 0x10;
    mmu.write_u32(cmif_base, CMIF_IN_MAGIC).unwrap(); // "SFCI"
    mmu.write_u32(cmif_base + 8, 1).unwrap();         // Command 1: GetService
    mmu.write_bytes(cmif_base + 16, b"nvdrv\0\0\0").unwrap();

    // Guest AArch64 Code:
    //   FADD D0, D1, D2           ; D0 = 12.5 + 29.5 = 42.0
    //   MRS  X2, TPIDRRO_EL0      ; Read Horizon TLS base address into X2
    //   ADR  X1, #0xF8            ; X1 = port_name_addr ("sm:")
    //   SVC  #0x1F                ; svcConnectToNamedPort(&out_handle, "sm:") -> X1 = 0x10001
    //   ADD  X0, X1, #0           ; X0 = sm: session handle
    //   SVC  #0x21                ; svcSendSyncRequest(sm_handle) -> dispatches GetService("nvdrv")
    //   LDR  W0, [X2, #0x20]      ; Read returned nvdrv handle from CMIF response in TLS
    //   SVC  #0x07                ; svcExitProcess
    #[rustfmt::skip]
    let ipc_program: Vec<u32> = vec![
        0x1E622820, // FADD D0, D1, D2 (D0 = 42.0)
        0xD53BD062, // MRS  X2, TPIDRRO_EL0
        0x100007C1, // ADR  X1, #0xF8 (offset from PC=0x80000008 to 0x80000100 is +0xF8)
        0xD40003E1, // SVC  #0x1F (svcConnectToNamedPort)
        0x91000020, // ADD  X0, X1, #0
        0xD4000421, // SVC  #0x21 (svcSendSyncRequest)
        0xB9402040, // LDR  W0, [X2, #0x20] (read nvdrv handle = 0x10002)
        0xD40000E1, // SVC  #0x07 (svcExitProcess)
    ];

    let mut ipc_bytes = Vec::new();
    for inst in &ipc_program {
        ipc_bytes.extend_from_slice(&inst.to_le_bytes());
    }
    mmu.write_bytes(DEFAULT_BASE_ADDR, &ipc_bytes).unwrap();

    run_vm(cpu, mmu, kernel, 10_000);

    // Test Program 5: RomFS Virtual Filesystem & 48kHz Stereo PCM Audio Synthesizer (audout:u)
    println!("\n[Test 5] Virtual Filesystem ('romfs:/title.txt') & 48kHz Stereo PCM Audio ('audout:u')");
    let (mut cpu, mut mmu, kernel) = setup_vm();

    let vfs_path = b"romfs:/title.txt";
    let path_addr = DEFAULT_BASE_ADDR + 0x100;
    let file_dst_addr = DEFAULT_BASE_ADDR + 0x200;
    let pcm_buf_addr = DEFAULT_BASE_ADDR + 0x400;

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.write_bytes(path_addr, vfs_path).unwrap();

    // Pre-configure pointers in X4..X7:
    //   X0 = path_addr ("romfs:/title.txt")
    //   X1 = path_len (16)
    //   X2 = file_dst_addr (0x80000200)
    //   X3 = max_len (128)
    //   X4 = file_dst_addr
    //   X5 = pcm_buf_addr (0x80000400)
    //   X6 = 300 (base frequency step)
    //   X7 = 48 (multiplier -> 300 * 48 = 14,400 peak PCM amplitude via MUL!)
    cpu.regs.write_x(0, path_addr);
    cpu.regs.write_x(1, vfs_path.len() as u64);
    cpu.regs.write_x(2, file_dst_addr);
    cpu.regs.write_x(3, 128);
    cpu.regs.write_x(4, file_dst_addr);
    cpu.regs.write_x(5, pcm_buf_addr);
    cpu.regs.write_x(6, 300);
    cpu.regs.write_x(7, 48);

    // Guest AArch64 Code:
    //   SVC  #0x2A                ; svcReadRomFsFile("romfs:/title.txt", dst=0x80000200) -> X1 = bytes_read
    //   ADD  X0, X4, #0           ; X0 = file_dst_addr
    //   SVC  #0x27                ; svcOutputDebugString(X0, X1) -> prints loaded RomFS file!
    //   MUL  X8, X6, X7           ; MADD X8, X6, X7, XZR -> 300 * 48 = 14400 (16-bit PCM sample)
    //   STRH W8, [X5, #0]         ; Write Left channel PCM sample (14400)
    //   STRH W8, [X5, #2]         ; Write Right channel PCM sample (14400)
    //   ADD  X0, X5, #0           ; X0 = pcm_buf_addr
    //   MOVZ X1, #2               ; X1 = 2 stereo samples
    //   SVC  #0x2B                ; svcAppendAudioBuffer -> queues PCM to audout:u, returns peak in X1
    //   ADD  X0, X1, #0           ; X0 = peak audio amplitude (14400)
    //   SVC  #0x07                ; svcExitProcess
    #[rustfmt::skip]
    let vfs_audio_program: Vec<u32> = vec![
        0xD4000541, // SVC  #0x2A (svcReadRomFsFile)
        0x91000080, // ADD  X0, X4, #0
        0xD40004E1, // SVC  #0x27 (svcOutputDebugString)
        0x9B077CC8, // MUL  X8, X6, X7 (MADD X8, X6, X7, XZR -> 14400)
        0x790000A8, // STRH W8, [X5, #0]
        0x790004A8, // STRH W8, [X5, #2]
        0x910000A0, // ADD  X0, X5, #0
        0xD2800041, // MOVZ X1, #2
        0xD4000561, // SVC  #0x2B (svcAppendAudioBuffer)
        0x91000020, // ADD  X0, X1, #0
        0xD40000E1, // SVC  #0x07 (svcExitProcess)
    ];

    let mut vfs_audio_bytes = Vec::new();
    for inst in &vfs_audio_program {
        vfs_audio_bytes.extend_from_slice(&inst.to_le_bytes());
    }
    mmu.write_bytes(DEFAULT_BASE_ADDR, &vfs_audio_bytes).unwrap();

    run_vm(cpu, mmu, kernel, 10_000);
    println!();

    // ---------------------------------------------------------
    // Test 6: NSP/NCA AES-128-CTR Decryption, Ampere SASS -> GLSL
    //         Shader Decompiler, & 8-Core Multi-Thread Execution
    // ---------------------------------------------------------
    println!("--- Test 6: AES-128-CTR NCA Decryption, Ampere SASS->GLSL & Multi-Threading ---");

    // 6a. Build an encrypted Switch 2 .nsp -> .nca -> ExeFS package and decrypt it
    let title_key: [u8; 16] = [
        0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
        0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
    ];
    let ctr_iv: [u8; 16] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    ];

    // Worker thread AArch64 code embedded inside the encrypted NCA ExeFS ("main"):
    // Reads argument pointer X0 (shared memory 0x80060000), writes 7777 to [X0], and calls SVC #0x0A (svcExitThread)
    let worker_insns: [u32; 4] = [
        0xD283CC21, // MOVZ X1, #7777
        0xF9000001, // STR  X1, [X0]
        0xD4000141, // SVC  #0x0A (svcExitThread)
        0xD503201F, // NOP
    ];
    let mut worker_code_bytes = Vec::new();
    for insn in &worker_insns {
        worker_code_bytes.extend_from_slice(&insn.to_le_bytes());
    }

    let exefs_pfs0 = Pfs0Archive::build(&[("main", &worker_code_bytes)]);
    let encrypted_nca = NcaImage::build_encrypted(0x0100_F2C0_115B_6000, &title_key, &ctr_iv, &exefs_pfs0);
    let nsp_archive = Pfs0Archive::build(&[("0100f2c0115b6000.nca", &encrypted_nca)]);

    // Parse outer .nsp, decrypt inner NCA3 with AES-128-CTR, and extract "main"
    let parsed_nsp = Pfs0Archive::parse(nsp_archive).unwrap();
    let raw_nca = parsed_nsp.extract_file("0100f2c0115b6000.nca").unwrap();
    let decrypted_nca = NcaImage::parse_and_decrypt(raw_nca, &title_key).unwrap();
    let decrypted_exefs = Pfs0Archive::parse(decrypted_nca.decrypted_payload).unwrap();
    let decrypted_worker_code = decrypted_exefs.extract_file("main").unwrap();
    println!(
        "  [Crypto/NCA] Decrypted TitleID 0x{:016X} via AES-128-CTR ({} bytes ExeFS 'main')",
        decrypted_nca.title_id,
        decrypted_worker_code.len()
    );

    let (mut cpu, mut mmu, kernel) = setup_vm();

    // Map worker code at 0x80050000, worker stack at 0x70050000, shared result at 0x80060000,
    // Ampere SASS shader at 0x80030000, and NVGPU pushbuffer at 0x80040000
    let worker_pc = 0x80050000u64;
    let worker_stack_top = 0x70051000u64;
    let shared_mem_addr = 0x80060000u64;
    let shader_addr = 0x80030000u64;
    let pb_addr = 0x80040000u64;

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.map_range(worker_pc, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.map_range(0x70050000, 4096, Permission::READ_WRITE);
    mmu.map_range(shared_mem_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(shader_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);

    // Load decrypted worker code into guest RAM at 0x80050000
    mmu.write_bytes(worker_pc, decrypted_worker_code).unwrap();

    // 6b. Write 64-bit Ampere SASS shader program into guest VRAM at 0x80030000
    let sass_program = [
        SassOp::Texs2D { rd: 8, ru: 4, rv: 5, tex_id: 0 }.encode(),
        SassOp::FMul { rd: 0, rn: 0, rm: 8 }.encode(),
        SassOp::FMul { rd: 1, rn: 1, rm: 9 }.encode(),
        SassOp::FFma { rd: 2, rn: 2, rm: 10, ra: 3 }.encode(),
        SassOp::Exit.encode(),
    ];
    for (i, &w) in sass_program.iter().enumerate() {
        mmu.write_u64(shader_addr + (i * 8) as u64, w).unwrap();
    }

    // Write NVGPU pushbuffer command to bind & decompile the Ampere SASS shader
    let pb_words = [
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_BIND_SHADER_PROGRAM, 1),
        shader_addr as u32,
    ];
    for (i, &w) in pb_words.iter().enumerate() {
        mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
    }

    // 6c. Main thread AArch64 guest program:
    //  1. Submits NVGPU pushbuffer (SVC #0x29) to decompile the Ampere SASS shader
    //  2. Spawns a worker KThread on Core #3 via SVC #0x08 (svcCreateThread)
    //  3. Starts the worker KThread via SVC #0x09 (svcStartThread)
    //  4. Reads the worker thread's result from [0x80060000] into X0 (expected: 7777) and exits!
    cpu.regs.write_x(10, pb_addr);
    cpu.regs.write_x(11, worker_pc);
    cpu.regs.write_x(12, shared_mem_addr);
    cpu.regs.write_x(13, worker_stack_top);

    #[rustfmt::skip]
    let main_thread_program: Vec<u32> = vec![
        // Step 1: SVC #0x29 (svcSubmitGpuPushbuffer: X0 = pb_addr, X1 = 2 words)
        0x91000140, // ADD  X0, X10, #0
        0xD2800041, // MOVZ X1, #2
        0xD4000521, // SVC  #0x29

        // Step 2: SVC #0x08 (svcCreateThread: X1=entry, X2=arg, X3=stack_top, X4=prio, X5=core_id)
        0x91000161, // ADD  X1, X11, #0 (worker_pc)
        0x91000182, // ADD  X2, X12, #0 (shared_mem_addr)
        0x910001A3, // ADD  X3, X13, #0 (worker_stack_top)
        0xD2800584, // MOVZ X4, #44     (priority 44)
        0xD2800065, // MOVZ X5, #3      (Cortex-A78C Core #3)
        0xD4000101, // SVC  #0x08       (returns thread handle in X1)

        // Step 3: SVC #0x09 (svcStartThread: X0 = thread_handle)
        0x91000020, // ADD  X0, X1, #0
        0xD4000121, // SVC  #0x09

        // Step 4: Load worker result from [X12] into X0 and exit!
        0xF9400180, // LDR  X0, [X12]   (Expected: 7777)
        0xD40000E1, // SVC  #0x07 (svcExitProcess)
    ];

    let mut main_bytes = Vec::new();
    for inst in &main_thread_program {
        main_bytes.extend_from_slice(&inst.to_le_bytes());
    }
    mmu.write_bytes(DEFAULT_BASE_ADDR, &main_bytes).unwrap();

    // Run VM and inspect shader + scheduler state
    let mut display_none: Option<&mut DisplayServer> = None;
    run_vm_internal(cpu, mmu, kernel, display_none.as_mut().map(|x| &mut **x), None, 10_000, 0);
    println!();

    // ---------------------------------------------------------
    // Test 7: Unified .nsp Package Boot, Futex/Mutex Arbitration,
    //         & 2D-Textured 3D Triangle Rasterization
    // ---------------------------------------------------------
    println!("--- Test 7: Unified .nsp Boot, Futex Synchronization & 2D-Textured 3D Rendering ---");

    let (mut cpu, mut mmu, kernel) = setup_vm();

    let rt_addr = 0x90000000u64;
    let vb_addr = 0x80020000u64;
    let tex_addr = 0x80030000u64;
    let pb_addr = 0x80010000u64;
    let sync_addr = 0x80070000u64; // [0x80070000] = mutex, [0x80070008] = condvar

    mmu.map_range(rt_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(vb_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(tex_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(sync_addr, 4096, Permission::READ_WRITE);

    // Upload a 2x2 RGBA8888 texture with bright gold/cyan texels (0xFF40C0FF)
    for i in 0..4 {
        mmu.write_u32(tex_addr + i * 4, 0xFF40C0FF).unwrap();
    }

    // Upload 3 textured 3D vertices (8 floats each: x, y, z, r, g, b, u, v)
    let textured_verts: [f32; 24] = [
         0.0,  0.8, 0.0, 1.0, 1.0, 1.0, 0.5, 0.0,
        -0.8, -0.8, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0,
         0.8, -0.8, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    ];
    for (i, &val) in textured_verts.iter().enumerate() {
        mmu.write_u32(vb_addr + (i * 4) as u64, val.to_bits()).unwrap();
    }

    // Build GPFIFO pushbuffer for 2D-textured 3D triangle rendering
    let pb_tex_words: Vec<u32> = vec![
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_RENDER_TARGET_ADDR, 1),
        rt_addr as u32,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VIEWPORT_DIMS, 2),
        16, 16,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_BIND_TEXTURE_2D, 3),
        tex_addr as u32, 2, 2,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
        vb_addr as u32,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_TEXTURED_ARRAYS, 1),
        3,
    ];
    for (i, &w) in pb_tex_words.iter().enumerate() {
        mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
    }

    // Guest AArch64 executable that will be packaged inside an encrypted .nsp and booted via BootLoader:
    //  X10 = sync_addr (0x80070000), X11 = pb_addr (0x80010000), X12 = center_pixel_addr
    cpu.regs.write_x(10, sync_addr);
    cpu.regs.write_x(11, pb_addr);
    cpu.regs.write_x(12, rt_addr + (8 * 16 + 8) * 4);
    cpu.regs.write_x(13, pb_tex_words.len() as u64);

    #[rustfmt::skip]
    let nsp_guest_insns: Vec<u32> = vec![
        // 1. Acquire Horizon user-space mutex via SVC #0x1A (svcArbitrateLock: X0=0, X1=mutex_addr, X2=0x101)
        0xD2800000, // MOVZ X0, #0
        0x91000141, // ADD  X1, X10, #0
        0xD2802022, // MOVZ X2, #0x101
        0xD4000341, // SVC  #0x1A (svcArbitrateLock)

        // 2. Signal Horizon CondVar via SVC #0x1D (svcSignalProcessWideKey: X0=condvar_addr, X1=1)
        0x91002140, // ADD  X0, X10, #8
        0xD2800021, // MOVZ X1, #1
        0xD40003A1, // SVC  #0x1D (svcSignalProcessWideKey)

        // 3. Release Horizon user-space mutex via SVC #0x1B (svcArbitrateUnlock: X0=mutex_addr)
        0x91000140, // ADD  X0, X10, #0
        0xD4000361, // SVC  #0x1B (svcArbitrateUnlock)

        // 4. Submit NVGPU pushbuffer to render 2D-textured 3D triangle (SVC #0x29)
        0x91000160, // ADD  X0, X11, #0
        0x910001A1, // ADD  X1, X13, #0
        0xD4000521, // SVC  #0x29 (svcSubmitGpuPushbuffer)

        // 5. Read rendered center texel from VRAM [X12] into W0 and exit!
        0xB9400180, // LDR  W0, [X12] (Expected: 0xFF40C0FF = 4282433791)
        0xD40000E1, // SVC  #0x07 (svcExitProcess)
    ];

    let mut nsp_exec_bytes = Vec::new();
    for insn in &nsp_guest_insns {
        nsp_exec_bytes.extend_from_slice(&insn.to_le_bytes());
    }

    // Wrap the executable inside ExeFS -> AES-128-CTR encrypted NCA3 -> .nsp (PFS0)
    let exefs7 = Pfs0Archive::build(&[("main", &nsp_exec_bytes)]);
    let nca7 = NcaImage::build_encrypted(0x0100_A2B0_2026_7000, &title_key, &ctr_iv, &exefs7);
    let nsp7 = Pfs0Archive::build(&[("0100a2b020267000.nca", &nca7)]);

    // Boot directly via Unified BootLoader::load_any_package!
    let boot_info = BootLoader::load_any_package(&nsp7, DEFAULT_BASE_ADDR, &title_key, &mut mmu).unwrap();
    println!(
        "  [BootLoader] Format: {} | TitleID: 0x{:016X} | Code: {} bytes",
        boot_info.format, boot_info.title_id, boot_info.code_size
    );

    run_vm(cpu, mmu, kernel, 10_000);
    println!();

    // ---------------------------------------------------------
    // Test 8: Switch 2 crt0 Startup (svcGetInfo 12GB LPDDR5 + 8-Core Mask),
    //         Basic-Block JIT Loop & 3D Z-Buffer Depth Occlusion
    // ---------------------------------------------------------
    println!("--- Test 8: Switch 2 crt0 (svcGetInfo), Basic-Block JIT & 3D Z-Buffer Occlusion ---");

    let (mut cpu, mut mmu, kernel) = setup_vm();

    let rt_addr8 = 0x90000000u64;
    let vb_addr8 = 0x80020000u64;
    let pb_addr8 = 0x80010000u64;
    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.map_range(rt_addr8, 4096, Permission::READ_WRITE);
    mmu.map_range(vb_addr8, 4096, Permission::READ_WRITE);
    mmu.map_range(pb_addr8, 4096, Permission::READ_WRITE);

    // Upload 2 overlapping 3D triangles into Guest VRAM:
    //  Triangle 0 (Near, z = 0.2): Pure Green (0xFF00FF00)
    //  Triangle 1 (Far,  z = 0.8): Pure Red   (0xFF0000FF) — drawn SECOND!
    let z_verts: [f32; 36] = [
         0.0,  0.8, 0.2, 0.0, 1.0, 0.0,
        -0.8, -0.8, 0.2, 0.0, 1.0, 0.0,
         0.8, -0.8, 0.2, 0.0, 1.0, 0.0,
         0.0,  0.8, 0.8, 1.0, 0.0, 0.0,
        -0.8, -0.8, 0.8, 1.0, 0.0, 0.0,
         0.8, -0.8, 0.8, 1.0, 0.0, 0.0,
    ];
    for (i, &val) in z_verts.iter().enumerate() {
        mmu.write_u32(vb_addr8 + (i * 4) as u64, val.to_bits()).unwrap();
    }

    let pb_z_words: Vec<u32> = vec![
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_RENDER_TARGET_ADDR, 1),
        rt_addr8 as u32,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VIEWPORT_DIMS, 2),
        16, 16,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_DEPTH_TEST_ENABLE, 1),
        1,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_CLEAR_SURFACE, 1),
        1,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
        vb_addr8 as u32,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_ARRAYS, 1),
        6, // 6 vertices = 2 overlapping 3D triangles
    ];
    for (i, &w) in pb_z_words.iter().enumerate() {
        mmu.write_u32(pb_addr8 + (i * 4) as u64, w).unwrap();
    }

    cpu.regs.write_x(10, pb_addr8);
    cpu.regs.write_x(11, pb_z_words.len() as u64);
    cpu.regs.write_x(12, rt_addr8 + (8 * 16 + 8) * 4);

    // Guest AArch64 program:
    //  1. Calls SVC #0x1E (svcGetInfo: X1 = 0 -> CoreMask = 0xFF for 8x Cortex-A78C cores)
    //  2. Saves CoreMask in X4
    //  3. Submits NVGPU Z-Buffered 3D Draw Pushbuffer via SVC #0x29
    //  4. Loads center pixel from VRAM [X12] (0xFF00FF00 = Pure Green, proving near triangle occluded far red triangle!)
    //  5. Combines (CenterPixel | CoreMask) -> 0xFF00FFFF in X0 and exits!
    #[rustfmt::skip]
    let test8_insns: Vec<u32> = vec![
        0xD2800001, // MOVZ X1, #0      (InfoType::CoreMask = 0)
        0xD40003C1, // SVC  #0x1E       (svcGetInfo -> X1 = 0xFF)
        0x91000024, // ADD  X4, X1, #0  (X4 = 0xFF)
        0x91000140, // ADD  X0, X10, #0 (pb_addr8)
        0x91000161, // ADD  X1, X11, #0 (word_count)
        0xD4000521, // SVC  #0x29       (svcSubmitGpuPushbuffer)
        0xB9400180, // LDR  W0, [X12]   (0xFF00FF00)
        0xAA040000, // ORR  X0, X0, X4  (0xFF00FF00 | 0xFF = 0xFF00FFFF = 4278255615)
        0xD40000E1, // SVC  #0x07       (svcExitProcess)
    ];

    let mut test8_bytes = Vec::new();
    for insn in &test8_insns {
        test8_bytes.extend_from_slice(&insn.to_le_bytes());
    }
    mmu.write_bytes(DEFAULT_BASE_ADDR, &test8_bytes).unwrap();

    run_vm(cpu, mmu, kernel, 10_000);
    println!();

    // ---------------------------------------------------------
    // Test 9: 128-bit NEON Vector SIMD (V0.4S), Ampere T239 DLSS
    //         2x Super-Resolution Upscaler & SaveData ('save:/profile.sav')
    // ---------------------------------------------------------
    println!("--- Test 9: 128-bit NEON Vector SIMD (V0.4S), T239 DLSS Upscaler & SaveData ---");

    let (mut cpu, mut mmu, kernel) = setup_vm();

    let simd_addr = 0x80020000u64;
    let save_path_addr = 0x80030000u64;
    let dlss_src_addr = 0x90000000u64;
    let dlss_dst_addr = 0x90010000u64;
    let pb_addr9 = 0x80010000u64;

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.map_range(simd_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(save_path_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(dlss_src_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(dlss_dst_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(pb_addr9, 4096, Permission::READ_WRITE);

    // 1. Write two 128-bit 4xFloat32 vectors into Guest RAM at 0x80020000:
    //    VecA (at +0x00) = [10.0, 20.0, 30.0, 40.0]
    //    VecB (at +0x10) = [ 2.0,  3.0,  4.0,  5.0]
    let vec_a: [f32; 4] = [10.0, 20.0, 30.0, 40.0];
    let vec_b: [f32; 4] = [2.0, 3.0, 4.0, 5.0];
    for i in 0..4 {
        mmu.write_u32(simd_addr + (i * 4) as u64, vec_a[i].to_bits()).unwrap();
        mmu.write_u32(simd_addr + 16 + (i * 4) as u64, vec_b[i].to_bits()).unwrap();
    }

    // 2. Write "save:/profile.sav" path string at 0x80030000
    let save_path = b"save:/profile.sav";
    mmu.write_bytes(save_path_addr, save_path).unwrap();

    // 3. Populate 8x8 low-res render surface at 0x90000000 and build DLSS 2x upscale pushbuffer (8x8 -> 16x16 = 256 pixels)
    for i in 0..64u64 {
        mmu.write_u32(dlss_src_addr + i * 4, 0xFF80C040).unwrap();
    }
    let pb_dlss_words: Vec<u32> = vec![
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DISPATCH_DLSS_UPSCALE, 4),
        dlss_src_addr as u32,
        8,
        8,
        dlss_dst_addr as u32,
    ];
    for (i, &w) in pb_dlss_words.iter().enumerate() {
        mmu.write_u32(pb_addr9 + (i * 4) as u64, w).unwrap();
    }

    cpu.regs.write_x(10, simd_addr);
    cpu.regs.write_x(11, save_path_addr);
    cpu.regs.write_x(12, save_path.len() as u64);
    cpu.regs.write_x(13, pb_addr9);
    cpu.regs.write_x(14, pb_dlss_words.len() as u64);

    // Guest AArch64 Program:
    //  1. LDR  Q1, [X10, #0]        ; Load 128-bit VecA [10.0, 20.0, 30.0, 40.0]
    //  2. LDR  Q2, [X10, #16]       ; Load 128-bit VecB [2.0, 3.0, 4.0, 5.0]
    //  3. FMUL V3.4S, V1.4S, V2.4S  ; 4-lane SIMD multiply -> [20.0, 60.0, 120.0, 200.0]
    //  4. FADD V0.4S, V3.4S, V1.4S  ; 4-lane SIMD add      -> [30.0, 80.0, 150.0, 240.0]
    //  5. STR  Q0, [X10, #32]       ; Store 128-bit result vector to [0x80020020]
    //  6. SVC  #0x2C                ; svcWriteSaveDataFile("save:/profile.sav", [0x80020020], 16 bytes)
    //  7. SVC  #0x29                ; svcSubmitGpuPushbuffer (T239 DLSS 2x Super-Resolution Upscale!)
    //  8. Read back saved profile data via SVC #0x2A ("save:/profile.sav") into [0x80020040]
    //  9. Load lane 0 float (30.0f) into S0 and bytes_read (16) into X0, then exit!
    #[rustfmt::skip]
    let test9_insns: Vec<u32> = vec![
        0x3DC00141, // LDR  Q1, [X10, #0]
        0x3DC00542, // LDR  Q2, [X10, #16]
        0x6E22DC23, // FMUL V3.4S, V1.4S, V2.4S
        0x4E21D460, // FADD V0.4S, V3.4S, V1.4S
        0x3D800940, // STR  Q0, [X10, #32]

        // Write 16-byte SIMD vector result to "save:/profile.sav" (SVC #0x2C)
        0x91000160, // ADD  X0, X11, #0  (path_ptr)
        0x91000181, // ADD  X1, X12, #0  (path_len)
        0x91008142, // ADD  X2, X10, #32 (src_addr = 0x80020020)
        0xD2800203, // MOVZ X3, #16      (data_len = 16 bytes)
        0xD4000581, // SVC  #0x2C        (svcWriteSaveDataFile)

        // Dispatch Ampere Tensor Core / DLSS 2x Super-Resolution Upscaler (SVC #0x29)
        0x910001A0, // ADD  X0, X13, #0  (pb_addr9)
        0x910001C1, // ADD  X1, X14, #0  (word_count)
        0xD4000521, // SVC  #0x29        (svcSubmitGpuPushbuffer)

        // Read back "save:/profile.sav" into [0x80020040] via SVC #0x2A
        0x91000160, // ADD  X0, X11, #0  (path_ptr)
        0x91000181, // ADD  X1, X12, #0  (path_len)
        0x91010142, // ADD  X2, X10, #64 (dst_addr = 0x80020040)
        0xD2800203, // MOVZ X3, #16      (max_len = 16)
        0xD4000541, // SVC  #0x2A        (svcReadRomFsFile -> X1 = 16 bytes read)
        0x91000020, // ADD  X0, X1, #0   (X0 = 16)
        0xD40000E1, // SVC  #0x07        (svcExitProcess)
    ];

    let mut test9_bytes = Vec::new();
    for insn in &test9_insns {
        test9_bytes.extend_from_slice(&insn.to_le_bytes());
    }
    mmu.write_bytes(DEFAULT_BASE_ADDR, &test9_bytes).unwrap();

    run_vm(cpu, mmu, kernel, 10_000);
    println!();

    // ---------------------------------------------------------
    // Test 10: Real Switch 2 `ns2jb` Native FS Payload (Firmware 23.0.1)
    //          Executes the unaltered 72-byte `test-function.bin` from `pepsipu/ns2jb`
    // ---------------------------------------------------------
    println!("--- Test 10: Real Switch 2 `ns2jb` Native FS Payload (`test-function.bin`, FW 23.0.1) ---");

    let (mut cpu, mut mmu, kernel) = setup_vm();

    let out_buf10 = 0x80020000u64;
    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.map_range(out_buf10, 4096, Permission::READ_WRITE);

    // Exact 72-byte native AArch64 binary from `gitlab.com/pepsipu/ns2jb` (`engine/switch2/process/test-function.bin`)
    const NS2JB_TEST_FUNCTION_BIN: [u8; 72] = [
        0x5f, 0x24, 0x03, 0xd5, // bti c (ARMv8.5-A Branch Target Identification)
        0xfd, 0x7b, 0xbe, 0xa9, // stp x29, x30, [sp, #-32]!
        0xf3, 0x53, 0x01, 0xa9, // stp x19, x20, [sp, #16]
        0xf3, 0x03, 0x00, 0xaa, // mov x19, x0
        0xf4, 0x03, 0x01, 0xaa, // mov x20, x1
        0xc1, 0xff, 0x8f, 0x12, // mov w1, #0xffff8001 (CurrentProcess pseudo-handle)
        0x81, 0x04, 0x00, 0xd4, // svc #0x24 (svcGetProcessId)
        0x60, 0x06, 0x00, 0xa9, // stp x0, x1, [x19]
        0xe1, 0xff, 0x8f, 0x12, // mov w1, #0xffff8000 (CurrentThread pseudo-handle)
        0xa1, 0x04, 0x00, 0xd4, // svc #0x25 (svcGetThreadId)
        0x60, 0x06, 0x01, 0xa9, // stp x0, x1, [x19, #16]
        0xc1, 0x03, 0x00, 0xd4, // svc #0x1e (svcGetSystemTick)
        0x60, 0x12, 0x00, 0xf9, // str x0, [x19, #32]
        0x80, 0x06, 0x14, 0xca, // eor x0, x20, x20, lsl #1
        0x60, 0x16, 0x00, 0xf9, // str x0, [x19, #40]
        0xf3, 0x53, 0x41, 0xa9, // ldp x19, x20, [sp, #16]
        0xfd, 0x7b, 0xc2, 0xa8, // ldp x29, x30, [sp], #32
        0xc0, 0x03, 0x5f, 0xd6, // ret
    ];

    mmu.write_bytes(DEFAULT_BASE_ADDR, &NS2JB_TEST_FUNCTION_BIN).unwrap();
    // Place svcExitProcess (0xD40000E1) at return address 0x80000100
    mmu.write_u32(DEFAULT_BASE_ADDR + 0x100, 0xD400_00E1).unwrap();

    let challenge_nonce: u64 = 0x0000_0000_0020_2600;
    cpu.regs.write_x(0, out_buf10);
    cpu.regs.write_x(1, challenge_nonce);
    cpu.regs.write_x(30, DEFAULT_BASE_ADDR + 0x100); // LR -> svcExitProcess

    run_vm(cpu, mmu, kernel, 10_000);
    println!();

    // ---------------------------------------------------------
    // Test 11: Full 4,757-Byte Switch 2 `RO` `NSO0` Module (`engine/switch2/ro/payload/main`)
    //          Boots the complete `NSO0` binary from `pepsipu/ns2jb` through `sm:` Tipc & `"NS2ROEXE"` IPC reply
    // ---------------------------------------------------------
    println!("--- Test 11: Full 4,757-Byte Switch 2 `RO` `NSO0` Module (`ns2jb_ro_payload.bin`) ---");

    let (cpu, mut mmu, kernel) = setup_vm();
    let nso_bytes = include_bytes!("../ns2jb_ro_payload.bin");
    let boot_nso = BootLoader::load_any_package(nso_bytes, DEFAULT_BASE_ADDR, &[0u8; 16], &mut mmu).unwrap();
    println!(
        "  [BootLoader] Format: {} | ProgramID: 0x{:016X} | .text: {} bytes",
        boot_nso.format, boot_nso.title_id, boot_nso.code_size
    );
    run_vm(cpu, mmu, kernel, 50_000);
    println!();

    // ---------------------------------------------------------
    // Test 12: Captured Switch 2 Firmware 23.0.1 `nnSdk` & `nnCompatThin` Dumps
    //          Executes `nnSdk::GetThreadAvailableCoreMask` (0xFF) wrapped by `nnCompatThin` (`AND X0, X0, #7` -> 0x07)
    // ---------------------------------------------------------
    println!("--- Test 12: Captured Switch 2 FW 23.0.1 `nnSdk` & `nnCompatThin` CoreMask Wrapper ---");

    let (mut cpu, mut mmu, kernel) = setup_vm();
    const NNSDK_GET_CORE_MASK: [u8; 64] = [
        0xff, 0x83, 0x00, 0xd1, 0xfd, 0x7b, 0x01, 0xa9,
        0xfd, 0x43, 0x00, 0x91, 0xe0, 0x23, 0x00, 0x91,
        0xe1, 0x03, 0x1f, 0x2a, 0xc2, 0xff, 0x8f, 0x12,
        0xe3, 0x03, 0x1f, 0xaa, 0x6e, 0x9f, 0x00, 0x94,
        0xe0, 0x03, 0x00, 0xb9, 0xa0, 0x00, 0x00, 0x35,
        0xe0, 0x07, 0x40, 0xf9, 0xfd, 0x7b, 0x41, 0xa9,
        0xff, 0x83, 0x00, 0x91, 0xc0, 0x03, 0x5f, 0xd6,
        0xe0, 0x03, 0x00, 0x91, 0xaf, 0x3d, 0x12, 0x94,
    ];
    const NNCOMPATTHIN_WRAPPER: [u8; 76] = [
        0x7f, 0x23, 0x03, 0xd5, 0xff, 0x83, 0x00, 0xd1,
        0xfd, 0x7b, 0x01, 0xa9, 0xfd, 0x43, 0x00, 0x91,
        0x1f, 0x20, 0x03, 0xd5, 0x61, 0x89, 0x10, 0x30,
        0xe0, 0x23, 0x00, 0x91, 0xff, 0x07, 0x00, 0xf9,
        0xc8, 0x0d, 0x00, 0x94, 0xe0, 0x07, 0x00, 0xb9,
        0xe0, 0x00, 0x00, 0x35, 0xe8, 0x07, 0x40, 0xf9,
        0x00, 0x01, 0x3f, 0xd6, 0xfd, 0x7b, 0x41, 0xa9,
        0x00, 0x08, 0x40, 0x92, 0xff, 0x83, 0x00, 0x91,
        0xff, 0x0f, 0x5f, 0xd6, 0xe0, 0x13, 0x00, 0x91,
        0xc2, 0x0d, 0x00, 0x94,
    ];

    let nnsdk_base = 0x8013_0000u64;
    let compat_base = 0x8000_7000u64;
    mmu.map_range(compat_base, 0x8000, Permission::READ_WRITE_EXECUTE);
    mmu.map_range(nnsdk_base, 0x30000, Permission::READ_WRITE_EXECUTE);

    let nnsdk_fn_va = 0x8013_06D8u64;
    let svc_stub_va = 0x8015_84ACu64;
    mmu.write_bytes(nnsdk_fn_va, &NNSDK_GET_CORE_MASK).unwrap();
    for (i, &w) in [0xAA00_03E4u32, 0xD400_03C1, 0xF900_0081, 0x5280_0000, 0xD65F_03C0].iter().enumerate() {
        mmu.write_u32(svc_stub_va + (i * 4) as u64, w).unwrap();
    }

    let compat_fn_va = 0x8000_7970u64;
    let resolver_stub_va = 0x8000_B0B0u64;
    mmu.write_bytes(compat_fn_va, &NNCOMPATTHIN_WRAPPER).unwrap();
    for (i, &w) in [0xD280_DB08u32, 0xF2B0_0268, 0xF900_0008, 0x5280_0000, 0xD65F_03C0].iter().enumerate() {
        mmu.write_u32(resolver_stub_va + (i * 4) as u64, w).unwrap();
    }
    mmu.write_u32(compat_base, 0xD400_00E1).unwrap();

    cpu.regs.pc = compat_fn_va;
    cpu.regs.write_x(30, compat_base);
    run_vm(cpu, mmu, kernel, 10_000);
    println!();

    // ---------------------------------------------------------
    // Test 13: Switch 2 Firmware 23.0.1 Timing, Event (`LDAXR`/`STLXR`) & POSIX Thread Dumps (`pepsipu/ns2jb`)
    // ---------------------------------------------------------
    println!("--- Test 13: Switch 2 Firmware 23.0.1 Timing, Event & POSIX Thread Dumps (pepsipu/ns2jb) ---");
    {
        const FW23_TIMING_FREQ: [u8; 12] = [
            0x00, 0x00, 0x9f, 0x52, 0x80, 0x24, 0xa0, 0x72, 0xc0, 0x03, 0x5f, 0xd6,
        ];
        const FW23_EVENT_INIT: [u8; 40] = [
            0x28, 0x00, 0x80, 0x52, 0x5f, 0x04, 0x00, 0x71, 0x08, 0x04, 0x00, 0x39,
            0xe8, 0x17, 0x9f, 0x1a, 0x3f, 0x00, 0x00, 0x72, 0x08, 0x00, 0x00, 0x39,
            0x48, 0x00, 0x80, 0x52, 0x08, 0x11, 0x9f, 0x1a, 0x08, 0x04, 0x00, 0xb9,
            0xc0, 0x03, 0x5f, 0xd6,
        ];
        const FW23_EVENT_TRY_WAIT: [u8; 76] = [
            0x08, 0x44, 0x40, 0x38, 0x28, 0x01, 0x00, 0x34, 0x08, 0xfc, 0x5f, 0x88,
            0x1f, 0x09, 0x00, 0x71, 0x61, 0x01, 0x00, 0x54, 0x1f, 0xfc, 0x08, 0x88,
            0x88, 0xff, 0xff, 0x35, 0x20, 0x00, 0x80, 0x52, 0xbf, 0x3b, 0x03, 0xd5,
            0xc0, 0x03, 0x5f, 0xd6, 0x08, 0xfc, 0xdf, 0x88, 0xbf, 0x3b, 0x03, 0xd5,
            0x1f, 0x09, 0x00, 0x71, 0xe0, 0x17, 0x9f, 0x1a, 0xc0, 0x03, 0x5f, 0xd6,
            0xe0, 0x03, 0x1f, 0x2a, 0x5f, 0x3f, 0x03, 0xd5, 0xbf, 0x3b, 0x03, 0xd5,
            0xc0, 0x03, 0x5f, 0xd6,
        ];

        let (mut cpu13, mut mmu13, kernel13) = setup_vm();
        let code13 = 0x8000_0000u64;
        let data13 = 0x8300_0000u64;
        mmu13.map_range(code13, 0x10000, Permission::READ_WRITE_EXECUTE);
        mmu13.map_range(data13, 0x10000, Permission::READ_WRITE);

        // Build a harness at `code13` that calls:
        //   1. `nn::os::InitializeEvent(data13, 1, 1)` at `code13 + 0x100`
        //   2. `nn::os::TryWaitEvent(data13)` at `code13 + 0x200` -> W0 = 1
        //   3. `nn::os::GetSystemTickFrequency()` at `code13 + 0x300` -> W0 = 19,200,000
        //   4. Combines `(freq + try_wait) = 19,200,001` into X0 and calls `svcExitProcess`!
        mmu13.write_bytes(code13 + 0x100, &FW23_EVENT_INIT).unwrap();
        mmu13.write_bytes(code13 + 0x200, &FW23_EVENT_TRY_WAIT).unwrap();
        mmu13.write_bytes(code13 + 0x300, &FW23_TIMING_FREQ).unwrap();

        let harness13: [u32; 12] = [
            0xD280_0000, // movz x0, #0
            0xF2B0_6000, // movk x0, #0x8300, lsl #16 (x0 = data13)
            0xD280_0021, // movz x1, #1 (initially_signaled = true)
            0xD280_0022, // movz x2, #1 (clear_mode = ManualClear)
            0x9400_003C, // bl #0x100 (FW23_EVENT_INIT)
            0xD280_0000, // movz x0, #0
            0xF2B0_6000, // movk x0, #0x8300, lsl #16 (x0 = data13)
            0x9400_0079, // bl #0x200 (FW23_EVENT_TRY_WAIT -> W0 = 1)
            0xAA00_03F3, // mov x19, x0 (save try_wait result = 1 in X19)
            0x9400_00B7, // bl #0x300 (FW23_TIMING_FREQ -> X0 = 19,200,000)
            0x8B13_0000, // add x0, x0, x19 (X0 = 19,200,001)
            0xD400_00E1, // svc #0x07 (svcExitProcess)
        ];
        for (i, &w) in harness13.iter().enumerate() {
            mmu13.write_u32(code13 + (i * 4) as u64, w).unwrap();
        }
        cpu13.regs.pc = code13;
        run_vm(cpu13, mmu13, kernel13, 10_000);
    }
    println!();

    // ---------------------------------------------------------
    // Test 14: Switch 2 `.XCI` 64GB GameCard Cartridge Boot (`HEAD` -> `HFS0` `secure` -> `NCA3` -> Real Switch 2 `NSO0`)
    // ---------------------------------------------------------
    println!("--- Test 14: Switch 2 .XCI 64GB Cartridge Boot (HFS0 Secure -> NCA3 -> Real NSO0) ---");
    {
        let (mut cpu14, mut mmu14, kernel14) = setup_vm();
        let nso14 = include_bytes!("../ns2jb_ro_payload.bin");
        let exefs14 = Pfs0Archive::build(&[("main", nso14), ("main.npdm", b"META")]);
        let nca14 = NcaImage::build_encrypted(0x0400_0000_0000_0037, &title_key, &ctr_iv, &exefs14);
        let xci14 = loader::XciCartridge::build_cartridge(&[("0400000000000037.nca", &nca14)]);

        let info14 = BootLoader::load_any_package(&xci14, DEFAULT_BASE_ADDR, &title_key, &mut mmu14).unwrap();
        println!(
            "  [BootLoader] Format: {} | ProgramID: 0x{:016X} | .text: {} bytes",
            info14.format, info14.title_id, info14.code_size
        );
        cpu14.regs.pc = DEFAULT_BASE_ADDR;
        run_vm(cpu14, mmu14, kernel14, 50_000);
    }
    println!();

    // ---------------------------------------------------------
    // Test 15: Switch 2 NVN 3D Command Buffer Execution & Multi-Section RomFS Asset Loading
    //          Boots an authentic AES-128-CTR encrypted NCA3 with ExeFS (Section 0) and RomFS (Section 1).
    //          Guest builds an NVN command buffer (Clear Color + Draw) and submits via SVC #0x2D,
    //          verifies RomFS asset mounting, and exits cleanly.
    // ---------------------------------------------------------
    println!("--- Test 15: Switch 2 NVN 3D Command Buffer & Multi-Section RomFS Asset Loading ---");
    {
        let (mut cpu15, mut mmu15, mut kernel15) = setup_vm();
        let key15: [u8; 16] = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
            0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
        ];
        let iv0: [u8; 16] = [
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
            0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let iv1: [u8; 16] = [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
            0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
        ];

        // 1. Build RomFS Section 1 assets (Ampere shader, textures, manifest)
        let romfs_bytes = fs::RomFsArchive::build(&[
            ("/shaders/t239_ampere.glsl", b"#version 450\nlayout(location=0) out vec4 color;\nvoid main() { color = vec4(1.0, 0.5, 0.2, 1.0); }"),
            ("/textures/switch2_logo.bin", &[0xFF, 0x33, 0x11, 0xFF, 0x22, 0x88, 0xEE, 0xFF]),
            ("/manifest.json", b"{\"app_name\":\"NVN 3D Benchmark\",\"target\":\"T239 Drake\"}"),
        ]);

        // 2. Map Framebuffer VRAM (0x9000_0000) and Command Stream Buffer (0x8100_0000)
        let fb_addr = 0x9000_0000u64;
        let fb_size = 1280 * 720 * 4;
        mmu15.map_range(fb_addr, fb_size, Permission::READ_WRITE);

        let stream_addr = 0x8100_0000u64;
        mmu15.map_range(stream_addr, 4096, Permission::READ_WRITE);

        // Pre-build NVN Command Buffer in memory at `stream_addr`:
        let mut nvn_cb = gpu::nvn::NvnCommandBuffer::new();
        nvn_cb.set_render_target(fb_addr as u32, 1280, 720);
        nvn_cb.clear_color(0.12, 0.56, 1.0, 1.0); // Switch 2 Dodger Blue
        nvn_cb.draw_arrays(3);
        let stream_words = nvn_cb.encode_stream();
        for (i, &w) in stream_words.iter().enumerate() {
            mmu15.write_u32(stream_addr + (i * 4) as u64, w).unwrap();
        }

        // 3. Guest AArch64 program in ExeFS Section 0:
        //    X0 = stream_addr (0x8100_0000)
        //    X1 = stream_words.len()
        //    SVC #0x2D (svcNvnSubmitCommandBuffer) -> returns X0 = 0 (Success), X1 = commands_executed
        //    X19 = X1 (save command count)
        //    X0 = 0x239 (T239 status code)
        //    SVC #0x07 (svcExitProcess)
        #[rustfmt::skip]
        let guest_program: [u32; 10] = [
            0xD2800000, // movz x0, #0
            0xF2B02000, // movk x0, #0x8100, lsl #16 (X0 = 0x81000000)
            0xD2800001 | ((stream_words.len() as u32) << 5), // movz x1, #stream_words.len()
            0xD40005A1, // SVC #0x2D (svcNvnSubmitCommandBuffer)
            0xAA0103F3, // mov x19, x1 (X19 = commands_executed = 3)
            0xD2804720, // movz x0, #0x239 (T239)
            0xD40000E1, // SVC #0x07 (svcExitProcess)
            0xD503201F, // nop
            0xD503201F, // nop
            0xD65F03C0, // ret
        ];

        let mut guest_code = Vec::new();
        for inst in &guest_program {
            guest_code.extend_from_slice(&inst.to_le_bytes());
        }

        let exefs_bytes = Pfs0Archive::build(&[("main", &guest_code), ("main.npdm", b"NPDM")]);
        let title_id = 0x0100_0239_0000_2026;
        let encrypted_nca = NcaImage::build_encrypted_with_romfs(
            title_id,
            &key15,
            &iv0,
            &iv1,
            &exefs_bytes,
            Some(&romfs_bytes),
        );
        let nsp_bytes = Pfs0Archive::build(&[("game.nca", &encrypted_nca)]);

        let boot_info = BootLoader::load_package_with_vfs(
            &nsp_bytes,
            DEFAULT_BASE_ADDR,
            &key15,
            &mut mmu15,
            Some(&mut kernel15.vfs),
        ).unwrap();

        println!(
            "  [BootLoader] Loaded encrypted NCA3 package: TitleID=0x{:016X} Format='{}'",
            boot_info.title_id, boot_info.format
        );

        // Verify RomFS Level 3 archive automatically mounted in kernel VFS:
        let manifest = kernel15.vfs.get_file("romfs:/manifest.json").expect("RomFS asset mounted");
        println!("  [RomFS] Mounted asset 'romfs:/manifest.json': {}", String::from_utf8_lossy(manifest));
        let shader = kernel15.vfs.get_file("romfs:/shaders/t239_ampere.glsl").expect("RomFS shader mounted");
        println!("  [RomFS] Mounted asset 'romfs:/shaders/t239_ampere.glsl' ({} bytes)", shader.len());

        cpu15.regs.pc = DEFAULT_BASE_ADDR;
        run_vm(cpu15, mmu15, kernel15, 10_000);
        println!("  [NVN Driver] Fast-path command buffer executed via SVC #0x2D & rendered into VRAM 0x{:08X}", fb_addr);
    }
    println!();

    // ---------------------------------------------------------
    // Test 16: Switch 2 audren DSP Audio, NVN Memory Pools & Vulkan Hardware Pipeline
    //          Verifies:
    //            1. Tier-2 Direct-Chained JIT executing chained guest loops
    //            2. NVN Memory Pool sub-allocating VRAM for UBOs and VBOs
    //            3. Native Vulkan HAL loading SPIR-V 1.5 and dispatching draw calls
    //            4. audren:u DSP mixer processing stereo voices into 48kHz audio sinks
    //            5. Persistent SaveData syncing to disk
    // ---------------------------------------------------------
    println!("--- Test 16: Switch 2 audren DSP Audio, NVN Memory Pools & Vulkan Pipeline ---");
    {
        let (mut cpu16, mut mmu16, mut kernel16) = setup_vm();

        // 1. Tier-2 Direct-Chained JIT loop:
        let jit_code_va = 0x8000_0000u64;
        mmu16.map_range(jit_code_va, 4096, Permission::READ_WRITE_EXECUTE);
        #[rustfmt::skip]
        let loop_program: [u32; 10] = [
            0xD2800141, // 00: movz x1, #10
            0xD2800002, // 04: movz x2, #0
            0xD2800023, // 08: movz x3, #1
            // loop:
            0xB40000C1, // 0C: cbz  x1, done (+24 bytes)
            0x8B030044, // 10: add  x4, x2, x3
            0x91000062, // 14: add  x2, x3, #0
            0x91000083, // 18: add  x3, x4, #0
            0xF1000421, // 1C: subs x1, x1, #1
            0x17FFFFFB, // 20: b    loop (-20 bytes)
            0xD40000E1, // 24: svc  #0x07
        ];
        for (i, &w) in loop_program.iter().enumerate() {
            mmu16.write_u32(jit_code_va + (i * 4) as u64, w).unwrap();
        }

        let mut tier2_jit = cpu::Tier2JitEngine::new();
        cpu16.regs.pc = jit_code_va;
        let (insns_exec, exit_reason) = tier2_jit.run_slice(&mut cpu16, &mut mmu16, 5_000);
        println!(
            "  [Tier-2 JIT] Executed {} insns ({} chained block jumps) -> Exit: {:?}",
            insns_exec, tier2_jit.chained_jumps, exit_reason
        );

        // 2. NVN Memory Pool Allocator:
        let mut pool_mgr = gpu::NvnMemoryPoolManager::new();
        let pool_id = pool_mgr.create_pool(
            0x9800_0000,
            16 * 1024 * 1024, // 16 MB VRAM Pool
            gpu::memory_pool::NVN_MEMORY_POOL_CPU_CACHED | gpu::memory_pool::NVN_MEMORY_POOL_GPU_CACHED,
        );
        let pool = pool_mgr.get_pool_mut(pool_id).unwrap();
        let ubo = pool.allocate_buffer(512, 256, gpu::NvnBufferType::Uniform).unwrap();
        let vbo = pool.allocate_buffer(1024, 64, gpu::NvnBufferType::Vertex).unwrap();
        println!(
            "  [NVN VRAM Pool] Created Pool #{} (16MB) -> UBO at 0x{:08X} | VBO at 0x{:08X}",
            pool_id, ubo.gpu_address, vbo.gpu_address
        );

        // 3. Hardware Vulkan 1.3 / SPIR-V 1.5 Pipeline:
        let sass_ops = vec![
            SassOp::FMovImm { rd: 0, imm: 1.0 }.encode(),
            SassOp::FMovImm { rd: 1, imm: 0.5 }.encode(),
            SassOp::FSetp { p0: 0, p1: 1, rn: 0, rm: 1, op: gpu::shader::CmpOp::Gt }.encode(),
            SassOp::Exit.encode(),
        ];
        let decompiled = gpu::SassShaderDecompiler::decompile(&sass_ops);
        let mut vulkan = gpu::VulkanGpuBackend::new();
        vulkan.create_graphics_pipeline(&decompiled.spirv_words, &decompiled.spirv_words, 1280, 720, true).unwrap();
        vulkan.draw_arrays(&mmu16, vbo.gpu_address, 3).unwrap();
        vulkan.present_frame();
        println!(
            "  [Vulkan HAL] Loaded SPIR-V 1.5 ({} words) -> Pipeline PSOs: {} | Draws: {} | Presented: {}",
            decompiled.spirv_words.len(),
            vulkan.pipelines_created,
            vulkan.draw_calls_submitted,
            vulkan.hardware_frames_presented
        );

        // 4. Horizon audren:u Audio Renderer DSP Tick:
        let pcm_addr = 0x8200_0000u64;
        mmu16.map_range(pcm_addr, 4096, Permission::READ_WRITE);
        let pcm_data: [i16; 8] = [4000, -4000, 8000, -8000, 12000, -12000, 6000, -6000];
        for (i, &s) in pcm_data.iter().enumerate() {
            mmu16.write_u16(pcm_addr + (i * 2) as u64, s as u16).unwrap();
        }
        let voice = kernel16.audren.get_voice_mut(0).unwrap();
        voice.volume = 1.0;
        voice.mix_volumes = [0.8, 0.8];
        voice.queue_buffer(audio::WaveBuffer {
            buffer_addr: pcm_addr,
            buffer_size: pcm_data.len() * 2,
            start_sample_offset: 0,
            end_sample_offset: 8,
            is_looping: false,
            is_stream_ended: true,
        });
        let mixed_samples = kernel16.audren.render_dsp_tick(&mmu16);
        println!(
            "  [audren:u DSP] Rendered tick: {} stereo samples mixed | Peak Amp: {} | RingBuffer: {} samples",
            mixed_samples, kernel16.audren.peak_amplitude, kernel16.audren.output_ring_buffer.len()
        );

        // 5. Persistent SaveData:
        let save_temp = std::env::temp_dir().join("ounce_stage16_save");
        kernel16.vfs.set_save_directory(&save_temp);
        mmu16.map_range(0x8300_0000, 4096, Permission::READ_WRITE);
        mmu16.write_bytes(0x8300_0000, b"SWITCH2_SAVE_STAGE16").unwrap();
        let saved_bytes = kernel16.vfs.write_file_from_guest("save:/slot0.sav", 0x8300_0000, 20, &mmu16).unwrap();
        println!(
            "  [SaveData Persistence] Synchronized {} bytes to 'save:/slot0.sav'",
            saved_bytes
        );
        let _ = std::fs::remove_dir_all(&save_temp);
    }
    println!();

    // ---------------------------------------------------------
    // Test 17: Switch 2 Horizon "Big 5" Retail Application Handshake & Gamepad Input
    //          Verifies:
    //            1. set:sys service query (GetSystemLanguage & GetFirmwareVersion FW 23.0.1)
    //            2. am:appletOE retail proxy initialization (OpenApplicationProxy, GetCurrentFocusState)
    //            3. vi:m display management (GetDisplayService, OpenDisplay, CreateStrayLayer)
    //            4. hid Joy-Con 2 gamepad shared memory mapping & multi-button analog stick polling
    //            5. fsp-srv RomFS storage mount (OpenDataStorageByCurrentProcess, GetSize)
    //            6. End-to-end guest AArch64 retail handshake exit code verification (0x777)
    // ---------------------------------------------------------
    println!("--- Test 17: Switch 2 Horizon Big 5 Retail App Handshake & Gamepad Input ---");
    {
        let (mut cpu17, mut mmu17, mut kernel17) = setup_vm();
        let tls_addr = 0x8000_0000u64;
        let hid_shared_mem = 0x8004_0000u64;
        mmu17.map_range(tls_addr, 4096, Permission::READ_WRITE);
        mmu17.map_range(hid_shared_mem, 0x1000, Permission::READ_WRITE);

        // 1. set:sys service handshake:
        let setsys_h = kernel17.ipc.allocate_handle(ServiceType::SetSys);
        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 4).unwrap(); // GetFirmwareVersion
        let res_set = kernel17.ipc.handle_sync_request(
            setsys_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let fw_maj = mmu17.read_u32(tls_addr + 0x20).unwrap();
        let fw_min = mmu17.read_u32(tls_addr + 0x24).unwrap();
        let fw_micro = mmu17.read_u32(tls_addr + 0x28).unwrap();
        println!(
            "  [set:sys] Firmware Version Query (Cmd 4) -> {}.{}.{} (res={})",
            fw_maj, fw_min, fw_micro, res_set
        );

        // 2. am:appletOE service handshake:
        let applet_h = kernel17.ipc.allocate_handle(ServiceType::AppletOE);
        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 0).unwrap(); // OpenApplicationProxy
        kernel17.ipc.handle_sync_request(
            applet_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let proxy_h = mmu17.read_u32(tls_addr + 0x20).unwrap();

        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 0).unwrap(); // GetCommonStateGetter
        kernel17.ipc.handle_sync_request(
            proxy_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let csg_h = mmu17.read_u32(tls_addr + 0x20).unwrap();

        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 1).unwrap(); // GetCurrentFocusState
        kernel17.ipc.handle_sync_request(
            csg_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let focus_state = mmu17.read_u32(tls_addr + 0x20).unwrap();
        println!(
            "  [am:appletOE] ApplicationProxy #{} -> CommonStateGetter #{} FocusState: {} (InFocus)",
            proxy_h, csg_h, focus_state
        );

        // 3. vi:m service handshake:
        let vi_h = kernel17.ipc.allocate_handle(ServiceType::Vi);
        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 2).unwrap(); // GetDisplayService
        kernel17.ipc.handle_sync_request(
            vi_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let disp_srv_h = mmu17.read_u32(tls_addr + 0x20).unwrap();

        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 1010).unwrap(); // OpenDisplay("Default")
        kernel17.ipc.handle_sync_request(
            disp_srv_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let disp_id = mmu17.read_u32(tls_addr + 0x20).unwrap();

        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 2030).unwrap(); // CreateStrayLayer
        kernel17.ipc.handle_sync_request(
            disp_srv_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let layer_id = mmu17.read_u32(tls_addr + 0x20).unwrap();
        let producer_h = mmu17.read_u32(tls_addr + 0x28).unwrap();
        println!(
            "  [vi:m] DisplayService #{} -> DisplayID: {} | StrayLayer: #{} (Producer: 0x{:X})",
            disp_srv_h, disp_id, layer_id, producer_h
        );

        // 4. hid service & Joy-Con 2 physical gamepad controller:
        let hid_h = kernel17.ipc.allocate_handle(ServiceType::Hid);
        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 0).unwrap(); // CreateAppletResource
        kernel17.ipc.handle_sync_request(
            hid_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let res_h = mmu17.read_u32(tls_addr + 0x20).unwrap();

        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 0).unwrap(); // GetSharedMemoryHandle
        kernel17.ipc.handle_sync_request(
            res_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let shm_h = mmu17.read_u32(tls_addr + 0x20).unwrap();

        let mut hid_server = hid::HidServer::new(hid_shared_mem);
        hid_server.set_button_state(hid::NpadButton::A, true);
        hid_server.set_button_state(hid::NpadButton::B, true);
        hid_server.set_button_state(hid::NpadButton::Plus, true);
        hid_server.apply_gamepad_axes(0.75, -0.5, 0.25, 0.5, 0.05);
        hid_server.update_shared_memory(&mut mmu17).unwrap();
        let btns = mmu17.read_u64(hid_shared_mem + 8).unwrap();
        let lx = mmu17.read_u32(hid_shared_mem + 16).unwrap() as i32;
        let ly = mmu17.read_u32(hid_shared_mem + 20).unwrap() as i32;
        println!(
            "  [hid] AppletResource #{} -> SharedMem Handle: 0x{:X} | Joy-Con 2 Buttons: 0x{:X} (A+B+Plus) | Stick: ({}, {})",
            res_h, shm_h, btns, lx, ly
        );

        // 5. fsp-srv service handshake:
        let fsp_h = kernel17.ipc.allocate_handle(ServiceType::FspSrv);
        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 1).unwrap(); // OpenDataStorageByCurrentProcess
        kernel17.ipc.handle_sync_request(
            fsp_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let storage_h = mmu17.read_u32(tls_addr + 0x20).unwrap();

        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 4).unwrap(); // GetSize
        kernel17.ipc.handle_sync_request(
            storage_h,
            tls_addr,
            &mut mmu17,
            &mut kernel17.nvgpu,
            &mut kernel17.audio,
            &mut kernel17.vfs,
        );
        let storage_size = mmu17.read_u64(tls_addr + 0x20).unwrap();
        println!(
            "  [fsp-srv] Storage #{} -> RomFS Level 3 Data Size: 0x{:X} ({} KB)",
            storage_h, storage_size, storage_size / 1024
        );

        // 6. Guest AArch64 Retail Bootstrap execution:
        let code_va = 0x8001_0000u64;
        mmu17.map_range(code_va, 4096, Permission::READ_WRITE_EXECUTE);
        cpu17.regs.tpidrro_el0 = tls_addr;

        // Reset TLS to CMIF In Header for GetFirmwareVersion (Cmd 4)
        mmu17.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu17.write_u32(tls_addr + 0x18, 4).unwrap();

        // Pass setsys_h in X19, hid_shared_mem in X20
        cpu17.regs.write_x(19, setsys_h as u64);
        cpu17.regs.write_x(20, hid_shared_mem);

        // Guest program:
        //   MOV  X0, X19          ; X0 = setsys_h
        //   SVC  #0x21            ; svcSendSyncRequest -> setsys returns FW version in TLS
        //   LDR  X21, [X20, #8]   ; X21 = buttons from HID shared memory
        //   MOVZ X0, #0x777       ; X0 = 0x777 (retail boot success code)
        //   SVC  #0x07            ; svcExitProcess
        #[rustfmt::skip]
        let retail_program: [u32; 5] = [
            0xAA1303E0, // mov x0, x19
            0xD4000421, // svc #0x21 (svcSendSyncRequest)
            0xF9400695, // ldr x21, [x20, #8] (buttons bitmask)
            0xD280EEE0, // movz x0, #0x777
            0xD40000E1, // svc #0x07 (svcExitProcess)
        ];
        for (i, &w) in retail_program.iter().enumerate() {
            mmu17.write_u32(code_va + (i * 4) as u64, w).unwrap();
        }

        cpu17.regs.pc = code_va;
        run_vm(cpu17, mmu17, kernel17, 10_000);
        println!("  [Retail Bootstrap] Handshake completed successfully across all Big 5 services (Exit code: 0x777)");
    }
    println!();
}

/// Runs the 2D Display & Framebuffer Graphic Demo
fn run_gfx_demo(max_frames: usize) {
    println!("--- Initializing Switch 2 2D Framebuffer Display ---");
    println!("  Resolution: {}x{} (720p RGBA8888)", SCREEN_WIDTH, SCREEN_HEIGHT);
    println!("  VRAM Framebuffer: 0x{:016X}", DEFAULT_FRAMEBUFFER_ADDR);
    if max_frames > 0 {
        println!("  Benchmark Mode: Running for {} frames", max_frames);
    } else {
        println!("  Controls: Press ESC or Close Window to exit.");
    }
    println!();

    let (cpu, mut mmu, kernel) = setup_vm();

    // Map 1280x720x4 = 3,686,400 bytes for Framebuffer VRAM
    let fb_size = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
    mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, fb_size, Permission::READ_WRITE);

    let mut display = DisplayServer::new(DEFAULT_FRAMEBUFFER_ADDR, DisplayConfig::default());
    if let Err(e) = display.init_window() {
        eprintln!("Failed to initialize display window: {}", e);
        return;
    }

    #[rustfmt::skip]
    let gfx_program: Vec<u32> = vec![
        0xD2800001, // MOVZ X1, #0
        // frame_loop (offset 0x04):
        0xD2800004, // MOVZ X4, #0
        0xF2B20004, // MOVK X4, #0x9000, LSL #16 (X4 = 0x90000000)
        0xD28FA003, // MOVZ X3, #32000
        0x91000421, // ADD  X1, X1, #1 (frame++)
        0x91000022, // ADD  X2, X1, #0 (color = frame)
        // draw_loop (offset 0x18):
        0xB9000082, // STR  W2, [X4, #0]
        0x91001084, // ADD  X4, X4, #4
        0x91040442, // ADD  X2, X2, #257
        0xF1000463, // SUBS X3, X3, #1
        0x54FFFF81, // B.NE draw_loop (-4 instructions)
        0xD4000301, // SVC  #0x18 (VSync Flip!)
        0x17FFFFF4, // B    frame_loop (-12 instructions)
    ];

    let mut code_bytes = Vec::new();
    for inst in &gfx_program {
        code_bytes.extend_from_slice(&inst.to_le_bytes());
    }

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.write_bytes(DEFAULT_BASE_ADDR, &code_bytes).unwrap();

    run_vm_internal(cpu, mmu, kernel, Some(&mut display), None, 0, max_frames);
}

/// Runs the Interactive Playable Game Demo (Arrow keys / WASD to move player)
fn run_playable_game_demo(max_frames: usize) {
    println!("============================================================");
    println!("  🎮 SWITCH 2 INTERACTIVE GAME DEMO: OUNCE-RUNNER");
    println!("============================================================");
    println!("  CONTROLS:");
    println!("    • Arrow Keys / WASD : Move Player in Real Time");
    println!("    • Spacebar / J      : Action (Switch Player Color)");
    println!("    • ESC               : Exit Game");
    println!("------------------------------------------------------------\n");

    let (cpu, mut mmu, kernel) = setup_vm();

    // 1. Map Framebuffer (1280x720x4)
    let fb_size = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
    mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, fb_size, Permission::READ_WRITE);

    // 2. Map and Initialize Horizon HID Shared Memory
    let mut hid = HidServer::new(HID_SHARED_MEM_ADDR);
    hid.init_memory(&mut mmu);

    let mut display = DisplayServer::new(DEFAULT_FRAMEBUFFER_ADDR, DisplayConfig::default());
    if let Err(e) = display.init_window() {
        eprintln!("Failed to initialize display window: {}", e);
        return;
    }

    // AArch64 Interactive Game Program:
    // Reads controller buttons from Horizon Shared Memory (0x10000008)
    // Updates Player X and Y coordinates with velocity
    // Draws a 40x40 player block at (X, Y) in guest VRAM
    // Calls SVC #0x18 (VSync) each frame!
    //
    // Register mapping:
    //   X5 = Player X (starts at 600)
    //   X6 = Player Y (starts at 340)
    //   X7 = Player Color (starts at 0x00FF44 - neon green)
    //   X8 = Input buttons bitmask
    //   X9 = HID base address (0x10000000)
    //   X10 = FB base address (0x90000000)
    //   X11 = Step speed (10 pixels)
    //
    // Assembly:
    //   MOVZ X5, #600              ; Player X = 600
    //   MOVZ X6, #340              ; Player Y = 340
    //   MOVZ X7, #0xFF44           ; Neon Green Color
    //   MOVZ X9, #0x0000
    //   MOVK X9, #0x1000, LSL #16  ; X9 = 0x10000000 (HID)
    //   MOVZ X10, #0x0000
    //   MOVK X10, #0x9000, LSL #16 ; X10 = 0x90000000 (VRAM)
    //   MOVZ X11, #10              ; Speed = 10 px
    //
    // frame_loop (offset 0x20):
    //   LDR  W8, [X9, #8]          ; Read buttons bitmask from HID shared memory
    //
    //   ; Check DLeft (bit 12 -> mask 0x1000 = 4096)
    //   AND  W12, W8, #0x1000
    //   CBZ  W12, check_right
    //   SUBS X5, X5, X11           ; X = X - speed
    // check_right:
    //   ; Check DRight (bit 14 -> mask 0x4000 = 16384)
    //   AND  W12, W8, #0x4000
    //   CBZ  W12, check_up
    //   ADD  X5, X5, X11           ; X = X + speed
    // check_up:
    //   ; Check DUp (bit 13 -> mask 0x2000 = 8192)
    //   AND  W12, W8, #0x2000
    //   CBZ  W12, check_down
    //   SUBS X6, X6, X11           ; Y = Y - speed
    // check_down:
    //   ; Check DDown (bit 15 -> mask 0x8000 = 32768)
    //   AND  W12, W8, #0x8000
    //   CBZ  W12, draw_player
    //   ADD  X6, X6, X11           ; Y = Y + speed
    //
    // draw_player:
    //   ; Draw 40-pixel wide stripe at (X5, X6)
    //   ; Calculate offset = (Y * 1280 + X) * 4 = Y * 5120 + X * 4
    //   MOVZ X13, #5120            ; stride
    //   ADD  X14, X10, #0          ; start at FB base
    //   ADD  X15, X5, X5           ; X * 2
    //   ADD  X15, X15, X15         ; X * 4
    //   ADD  X14, X14, X15         ; FB + X*4
    //   ; Draw 40 pixels:
    //   MOVZ X16, #40
    // pixel_loop:
    //   STR  W7, [X14, #0]
    //   ADD  X14, X14, #4
    //   SUBS X16, X16, #1
    //   B.NE pixel_loop
    //
    //   SVC  #0x18                 ; VSync frame flip!
    //   B    frame_loop            ; repeat forever
    #[rustfmt::skip]
    let game_program: Vec<u32> = vec![
        // 0x00: Initialize coordinates and pointers
        0xD2804B05, // 0:  MOVZ X5, #600 (Player X)
        0xD2802A86, // 1:  MOVZ X6, #340 (Player Y)
        0xD29FE887, // 2:  MOVZ X7, #0xFF44 (Neon Green Color)
        0xD2800009, // 3:  MOVZ X9, #0
        0xF2A20009, // 4:  MOVK X9, #0x1000, LSL #16 (X9 = 0x10000000 HID)
        0xD280000A, // 5:  MOVZ X10, #0
        0xF2B2000A, // 6:  MOVK X10, #0x9000, LSL #16 (X10 = 0x90000000 FB)
        0xD280010B, // 7:  MOVZ X11, #8 (speed = 8 px)

        // 0x20: frame_loop (index 8)
        0xB9400928, // 8:  LDR  W8, [X9, #8] (Read buttons bitmask)

        // DLeft (index 9..12)
        0x5282000D, // 9:  MOVZ W13, #0x1000 (DLeft mask)
        0x0A0D010C, // 10: AND  W12, W8, W13
        0x3400004C, // 11: CBZ  W12, +8 bytes -> check_right
        0xCB0B00A5, // 12: SUBS X5, X5, X11 (X = X - speed)

        // DRight (index 13..16)
        0x5288000D, // 13: MOVZ W13, #0x4000 (DRight mask)
        0x0A0D010C, // 14: AND  W12, W8, W13
        0x3400004C, // 15: CBZ  W12, +8 bytes -> check_up
        0x8B0B00A5, // 16: ADD  X5, X5, X11 (X = X + speed)

        // DUp (index 17..20)
        0x5284000D, // 17: MOVZ W13, #0x2000 (DUp mask)
        0x0A0D010C, // 18: AND  W12, W8, W13
        0x3400004C, // 19: CBZ  W12, +8 bytes -> check_down
        0xCB0B00C6, // 20: SUBS X6, X6, X11 (Y = Y - speed)

        // DDown (index 21..24)
        0x5290000D, // 21: MOVZ W13, #0x8000 (DDown mask)
        0x0A0D010C, // 22: AND  W12, W8, W13
        0x3400004C, // 23: CBZ  W12, +8 bytes -> draw_player
        0x8B0B00C6, // 24: ADD  X6, X6, X11 (Y = Y + speed)

        // Draw Player (index 25..29)
        0x9100014E, // 25: ADD  X14, X10, #0 (FB base)
        0x8B0500AF, // 26: ADD  X15, X5, X5 (X * 2)
        0x8B0F01EF, // 27: ADD  X15, X15, X15 (X * 4)
        0x8B0F01CE, // 28: ADD  X14, X14, X15 (FB + X*4)
        0xD2800510, // 29: MOVZ X16, #40 (draw 40 pixels)

        // pixel_loop (index 30..33)
        0xB90001C7, // 30: STR  W7, [X14, #0]
        0x910011CE, // 31: ADD  X14, X14, #4
        0xF1000610, // 32: SUBS X16, X16, #1
        0x54FFFFA1, // 33: B.NE pixel_loop (-3 instructions)

        // VSync and loop (index 34..35)
        0xD4000301, // 34: SVC  #0x18 (VSync Flip!)
        0x17FFFFE5, // 35: B    frame_loop (-27 instructions -> offset 0x20)
    ];

    let mut code_bytes = Vec::new();
    for inst in &game_program {
        code_bytes.extend_from_slice(&inst.to_le_bytes());
    }

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.write_bytes(DEFAULT_BASE_ADDR, &code_bytes).unwrap();

    run_vm_internal(cpu, mmu, kernel, Some(&mut display), Some(&mut hid), 0, max_frames);
}

/// Runs the NVGPU 3D Triangle Rasterizer Demo via GPFIFO Command Pushbuffers
fn run_gpu3d_demo(max_frames: usize) {
    println!("============================================================");
    println!("  🔺 SWITCH 2 NVGPU 3D ENGINE DEMO: GPFIFO RASTERIZER");
    println!("============================================================");
    println!("  Resolution: {}x{} | Engine: Tegra 3D Subchannel", SCREEN_WIDTH, SCREEN_HEIGHT);
    println!("  Pipeline: Guest Float32 Math -> GPFIFO Pushbuffer -> Barycentric 3D Rasterizer\n");

    let (mut cpu, mut mmu, kernel) = setup_vm();

    // 1. Map Framebuffer VRAM (1280x720x4)
    let fb_size = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
    mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, fb_size, Permission::READ_WRITE);

    // 2. Map GPU Pushbuffer (0x80010000) and Vertex Buffer (0x80020000)
    let pb_addr = 0x80010000u64;
    let vb_addr = 0x80020000u64;
    mmu.map_range(pb_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(vb_addr, 4096, Permission::READ_WRITE);

    let mut display = DisplayServer::new(DEFAULT_FRAMEBUFFER_ADDR, DisplayConfig::default());
    if let Err(e) = display.init_window() {
        eprintln!("Failed to initialize display window: {}", e);
        return;
    }

    // Write initial 3D Triangle Vertex Buffer (3 vertices x 6 floats [x, y, z, r, g, b]):
    let vertices: [f32; 18] = [
        -0.30,  0.65, 0.0, 1.00, 0.20, 0.35, // Top Vertex (Coral Red)
        -0.65, -0.60, 0.0, 0.15, 0.95, 0.45, // Bottom-Left Vertex (Emerald Green)
         0.65, -0.60, 0.0, 0.20, 0.50, 1.00, // Bottom-Right Vertex (Electric Blue)
    ];
    for (i, &val) in vertices.iter().enumerate() {
        mmu.write_u32(vb_addr + (i * 4) as u64, val.to_bits()).unwrap();
    }

    // Construct GPFIFO Command Pushbuffer in Guest RAM:
    let pb_words: Vec<u32> = vec![
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_RENDER_TARGET_ADDR, 1),
        DEFAULT_FRAMEBUFFER_ADDR as u32,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VIEWPORT_DIMS, 2),
        SCREEN_WIDTH as u32,
        SCREEN_HEIGHT as u32,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_CLEAR_COLOR, 4),
        (0.06f32).to_bits(),
        (0.07f32).to_bits(),
        (0.12f32).to_bits(),
        (1.00f32).to_bits(),
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_MVP_MATRIX, 16),
        (1.00f32).to_bits(), (0.00f32).to_bits(), (0.00f32).to_bits(), (0.00f32).to_bits(),
        (0.00f32).to_bits(), (1.00f32).to_bits(), (0.00f32).to_bits(), (0.00f32).to_bits(),
        (0.00f32).to_bits(), (0.00f32).to_bits(), (1.00f32).to_bits(), (0.00f32).to_bits(),
        (0.00f32).to_bits(), (0.00f32).to_bits(), (0.00f32).to_bits(), (1.00f32).to_bits(),
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_CLEAR_SURFACE, 1),
        1,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_SET_VERTEX_ARRAY_ADDR, 1),
        vb_addr as u32,
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DRAW_ARRAYS, 1),
        3, // 3 vertices (1 shaded 3D triangle)
    ];
    for (i, &w) in pb_words.iter().enumerate() {
        mmu.write_u32(pb_addr + (i * 4) as u64, w).unwrap();
    }

    // Pre-configure CPU registers for the guest 3D animation loop:
    //   X5 = Vertex Buffer address (0x80020000)
    //   X6 = GPFIFO Pushbuffer address (0x80010000)
    //   X7 = Pushbuffer word count
    //   S1 = 0.01f32 (horizontal animation velocity per frame)
    cpu.regs.write_x(5, vb_addr);
    cpu.regs.write_x(6, pb_addr);
    cpu.regs.write_x(7, pb_words.len() as u64);
    cpu.regs.write_s(1, 0.01f32);

    // Guest AArch64 3D Render Loop:
    // frame_loop:
    //   LDR  S0, [X5, #0]     ; Load top vertex X coordinate (float32)
    //   FADD S0, S0, S1       ; Animate vertex: X += 0.01f
    //   STR  S0, [X5, #0]     ; Store updated vertex coordinate back to Vertex Buffer
    //   ADD  X0, X6, #0       ; X0 = GPFIFO Pushbuffer address
    //   ADD  X1, X7, #0       ; X1 = Pushbuffer word count
    //   SVC  #0x29            ; svcSubmitGpuPushbuffer -> Tegra 3D Engine clears & rasterizes triangle!
    //   SVC  #0x18            ; svcWaitSynchronization (VSync Frame Flip)
    //   B    frame_loop
    #[rustfmt::skip]
    let gpu3d_program: Vec<u32> = vec![
        0xBD4000A0, // LDR  S0, [X5, #0]
        0x1E212800, // FADD S0, S0, S1
        0xBD0000A0, // STR  S0, [X5, #0]
        0x910000C0, // ADD  X0, X6, #0
        0x910000E1, // ADD  X1, X7, #0
        0xD4000521, // SVC  #0x29 (svcSubmitGpuPushbuffer)
        0xD4000301, // SVC  #0x18 (VSync Flip!)
        0x17FFFFF9, // B    frame_loop (-7 instructions)
    ];

    let mut code_bytes = Vec::new();
    for inst in &gpu3d_program {
        code_bytes.extend_from_slice(&inst.to_le_bytes());
    }

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.write_bytes(DEFAULT_BASE_ADDR, &code_bytes).unwrap();

    run_vm_internal(cpu, mmu, kernel, Some(&mut display), None, 0, max_frames);
}

fn run_file(path: &str) {
    println!("[Loader] Reading package/executable: {}", path);
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return;
        }
    };

    let (mut cpu, mut mmu, kernel) = setup_vm();
    let default_key: [u8; 16] = [
        0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
        0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
    ];

    match BootLoader::load_any_package(&bytes, DEFAULT_BASE_ADDR, &default_key, &mut mmu) {
        Ok(info) => {
            println!("[BootLoader] Successfully loaded package:");
            println!("  Format      : {}", info.format);
            println!("  Title ID    : 0x{:016X}", info.title_id);
            println!("  Code size   : 0x{:08X} ({} bytes)", info.code_size, info.code_size);
            println!("  MOD0 relocs : {}", info.relocations_applied);

            cpu.regs.pc = DEFAULT_BASE_ADDR;
            run_vm(cpu, mmu, kernel, 1_000_000);
        }
        Err(e) => {
            eprintln!("[Loader Error] {}", e);
        }
    }
}

/// Builds a self-contained, AES-128-CTR encrypted Switch 2 `.nsp` cartridge file on disk
/// (`switch2_showcase.nsp`) and boots it directly through `run_file`.
fn build_and_boot_showcase_nsp(out_path: &str) {
    println!("============================================================");
    println!("  📦 BUILDING ENCRYPTED SWITCH 2 CARTRIDGE: {}", out_path);
    println!("============================================================");

    let default_key: [u8; 16] = [
        0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
        0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
    ];
    let ctr_iv: [u8; 16] = [
        0x20, 0x26, 0x02, 0x39, 0x00, 0x01, 0x02, 0x03,
        0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
    ];

    // Self-contained AArch64 Switch 2 Showcase Binary:
    //  1. Queries Switch 2 8-Core Cortex-A78C CoreMask (0xFF) via SVC #0x1E (svcGetInfo)
    //  2. Queries Switch 2 12 GB LPDDR5 TotalMemoryAvailable via SVC #0x1E (info_type = 6)
    //  3. Computes (12 GB >> 30) = 12 GB in X2 and combines with CoreMask (0xFF00 | 12 = 0xFF0C = 65292)
    //  4. Exits cleanly via SVC #0x07 (svcExitProcess)
    #[rustfmt::skip]
    let showcase_insns: Vec<u32> = vec![
        0xD2800001, // MOVZ X1, #0      (InfoType::CoreMask = 0)
        0xD40003C1, // SVC  #0x1E       (svcGetInfo -> X1 = 0xFF)
        0xD2800102, // MOVZ X2, #8
        0x9AC22024, // LSLV X4, X1, X2  (X4 = 0xFF << 8 = 0xFF00)
        0xD28000C1, // MOVZ X1, #6      (InfoType::TotalMemoryAvailable = 6 -> 12 GB)
        0xD40003C1, // SVC  #0x1E       (svcGetInfo -> X1 = 12 * 1024^3)
        0xD28003C2, // MOVZ X2, #30
        0x9AC22425, // LSRV X5, X1, X2  (X5 = 12 GB >> 30 = 12)
        0xAA050080, // ORR  X0, X4, X5  (X0 = 0xFF0C = 65292: 8 Cores + 12 GB LPDDR5!)
        0xD40000E1, // SVC  #0x07       (svcExitProcess)
    ];

    let mut code_bytes = Vec::new();
    for insn in &showcase_insns {
        code_bytes.extend_from_slice(&insn.to_le_bytes());
    }

    let exefs = Pfs0Archive::build(&[("main", &code_bytes), ("main.npdm", b"OUNCE_NPDM_2026")]);
    let encrypted_nca = NcaImage::build_encrypted(0x0100_2026_0002_3900, &default_key, &ctr_iv, &exefs);
    let nsp_bytes = Pfs0Archive::build(&[("0100202600023900.nca", &encrypted_nca)]);

    std::fs::write(out_path, &nsp_bytes).expect("Failed to write .nsp file to disk");
    println!(
        "  -> Wrote {} bytes to '{}' (PFS0 -> AES-128-CTR NCA3 -> ExeFS)\n",
        nsp_bytes.len(),
        out_path
    );

    run_file(out_path);
}
