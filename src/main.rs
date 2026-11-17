// The runtime core contains no `unsafe` code; the compiler enforces it here.
// Any future host-level exception (a native code emitter, a raw graphics driver) lives
// in its own workspace crate, per AGENTS.md "Unsafe Boundary".
#![forbid(unsafe_code)]
// Explicit zero terms (`t * 3 + 0`, `0 << 5`, `ALL_ONES & MASK`) spell out register, lane and
// bit-field layouts next to their non-zero neighbours; they are documentation, not mistakes.
#![allow(clippy::identity_op)]

mod audio;
mod cli;
mod census;
mod cli_flow;
mod cpu;
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
mod ui;

use cli::*;
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

fn main() {
    if let Ok(path) = std::env::var("OUNCE_REPORT_JSON") && !path.is_empty() {
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
        if arg == "--max-frames" || arg == "--press" || arg.starts_with("--max-frames=") || arg.starts_with("--press=") {
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
            match cpu::CpuTier::parse(value) {
                Some(tier) => cpu::CpuTier::set_cli_override(tier),
                None => eprintln!("[CPU] Unknown --cpu value '{value}' (expected interpreter, tier3, tier4 or native); ignoring."),
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
                let what = if nro_only { "the path of a homebrew .nro file" } else { "the path of a .nro, .nso or loose folder" };
                eprint!("{}", cli_flow::missing_path(&args[1], what));
                std::process::exit(2);
            }
            let file_path = &args[2];
            if !std::path::Path::new(file_path).exists() {
                eprint!("{}", cli_flow::file_not_found(file_path));
                std::process::exit(1);
            }
            let _ = RUN_TARGET.set(file_path.clone());
            print_banner();
            let gui_mode = args.iter().any(|a| a == "--gui" || a == "--window" || a == "-g");
            // Anything after the path that is not a flag is passed to homebrew as argv[1..].
            let guest_args: Vec<&str> = args[3..].iter().filter(|a| !a.starts_with('-')).map(|a| a.as_str()).collect();
            run_file(file_path, &guest_args, gui_mode, nro_only);
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
                eprint!("{}", cli_flow::missing_path("inspect", "the path of a .nro, .nso or loose folder"));
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
