//! `run`, `run-nro` and directory-package execution, plus NRO helpers.

use crate::*;

pub(crate) fn inspect_directory_package(dir_path: &str) {
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

/// Shell callers must distinguish a completed run from a load/execution failure.
pub(crate) fn require_cli_success(outcome: VmOutcome) {
    run_report::write_if_configured(&outcome.report);
    if !matches!(outcome.stop, VmStop::ExitProcess | VmStop::WindowClosed | VmStop::FrameLimit) {
        let stop = match outcome.stop {
            VmStop::Break => cli_flow::Stop::Break,
            VmStop::UnhandledSvc => cli_flow::Stop::UnhandledSvc,
            VmStop::Halt => cli_flow::Stop::Halt,
            VmStop::Budget => cli_flow::Stop::Budget,
            _ => cli_flow::Stop::Fault,
        };
        let engine = match cpu::CpuTier::from_env() {
            cpu::CpuTier::Interpreter => "interpreter",
            cpu::CpuTier::Tier3 => "tier3",
            cpu::CpuTier::Tier4 => "tier4",
            cpu::CpuTier::Native => "native",
        };
        let program = RUN_TARGET.get().map(String::as_str).unwrap_or("<file.nro>");
        eprint!("{}", cli_flow::next_steps(stop, program, engine));
        std::process::exit(1);
    }
}

pub(crate) fn run_directory_package(dir_path: &str, gui_mode: bool) {
    println!("[Loader] Bootstrapping loose directory package (OunceCake format): {}", dir_path);
    let pkg = match loader::LoosePackageLoader::discover(dir_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[Loader Error] Failed to discover loose package: {}", e);
            std::process::exit(1);
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
                require_cli_success(run_vm_internal(cpu, mmu, kernel, Some(&mut display), Some(&mut hid), 0, 0));
            } else {
                cpu.regs.pc = DEFAULT_BASE_ADDR;
                require_cli_success(run_vm(cpu, mmu, kernel, 100_000));
            }
        }
        Err(e) => {
            eprintln!("[Loader Error] Failed to load loose package into VM: {}", e);
            std::process::exit(1);
        }
    }
}

/// The words after the program path of `run` / `run-nro`: host flags, and the guest's `argv[1..]`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RunArgs<'a> {
    pub gui: bool,
    pub guest: Vec<&'a str>,
}

/// Splits the words after the program path. Words after `--` go to the guest unchanged, dashes and all.
/// Before `--`, a dashed word must be a `run` flag: an unknown one is an error rather than being
/// dropped, so a flag meant for the program never silently disappears (#487). Global options
/// (`--cpu=`, `--present=`, `--sdmc=`, `--report-json`, ...) were already removed by `main`.
pub(crate) fn parse_run_args<'a>(command: &str, path: &str, words: &'a [String]) -> Result<RunArgs<'a>, String> {
    let mut parsed = RunArgs { gui: false, guest: Vec::new() };
    let mut words = words.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--" => {
                parsed.guest.extend(words.map(String::as_str));
                break;
            }
            "--gui" | "--window" | "-g" => parsed.gui = true,
            // Already applied by `main`; kept in the argument list for commands that forward it.
            "--deterministic" => {}
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(format!(
                    "{command}: unknown option '{flag}'. To pass it to the program, put it after `--`:\n  \
                     ounce-core {command} {path} -- {flag}\nSee `ounce-core {command} --help`."
                ));
            }
            arg => parsed.guest.push(arg),
        }
    }
    // libnx splits argv on spaces and drops an empty token even inside quotes, so an empty argument would vanish and
    // the guest would see a smaller argc than it was run with. Refuse it before anything starts.
    if let Some(position) = parsed.guest.iter().position(|a| a.is_empty()) {
        return Err(format!(
            "{command}: empty guest argument (word {} of the program's arguments). libnx cannot receive an empty argv \
             entry, so the program would see fewer arguments than you passed. Remove it.",
            position + 1
        ));
    }
    Ok(parsed)
}

pub(crate) fn print_run_usage(command: &str) {
    let accepts = if command == "run-nro" { "a homebrew .nro" } else { "a .nro, .nso or loose folder" };
    println!("usage: ounce-core {command} <path> [--gui] [args...] [-- args...]\n");
    println!("Runs {accepts}. Words after the path that do not start with '-' become the program's argv[1..].");
    println!("Only .nro programs have an argv: guest arguments for a .nso or a folder are refused (exit status 2).");
    println!("Everything after `--` is passed to the program unchanged, including words that start with '-'.");
    println!("Any other word starting with '-' before `--` is an error (exit status 2), never silently dropped.\n");
    println!("  --gui, --window, -g   open a window");
    println!("  --deterministic       fixed guest clock (time advances per system call) and software 3D, so a run repeats");
    println!("Global options such as --cpu=, --present=, --sdmc=, --report-json, --max-frames and --press may appear");
    println!("anywhere before `--`; see `ounce-core help`.\n");
    println!("examples:");
    println!("  ounce-core {command} tests/fixtures/nro/hello/hello.nro --present=headless fixture");
    println!("  ounce-core {command} my_test.nro --present=headless -- --verbose --filter=fp");
}

/// Only `.nro` programs receive an argv (the homebrew ABI). Refuse guest arguments for any other target instead of
/// dropping them (#487).
fn refuse_guest_args(target: &str, guest_args: &[&str]) -> ! {
    eprintln!(
        "[CLI] guest arguments ({}) are only passed to .nro programs; {target} has no argv. Remove them, or run a .nro.",
        guest_args.join(" ")
    );
    std::process::exit(2);
}

pub(crate) fn run_file(path: &str, guest_args: &[&str], gui_mode: bool, nro_only: bool) {
    if !nro_only && std::path::Path::new(path).is_dir() {
        if !guest_args.is_empty() {
            refuse_guest_args("a loose package folder", guest_args);
        }
        run_directory_package(path, gui_mode);
        return;
    }
    println!("[Loader] Reading package/executable: {}", path);
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[Loader Error] Failed to read file '{}': {}", path, e);
            std::process::exit(1);
        }
    };
    if !guest_args.is_empty() && bytes.get(0x10..0x14) != Some(b"NRO0".as_slice()) {
        refuse_guest_args("this file is not an NRO and", guest_args);
    }

    if cpu::CpuTier::from_env() == cpu::CpuTier::Native && bytes.get(0x10..0x14) == Some(b"NRO0".as_slice()) {
        #[cfg(feature = "nce")]
        {
            if gui_mode {
                println!("[NCE] Native execution runs headless for now.");
            }
            if !run_native_nro(path, &bytes, guest_args) { std::process::exit(1); }
            return;
        }
        #[cfg(not(feature = "nce"))]
        eprintln!("[CPU] --cpu=native needs a build with `--features nce`; running the interpreter.");
    }

    let (mut cpu, mut mmu, mut kernel) = setup_vm();
    let package = if nro_only {
        BootLoader::load_homebrew_nro(&bytes, DEFAULT_BASE_ADDR, &mut mmu)
    } else {
        BootLoader::load_any_package(&bytes, DEFAULT_BASE_ADDR, &mut mmu)
    };
    match package {
        Ok(info) => {
            let is_nro = info.format == "NRO0";
            println!("[BootLoader] Successfully loaded package:");
            println!("  Format      : {}", info.format);
            println!("  Title ID    : 0x{:016X}", info.title_id);
            println!("  Code size   : 0x{:08X} ({} bytes)", info.code_size, info.code_size);
            println!("  MOD0 relocs : {}", info.relocations_applied);

            // Homebrew ABI for NRO0: ConfigEntry list in X0 (X1 = -1), a heap handed over so libnx skips
            // svcSetHeapSize, and X30 at a svcExitProcess stub, which is where libnx's exit returns.
            if is_nro && let Err(e) = setup_homebrew_nro(&mut cpu, &mut mmu, &mut kernel, path, &bytes, guest_args) {
                eprintln!("[Loader Error] homebrew environment: {}", e);
                std::process::exit(1);
            }
            // After the NRO went on the virtual SD card, so it stays in memory and is not copied into the folder.
            if let Some(dir) = SDMC_HOST_DIR.get() {
                kernel.vfs.set_sdmc_directory(dir);
                kernel.vfs.sdmc_write_through = SDMC_WRITEBACK.load(std::sync::atomic::Ordering::Relaxed);
                println!("  sdmc:/      : {} (sandboxed copy-on-write overlay)", dir.display());
            }

            let fb_size = SCREEN_WIDTH * SCREEN_HEIGHT * 4;
            mmu.map_range(DEFAULT_FRAMEBUFFER_ADDR, fb_size, Permission::READ_WRITE);
            mmu.map_range(HID_SHARED_MEM_ADDR, 0x1000, Permission::READ_WRITE);

            if gui_mode {
                println!("  [Display Server] Launching native window (1280x720 60 FPS) with Joy-Con 2 controls...");
                let title = if is_nro {
                    let name = std::path::Path::new(path).file_stem().and_then(|stem| stem.to_str()).unwrap_or("Homebrew");
                    format!("{name} — Ounce Core")
                } else {
                    format!("Ounce-Core | 0x{:016X} [T239]", info.title_id)
                };
                let mut display = DisplayServer::new(
                    DEFAULT_FRAMEBUFFER_ADDR,
                    DisplayConfig {
                        title,
                        ..Default::default()
                    },
                );
                let mut hid = HidServer::new(HID_SHARED_MEM_ADDR);
                cpu.regs.pc = DEFAULT_BASE_ADDR;
                require_cli_success(run_vm_internal(cpu, mmu, kernel, Some(&mut display), Some(&mut hid), 0, 0));
            } else {
                cpu.regs.pc = DEFAULT_BASE_ADDR;
                // libnx startup alone runs tens of thousands of instructions.
                let budget = if is_nro { nro_instruction_budget() } else { 100_000 };
                // `--present=headless` gets a windowless display so queued VI frames are decoded, counted and
                // hashed for the run report. Without it the run has no display, as before.
                let mut headless_display = (gpu::BackendKind::from_env() == gpu::BackendKind::Headless).then(|| {
                    DisplayServer::new(
                        DEFAULT_FRAMEBUFFER_ADDR,
                        DisplayConfig { backend: gpu::BackendKind::Headless, ..Default::default() },
                    )
                });
                if loop_controls::active() {
                    let mut hid = HidServer::new(HID_SHARED_MEM_ADDR);
                    require_cli_success(run_vm_internal(cpu, mmu, kernel, headless_display.as_mut(), Some(&mut hid), budget, 0));
                } else if headless_display.is_some() {
                    require_cli_success(run_vm_internal(cpu, mmu, kernel, headless_display.as_mut(), None, budget, 0));
                } else {
                    require_cli_success(run_vm(cpu, mmu, kernel, budget));
                }
            }
        }
        Err(e) => {
            if nro_only && bytes.starts_with(b"\x7fELF") {
                eprintln!(
                    "[Loader Error] '{}' is an ELF binary, not a Switch homebrew .nro ({}). Static AArch64 Linux ELFs run with: ounce-core run-elf {}",
                    path, e, path
                );
            } else if nro_only && matches!(e, loader::LoadError::TooSmall) {
                eprintln!(
                    "[Loader Error] '{}' is {} bytes (an NRO0 header requires at least 128 bytes): {}",
                    path,
                    bytes.len(),
                    e
                );
            } else {
                eprintln!("[Loader Error] {}", e);
            }
            std::process::exit(1);
        }
    }
}

/// `run <nro> --cpu=native`: the NRO on the host CPU, headless, until it exits, breaks or faults.
#[cfg(feature = "nce")]
pub(crate) fn run_native_nro(path: &str, bytes: &[u8], guest_args: &[&str]) -> bool {
    use cpu::nce::{NceEngine, NceExit};
    let arena = match cpu::nce::reserve_window() {
        Ok(arena) => arena,
        Err(e) => return { eprintln!("[NCE] Native execution unavailable: {e}"); false },
    };
    let base = arena.base();
    let (mut cpu, mut mmu, mut kernel) = setup_vm_in(Memory::with_arena(arena), base, cpu::nce::ALIAS_SIZE);
    let code = base + DEFAULT_BASE_ADDR;
    if let Err(e) = BootLoader::load_homebrew_nro(bytes, code, &mut mmu) {
        return { eprintln!("[Loader Error] {e}"); false };
    }
    if let Err(e) = setup_homebrew_nro(&mut cpu, &mut mmu, &mut kernel, path, bytes, guest_args) {
        return { eprintln!("[Loader Error] homebrew environment: {e}"); false };
    }
    if let Some(dir) = SDMC_HOST_DIR.get() {
        kernel.vfs.set_sdmc_directory(dir);
                kernel.vfs.sdmc_write_through = SDMC_WRITEBACK.load(std::sync::atomic::Ordering::Relaxed);
    }
    cpu.regs.pc = code;
    // The NRO header (after the entry branch) is data.
    let mut engine = match NceEngine::new(&mut mmu, base, vec![(code + 4, code + 0x80)]) {
        Ok(engine) => engine,
        Err(e) => return { eprintln!("[NCE] {e}"); false },
    };
    println!("[NCE] Running natively at 0x{code:016X} (guest window 0x{base:X})");
    let start = Instant::now();
    let (mut svcs, mut frames) = (0u64, 0u64);
    let mut completed = false;
    let time_limit = nro_time_limit();
    loop {
        if time_limit.is_some_and(|limit| start.elapsed() >= limit) {
            println!("[NCE] Time limit reached ({:.1} s, OUNCE_NRO_TIMEOUT_SECS); stopping the guest.", start.elapsed().as_secs_f64());
            break;
        }
        match engine.run(&mut cpu, &mut mmu, &|| kernel.system_tick()) {
            NceExit::Svc(id) => {
                svcs += 1;
                match kernel.dispatch_svc(id, &mut cpu, &mut mmu) {
                    SvcResult::Success => {}
                    SvcResult::PresentFrame => {
                        frames += 1;
                        println!("[VM] VSync / Frame {frames} presented.");
                    }
                    SvcResult::ExitProcess => { completed = true; println!("[VM] Guest exited (svcExitProcess)."); break; }
                    SvcResult::Break { reason, info1, info2 } => {
                        println!("[VM] Hit svcBreak: reason=0x{reason:X}, info1=0x{info1:X}, info2=0x{info2:X}");
                        println!("[VM] Break PC: 0x{:016X}", cpu.regs.pc);
                        if info2 == 4 {
                            match mmu.read_u32(info1) {
                                Ok(result) => println!(
                                    "[VM] Break result: 0x{result:08X} (module={}, description={})",
                                    result & 0x1FF,
                                    (result >> 9) & 0x1FFF
                                ),
                                Err(error) => println!("[VM] Break result unreadable: {error}"),
                            }
                        }
                        break;
                    }
                    SvcResult::Unhandled(id) => { println!("[VM] Stopping on unhandled SVC #0x{id:02X}."); break; }
                }
            }
            NceExit::Preempted => kernel.scheduler.preempt(&mut cpu),
            NceExit::Fault { pc, signal, addr } => {
                if let Some(reason) = cpu.last_error.take().filter(|_| signal == 0) {
                    println!("[VM Error] {reason}");
                    println!("  Final PC: 0x{pc:016X}");
                } else {
                    println!("[VM] CPU fault (native signal {signal}) at 0x{pc:016X}, address 0x{addr:X}");
                }
                break;
            }
            NceExit::Error(e) => { eprintln!("[NCE] {e}"); break; }
        }
    }
    println!("[NCE] {svcs} SVCs, {frames} frames in {:.3} s; rewrote {:?}; {} instructions interpreted",
        start.elapsed().as_secs_f64(), engine.stats, engine.interpreted_steps);
    completed
}

/// Heap handed to homebrew through the `OverrideHeap` ConfigEntry.
pub(crate) const NRO_HEAP_SIZE: usize = 0x1000_0000; // 256 MB: Mesa-based homebrew allocates tens of MB of GPU buffers
pub(crate) const NRO_CONFIG_ADDR: u64 = 0x7FFF_0000;
pub(crate) const NRO_EXIT_STUB_ADDR: u64 = 0x7FFE_0000;
/// Main thread handle passed to homebrew. Not the `CurrentThread` pseudo-handle (0xFFFF8000): libnx mutexes
/// store the owner's handle and use bit 30 as their waiters flag, so a handle with bit 30 set never reads
/// as "locked by this thread". Below the scheduler's (0x2000+) and IPC's (0x10001+) handle ranges.
pub(crate) const NRO_MAIN_THREAD_HANDLE: u32 = 0x1000;
pub(crate) const NRO_INSTRUCTION_BUDGET: usize = 50_000_000;
#[cfg(any(feature = "nce", test))]
/// Parses a wall-clock limit in seconds (`OUNCE_NRO_TIMEOUT_SECS`); anything non-positive or unparsable means no limit.
pub(crate) fn parse_time_limit(value: &str) -> Option<std::time::Duration> {
    value.trim().parse::<f64>().ok().filter(|secs| secs.is_finite() && *secs > 0.0).map(std::time::Duration::from_secs_f64)
}

#[cfg(feature = "nce")]
/// `OUNCE_NRO_TIMEOUT_SECS=<n>` stops a native (`--cpu=native`) run after `n` seconds of wall-clock time. Native has
/// no instruction budget, so an interactive program that waits for input would otherwise run until killed.
pub(crate) fn nro_time_limit() -> Option<std::time::Duration> {
    std::env::var("OUNCE_NRO_TIMEOUT_SECS").ok().and_then(|v| parse_time_limit(&v))
}

/// Instruction cap for an NRO run: `OUNCE_NRO_BUDGET=<n>` raises it for long programs (default 50 million).
pub(crate) fn nro_instruction_budget() -> usize {
    std::env::var("OUNCE_NRO_BUDGET").ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(NRO_INSTRUCTION_BUDGET)
}
/// The host audio sink for a run. Headless runs (`--present=headless`, or `OUNCE_PRESENT=headless`) have no window and
/// open no audio device, so an audio-less machine (a CI runner) prints no device errors. Other modes open the default
/// output device as before.
pub(crate) fn host_audio_for(present: gpu::BackendKind) -> HostAudioSink {
    if audio_wanted(present) {
        HostAudioSink::new()
    } else {
        HostAudioSink::disabled()
    }
}

pub(crate) fn audio_wanted(present: gpu::BackendKind) -> bool {
    present != gpu::BackendKind::Headless && !run_report::is_audio_null()
}

/// Mixed audio (interleaved stereo samples) allowed to queue on the host before the guest waits for
/// playback to catch up: 100 ms at 48 kHz.
pub(crate) const AUDIO_LEAD_SAMPLES: usize = 48_000 * 2 / 10;

/// Hands newly mixed audio renderer samples to the host and waits while playback is more than
/// `AUDIO_LEAD_SAMPLES` behind, so a guest's audio loop runs at real-time speed. Without an audio
/// device (`is_enabled` false) nothing waits.
pub(crate) fn stream_audio_to_host(kernel: &mut HorizonKernel, host_audio: &HostAudioSink) {
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
pub(crate) const INPUT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(4);

/// Sets up the homebrew loader ABI for an NRO loaded at `DEFAULT_BASE_ADDR`.
/// `nro` is also placed on the virtual SD card and `argv[0]` names it as `sdmc:/<file name>`, the way
/// homebrew loaders start NROs; libnx `romfsInit` reads the NRO's RomFS back through that path.
pub(crate) fn setup_homebrew_nro(
    cpu: &mut Cpu,
    mmu: &mut Memory,
    kernel: &mut HorizonKernel,
    path: &str,
    nro: &[u8],
    guest_args: &[&str],
) -> Result<(), loader::ConfigError> {
    mmu.map_range(kernel.heap_base, NRO_HEAP_SIZE, Permission::READ_WRITE);
    kernel.heap_size = NRO_HEAP_SIZE;
    kernel.scheduler.main_handle = Some(NRO_MAIN_THREAD_HANDLE);
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
        next_load: true,
        ..Default::default()
    }
    .setup(cpu, mmu, kernel.addr_base + NRO_CONFIG_ADDR)?;
    let stub = loader::HomebrewConfig::install_exit_stub(mmu, kernel.addr_base + NRO_EXIT_STUB_ADDR)?;
    cpu.regs.write_x(30, stub);
    Ok(())
}

/// Synthesizes a clean-room unencrypted loose Switch 2 directory package (OunceCake format)
/// on disk and executes it directly through `run_directory_package`.
pub(crate) fn build_and_boot_demo_dir(out_dir: &str, gui_mode: bool) {
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

#[cfg(test)]
mod host_audio_tests {
    use super::*;

    #[test]
    fn headless_run_does_not_open_the_audio_device() {
        // Only headless skips the device; the other presentation modes keep opening it (not exercised here, so
        // this test needs no audio hardware).
        let sink = host_audio_for(gpu::BackendKind::Headless);
        assert!(!sink.open_attempted);
        assert!(!sink.is_enabled);
        assert!(!audio_wanted(gpu::BackendKind::Headless));
        assert!(audio_wanted(gpu::BackendKind::Auto));
        assert!(audio_wanted(gpu::BackendKind::Minifb));
        assert!(audio_wanted(gpu::BackendKind::Gpu));
    }
}

#[cfg(test)]
mod time_limit_tests {
    use super::parse_time_limit;
    use std::time::Duration;

    #[test]
    fn parses_positive_seconds_and_rejects_the_rest() {
        assert_eq!(parse_time_limit("2"), Some(Duration::from_secs(2)));
        assert_eq!(parse_time_limit(" 0.5 "), Some(Duration::from_millis(500)));
        for bad in ["", "0", "-3", "abc", "NaN", "inf"] {
            assert_eq!(parse_time_limit(bad), None, "{bad:?}");
        }
    }
}

#[cfg(test)]
mod run_args_tests {
    use super::{parse_run_args, RunArgs};

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn plain_words_before_the_separator_are_guest_arguments() {
        let w = words(&["fixture", "two"]);
        assert_eq!(parse_run_args("run-nro", "a.nro", &w), Ok(RunArgs { gui: false, guest: vec!["fixture", "two"] }));
    }

    #[test]
    fn everything_after_the_separator_reaches_the_guest_unchanged() {
        // Including words that look like ounce-core flags: they belong to the program now (#487).
        let w = words(&["fixture", "--", "-x", "--gui", "--", "--deterministic"]);
        let parsed = parse_run_args("run-nro", "a.nro", &w).unwrap();
        assert_eq!(parsed.guest, ["fixture", "-x", "--gui", "--", "--deterministic"]);
        assert!(!parsed.gui, "--gui after `--` is a guest argument, not a window request");
    }

    #[test]
    fn run_flags_before_the_separator_are_consumed() {
        for flag in ["--gui", "--window", "-g"] {
            let w = words(&[flag, "fixture"]);
            assert_eq!(parse_run_args("run", "a.nro", &w), Ok(RunArgs { gui: true, guest: vec!["fixture"] }));
        }
        let w = words(&["--deterministic"]);
        assert_eq!(parse_run_args("run", "a.nro", &w), Ok(RunArgs { gui: false, guest: vec![] }));
    }

    #[test]
    fn unknown_dashed_word_before_the_separator_is_an_error_that_shows_the_fix() {
        let w = words(&["fixture", "--verbose"]);
        let err = parse_run_args("run-nro", "app.nro", &w).unwrap_err();
        assert!(err.contains("unknown option '--verbose'"), "{err}");
        assert!(err.contains("ounce-core run-nro app.nro -- --verbose"), "{err}");
    }

    #[test]
    fn a_lone_dash_is_an_ordinary_argument() {
        let w = words(&["-"]);
        assert_eq!(parse_run_args("run", "a.nro", &w).unwrap().guest, ["-"]);
    }
}
