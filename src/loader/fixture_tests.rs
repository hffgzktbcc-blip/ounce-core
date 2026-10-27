//! Boots a real devkitA64 + libnx homebrew NRO (`tests/fixtures/nro/hello`) through its own `crt0`.
//!
//! `boot` services the few Horizon SVCs libnx startup makes with a small shim of its own, so the
//! loader is checked independently of the kernel model; `libnx_nro_runs_under_cli_setup_and_kernel`
//! runs the same program through the CLI's setup and `HorizonKernel::dispatch_svc`. The fixture reports `0xEEE` when its checks pass, `0xBAD0000 | bits` otherwise.

use super::homebrew::HomebrewConfig;
use super::nro::NroLoader;
use crate::cpu::{Cpu, StepResult};
use crate::mmu::{Memory, Permission};

const NRO_BASE: u64 = 0x0800_0000;
const HEAP_BASE: u64 = 0x2_0000_0000;
const HEAP_SIZE: u64 = 0x100_0000;
const STACK_REGION: u64 = 0x4_0000_0000;
const STACK_SIZE: u64 = 0x10_0000;
const TLS_BASE: u64 = 0x5_0000_0000;
const CONFIG_BASE: u64 = 0x5_0001_0000;
const RETURN_SENTINEL: u64 = 0xFFFF_0000_DEAD_0000;
const MAIN_THREAD_HANDLE: u32 = 0xD000;
const MAX_STEPS: usize = 50_000_000;

#[derive(Debug)]
#[allow(dead_code)] // fields are read through Debug in the failure message
enum Outcome {
    Returned(u64),
    ExitProcess,
    Break { reason: u64, pc: u64 },
    Fault { error: String, pc: u64 },
    StepLimit,
}

#[derive(Debug, Default)]
struct Shim {
    svcs: Vec<u16>,
    debug_output: String,
    /// Last guest PCs executed, oldest first, for failure messages.
    trail: std::collections::VecDeque<u64>,
}

impl Shim {
    /// Services one Horizon SVC. Returns `Some` when the guest is done.
    fn handle(&mut self, svc: u16, cpu: &mut Cpu, mmu: &mut Memory) -> Option<Outcome> {
        self.svcs.push(svc);
        let x = |cpu: &Cpu, i| cpu.regs.read_x(i);
        match svc {
            // svcQueryMemory(out_info, out_page_info, addr)
            0x06 => {
                let info = x(cpu, 0);
                let page = x(cpu, 2) & !0xFFF;
                let (kind, perm) = if mmu.is_mapped(page) { (3u32, 3u32) } else { (0, 0) };
                let _ = mmu.write_u64(info, page);
                let _ = mmu.write_u64(info + 8, 0x1000);
                let _ = mmu.write_u32(info + 16, kind);
                let _ = mmu.write_u32(info + 20, 0);
                let _ = mmu.write_u32(info + 24, perm);
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, 0);
            }
            // svcExitProcess()
            0x07 => return Some(Outcome::ExitProcess),
            // svcBreak(reason, arg, size)
            0x26 => return Some(Outcome::Break { reason: x(cpu, 0), pc: cpu.regs.pc }),
            // svcOutputDebugString(str, len)
            0x27 => {
                let mut buf = vec![0u8; x(cpu, 1).min(0x1000) as usize];
                let _ = mmu.read_bytes(x(cpu, 0), &mut buf);
                self.debug_output.push_str(&String::from_utf8_lossy(&buf));
                cpu.regs.write_x(0, 0);
            }
            // svcGetInfo(out, type, handle, subtype)
            0x29 => {
                let value = match x(cpu, 1) as u32 {
                    2 => 0x10_0000_0000,  // alias region address
                    3 => 0x10_0000_0000,  // alias region size
                    4 => HEAP_BASE,       // heap region address
                    5 => 0x1_0000_0000,   // heap region size
                    6 => 0x1_0000_0000,   // total memory
                    7 => HEAP_SIZE,       // used memory
                    11 => 0x0123_4567_89AB_CDEF ^ x(cpu, 3), // random entropy
                    12 => 0x0800_0000,    // ASLR region address
                    13 => 0x7F_F800_0000, // ASLR region size
                    14 => STACK_REGION,   // stack region address
                    15 => 0x1_0000_0000,  // stack region size
                    _ => 0,
                };
                cpu.regs.write_x(0, 0);
                cpu.regs.write_x(1, value);
            }
            // Anything else succeeds with no outputs; the SVC list in the failure message shows it.
            _ => cpu.regs.write_x(0, 0),
        }
        None
    }
}

/// Loads `nro`, hands it a homebrew `ConfigEntry` list and runs it from its entry point.
fn boot(nro: &[u8], argv: &str) -> (Outcome, Shim) {
    let mut mmu = Memory::new();
    let mut cpu = Cpu::new();
    NroLoader::load_into_memory(nro, NRO_BASE, &mut mmu).expect("NRO loads");

    mmu.map_range(HEAP_BASE, HEAP_SIZE as usize, Permission::READ_WRITE);
    mmu.map_range(STACK_REGION, STACK_SIZE as usize, Permission::READ_WRITE);
    mmu.map_range(TLS_BASE, 0x1000, Permission::READ_WRITE);
    cpu.regs.tpidrro_el0 = TLS_BASE;

    HomebrewConfig {
        main_thread_handle: Some(MAIN_THREAD_HANDLE),
        heap: Some((HEAP_BASE, HEAP_SIZE)),
        argv: Some(argv.into()),
        ..Default::default()
    }
    .setup(&mut cpu, &mut mmu, CONFIG_BASE)
    .expect("config list written");

    cpu.regs.pc = NRO_BASE;
    cpu.regs.sp = STACK_REGION + STACK_SIZE;
    cpu.regs.write_x(30, RETURN_SENTINEL);

    let mut shim = Shim::default();
    for _ in 0..MAX_STEPS {
        if cpu.regs.pc == RETURN_SENTINEL {
            return (Outcome::Returned(cpu.regs.read_x(0)), shim);
        }
        let pc = cpu.regs.pc;
        if shim.trail.len() == 24 {
            shim.trail.pop_front();
        }
        shim.trail.push_back(pc - NRO_BASE);
        match cpu.step(&mut mmu) {
            Ok(StepResult::Continue) => {}
            Ok(StepResult::Svc(svc)) => {
                if let Some(done) = shim.handle(svc, &mut cpu, &mut mmu) {
                    return (done, shim);
                }
            }
            Ok(StepResult::Halt(reason)) => return (Outcome::Fault { error: reason, pc }, shim),
            Err(e) => return (Outcome::Fault { error: e.to_string(), pc }, shim),
        }
    }
    (Outcome::StepLimit, shim)
}

fn fixture() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/hello/hello.nro");
    std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e} (build it with `make` in that folder)"))
}

#[test]
fn libnx_nro_header_parses() {
    let nro = fixture();
    let header = NroLoader::parse_header(&nro).expect("NRO0 header");
    assert_eq!(header.text_offset, 0);
    assert!(header.text_size > 0 && header.rodata_size > 0 && header.data_size > 0);
    assert_eq!(header.text_size % 0x1000, 0);
    assert_eq!(header.rodata_offset, header.text_size);
    let dynamic = {
        let mut mmu = Memory::new();
        NroLoader::load_into_memory(&nro, NRO_BASE, &mut mmu).unwrap();
        NroLoader::parse_dynamic(NRO_BASE, &mmu)
    };
    let dynamic = dynamic.expect("MOD0 points at a .dynamic section");
    assert!(dynamic.relr.is_some() && dynamic.relr_size > 0, "devkitA64 links libnx PIE with DT_RELR");
    assert!(dynamic.init_array.is_some(), "the fixture's constructor is in DT_INIT_ARRAY");
}

#[test]
fn libnx_nro_relr_relocations_apply() {
    let nro = fixture();
    let mut mmu = Memory::new();
    NroLoader::load_into_memory(&nro, NRO_BASE, &mut mmu).unwrap();
    let mut before = Memory::new();
    NroLoader::load_into_memory(&nro, NRO_BASE, &mut before).unwrap();
    let applied = NroLoader::apply_relr(NRO_BASE, &mut mmu);
    assert!(applied > 0);
    // Every relocated word now points into the image (or one past its end, like `__bss_end`).
    let header = NroLoader::parse_header(&nro).unwrap();
    let image_end = NRO_BASE + (header.data_offset + header.data_size + header.bss_size) as u64;
    let data = NRO_BASE + header.rodata_offset as u64;
    let mut changed = 0;
    for addr in (data..image_end).step_by(8) {
        let (old, new) = (before.read_u64(addr).unwrap_or(0), mmu.read_u64(addr).unwrap_or(0));
        if old != new {
            assert_eq!(new, old + NRO_BASE);
            assert!((NRO_BASE..=image_end).contains(&new), "0x{addr:X} -> 0x{new:X}");
            changed += 1;
        }
    }
    assert_eq!(changed, applied);
}

/// Boots the NRO through libnx `crt0` to `main` and back out through `exit` to the loader return
/// address. libnx passes the loader 0 whatever `main` returns, so the fixture's own verdict is read
/// from its `svcOutputDebugString` line.
#[test]
fn libnx_nro_runs_to_main_return() {
    let (outcome, shim) = boot(&fixture(), "sdmc:/hello.nro fixture");
    let ok = matches!(outcome, Outcome::Returned(0)) && shim.debug_output == "ounce-fixture result=0x00000EEE";
    assert!(
        ok,
        "fixture did not report 0xEEE: {outcome:x?}\nSVCs: {:x?}\nlast PCs (image offsets): {:x?}\ndebug output: {}",
        shim.svcs, shim.trail, shim.debug_output
    );
    // Startup asked for memory layout and permissions only; the heap came from the ConfigEntry list.
    assert!(!shim.svcs.contains(&0x01), "svcSetHeapSize despite OverrideHeap");
}

/// Boots `nro` the way `ounce-core run <nro> <args>` does: `setup_vm`, `setup_homebrew_nro`, and the
/// kernel's own SVC dispatch (no shim). Returns the guest's debug output and every SVC it made.
fn boot_with_kernel(nro: &[u8], args: &[&str]) -> (String, Vec<u16>) {
    let (output, svcs, _) = boot_with_kernel_state(nro, args);
    (output, svcs)
}

/// `boot_with_kernel`, also returning the kernel so tests can inspect service state afterwards.
fn boot_with_kernel_state(nro: &[u8], args: &[&str]) -> (String, Vec<u16>, crate::kernel::HorizonKernel) {
    boot_with_kernel_input(nro, args, Default::default())
}

/// `boot_with_kernel_state` with `input` held on the host controller for the whole run.
fn boot_with_kernel_input(
    nro: &[u8],
    args: &[&str],
    input: crate::kernel::hid::NpadInput,
) -> (String, Vec<u16>, crate::kernel::HorizonKernel) {
    let (mut cpu, mut mmu, mut kernel) = crate::setup_vm();
    kernel.ipc.hid.input = input;
    let info = super::BootLoader::load_any_package(nro, crate::DEFAULT_BASE_ADDR, &[0; 16], &mut mmu).expect("NRO loads");
    assert_eq!(info.format, "NRO0");
    crate::setup_homebrew_nro(&mut cpu, &mut mmu, &mut kernel, "fixture.nro", nro, args).unwrap();
    cpu.regs.pc = crate::DEFAULT_BASE_ADDR;

    // One CPU runs every guest thread: the kernel swaps thread registers inside `dispatch_svc`.
    let mut output = String::new();
    let mut svcs = Vec::new();
    let mut exited = false;
    for step in 0..crate::NRO_INSTRUCTION_BUDGET {
        if step % crate::kernel::scheduler::PREEMPT_INTERVAL == crate::kernel::scheduler::PREEMPT_INTERVAL - 1 {
            kernel.scheduler.preempt(&mut cpu);
        }
        match cpu.step(&mut mmu) {
            Ok(StepResult::Continue) => {}
            Ok(StepResult::Svc(svc)) => {
                svcs.push(svc);
                if svc == 0x27 {
                    let mut buf = vec![0u8; cpu.regs.read_x(1).min(0x1000) as usize];
                    mmu.read_bytes(cpu.regs.read_x(0), &mut buf).unwrap();
                    output.push_str(&String::from_utf8_lossy(&buf));
                }
                match kernel.dispatch_svc(svc, &mut cpu, &mut mmu) {
                    crate::kernel::SvcResult::ExitProcess => {
                        exited = true;
                        break;
                    }
                    crate::kernel::SvcResult::Success => {}
                    other => panic!("SVC 0x{svc:X} -> {other:?}; SVCs so far {svcs:x?}"),
                }
            }
            other => panic!("guest stopped: {other:?} at 0x{:X}; SVCs {svcs:x?}", cpu.regs.pc),
        }
    }
    assert!(
        exited,
        "guest did not exit within {} instructions (PC 0x{:X}); output so far: {output:?}",
        crate::NRO_INSTRUCTION_BUDGET,
        cpu.regs.pc
    );
    (output, svcs, kernel)
}

/// The hello fixture under the CLI setup and the real kernel.
#[test]
fn libnx_nro_runs_under_cli_setup_and_kernel() {
    let (output, svcs) = boot_with_kernel(&fixture(), &["fixture"]);
    assert_eq!(output, "ounce-fixture result=0x00000EEE", "SVCs: {svcs:x?}");
    assert_eq!(svcs.last(), Some(&0x07), "exits through the loader return stub");
    assert!(svcs.contains(&0x29), "libnx startup calls svcGetInfo");
}

/// `tests/fixtures/nro/services`: libnx `smInitialize` and `hidInitialize` against the kernel's services.
#[test]
fn libnx_services_fixture_opens_sm_and_hid() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/services/services.nro");
    let nro = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (output, svcs) = boot_with_kernel(&nro, &[]);
    assert!(output.ends_with("ounce-services result=0x00000EEE\n"), "output: {output:?}\nSVCs: {svcs:x?}");
    assert!(svcs.contains(&0x1F) && svcs.contains(&0x21), "connects to sm: and sends requests");
    assert!(svcs.contains(&0x13), "maps the HID shared memory");
}

/// `tests/fixtures/nro/romfs`: libnx `fsInitialize` + `romfsInit` (the NRO's own RomFS, read from the
/// SD card filesystem through fsp-srv) and a read of `romfs:/greeting.txt`.
#[test]
fn libnx_romfs_fixture_reads_its_romfs() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/romfs/romfs.nro");
    let nro = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (output, svcs) = boot_with_kernel(&nro, &[]);
    assert!(output.ends_with("ounce-romfs result=0x00000EEE\n"), "output: {output:?}\nSVCs: {svcs:x?}");
}

/// `tests/fixtures/nro/nvdrv`: libnx `nvInitialize` (transfer memory, Initialize, cloned session),
/// `nvMapInit` and `nvMapCreate` (nvmap CREATE, ALLOC, GET_ID ioctls).
#[test]
fn libnx_nvdrv_fixture_creates_an_nvmap_object() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/nvdrv/nvdrv.nro");
    let nro = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (output, svcs) = boot_with_kernel(&nro, &[]);
    assert!(output.ends_with("ounce-nvdrv result=0x00000EEE\n"), "output: {output:?}\nSVCs: {svcs:x?}");
}

/// `tests/fixtures/nro/gpu`: libnx GPU bring-up (nvhost-ctrl, nvhost-ctrl-gpu, address space, GPFIFO
/// channel), a mapped command buffer submitted through SUBMIT_GPFIFO, and a fence wait.
#[test]
fn libnx_gpu_fixture_submits_a_command_buffer() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/gpu/gpu.nro");
    let nro = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (output, svcs, kernel) = boot_with_kernel_state(&nro, &[]);
    assert!(output.ends_with("ounce-gpu result=0x00000EEE\n"), "output: {output:?}\nSVCs: {svcs:x?}");
    // The fixture's one GPFIFO entry reached the GPU model: one header with two arguments.
    assert_eq!(kernel.nvgpu.state.total_commands_processed, 1);
    assert!(kernel.ipc.nvdrv.unhandled.is_empty(), "unhandled ioctls: {:x?}", kernel.ipc.nvdrv.unhandled);
}

/// `tests/fixtures/nro/input`: libnx `padConfigureInput` / `padInitializeDefault` / `padUpdate` read
/// the host controller from the HID shared memory the guest mapped.
#[test]
fn libnx_input_fixture_reads_host_controller() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/input/input.nro");
    let nro = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    // A + D-pad right held, left stick right and down.
    let input = crate::kernel::hid::NpadInput { buttons: 1 << 0 | 1 << 14, stick_l: (0x1234, -0x100), stick_r: (0, 0) };
    let (output, svcs, _) = boot_with_kernel_input(&nro, &[], input);
    assert!(
        output.contains("ounce-input buttons=0x00004001\n"),
        "output: {output:?}\nSVCs: {svcs:x?}"
    );
    assert!(output.contains("ounce-input stick_l=0x1234FF00\n"), "output: {output:?}");
    assert!(output.ends_with("ounce-input result=0x00000EEE\n"), "output: {output:?}");
}

/// `tests/fixtures/nro/threads`: two libnx worker threads share a total under a `Mutex`, yield while
/// holding it (so the other must block in svcArbitrateLock), report through a `CondVar`, and are joined
/// with `threadWaitForExit`; each checks its own `__thread` variable. A third thread spins without
/// SVCs and finishes only because the runner preempts it.
#[test]
fn libnx_threads_fixture_synchronizes_two_workers() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/threads/threads.nro");
    let nro = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (output, svcs, kernel) = boot_with_kernel_state(&nro, &[]);
    assert!(output.contains("ounce-threads total=0x00000384\n"), "output: {output:?}\nSVCs: {svcs:x?}");
    assert!(output.ends_with("ounce-threads result=0x00000EEE\n"), "output: {output:?}");
    // The workers really contended: some lock requests blocked and were handed over.
    assert!(svcs.iter().filter(|&&s| s == 0x1A).count() > 0, "no svcArbitrateLock");
    assert!(svcs.contains(&0x1C) && svcs.contains(&0x1D), "condvar wait and signal");
    assert!(kernel.scheduler.switches > 10, "switches: {}", kernel.scheduler.switches);
    assert_eq!(kernel.scheduler.total_threads_completed, 3); // two workers and the spinner
}

/// `tests/fixtures/nro/audren`: libnx `audrenInitialize` + audio driver play a 2400-sample tone (a square
/// wave of +/-8000, period 48 samples) on one voice routed to both channels of a stereo device sink.
#[test]
fn libnx_audren_fixture_plays_a_tone() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/audren/audren.nro");
    let nro = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (output, svcs, kernel) = boot_with_kernel_state(&nro, &[]);
    assert!(output.contains("ounce-audren played=0x00000960\n"), "output: {output:?}\nSVCs: {svcs:x?}");
    assert!(output.ends_with("ounce-audren result=0x00000EEE\n"), "output: {output:?}");
    // The mixed 48 kHz stereo output holds the tone, identical on both sides, then silence.
    let pcm = &kernel.audren.output_ring_buffer;
    let frames: Vec<(i16, i16)> = pcm.chunks_exact(2).map(|f| (f[0], f[1])).collect();
    let start = frames.iter().position(|f| f.0 != 0).expect("some audio was mixed");
    let tone = &frames[start..start + 2400];
    for (i, &(l, r)) in tone.iter().enumerate() {
        let expected = if (i / 24) % 2 == 1 { -8000 } else { 8000 };
        assert_eq!((l, r), (expected, expected), "frame {i}");
    }
    assert!(frames[start + 2400..].iter().all(|&f| f == (0, 0)));
}

/// The audren fixture's "long" mode: a looping wave buffer plays for 400 frames (2 seconds of audio).
#[test]
fn libnx_audren_fixture_loops_for_two_seconds() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/nro/audren/audren.nro");
    let nro = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (output, _, kernel) = boot_with_kernel_state(&nro, &["long"]);
    assert!(output.ends_with("ounce-audren result=0x00000EEE\n"), "output: {output:?}");
    // 400 frames of 240 stereo samples were mixed, with the tone all the way through.
    let pcm = &kernel.audren.output_ring_buffer;
    assert!(pcm.len() >= 400 * 240 * 2, "{} samples", pcm.len());
    assert_eq!(pcm.iter().filter(|&&s| s.abs() == 8000).count() / 2, 400 * 240);
}
