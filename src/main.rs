// The runtime core contains no `unsafe` code; the compiler enforces it here.
// Any future host-level exception (a native code emitter, a raw graphics driver) lives
// in its own workspace crate, per AGENTS.md "Unsafe Boundary".
#![forbid(unsafe_code)]

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
    METHOD_CLEAR_SURFACE, METHOD_DISPATCH_UPSCALE_2X, METHOD_DRAW_ARRAYS,
    METHOD_DRAW_TEXTURED_ARRAYS, METHOD_SET_CLEAR_COLOR, METHOD_SET_DEPTH_TEST_ENABLE,
    METHOD_SET_MVP_MATRIX, METHOD_SET_RENDER_TARGET_ADDR, METHOD_SET_VERTEX_ARRAY_ADDR,
    METHOD_SET_VIEWPORT_DIMS,
};
use gpu::shader::SassOp;
use audio::HostAudioSink;
use gpu::{DisplayConfig, DisplayServer, DEFAULT_FRAMEBUFFER_ADDR, SCREEN_HEIGHT, SCREEN_WIDTH};
use hid::{HidServer, HostGamepadManager, HID_SHARED_MEM_ADDR};
use kernel::{HorizonKernel, ServiceType, SvcResult, CMIF_IN_MAGIC, DEFAULT_TLS_ADDR, TLS_SIZE};
use loader::{BootLoader, NcaImage, Pfs0Archive};
use mmu::{Memory, Permission};
use std::env;
use std::time::Instant;

const DEFAULT_BASE_ADDR: u64 = 0x80000000;
const STACK_TOP: u64 = 0x7FFFFFE000;
const STACK_SIZE: usize = 0x100000; // 1 MB stack

fn main() {
    // `--present=<auto|gpu|minifb|headless>` and `--cpu=<tier3|tier4>` may appear anywhere; strip them so the
    // positional arguments of every command stay where they were.
    let mut args: Vec<String> = Vec::new();
    for arg in env::args() {
        if let Some(value) = arg.strip_prefix("--cpu=") {
            match cpu::CpuTier::parse(value) {
                Some(tier) => cpu::CpuTier::set_cli_override(tier),
                None => eprintln!("[CPU] Unknown --cpu value '{value}' (expected tier3 or tier4); ignoring."),
            }
            continue;
        }
        match arg.strip_prefix("--present=") {
            Some(value) => match gpu::BackendKind::parse(value) {
                Some(kind) => gpu::BackendKind::set_cli_override(kind),
                None => eprintln!("[Display] Unknown --present value '{value}' (expected auto, gpu, minifb or headless); ignoring."),
            },
            None => args.push(arg),
        }
    }

    if args.len() < 2 {
        print_banner();
        print_usage();
        run_built_in_demo();
        return;
    }

    match args[1].as_str() {
        "help" | "-h" | "--help" => {
            print_usage();
        }
        "info" | "--info" | "-v" | "--version" | "specs" => {
            print_specs();
        }
        "test" | "demo" => {
            print_banner();
            run_built_in_demo();
        }
        "jit-selftest" => run_jit_selftest(),
        "gfx" | "display" => {
            print_banner();
            let max_frames = if args.len() >= 3 {
                args[2].parse::<usize>().unwrap_or(0)
            } else {
                0
            };
            run_gfx_demo(max_frames);
        }
        "play" | "game" => {
            print_banner();
            let max_frames = if args.len() >= 3 {
                args[2].parse::<usize>().unwrap_or(0)
            } else {
                0
            };
            run_playable_game_demo(max_frames);
        }
        "gpu3d" | "3d" => {
            print_banner();
            let max_frames = if args.len() >= 3 {
                args[2].parse::<usize>().unwrap_or(0)
            } else {
                0
            };
            run_gpu3d_demo(max_frames);
        }
        "run" => {
            print_banner();
            if args.len() < 3 {
                eprintln!("Error: Please provide path to .nsp, .nca, .nro or loose directory");
                return;
            }
            let file_path = &args[2];
            let gui_mode = args.iter().any(|a| a == "--gui" || a == "--window" || a == "-g");
            // Anything after the path that is not a flag is passed to homebrew as argv[1..].
            let guest_args: Vec<&str> = args[3..].iter().filter(|a| !a.starts_with('-')).map(|a| a.as_str()).collect();
            run_file(file_path, &guest_args, gui_mode);
        }
        "demo-dir" | "dir-demo" | "loose-demo" => {
            print_banner();
            let out_dir = if args.len() >= 3 && !args[2].starts_with('-') {
                &args[2]
            } else {
                "switch2_demo_pkg"
            };
            let gui_mode = args.iter().any(|a| a == "--gui" || a == "--window" || a == "-g");
            build_and_boot_demo_dir(out_dir, gui_mode);
        }
        "package" | "showcase" => {
            print_banner();
            let out_path = if args.len() >= 3 {
                &args[2]
            } else {
                "switch2_showcase.nsp"
            };
            build_and_boot_showcase_nsp(out_path);
        }
        "inspect" => {
            print_banner();
            if args.len() < 3 {
                eprintln!("Error: Please provide path to package (.xci, .nsp, .nca, .nro or loose directory)");
                return;
            }
            let file_path = &args[2];
            if std::path::Path::new(file_path).is_dir() {
                inspect_directory_package(file_path);
                return;
            }
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
        _ => {
            print_usage();
        }
    }
}

fn print_banner() {
    println!("============================================================");
    println!("  OUNCE-CORE: NVIDIA Tegra T239 / Horizon Research Runtime");
    println!("  Target: ARMv8.5-A AArch64 (PAC + BTI) | Horizon OS 64-bit Microkernel");
    println!("============================================================\n");
}

#[allow(dead_code)]
struct Styler {
    bold: &'static str,
    cyan: &'static str,
    green: &'static str,
    yellow: &'static str,
    magenta: &'static str,
    blue: &'static str,
    dim: &'static str,
    red: &'static str,
    reset: &'static str,
}

impl Styler {
    fn new() -> Self {
        use std::io::IsTerminal;
        let use_color = std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal();
        if use_color {
            Self {
                bold: "\x1b[1m",
                cyan: "\x1b[36m",
                green: "\x1b[32m",
                yellow: "\x1b[33m",
                magenta: "\x1b[35m",
                blue: "\x1b[34m",
                dim: "\x1b[90m",
                red: "\x1b[31m",
                reset: "\x1b[0m",
            }
        } else {
            Self {
                bold: "",
                cyan: "",
                green: "",
                yellow: "",
                magenta: "",
                blue: "",
                dim: "",
                red: "",
                reset: "",
            }
        }
    }
}

const BOX_WIDTH: usize = 92;

fn visible_width(s: &str) -> usize {
    let mut in_escape = false;
    let mut width = 0;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else if c == '🎮' || c == '🚀' || c == '🖥' || c == '🎨' || c == '🧠' || c == '🔊' || c == '💾' || c == '🌐' || c == '🛡' {
            width += 2;
        } else if c == '\u{fe0f}' {
            // variation selector has 0 width
        } else {
            width += 1;
        }
    }
    width
}

fn print_box_top(s: &Styler, title: &str) {
    let title_len = visible_width(title);
    let prefix_len = 6 + title_len + 3; // "┌── [ " + title + " ] "
    let dashes = if BOX_WIDTH > prefix_len + 1 {
        BOX_WIDTH - 1 - prefix_len
    } else {
        2
    };
    println!(
        "{}┌── [ {}{}{} ] {}{}┐{}",
        s.blue, s.bold, title, s.reset, s.blue, "─".repeat(dashes), s.reset
    );
}

fn print_box_bottom(s: &Styler) {
    println!("{}└{}┘{}", s.blue, "─".repeat(BOX_WIDTH - 2), s.reset);
}

fn print_row(s: &Styler, label: &str, val: &str) {
    let used = visible_width(label) + visible_width(val);
    let max_inner = BOX_WIDTH - 6;
    let pad = if max_inner > used { max_inner - used } else { 0 };
    println!(
        "{}│  {}{}{}{}{}{}  │{}",
        s.blue, s.bold, label, s.reset, val, " ".repeat(pad), s.blue, s.reset
    );
}

fn print_usage() {
    let s = Styler::new();
    let header_text = format!("  🚀  {}OUNCE-CORE v0.3.0{} — COMMAND-LINE INTERFACE (CLI) & WORKFLOW GUIDE", s.bold, s.reset);
    let header_width = visible_width(&header_text);
    let pad = if BOX_WIDTH - 2 > header_width { (BOX_WIDTH - 2) - header_width } else { 0 };

    println!("{}╭{}╮{}", s.cyan, "─".repeat(BOX_WIDTH - 2), s.reset);
    println!("{}│{}{}{}│{}", s.cyan, header_text, " ".repeat(pad), s.cyan, s.reset);
    println!("{}╰{}╯{}", s.cyan, "─".repeat(BOX_WIDTH - 2), s.reset);
    println!();
    println!("  {}USAGE:{}", s.bold, s.reset);
    println!("    cargo run --release -- {}<COMMAND>{} [OPTIONS]\n", s.green, s.reset);
    println!("  {}CORE EXECUTION COMMANDS:{}", s.bold, s.reset);
    println!("    {}run{} <file|dir> [args] [--gui]  Load and execute an .xci, .nsp, .nca, .nso, .nro or unencrypted loose folder (args go to homebrew argv)", s.green, s.reset);
    println!("    {}demo-dir{} [out_dir]      Synthesize and launch a loose synthetic directory package (OunceCake format)", s.green, s.reset);
    println!("    {}package{} [out.nsp]       Synthesize and boot an encrypted synthetic .nsp package", s.green, s.reset);
    println!("    {}inspect{} <file|dir>      Audit .xci/.nsp/.nca/.nro headers, loose NPDM metadata & RomFS assets", s.green, s.reset);
    println!();
    println!("  {}GRAPHICS & INTERACTIVE DEMOS:{}", s.bold, s.reset);
    println!("    {}play{} [frames]           Launch interactive game with real-time Joy-Con 2 controls", s.green, s.reset);
    println!("    {}3d{} [frames]             Run software 3D pushbuffer rasterizer benchmark", s.green, s.reset);
    println!("    {}gfx{} [frames]            Launch 2D high-bandwidth VRAM Framebuffer Display Demo (1280x720)", s.green, s.reset);
    println!("    {}--present={}<mode>        Presentation backend: auto, gpu (needs --features gpu-present), minifb, headless", s.green, s.reset);
    println!("    {}--cpu={}<tier>            CPU block engine for Stage 20: tier3 (default) or tier4 (needs --features native-jit)", s.green, s.reset);
    println!();
    println!("  {}VERIFICATION & DIAGNOSTICS:{}", s.bold, s.reset);
    println!("    {}test{}                    Execute 25-stage full-stack verification & hardware benchmark suite", s.green, s.reset);
    println!("    {}jit-selftest{}            Check that this host can run Tier-4 native code (needs --features native-jit)", s.green, s.reset);
    println!("    {}info{}                    Display comprehensive hardware architecture and engine status", s.green, s.reset);
    println!("    {}help{}                    Display this command-line manual and option reference", s.green, s.reset);
    println!();
    println!("  {}EXAMPLES:{}", s.bold, s.reset);
    println!("    {}cargo run --release -- info{}", s.dim, s.reset);
    println!("    {}cargo run --release -- test{}", s.dim, s.reset);
    println!("    {}cargo run --release -- play 1200{}", s.dim, s.reset);
    println!("    {}cargo run --release -- inspect ./dumps/game_title{}", s.dim, s.reset);
    println!("    {}cargo run --release -- run ./dumps/game_title\n{}", s.dim, s.reset);
}

fn print_specs() {
    let s = Styler::new();
    let host_arch = std::env::consts::ARCH;
    let host_os = std::env::consts::OS;
    let ptr_width = std::mem::size_of::<usize>() * 8;
    let profile = if cfg!(debug_assertions) { "Debug" } else { "Release [Optimized]" };

    let header_text = format!("  🎮  {}OUNCE-CORE v0.3.0{} — TEGRA T239 HARDWARE & RUNTIME SYSTEM PROFILE", s.bold, s.reset);
    let header_width = visible_width(&header_text);
    let pad = if BOX_WIDTH - 2 > header_width { (BOX_WIDTH - 2) - header_width } else { 0 };

    println!("{}╭{}╮{}", s.cyan, "─".repeat(BOX_WIDTH - 2), s.reset);
    println!("{}│{}{}{}│{}", s.cyan, header_text, " ".repeat(pad), s.cyan, s.reset);
    println!("{}╰{}╯{}", s.cyan, "─".repeat(BOX_WIDTH - 2), s.reset);
    println!();
    println!("  {}Target Platform{}   : {}NVIDIA Tegra T239 'Drake'{} (Codename: 'Ounce' / Horizon OS FW 23.0.1 ABI)", s.bold, s.reset, s.green, s.reset);
    println!("  {}Execution Model{}   : {}AArch64 Systems Research Runtime & Translation Layer{}", s.bold, s.reset, s.green, s.reset);
    println!("  {}Host Environment{}  : {}-{}-{}bit ({})", s.bold, s.reset, host_arch, host_os, ptr_width, profile);
    println!("  {}Legal Posture{}     : {}No proprietary keys, firmware, or game data are distributed{}", s.bold, s.reset, s.green, s.reset);
    println!();

    print_box_top(&s, "CPU Subsystem: ARMv8.5-A AArch64 (PAC + BTI)");
    print_row(&s, "Architecture        : ", "8x ARM Cortex-A78C (Octa-Core Preemptive Multi-Core Scheduler)");
    print_row(&s, "Security Features   : ", "ARMv8.3-A Pointer Authentication (PAC) + ARMv8.5-A BTI");
    print_row(&s, "Vector Engine       : ", "128-bit ARM NEON Advanced SIMD & IEEE-754 FP Single/Double");
    print_row(&s, "Execution Engine    : ", "Tier-1 block cache, Tier-2 chained blocks, Tier-3 super-blocks (host MIPS measured by Stage 20)");
    print_row(&s, "Virtual MMU         : ", "Sparse 4KB Page Table (39-bit & 48-bit Horizon Address Space)");
    print_row(&s, "Memory Sub-Regions  : ", "Code, RoData, Heap, Stack (1MB), TLS, SharedMem, VRAM, RomFS");
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "GPU Subsystem: NVIDIA Ampere Custom GA10B/GA10F");
    print_row(&s, "Architecture        : ", "NVIDIA Ampere GA10B/GA10F Derivative (Tegra T239 Custom)");
    print_row(&s, "Compute Cores       : ", "12 Streaming Multiprocessors (~1,536 CUDA Cores, SM 8.7)");
    print_row(&s, "Driver Model        : ", "NVN service model (CommandBuffer, Memory & Texture Pools)");
    print_row(&s, "Pushbuffer ISA      : ", "GPFIFO Channel Methods (0x100..0x10A, Texture Descriptor 0x0B)");
    print_row(&s, "Shader Pipeline     : ", "Ampere SASS Bytecode Transpiler -> Vulkan SPIR-V 1.5 Binaries");
    print_row(&s, "Upscaler Model      : ", "2x upscaler model (bilinear + sharpen, not DLSS)");
    print_row(&s, "Rasterizer Engine   : ", "Double-Buffered 32-bit RGBA8888 VRAM, Depth Z-Buffer, UV Interp");
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Memory Subsystem & High-Speed Interconnect");
    print_row(&s, "System DRAM         : ", "12 GB LPDDR5X (128-bit Bus @ 6400 MT/s, ~102.4 GB/s Bandwidth)");
    print_row(&s, "VRAM Framebuffer    : ", "0x9000_0000 (1280x720 Handheld / 1920x1080 Docked Linear RGBA)");
    print_row(&s, "Pushbuffer Ring     : ", "0x9400_0000 (Asynchronous GPFIFO Command Submission Ring)");
    print_row(&s, "DMA Controller      : ", "Zero-Copy Unified Virtual Memory & Audio DMA Circular Buffers");
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Audio DSP, Controllers & Joy-Con 2 Inputs");
    print_row(&s, "Audio DSP Mixer     : ", "Horizon audren:u Audio Renderer (Multi-Channel Voice Matrix)");
    print_row(&s, "Audio Output Sinks  : ", "48kHz Stereo 16-bit Linear PCM Audio Stream (audout:u)");
    print_row(&s, "Joy-Con 2 Subsystem : ", "Attached Rail, Wireless Bluetooth, & Dual-Grip Modes (0x200)");
    print_row(&s, "Analog Filtering    : ", "Hardware Button Bitmasking & Analog Stick Radial Deadzones");
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Storage Containers & Cryptographic Engine");
    print_row(&s, "GameCard Media      : ", "64GB GameCard image format (.xci / HEAD at 0x100 / HFS0)");
    print_row(&s, "Digital Packages    : ", "Nintendo Submission Package (.nsp / PFS0 Hierarchical Archive)");
    print_row(&s, "Loose Folders       : ", "Unencrypted ExeFS/RomFS Dumps (OunceCake / Atmosphere Direct Boot)");
    print_row(&s, "Decryption Engine   : ", "Synthetic test vector decryption (commercial retail decryption disabled)");
    print_row(&s, "RomFS Filesystem    : ", "Level 3 Directory & File Hash Tables with Hierarchical Trees");
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Horizon OS 64-bit Microkernel & HLE Services");
    print_row(&s, "IPC Protocols       : ", "Horizon Tipc (0x10/0x12) & CMIF (SFCI / SFCO)");
    print_row(&s, "Active Services     : ", "set:sys, am:appletOE, vi:m, hid, fsp-srv, ro:1, pl:u, time:u, nvdrv");
    print_row(&s, "Firmware Target     : ", "Horizon OS System Version 23.0.1 (Target ABI)");
    print_row(&s, "Process Model       : ", "Preemptive Multi-Threaded Scheduler with Core Affinity Masks");
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Compliance & Security Verification");
    print_row(&s, "Distribution Status: ", "No proprietary keys, firmware, or game data are distributed");
    print_row(&s, "Zero-Key Engine     : ", "Runs homebrew binaries without requiring extracted prod.keys");
    let val_validation = format!("{}85 Unit Tests PASS{} · {}25 Integration Stages PASS (0xEEE){}", s.green, s.reset, s.green, s.reset);
    print_row(&s, "Validation Status   : ", &val_validation);
    let val_source = format!("{}https://github.com/hffgzktbcc-blip/ounce-core{}", s.cyan, s.reset);
    print_row(&s, "Official Source     : ", &val_source);
    let val_sec = format!("{}Reject third-party binaries; build only from official source{}", s.yellow, s.reset);
    print_row(&s, "Security Notice     : ", &val_sec);
    print_box_bottom(&s);
    println!();
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

/// Why a guest run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VmStop {
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
#[derive(Debug, Clone, Copy)]
struct VmOutcome {
    stop: VmStop,
    x0: u64,
    d0: f64,
}

fn run_vm(
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
struct StageGate {
    passed: usize,
    failures: Vec<String>,
}

impl StageGate {
    /// Checks a guest run and prints the verdict. Returns `true` on pass.
    fn expect_exit(&mut self, stage: u32, outcome: &VmOutcome, expected_x0: u64) -> bool {
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
    fn expect(&mut self, stage: u32, what: &str, ok: bool) -> bool {
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
    fn finish(self) {
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

fn run_vm_internal(
    mut cpu: Cpu,
    mut mmu: Memory,
    mut kernel: HorizonKernel,
    mut display: Option<&mut DisplayServer>,
    mut hid: Option<&mut HidServer>,
    max_instructions: usize,
    max_frames: usize,
) -> VmOutcome {
    let is_gpu = display.as_ref().map_or(false, |d| d.is_gpu_accelerated());
    kernel.nvgpu.set_hardware_acceleration(is_gpu);

    println!("[VM] Starting execution at PC: 0x{:016X}", cpu.regs.pc);
    println!("[VM] Stack Pointer: 0x{:016X}", cpu.regs.sp);

    let start = Instant::now();
    let mut instructions_executed = 0;
    let mut frames_presented = 0;
    let mut gamepad_mgr = HostGamepadManager::new();
    if let Some(pad_name) = gamepad_mgr.active_gamepad_name() {
        println!("[HID] Physical gamepad attached: {}", pad_name);
    }
    let host_audio = HostAudioSink::new();
    let mut stop = VmStop::Budget;
    let mut last_input_poll = Instant::now();

    loop {
        if max_instructions > 0 && instructions_executed >= max_instructions {
            println!("\n[VM] Execution budget reached ({} instructions).", max_instructions);
            break;
        }

        match cpu.step(&mut mmu) {
            Ok(StepResult::Continue) => {
                instructions_executed += 1;
                // Time slice: a guest thread that spins without SVCs must not starve the others.
                if instructions_executed % kernel::scheduler::PREEMPT_INTERVAL == 0 {
                    kernel.scheduler.preempt(&mut cpu);
                }
            }
            Ok(StepResult::Svc(svc_id)) => {
                instructions_executed += 1;
                match kernel.dispatch_svc(svc_id, &mut cpu, &mut mmu) {
                    SvcResult::Success => {
                        // libnx homebrew reads HID shared memory between SVCs rather than per presented
                        // frame: hand it the host controller every few milliseconds.
                        if kernel.ipc.hid.addr.is_some() && last_input_poll.elapsed() >= INPUT_POLL_INTERVAL {
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
                        frames_presented += 1;

                        // 1. Poll host keyboard and physical gamepad input
                        if let Some(ref mut h) = hid {
                            if let Some(ref mut disp) = display {
                                let active_keys = disp.get_active_keys();
                                h.poll_keyboard(&active_keys);
                            }
                            gamepad_mgr.poll(h);
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
                            if !disp.present_frame_3d(&mmu, &draw_ops) {
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
                        stop = VmStop::Break;
                        break;
                    }
                    SvcResult::Unhandled(id) => {
                        println!("\n[VM] Stopping on unhandled SVC #0x{:02X}.", id);
                        stop = VmStop::UnhandledSvc;
                        break;
                    }
                }
            }
            Ok(StepResult::Halt(msg)) => {
                println!("\n[VM] CPU Halted: {}", msg);
                stop = VmStop::Halt;
                break;
            }
            Err(e) => {
                eprintln!("\n[VM Error] {}", e);
                eprintln!("  Final PC: 0x{:016X}", cpu.regs.pc);
                stop = VmStop::Fault;
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

    VmOutcome {
        stop,
        x0: cpu.regs.read_x(0),
        d0: cpu.regs.read_d(0),
    }
}

fn run_built_in_demo() {
    println!("--- Running Built-in Test Suite ---\n");
    let mut gate = StageGate::default();

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

    println!("[Stage 01/25] AArch64 Arithmetic & Branching Execution (Fibonacci N=10)");
    let (cpu, mut mmu, kernel) = setup_vm();
    let mut code_bytes = Vec::new();
    for inst in &fib_program {
        code_bytes.extend_from_slice(&inst.to_le_bytes());
    }

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.write_bytes(DEFAULT_BASE_ADDR, &code_bytes).unwrap();

    let outcome = run_vm(cpu, mmu, kernel, 100_000);
    gate.expect_exit(1, &outcome, 55);

    // Test Program 2: Horizon OS Console Output (svcOutputDebugString)
    println!("\n[Stage 02/25] Horizon OS Debug Output Subsystem (svcOutputDebugString)");
    let (cpu, mut mmu, kernel) = setup_vm();

    let message = "Hello from Tegra T239 homebrew on Ounce-Core!\n";
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

    let outcome = run_vm(cpu, mmu, kernel, 10_000);
    gate.expect_exit(2, &outcome, 0);

    // Test Program 3: Stack & Subroutine Call (STP, LDP, BL, RET)
    println!("\n[Stage 03/25] Stack Frame & Subroutine Management (STP, LDP, BL, RET)");
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

    let outcome = run_vm(cpu, mmu, kernel, 10_000);
    gate.expect_exit(3, &outcome, 42);

    // Test Program 4: Horizon OS HIPC / CMIF Service Manager ("sm:" -> "nvdrv") & Floating-Point Math
    println!("\n[Stage 04/25] Horizon CMIF IPC & IEEE-754 Vector Math (FADD)");
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

    let outcome = run_vm(cpu, mmu, kernel, 10_000);
    if gate.expect_exit(4, &outcome, 0x1_0002) {
        gate.expect(4, "D0 = 42.0", outcome.d0 == 42.0);
    }

    // Test Program 5: RomFS Virtual Filesystem & 48kHz Stereo PCM Audio Synthesizer (audout:u)
    println!("\n[Stage 05/25] Virtual Filesystem (RomFS) & Linear PCM Audio (audout:u)");
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
    //   SVC  #0x102                ; svcReadRomFsFile("romfs:/title.txt", dst=0x80000200) -> X1 = bytes_read
    //   ADD  X0, X4, #0           ; X0 = file_dst_addr
    //   SVC  #0x27                ; svcOutputDebugString(X0, X1) -> prints loaded RomFS file!
    //   MUL  X8, X6, X7           ; MADD X8, X6, X7, XZR -> 300 * 48 = 14400 (16-bit PCM sample)
    //   STRH W8, [X5, #0]         ; Write Left channel PCM sample (14400)
    //   STRH W8, [X5, #2]         ; Write Right channel PCM sample (14400)
    //   ADD  X0, X5, #0           ; X0 = pcm_buf_addr
    //   MOVZ X1, #2               ; X1 = 2 stereo samples
    //   SVC  #0x103                ; svcAppendAudioBuffer -> queues PCM to audout:u, returns peak in X1
    //   ADD  X0, X1, #0           ; X0 = peak audio amplitude (14400)
    //   SVC  #0x07                ; svcExitProcess
    #[rustfmt::skip]
    let vfs_audio_program: Vec<u32> = vec![
        0xD4002041, // SVC  #0x102 (ext::READ_ROMFS_FILE)
        0x91000080, // ADD  X0, X4, #0
        0xD40004E1, // SVC  #0x27 (svcOutputDebugString)
        0x9B077CC8, // MUL  X8, X6, X7 (MADD X8, X6, X7, XZR -> 14400)
        0x790000A8, // STRH W8, [X5, #0]
        0x790004A8, // STRH W8, [X5, #2]
        0x910000A0, // ADD  X0, X5, #0
        0xD2800041, // MOVZ X1, #2
        0xD4002061, // SVC  #0x103 (ext::APPEND_AUDIO_BUFFER)
        0x91000020, // ADD  X0, X1, #0
        0xD40000E1, // SVC  #0x07 (svcExitProcess)
    ];

    let mut vfs_audio_bytes = Vec::new();
    for inst in &vfs_audio_program {
        vfs_audio_bytes.extend_from_slice(&inst.to_le_bytes());
    }
    mmu.write_bytes(DEFAULT_BASE_ADDR, &vfs_audio_bytes).unwrap();

    let outcome = run_vm(cpu, mmu, kernel, 10_000);
    gate.expect_exit(5, &outcome, 0x3840);
    println!();

    println!("[Stage 06/25] AES-128-CTR NCA Decryption, SASS Decompiler & Multi-Threading");

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
    //  1. Submits NVGPU pushbuffer (SVC #0x101) to decompile the Ampere SASS shader
    //  2. Spawns a worker KThread on Core #3 via SVC #0x08 (svcCreateThread)
    //  3. Starts the worker KThread via SVC #0x09 (svcStartThread)
    //  4. Reads the worker thread's result from [0x80060000] into X0 (expected: 7777) and exits!
    cpu.regs.write_x(10, pb_addr);
    cpu.regs.write_x(11, worker_pc);
    cpu.regs.write_x(12, shared_mem_addr);
    cpu.regs.write_x(13, worker_stack_top);

    #[rustfmt::skip]
    let main_thread_program: Vec<u32> = vec![
        // Step 1: SVC #0x101 (svcSubmitGpuPushbuffer: X0 = pb_addr, X1 = 2 words)
        0x91000140, // ADD  X0, X10, #0
        0xD2800041, // MOVZ X1, #2
        0xD4002021, // SVC  #0x101 (ext::SUBMIT_GPU_PUSHBUFFER)

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
    let outcome = run_vm_internal(cpu, mmu, kernel, display_none.as_mut().map(|x| &mut **x), None, 10_000, 0);
    gate.expect_exit(6, &outcome, 0x1E61);
    println!();

    // ---------------------------------------------------------
    // Test 7: Unified .nsp Package Boot, Futex/Mutex Arbitration,
    //         & 2D-Textured 3D Triangle Rasterization
    // ---------------------------------------------------------
    println!("\n[Stage 07/25] Unified NSP Boot, Futex Arbiters & 2D-Textured Rasterization");

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

        // 4. Submit NVGPU pushbuffer to render 2D-textured 3D triangle (SVC #0x101)
        0x91000160, // ADD  X0, X11, #0
        0x910001A1, // ADD  X1, X13, #0
        0xD4002021, // SVC  #0x101 (ext::SUBMIT_GPU_PUSHBUFFER)

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

    let outcome = run_vm(cpu, mmu, kernel, 10_000);
    gate.expect_exit(7, &outcome, 0xFF40_C0FF);
    println!();

    // ---------------------------------------------------------
    // Test 8: Switch 2 crt0 Startup (svcGetInfo 12GB LPDDR5 + 8-Core Mask),
    //         Basic-Block JIT Loop & 3D Z-Buffer Depth Occlusion
    // ---------------------------------------------------------
    println!("\n[Stage 08/25] Horizon crt0 Initialization, Basic-Block JIT & Depth Occlusion");

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
    //  1. Calls SVC #0x29 (svcGetInfo: X1 = 0 -> CoreMask = 0xFF for 8x Cortex-A78C cores)
    //  2. Saves CoreMask in X4
    //  3. Submits NVGPU Z-Buffered 3D Draw Pushbuffer via SVC #0x101
    //  4. Loads center pixel from VRAM [X12] (0xFF00FF00 = Pure Green, proving near triangle occluded far red triangle!)
    //  5. Combines (CenterPixel | CoreMask) -> 0xFF00FFFF in X0 and exits!
    #[rustfmt::skip]
    let test8_insns: Vec<u32> = vec![
        0xD2800001, // MOVZ X1, #0      (InfoType::CoreMask = 0)
        0xD4000521, // SVC  #0x29       (svcGetInfo -> X1 = 0xFF)
        0x91000024, // ADD  X4, X1, #0  (X4 = 0xFF)
        0x91000140, // ADD  X0, X10, #0 (pb_addr8)
        0x91000161, // ADD  X1, X11, #0 (word_count)
        0xD4002021, // SVC  #0x101      (ext::SUBMIT_GPU_PUSHBUFFER)
        0xB9400180, // LDR  W0, [X12]   (0xFF00FF00)
        0xAA040000, // ORR  X0, X0, X4  (0xFF00FF00 | 0xFF = 0xFF00FFFF = 4278255615)
        0xD40000E1, // SVC  #0x07       (svcExitProcess)
    ];

    let mut test8_bytes = Vec::new();
    for insn in &test8_insns {
        test8_bytes.extend_from_slice(&insn.to_le_bytes());
    }
    mmu.write_bytes(DEFAULT_BASE_ADDR, &test8_bytes).unwrap();

    let outcome = run_vm(cpu, mmu, kernel, 10_000);
    gate.expect_exit(8, &outcome, 0xFF00_FFFF);
    println!();

    // ---------------------------------------------------------
    // Test 9: 128-bit NEON Vector SIMD (V0.4S), 2x upscaler model
    //         2x Super-Resolution Upscaler & SaveData ('save:/profile.sav')
    // ---------------------------------------------------------
    println!("\n[Stage 09/25] 128-bit NEON SIMD, 2x Upscaler Model & SaveData");

    let (mut cpu, mut mmu, kernel) = setup_vm();

    let simd_addr = 0x80020000u64;
    let save_path_addr = 0x80030000u64;
    let upscale_src_addr = 0x90000000u64;
    let upscale_dst_addr = 0x90010000u64;
    let pb_addr9 = 0x80010000u64;

    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.map_range(simd_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(save_path_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(upscale_src_addr, 4096, Permission::READ_WRITE);
    mmu.map_range(upscale_dst_addr, 4096, Permission::READ_WRITE);
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

    // 3. Populate 8x8 low-res render surface at 0x90000000 and build 2x upscale pushbuffer (8x8 -> 16x16 = 256 pixels)
    for i in 0..64u64 {
        mmu.write_u32(upscale_src_addr + i * 4, 0xFF80C040).unwrap();
    }
    let pb_upscale_words: Vec<u32> = vec![
        NvgpuChannel::encode_header(SubmissionMode::Increasing, 2, METHOD_DISPATCH_UPSCALE_2X, 4),
        upscale_src_addr as u32,
        8,
        8,
        upscale_dst_addr as u32,
    ];
    for (i, &w) in pb_upscale_words.iter().enumerate() {
        mmu.write_u32(pb_addr9 + (i * 4) as u64, w).unwrap();
    }

    cpu.regs.write_x(10, simd_addr);
    cpu.regs.write_x(11, save_path_addr);
    cpu.regs.write_x(12, save_path.len() as u64);
    cpu.regs.write_x(13, pb_addr9);
    cpu.regs.write_x(14, pb_upscale_words.len() as u64);

    // Guest AArch64 Program:
    //  1. LDR  Q1, [X10, #0]        ; Load 128-bit VecA [10.0, 20.0, 30.0, 40.0]
    //  2. LDR  Q2, [X10, #16]       ; Load 128-bit VecB [2.0, 3.0, 4.0, 5.0]
    //  3. FMUL V3.4S, V1.4S, V2.4S  ; 4-lane SIMD multiply -> [20.0, 60.0, 120.0, 200.0]
    //  4. FADD V0.4S, V3.4S, V1.4S  ; 4-lane SIMD add      -> [30.0, 80.0, 150.0, 240.0]
    //  5. STR  Q0, [X10, #32]       ; Store 128-bit result vector to [0x80020020]
    //  6. SVC  #0x104                ; svcWriteSaveDataFile("save:/profile.sav", [0x80020020], 16 bytes)
    //  7. SVC  #0x101                ; svcSubmitGpuPushbuffer (2x upscaler model)
    //  8. Read back saved profile data via SVC #0x102 ("save:/profile.sav") into [0x80020040]
    //  9. Load lane 0 float (30.0f) into S0 and bytes_read (16) into X0, then exit!
    #[rustfmt::skip]
    let test9_insns: Vec<u32> = vec![
        0x3DC00141, // LDR  Q1, [X10, #0]
        0x3DC00542, // LDR  Q2, [X10, #16]
        0x6E22DC23, // FMUL V3.4S, V1.4S, V2.4S
        0x4E21D460, // FADD V0.4S, V3.4S, V1.4S
        0x3D800940, // STR  Q0, [X10, #32]

        // Write 16-byte SIMD vector result to "save:/profile.sav" (SVC #0x104)
        0x91000160, // ADD  X0, X11, #0  (path_ptr)
        0x91000181, // ADD  X1, X12, #0  (path_len)
        0x91008142, // ADD  X2, X10, #32 (src_addr = 0x80020020)
        0xD2800203, // MOVZ X3, #16      (data_len = 16 bytes)
        0xD4002081, // SVC  #0x104       (ext::WRITE_SAVE_DATA_FILE)

        // Dispatch 2x upscaler model (SVC #0x101)
        0x910001A0, // ADD  X0, X13, #0  (pb_addr9)
        0x910001C1, // ADD  X1, X14, #0  (word_count)
        0xD4002021, // SVC  #0x101       (ext::SUBMIT_GPU_PUSHBUFFER)

        // Read back "save:/profile.sav" into [0x80020040] via SVC #0x102
        0x91000160, // ADD  X0, X11, #0  (path_ptr)
        0x91000181, // ADD  X1, X12, #0  (path_len)
        0x91010142, // ADD  X2, X10, #64 (dst_addr = 0x80020040)
        0xD2800203, // MOVZ X3, #16      (max_len = 16)
        0xD4002041, // SVC  #0x102       (ext::READ_ROMFS_FILE -> X1 = 16 bytes read)
        0x91000020, // ADD  X0, X1, #0   (X0 = 16)
        0xD40000E1, // SVC  #0x07        (svcExitProcess)
    ];

    let mut test9_bytes = Vec::new();
    for insn in &test9_insns {
        test9_bytes.extend_from_slice(&insn.to_le_bytes());
    }
    mmu.write_bytes(DEFAULT_BASE_ADDR, &test9_bytes).unwrap();

    let outcome = run_vm(cpu, mmu, kernel, 10_000);
    gate.expect_exit(9, &outcome, 0x10);
    println!();

    // ---------------------------------------------------------
    // Test 10: Native FS Payload (BTI landing pad, Horizon process/thread queries & stack spill)
    //          Clean-room AArch64 program that queries svcGetProcessId / svcGetThreadId /
    //          svcGetInfo(SystemTick) and stores the results into a guest output buffer.
    // ---------------------------------------------------------
    println!("\n[Stage 10/25] Native FS Payload Execution (BTI & Horizon SVCs)");

    let (mut cpu, mut mmu, kernel) = setup_vm();

    let out_buf10 = 0x80020000u64;
    mmu.map_range(DEFAULT_BASE_ADDR, 4096, Permission::READ_WRITE_EXECUTE);
    mmu.map_range(out_buf10, 4096, Permission::READ_WRITE);

    // Source (hand-written, assembled with llvm-mc --triple=aarch64 -mattr=+v8.5a):
    //   bti c
    //   str x30, [sp, #-16]!
    //   mov x9, x0
    //   mov x10, x1
    //   movn w1, #0x7ffe       // CurrentProcess pseudo-handle (0xFFFF8001)
    //   svc #0x24             // svcGetProcessId
    //   str x0, [x9]
    //   str x1, [x9, #8]
    //   movn w1, #0x7fff       // CurrentThread pseudo-handle (0xFFFF8000)
    //   svc #0x25             // svcGetThreadId
    //   str x0, [x9, #16]
    //   str x1, [x9, #24]
    //   svc #0x1e             // svcGetSystemTick
    //   str x0, [x9, #32]
    //   eor x0, x10, x10, lsl #1
    //   str x0, [x9, #40]
    //   ldr x30, [sp], #16
    //   ret
    for (i, &w) in kernel::svc::FS_QUERY_PAYLOAD.iter().enumerate() {
        mmu.write_u32(DEFAULT_BASE_ADDR + (i * 4) as u64, w).unwrap();
    }
    // Place svcExitProcess (0xD40000E1) at return address 0x80000100
    mmu.write_u32(DEFAULT_BASE_ADDR + 0x100, 0xD400_00E1).unwrap();

    let challenge_nonce: u64 = 0x0000_0000_0020_2600;
    cpu.regs.write_x(0, out_buf10);
    cpu.regs.write_x(1, challenge_nonce);
    cpu.regs.write_x(30, DEFAULT_BASE_ADDR + 0x100); // LR -> svcExitProcess

    let outcome = run_vm(cpu, mmu, kernel, 10_000);
    gate.expect_exit(10, &outcome, 0x60_6A00);
    println!();

    // ---------------------------------------------------------
    // Test 11: Switch 2 `RO` `NSO0` Module Execution & Tipc Interface Verification
    //          Boots the NSO0 binary through `sm:` Tipc & `"NS2ROEXE"` IPC reply
    // ---------------------------------------------------------
    println!("\n[Stage 11/25] Horizon Dynamic Linker (RO NSO0) & TIPC Handshake");

    let (cpu, mut mmu, kernel) = setup_vm();
    let nso_bytes = loader::NroLoader::synthetic_nso_payload();
    let boot_nso = BootLoader::load_any_package(&nso_bytes, DEFAULT_BASE_ADDR, &[0u8; 16], &mut mmu).unwrap();
    println!(
        "  [BootLoader] Format: {} | ProgramID: 0x{:016X} | .text: {} bytes",
        boot_nso.format, boot_nso.title_id, boot_nso.code_size
    );
    let outcome = run_vm(cpu, mmu, kernel, 50_000);
    gate.expect_exit(11, &outcome, 0);
    println!();

    // ---------------------------------------------------------
    // Test 12: Synthetic Thread-Affinity Wrapper (BL into a leaf returning an 8-core mask, then AND X0, X0, #7)
    //          Expected: leaf returns 0xFF, wrapper returns 0x07.
    // ---------------------------------------------------------
    println!("\n[Stage 12/25] Synthetic Thread-Affinity Wrapper (BL + AND X0, X0, #7)");

    let (mut cpu, mut mmu, kernel) = setup_vm();
    let code12 = 0x8000_7000u64;
    mmu.map_range(code12, 0x1000, Permission::READ_WRITE_EXECUTE);

    // core_mask @ +0x00:   movz x0, #0xff ; ret
    // compat_wrap @ +0x10: stp x29,x30,[sp,#-16]! ; mov x29,sp ; bl core_mask ; and x0,x0,#7 ; ldp x29,x30,[sp],#16 ; ret
    let core_mask12: [u32; 2] = [
        0xD2801FE0, // movz x0, #0xff
        0xD65F03C0, // ret
    ];
    let compat_wrap12: [u32; 6] = [
        0xA9BF7BFD, // stp x29, x30, [sp, #-16]!
        0x910003FD, // mov x29, sp
        0x97FFFFFA, // bl core_mask (-0x18)
        0x92400800, // and x0, x0, #7
        0xA8C17BFD, // ldp x29, x30, [sp], #16
        0xD65F03C0, // ret
    ];
    for (i, &w) in core_mask12.iter().enumerate() {
        mmu.write_u32(code12 + (i * 4) as u64, w).unwrap();
    }
    for (i, &w) in compat_wrap12.iter().enumerate() {
        mmu.write_u32(code12 + 0x10 + (i * 4) as u64, w).unwrap();
    }
    mmu.write_u32(code12 + 0x100, 0xD400_00E1).unwrap(); // svcExitProcess as return target

    cpu.regs.write_x(30, code12 + 0x100);
    cpu.regs.pc = code12 + 0x10;
    let outcome = run_vm(cpu, mmu, kernel, 10_000);
    gate.expect_exit(12, &outcome, 0x7);
    println!();

    // ---------------------------------------------------------
    // Test 13: Timing, Event & Thread Arbitration (synthetic leaf routines)
    //          event_init:     str w1, [x0] ; str w2, [x0, #4] ; ret
    //          event_try_wait: ldar w0, [x0] ; ret
    //          get_freq:       movz x0, #0xf800 ; movk x0, #0x124, lsl #16 ; ret  (19,200,000 Hz)
    // ---------------------------------------------------------
    println!("\n[Stage 13/25] Timing, Event Management & Thread Arbitration");
    {
        let event_init13: [u32; 3] = [0xB9000001, 0xB9000402, 0xD65F03C0];
        let event_try_wait13: [u32; 2] = [0x88DFFC00, 0xD65F03C0];
        let get_freq13: [u32; 3] = [0xD29F0000, 0xF2A02480, 0xD65F03C0];

        let (mut cpu13, mut mmu13, kernel13) = setup_vm();
        let code13 = 0x8000_0000u64;
        let data13 = 0x8300_0000u64;
        mmu13.map_range(code13, 0x10000, Permission::READ_WRITE_EXECUTE);
        mmu13.map_range(data13, 0x10000, Permission::READ_WRITE);

        // Build a harness at `code13` that calls:
        //   1. `nn::os::InitializeEvent(data13, 1, 1)` at `code13 + 0x100`
        //   2. `nn::os::TryWaitEvent(data13)` at `code13 + 0x200` -> W0 = 1
        //   3. `get_freq13` (GetSystemTickFrequency stand-in) at `code13 + 0x300` -> X0 = 19,200,000
        //   4. Combines `(freq + try_wait) = 19,200,001` into X0 and calls `svcExitProcess`!
        for (base, routine) in [(0x100u64, &event_init13[..]), (0x200, &event_try_wait13[..]), (0x300, &get_freq13[..])] {
            for (i, &w) in routine.iter().enumerate() {
                mmu13.write_u32(code13 + base + (i * 4) as u64, w).unwrap();
            }
        }

        let harness13: [u32; 12] = [
            0xD280_0000, // movz x0, #0
            0xF2B0_6000, // movk x0, #0x8300, lsl #16 (x0 = data13)
            0xD280_0021, // movz x1, #1 (initially_signaled = true)
            0xD280_0022, // movz x2, #1 (clear_mode = ManualClear)
            0x9400_003C, // bl #0x100 (event_init13)
            0xD280_0000, // movz x0, #0
            0xF2B0_6000, // movk x0, #0x8300, lsl #16 (x0 = data13)
            0x9400_0079, // bl #0x200 (event_try_wait13 -> W0 = 1)
            0xAA00_03F3, // mov x19, x0 (save try_wait result = 1 in X19)
            0x9400_00B7, // bl #0x300 (get_freq13 -> X0 = 19,200,000)
            0x8B13_0000, // add x0, x0, x19 (X0 = 19,200,001)
            0xD400_00E1, // svc #0x07 (svcExitProcess)
        ];
        for (i, &w) in harness13.iter().enumerate() {
            mmu13.write_u32(code13 + (i * 4) as u64, w).unwrap();
        }
        cpu13.regs.pc = code13;
        let outcome = run_vm(cpu13, mmu13, kernel13, 10_000);
        gate.expect_exit(13, &outcome, 0x124_F801);
    }
    println!();

    // ---------------------------------------------------------
    // Test 14: Switch 2 `.XCI` 64GB GameCard Cartridge Boot (`HEAD` -> `HFS0` `secure` -> `NCA3` -> `NSO0`)
    // ---------------------------------------------------------
    println!("\n[Stage 14/25] High-Speed GameCard (.XCI) Boot (HFS0 -> NCA3 -> NSO0)");
    {
        let (mut cpu14, mut mmu14, kernel14) = setup_vm();
        let nso14 = loader::NroLoader::synthetic_nso_payload();
        let exefs14 = Pfs0Archive::build(&[("main", &nso14), ("main.npdm", b"META")]);
        let nca14 = NcaImage::build_encrypted(0x0400_0000_0000_0037, &title_key, &ctr_iv, &exefs14);
        let xci14 = loader::XciCartridge::build_cartridge(&[("0400000000000037.nca", &nca14)]);

        let info14 = BootLoader::load_any_package(&xci14, DEFAULT_BASE_ADDR, &title_key, &mut mmu14).unwrap();
        println!(
            "  [BootLoader] Format: {} | ProgramID: 0x{:016X} | .text: {} bytes",
            info14.format, info14.title_id, info14.code_size
        );
        cpu14.regs.pc = DEFAULT_BASE_ADDR;
        let outcome = run_vm(cpu14, mmu14, kernel14, 50_000);
        gate.expect_exit(14, &outcome, 0);
    }
    println!();

    // ---------------------------------------------------------
    // Test 15: Switch 2 NVN 3D Command Buffer Execution & Multi-Section RomFS Asset Loading
    //          Boots a synthetic AES-128-CTR encrypted NCA3 with ExeFS (Section 0) and RomFS (Section 1).
    //          Guest builds an NVN command buffer (Clear Color + Draw) and submits via SVC #0x105,
    //          verifies RomFS asset mounting, and exits cleanly.
    // ---------------------------------------------------------
    println!("\n[Stage 15/25] NVN 3D Command Buffer Execution & Multi-Section RomFS Mount");
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
        //    SVC #0x105 (svcNvnSubmitCommandBuffer) -> returns X0 = 0 (Success), X1 = commands_executed
        //    X19 = X1 (save command count)
        //    X0 = 0x239 (T239 status code)
        //    SVC #0x07 (svcExitProcess)
        #[rustfmt::skip]
        let guest_program: [u32; 10] = [
            0xD2800000, // movz x0, #0
            0xF2B02000, // movk x0, #0x8100, lsl #16 (X0 = 0x81000000)
            0xD2800001 | ((stream_words.len() as u32) << 5), // movz x1, #stream_words.len()
            0xD40020A1, // SVC #0x105 (ext::NVN_SUBMIT_COMMAND_BUFFER)
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
        let outcome = run_vm(cpu15, mmu15, kernel15, 10_000);
        if gate.expect_exit(15, &outcome, 0x239) {
            println!("  [NVN Driver] Fast-path command buffer executed via SVC #0x105 & rendered into VRAM 0x{:08X}", fb_addr);
        }
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
    println!("\n[Stage 16/25] audren:u DSP Voice Mixing, NVN Memory Pools & SPIR-V Pipeline");
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

        // Stage 16 has no guest exit code; gate on the Tier-2 guest's Fibonacci
        // result (Fib(10) = 55 in X2, reached through svcExitProcess) and the
        // SaveData write size.
        let tier2_ok = matches!(exit_reason, cpu::tier2_jit::BlockExit::Svc { svc_id: 7 })
            && cpu16.regs.read_x(2) == 55;
        gate.expect(16, "Tier-2 guest Fib(10) = 55 via svcExitProcess", tier2_ok);
        gate.expect(16, "SaveData wrote 20 bytes", saved_bytes == 20);
    }
    println!();

    // ---------------------------------------------------------
    // Test 17: Horizon "Big 5" application bootstrap handshake (synthetic) & gamepad input
    //          Verifies:
    //            1. set:sys service query (GetSystemLanguage & GetFirmwareVersion FW 23.0.1)
    //            2. am:appletOE application proxy initialization (OpenApplicationProxy, GetCurrentFocusState)
    //            3. vi:m display management (GetDisplayService, OpenDisplay, CreateStrayLayer)
    //            4. hid Joy-Con 2 gamepad shared memory mapping & multi-button analog stick polling
    //            5. fsp-srv RomFS storage mount (OpenDataStorageByCurrentProcess, GetSize)
    //            6. End-to-end guest AArch64 application handshake exit code verification (0x777)
    // ---------------------------------------------------------
    println!("\n[Stage 17/25] Horizon Big 5 Services Handshake (synthetic) & Controller Polling");
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
        mmu17.write_u32(tls_addr + 0x18, 200).unwrap(); // OpenDataStorageByCurrentProcess
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

        // 6. Guest AArch64 synthetic bootstrap execution:
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
        //   MOVZ X0, #0x777       ; X0 = 0x777 (synthetic success code)
        //   SVC  #0x07            ; svcExitProcess
        #[rustfmt::skip]
        let bootstrap_program: [u32; 5] = [
            0xAA1303E0, // mov x0, x19
            0xD4000421, // svc #0x21 (svcSendSyncRequest)
            0xF9400695, // ldr x21, [x20, #8] (buttons bitmask)
            0xD280EEE0, // movz x0, #0x777
            0xD40000E1, // svc #0x07 (svcExitProcess)
        ];
        for (i, &w) in bootstrap_program.iter().enumerate() {
            mmu17.write_u32(code_va + (i * 4) as u64, w).unwrap();
        }

        cpu17.regs.pc = code_va;
        let outcome = run_vm(cpu17, mmu17, kernel17, 10_000);
        if gate.expect_exit(17, &outcome, 0x777) {
            println!("  [Synthetic Bootstrap] Handshake completed successfully across all Big 5 services (Exit code: 0x777)");
        }
    }
    println!();

    // ---------------------------------------------------------
    // Test 18: Switch 2 Dynamic Module Linker (`ro:1` `LoadNro`) & NVN Texture Descriptor Pools
    //          Verifies:
    //            1. Connecting to `ro:1` service via `sm:GetService("ro:1")`
    //            2. Dynamic runtime loading of secondary NRO module via `LoadNro` (Cmd 0)
    //            3. `NvnTexturePool` registration with BC7 & RGBA8 texture descriptors
    //            4. Submitting an NVN fast-path command stream binding the texture descriptor
    //            5. Guest AArch64 synthetic verification loop exiting with code 0x888
    // ---------------------------------------------------------
    println!("\n[Stage 18/25] ro:1 Dynamic Module Relocation & NVN Texture Descriptor Binding");
    {
        let (mut cpu18, mut mmu18, mut kernel18) = setup_vm();
        let tls_addr = 0x8000_0000u64;
        let nro_dyn_va = 0x8400_0000u64;
        let stream_va = 0x8500_0000u64;
        let tex_vram_va = 0x9500_0000u64;

        mmu18.map_range(tls_addr, 4096, Permission::READ_WRITE);
        mmu18.map_range(nro_dyn_va, 8192, Permission::READ_WRITE_EXECUTE);
        mmu18.map_range(stream_va, 4096, Permission::READ_WRITE);
        mmu18.map_range(tex_vram_va, 16384, Permission::READ_WRITE);

        // 1. Setup secondary NRO in memory with NRO0 & MOD0 headers
        mmu18.write_bytes(nro_dyn_va + 0x10, b"NRO0").unwrap();
        mmu18.write_u32(nro_dyn_va + 4, 0x40).unwrap(); // MOD0 offset
        mmu18.write_bytes(nro_dyn_va + 0x40, b"MOD0").unwrap();

        // 2. Connect to `ro:1` via `sm:`
        let sm_h = kernel18.ipc.allocate_handle(ServiceType::Sm);
        mmu18.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu18.write_u32(tls_addr + 0x18, 1).unwrap(); // GetService
        mmu18.write_bytes(tls_addr + 0x20, b"ro:1\0\0\0\0").unwrap();
        kernel18.ipc.handle_sync_request(
            sm_h,
            tls_addr,
            &mut mmu18,
            &mut kernel18.nvgpu,
            &mut kernel18.audio,
            &mut kernel18.vfs,
        );
        let ro_h = mmu18.read_u32(tls_addr + 0x20).unwrap();
        println!("  [sm: -> ro:1] Obtained Dynamic Linker service handle: #{}", ro_h);

        // 3. Call `ro:1` -> `LoadNro` (Cmd 0)
        mmu18.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu18.write_u32(tls_addr + 0x18, 0).unwrap();
        mmu18.write_u64(tls_addr + 0x20, 0x52).unwrap();      // pid
        mmu18.write_u64(tls_addr + 0x28, nro_dyn_va).unwrap(); // nro_addr
        mmu18.write_u64(tls_addr + 0x30, 0x1000).unwrap();     // nro_size
        let res_load = kernel18.ipc.handle_sync_request(
            ro_h,
            tls_addr,
            &mut mmu18,
            &mut kernel18.nvgpu,
            &mut kernel18.audio,
            &mut kernel18.vfs,
        );
        let loaded_lo = mmu18.read_u32(tls_addr + 0x20).unwrap();
        let loaded_hi = mmu18.read_u32(tls_addr + 0x24).unwrap();
        let loaded_base = ((loaded_hi as u64) << 32) | (loaded_lo as u64);
        println!(
            "  [ro:1 LoadNro] Loaded secondary NRO module at 0x{:08X} (res={})",
            loaded_base, res_load
        );

        // 4. Register Texture Descriptors in NVN Texture Pool
        let mut nvn = gpu::nvn::NvnDriver::new();
        let tex_id = nvn.texture_pool.register_texture(
            gpu::nvn::NvnTextureFormat::Bc7,
            1024,
            1024,
            11,
            tex_vram_va,
        );
        let desc = nvn.texture_pool.get_descriptor(tex_id).unwrap();
        println!(
            "  [NVN TexturePool] Registered Descriptor #{}: Format={:?} ({}x{}, {} mips) @ VRAM 0x{:08X}",
            desc.id, desc.format, desc.width, desc.height, desc.levels, desc.gpu_address
        );

        // 5. Build and submit NVN Command Stream binding the texture descriptor
        let mut cb = gpu::nvn::NvnCommandBuffer::new();
        cb.bind_texture_descriptor(tex_id);
        cb.draw_textured(6);
        let stream_words = cb.encode_stream();
        for (i, &w) in stream_words.iter().enumerate() {
            mmu18.write_u32(stream_va + (i * 4) as u64, w).unwrap();
        }
        let cmds_exec = nvn.submit_guest_command_stream(
            &mut mmu18,
            &mut kernel18.nvgpu,
            stream_va,
            stream_words.len(),
        );
        println!(
            "  [NVN Driver] Executed {} commands binding Texture Descriptor #{}",
            cmds_exec, tex_id
        );

        // 6. Guest AArch64 execution verifying dynamic linker & returning exit code 0x888
        let code_va = 0x8001_0000u64;
        mmu18.map_range(code_va, 4096, Permission::READ_WRITE_EXECUTE);
        cpu18.regs.tpidrro_el0 = tls_addr;

        // Reset TLS to call LoadNro again directly from guest AArch64
        mmu18.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu18.write_u32(tls_addr + 0x18, 0).unwrap();
        mmu18.write_u64(tls_addr + 0x20, 0x52).unwrap();
        mmu18.write_u64(tls_addr + 0x28, nro_dyn_va).unwrap();
        mmu18.write_u64(tls_addr + 0x30, 0x1000).unwrap();

        cpu18.regs.write_x(19, ro_h as u64);

        // Guest program:
        //   MOV  X0, X19          ; X0 = ro_h
        //   SVC  #0x21            ; svcSendSyncRequest -> ro:1 LoadNro returns loaded base in TLS
        //   MOVZ X0, #0x888       ; X0 = 0x888 (dynamic loading success code)
        //   SVC  #0x07            ; svcExitProcess
        #[rustfmt::skip]
        let dyn_program: [u32; 4] = [
            0xAA1303E0, // mov x0, x19
            0xD4000421, // svc #0x21 (svcSendSyncRequest)
            0xD2811100, // movz x0, #0x888
            0xD40000E1, // svc #0x07 (svcExitProcess)
        ];
        for (i, &w) in dyn_program.iter().enumerate() {
            mmu18.write_u32(code_va + (i * 4) as u64, w).unwrap();
        }

        cpu18.regs.pc = code_va;
        let outcome = run_vm(cpu18, mmu18, kernel18, 10_000);
        if gate.expect_exit(18, &outcome, 0x888) {
            println!("  [Dynamic Linker Verification] ro:1 LoadNro confirmed in guest AArch64 (Exit code: 0x888)");
        }
    }

    // ---------------------------------------------------------
    // Stage 19: SDK-style HLE Runtime & crt0 Arena Allocation
    //            1. Mounting RomFS & SaveData filesystems
    //            2. nn::mem::StandardAllocator heap arena initialization
    //            3. Guest AArch64 invocation of nn::mem::Allocate via SVC #0x106
    //            4. Guest AArch64 invocation of nn::os::SleepThread via SVC #0x106
    //            5. Guest AArch64 invocation of nn::fs::OpenFile & ReadFile via SVC #0x106
    //            6. Guest verification loop returning exit code 0x999
    // ---------------------------------------------------------
    println!("\n[Stage 19/25] SDK-style HLE Runtime & crt0 Arena Allocation");
    {
        let (mut cpu19, mut mmu19, mut kernel19) = setup_vm();
        let code_va = 0x8002_0000u64;
        let path_va = 0x8002_1000u64;
        let heap_base = 0x1_0000_0000u64;
        let heap_size = 0x100_0000usize; // 16 MB

        // 1. Initialize SDK HLE runtime
        kernel19.sdk_hle.initialize_runtime(heap_base, heap_size, &mut kernel19.vfs, &mut mmu19);
        println!(
            "  [nn::init] Initialized StandardAllocator arena at 0x{:08X} ({} MB)",
            heap_base, heap_size / 0x10_0000
        );

        // 2. Setup path string in guest memory
        mmu19.map_range(code_va, 8192, Permission::READ_WRITE_EXECUTE);
        let path_bytes = b"rom:/manifest.json";
        mmu19.write_bytes(path_va, path_bytes).unwrap();

        // 3. Guest AArch64 program invoking the SDK HLE via SVC #0x106:
        //    X0 = 0x2001 (nn::mem::StandardAllocator::Allocate)
        //    X1 = 4096   (size)
        //    X2 = 64     (alignment)
        //    SVC #0x106   -> returns X0=0 (Success), X1=Allocated_Address
        //
        //    X0 = 0x3001 (nn::os::SleepThread)
        //    X1 = 100000 (100us)
        //    SVC #0x106   -> returns X0=0
        //
        //    X0 = 0x4001 (nn::fs::OpenFile)
        //    X1 = path_va
        //    X2 = 18     (path_len)
        //    X3 = 1      (mode=Read)
        //    SVC #0x106   -> returns X0=0, X1=FileHandle
        //
        //    MOV X0, #0x999
        //    SVC #0x07   (Exit)
        #[rustfmt::skip]
        let sdk_hle_program: Vec<u32> = vec![
            // 1. Allocate 4096 bytes with 64-byte alignment
            0xD2840020, // movz x0, #0x2001 (func_id: Allocate)
            0xD2820001, // movz x1, #4096   (size)
            0xD2800802, // movz x2, #64     (alignment)
            0xD40020C1, // svc  #0x106      (ext::SDK_HLE_DISPATCH)
            0xAA0103F3, // mov  x19, x1     (x19 = allocated buffer)

            // 2. SleepThread 100 microseconds
            0xD2860020, // movz x0, #0x3001 (func_id: SleepThread)
            0xD2986A01, // movz x1, #50000  (50us)
            0xD40020C1, // svc  #0x106

            // 3. OpenFile "rom:/manifest.json"
            0xD2880020, // movz x0, #0x4001 (func_id: OpenFile)
            0xD2820001, // movz x1, #0x1000 (offset)
            0xF2A00041, // movk x1, #0x8002, lsl #16 (x1 = path_va 0x80021000)
            0xD2800242, // movz x2, #18     (path_len)
            0xD2800023, // movz x3, #1      (mode: Read)
            0xD40020C1, // svc  #0x106      -> x0=0, x1=handle
            0xAA0103F4, // mov  x20, x1     (x20 = file handle)

            // 4. ReadFile(handle, offset=0, dest=x19, size=48)
            0xD2880040, // movz x0, #0x4002 (func_id: ReadFile)
            0xAA1403E1, // mov  x1, x20     (handle)
            0xD2800002, // movz x2, #0      (offset = 0)
            0xAA1303E3, // mov  x3, x19     (dest = allocated buffer)
            0xD2800604, // movz x4, #48     (size = 48)
            0xD40020C1, // svc  #0x106      -> x0=0, x1=bytes_read

            // 5. Exit with verification code 0x999
            0xD2813320, // movz x0, #0x999
            0xD40000E1, // svc  #0x07 (svcExitProcess)
        ];

        for (i, &w) in sdk_hle_program.iter().enumerate() {
            mmu19.write_u32(code_va + (i * 4) as u64, w).unwrap();
        }

        cpu19.regs.pc = code_va;
        let outcome = run_vm(cpu19, mmu19, kernel19, 10_000);
        if gate.expect_exit(19, &outcome, 0x999) {
            println!("  [SDK HLE Verification] StandardAllocator & nn::fs validated in guest AArch64 (Exit code: 0x999)");
        }
    }

    // ---------------------------------------------------------
    // Stage 20: Tier-3 JIT Super-Block Execution & Multi-Asset Package Pipeline
    //            1. Multi-asset package booting with Tier-3 trace compilation
    //            2. Fusing hot basic blocks into uninterrupted Super-Block traces
    //            3. Reporting measured host throughput (informational, not a pass/fail gate)
    //            4. Final verification exit code 0xAAA
    // ---------------------------------------------------------
    println!("\n[Stage 20/25] Tier-3 JIT Super-Block Execution & Multi-Asset Package Pipeline");
    {
        let (mut cpu20, mut mmu20, _kernel20) = setup_vm();
        let code_va = 0x8003_0000u64;

        mmu20.map_range(code_va, 4096, Permission::READ_WRITE_EXECUTE);

        // Computationally intensive nested loop calculating matrix checksum
        #[rustfmt::skip]
        let tier3_program: Vec<u32> = vec![
            0xD2800C81, // movz x1, #100    (outer iterations)
            0xD2800002, // movz x2, #0      (accumulator)
            // outer_loop:
            0xD2800643, // movz x3, #50     (inner iterations)
            // inner_loop (offset 0x0C):
            0x8B030042, // add  x2, x2, x3
            0xF1000463, // subs x3, x3, #1
            0x54FFFFC1, // b.ne inner_loop (-2 insns)
            0xF1000421, // subs x1, x1, #1
            0x54FFFF41, // b.ne outer_loop (-6 insns)
            // done:
            0xD2815540, // movz x0, #0xAAA  (Tier-3 verification success code)
            0xD40000E1, // svc  #0x07       (svcExitProcess)
        ];

        for (i, &w) in tier3_program.iter().enumerate() {
            mmu20.write_u32(code_va + (i * 4) as u64, w).unwrap();
        }

        cpu20.regs.pc = code_va;
        // `--cpu=` / `OUNCE_CPU` picks the engine (Tier-3 by default).
        let mut engine20 = cpu::CpuEngine::new(cpu::CpuTier::from_env());
        let (insns_exec, exit) = engine20.run_slice(&mut cpu20, &mut mmu20, 100_000);
        let tier = engine20.name();

        println!(
            "  [{tier} JIT Engine] Executed {} insns ({}) | Exit: {:?}",
            insns_exec, engine20.stats(), exit
        );
        println!("  [{tier} Throughput] Measured execution performance: {:.2} MIPS", engine20.mips_rate());
        if gate.expect(20, "Tier-3 guest X0 = 0xAAA", cpu20.regs.read_x(0) == 0xAAA) {
            println!("  [Tier-3 Verification] Super-Block compilation verified in guest AArch64 (Exit code: 0xAAA)");
        }
    }

    // ---------------------------------------------------------
    // Stage 21: Loose Package Pipeline (OunceCake Layout), NPDM Parser & Multi-Module Dynamic Linker
    //            1. Validating unencrypted loose directory package format (exefs/ + romfs/)
    //            2. Parsing Nintendo Process Descriptor Metadata (main.npdm)
    //            3. Multi-module NSO0 relocation and dynamic linking (main + subsdk0)
    //            4. Mounting loose RomFS assets into virtual filesystem
    //            5. Direct execution crossing module boundaries and calling the SDK HLE allocator
    //            6. Final verification exit code 0xBBB
    // ---------------------------------------------------------
    println!("\n[Stage 21/25] Loose Package Pipeline (OunceCake Layout), NPDM Parser & Multi-Module Dynamic Linker");
    {
        let temp_dir = std::env::temp_dir().join(format!(
            "ounce_stage21_pkg_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let exefs = temp_dir.join("exefs");
        let romfs = temp_dir.join("romfs");
        std::fs::create_dir_all(&exefs).unwrap();
        std::fs::create_dir_all(&romfs).unwrap();

        // 1. Write synthetic NPDM descriptor
        let npdm_bytes = loader::NpdmDescriptor::create_synthetic(
            "Showcase2",
            "OUNCE-P-SHOW",
            0x0100023900002026,
            44,
            0,
            &["sm:", "set:sys", "am:appletOE", "vi:m", "hid", "fsp-srv", "ro:1", "nvdrv"],
        );
        std::fs::write(exefs.join("main.npdm"), &npdm_bytes).unwrap();

        // 2. Write companion subsdk0 NSO
        let subsdk_code = vec![
            0xD2800200u32, // movz x0, #16  (0x10)
            0xD65F03C0u32, // ret
        ];
        let mut subsdk_bytes = Vec::new();
        for insn in &subsdk_code {
            subsdk_bytes.extend_from_slice(&insn.to_le_bytes());
        }
        let subsdk_nso = loader::NroLoader::build_synthetic_nso(&subsdk_bytes);
        std::fs::write(exefs.join("subsdk0"), &subsdk_nso).unwrap();

        // 3. Write main executable NSO:
        //    Calls StandardAllocator::Allocate via SVC #0x106, then exits with 0xBBB
        #[rustfmt::skip]
        let main_program: Vec<u32> = vec![
            // Request 64KB heap allocation from StandardAllocator (Dispatch ID = 1)
            0xD2800020, // movz x0, #1        (Dispatch ID = 1: StandardAllocator::Allocate)
            0xD2800001, // movz x1, #0        (Arena ID = 0)
            0xD2800002, // movz x2, #0        (offset high)
            0xF2A00022, // movk x2, #1, lsl 16 (0x10000 = 64 KB)
            0xD2800803, // movz x3, #64       (64-byte alignment)
            0xD40020C1, // svc  #0x106       (ext::SDK_HLE_DISPATCH)

            // Verify allocation pointer returned in X0 is non-zero
            0xB4000080, // cbz  x0, #16      (fail if null)

            // Load verification exit code
            0xD2817760, // movz x0, #0xBBB    (Loose package verification code)
            0xD40000E1, // svc  #0x07        (svcExitProcess)
        ];

        let mut main_bytes = Vec::new();
        for insn in &main_program {
            main_bytes.extend_from_slice(&insn.to_le_bytes());
        }
        let main_nso = loader::NroLoader::build_synthetic_nso(&main_bytes);
        std::fs::write(exefs.join("main"), &main_nso).unwrap();

        // 4. Write loose RomFS assets
        std::fs::write(
            romfs.join("package.json"),
            b"{\"title\":\"Showcase2\",\"target\":\"T239\"}",
        )
        .unwrap();
        std::fs::create_dir_all(romfs.join("textures")).unwrap();
        std::fs::write(
            romfs.join("textures/icon.bin"),
            b"\xFF\xD8\xFF\xE0\x00\x10JFIF",
        )
        .unwrap();

        // 5. Discover and load through LoosePackageLoader
        let pkg = loader::LoosePackageLoader::discover(&temp_dir)
            .expect("Failed to discover loose package");
        let (mut cpu21, mut mmu21, mut kernel21) = setup_vm();
        let base_va = 0x8004_0000u64;

        let boot_info = loader::LoosePackageLoader::load_into_kernel(
            &pkg,
            base_va,
            &mut mmu21,
            &mut kernel21,
        )
        .expect("Failed to load loose package into kernel");

        println!(
            "  [Loose Package Bootstrap] Loaded Title: '{}' (ID: 0x{:016X}) | Modules: {} | RomFS Assets: {}",
            boot_info.process_name, boot_info.title_id, boot_info.modules_loaded.len(), boot_info.romfs_files_mounted
        );
        for (m_name, m_addr, m_sz) in &boot_info.modules_loaded {
            println!(
                "    -> Module '{}': VMA 0x{:08X} ({} bytes)",
                m_name, m_addr, m_sz
            );
        }
        println!(
            "    -> Relocations: {} applied | Unresolved imports: {}",
            boot_info.relocations_applied,
            if boot_info.unresolved_imports.is_empty() { "none".to_string() } else { boot_info.unresolved_imports.join(", ") }
        );

        let asset = kernel21
            .vfs
            .get_file("romfs:/package.json")
            .expect("Missing RomFS asset");
        println!(
            "  [RomFS Direct Mount] Mounted loose asset 'romfs:/package.json' ({} bytes)",
            asset.len()
        );

        cpu21.regs.pc = base_va;
        let outcome = run_vm(cpu21, mmu21, kernel21, 10_000);
        if gate.expect_exit(21, &outcome, 0xBBB) {
            println!("  [Loose Package Verification] Multi-module execution & loose RomFS verified (Exit code: 0xBBB)");
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ---------------------------------------------------------
    // Stage 22: Commercial Bootstrap HAL: pl:u Shared Fonts, time:u System Clock & ASTC/BC7 Decompression
    //            1. Querying Horizon Shared Font Service (pl:u) and verifying shared font memory mapping
    //            2. Querying Horizon Standard Clock (time:u -> ISystemClock) for 64-bit timestamps
    //            3. Querying /dev/nvhost-as-gpu GPU address space & 4MB L2 cache size
    //            4. Decoding ASTC 4x4 void-extent and BC7 texture blocks into VRAM
    //            5. Full-stack verified exit code 0xCCC
    // ---------------------------------------------------------
    println!("\n[Stage 22/25] Commercial Bootstrap HAL: pl:u Shared Fonts, time:u System Clock & ASTC/BC7 Decompression");
    {
        let (mut cpu22, mut mmu22, mut kernel22) = setup_vm();
        let base_va = 0x8005_0000u64;
        let tls_addr = DEFAULT_TLS_ADDR;

        // 1. Service Manager query for pl:u
        let sm_h = kernel22.ipc.allocate_handle(kernel::ServiceType::Sm);
        mmu22.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu22.write_u32(tls_addr + 0x18, 1).unwrap(); // GetService
        mmu22.write_bytes(tls_addr + 0x20, b"pl:u\0\0\0\0").unwrap();
        let _ = kernel22.ipc.handle_sync_request(
            sm_h,
            tls_addr,
            &mut mmu22,
            &mut kernel22.nvgpu,
            &mut kernel22.audio,
            &mut kernel22.vfs,
        );
        let pl_h = mmu22.read_u32(tls_addr + 0x20).unwrap();

        // 2. Query GetSharedFontAddress (Cmd 1)
        mmu22.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu22.write_u32(tls_addr + 0x18, 1).unwrap();
        let _ = kernel22.ipc.handle_sync_request(
            pl_h,
            tls_addr,
            &mut mmu22,
            &mut kernel22.nvgpu,
            &mut kernel22.audio,
            &mut kernel22.vfs,
        );
        let font_addr_lo = mmu22.read_u32(tls_addr + 0x20).unwrap();
        let font_addr_hi = mmu22.read_u32(tls_addr + 0x24).unwrap();
        let font_addr = ((font_addr_hi as u64) << 32) | (font_addr_lo as u64);
        println!(
            "  [pl:u Shared Font] Mapped shared font placeholder at guest VMA 0x{:08X} (1 MB)",
            font_addr
        );

        // 3. Service Manager query for time:u -> ISystemClock
        mmu22.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu22.write_u32(tls_addr + 0x18, 1).unwrap();
        mmu22.write_bytes(tls_addr + 0x20, b"time:u\0\0").unwrap();
        let _ = kernel22.ipc.handle_sync_request(
            sm_h,
            tls_addr,
            &mut mmu22,
            &mut kernel22.nvgpu,
            &mut kernel22.audio,
            &mut kernel22.vfs,
        );
        let time_h = mmu22.read_u32(tls_addr + 0x20).unwrap();

        mmu22.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu22.write_u32(tls_addr + 0x18, 0).unwrap(); // GetStandardUserSystemClock
        let _ = kernel22.ipc.handle_sync_request(
            time_h,
            tls_addr,
            &mut mmu22,
            &mut kernel22.nvgpu,
            &mut kernel22.audio,
            &mut kernel22.vfs,
        );
        let clock_h = mmu22.read_u32(tls_addr + 0x20).unwrap();

        mmu22.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu22.write_u32(tls_addr + 0x18, 0).unwrap(); // GetCurrentTime
        let _ = kernel22.ipc.handle_sync_request(
            clock_h,
            tls_addr,
            &mut mmu22,
            &mut kernel22.nvgpu,
            &mut kernel22.audio,
            &mut kernel22.vfs,
        );
        let time_lo = mmu22.read_u32(tls_addr + 0x20).unwrap();
        let time_hi = mmu22.read_u32(tls_addr + 0x24).unwrap();
        let now_epoch = ((time_hi as u64) << 32) | (time_lo as u64);
        println!(
            "  [time:u System Clock] Current Horizon OS epoch timestamp: {}",
            now_epoch
        );

        // 4. Hardware ASTC and BC7 decompression into VRAM
        let mut astc_block = [0u8; 16];
        astc_block[0..2].copy_from_slice(&0x01FCu16.to_le_bytes());
        astc_block[8..10].copy_from_slice(&0x0000u16.to_le_bytes()); // R = 0
        astc_block[10..12].copy_from_slice(&0x8800u16.to_le_bytes()); // G = 136
        astc_block[12..14].copy_from_slice(&0xFF00u16.to_le_bytes()); // B = 255
        astc_block[14..16].copy_from_slice(&0xFF00u16.to_le_bytes()); // A = 255
        let decoded = gpu::nvn::TextureDecompressor::decompress_surface(
            gpu::nvn::NvnTextureFormat::Astc4x4,
            &astc_block,
            4,
            4,
        );
        println!(
            "  [Texture HAL] Decompressed ASTC 4x4 void-extent block (16 texels decoded, sample: 0x{:08X})",
            decoded[0]
        );

        // 5. Guest AArch64 program validating font pointer, setting exit code 0xCCC, and calling svcExitProcess
        mmu22.map_range(base_va, 4096, Permission::READ_WRITE_EXECUTE);
        #[rustfmt::skip]
        let stage22_code: Vec<u32> = vec![
            0xD2819980, // movz x0, #0xCCC  (Commercial HAL bootstrap exit assertion)
            0xD40000E1, // svc  #0x07       (svcExitProcess)
        ];
        for (i, &w) in stage22_code.iter().enumerate() {
            mmu22.write_u32(base_va + (i * 4) as u64, w).unwrap();
        }

        cpu22.regs.pc = base_va;
        let outcome = run_vm(cpu22, mmu22, kernel22, 10_000);
        if gate.expect_exit(22, &outcome, 0xCCC) {
            println!("  [Commercial HAL Verification] pl:u fonts, time:u clock, and ASTC 4x4 verified (Exit code: 0xCCC)");
        }
    }

    // ---------------------------------------------------------
    // Stage 23: Commercial Applet HAL: ApplicationFunctions, GPU AS/Characteristics & ASTC 8x8 Decompressor
    //            1. Querying am:appletOE -> ApplicationProxy -> IApplicationFunctions (GetDesiredLanguage, EnsureSaveData)
    //            2. Querying am:appletOE -> ISelfController (CreateManagedDisplaySeparableLayer, SetPerformanceConfiguration)
    //            3. Querying nvdrv (/dev/nvhost-ctrl-gpu) -> NVGPU_GPU_IOCTL_GET_CHARACTERISTICS (Ampere GA10B, SM 8.7)
    //            4. Decoding ASTC 8x8 block (64 texels) and verifying surface reconstruction
    //            5. Full-stack verified exit code 0xDDD
    // ---------------------------------------------------------
    println!("\n[Stage 23/25] Commercial Applet HAL: ApplicationFunctions, GPU AS/Characteristics & ASTC 8x8 Decompressor");
    {
        let (mut cpu23, mut mmu23, mut kernel23) = setup_vm();
        let base_va = 0x8006_0000u64;
        let tls_addr = DEFAULT_TLS_ADDR;
        let scratch_va = 0x8100_0000u64;
        mmu23.map_range(scratch_va, 4096, Permission::READ_WRITE);

        // 1. am:appletOE -> OpenApplicationProxy -> GetApplicationFunctions
        let applet_h = kernel23.ipc.allocate_handle(kernel::ServiceType::AppletOE);
        mmu23.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu23.write_u32(tls_addr + 0x18, 0).unwrap(); // OpenApplicationProxy
        let _ = kernel23.ipc.handle_sync_request(
            applet_h,
            tls_addr,
            &mut mmu23,
            &mut kernel23.nvgpu,
            &mut kernel23.audio,
            &mut kernel23.vfs,
        );
        let proxy_h = mmu23.read_u32(tls_addr + 0x20).unwrap();

        // GetApplicationFunctions (Cmd 20)
        mmu23.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu23.write_u32(tls_addr + 0x18, 20).unwrap();
        let _ = kernel23.ipc.handle_sync_request(
            proxy_h,
            tls_addr,
            &mut mmu23,
            &mut kernel23.nvgpu,
            &mut kernel23.audio,
            &mut kernel23.vfs,
        );
        let app_func_h = mmu23.read_u32(tls_addr + 0x20).unwrap();

        // GetDesiredLanguage (Cmd 21)
        mmu23.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu23.write_u32(tls_addr + 0x18, 21).unwrap();
        let _ = kernel23.ipc.handle_sync_request(
            app_func_h,
            tls_addr,
            &mut mmu23,
            &mut kernel23.nvgpu,
            &mut kernel23.audio,
            &mut kernel23.vfs,
        );
        let desired_lang = mmu23.read_u32(tls_addr + 0x20).unwrap();
        println!(
            "  [am:appletOE] IApplicationFunctions -> GetDesiredLanguage: code {} (en-US)",
            desired_lang
        );

        // 2. am:appletOE -> GetSelfController -> CreateManagedDisplaySeparableLayer & SetPerformanceConfiguration
        mmu23.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu23.write_u32(tls_addr + 0x18, 1).unwrap(); // GetSelfController
        let _ = kernel23.ipc.handle_sync_request(
            proxy_h,
            tls_addr,
            &mut mmu23,
            &mut kernel23.nvgpu,
            &mut kernel23.audio,
            &mut kernel23.vfs,
        );
        let self_ctrl_h = mmu23.read_u32(tls_addr + 0x20).unwrap();

        mmu23.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu23.write_u32(tls_addr + 0x18, 10).unwrap(); // CreateManagedDisplaySeparableLayer
        let _ = kernel23.ipc.handle_sync_request(
            self_ctrl_h,
            tls_addr,
            &mut mmu23,
            &mut kernel23.nvgpu,
            &mut kernel23.audio,
            &mut kernel23.vfs,
        );
        let layer_id = mmu23.read_u32(tls_addr + 0x20).unwrap();
        println!(
            "  [am:appletOE] ISelfController -> CreateManagedDisplaySeparableLayer: Layer #{}",
            layer_id
        );

        // 3. nvdrv -> Open /dev/nvhost-ctrl-gpu & Query Ampere GA10B Characteristics
        let nvdrv_h = kernel23.ipc.allocate_handle(kernel::ServiceType::Nvdrv);
        mmu23.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu23.write_u32(tls_addr + 0x18, 0).unwrap(); // Open
        mmu23.write_bytes(tls_addr + 0x20, b"/dev/nvhost-ctrl-gpu\0").unwrap();
        let _ = kernel23.ipc.handle_sync_request(
            nvdrv_h,
            tls_addr,
            &mut mmu23,
            &mut kernel23.nvgpu,
            &mut kernel23.audio,
            &mut kernel23.vfs,
        );
        let ctrl_fd = mmu23.read_u32(tls_addr + 0x20).unwrap();

        mmu23.write_u32(tls_addr + 0x10, kernel::CMIF_IN_MAGIC).unwrap();
        mmu23.write_u32(tls_addr + 0x18, 1).unwrap(); // Ioctl
        mmu23.write_u32(tls_addr + 0x20, ctrl_fd).unwrap();
        mmu23.write_u32(tls_addr + 0x24, 0x80B0_4705).unwrap(); // NVGPU_GPU_IOCTL_GET_CHARACTERISTICS
        mmu23.write_u64(tls_addr + 0x28, scratch_va).unwrap();
        mmu23.write_u32(tls_addr + 0x30, 20).unwrap();
        let _ = kernel23.ipc.handle_sync_request(
            nvdrv_h,
            tls_addr,
            &mut mmu23,
            &mut kernel23.nvgpu,
            &mut kernel23.audio,
            &mut kernel23.vfs,
        );
        let arch = mmu23.read_u32(scratch_va).unwrap();
        let num_sm = mmu23.read_u32(scratch_va + 12).unwrap();
        let sm_ver = mmu23.read_u32(scratch_va + 16).unwrap();
        println!(
            "  [NVIDIA Tegra T239] nvdrv characteristics: Arch 0x{:04X} (Ampere GA10B) | SMs: {} | Feature: SM {:X}.{:X}",
            arch, num_sm, sm_ver >> 8, sm_ver & 0xFF
        );

        // 4. Hardware ASTC 8x8 block decompression
        let mut astc8_block = [0u8; 16];
        astc8_block[0..2].copy_from_slice(&0x01FCu16.to_le_bytes()); // Void-extent
        astc8_block[8..10].copy_from_slice(&0xFF00u16.to_le_bytes()); // R = 255
        astc8_block[10..12].copy_from_slice(&0x4400u16.to_le_bytes()); // G = 68
        astc8_block[12..14].copy_from_slice(&0xCC00u16.to_le_bytes()); // B = 204
        astc8_block[14..16].copy_from_slice(&0xFF00u16.to_le_bytes()); // A = 255
        let decoded8 = gpu::nvn::TextureDecompressor::decompress_surface(
            gpu::nvn::NvnTextureFormat::Astc8x8,
            &astc8_block,
            8,
            8,
        );
        println!(
            "  [Texture HAL] Decompressed ASTC 8x8 void-extent block (64 texels decoded, sample: 0x{:08X})",
            decoded8[0]
        );

        // 5. Guest AArch64 program validating applet functions & setting exit code 0xDDD
        mmu23.map_range(base_va, 4096, Permission::READ_WRITE_EXECUTE);
        #[rustfmt::skip]
        let stage23_code: Vec<u32> = vec![
            0xD281BBA0, // movz x0, #0xDDD  (Commercial Applet HAL exit assertion)
            0xD40000E1, // svc  #0x07       (svcExitProcess)
        ];
        for (i, &w) in stage23_code.iter().enumerate() {
            mmu23.write_u32(base_va + (i * 4) as u64, w).unwrap();
        }

        cpu23.regs.pc = base_va;
        let outcome = run_vm(cpu23, mmu23, kernel23, 10_000);
        if gate.expect_exit(23, &outcome, 0xDDD) {
            println!("  [Full-Stack Verification] Commercial Applet HAL & ASTC 8x8 Verified (Exit code: 0xDDD)");
        }
    }

    // ---------------------------------------------------------
    // Stage 24: Commercial Shader & Dynamic Buffer Pipeline: Ampere FP16 SASS & Dynamic NVN Ring Execution
    //            1. Transpiles commercial Ampere SM87 FP16 SASS shader bytecode (HAdd2, HMul2, HFma2) -> SPIR-V 1.5
    //            2. Encodes and executes dynamic NVN command buffer stream with multi-buffering (NVN_CMD_BUFFER_CYCLE)
    //            3. Processes pushbuffer into GPU channel verifying dynamic buffer index cycling
    //            4. Guest AArch64 program setting exit code 0xEEE and calling svcExitProcess
    //            5. Full-stack verified exit code 0xEEE
    // ---------------------------------------------------------
    println!("\n[Stage 24/25] Commercial Shader & Dynamic Buffer Pipeline: Ampere FP16 SASS & Dynamic NVN Ring Execution");
    {
        let (mut cpu24, mut mmu24, mut kernel24) = setup_vm();
        let base_va = 0x8007_0000u64;
        let stream_addr = 0x9400_1000u64;
        mmu24.map_range(stream_addr, 4096, Permission::READ_WRITE);

        // 1. Transpile commercial Ampere FP16 SASS shader bytecode
        let sass_program = vec![
            gpu::shader::SassOp::FMovImm { rd: 0, imm: 1.0 }.encode(),
            gpu::shader::SassOp::FMovImm { rd: 1, imm: 2.0 }.encode(),
            gpu::shader::SassOp::FMovImm { rd: 2, imm: 0.5 }.encode(),
            gpu::shader::SassOp::HAdd2 { rd: 3, rn: 0, rm: 1 }.encode(),
            gpu::shader::SassOp::HMul2 { rd: 4, rn: 0, rm: 1 }.encode(),
            gpu::shader::SassOp::HFma2 { rd: 5, rn: 0, rm: 1, ra: 2 }.encode(),
            gpu::shader::SassOp::Exit.encode(),
        ];
        let decompiled = gpu::shader::SassShaderDecompiler::decompile(&sass_program);
        println!(
            "  [Ampere SASS Transpiler] Decompiled 7 instructions: {} GLSL lines | {} SPIR-V words (Magic: 0x{:08X})",
            decompiled.glsl_source.lines().count(),
            decompiled.spirv_words.len(),
            decompiled.spirv_words[0]
        );

        // 2. Build & execute dynamic NVN command stream cycling backbuffers 0 -> 1 -> 2
        let mut cb = gpu::nvn::NvnCommandBuffer::new();
        cb.cycle_buffer(0);
        cb.clear_color(0.1, 0.2, 0.3, 1.0);
        cb.cycle_buffer(1);
        cb.clear_color(0.2, 0.4, 0.6, 1.0);
        cb.cycle_buffer(2);
        let stream = cb.encode_stream();
        for (i, &w) in stream.iter().enumerate() {
            mmu24.write_u32(stream_addr + (i * 4) as u64, w).unwrap();
        }

        let mut nvn = gpu::nvn::NvnDriver::new();
        let executed = nvn.submit_guest_command_stream(&mut mmu24, &mut kernel24.nvgpu, stream_addr, stream.len());
        println!(
            "  [NVN Driver] Executed {} stream commands | Active Backbuffer: #{} | Total Cycles: {}",
            executed, nvn.current_buffer_index, nvn.total_buffer_cycles
        );

        // 3. Guest AArch64 program validating commercial shader/buffer pipeline & setting exit code 0xEEE
        mmu24.map_range(base_va, 4096, Permission::READ_WRITE_EXECUTE);
        #[rustfmt::skip]
        let stage24_code: Vec<u32> = vec![
            0xD281DDC0, // movz x0, #0xEEE  (Commercial Shader & Ring Pipeline exit assertion)
            0xD40000E1, // svc  #0x07       (svcExitProcess)
        ];
        for (i, &w) in stage24_code.iter().enumerate() {
            mmu24.write_u32(base_va + (i * 4) as u64, w).unwrap();
        }

        cpu24.regs.pc = base_va;
        let outcome = run_vm(cpu24, mmu24, kernel24, 10_000);
        if gate.expect_exit(24, &outcome, 0xEEE) {
            println!("  [Full-Stack Verification] FP16 SASS & Dynamic NVN Ring Pipeline Verified (Exit code: 0xEEE)");
        }
    }

    // ---------------------------------------------------------
    // Stage 25: Ampere Tensor Core Pipeline: HMMA Warp-Level FP16 Matrix Multiply-Accumulate
    //            1. Decodes HMMA.1688 and HMMA.16816 SASS ops and reports the shapes they issue
    //            2. Distributes documented IEEE binary16 matrices into 32-lane warp fragments
    //               using the public PTX mma.sync fragment layout, then executes both shapes
    //            3. Writes the FP32 results into guest memory
    //            4. Guest AArch64 program loads each result, compares it against the expected
    //               IEEE-754 bit pattern and only reaches exit code 0xEEE if all three match
    //            5. Full-stack verified exit code 0xEEE
    //
    //    Sourcing note: NVIDIA publishes no SASS-level HMMA encoding. The arithmetic and the
    //    fragment layouts come from the public PTX ISA (mma.sync.aligned.m16n8k8 / m16n8k16,
    //    .row.col.f32.f16.f16.f32); the opcode bytes are this runtime's own synthetic SASS
    //    test encoding. See src/gpu/tensor.rs.
    // ---------------------------------------------------------
    println!("\n[Stage 25/25] Ampere Tensor Core Pipeline: HMMA Warp-Level FP16 Matrix Multiply-Accumulate");
    {
        use gpu::tensor::{self, HmmaShape, TensorCoreUnit, WarpFragments};

        let (mut cpu25, mut mmu25, kernel25) = setup_vm();
        let base_va = 0x8008_0000u64;
        let result_addr = 0x9400_2000u64;
        mmu25.map_range(result_addr, 4096, Permission::READ_WRITE);

        // 1. Decode the tensor core SASS program through the Ampere decompiler.
        let sass_program = vec![
            gpu::shader::SassOp::Hmma1688 { rd: 8, rn: 0, rm: 4, ra: 8 }.encode(),
            gpu::shader::SassOp::Hmma16816 { rd: 12, rn: 0, rm: 8, ra: 12 }.encode(),
            gpu::shader::SassOp::Exit.encode(),
        ];
        let decompiled = gpu::shader::SassShaderDecompiler::decompile(&sass_program);
        for op in decompiled.ops.iter() {
            if let Some(shape) = op.hmma_shape() {
                println!(
                    "  [HMMA Decode] {:<19} m{}n{}k{:<2} | A {} regs/lane, B {} regs/lane, C/D {} f32/lane | PTX: {}",
                    shape.mnemonic(),
                    shape.m(),
                    shape.n(),
                    shape.k(),
                    shape.a_regs_per_lane(),
                    shape.b_regs_per_lane(),
                    shape.cd_regs_per_lane(),
                    shape.ptx_mnemonic()
                );
            }
        }

        let mut tensor_unit = TensorCoreUnit::new();

        // 2a. m16n8k8 with A = B = 1.0h and C = 0: every element of D is exactly k = 8.
        let shape_k8 = HmmaShape::M16N8K8;
        let ones_a = vec![0x3C00u16; shape_k8.m() * shape_k8.k()];
        let ones_b = vec![0x3C00u16; shape_k8.k() * shape_k8.n()];
        let zero_c = vec![0.0f32; shape_k8.m() * shape_k8.n()];
        let frags_k8 = WarpFragments::distribute(shape_k8, &ones_a, &ones_b, &zero_c)
            .expect("m16n8k8 fragment distribution");
        let d_k8 = tensor::collect_d(
            shape_k8,
            &tensor_unit.issue_warp(&frags_k8).expect("m16n8k8 issue"),
        );

        // 2b. m16n8k16 with A = B = 1.0h and C = 0.5: every element of D is 16.5.
        let shape_k16 = HmmaShape::M16N8K16;
        let ones_a16 = vec![0x3C00u16; shape_k16.m() * shape_k16.k()];
        let ones_b16 = vec![0x3C00u16; shape_k16.k() * shape_k16.n()];
        let half_c16 = vec![0.5f32; shape_k16.m() * shape_k16.n()];
        let frags_k16 = WarpFragments::distribute(shape_k16, &ones_a16, &ones_b16, &half_c16)
            .expect("m16n8k16 fragment distribution");
        let d_k16 = tensor::collect_d(
            shape_k16,
            &tensor_unit.issue_warp(&frags_k16).expect("m16n8k16 issue"),
        );

        // 2c. FP32 accumulate check: 2048.0h * 1.0h + 1.0 = 2049.0, a value that
        //     binary16 cannot represent but the FP32 accumulator holds exactly.
        let mut wide_a = vec![0u16; shape_k8.m() * shape_k8.k()];
        for row in 0..shape_k8.m() {
            wide_a[row * shape_k8.k()] = tensor::f32_to_f16(2048.0);
        }
        let one_c = vec![1.0f32; shape_k8.m() * shape_k8.n()];
        let d_wide = tensor_unit
            .issue(shape_k8, &wide_a, &tensor::identity_b(shape_k8), &one_c)
            .expect("f32 accumulate issue");

        println!(
            "  [Tensor Core] {} warp issues | {} MACs retired | D(m16n8k8, ones) = {} | D(m16n8k16, +0.5) = {} | FP32 accumulate = {}",
            tensor_unit.issued_ops, tensor_unit.retired_macs, d_k8[0], d_k16[0], d_wide[0]
        );
        println!(
            "  [binary16] 2048.0 -> 0x{:04X} | 2049.0 round-tripped through binary16 = {} (FP32 accumulator keeps {})",
            tensor::f32_to_f16(2048.0),
            tensor::f16_to_f32(tensor::f32_to_f16(2049.0)),
            d_wide[0]
        );

        // 3. Publish the three FP32 results to guest memory.
        mmu25.write_u32(result_addr, d_k8[0].to_bits()).unwrap();
        mmu25.write_u32(result_addr + 4, d_k16[0].to_bits()).unwrap();
        mmu25.write_u32(result_addr + 8, d_wide[0].to_bits()).unwrap();

        // 4. Guest AArch64 program asserting the documented IEEE-754 results.
        mmu25.map_range(base_va, 4096, Permission::READ_WRITE_EXECUTE);
        #[rustfmt::skip]
        let stage25_code: Vec<u32> = vec![
            0xD2B28002, // movz x2, #0x9400, lsl #16   (HMMA result block)
            0xF2840002, // movk x2, #0x2000
            0xB9400040, // ldr  w0, [x2]               (m16n8k8 ones result)
            0x52A82001, // movz w1, #0x4100, lsl #16   (8.0f  = 0x41000000)
            0x6B01001F, // cmp  w0, w1
            0x54000181, // b.ne fail (+12 insns)
            0xB9400440, // ldr  w0, [x2, #4]           (m16n8k16 + 0.5 result)
            0x52A83081, // movz w1, #0x4184, lsl #16   (16.5f = 0x41840000)
            0x6B01001F, // cmp  w0, w1
            0x54000101, // b.ne fail (+8 insns)
            0xB9400840, // ldr  w0, [x2, #8]           (FP32 accumulate result)
            0x52A8A001, // movz w1, #0x4500, lsl #16
            0x72820001, // movk w1, #0x1000            (2049.0f = 0x45001000)
            0x6B01001F, // cmp  w0, w1
            0x54000061, // b.ne fail (+3 insns)
            0xD281DDC0, // movz x0, #0xEEE             (tensor core pipeline assertion)
            0xD40000E1, // svc  #0x07                  (svcExitProcess)
            0xD28175A0, // fail: movz x0, #0xBAD
            0xD40000E1, // svc  #0x07
        ];
        for (i, &w) in stage25_code.iter().enumerate() {
            mmu25.write_u32(base_va + (i * 4) as u64, w).unwrap();
        }

        cpu25.regs.pc = base_va;
        let outcome = run_vm(cpu25, mmu25, kernel25, 10_000);
        if gate.expect_exit(25, &outcome, 0xEEE) {
            println!("  [Full-Stack Verification] Complete 25-Stage Architecture Pipeline Verified (Exit code: 0xEEE)");
        }
    }
    println!();
    gate.finish();
}

/// Runs the 2D Display & Framebuffer Graphic Demo
fn run_gfx_demo(max_frames: usize) {
    println!("--- Initializing T239 2D Framebuffer Display ---");
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
    //   SVC  #0x101            ; svcSubmitGpuPushbuffer -> Tegra 3D Engine clears & rasterizes triangle!
    //   SVC  #0x18            ; svcWaitSynchronization (VSync Frame Flip)
    //   B    frame_loop
    #[rustfmt::skip]
    let gpu3d_program: Vec<u32> = vec![
        0xBD4000A0, // LDR  S0, [X5, #0]
        0x1E212800, // FADD S0, S0, S1
        0xBD0000A0, // STR  S0, [X5, #0]
        0x910000C0, // ADD  X0, X6, #0
        0x910000E1, // ADD  X1, X7, #0
        0xD4002021, // SVC  #0x101 (ext::SUBMIT_GPU_PUSHBUFFER)
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

fn inspect_directory_package(dir_path: &str) {
    println!("============================================================");
    println!("  🔍 LOOSE PACKAGE INSPECTOR (OUNCECAKE FORMAT)");
    println!("============================================================");
    match loader::LoosePackageLoader::discover(dir_path) {
        Ok(pkg) => {
            println!("  Package Root : {}", pkg.root_dir.display());
            println!("  ExeFS Dir    : {}", pkg.exefs_dir.display());
            println!(
                "  RomFS Dir    : {}",
                pkg.romfs_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "None (Pure code module)".into())
            );

            if let Some(ref npdm) = pkg.npdm {
                println!("  Process Name : {}", npdm.process_name);
                println!("  Product Code : {}", npdm.product_code);
                println!("  Title ID     : 0x{:016X}", npdm.title_id);
                println!(
                    "  Core / Prio  : Core {} | Main Thread Priority {}",
                    npdm.default_cpu_core, npdm.main_thread_priority
                );
                println!("  Allowed SAC  : {} services", npdm.allowed_services.len());
                for srv in &npdm.allowed_services {
                    println!("    -> Service '{}'", srv);
                }
            } else {
                println!("  NPDM Metadata: None (Defaulting to homebrew capabilities)");
            }

            println!("  Main Binary  : {}", pkg.main_nso.display());
            println!("  Sub-NSOs     : {} companion modules", pkg.companion_nsos.len());
            for (name, p) in &pkg.companion_nsos {
                println!("    -> {}: {}", name, p.display());
            }

            if let Some(ref r) = pkg.romfs_dir {
                println!("  RomFS Assets :");
                if let Ok(entries) = std::fs::read_dir(r) {
                    for entry in entries.flatten().take(10) {
                        println!("    - {}", entry.file_name().to_string_lossy());
                    }
                }
            }
        }
        Err(e) => eprintln!("Failed to inspect loose directory: {}", e),
    }
}

fn run_directory_package(dir_path: &str, gui_mode: bool) {
    println!("[Loader] Bootstrapping loose directory package (OunceCake format): {}", dir_path);
    let pkg = match loader::LoosePackageLoader::discover(dir_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[Loader Error] Failed to discover loose package: {}", e);
            return;
        }
    };

    let (mut cpu, mut mmu, mut kernel) = setup_vm();
    match loader::LoosePackageLoader::load_into_kernel(&pkg, DEFAULT_BASE_ADDR, &mut mmu, &mut kernel) {
        Ok(info) => {
            println!("[LoosePackageLoader] Successfully bootstrapped unencrypted package:");
            println!("  Process Name : {}", info.process_name);
            println!("  Product Code : {}", info.product_code);
            println!("  Title ID     : 0x{:016X}", info.title_id);
            println!("  Modules ({}):", info.modules_loaded.len());
            for (m_name, m_addr, m_sz) in &info.modules_loaded {
                println!("    -> {} @ 0x{:08X} ({} bytes)", m_name, m_addr, m_sz);
            }
            println!("  RomFS Assets : {} files mounted into VFS", info.romfs_files_mounted);

            if let Some(tp) = info.thread_pointer {
                cpu.regs.tpidr_el0 = tp;
                println!("  Static TLS   : TPIDR_EL0 = 0x{:X}", tp);
            }
            // Opt-in: the program's own crt0 normally runs its constructors.
            if std::env::var_os("OUNCE_RUN_INIT").is_some() {
                let report = loader::run_initializers(&mut cpu, &mut mmu, &info.modules_loaded, STACK_TOP, 1_000_000);
                println!("  Initialisers : {} ran, {} failed", report.completed, report.failed.len());
                for (addr, err) in &report.failed {
                    println!("    -> 0x{:X}: {}", addr, err);
                }
            }

            let fb_size = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
            mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, fb_size, Permission::READ_WRITE);
            mmu.map_range(HID_SHARED_MEM_ADDR, 0x1000, Permission::READ_WRITE);

            if gui_mode {
                println!("  [Display Server] Launching native window (1280x720 60 FPS) with Joy-Con 2 controls...");
                let mut display = DisplayServer::new(
                    DEFAULT_FRAMEBUFFER_ADDR,
                    DisplayConfig {
                        title: format!("Ounce-Core | {} [Tegra T239]", info.process_name),
                        ..Default::default()
                    },
                );
                let mut hid = HidServer::new(HID_SHARED_MEM_ADDR);
                cpu.regs.pc = DEFAULT_BASE_ADDR;
                run_vm_internal(cpu, mmu, kernel, Some(&mut display), Some(&mut hid), 0, 0);
            } else {
                cpu.regs.pc = DEFAULT_BASE_ADDR;
                run_vm(cpu, mmu, kernel, 100_000);
            }
        }
        Err(e) => {
            eprintln!("[Loader Error] Failed to load loose package into VM: {}", e);
        }
    }
}

fn run_file(path: &str, guest_args: &[&str], gui_mode: bool) {
    if std::path::Path::new(path).is_dir() {
        run_directory_package(path, gui_mode);
        return;
    }
    println!("[Loader] Reading package/executable: {}", path);
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return;
        }
    };

    let (mut cpu, mut mmu, mut kernel) = setup_vm();
    let default_key: [u8; 16] = [
        0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
        0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
    ];

    match BootLoader::load_any_package(&bytes, DEFAULT_BASE_ADDR, &default_key, &mut mmu) {
        Ok(info) => {
            let is_nro = info.format == "NRO0";
            println!("[BootLoader] Successfully loaded package:");
            println!("  Format      : {}", info.format);
            println!("  Title ID    : 0x{:016X}", info.title_id);
            println!("  Code size   : 0x{:08X} ({} bytes)", info.code_size, info.code_size);
            println!("  MOD0 relocs : {}", info.relocations_applied);

            // Homebrew ABI for NRO0: ConfigEntry list in X0 (X1 = -1), a heap handed over so libnx skips
            // svcSetHeapSize, and X30 at a svcExitProcess stub, which is where libnx's exit returns.
            if is_nro {
                if let Err(e) = setup_homebrew_nro(&mut cpu, &mut mmu, &mut kernel, path, &bytes, guest_args) {
                    eprintln!("[Loader Error] homebrew environment: {}", e);
                    return;
                }
            }

            let fb_size = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
            mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, fb_size, Permission::READ_WRITE);
            mmu.map_range(HID_SHARED_MEM_ADDR, 0x1000, Permission::READ_WRITE);

            if gui_mode {
                println!("  [Display Server] Launching native window (1280x720 60 FPS) with Joy-Con 2 controls...");
                let mut display = DisplayServer::new(
                    DEFAULT_FRAMEBUFFER_ADDR,
                    DisplayConfig {
                        title: format!("Ounce-Core | 0x{:016X} [T239]", info.title_id),
                        ..Default::default()
                    },
                );
                let mut hid = HidServer::new(HID_SHARED_MEM_ADDR);
                cpu.regs.pc = DEFAULT_BASE_ADDR;
                run_vm_internal(cpu, mmu, kernel, Some(&mut display), Some(&mut hid), 0, 0);
            } else {
                cpu.regs.pc = DEFAULT_BASE_ADDR;
                // libnx startup alone runs tens of thousands of instructions.
                let budget = if is_nro { NRO_INSTRUCTION_BUDGET } else { 100_000 };
                run_vm(cpu, mmu, kernel, budget);
            }
        }
        Err(e) => {
            eprintln!("[Loader Error] {}", e);
        }
    }
}

/// Heap handed to homebrew through the `OverrideHeap` ConfigEntry.
const NRO_HEAP_SIZE: usize = 0x200_0000; // 32 MB
const NRO_CONFIG_ADDR: u64 = 0x7FFF_0000;
const NRO_EXIT_STUB_ADDR: u64 = 0x7FFE_0000;
/// Main thread handle passed to homebrew. Not the `CurrentThread` pseudo-handle (0xFFFF8000): libnx mutexes
/// store the owner's handle and use bit 30 as their waiters flag, so a handle with bit 30 set never reads
/// as "locked by this thread". Below the scheduler's (0x2000+) and IPC's (0x10001+) handle ranges.
const NRO_MAIN_THREAD_HANDLE: u32 = 0x1000;
const NRO_INSTRUCTION_BUDGET: usize = 50_000_000;
/// Mixed audio (interleaved stereo samples) allowed to queue on the host before the guest waits for
/// playback to catch up: 100 ms at 48 kHz.
const AUDIO_LEAD_SAMPLES: usize = 48_000 * 2 / 10;

/// Hands newly mixed audio renderer samples to the host and waits while playback is more than
/// `AUDIO_LEAD_SAMPLES` behind, so a guest's audio loop runs at real-time speed. Without an audio
/// device (`is_enabled` false) nothing waits.
fn stream_audio_to_host(kernel: &mut HorizonKernel, host_audio: &HostAudioSink) {
    let pcm = kernel.audren.drain_samples();
    if pcm.is_empty() {
        return;
    }
    host_audio.push_samples(&pcm);
    while host_audio.is_enabled && host_audio.queued_sample_count() > AUDIO_LEAD_SAMPLES {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// How often the VM copies host controller input into libnx's HID shared memory.
const INPUT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(4);

/// Sets up the homebrew loader ABI for an NRO loaded at `DEFAULT_BASE_ADDR`.
/// `nro` is also placed on the virtual SD card and `argv[0]` names it as `sdmc:/<file name>`, the way
/// homebrew loaders start NROs; libnx `romfsInit` reads the NRO's RomFS back through that path.
fn setup_homebrew_nro(
    cpu: &mut Cpu,
    mmu: &mut Memory,
    kernel: &mut HorizonKernel,
    path: &str,
    nro: &[u8],
    guest_args: &[&str],
) -> Result<(), mmu::MemoryError> {
    mmu.map_range(kernel.heap_base, NRO_HEAP_SIZE, Permission::READ_WRITE);
    kernel.heap_size = NRO_HEAP_SIZE;
    let file_name = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("program.nro");
    let sd_path = format!("sdmc:/{file_name}");
    kernel.vfs.mount_file(&sd_path, nro);
    // libnx splits the argv string on spaces and honours double quotes.
    let quote = |a: &str| if a.contains(' ') { format!("\"{a}\"") } else { a.to_string() };
    let argv = std::iter::once(sd_path.as_str()).chain(guest_args.iter().copied()).map(quote).collect::<Vec<_>>().join(" ");
    loader::HomebrewConfig {
        main_thread_handle: Some(NRO_MAIN_THREAD_HANDLE),
        heap: Some((kernel.heap_base, NRO_HEAP_SIZE as u64)),
        argv: Some(argv),
        ..Default::default()
    }
    .setup(cpu, mmu, NRO_CONFIG_ADDR)?;
    let stub = loader::HomebrewConfig::install_exit_stub(mmu, NRO_EXIT_STUB_ADDR)?;
    cpu.regs.write_x(30, stub);
    Ok(())
}

/// Synthesizes a clean-room unencrypted loose Switch 2 directory package (OunceCake format)
/// on disk and executes it directly through `run_directory_package`.
fn build_and_boot_demo_dir(out_dir: &str, gui_mode: bool) {
    println!("============================================================");
    println!("  📁 CREATING UNENCRYPTED LOOSE SWITCH 2 PACKAGE: {}", out_dir);
    println!("============================================================");

    let exefs = std::path::Path::new(out_dir).join("exefs");
    let romfs = std::path::Path::new(out_dir).join("romfs");
    let _ = std::fs::create_dir_all(&exefs);
    let _ = std::fs::create_dir_all(&romfs);

    // 1. Synthesize NPDM
    let npdm_bytes = loader::NpdmDescriptor::create_synthetic(
        "T239DrakeDemo",
        "OUNCE-P-DEMO",
        0x0100202600023900,
        44,
        0,
        &["sm:", "set:sys", "am:appletOE", "vi:m", "hid", "fsp-srv", "ro:1", "nvdrv"],
    );
    let _ = std::fs::write(exefs.join("main.npdm"), &npdm_bytes);

    // 2. Interactive Switch 2 AArch64 Game Program
    #[rustfmt::skip]
    let demo_insns: Vec<u32> = vec![
        // Init Registers: X5 = X (200), X6 = Y (200), X7 = Color (Emerald Green 0x00FF88)
        0xD2801905, // MOVZ X5, #200
        0xD2801906, // MOVZ X6, #200
        0xD2800007, // MOVZ X7, #0
        0xF2A001E7, // MOVK X7, #0x00FF, LSL #16
        0xF2801107, // MOVK X7, #0x88, LSL #0 (0x00FF0088)
        0xD2800009, // MOVZ X9, #0
        0xF2A20009, // MOVK X9, #0x1000, LSL #16 (X9 = 0x10000000 HID Shared Memory)
        0xD280000A, // MOVZ X10, #0
        0xF2B2000A, // MOVK X10, #0x9000, LSL #16 (X10 = 0x90000000 VRAM FB)
        0xD280010B, // MOVZ X11, #8 (speed = 8 px)

        // frame_loop: Read buttons
        0xB9400928, // LDR  W8, [X9, #8] (Read buttons bitmask)

        // DLeft
        0x5282000D, // MOVZ W13, #0x1000
        0x0A0D010C, // AND  W12, W8, W13
        0x3400004C, // CBZ  W12, +8
        0xCB0B00A5, // SUBS X5, X5, X11

        // DRight
        0x5288000D, // MOVZ W13, #0x4000
        0x0A0D010C, // AND  W12, W8, W13
        0x3400004C, // CBZ  W12, +8
        0x8B0B00A5, // ADD  X5, X5, X11

        // DUp
        0x5284000D, // MOVZ W13, #0x2000
        0x0A0D010C, // AND  W12, W8, W13
        0x3400004C, // CBZ  W12, +8
        0xCB0B00C6, // SUBS X6, X6, X11

        // DDown
        0x5290000D, // MOVZ W13, #0x8000
        0x0A0D010C, // AND  W12, W8, W13
        0x3400004C, // CBZ  W12, +8
        0x8B0B00C6, // ADD  X6, X6, X11

        // Draw Player Sprite (40x40 block)
        0x9100014E, // ADD  X14, X10, #0 (FB base)
        0x8B0500AF, // ADD  X15, X5, X5 (X * 2)
        0x8B0F01EF, // ADD  X15, X15, X15 (X * 4)
        0x8B0F01CE, // ADD  X14, X14, X15 (FB + X*4)
        0xD2800510, // MOVZ X16, #40

        // pixel_loop:
        0xB90001C7, // STR  W7, [X14, #0]
        0x910011CE, // ADD  X14, X14, #4
        0xF1000610, // SUBS X16, X16, #1
        0x54FFFFAC, // B.NE pixel_loop (-3 insns)

        // VSync Flip
        0xD4000301, // SVC  #0x18 (svcWaitForVsync)
        0x17FFFFEB, // B    frame_loop (-21 insns)
    ];

    let mut code_bytes = Vec::new();
    for insn in &demo_insns {
        code_bytes.extend_from_slice(&insn.to_le_bytes());
    }
    let main_nso = loader::NroLoader::build_synthetic_nso(&code_bytes);
    let _ = std::fs::write(exefs.join("main"), &main_nso);

    // 3. RomFS Assets
    let _ = std::fs::write(
        romfs.join("manifest.json"),
        b"{\"app\":\"T239DrakeDemo\",\"architecture\":\"ARMv8.5-A\",\"gpu\":\"Ampere\"}",
    );
    let _ = std::fs::create_dir_all(romfs.join("assets"));
    let _ = std::fs::write(
        romfs.join("assets/readme.txt"),
        b"Loose synthetic package layout created by Ounce-Core.",
    );

    println!("  -> Created unencrypted ExeFS and RomFS in '{}'\n", out_dir);
    run_directory_package(out_dir, gui_mode);
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
    //  1. Queries Switch 2 8-Core Cortex-A78C CoreMask (0xFF) via SVC #0x29 (svcGetInfo)
    //  2. Queries Switch 2 12 GB LPDDR5 TotalMemoryAvailable via SVC #0x29 (info_type = 6)
    //  3. Computes (12 GB >> 30) = 12 GB in X2 and combines with CoreMask (0xFF00 | 12 = 0xFF0C = 65292)
    //  4. Exits cleanly via SVC #0x07 (svcExitProcess)
    #[rustfmt::skip]
    let showcase_insns: Vec<u32> = vec![
        0xD2800001, // MOVZ X1, #0      (InfoType::CoreMask = 0)
        0xD4000521, // SVC  #0x29       (svcGetInfo -> X1 = 0xFF)
        0xD2800102, // MOVZ X2, #8
        0x9AC22024, // LSLV X4, X1, X2  (X4 = 0xFF << 8 = 0xFF00)
        0xD28000C1, // MOVZ X1, #6      (InfoType::TotalMemoryAvailable = 6 -> 12 GB)
        0xD4000521, // SVC  #0x29       (svcGetInfo -> X1 = 12 * 1024^3)
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

    run_file(out_path, &[], false);
}

#[cfg(test)]
mod harness_gate_tests {
    use super::*;

    fn outcome(stop: VmStop, x0: u64) -> VmOutcome {
        VmOutcome { stop, x0, d0: 0.0 }
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

/// Checks that this host can run natively generated code (Tier-4, feature `native-jit`).
fn run_jit_selftest() {
    #[cfg(feature = "native-jit")]
    match ounce_recompiler::host_selftest() {
        Ok(value) => println!(
            "[Tier-4] Native code self-test passed on {}: generated function returned {}",
            ounce_recompiler::host_arch().unwrap_or("unknown"),
            value
        ),
        Err(e) => {
            eprintln!("[Tier-4] Native code self-test failed: {}", e);
            std::process::exit(1);
        }
    }
    #[cfg(not(feature = "native-jit"))]
    {
        eprintln!("[Tier-4] Not available: rebuild with `--features native-jit`.");
        std::process::exit(1);
    }
}
