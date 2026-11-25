//! Banner, usage text and the `specs` box.

use crate::*;

pub(crate) fn print_banner() {
    if crate::run_report::is_stdout_report() {
        eprintln!("============================================================");
        eprintln!("  OUNCE-CORE: ARM64 Execution & Verification Runtime");
        eprintln!("  Target: ARMv8.5-A AArch64 | Headless & Windowed Lab");
        eprintln!("============================================================\n");
    } else {
        println!("============================================================");
        println!("  OUNCE-CORE: ARM64 Execution & Verification Runtime");
        println!("  Target: ARMv8.5-A AArch64 | Headless & Windowed Lab");
        println!("============================================================\n");
    }
}

#[allow(dead_code)]
pub(crate) struct Styler {
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

pub(crate) const BOX_WIDTH: usize = 92;

pub(crate) fn visible_width(s: &str) -> usize {
    let mut in_escape = false;
    let mut width = 0;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else if c == '🎮'
            || c == '🚀'
            || c == '🖥'
            || c == '🎨'
            || c == '🧠'
            || c == '🔊'
            || c == '💾'
            || c == '🌐'
            || c == '🛡'
        {
            width += 2;
        } else if c == '\u{fe0f}' {
            // variation selector has 0 width
        } else {
            width += 1;
        }
    }
    width
}

pub(crate) fn print_box_top(s: &Styler, title: &str) {
    let title_len = visible_width(title);
    let prefix_len = 6 + title_len + 3; // "┌── [ " + title + " ] "
    let dashes = if BOX_WIDTH > prefix_len + 1 {
        BOX_WIDTH - 1 - prefix_len
    } else {
        2
    };
    println!(
        "{}┌── [ {}{}{} ] {}{}┐{}",
        s.blue,
        s.bold,
        title,
        s.reset,
        s.blue,
        "─".repeat(dashes),
        s.reset
    );
}

pub(crate) fn print_box_bottom(s: &Styler) {
    println!("{}└{}┘{}", s.blue, "─".repeat(BOX_WIDTH - 2), s.reset);
}

pub(crate) fn print_row(s: &Styler, label: &str, val: &str) {
    let used = visible_width(label) + visible_width(val);
    let max_inner = BOX_WIDTH - 6;
    let pad = max_inner.saturating_sub(used);
    println!(
        "{}│  {}{}{}{}{}{}  │{}",
        s.blue,
        s.bold,
        label,
        s.reset,
        val,
        " ".repeat(pad),
        s.blue,
        s.reset
    );
}

pub(crate) fn print_usage() {
    let s = Styler::new();
    println!(
        "{}Ounce Core v{}{}  command reference",
        s.bold,
        env!("CARGO_PKG_VERSION"),
        s.reset
    );
    println!();
    println!("  {}USAGE:{}", s.bold, s.reset);
    println!("    ounce-core {}<COMMAND>{} [OPTIONS]", s.green, s.reset);
    println!("    From a source checkout: cargo run --release -- <COMMAND> [OPTIONS]\n");
    println!("  {}RUN AND CHECK A PROGRAM (start here):{}", s.bold, s.reset);
    println!(
        "    {}ui{} | {}lab{}               Open the local workbench: drop a .nro, compare engines (and this Mac) [--port N --no-open --dir D]",
        s.green, s.reset, s.green, s.reset
    );
    println!(
        "    {}run-nro{} <file> [args] [-- args]  Run a homebrew .nro; if it stops early, the output says why and what to do next (run-nro --help)",
        s.green, s.reset
    );
    println!(
        "    {}run-elf{} <elf> [-- args]       Run a static AArch64 Linux ELF (--cpu=interpreter|tier3|tier4, --max-insns=N, --trace-syscalls, --deterministic, --dump-regs)",
        s.green, s.reset
    );
    println!(
        "    {}diverge{} <prog.nro|prog.elf> [-- args]  Compare NRO (AArch64 hardware replay) or static ELF across interpreter/Tier-3/Tier-4",
        s.green, s.reset
    );
    println!(
        "    {}trace-diff{} <expected.json> <actual.json>  First step where two ounce.trace.v1 traces differ (record with OUNCE_TRACE_SUMMARY=<file>); [--window N] [--json out.json]",
        s.green, s.reset
    );
    println!(
        "    {}run{} <file|dir> [args] [-- args]  Like run-nro, but also accepts .nso and loose folders (guest args: .nro only)",
        s.green, s.reset
    );
    println!();
    println!("  {}OPTIONS FOR RUNS:{}", s.bold, s.reset);
    println!(
        "    {}--cpu={}<tier>            Engine: tier3 (decoded block cache, default), tier4 (needs --features native-jit) or native (AArch64 hosts, needs --features nce)",
        s.green, s.reset
    );
    println!(
        "    {}--present={}<mode>        Display: auto, gpu (needs --features gpu-present), minifb, headless (no window, no audio device)",
        s.green, s.reset
    );
    println!(
        "    {}--sdmc={}<dir>            Host folder behind the guest's sdmc:/ (sandboxed copy-on-write; default: in-memory SD card)",
        s.green, s.reset
    );
    println!(
        "    {}--sdmc-writeback{}        With --sdmc: write guest changes through to the host folder (default: never modify it)",
        s.green, s.reset
    );
    println!(
        "    {}--max-frames {}<N>        Exit with status 0 after N presented frames (headless runs count guest present calls)",
        s.green, s.reset
    );
    println!(
        "    {}--press {}<BTN@FRAME>     Hold a HID button for {} frames from presented frame FRAME; repeatable (e.g. --press PLUS@60)",
        s.green,
        s.reset,
        loop_controls::PRESS_HOLD_FRAMES
    );
    println!("      valid buttons: {}", loop_controls::button_list());
    println!();
    println!("  {}INSPECT AND REPLAY:{}", s.bold, s.reset);
    println!(
        "    {}inspect{} <file|dir>      Audit .nro/.nso headers, loose NPDM metadata and RomFS assets",
        s.green, s.reset
    );
    println!(
        "    {}replay-instruction{} <bundle>  Check retained instruction bytes and re-execute the recorded case",
        s.green, s.reset
    );
    println!(
        "    {}info{}                    Show hardware architecture and engine status (also --version)",
        s.green, s.reset
    );
    println!("    {}help{}                    Show this reference", s.green, s.reset);
    println!();
    println!("  {}DEMOS AND SELF-TESTS:{}", s.bold, s.reset);
    println!(
        "    {}test{}                    Run the 25-stage verification and benchmark suite",
        s.green, s.reset
    );
    println!(
        "    {}jit-selftest{}            Check that this host can run Tier-4 native code (needs --features native-jit)",
        s.green, s.reset
    );
    println!(
        "    {}play{} [frames]           Interactive demo game with Joy-Con 2 controls",
        s.green, s.reset
    );
    println!(
        "    {}gfx{} [frames]            2D framebuffer display demo (1280x720)",
        s.green, s.reset
    );
    println!(
        "    {}3d{} [frames]             Software 3D pushbuffer rasterizer benchmark",
        s.green, s.reset
    );
    println!(
        "    {}demo-dir{} [out_dir]      Synthesize and launch a loose synthetic directory package",
        s.green, s.reset
    );
    println!();
    println!("  {}EXAMPLES:{}", s.bold, s.reset);
    println!("    {}ounce-core ui{}", s.dim, s.reset);
    println!(
        "    {}ounce-core run-nro tests/fixtures/nro/hello/hello.nro --present=headless{}",
        s.dim, s.reset
    );
    println!("    {}ounce-core run-nro my_program.nro --cpu=tier4{}", s.dim, s.reset);
    println!("    {}ounce-core diverge my_program.nro{}", s.dim, s.reset);
    println!(
        "    {}ounce-core diverge tests/linux-elf/bin/basic_math.elf{}",
        s.dim, s.reset
    );
    println!();
    println!("  Report a problem: {}\n", cli_flow::ISSUE_FORM);
}

pub(crate) fn print_specs() {
    let s = Styler::new();
    let host_arch = std::env::consts::ARCH;
    let host_os = std::env::consts::OS;
    let ptr_width = std::mem::size_of::<usize>() * 8;
    let profile = if cfg!(debug_assertions) {
        "Debug"
    } else {
        "Release [Optimized]"
    };

    let header_text = format!(
        "  {}OUNCE-CORE v{}{} — ARM64 EXECUTION & VERIFICATION RUNTIME",
        s.bold,
        env!("CARGO_PKG_VERSION"),
        s.reset
    );
    let header_width = visible_width(&header_text);
    let pad = (BOX_WIDTH - 2).saturating_sub(header_width);

    println!("{}╭{}╮{}", s.cyan, "─".repeat(BOX_WIDTH - 2), s.reset);
    println!("{}│{}{}{}│{}", s.cyan, header_text, " ".repeat(pad), s.cyan, s.reset);
    println!("{}╰{}╯{}", s.cyan, "─".repeat(BOX_WIDTH - 2), s.reset);
    println!();

    let feat_state = |enabled: bool| if enabled { "enabled" } else { "disabled" };
    println!(
        "  {}Target Architecture{}: {}ARMv8.5-A AArch64 (PAC + BTI){}",
        s.bold, s.reset, s.green, s.reset
    );
    println!(
        "  {}Execution Model{}   : {}ARM64 Multi-Engine Execution & Differential Verification{}",
        s.bold, s.reset, s.green, s.reset
    );
    println!(
        "  {}Host Environment{}  : {}-{}-{}bit ({})",
        s.bold, s.reset, host_arch, host_os, ptr_width, profile
    );
    println!(
        "  {}Optional Engines{}  : native-jit (Tier-4): {}, nce (native): {}",
        s.bold,
        s.reset,
        feat_state(cfg!(feature = "native-jit")),
        feat_state(cfg!(feature = "nce")),
    );
    println!(
        "  {}Optional Features{} : host-io: {}",
        s.bold,
        s.reset,
        feat_state(cfg!(feature = "host-io")),
    );
    println!(
        "  {}Project Notice{}    : {}Not affiliated with Nintendo or NVIDIA; no proprietary material distributed{}",
        s.bold, s.reset, s.green, s.reset
    );
    println!();

    print_box_top(&s, "CPU Subsystem: ARMv8.5-A AArch64 (PAC + BTI)");
    print_row(
        &s,
        "Architecture        : ",
        "Single-threaded AArch64 execution for bounded program testing",
    );
    print_row(
        &s,
        "Security Features   : ",
        "ARMv8.3-A Pointer Authentication (PAC) + ARMv8.5-A BTI",
    );
    print_row(
        &s,
        "Vector Engine       : ",
        "128-bit ARM NEON Advanced SIMD & IEEE-754 FP Single/Double",
    );
    let engines_info = format!(
        "Tier-1/2/3 interpreter & super-blocks; Tier-4 JIT ({}); NCE ({})",
        feat_state(cfg!(feature = "native-jit")),
        feat_state(cfg!(feature = "nce")),
    );
    print_row(&s, "Execution Engines   : ", &engines_info);
    print_row(
        &s,
        "Virtual MMU         : ",
        "Sparse 4KB Page Table (39-bit & 48-bit Horizon Address Space)",
    );
    print_row(
        &s,
        "Memory Sub-Regions  : ",
        "Code, RoData, Heap, Stack (1MB), TLS, SharedMem, VRAM, RomFS",
    );
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Graphics Emulation");
    print_row(
        &s,
        "Architecture        : ",
        "Bounded Maxwell/Ampere guest command emulation",
    );
    print_row(
        &s,
        "Execution           : ",
        "Software paths, with optional host-GPU presentation",
    );
    print_row(
        &s,
        "Guest Driver Path   : ",
        "nvdrv GPU mappings and Maxwell/Ampere channel decoding",
    );
    print_row(
        &s,
        "Pushbuffer ISA      : ",
        "GPFIFO Channel Methods (0x100..0x10A, Texture Descriptor 0x0B)",
    );
    print_row(
        &s,
        "Shader Execution    : ",
        "Guest SASS -> IR -> CPU, or WGSL host draws with readback",
    );
    print_row(
        &s,
        "Upscaler Model      : ",
        "2x upscaler model (bilinear + sharpen, not DLSS)",
    );
    print_row(
        &s,
        "Host Execution      : ",
        "Experimental: gpu-present + OUNCE_GPU_BACKEND=host; see docs/guest-host-gpu.md",
    );
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Guest Memory Model");
    print_row(&s, "Address Space       : ", "Sparse guest pages allocated on demand");
    print_row(
        &s,
        "VRAM Framebuffer    : ",
        "0x9000_0000 (1280x720 Handheld / 1920x1080 Docked Linear RGBA)",
    );
    print_row(
        &s,
        "Pushbuffer Ring     : ",
        "0x9400_0000 (Asynchronous GPFIFO Command Submission Ring)",
    );
    print_row(
        &s,
        "DMA Controller      : ",
        "Zero-Copy Unified Virtual Memory & Audio DMA Circular Buffers",
    );
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Audio and Input Emulation");
    print_row(
        &s,
        "Audio DSP Mixer     : ",
        "Horizon audren:u Audio Renderer (Multi-Channel Voice Matrix)",
    );
    print_row(
        &s,
        "Audio Output Sinks  : ",
        "48kHz Stereo 16-bit Linear PCM Audio Stream (audout:u)",
    );
    print_row(
        &s,
        "Controller Input    : ",
        "Guest HID state from configured host input",
    );
    print_row(
        &s,
        "Analog Filtering    : ",
        "Hardware Button Bitmasking & Analog Stick Radial Deadzones",
    );
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Homebrew Formats & Storage");
    print_row(&s, "Executables        : ", "Homebrew NRO0 and NSO0 modules");
    print_row(
        &s,
        "Loose Folders       : ",
        "Unencrypted exefs/ + romfs/ directories (OunceCake format)",
    );
    print_row(
        &s,
        "RomFS Filesystem    : ",
        "Level 3 Directory & File Hash Tables with Hierarchical Trees",
    );
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Horizon OS 64-bit Microkernel & HLE Services");
    print_row(
        &s,
        "IPC Protocols       : ",
        "Horizon Tipc (0x10/0x12) & CMIF (SFCI / SFCO)",
    );
    print_row(
        &s,
        "Active Services     : ",
        "set:sys, am:appletOE, vi:m, hid, fsp-srv, ro:1, pl:u, time:u, nvdrv",
    );
    print_row(&s, "Firmware Target     : ", "Selected Horizon service ABI behaviours");
    print_row(
        &s,
        "Process Model       : ",
        "Single host thread; guest scheduling is emulated",
    );
    print_box_bottom(&s);
    println!();

    print_box_top(&s, "Compliance & Security Verification");
    print_row(
        &s,
        "Distribution Status: ",
        "No keys, firmware, or retail content are distributed",
    );
    print_row(
        &s,
        "Homebrew Loader     : ",
        "Runs unencrypted homebrew NRO/NSO binaries",
    );
    let val_validation = "Run cargo test --release and ounce-core test for current results";
    print_row(&s, "Validation Status   : ", val_validation);
    let val_source = format!("{}https://github.com/hffgzktbcc-blip/ounce-core{}", s.cyan, s.reset);
    print_row(&s, "Official Source     : ", &val_source);
    let val_sec = format!(
        "{}Reject third-party binaries; build only from official source{}",
        s.yellow, s.reset
    );
    print_row(&s, "Security Notice     : ", &val_sec);
    print_box_bottom(&s);
    println!();
}
