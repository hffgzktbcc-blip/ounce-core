//! Bounded fuzz test for the SVC dispatcher. A guest controls the SVC number and every argument
//! register, so any sequence of calls must produce a result code or an `SvcResult`, never a panic,
//! a hang or an allocation sized by a register. See `loader::fuzz_tests` for the harness
//! (`OUNCE_FUZZ_CASES` sets the number of cases).

use super::svc::HorizonKernel;
use crate::cpu::Cpu;
use crate::loader::fuzz_tests::{cases, Rng};
use crate::mmu::{Memory, Permission};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

const RW: u64 = 0x9000;
const RO: u64 = 0xD000;
const CASE_BUDGET: Duration = Duration::from_secs(5);

/// A small address-space image: two RW pages, one RO page, then whatever earlier calls map.
fn memory(rng: &mut Rng) -> Memory {
    let mut mmu = Memory::new();
    mmu.map_range(RW, 0x2000, Permission::READ_WRITE);
    mmu.map_range(RO, 0x1000, Permission::READ_ONLY);
    let noise: Vec<u8> = (0..0x2000).map(|_| rng.next() as u8).collect();
    mmu.write_bytes(RW, &noise).unwrap();
    mmu
}

/// Mostly boundary values and pointers into the mapped pages, with handles the kernel just returned.
fn argument(rng: &mut Rng, handles: &[u64]) -> u64 {
    match rng.below(8) {
        0 | 1 if !handles.is_empty() => handles[rng.below(handles.len() as u64) as usize],
        2 => RW + rng.below(0x2000),
        3 => RO + rng.below(0x1000),
        4 => 0xFFFF_8000 + rng.below(4), // pseudo-handles
        5 => rng.below(0x40) * 0x1000,
        _ => rng.interesting_u64(),
    }
}

/// SVC numbers that exist, plus a few that do not.
fn svc_id(rng: &mut Rng) -> u16 {
    if rng.below(8) == 0 { rng.next() as u16 } else { rng.below(0x80) as u16 }
}

fn run_case(seed: u64, case: u64) {
    let mut rng = Rng::new(seed ^ case.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut mmu = memory(&mut rng);
    let mut cpu = Cpu::new();
    let mut kernel = HorizonKernel::new();
    // Guests may ask to sleep for minutes; idle time is skipped, not slept, so the budget measures work.
    kernel.scheduler.set_deterministic();
    let mut handles: Vec<u64> = Vec::new();
    for _ in 0..1 + rng.below(24) {
        for r in 0..8u8 {
            let v = argument(&mut rng, &handles);
            cpu.regs.write_x(r, v);
        }
        let id = svc_id(&mut rng);
        if id == 0x01 {
            // svcSetHeapSize may legitimately map 4 GiB; keep the fuzzer's own memory use small.
            cpu.regs.write_x(1, cpu.regs.read_x(1) & 0x3FF_FFFF);
        }
        if std::env::var_os("OUNCE_FUZZ_TRACE").is_some() {
            let x: Vec<u64> = (0..8).map(|r| cpu.regs.read_x(r)).collect();
            eprintln!("svc {id:#x} x0..x7 = {x:x?}");
        }
        let started = Instant::now();
        let _ = kernel.dispatch_svc(id, &mut cpu, &mut mmu);
        if started.elapsed() > Duration::from_millis(200) && std::env::var_os("OUNCE_FUZZ_TRACE").is_some() {
            eprintln!("  ^ slow: {:?}", started.elapsed());
        }
        // Outputs land in X1/X2 or in the scratch page; offer both as handles to later calls.
        handles.push(cpu.regs.read_x(1) & 0xFFFF_FFFF);
        if let Ok(h) = mmu.read_u32(RW + rng.below(0x100) * 4) {
            handles.push(h as u64);
        }
    }
}

#[test]
fn fuzz_svc_dispatch() {
    let seed = 0x5356_4330;
    for case in 0..cases() {
        let start = Instant::now();
        if catch_unwind(AssertUnwindSafe(|| run_case(seed, case))).is_err() {
            panic!("svc: case {case} panicked (fuzz seed {seed:#x})");
        }
        let elapsed = start.elapsed();
        assert!(elapsed < CASE_BUDGET, "svc: case {case} took {elapsed:?} (fuzz seed {seed:#x})");
    }
}
