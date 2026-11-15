use super::runner::{run, Options, Report, Stop};
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
    let opts = Options { tier, ..Options::default() };
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
        let Stop::Failed { diagnosis, status } = r.stop else { panic!("expected failure: {r:?}") };
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
    let Stop::Failed { diagnosis, status } = r.stop else { panic!("expected failure: {r:?}") };
    assert_eq!(status, 71);
    assert!(diagnosis.contains("instruction word 0x00000000"), "{diagnosis}");
}

#[test]
fn non_linux_svc_is_rejected() {
    // svc #0x7
    let elf = tiny_elf(&[0xD400_00E1]);
    let (r, _) = go(&elf, &["svc7"], CpuTier::Tier3);
    let Stop::Failed { diagnosis, .. } = r.stop else { panic!("{r:?}") };
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
    let opts = Options { max_instructions: 10_000, ..Options::default() };
    let r = run(&elf, &argv, &opts, &mut Capture::default(), &mut |_| {});
    assert!(matches!(r.stop, Stop::Failed { status: 73, .. }), "{r:?}");
}

#[test]
fn loader_rejects_bad_inputs() {
    let reject = |bytes: &[u8], needle: &str| {
        let (r, _) = go(bytes, &["x"], CpuTier::Tier3);
        let Stop::Failed { diagnosis, status } = r.stop else { panic!("accepted") };
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
            let opts = Options { tier, deterministic: true, ..Options::default() };
            let r = run(elf, &["clock".into()], &opts, &mut cap, &mut |_| {});
            assert_eq!(r.stop, Stop::Exited(0), "{r:?}");
            assert_eq!(cap.out, expected);
        }
    }
}

#[test]
fn deterministic_clock_errors_do_not_consume_time_and_seconds_roll_over() {
    use super::syscalls::{Process, Outcome, SYS_CLOCK_GETTIME, EINVAL, EFAULT};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new_deterministic(0x1000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x1000, Permission::READ_WRITE);
    let mut out = Capture::default();
    assert_eq!(proc.dispatch(SYS_CLOCK_GETTIME, [99, 0x1000, 0, 0, 0, 0], &mut mem, &mut out), Outcome::Return(-EINVAL));
    assert_eq!(proc.dispatch(SYS_CLOCK_GETTIME, [1, 0, 0, 0, 0, 0], &mut mem, &mut out), Outcome::Return(-EFAULT));
    for i in 0..1002u64 {
        assert_eq!(proc.dispatch(SYS_CLOCK_GETTIME, [1, 0x1000, 0, 0, 0, 0], &mut mem, &mut out), Outcome::Return(0));
        assert_eq!(mem.read_u64(0x1000).unwrap(), i / 1000);
        assert_eq!(mem.read_u64(0x1008).unwrap(), (i % 1000) * 1_000_000);
    }
}

#[test]
fn sched_getaffinity_reports_one_cpu_and_validates_like_linux() {
    use super::syscalls::{Process, Outcome, SYS_SCHED_GETAFFINITY, EINVAL, EFAULT, ESRCH, PID};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x1000, Permission::READ_WRITE);
    mem.write_bytes(0x1000, &[0xff; 16]).unwrap();
    let mut out = Capture::default();
    let mut call = |pid, len, mask, mem: &mut Memory| proc.dispatch(SYS_SCHED_GETAFFINITY, [pid, len, mask, 0, 0, 0], mem, &mut out);
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
    use super::syscalls::{Process, Outcome, SYS_MMAP, SYS_MREMAP, EINVAL, ENOMEM};
    use crate::mmu::Memory;
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    let mut out = Capture::default();
    let Outcome::Return(a) = proc.dispatch(SYS_MMAP, [0, 0x4000, 3, 0x22, u64::MAX, 0], &mut mem, &mut out) else { panic!() };
    let a = a as u64;
    mem.write_u64(a, 0x1122_3344_5566_7788).unwrap();
    mem.write_u64(a + 0x3ff8, 42).unwrap();
    // without MAYMOVE growth is refused, bad flags are invalid
    assert_eq!(proc.dispatch(SYS_MREMAP, [a, 0x4000, 0x8000, 0, 0, 0], &mut mem, &mut out), Outcome::Return(-ENOMEM));
    assert_eq!(proc.dispatch(SYS_MREMAP, [a, 0x4000, 0x8000, 2, 0, 0], &mut mem, &mut out), Outcome::Return(-EINVAL));
    // grow with MAYMOVE: contents preserved at the new address, old range unmapped
    let Outcome::Return(b) = proc.dispatch(SYS_MREMAP, [a, 0x4000, 0x8000, 1, 0, 0], &mut mem, &mut out) else { panic!() };
    let b = b as u64;
    assert_ne!(a, b);
    assert_eq!(mem.read_u64(b).unwrap(), 0x1122_3344_5566_7788);
    assert_eq!(mem.read_u64(b + 0x3ff8).unwrap(), 42);
    assert_eq!(mem.read_u64(b + 0x7ff8).unwrap(), 0);
    assert!(!mem.is_mapped(a));
    // shrink in place
    assert_eq!(proc.dispatch(SYS_MREMAP, [b, 0x8000, 0x1000, 0, 0, 0], &mut mem, &mut out), Outcome::Return(b as i64));
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
    use super::syscalls::{Process, Outcome, SYS_MMAP, SYS_MUNMAP, SYS_MPROTECT, ENOMEM, EINVAL};
    use crate::mmu::Memory;
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    let mut out = Capture::default();
    let n = 0x10000100000;
    let Outcome::Return(base) = proc.dispatch(SYS_MMAP, [0, n, 0, 0x4022, u64::MAX, 0], &mut mem, &mut out) else { panic!() };
    assert!(base > 0);
    let base = base as u64;
    assert_eq!(mem.bucket_stats().total_pages, 0);
    assert_eq!(proc.dispatch(SYS_MPROTECT, [base, n, 3, 0, 0, 0], &mut mem, &mut out), Outcome::Return(-ENOMEM));
    assert_eq!(proc.dispatch(SYS_MPROTECT, [base + 4096, 1, 3, 0, 0, 0], &mut mem, &mut out), Outcome::Return(0));
    assert_eq!(mem.bucket_stats().total_pages, 1);
    assert_eq!(proc.dispatch(SYS_MUNMAP, [base + 8192, 4096, 0, 0, 0, 0], &mut mem, &mut out), Outcome::Return(0));
    assert_eq!(proc.dispatch(SYS_MPROTECT, [base + 4096, 8192, 3, 0, 0, 0], &mut mem, &mut out), Outcome::Return(-ENOMEM));
    assert_eq!(mem.bucket_stats().total_pages, 1);
    assert_eq!(proc.dispatch(SYS_MMAP, [0, u64::MAX, 0, 0x4022, u64::MAX, 0], &mut mem, &mut out), Outcome::Return(-EINVAL));
    assert_eq!(proc.dispatch(SYS_MUNMAP, [base, n, 0, 0, 0, 0], &mut mem, &mut out), Outcome::Return(0));
    assert_eq!(mem.bucket_stats().total_pages, 0);
}

#[test]
fn mmap_hints_do_not_overlap_reservations_and_fixed_maps_trim_them() {
    use super::syscalls::{Process, Outcome, SYS_MMAP, SYS_MPROTECT, SYS_MUNMAP, ENOMEM};
    use crate::mmu::Memory;
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    let mut out = Capture::default();
    let mut map = |hint, len, prot, flags, mem: &mut Memory| {
        let Outcome::Return(addr) = proc.dispatch(SYS_MMAP, [hint, len, prot, flags, u64::MAX, 0], mem, &mut out) else { panic!() };
        assert!(addr > 0);
        addr as u64
    };
    let a = map(0, 0x100000000, 0, 0x4022, &mut mem);
    let b = map(a + 4096, 4096, 3, 0x22, &mut mem);
    assert!(b >= a + 0x100000000);
    assert_eq!(map(a + 8192, 4096, 3, 0x32, &mut mem), a + 8192);
    assert_eq!(mem.bucket_stats().total_pages, 2);
    assert_eq!(proc.dispatch(SYS_MPROTECT, [a, 4096, 3, 0, 0, 0], &mut mem, &mut out), Outcome::Return(0));
    assert_eq!(proc.dispatch(SYS_MUNMAP, [a + 4096, 4096, 0, 0, 0, 0], &mut mem, &mut out), Outcome::Return(0));
    assert_eq!(proc.dispatch(SYS_MPROTECT, [a, 12288, 3, 0, 0, 0], &mut mem, &mut out), Outcome::Return(-ENOMEM));
}
