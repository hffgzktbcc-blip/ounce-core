//! Boots a real devkitA64 + libnx homebrew NRO (`tests/fixtures/nro/hello`) through its own `crt0`.
//!
//! The kernel's SVC table does not follow Horizon numbering for every call (for example `0x29`), so
//! this test services the few Horizon SVCs libnx startup makes with a small shim of its own and
//! records every SVC it sees. The fixture returns `0xEEE` when its checks pass, `0xBAD0000 | bits` otherwise.

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

/// Blocked on the CPU, not the loader: libnx's `memset`, `strlen`, `strcmp` and `__nx_dynamic` use
/// AdvSIMD `DUP`/`CMEQ`/`UMINP`/`ADDP`/`AND`/`SHRN`/`LD1`/`MOV` (vector) and scalar `REV`, `CLZ`, `RBIT`,
/// `UMULH`, `LDUR`, `STUR`, which the interpreter does not decode yet. Run with `cargo test -- --ignored`.
#[test]
#[ignore = "interpreter lacks AdvSIMD and a few scalar instructions libnx startup uses"]
fn libnx_nro_runs_to_main_return() {
    let (outcome, shim) = boot(&fixture(), "sdmc:/hello.nro fixture");
    match outcome {
        Outcome::Returned(0xEEE) => {}
        other => panic!(
            "fixture did not return 0xEEE: {other:x?}\nSVCs: {:x?}\ndebug output: {}",
            shim.svcs, shim.debug_output
        ),
    }
}

