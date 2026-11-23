use super::runner::{Options, Report, Stop, describe_regs, run};
use super::syscalls::Output;
use crate::cpu::CpuTier;

const HELLO: &[u8] = include_bytes!("../../tests/linux-elf/bin/hello.elf");
const EXITCODE: &[u8] = include_bytes!("../../tests/linux-elf/bin/exitcode.elf");
const ARGS: &[u8] = include_bytes!("../../tests/linux-elf/bin/args.elf");
const MEMORY: &[u8] = include_bytes!("../../tests/linux-elf/bin/memory.elf");
const BADSYS: &[u8] = include_bytes!("../../tests/linux-elf/bin/badsyscall.elf");
const BASIC_MATH: &[u8] = include_bytes!("../../tests/linux-elf/bin/basic_math.elf");

#[derive(Default)]
struct Capture {
    out: Vec<u8>,
    err: Vec<u8>,
}
impl Output for Capture {
    fn write(&mut self, fd: u64, data: &[u8]) {
        if fd == 2 { &mut self.err } else { &mut self.out }.extend_from_slice(data);
    }
}

fn go(elf: &[u8], args: &[&str], tier: CpuTier) -> (Report, String) {
    let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut cap = Capture::default();
    let opts = Options {
        tier,
        ..Options::default()
    };
    let r = run(elf, &argv, &opts, &mut cap, &mut |_| {});
    (r, String::from_utf8_lossy(&cap.out).into_owned())
}

const TIERS: [CpuTier; 3] = [CpuTier::Interpreter, CpuTier::Tier3, CpuTier::Tier4];

#[test]
fn hello_world() {
    for t in TIERS {
        let (r, out) = go(HELLO, &["hello"], t);
        assert_eq!(r.stop, Stop::Exited(0), "{r:?}");
        assert_eq!(out, "hello from aarch64 linux\n");
    }
}

#[test]
fn exit_code_propagates() {
    for t in TIERS {
        let (r, _) = go(EXITCODE, &["exitcode"], t);
        assert_eq!(r.stop, Stop::Exited(42));
        assert_eq!(r.exit_status(), 42);
    }
}

#[test]
fn basic_math_execution() {
    for t in TIERS {
        let (r, out) = go(BASIC_MATH, &["basic_math"], t);
        assert_eq!(r.stop, Stop::Exited(42), "{r:?}");
        assert_eq!(out, "calculating 15 + 27 = 42\n");
    }
}

#[test]
fn argv_and_auxv_reach_the_program() {
    for t in TIERS {
        let (r, out) = go(ARGS, &["args", "one", "two words"], t);
        assert_eq!(r.stop, Stop::Exited(3), "{r:?}");
        assert_eq!(out, "argc=3\nargv: args\nargv: one\nargv: two words\npagesz=4096\n");
    }
}

#[test]
fn brk_mmap_munmap_writev() {
    // heap bytes (i*7)&0xff over 12288 bytes; mmap bytes (i^0x55)&0xff over 65536 bytes.
    let heap: u64 = (0..3 * 4096u64).map(|i| (i * 7) & 0xff).sum();
    let map: u64 = (0..65536u64).map(|i| (i ^ 0x55) & 0xff).sum();
    for t in TIERS {
        let (r, out) = go(MEMORY, &["memory"], t);
        assert_eq!(r.stop, Stop::Exited(0), "{r:?}");
        assert_eq!(out, format!("heap_sum={heap}\nmmap_sum={map}\nheap_sum=\n"));
    }
}

#[test]
fn unknown_syscall_is_diagnosed_by_number() {
    for t in TIERS {
        let (r, out) = go(BADSYS, &["badsyscall"], t);
        assert_eq!(out, "before\n");
        let Stop::Failed { diagnosis, status } = r.stop else {
            panic!("expected failure: {r:?}")
        };
        assert_eq!(status, 70);
        assert!(diagnosis.contains("syscall 278 (getrandom)"), "{diagnosis}");
        assert!(diagnosis.contains("pc=0x"), "{diagnosis}");
    }
}

#[test]
fn undecodable_instruction_is_diagnosed_by_word() {
    // Hand-built ELF whose entry is the permanently-undefined word 0x00000000 (UDF #0).
    let elf = tiny_elf(&[0x0000_0000]);
    let (r, _) = go(&elf, &["udf"], CpuTier::Tier3);
    let Stop::Failed { diagnosis, status } = r.stop else {
        panic!("expected failure: {r:?}")
    };
    assert_eq!(status, 71);
    assert!(diagnosis.contains("instruction word 0x00000000"), "{diagnosis}");
}

#[test]
fn non_linux_svc_is_rejected() {
    // svc #0x7
    let elf = tiny_elf(&[0xD400_00E1]);
    let (r, _) = go(&elf, &["svc7"], CpuTier::Tier3);
    let Stop::Failed { diagnosis, .. } = r.stop else {
        panic!("{r:?}")
    };
    assert!(diagnosis.contains("SVC #0x7"), "{diagnosis}");
}

#[test]
fn openat_and_readlinkat_are_enoent() {
    // mov x8,#56; svc #0; mov x8,#93; neg x0 -> exit(-x0)  (exit status = ENOENT = 2)
    let elf = tiny_elf(&[0xD280_0708, 0xD400_0001, 0x4B00_03E0, 0xD280_0BA8, 0xD400_0001]);
    let (r, _) = go(&elf, &["openat"], CpuTier::Tier3);
    assert_eq!(r.stop, Stop::Exited(2));
}

#[test]
fn runaway_loop_hits_the_budget() {
    let elf = tiny_elf(&[0x1400_0000]); // b .
    let argv = vec!["spin".to_string()];
    let opts = Options {
        max_instructions: 10_000,
        ..Options::default()
    };
    let r = run(&elf, &argv, &opts, &mut Capture::default(), &mut |_| {});
    assert!(matches!(r.stop, Stop::Failed { status: 73, .. }), "{r:?}");
}

#[test]
fn loader_rejects_bad_inputs() {
    let reject = |bytes: &[u8], needle: &str| {
        let (r, _) = go(bytes, &["x"], CpuTier::Tier3);
        let Stop::Failed { diagnosis, status } = r.stop else {
            panic!("accepted")
        };
        assert_eq!(status, 72);
        assert!(diagnosis.contains(needle), "{diagnosis}");
    };
    reject(b"not an elf at all", "not an ELF");
    let mut dynamic = HELLO.to_vec();
    dynamic[16] = 3; // ET_DYN
    reject(&dynamic, "ET_DYN");
    let mut x86 = HELLO.to_vec();
    x86[18] = 62; // EM_X86_64
    reject(&x86, "not AArch64");
    let mut be = HELLO.to_vec();
    be[5] = 2;
    reject(&be, "big-endian");
    let mut c32 = HELLO.to_vec();
    c32[4] = 1;
    reject(&c32, "32-bit");
    // Truncated: program headers claim data past EOF.
    reject(&HELLO[..70], "malformed");
    // PT_INTERP in the first program header.
    let mut interp = HELLO.to_vec();
    let phoff = u64::from_le_bytes(interp[32..40].try_into().unwrap()) as usize;
    interp[phoff..phoff + 4].copy_from_slice(&3u32.to_le_bytes());
    reject(&interp, "PT_INTERP");
}

#[test]
fn loader_never_panics_on_corrupted_headers() {
    for i in 0..200usize {
        let mut b = HELLO.to_vec();
        let idx = (i * 7) % 176;
        b[idx] ^= 0xff ^ (i as u8);
        let (_r, _) = go(&b, &["fuzz"], CpuTier::Tier3);
    }
}

/// Minimal static ET_EXEC AArch64 ELF: one RWX PT_LOAD at 0x400000 holding `code`, entry at its start.
fn tiny_elf(code: &[u32]) -> Vec<u8> {
    let mut e = vec![0u8; 64 + 56];
    e[..4].copy_from_slice(b"\x7fELF");
    e[4] = 2;
    e[5] = 1;
    e[6] = 1;
    e[16..18].copy_from_slice(&2u16.to_le_bytes());
    e[18..20].copy_from_slice(&183u16.to_le_bytes());
    e[20..24].copy_from_slice(&1u32.to_le_bytes());
    let base = 0x40_0000u64;
    let code_off = 120u64;
    e[24..32].copy_from_slice(&(base + code_off).to_le_bytes());
    e[32..40].copy_from_slice(&64u64.to_le_bytes());
    e[52..54].copy_from_slice(&64u16.to_le_bytes());
    e[54..56].copy_from_slice(&56u16.to_le_bytes());
    e[56..58].copy_from_slice(&1u16.to_le_bytes());
    let size = code_off + code.len() as u64 * 4;
    let ph = 64;
    e[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes());
    e[ph + 4..ph + 8].copy_from_slice(&7u32.to_le_bytes());
    e[ph + 16..ph + 24].copy_from_slice(&base.to_le_bytes());
    e[ph + 24..ph + 32].copy_from_slice(&base.to_le_bytes());
    e[ph + 32..ph + 40].copy_from_slice(&size.to_le_bytes());
    e[ph + 40..ph + 48].copy_from_slice(&size.to_le_bytes());
    e[ph + 48..ph + 56].copy_from_slice(&4096u64.to_le_bytes());
    for w in code {
        e.extend_from_slice(&w.to_le_bytes());
    }
    e
}

#[test]
fn deterministic_clocks_replay_identically_across_engines() {
    let elf = include_bytes!("../../tests/linux-elf/bin/clock.elf");
    let expected = b"realtime=1700000000:0\nmonotonic=0:1000000\nraw=0:2000000\nboottime=0:3000000\n";
    for tier in TIERS {
        for _ in 0..2 {
            let mut cap = Capture::default();
            let opts = Options {
                tier,
                deterministic: true,
                ..Options::default()
            };
            let r = run(elf, &["clock".into()], &opts, &mut cap, &mut |_| {});
            assert_eq!(r.stop, Stop::Exited(0), "{r:?}");
            assert_eq!(cap.out, expected);
        }
    }
}

#[test]
fn deterministic_clock_errors_do_not_consume_time_and_seconds_roll_over() {
    use super::syscalls::{EFAULT, EINVAL, Outcome, Process, SYS_CLOCK_GETTIME};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new_deterministic(0x1000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x1000, Permission::READ_WRITE);
    let mut out = Capture::default();
    assert_eq!(
        proc.dispatch(SYS_CLOCK_GETTIME, [99, 0x1000, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EINVAL)
    );
    assert_eq!(
        proc.dispatch(SYS_CLOCK_GETTIME, [1, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EFAULT)
    );
    for i in 0..1002u64 {
        assert_eq!(
            proc.dispatch(SYS_CLOCK_GETTIME, [1, 0x1000, 0, 0, 0, 0], &mut mem, &mut out),
            Outcome::Return(0)
        );
        assert_eq!(mem.read_u64(0x1000).unwrap(), i / 1000);
        assert_eq!(mem.read_u64(0x1008).unwrap(), (i % 1000) * 1_000_000);
    }
}

#[test]
fn sched_getaffinity_reports_one_cpu_and_validates_like_linux() {
    use super::syscalls::{EFAULT, EINVAL, ESRCH, Outcome, PID, Process, SYS_SCHED_GETAFFINITY};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x1000, Permission::READ_WRITE);
    mem.write_bytes(0x1000, &[0xff; 16]).unwrap();
    let mut out = Capture::default();
    let mut call = |pid, len, mask, mem: &mut Memory| {
        proc.dispatch(SYS_SCHED_GETAFFINITY, [pid, len, mask, 0, 0, 0], mem, &mut out)
    };
    // musl's sysconf passes a 128-byte buffer; only the first 8 bytes are written.
    assert_eq!(call(0, 0x80, 0x1000, &mut mem), Outcome::Return(8));
    assert_eq!(mem.read_u64(0x1000).unwrap(), 1);
    assert_eq!(mem.read_u64(0x1008).unwrap(), u64::MAX);
    assert_eq!(call(PID, 8, 0x1000, &mut mem), Outcome::Return(8));
    assert_eq!(call(0, 4, 0x1000, &mut mem), Outcome::Return(-EINVAL));
    assert_eq!(call(0, 12, 0x1000, &mut mem), Outcome::Return(-EINVAL));
    assert_eq!(call(PID + 1, 8, 0x1000, &mut mem), Outcome::Return(-ESRCH));
    assert_eq!(call(0, 8, 0, &mut mem), Outcome::Return(-EFAULT));
}

#[test]
fn mremap_shrinks_in_place_and_grows_by_moving_with_contents() {
    use super::syscalls::{EINVAL, ENOMEM, Outcome, Process, SYS_MMAP, SYS_MREMAP};
    use crate::mmu::Memory;
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    let mut out = Capture::default();
    let Outcome::Return(a) = proc.dispatch(SYS_MMAP, [0, 0x4000, 3, 0x22, u64::MAX, 0], &mut mem, &mut out) else {
        panic!()
    };
    let a = a as u64;
    mem.write_u64(a, 0x1122_3344_5566_7788).unwrap();
    mem.write_u64(a + 0x3ff8, 42).unwrap();
    // without MAYMOVE growth is refused, bad flags are invalid
    assert_eq!(
        proc.dispatch(SYS_MREMAP, [a, 0x4000, 0x8000, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOMEM)
    );
    assert_eq!(
        proc.dispatch(SYS_MREMAP, [a, 0x4000, 0x8000, 2, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EINVAL)
    );
    // grow with MAYMOVE: contents preserved at the new address, old range unmapped
    let Outcome::Return(b) = proc.dispatch(SYS_MREMAP, [a, 0x4000, 0x8000, 1, 0, 0], &mut mem, &mut out) else {
        panic!()
    };
    let b = b as u64;
    assert_ne!(a, b);
    assert_eq!(mem.read_u64(b).unwrap(), 0x1122_3344_5566_7788);
    assert_eq!(mem.read_u64(b + 0x3ff8).unwrap(), 42);
    assert_eq!(mem.read_u64(b + 0x7ff8).unwrap(), 0);
    assert!(!mem.is_mapped(a));
    // shrink in place
    assert_eq!(
        proc.dispatch(SYS_MREMAP, [b, 0x8000, 0x1000, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(b as i64)
    );
    assert!(mem.is_mapped(b) && !mem.is_mapped(b + 0x1000));
}

#[test]
fn guarded_virtual_memory_and_linux_signal_return() {
    let elf = include_bytes!("../../tests/linux-elf/bin/guarded_memory.elf");
    for tier in TIERS {
        let (report, out) = go(elf, &["guarded_memory"], tier);
        assert_eq!(report.stop, Stop::Exited(0), "{tier:?}: {report:?}");
        assert_eq!(out, "guarded reservation, commit, fault, return, decommit: ok\n");
    }
}

#[test]
fn reservations_are_sparse_bounded_and_preserve_holes() {
    use super::syscalls::{EINVAL, ENOMEM, Outcome, Process, SYS_MMAP, SYS_MPROTECT, SYS_MUNMAP};
    use crate::mmu::Memory;
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    let mut out = Capture::default();
    let n = 0x10000100000;
    let Outcome::Return(base) = proc.dispatch(SYS_MMAP, [0, n, 0, 0x4022, u64::MAX, 0], &mut mem, &mut out) else {
        panic!()
    };
    assert!(base > 0);
    let base = base as u64;
    assert_eq!(mem.bucket_stats().total_pages, 0);
    assert_eq!(
        proc.dispatch(SYS_MPROTECT, [base, n, 3, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOMEM)
    );
    assert_eq!(
        proc.dispatch(SYS_MPROTECT, [base + 4096, 1, 3, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(mem.bucket_stats().total_pages, 1);
    assert_eq!(
        proc.dispatch(SYS_MUNMAP, [base + 8192, 4096, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(SYS_MPROTECT, [base + 4096, 8192, 3, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOMEM)
    );
    assert_eq!(mem.bucket_stats().total_pages, 1);
    assert_eq!(
        proc.dispatch(SYS_MMAP, [0, u64::MAX, 0, 0x4022, u64::MAX, 0], &mut mem, &mut out),
        Outcome::Return(-EINVAL)
    );
    assert_eq!(
        proc.dispatch(SYS_MUNMAP, [base, n, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(mem.bucket_stats().total_pages, 0);
}

#[test]
fn mmap_hints_do_not_overlap_reservations_and_fixed_maps_trim_them() {
    use super::syscalls::{ENOMEM, Outcome, Process, SYS_MMAP, SYS_MPROTECT, SYS_MUNMAP};
    use crate::mmu::Memory;
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    let mut out = Capture::default();
    let mut map = |hint, len, prot, flags, mem: &mut Memory| {
        let Outcome::Return(addr) = proc.dispatch(SYS_MMAP, [hint, len, prot, flags, u64::MAX, 0], mem, &mut out)
        else {
            panic!()
        };
        assert!(addr > 0);
        addr as u64
    };
    let a = map(0, 0x100000000, 0, 0x4022, &mut mem);
    let b = map(a + 4096, 4096, 3, 0x22, &mut mem);
    assert!(b >= a + 0x100000000);
    assert_eq!(map(a + 8192, 4096, 3, 0x32, &mut mem), a + 8192);
    assert_eq!(mem.bucket_stats().total_pages, 2);
    assert_eq!(
        proc.dispatch(SYS_MPROTECT, [a, 4096, 3, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(SYS_MUNMAP, [a + 4096, 4096, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(SYS_MPROTECT, [a, 12288, 3, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOMEM)
    );
}

// ---- ounce.run.v1 stop details (#473) ----

fn go_t3(elf: &[u8]) -> Report {
    let opts = Options {
        max_instructions: 10_000,
        ..Options::default()
    };
    run(elf, &["t".to_string()], &opts, &mut Capture::default(), &mut |_| {})
}

#[test]
fn clean_exit_reports_the_guest_status_separately_from_the_engine_exit_code() {
    for t in TIERS {
        let (r, _) = go(EXITCODE, &["exitcode"], t);
        let report = r.to_run_report(t);
        assert_eq!(
            (report.stop_kind, report.exit_code, report.guest_exit_status),
            ("clean_exit", 0, Some(42)),
            "{t:?}"
        );
        assert_eq!(
            r.detail.regs.map(|regs| regs.x0),
            Some(42),
            "x0 holds the exit status at the exit syscall"
        );
        assert!(report.message.is_none());
    }
}

#[test]
fn unimplemented_syscall_reports_its_number_as_svc_id() {
    let (r, _) = go(BADSYS, &["badsyscall"], CpuTier::Tier3);
    let report = r.to_run_report(CpuTier::Tier3);
    assert_eq!(
        (report.stop_kind, report.exit_code, report.svc_id),
        ("unhandled_svc", 14, Some(278))
    );
    assert!(report.message.unwrap().contains("getrandom"));
    assert_eq!(report.pc, r.detail.regs.map(|regs| regs.pc));
}

#[test]
fn non_linux_svc_reports_its_immediate_as_svc_id() {
    let r = go_t3(&tiny_elf(&[0xD400_00E1])); // svc #0x7
    assert_eq!((r.detail.stop_kind, r.detail.svc_id), ("unhandled_svc", Some(7)));
}

#[test]
fn undefined_instruction_reports_its_opcode() {
    let r = go_t3(&tiny_elf(&[0x0000_0000])); // UDF #0
    let report = r.to_run_report(CpuTier::Tier3);
    assert_eq!(
        (report.stop_kind, report.exit_code, report.opcode),
        ("unimplemented_insn", 10, Some(0))
    );
    assert_eq!(report.pc, Some(r.detail.regs.unwrap().pc));
}

#[test]
fn unmapped_load_reports_the_fault_address() {
    // mov x1, #0x10; ldr x0, [x1]
    let r = go_t3(&tiny_elf(&[0xD280_0201, 0xF940_0020]));
    let report = r.to_run_report(CpuTier::Tier3);
    assert_eq!(
        (report.stop_kind, report.exit_code, report.fault_addr),
        ("memory_fault", 11, Some(0x10)),
        "{r:?}"
    );
}

#[test]
fn budget_and_loader_stops_map_to_their_stop_kinds() {
    let spin = go_t3(&tiny_elf(&[0x1400_0000])); // b .
    assert_eq!(spin.to_run_report(CpuTier::Tier3).exit_code, 16);
    assert_eq!(spin.detail.stop_kind, "budget_exhausted");
    let bad = go_t3(b"not an elf at all");
    let report = bad.to_run_report(CpuTier::Tier3);
    assert_eq!(
        (report.stop_kind, report.exit_code, report.tier, report.silent_fallback),
        ("loader_error", 17, "tier3", false)
    );
    assert!(bad.detail.regs.is_none());
    assert_eq!(
        describe_regs(bad.detail.regs),
        "registers at stop: unavailable (the program did not start)"
    );
}

#[test]
fn dump_regs_line_has_pc_x0_d0_and_nzcv() {
    let (r, _) = go(EXITCODE, &["exitcode"], CpuTier::Tier3);
    let line = describe_regs(r.detail.regs);
    assert!(line.starts_with("registers at stop: pc=0x"), "{line}");
    assert!(line.contains(" x0=0x000000000000002a "), "{line}");
    assert!(line.contains(" d0=0x") && line.contains(" nzcv="), "{line}");
}

#[test]
fn identity_and_process_syscalls() {
    use super::syscalls::{
        EBADF, GID, Outcome, PID, Process, SYS_CLOSE, SYS_GETEGID, SYS_GETEUID, SYS_GETGID, SYS_GETPID, SYS_GETPPID,
        SYS_GETTID, SYS_GETUID, SYS_SCHED_YIELD, UID,
    };
    use crate::mmu::Memory;
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    let mut out = Capture::default();

    assert_eq!(
        proc.dispatch(SYS_GETPID, [0; 6], &mut mem, &mut out),
        Outcome::Return(PID as i64)
    );
    assert_eq!(
        proc.dispatch(SYS_GETTID, [0; 6], &mut mem, &mut out),
        Outcome::Return(PID as i64)
    );
    assert_eq!(
        proc.dispatch(SYS_GETPPID, [0; 6], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(SYS_GETUID, [0; 6], &mut mem, &mut out),
        Outcome::Return(UID as i64)
    );
    assert_eq!(
        proc.dispatch(SYS_GETEUID, [0; 6], &mut mem, &mut out),
        Outcome::Return(UID as i64)
    );
    assert_eq!(
        proc.dispatch(SYS_GETGID, [0; 6], &mut mem, &mut out),
        Outcome::Return(GID as i64)
    );
    assert_eq!(
        proc.dispatch(SYS_GETEGID, [0; 6], &mut mem, &mut out),
        Outcome::Return(GID as i64)
    );
    assert_eq!(
        proc.dispatch(SYS_SCHED_YIELD, [0; 6], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(SYS_CLOSE, [0, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(SYS_CLOSE, [1, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(SYS_CLOSE, [2, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(SYS_CLOSE, [3, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EBADF)
    );
}

#[test]
fn gettimeofday_writes_timeval_and_supports_deterministic_mode() {
    use super::syscalls::{EFAULT, Outcome, Process, SYS_GETTIMEOFDAY};
    use crate::mmu::Memory;
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    let mut out = Capture::default();

    // Null pointer returns 0 without writing
    assert_eq!(
        proc.dispatch(SYS_GETTIMEOFDAY, [0, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );

    // Unmapped pointer returns -EFAULT
    assert_eq!(
        proc.dispatch(SYS_GETTIMEOFDAY, [0x2000, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EFAULT)
    );

    // Mapped buffer receives seconds and microseconds
    mem.map_range(
        0x2000,
        4096,
        crate::mmu::Permission {
            read: true,
            write: true,
            execute: false,
        },
    );
    assert_eq!(
        proc.dispatch(SYS_GETTIMEOFDAY, [0x2000, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    let sec = mem.read_u64(0x2000).unwrap();
    let usec = mem.read_u64(0x2008).unwrap();
    assert!(sec > 0);
    assert!(usec < 1_000_000);

    // Deterministic mode produces reproducible progressive virtual time
    let mut det_proc = Process::new_deterministic(0x1000);
    assert_eq!(
        det_proc.dispatch(SYS_GETTIMEOFDAY, [0x2000, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    let sec1 = mem.read_u64(0x2000).unwrap();
    let usec1 = mem.read_u64(0x2008).unwrap();
    assert_eq!(sec1, 1_700_000_000);
    assert_eq!(usec1, 0);

    assert_eq!(
        det_proc.dispatch(SYS_GETTIMEOFDAY, [0x2000, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    let sec2 = mem.read_u64(0x2000).unwrap();
    let usec2 = mem.read_u64(0x2008).unwrap();
    assert_eq!(sec2, 1_700_000_000);
    assert_eq!(usec2, 1_000); // 1 ms tick = 1000 microseconds
}

/// `abort()` in musl is: block every signal, `tkill(tid, SIGABRT)` (glibc: `tgkill`), restore the mask. The kernel keeps the blocked
/// signal pending and, with the default action, ends the process when the mask is restored, which a shell reports as
/// status 134. Before this, `tkill` (130) and `tgkill` (131) stopped `run-elf` as an unimplemented syscall (exit 70).
/// libc-test and gcc.c-torture programs that call `abort()` showed it: qemu-aarch64 exits 134, ounce-core exited 70.
#[test]
fn tgkill_abort_sequence_ends_the_guest_with_status_134() {
    use super::syscalls::{ESRCH, Outcome, PID, Process, SYS_RT_SIGPROCMASK};
    use crate::mmu::{Memory, Permission};
    const DATA: u64 = 0x1000;
    const TGKILL: u64 = 131;
    const TKILL: u64 = 130;
    let mut proc = Process::new(0x10_0000);
    let mut mem = Memory::new();
    mem.map_range(DATA, 0x1000, Permission::READ_WRITE);
    let mut out = Capture::default();
    // sigprocmask(SIG_BLOCK, &all, &old, 8): block everything the guest may block
    mem.write_u64(DATA, u64::MAX).unwrap();
    assert_eq!(
        proc.dispatch(SYS_RT_SIGPROCMASK, [0, DATA, DATA + 8, 8, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(mem.read_u64(DATA + 8).unwrap(), 0, "the old mask was empty");
    // tgkill(pid, pid, SIGABRT) while blocked: accepted, nothing happens yet
    assert_eq!(
        proc.dispatch(TGKILL, [PID, PID, 6, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    // restoring the old mask (SIG_SETMASK) delivers it: default action, terminated by SIGABRT = status 134
    assert_eq!(
        proc.dispatch(SYS_RT_SIGPROCMASK, [2, DATA + 8, 0, 8, 0, 0], &mut mem, &mut out),
        Outcome::Exit(128 + 6)
    );

    // unblocked, the default action takes effect at once
    let mut proc = Process::new(0x10_0000);
    assert_eq!(
        proc.dispatch(TGKILL, [PID, PID, 6, 0, 0, 0], &mut mem, &mut out),
        Outcome::Exit(134)
    );
    let mut proc = Process::new(0x10_0000);
    assert_eq!(
        proc.dispatch(TGKILL, [PID, PID, 15, 0, 0, 0], &mut mem, &mut out),
        Outcome::Exit(143)
    );
    // musl's raise() uses tkill(tid, sig) rather than tgkill: the same effect, the same checks
    let mut proc = Process::new(0x10_0000);
    assert_eq!(
        proc.dispatch(TKILL, [PID, 6, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Exit(134)
    );
    assert_eq!(
        proc.dispatch(TKILL, [PID + 1, 6, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ESRCH)
    );
    assert_eq!(
        proc.dispatch(TKILL, [PID, 65, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-super::syscalls::EINVAL)
    );

    // signals that are ignored by default, signal 0 (an existence check) and a wrong target change nothing
    let mut proc = Process::new(0x10_0000);
    for sig in [17, 18, 23, 28, 0] {
        assert_eq!(
            proc.dispatch(TGKILL, [PID, PID, sig, 0, 0, 0], &mut mem, &mut out),
            Outcome::Return(0),
            "{sig}"
        );
    }
    assert_eq!(
        proc.dispatch(TGKILL, [PID + 1, PID, 6, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ESRCH)
    );
    assert_eq!(
        proc.dispatch(TGKILL, [PID, PID + 1, 6, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ESRCH)
    );
    assert_eq!(
        proc.dispatch(TGKILL, [PID, PID, 65, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-super::syscalls::EINVAL)
    );
    assert_eq!(
        proc.dispatch(TGKILL, [0, PID, 6, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-super::syscalls::EINVAL)
    );

    // SIG_IGN installed with rt_sigaction: the signal is discarded, also when it was blocked first
    let mut proc = Process::new(0x10_0000);
    mem.write_u64(DATA + 0x100, 1).unwrap(); // handler = SIG_IGN, flags/restorer/mask left zero
    for off in [8, 16, 24] {
        mem.write_u64(DATA + 0x100 + off, 0).unwrap();
    }
    assert_eq!(
        proc.dispatch(
            super::syscalls::SYS_RT_SIGACTION,
            [6, DATA + 0x100, 0, 8, 0, 0],
            &mut mem,
            &mut out
        ),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(TGKILL, [PID, PID, 6, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    mem.write_u64(DATA + 0x200, 1 << 5).unwrap(); // block SIGABRT, raise, unblock
    assert_eq!(
        proc.dispatch(SYS_RT_SIGPROCMASK, [0, DATA + 0x200, 0, 8, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(TGKILL, [PID, PID, 6, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        proc.dispatch(SYS_RT_SIGPROCMASK, [1, DATA + 0x200, 0, 8, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );

    // a real guest handler is not delivered by this layer: the run stops and names the signal
    let mut proc = Process::new(0x10_0000);
    mem.write_u64(DATA + 0x100, 0x4000).unwrap();
    assert_eq!(
        proc.dispatch(
            super::syscalls::SYS_RT_SIGACTION,
            [6, DATA + 0x100, 0, 8, 0, 0],
            &mut mem,
            &mut out
        ),
        Outcome::Return(0)
    );
    match proc.dispatch(TGKILL, [PID, PID, 6, 0, 0, 0], &mut mem, &mut out) {
        Outcome::Unimplemented(why) => assert!(why.contains("signal 6") && why.contains("handler"), "{why}"),
        other => panic!("{other:?}"),
    }
}

/// MicroPython (and many other programs) look for a file with `stat` before using a built-in: it calls
/// `newfstatat(AT_FDCWD, "random.py", ...)` and treats "no such file" as "not there". `run-elf` has no filesystem and
/// already answers `openat` with ENOENT, but `newfstatat` stopped the run as an unimplemented syscall (exit 70).
/// A stat by path now finds nothing; a stat of a descriptor (AT_EMPTY_PATH with an empty path) stays unimplemented.
#[test]
fn newfstatat_by_path_finds_nothing_but_fstat_of_a_descriptor_is_still_unimplemented() {
    use super::syscalls::{EFAULT, ENOENT, Outcome, Process};
    use crate::mmu::{Memory, Permission};
    const DATA: u64 = 0x1000;
    const NEWFSTATAT: u64 = 79;
    const AT_FDCWD: u64 = -100i64 as u64;
    const AT_EMPTY_PATH: u64 = 0x1000;
    let mut proc = Process::new(0x10_0000);
    let mut mem = Memory::new();
    mem.map_range(DATA, 0x1000, Permission::READ_WRITE);
    let mut out = Capture::default();
    mem.write_bytes(DATA, b"random.py\0").unwrap();
    mem.write_bytes(DATA + 0x100, b"\0").unwrap();
    // newfstatat(AT_FDCWD, "random.py", &st, 0): nothing there
    assert_eq!(
        proc.dispatch(NEWFSTATAT, [AT_FDCWD, DATA, DATA + 0x200, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOENT)
    );
    // an empty path without AT_EMPTY_PATH is ENOENT in Linux too
    assert_eq!(
        proc.dispatch(
            NEWFSTATAT,
            [AT_FDCWD, DATA + 0x100, DATA + 0x200, 0, 0, 0],
            &mut mem,
            &mut out
        ),
        Outcome::Return(-ENOENT)
    );
    // a bad path pointer is EFAULT
    assert_eq!(
        proc.dispatch(
            NEWFSTATAT,
            [AT_FDCWD, 0x9000_0000, DATA + 0x200, 0, 0, 0],
            &mut mem,
            &mut out
        ),
        Outcome::Return(-EFAULT)
    );
    // fstat(1, &st) as newfstatat(1, "", &st, AT_EMPTY_PATH) asks about a descriptor: now implemented
    assert_eq!(
        proc.dispatch(
            NEWFSTATAT,
            [1, DATA + 0x100, DATA + 0x200, AT_EMPTY_PATH, 0, 0],
            &mut mem,
            &mut out,
        ),
        Outcome::Return(0)
    );
}

#[test]
fn hardware_fast_path_instructions_execute_across_all_tiers() {
    assert_eq!(
        crate::linux::elf::HWCAP_VALUE,
        0x20093,
        "AT_HWCAP must expose verified bits: FP | ASIMD | PMULL | CRC32 | SHA3"
    );

    // Assembly sequence testing PMULL, CRC32, and SHA3:
    // pmull v0.8h, v1.8b, v2.8b    (0x0E22E020)
    // pmull2 v0.8h, v1.16b, v2.16b  (0x4E22E020)
    // pmull v0.1q, v1.1d, v2.1d     (0x0EE2E020)
    // pmull2 v0.1q, v1.2d, v2.2d    (0x4EE2E020)
    // crc32b w0, w1, w2             (0x1AC24020)
    // crc32h w0, w1, w2             (0x1AC24420)
    // crc32w w0, w1, w2             (0x1AC24820)
    // crc32x w0, w1, x2             (0x9AC24C20)
    // crc32cb w0, w1, w2            (0x1AC25020)
    // crc32ch w0, w1, w2            (0x1AC25420)
    // crc32cw w0, w1, w2            (0x1AC25820)
    // crc32cx w0, w1, x2            (0x9AC25C20)
    // eor3 v0.16b, v1.16b, v2.16b, v3.16b (0xCE020020)
    // bcax v0.16b, v1.16b, v2.16b, v3.16b (0xCE220020)
    // rax1 v0.2d, v1.2d, v2.2d      (0xCE628C20)
    // xar v0.2d, v1.2d, v2.2d, #1   (0xCE820420)
    // mov x8, #93 (exit); mov x0, #0; svc #0 (0xD2800BA8, 0xD2800000, 0xD4000001)
    let code: [u32; 19] = [
        0x0E22_E020,
        0x4E22_E020,
        0x0EE2_E020,
        0x4EE2_E020,
        0x1AC2_4020,
        0x1AC2_4420,
        0x1AC2_4820,
        0x9AC2_4C20,
        0x1AC2_5020,
        0x1AC2_5420,
        0x1AC2_5820,
        0x9AC2_5C20,
        0xCE02_0020,
        0xCE22_0020,
        0xCE62_8C20,
        0xCE82_0420,
        0xD280_0BA8,
        0xD280_0000,
        0xD400_0001,
    ];
    let elf = tiny_elf(&code);
    for tier in TIERS {
        let (r, _) = go(&elf, &["hwcap_probe"], tier);
        assert_eq!(r.stop, Stop::Exited(0), "Failed on {tier:?}: {r:?}");
    }
}

#[test]
fn ppoll_handles_descriptors_and_bounds() {
    use super::syscalls::{EFAULT, EINVAL, Outcome, Process, SYS_PPOLL};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new(0x10_0000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x2000, Permission::READ_WRITE);
    let mut out = Capture::default();

    // nfds > 1024 returns -EINVAL
    assert_eq!(
        proc.dispatch(SYS_PPOLL, [0x1000, 1025, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EINVAL)
    );

    // sigmask with sigsetsize != 8 returns -EINVAL
    assert_eq!(
        proc.dispatch(SYS_PPOLL, [0x1000, 1, 0, 0x1500, 4, 0], &mut mem, &mut out),
        Outcome::Return(-EINVAL)
    );

    // Bad sigmask pointer returns -EFAULT
    assert_eq!(
        proc.dispatch(SYS_PPOLL, [0x1000, 1, 0, 0x9000_0000, 8, 0], &mut mem, &mut out),
        Outcome::Return(-EFAULT)
    );

    // Setup pollfd array:
    // Entry 0: fd = -1, events = POLLIN (0x1) -> expect revents = 0
    // Entry 1: fd = 50 (closed), events = POLLIN (0x1) -> expect revents = POLLNVAL (0x20)
    // Entry 2: fd = 1 (stdout open), events = POLLOUT (0x4) -> expect revents = POLLOUT (0x4)
    // Entry 3: fd = 0 (stdin open), events = POLLIN (0x1) -> expect revents = POLLIN (0x1)
    let pfd = [
        (-1i32, 0x0001i16, 0i16),
        (50i32, 0x0001i16, 0i16),
        (1i32, 0x0004i16, 0i16),
        (0i32, 0x0001i16, 0i16),
    ];
    for (i, (fd, ev, rev)) in pfd.iter().enumerate() {
        let base = 0x1000 + i as u64 * 8;
        mem.write_bytes(base, &fd.to_le_bytes()).unwrap();
        mem.write_bytes(base + 4, &ev.to_le_bytes()).unwrap();
        mem.write_bytes(base + 6, &rev.to_le_bytes()).unwrap();
    }

    let ret = proc.dispatch(SYS_PPOLL, [0x1000, 4, 0, 0, 0, 0], &mut mem, &mut out);
    // Entries 1, 2, 3 have non-zero revents -> count = 3
    assert_eq!(ret, Outcome::Return(3));

    let mut rev0 = [0u8; 2];
    let mut rev1 = [0u8; 2];
    let mut rev2 = [0u8; 2];
    let mut rev3 = [0u8; 2];
    mem.read_bytes(0x1006, &mut rev0).unwrap();
    mem.read_bytes(0x100e, &mut rev1).unwrap();
    mem.read_bytes(0x1016, &mut rev2).unwrap();
    mem.read_bytes(0x101e, &mut rev3).unwrap();

    assert_eq!(i16::from_le_bytes(rev0), 0);
    assert_eq!(i16::from_le_bytes(rev1), 0x0020); // POLLNVAL
    assert_eq!(i16::from_le_bytes(rev2), 0x0004); // POLLOUT
    assert_eq!(i16::from_le_bytes(rev3), 0x0001); // POLLIN
}

#[test]
fn proc_self_auxv_virtual_file_is_read_only_and_matches_stack() {
    use super::syscalls::{EACCES, Outcome, Process, SYS_FSTAT, SYS_LSEEK, SYS_OPENAT, SYS_READ};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new(0x10_0000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x2000, Permission::READ_WRITE);
    let mut out = Capture::default();

    let fake_auxv = vec![0x12u8; 256];
    proc.set_auxv(fake_auxv.clone());

    mem.write_bytes(0x1000, b"/proc/self/auxv\0").unwrap();

    // Opening with O_WRONLY (1) or O_RDWR (2) fails EACCES
    assert_eq!(
        proc.dispatch(SYS_OPENAT, [-100i64 as u64, 0x1000, 1, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EACCES)
    );

    // Opening with O_RDONLY (0) succeeds
    let fd = match proc.dispatch(SYS_OPENAT, [-100i64 as u64, 0x1000, 0, 0, 0, 0], &mut mem, &mut out) {
        Outcome::Return(fd) => fd as u64,
        other => panic!("expected fd, got {other:?}"),
    };

    // fstat reports 256 bytes size
    assert_eq!(
        proc.dispatch(SYS_FSTAT, [fd, 0x1500, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    let mut size_bytes = [0u8; 8];
    mem.read_bytes(0x1500 + 48, &mut size_bytes).unwrap();
    assert_eq!(i64::from_le_bytes(size_bytes), 256);

    // read 256 bytes
    let read_n = proc.dispatch(SYS_READ, [fd, 0x1100, 512, 0, 0, 0], &mut mem, &mut out);
    assert_eq!(read_n, Outcome::Return(256));
    let mut buf = vec![0u8; 256];
    mem.read_bytes(0x1100, &mut buf).unwrap();
    assert_eq!(buf, fake_auxv);

    // second read is at EOF
    assert_eq!(
        proc.dispatch(SYS_READ, [fd, 0x1100, 512, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );

    // lseek back to start
    assert_eq!(
        proc.dispatch(SYS_LSEEK, [fd, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    // read again succeeds
    assert_eq!(
        proc.dispatch(SYS_READ, [fd, 0x1100, 10, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(10)
    );
}

#[test]
fn ephemeral_scratch_vfs_supports_tmpfile_unlink_and_reclaims_memory() {
    use super::syscalls::{
        EBADF, ENOENT, Outcome, Process, SYS_CLOSE, SYS_LSEEK, SYS_OPENAT, SYS_READ, SYS_UNLINKAT, SYS_WRITE,
    };
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new(0x10_0000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x2000, Permission::READ_WRITE);
    let mut out = Capture::default();

    mem.write_bytes(0x1000, b"/tmp/probe_temp.txt\0").unwrap();

    // 1. openat O_CREAT | O_RDWR (0x42)
    let fd = match proc.dispatch(
        SYS_OPENAT,
        [-100i64 as u64, 0x1000, 0o102, 0o666, 0, 0],
        &mut mem,
        &mut out,
    ) {
        Outcome::Return(fd) => fd as u64,
        other => panic!("expected fd, got {other:?}"),
    };

    // 2. write data
    let payload = b"ephemeral scratch buffer test";
    mem.write_bytes(0x1200, payload).unwrap();
    assert_eq!(
        proc.dispatch(
            SYS_WRITE,
            [fd, 0x1200, payload.len() as u64, 0, 0, 0],
            &mut mem,
            &mut out
        ),
        Outcome::Return(payload.len() as i64)
    );

    // 3. unlinkat while open (POSIX tmpfile pattern)
    assert_eq!(
        proc.dispatch(SYS_UNLINKAT, [-100i64 as u64, 0x1000, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );

    // Path lookup now fails ENOENT
    assert_eq!(
        proc.dispatch(SYS_OPENAT, [-100i64 as u64, 0x1000, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOENT)
    );

    // 4. Descriptor is STILL open, readable, seekable
    assert_eq!(
        proc.dispatch(SYS_LSEEK, [fd, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    let mut readback = vec![0u8; payload.len()];
    assert_eq!(
        proc.dispatch(
            SYS_READ,
            [fd, 0x1400, payload.len() as u64, 0, 0, 0],
            &mut mem,
            &mut out
        ),
        Outcome::Return(payload.len() as i64)
    );
    mem.read_bytes(0x1400, &mut readback).unwrap();
    assert_eq!(readback, payload);

    // 5. Close reclaims inode
    assert_eq!(
        proc.dispatch(SYS_CLOSE, [fd, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );

    // Reading closed fd fails EBADF
    assert_eq!(
        proc.dispatch(SYS_READ, [fd, 0x1400, 10, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EBADF)
    );
}

#[test]
fn path_traversal_outside_sandbox_is_rejected_with_enoent() {
    use super::syscalls::{ENOENT, Outcome, Process, SYS_NEWFSTATAT, SYS_OPENAT, SYS_UNLINKAT};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new(0x10_0000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x2000, Permission::READ_WRITE);
    let mut out = Capture::default();

    let outside_paths: [&[u8]; 4] = [
        b"/tmp/../etc/passwd\0",
        b"/tmp/../../var/log\0",
        b"/etc/shadow\0",
        b"../secret.txt\0",
    ];

    for path in outside_paths {
        mem.write_bytes(0x1000, path).unwrap();
        assert_eq!(
            proc.dispatch(SYS_OPENAT, [-100i64 as u64, 0x1000, 0, 0, 0, 0], &mut mem, &mut out),
            Outcome::Return(-ENOENT)
        );
        assert_eq!(
            proc.dispatch(
                SYS_NEWFSTATAT,
                [-100i64 as u64, 0x1000, 0x1500, 0, 0, 0],
                &mut mem,
                &mut out
            ),
            Outcome::Return(-ENOENT)
        );
        assert_eq!(
            proc.dispatch(SYS_UNLINKAT, [-100i64 as u64, 0x1000, 0, 0, 0, 0], &mut mem, &mut out),
            Outcome::Return(-ENOENT)
        );
    }
}

#[test]
fn sandboxed_stdin_returns_eof_by_default_and_reads_provided_data() {
    use super::runner::run_with_stdin;
    // Assembly:
    // read(0, sp, 16):
    //   mov x0, #0
    //   mov x1, sp
    //   mov x2, #16
    //   mov x8, #63 (read)
    //   svc #0
    // exit(x0):
    //   mov x8, #93
    //   svc #0
    let code = [
        0xD280_0000, // mov x0, #0
        0x9100_03E1, // mov x1, sp
        0xD280_0202, // mov x2, #16
        0xD280_07E8, // mov x8, #63
        0xD400_0001, // svc #0
        0xD280_0BA8, // mov x8, #93
        0xD400_0001, // svc #0
    ];
    let elf = tiny_elf(&code);
    let opts = Options::default();
    let mut cap = Capture::default();

    // Default: no stdin -> read returns 0 (EOF) -> exit(0)
    let r = run_with_stdin(&elf, &["test".into()], &opts, &[], &mut cap, &mut |_| {});
    assert_eq!(r.stop, Stop::Exited(0));

    // With stdin -> read returns 16 -> exit(16)
    let r2 = run_with_stdin(
        &elf,
        &["test".into()],
        &opts,
        b"hello 16-byte!!\n",
        &mut cap,
        &mut |_| {},
    );
    assert_eq!(r2.stop, Stop::Exited(16));
}

#[test]
fn run_elf_cli_double_dash_argument_handling() {
    use super::runner::cli;
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/linux-elf/bin/args.elf").to_string();

    // ounce-core run-elf -- <elf>
    assert_eq!(cli(&["--".into(), fixture.clone()]), 1);

    // ounce-core run-elf -- <elf> arg1 arg2
    assert_eq!(cli(&["--".into(), fixture, "a".into(), "b".into()]), 3);

    // ounce-core run-elf -- (no elf) exits 2
    assert_eq!(cli(&["--".into()]), 2);
}
