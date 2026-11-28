//! Ounce-Core library API (`ounce.run.v1`).
//!
//! The runtime core contains no `unsafe` code; the compiler enforces it here.
//! Any host-level exception (a native code emitter, a raw graphics driver) lives
//! in its own workspace crate, per `AGENTS.md` "Unsafe Boundary".
#![forbid(unsafe_code)]
// Explicit zero terms (`t * 3 + 0`, `0 << 5`, `ALL_ONES & MASK`) spell out register, lane and
// bit-field layouts next to their non-zero neighbours; they are documentation, not mistakes.
#![allow(clippy::identity_op)]

// While `--report-json=-` owns stdout, every `print!`/`println!` in this crate goes to stderr so stdout stays pure
// `ounce.run.v1` JSON. These shadow the std macros for all modules declared below.
macro_rules! print {
    ($($arg:tt)*) => {
        if crate::run_report::is_stdout_report() {
            ::std::eprint!($($arg)*)
        } else {
            ::std::print!($($arg)*)
        }
    };
}
macro_rules! println {
    ($($arg:tt)*) => {
        if crate::run_report::is_stdout_report() {
            ::std::eprintln!($($arg)*)
        } else {
            ::std::println!($($arg)*)
        }
    };
}

mod audio;
mod census;
mod cli;
mod cli_flow;
mod cpu;
mod disasm;
mod diverge;
mod fs;
mod fuzz_replay;
mod gpu;
mod hid;
mod kernel;
mod lab;
mod linux;
mod loader;
mod loop_controls;
mod mmu;
mod run_report;
mod trace;
mod trace_summary;
mod ui;

use audio::HostAudioSink;
use cli::*;
use cpu::{Cpu, StepResult};
use gpu::nvgpu::{
    METHOD_BIND_SHADER_PROGRAM, METHOD_BIND_TEXTURE_2D, METHOD_CLEAR_SURFACE, METHOD_DISPATCH_UPSCALE_2X,
    METHOD_DRAW_ARRAYS, METHOD_DRAW_TEXTURED_ARRAYS, METHOD_SET_CLEAR_COLOR, METHOD_SET_DEPTH_TEST_ENABLE,
    METHOD_SET_MVP_MATRIX, METHOD_SET_RENDER_TARGET_ADDR, METHOD_SET_VERTEX_ARRAY_ADDR, METHOD_SET_VIEWPORT_DIMS,
    NvgpuChannel, SubmissionMode,
};
use gpu::shader::SassOp;
use gpu::{DEFAULT_FRAMEBUFFER_ADDR, DisplayConfig, DisplayServer, SCREEN_HEIGHT, SCREEN_WIDTH};
use hid::{HID_SHARED_MEM_ADDR, HidServer, HostGamepadManager};
use kernel::{CMIF_IN_MAGIC, DEFAULT_TLS_ADDR, HorizonKernel, ServiceType, SvcResult, TLS_SIZE};
use loader::BootLoader;
use mmu::{Memory, Permission};
use std::env;
use std::time::Instant;

const DEFAULT_BASE_ADDR: u64 = 0x80000000;
const STACK_TOP: u64 = 0x7FFFFFE000;
const STACK_SIZE: usize = 0x100000; // 1 MB stack

/// `--sdmc=<dir>`: host folder behind the guest's `sdmc:/` (unset: in-memory SD card only).
static SDMC_HOST_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
static SDMC_WRITEBACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The program path given to `run`/`run-nro`, quoted back in the next-step text when a run stops early.
static RUN_TARGET: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Execution engine selection for [`RunOptions`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Interpreter,
    Tier3,
    Tier4,
    Native,
}

impl From<EngineKind> for cpu::CpuTier {
    fn from(kind: EngineKind) -> Self {
        match kind {
            EngineKind::Interpreter => Self::Interpreter,
            EngineKind::Tier3 => Self::Tier3,
            EngineKind::Tier4 => Self::Tier4,
            EngineKind::Native => Self::Native,
        }
    }
}

/// Configuration for running a static Linux AArch64 ELF or unencrypted `.nro` via the library API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    pub engine: EngineKind,
    pub max_instructions: u64,
    pub deterministic: bool,
    pub headless: bool,
    pub guest_args: Vec<String>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            engine: EngineKind::Tier3,
            max_instructions: 100_000_000,
            deterministic: false,
            headless: true,
            guest_args: Vec::new(),
        }
    }
}

/// Typed classification of an [`ounce.run.v1`](StopReport) stop reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopClass {
    CleanExit,
    WindowClosed,
    FrameLimit,
    UnimplementedInsn,
    MemoryFault,
    CpuError,
    SvcBreak,
    UnhandledSvc,
    CpuHalted,
    BudgetExhausted,
    LoaderError,
}

/// Structured stop report matching `ounce.run.v1`.
pub type StopReport = run_report::RunReport;

impl StopReport {
    /// Returns the typed [`StopClass`] for this report's `stop_kind`.
    pub fn stop_class(&self) -> StopClass {
        match self.stop_kind {
            "clean_exit" => StopClass::CleanExit,
            "window_closed" => StopClass::WindowClosed,
            "frame_limit" => StopClass::FrameLimit,
            "unimplemented_insn" => StopClass::UnimplementedInsn,
            "memory_fault" => StopClass::MemoryFault,
            "svc_break" => StopClass::SvcBreak,
            "unhandled_svc" => StopClass::UnhandledSvc,
            "cpu_halted" => StopClass::CpuHalted,
            "budget_exhausted" => StopClass::BudgetExhausted,
            "loader_error" => StopClass::LoaderError,
            _ => StopClass::CpuError,
        }
    }

    /// Serializes this report into the flat `ounce.run.v1` JSON object.
    pub fn to_json(&self) -> serde_json::Value {
        run_report::to_json(self)
    }
}

/// Runs a static Linux AArch64 ELF image in memory and returns its `ounce.run.v1` [`StopReport`].
pub fn run_elf(bytes: &[u8], options: &RunOptions) -> StopReport {
    struct NullOutput;
    impl linux::syscalls::Output for NullOutput {
        fn write(&mut self, _fd: u64, _data: &[u8]) {}
    }

    let tier: cpu::CpuTier = options.engine.into();
    let mut argv = vec!["ounce-core".to_string()];
    argv.extend(options.guest_args.iter().cloned());
    let linux_opts = linux::runner::Options {
        tier,
        max_instructions: options.max_instructions,
        trace_syscalls: false,
        deterministic: options.deterministic,
        root: None,
    };
    let mut sink = NullOutput;
    let report = linux::runner::run(bytes, &argv, &linux_opts, &mut sink, &mut |_| {});
    report.to_run_report(tier)
}

/// Runs an unencrypted homebrew `.nro` image in memory and returns its `ounce.run.v1` [`StopReport`].
pub fn run_nro(bytes: &[u8], options: &RunOptions) -> StopReport {
    let tier: cpu::CpuTier = options.engine.into();
    cpu::CpuTier::set_cli_override(tier);
    if options.deterministic {
        run_report::set_deterministic(true);
    }
    let (mut cpu, mut mmu, mut kernel) = setup_vm();
    if let Err(err) = BootLoader::load_homebrew_nro(bytes, DEFAULT_BASE_ADDR, &mut mmu) {
        return loader_error_report(tier, format!("{err}"));
    }
    let guest_refs: Vec<&str> = options.guest_args.iter().map(String::as_str).collect();
    if let Err(err) = setup_homebrew_nro(&mut cpu, &mut mmu, &mut kernel, "<memory>.nro", bytes, &guest_refs) {
        return loader_error_report(tier, format!("{err}"));
    }
    let fb_size = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
    mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, fb_size, Permission::READ_WRITE);
    mmu.map_range(HID_SHARED_MEM_ADDR, 0x1000, Permission::READ_WRITE);
    cpu.regs.pc = DEFAULT_BASE_ADDR;
    let budget = usize::try_from(options.max_instructions).unwrap_or(usize::MAX);
    let mut headless_display = options.headless.then(|| {
        DisplayServer::new(
            DEFAULT_FRAMEBUFFER_ADDR,
            DisplayConfig {
                backend: gpu::BackendKind::Headless,
                ..Default::default()
            },
        )
    });
    let outcome = run_vm_internal(cpu, mmu, kernel, headless_display.as_mut(), None, budget, 0);
    outcome.report
}

fn loader_error_report(tier: cpu::CpuTier, message: String) -> StopReport {
    let tier_name = match tier {
        cpu::CpuTier::Interpreter => "interpreter",
        cpu::CpuTier::Tier3 => "tier3",
        cpu::CpuTier::Tier4 => "tier4",
        cpu::CpuTier::Native => "native",
    };
    StopReport {
        tier: tier_name,
        silent_fallback: false,
        stop_kind: "loader_error",
        exit_code: run_report::exit_code_for("loader_error"),
        pc: None,
        lr: None,
        sp: None,
        opcode: None,
        fault_addr: None,
        fault_symbol: None,
        backtrace: None,
        svc_id: None,
        sm_service: None,
        cmif_cmd_id: None,
        break_result: None,
        insns_executed: 0,
        frames_presented: 0,
        framebuffer_hash: None,
        audio_frames_mixed: 0,
        audio_hash: None,
        guest_exit_status: None,
        message: Some(message),
    }
}

/// CLI entrypoint used by `src/main.rs`.
pub fn cli_main() {
    if let Ok(path) = std::env::var("OUNCE_REPORT_JSON")
        && !path.is_empty()
    {
        run_report::set_report_path(std::path::PathBuf::from(path));
    }
    if std::env::var("OUNCE_AUDIO").as_deref() == Ok("null") {
        run_report::set_audio_null(true);
    }
    if matches!(std::env::var("OUNCE_DETERMINISTIC").as_deref(), Ok("1" | "true")) {
        run_report::set_deterministic(true);
    }
    if let Ok(backend) = std::env::var("OUNCE_GPU_BACKEND")
        && (!matches!(backend.as_str(), "cpu" | "host") || (backend == "host" && !cfg!(feature = "gpu-present")))
    {
        eprintln!("[guest-gpu] OUNCE_GPU_BACKEND expects cpu or host; host requires --features gpu-present");
        std::process::exit(2);
    }
    if let Err(e) = cpu::CpuTier::check_env() {
        eprintln!("[CPU] {e}");
        std::process::exit(2);
    }
    // `--present=<auto|gpu|minifb|headless>` and `--cpu=<interpreter|tier3|tier4|native>` may appear anywhere; strip them so the
    // positional arguments of every command stay where they were.
    let mut args: Vec<String> = Vec::new();
    let mut after_separator = false;
    let mut loop_max_frames: Option<usize> = None;
    let mut loop_presses: Vec<loop_controls::Press> = Vec::new();
    let mut raw_args = env::args().peekable();
    while let Some(arg) = raw_args.next() {
        if after_separator || arg == "--" {
            after_separator = true;
            args.push(arg);
            continue;
        }
        if let Some(value) = arg.strip_prefix("--report-json=") {
            run_report::set_report_path(std::path::PathBuf::from(value));
            continue;
        }
        if arg == "--report-json" {
            if let Some(value) = raw_args.next() {
                run_report::set_report_path(std::path::PathBuf::from(value));
            }
            continue;
        }
        if arg == "--max-frames" || arg == "--press" || arg.starts_with("--max-frames=") || arg.starts_with("--press=")
        {
            let (flag, inline) = match arg.split_once('=') {
                Some((f, v)) => (f.to_string(), Some(v.to_string())),
                None => (arg.clone(), None),
            };
            let value = inline.or_else(|| raw_args.next()).unwrap_or_default();
            let parsed = if flag == "--max-frames" {
                loop_controls::parse_max_frames(&value).map(|n| loop_max_frames = Some(n))
            } else {
                loop_controls::parse_press(&value).map(|p| loop_presses.push(p))
            };
            if let Err(e) = parsed {
                eprintln!("[CLI] {e}");
                std::process::exit(2);
            }
            continue;
        }
        if arg == "--audio=null" {
            run_report::set_audio_null(true);
            continue;
        }
        if arg == "--deterministic" {
            run_report::set_deterministic(true);
            args.push(arg);
            continue;
        }
        if arg == "--sdmc-writeback" {
            SDMC_WRITEBACK.store(true, std::sync::atomic::Ordering::Relaxed);
            continue;
        }
        if let Some(value) = arg.strip_prefix("--sdmc=") {
            let _ = SDMC_HOST_DIR.set(std::path::PathBuf::from(value));
            continue;
        }
        if let Some(value) = arg.strip_prefix("--cpu=") {
            match cpu::CpuTier::parse_arg(value) {
                Ok(tier) => cpu::CpuTier::set_cli_override(tier),
                Err(e) => {
                    eprintln!("[CPU] --cpu: {e}");
                    std::process::exit(2);
                }
            }
            continue;
        }
        match arg.strip_prefix("--present=") {
            Some(value) => match gpu::BackendKind::parse(value) {
                Some(kind) => gpu::BackendKind::set_cli_override(kind),
                None => eprintln!(
                    "[Display] Unknown --present value '{value}' (expected auto, gpu, minifb or headless); ignoring."
                ),
            },
            None => args.push(arg),
        }
    }

    loop_controls::configure(loop_max_frames, loop_presses);
    if args.len() < 2 {
        print!("{}", cli_flow::getting_started(env!("CARGO_PKG_VERSION")));
        return;
    }

    match args[1].as_str() {
        "help" | "-h" | "--help" => {
            print_usage();
        }
        "info" | "--info" | "-v" | "--version" | "specs" => {
            print_specs();
        }
        "diverge" => {
            std::process::exit(diverge::run_cli(&args[2..]));
        }
        "trace-diff" => {
            std::process::exit(trace_summary::run_cli(&args[2..]));
        }
        "ui" | "lab" => {
            print_banner();
            lab::run_cli(&args[2..]);
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
        "replay-instruction" => std::process::exit(fuzz_replay::replay_bundle(&args[2..])),
        "run-elf" => std::process::exit(linux::runner::cli(&args[2..])),
        "run" | "run-nro" => {
            let nro_only = args[1] == "run-nro";
            if args.len() < 3 {
                let what = if nro_only {
                    "the path of a homebrew .nro file"
                } else {
                    "the path of a .nro, .nso or loose folder"
                };
                eprint!("{}", cli_flow::missing_path(&args[1], what));
                std::process::exit(2);
            }
            let file_path = &args[2];
            if matches!(file_path.as_str(), "--help" | "-h") {
                print_run_usage(&args[1]);
                return;
            }
            let run_args = match parse_run_args(&args[1], file_path, &args[3..]) {
                Ok(parsed) => parsed,
                Err(message) => {
                    eprintln!("{message}");
                    std::process::exit(2);
                }
            };
            if !std::path::Path::new(file_path).exists() {
                eprint!("{}", cli_flow::file_not_found(file_path));
                std::process::exit(1);
            }
            let _ = RUN_TARGET.set(file_path.clone());
            print_banner();
            run_file(file_path, &run_args.guest, run_args.gui, nro_only);
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
        "fuzz-replay" => {
            std::process::exit(fuzz_replay::run(&args[2..]));
        }
        "census" => {
            std::process::exit(census::run(&args[2..]));
        }
        "inspect" => {
            if args.len() < 3 {
                eprint!(
                    "{}",
                    cli_flow::missing_path("inspect", "the path of a .nro, .nso or loose folder")
                );
                std::process::exit(2);
            }
            print_banner();
            let file_path = &args[2];
            if std::path::Path::new(file_path).is_dir() {
                inspect_directory_package(file_path);
                return;
            }
            match std::fs::read(file_path) {
                Ok(bytes) => match ui::PackageInspector::inspect_bytes(file_path, &bytes) {
                    Ok(report) => ui::PackageInspector::print_report(&report),
                    Err(e) => {
                        eprintln!("Inspector error: {}", e);
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to read file '{}': {}", file_path, e);
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprint!("{}", cli_flow::unknown_command(other));
            std::process::exit(2);
        }
    }
}
