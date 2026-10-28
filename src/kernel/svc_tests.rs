//! Per-SVC ABI tests for the calls libnx makes (registers in, result and outputs out), checked against the
//! public libnx `svc.h`/`result.h` and Atmosphère's kernel for semantics. Thread tests drive the scheduler at
//! SVC level: after svcStartThread the new thread is the running one, and its SVCs are dispatched directly.

use super::scheduler::{MAIN_THREAD_KEY, RESULT_CANCELLED, RESULT_INVALID_HANDLE, RESULT_INVALID_STATE, RESULT_TIMED_OUT};
use super::svc::*;
use crate::cpu::Cpu;
use crate::mmu::{Memory, Permission};

const SCRATCH: u64 = 0x9000;
const CURRENT_PROCESS: u64 = 0xFFFF_8001;

fn setup() -> (Cpu, Memory, HorizonKernel) {
    let mut mmu = Memory::new();
    mmu.map_range(SCRATCH, 0x1000, Permission::READ_WRITE);
    mmu.map_range(SCRATCH + 0x1000, 0x1000, Permission::READ_WRITE);
    mmu.map_range(SCRATCH + 0x4000, 0x1000, Permission::READ_ONLY);
    (Cpu::new(), mmu, HorizonKernel::new())
}

/// Puts `args` in X0.. and dispatches `id`.
fn call(k: &mut HorizonKernel, cpu: &mut Cpu, mmu: &mut Memory, id: u16, args: &[u64]) -> SvcResult {
    for (i, &a) in args.iter().enumerate() {
        cpu.regs.write_x(i as u8, a);
    }
    k.dispatch_svc(id, cpu, mmu)
}

/// svcCreateThread + svcStartThread for a priority-44 thread (the main thread's priority); returns its handle.
/// The new thread is running afterwards.
fn spawn(k: &mut HorizonKernel, cpu: &mut Cpu, mmu: &mut Memory) -> u32 {
    call(k, cpu, mmu, 0x08, &[0, 0x5000, 0, 0x7000, 44, (-2i32) as u32 as u64]);
    assert_eq!(cpu.regs.read_x(0), 0);
    let handle = cpu.regs.read_x(1) as u32;
    call(k, cpu, mmu, 0x09, &[handle as u64]);
    assert_eq!(k.scheduler.current, handle);
    handle
}

/// svcWaitSynchronization on one handle, forever.
fn wait_one(k: &mut HorizonKernel, cpu: &mut Cpu, mmu: &mut Memory, handle: u32, timeout: i64) -> SvcResult {
    mmu.write_u32(SCRATCH, handle).unwrap();
    call(k, cpu, mmu, 0x18, &[0, SCRATCH, 1, timeout as u64])
}

#[test]
fn set_heap_size_steps_keep_contents_and_shrink() {
    let (mut cpu, mut mmu, mut k) = setup();
    call(&mut k, &mut cpu, &mut mmu, 0x01, &[0, 0x1000]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_SIZE);
    call(&mut k, &mut cpu, &mut mmu, 0x01, &[0, HEAP_REGION_SIZE + HEAP_SIZE_ALIGN]);
    assert_eq!(cpu.regs.read_x(0), RESULT_OUT_OF_MEMORY);

    call(&mut k, &mut cpu, &mut mmu, 0x01, &[0, 0x40_0000]);
    let base = cpu.regs.read_x(1);
    assert_eq!((cpu.regs.read_x(0), base), (0, k.heap_base));
    mmu.write_u64(base + 0x10, 0x1234_5678).unwrap();

    // Growing keeps what is there.
    call(&mut k, &mut cpu, &mut mmu, 0x01, &[0, 0x80_0000]);
    assert_eq!(mmu.read_u64(base + 0x10).unwrap(), 0x1234_5678);
    assert!(mmu.is_mapped(base + 0x7F_F000));

    // svcGetInfo(UsedMemorySize) follows the heap; shrinking unmaps the tail.
    call(&mut k, &mut cpu, &mut mmu, 0x29, &[0, 7, CURRENT_PROCESS, 0]);
    assert_eq!(cpu.regs.read_x(1), 0x80_0000 + 0x10_0000);
    call(&mut k, &mut cpu, &mut mmu, 0x01, &[0, HEAP_SIZE_ALIGN]);
    assert!(mmu.is_mapped(base + 0x1F_F000) && !mmu.is_mapped(base + HEAP_SIZE_ALIGN));
    assert_eq!(mmu.read_u64(base + 0x10).unwrap(), 0x1234_5678);
}

#[test]
fn get_info_answers_every_type_libnx_asks_for() {
    let (mut cpu, mut mmu, mut k) = setup();
    // init.c, virtmem.c, newlib.c, random.c, detect.h.
    for (ty, sub) in [(0, 0), (2, 0), (3, 0), (4, 0), (5, 0), (6, 0), (7, 0), (8, 0), (11, 0), (11, 3), (12, 0), (13, 0), (14, 0), (15, 0), (16, 0), (17, 0), (18, 0), (21, 0), (22, 0), (23, 0), (28, 0)] {
        call(&mut k, &mut cpu, &mut mmu, 0x29, &[0, ty, CURRENT_PROCESS, sub]);
        assert_eq!(cpu.regs.read_x(0), 0, "info type {ty}/{sub}");
    }
    // Regions do not overlap, and the heap region holds the heap.
    let info = |t| k.info(t, 0).unwrap();
    let (alias, heap, stack) = ((info(2), info(3)), (info(4), info(5)), (info(14), info(15)));
    let overlap = |a: (u64, u64), b: (u64, u64)| a.0 < b.0 + b.1 && b.0 < a.0 + a.1;
    assert!(!overlap(alias, heap) && !overlap(alias, stack) && !overlap(heap, stack));
    assert_eq!(k.info(28, 0), Some(0), "no alias extra size, so virtmem keeps the whole alias region");
    // Mesosphere's meta type (detect.h) and RandomEntropy past sub-type 3 are not provided.
    for (ty, sub) in [(65000, 0), (11, 4), (19, 0)] {
        call(&mut k, &mut cpu, &mut mmu, 0x29, &[0, ty, 0, sub]);
        assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_ENUM_VALUE, "info type {ty}/{sub}");
    }
}

#[test]
fn map_memory_and_physical_memory() {
    let (mut cpu, mut mmu, mut k) = setup();
    // svcMapMemory moves the pages (libnx threadCreate mirrors a stack this way), svcUnmapMemory moves them back.
    mmu.map_range(0x20_0000, 0x2000, Permission::READ_WRITE);
    mmu.write_u32(0x20_1000, 0xABCD).unwrap();
    call(&mut k, &mut cpu, &mut mmu, 0x04, &[0x40_0000, 0x20_0000, 0x2000]);
    assert_eq!(cpu.regs.read_x(0), 0);
    assert_eq!(mmu.read_u32(0x40_1000).unwrap(), 0xABCD);
    assert!(!mmu.is_mapped(0x20_0000));
    call(&mut k, &mut cpu, &mut mmu, 0x05, &[0x40_0000, 0x20_0000, 0x2000]);
    assert_eq!(mmu.read_u32(0x20_1000).unwrap(), 0xABCD);
    assert!(!mmu.is_mapped(0x40_0000));

    // svcMapPhysicalMemory / svcUnmapPhysicalMemory back and release alias-region pages.
    call(&mut k, &mut cpu, &mut mmu, 0x2C, &[0x2_0000_0000, 0x4000]);
    assert!(cpu.regs.read_x(0) == 0 && mmu.is_mapped(0x2_0000_3000));
    call(&mut k, &mut cpu, &mut mmu, 0x2D, &[0x2_0000_0000, 0x4000]);
    assert!(cpu.regs.read_x(0) == 0 && !mmu.is_mapped(0x2_0000_0000));
}

#[test]
fn shared_and_transfer_memory() {
    let (mut cpu, mut mmu, mut k) = setup();
    // shmemCreate: svcCreateSharedMemory, then map / unmap it.
    call(&mut k, &mut cpu, &mut mmu, 0x50, &[0, 0x123, 3, 1]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_SIZE);
    call(&mut k, &mut cpu, &mut mmu, 0x50, &[0, 0x2000, 3, 1]);
    assert_eq!(cpu.regs.read_x(0), 0);
    let shm = cpu.regs.read_x(1);
    call(&mut k, &mut cpu, &mut mmu, 0x13, &[shm, 0x3_0000_0000, 0x2000, 3]);
    assert!(cpu.regs.read_x(0) == 0 && mmu.is_mapped(0x3_0000_1000));
    call(&mut k, &mut cpu, &mut mmu, 0x14, &[shm, 0x3_0000_0000, 0x2000]);
    assert!(cpu.regs.read_x(0) == 0 && !mmu.is_mapped(0x3_0000_0000));

    // tmemCreate / tmemMap / tmemUnmap.
    call(&mut k, &mut cpu, &mut mmu, 0x15, &[0, 0x20_0000, 0x1000, 0]);
    assert_eq!(cpu.regs.read_x(0), 0);
    let tmem = cpu.regs.read_x(1);
    call(&mut k, &mut cpu, &mut mmu, 0x51, &[tmem, 0x3_1000_0000, 0x1000, 3]);
    assert!(cpu.regs.read_x(0) == 0 && mmu.is_mapped(0x3_1000_0000));
    call(&mut k, &mut cpu, &mut mmu, 0x52, &[tmem, 0x3_1000_0000, 0x1000]);
    assert!(cpu.regs.read_x(0) == 0 && !mmu.is_mapped(0x3_1000_0000));
    // A shared memory handle is not transfer memory.
    call(&mut k, &mut cpu, &mut mmu, 0x51, &[shm, 0x3_1000_0000, 0x1000, 3]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_HANDLE);
}

#[test]
fn process_code_memory_moves_jit_pages() {
    let (mut cpu, mut mmu, mut k) = setup();
    // jit.c (JitType_SetProcessMemoryPermission): code written at src appears at dst, and moves back to src.
    let (src, dst) = (0x1_0000_0000u64, 0x8800_0000u64);
    mmu.map_range(src, 0x2000, Permission::READ_WRITE);
    mmu.write_u32(src + 0x1000, 0xD65F_03C0).unwrap(); // ret
    call(&mut k, &mut cpu, &mut mmu, 0x77, &[CURRENT_PROCESS, dst, src, 0x2000]);
    assert_eq!(cpu.regs.read_x(0), 0);
    assert_eq!(mmu.read_u32(dst + 0x1000).unwrap(), 0xD65F_03C0);
    call(&mut k, &mut cpu, &mut mmu, 0x73, &[CURRENT_PROCESS, dst, 0x2000, 5]);
    assert_eq!(cpu.regs.read_x(0), 0);
    call(&mut k, &mut cpu, &mut mmu, 0x78, &[CURRENT_PROCESS, dst, src, 0x2000]);
    assert_eq!(cpu.regs.read_x(0), 0);
    assert_eq!(mmu.read_u32(src + 0x1000).unwrap(), 0xD65F_03C0);
    assert!(!mmu.is_mapped(dst));

    call(&mut k, &mut cpu, &mut mmu, 0x77, &[CURRENT_PROCESS, dst + 4, src, 0x2000]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_ADDRESS);
    call(&mut k, &mut cpu, &mut mmu, 0x77, &[CURRENT_PROCESS, dst, 0x5_0000_0000, 0x1000]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_MEMORY_STATE);
}

#[test]
fn events_signal_clear_reset_and_bad_handles() {
    let (mut cpu, mut mmu, mut k) = setup();
    call(&mut k, &mut cpu, &mut mmu, 0x45, &[]);
    assert_eq!(cpu.regs.read_x(0), 0);
    let (write, read) = (cpu.regs.read_x(1) as u32, cpu.regs.read_x(2) as u32);

    wait_one(&mut k, &mut cpu, &mut mmu, read, 0);
    assert_eq!(cpu.regs.read_x(0), RESULT_TIMED_OUT);
    call(&mut k, &mut cpu, &mut mmu, 0x11, &[write as u64]);
    assert_eq!(cpu.regs.read_x(0), 0);
    wait_one(&mut k, &mut cpu, &mut mmu, read, -1);
    assert_eq!((cpu.regs.read_x(0), cpu.regs.read_x(1)), (0, 0));
    // Manual-clear: still signaled until svcClearEvent / svcResetSignal.
    wait_one(&mut k, &mut cpu, &mut mmu, read, 0);
    assert_eq!(cpu.regs.read_x(0), 0);
    call(&mut k, &mut cpu, &mut mmu, 0x12, &[write as u64]);
    wait_one(&mut k, &mut cpu, &mut mmu, read, 0);
    assert_eq!(cpu.regs.read_x(0), RESULT_TIMED_OUT);
    call(&mut k, &mut cpu, &mut mmu, 0x11, &[write as u64]);
    call(&mut k, &mut cpu, &mut mmu, 0x17, &[read as u64]);
    assert_eq!(cpu.regs.read_x(0), 0);
    wait_one(&mut k, &mut cpu, &mut mmu, read, 0);
    assert_eq!(cpu.regs.read_x(0), RESULT_TIMED_OUT);

    for id in [0x11, 0x12, 0x17] {
        call(&mut k, &mut cpu, &mut mmu, id, &[0xDEAD]);
        assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_HANDLE, "svc 0x{id:X}");
    }
    call(&mut k, &mut cpu, &mut mmu, 0x16, &[read as u64]);
    assert_eq!(cpu.regs.read_x(0), 0);
}

#[test]
fn sync_request_with_user_buffer_uses_the_supplied_hipc_area() {
    let (mut cpu, mut mmu, mut k) = setup();
    let user_buffer = SCRATCH + 0x1000;
    cpu.regs.tpidrro_el0 = SCRATCH;

    // libnx svc.h: argument registers are (usrBuffer, size, session); the kernel accepts
    // any nonzero page-multiple size, not only one page.
    mmu.write_bytes(SCRATCH, b"fake:\0").unwrap();
    call(&mut k, &mut cpu, &mut mmu, 0x1F, &[0, SCRATCH]);
    assert_eq!(cpu.regs.read_x(0), 0);
    let session = cpu.regs.read_x(1);

    // A minimal HIPC request with an unknown CMIF command gets a real SFCO response.
    mmu.write_u32(user_buffer, 4).unwrap(); // HIPC request
    mmu.write_u32(user_buffer + 4, 4).unwrap(); // four CMIF words
    mmu.write_u32(user_buffer + 0x10, crate::kernel::ipc::CMIF_IN_MAGIC).unwrap();
    mmu.write_u32(user_buffer + 0x14, 0).unwrap();
    mmu.write_u32(user_buffer + 0x18, 0xDEAD).unwrap();
    mmu.write_u32(SCRATCH, 0xA5A5_A5A5).unwrap(); // TLS must not become the request area.

    let requests = k.ipc.total_ipc_requests;
    call(&mut k, &mut cpu, &mut mmu, 0x22, &[user_buffer + 1, 0x1000, session]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_ADDRESS);
    call(&mut k, &mut cpu, &mut mmu, 0x22, &[user_buffer, 0, session]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_SIZE);
    call(&mut k, &mut cpu, &mut mmu, 0x22, &[user_buffer, 0x800, session]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_SIZE);
    call(&mut k, &mut cpu, &mut mmu, 0x22, &[u64::MAX & !0xFFF, 0x2000, session]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_MEMORY_STATE);
    call(&mut k, &mut cpu, &mut mmu, 0x22, &[0xC000, 0x1000, session]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_MEMORY_STATE);
    call(&mut k, &mut cpu, &mut mmu, 0x22, &[SCRATCH + 0x4000, 0x1000, session]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_MEMORY_STATE, "read-only buffer");

    // The whole requested range must be mapped and writable, including every crossed page.
    call(&mut k, &mut cpu, &mut mmu, 0x22, &[user_buffer, 0x2000, session]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_MEMORY_STATE, "second page is unmapped");
    assert_eq!(k.ipc.total_ipc_requests, requests, "invalid buffers must not dispatch IPC");
    mmu.map_range(user_buffer + 0x1000, 0x1000, Permission::READ_WRITE);

    call(&mut k, &mut cpu, &mut mmu, 0x22, &[user_buffer, 0x2000, session]);
    assert_eq!(cpu.regs.read_x(0), 0);
    assert_eq!(mmu.read_u32(user_buffer + 0x10).unwrap(), crate::kernel::ipc::CMIF_OUT_MAGIC);
    assert_eq!(mmu.read_u32(user_buffer + 0x18).unwrap(), crate::kernel::ipc::RESULT_UNKNOWN_COMMAND);
    assert_eq!(mmu.read_u32(SCRATCH).unwrap(), 0xA5A5_A5A5);

    call(&mut k, &mut cpu, &mut mmu, 0x22, &[user_buffer, 0x1000, 0xDEAD]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_HANDLE, "invalid session handle result");
}

#[test]
fn cancel_synchronization_wakes_or_arms() {
    let (mut cpu, mut mmu, mut k) = setup();
    call(&mut k, &mut cpu, &mut mmu, 0x45, &[]);
    let event = cpu.regs.read_x(2) as u32;

    // Not waiting: the thread's next wait is cancelled once.
    call(&mut k, &mut cpu, &mut mmu, 0x19, &[0xFFFF_8000]);
    assert_eq!(cpu.regs.read_x(0), 0);
    wait_one(&mut k, &mut cpu, &mut mmu, event, -1);
    assert_eq!(cpu.regs.read_x(0), RESULT_CANCELLED);
    wait_one(&mut k, &mut cpu, &mut mmu, event, 0);
    assert_eq!(cpu.regs.read_x(0), RESULT_TIMED_OUT);

    // Waiting: the waiter leaves svcWaitSynchronization with Cancelled.
    let worker = spawn(&mut k, &mut cpu, &mut mmu);
    wait_one(&mut k, &mut cpu, &mut mmu, event, -1);
    assert_eq!(k.scheduler.current, MAIN_THREAD_KEY);
    call(&mut k, &mut cpu, &mut mmu, 0x19, &[worker as u64]);
    assert_eq!(cpu.regs.read_x(0), 0);
    call(&mut k, &mut cpu, &mut mmu, 0x0B, &[(-2i64) as u64]);
    assert_eq!(k.scheduler.current, worker);
    assert_eq!(cpu.regs.read_x(0), RESULT_CANCELLED);

    call(&mut k, &mut cpu, &mut mmu, 0x19, &[0xDEAD]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_HANDLE);
}

#[test]
fn wait_for_address_checks_value_alignment_and_type() {
    let (mut cpu, mut mmu, mut k) = setup();
    let addr = SCRATCH + 0x100;
    mmu.write_u32(addr, 5).unwrap();
    let wait = |k: &mut HorizonKernel, cpu: &mut Cpu, mmu: &mut Memory, a: u64, ty: u64, v: u64, t: i64| {
        call(k, cpu, mmu, 0x34, &[a, ty, v, t as u64]);
        cpu.regs.read_x(0)
    };
    // WaitIfEqual / WaitIfLessThan with the test false: InvalidState at once.
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr, 2, 4, -1), RESULT_INVALID_STATE);
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr, 0, 5, -1), RESULT_INVALID_STATE);
    // Test true, timeout 0: TimedOut. DecrementAndWaitIfLessThan decrements first.
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr, 2, 5, 0), RESULT_TIMED_OUT);
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr, 1, 6, 0), RESULT_TIMED_OUT);
    assert_eq!(mmu.read_u32(addr).unwrap(), 4);
    // The comparison is signed.
    mmu.write_u32(addr, (-1i32) as u32).unwrap();
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr, 0, 0, 0), RESULT_TIMED_OUT);
    // WaitIfEqual64 compares the whole doubleword and needs 8-byte alignment.
    mmu.write_u64(addr, 0x1_0000_0002).unwrap();
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr, 3, 2, 0), RESULT_INVALID_STATE);
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr, 3, 0x1_0000_0002, 0), RESULT_TIMED_OUT);
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr + 4, 3, 0, 0), RESULT_INVALID_ADDRESS);

    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr + 2, 2, 0, 0), RESULT_INVALID_ADDRESS);
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr, 4, 0, 0), RESULT_INVALID_ENUM_VALUE);
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, 0x5_0000_0000, 2, 0, 0), RESULT_INVALID_MEMORY_STATE);
    // A timed wait that nothing signals times out; an endless one is a deadlock.
    mmu.write_u32(addr, 1).unwrap();
    assert_eq!(wait(&mut k, &mut cpu, &mut mmu, addr, 2, 1, 1_000_000), RESULT_TIMED_OUT);
    assert!(matches!(call(&mut k, &mut cpu, &mut mmu, 0x34, &[addr, 2, 1, u64::MAX]), SvcResult::Break { .. }));
}

#[test]
fn signal_to_address_updates_and_wakes_like_levent_needs() {
    let (mut cpu, mut mmu, mut k) = setup();
    let counter = SCRATCH + 0x200;
    let signal = |k: &mut HorizonKernel, cpu: &mut Cpu, mmu: &mut Memory, ty: u64, v: u64, n: i32| {
        call(k, cpu, mmu, 0x35, &[counter, ty, v, n as u32 as u64]);
        cpu.regs.read_x(0)
    };

    // No waiters: Signal leaves the word, IncrementIfEqual adds one, ModifyByWaitingCount adds one.
    mmu.write_u32(counter, 1).unwrap();
    assert_eq!(signal(&mut k, &mut cpu, &mut mmu, 0, 0, -1), 0);
    assert_eq!(mmu.read_u32(counter).unwrap(), 1);
    assert_eq!(signal(&mut k, &mut cpu, &mut mmu, 2, 1, 1), 0);
    assert_eq!(mmu.read_u32(counter).unwrap(), 2);
    assert_eq!(signal(&mut k, &mut cpu, &mut mmu, 1, 2, -1), 0);
    assert_eq!(mmu.read_u32(counter).unwrap(), 3);
    // A different value: InvalidState, word untouched (LEvent retries its RMW loop on that).
    assert_eq!(signal(&mut k, &mut cpu, &mut mmu, 1, 1, -1), RESULT_INVALID_STATE);
    assert_eq!(mmu.read_u32(counter).unwrap(), 3);
    assert_eq!(signal(&mut k, &mut cpu, &mut mmu, 9, 0, 0), RESULT_INVALID_ENUM_VALUE);

    // LEvent (non-autoclear): two waiters at "unsignaled with waiters" (1); IncrementIfEqual(1, -1) wakes both.
    mmu.write_u32(counter, 1).unwrap();
    let a = spawn(&mut k, &mut cpu, &mut mmu);
    call(&mut k, &mut cpu, &mut mmu, 0x34, &[counter, 2, 1, u64::MAX]);
    let b = spawn(&mut k, &mut cpu, &mut mmu);
    call(&mut k, &mut cpu, &mut mmu, 0x34, &[counter, 2, 1, u64::MAX]);
    assert_eq!(k.scheduler.current, MAIN_THREAD_KEY);
    assert_eq!(k.scheduler.address_waiters(counter), vec![a, b]);
    assert_eq!(signal(&mut k, &mut cpu, &mut mmu, 1, 1, -1), 0);
    assert_eq!(mmu.read_u32(counter).unwrap(), 2);
    assert!(k.scheduler.address_waiters(counter).is_empty());
    call(&mut k, &mut cpu, &mut mmu, 0x0B, &[(-2i64) as u64]);
    assert_eq!((k.scheduler.current, cpu.regs.read_x(0)), (a, 0));
    call(&mut k, &mut cpu, &mut mmu, 0x0A, &[]);
    assert_eq!((k.scheduler.current, cpu.regs.read_x(0)), (b, 0));
    call(&mut k, &mut cpu, &mut mmu, 0x0A, &[]);
    assert_eq!(k.scheduler.current, MAIN_THREAD_KEY);

    // ModifyByWaitingCountIfEqual with count 1: two waiters, one woken, value stays; then the last one,
    // value - 1.
    mmu.write_u32(counter, 1).unwrap();
    let c = spawn(&mut k, &mut cpu, &mut mmu);
    call(&mut k, &mut cpu, &mut mmu, 0x34, &[counter, 2, 1, u64::MAX]);
    let d = spawn(&mut k, &mut cpu, &mut mmu);
    call(&mut k, &mut cpu, &mut mmu, 0x34, &[counter, 2, 1, u64::MAX]);
    assert_eq!(signal(&mut k, &mut cpu, &mut mmu, 2, 1, 1), 0);
    assert_eq!(mmu.read_u32(counter).unwrap(), 1);
    assert_eq!(k.scheduler.address_waiters(counter), vec![d]);
    assert_eq!(signal(&mut k, &mut cpu, &mut mmu, 2, 1, 1), 0);
    assert_eq!(mmu.read_u32(counter).unwrap(), 0);
    assert!(k.scheduler.address_waiters(counter).is_empty());
    call(&mut k, &mut cpu, &mut mmu, 0x0B, &[(-2i64) as u64]);
    assert_eq!((k.scheduler.current, cpu.regs.read_x(0)), (c, 0));
}

#[test]
fn signal_to_address_wakes_better_priority_first() {
    let (mut cpu, mut mmu, mut k) = setup();
    let word = SCRATCH + 0x300;
    let low = spawn(&mut k, &mut cpu, &mut mmu);
    call(&mut k, &mut cpu, &mut mmu, 0x34, &[word, 2, 0, u64::MAX]);
    let high = spawn(&mut k, &mut cpu, &mut mmu);
    call(&mut k, &mut cpu, &mut mmu, 0x34, &[word, 2, 0, u64::MAX]);
    call(&mut k, &mut cpu, &mut mmu, 0x0D, &[high as u64, 30]);
    assert_eq!(k.scheduler.address_waiters(word), vec![high, low]);
    call(&mut k, &mut cpu, &mut mmu, 0x35, &[word, 0, 0, 1]);
    // The woken thread outranks the caller, so it runs at once.
    assert_eq!(k.scheduler.current, high);
    assert_eq!(k.scheduler.address_waiters(word), vec![low]);
}

#[test]
fn thread_activity_pauses_and_context_reads_registers() {
    let (mut cpu, mut mmu, mut k) = setup();
    call(&mut k, &mut cpu, &mut mmu, 0x45, &[]);
    let (write, event) = (cpu.regs.read_x(1), cpu.regs.read_x(2) as u32);
    let worker = spawn(&mut k, &mut cpu, &mut mmu);
    cpu.regs.write_x(7, 0x7777);
    cpu.regs.write_x(30, 0x3030);
    cpu.regs.tpidr_el0 = 0x7171;
    cpu.regs.v[1] = 0x1111_2222_3333_4444_5555_6666_7777_8888;
    wait_one(&mut k, &mut cpu, &mut mmu, event, -1);
    assert_eq!(k.scheduler.current, MAIN_THREAD_KEY);

    // Reading a running thread's context needs it paused; the caller cannot pause itself.
    call(&mut k, &mut cpu, &mut mmu, 0x33, &[SCRATCH, worker as u64]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_STATE);
    call(&mut k, &mut cpu, &mut mmu, 0x32, &[0xFFFF_8000, 1]);
    assert_eq!(cpu.regs.read_x(0), RESULT_BUSY);
    call(&mut k, &mut cpu, &mut mmu, 0x32, &[worker as u64, 2]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_ENUM_VALUE);
    call(&mut k, &mut cpu, &mut mmu, 0x32, &[worker as u64, 0]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_STATE, "already runnable");

    call(&mut k, &mut cpu, &mut mmu, 0x32, &[worker as u64, 1]);
    assert_eq!(cpu.regs.read_x(0), 0);
    call(&mut k, &mut cpu, &mut mmu, 0x32, &[worker as u64, 1]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_STATE, "already paused");
    call(&mut k, &mut cpu, &mut mmu, 0x33, &[SCRATCH, worker as u64]);
    assert_eq!(cpu.regs.read_x(0), 0);
    assert_eq!(mmu.read_u64(SCRATCH + 7 * 8).unwrap(), 0x7777);
    assert_eq!(mmu.read_u64(SCRATCH + 0xF0).unwrap(), 0x3030, "lr");
    assert_eq!(mmu.read_u64(SCRATCH + 0xF8).unwrap(), 0x7000, "sp");
    assert_eq!(mmu.read_u64(SCRATCH + 0x110 + 16).unwrap(), 0x5555_6666_7777_8888);
    assert_eq!(mmu.read_u64(SCRATCH + 0x318).unwrap(), 0x7171, "tpidr");

    // Woken while paused, the worker does not run until resumed.
    call(&mut k, &mut cpu, &mut mmu, 0x11, &[write]);
    call(&mut k, &mut cpu, &mut mmu, 0x0B, &[(-2i64) as u64]);
    assert_eq!(k.scheduler.current, MAIN_THREAD_KEY);
    call(&mut k, &mut cpu, &mut mmu, 0x32, &[worker as u64, 0]);
    assert_eq!(cpu.regs.read_x(0), 0);
    call(&mut k, &mut cpu, &mut mmu, 0x0B, &[(-2i64) as u64]);
    assert_eq!((k.scheduler.current, cpu.regs.read_x(0), cpu.regs.read_x(7)), (worker, 0, 0x7777));

    call(&mut k, &mut cpu, &mut mmu, 0x32, &[0xDEAD, 1]);
    assert_eq!(cpu.regs.read_x(0), RESULT_INVALID_HANDLE);
}

#[test]
fn identity_debug_and_process_svcs() {
    let (mut cpu, mut mmu, mut k) = setup();
    call(&mut k, &mut cpu, &mut mmu, 0x24, &[0, CURRENT_PROCESS]);
    assert_eq!(cpu.regs.read_x(0), 0);
    call(&mut k, &mut cpu, &mut mmu, 0x25, &[0, 0xFFFF_8000]);
    assert_eq!((cpu.regs.read_x(0), cpu.regs.read_x(1)), (0, 1));
    call(&mut k, &mut cpu, &mut mmu, 0x10, &[]);
    assert_eq!(cpu.regs.read_x(0), 0);

    // console_debug.c: svcOutputDebugString(str, len).
    mmu.write_bytes(SCRATCH, b"svc ok\n").unwrap();
    call(&mut k, &mut cpu, &mut mmu, 0x27, &[SCRATCH, 7]);
    assert_eq!(cpu.regs.read_x(0), 0);

    // sm.c: svcConnectToNamedPort("sm:").
    mmu.write_bytes(SCRATCH, b"sm:\0").unwrap();
    call(&mut k, &mut cpu, &mut mmu, 0x1F, &[0, SCRATCH]);
    assert!(cpu.regs.read_x(0) == 0 && cpu.regs.read_x(1) != 0);

    assert_eq!(
        call(&mut k, &mut cpu, &mut mmu, 0x26, &[1, 2, 3]),
        SvcResult::Break { reason: 1, info1: 2, info2: 3 }
    );
    assert_eq!(call(&mut k, &mut cpu, &mut mmu, 0x07, &[]), SvcResult::ExitProcess);
    // The last thread exiting ends the process.
    assert_eq!(call(&mut k, &mut cpu, &mut mmu, 0x0A, &[]), SvcResult::ExitProcess);
}

#[test]
fn unimplemented_svcs_are_reported_not_faked() {
    let (mut cpu, mut mmu, mut k) = setup();
    // Code memory (needs aliased pages), resource limits, light IPC, debug and process management.
    for id in [0x20, 0x23, 0x2E, 0x2F, 0x30, 0x31, 0x36, 0x37, 0x40, 0x42, 0x44, 0x4B, 0x4C, 0x6F, 0x72, 0x7F] {
        assert_eq!(call(&mut k, &mut cpu, &mut mmu, id, &[]), SvcResult::Unhandled(id), "svc 0x{id:X}");
    }
}
