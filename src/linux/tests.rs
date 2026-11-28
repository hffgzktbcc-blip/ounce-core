use super::runner::{Options, Report, Stop, describe_regs, run};
use super::syscalls::Output;
use crate::cpu::CpuTier;

const HELLO: &[u8] = include_bytes!("../../tests/linux-elf/bin/hello.elf");
const EXITCODE: &[u8] = include_bytes!("../../tests/linux-elf/bin/exitcode.elf");
const ARGS: &[u8] = include_bytes!("../../tests/linux-elf/bin/args.elf");
const PIE: &[u8] = include_bytes!("../../tests/linux-elf/bin/pie.elf");
const FAKELD: &[u8] = include_bytes!("../../tests/linux-elf/bin/fakeld.elf");
const DYNMAIN: &[u8] = include_bytes!("../../tests/linux-elf/bin/dynmain.elf");
const READFILE: &[u8] = include_bytes!("../../tests/linux-elf/bin/readfile.elf");
const THREADS: &[u8] = include_bytes!("../../tests/linux-elf/bin/threads.elf");
const MAPFILE: &[u8] = include_bytes!("../../tests/linux-elf/bin/mapfile.elf");
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
        assert!(diagnosis.contains("syscall 280 (bpf)"), "{diagnosis}");
        assert!(diagnosis.contains("pc=0x"), "{diagnosis}");
    }
}

/// `RMIF x0, #0, #1` (FEAT_FlagM) is a valid encoding that executes on the Apple M3 but that ounce-core does not
/// decode yet: an emulator gap, so it keeps stopping with the unimplemented-instruction diagnosis (not SIGILL).
const RMIF: u32 = 0xBA00_0401;

#[test]
fn unimplemented_instruction_is_diagnosed_by_word() {
    let elf = tiny_elf(&[RMIF]);
    for tier in TIERS {
        let (r, _) = go(&elf, &["rmif"], tier);
        let Stop::Failed { diagnosis, status } = r.stop else {
            panic!("expected failure: {r:?}")
        };
        assert_eq!(status, 71, "{tier:?}");
        assert!(diagnosis.contains("instruction word 0xba000401"), "{diagnosis}");
        assert!(diagnosis.contains("Unimplemented instruction"), "{diagnosis}");
    }
}

/// With no handler, UDF and the rest of the reserved class kill the guest with SIGILL (status 128 + 4), as on
/// Linux, and the diagnosis still names the word.
#[test]
fn undefined_encodings_without_a_handler_end_in_sigill_on_every_tier() {
    for word in [0x0000_0000u32, 0x0000_ffff, 0x0001_0000, 0x01ff_ffff] {
        let elf = tiny_elf(&[word]);
        for tier in TIERS {
            let (r, _) = go(&elf, &["udf"], tier);
            let Stop::Failed { diagnosis, status } = r.stop else {
                panic!("expected failure: {r:?}")
            };
            assert_eq!(status, 132, "{word:#x} on {tier:?}");
            assert!(diagnosis.contains("SIGILL"), "{diagnosis}");
            assert!(
                diagnosis.contains(&format!("instruction word {word:#010x}")),
                "{diagnosis}"
            );
        }
    }
}

/// Ignoring SIGILL does not help a forced signal: the kernel resets the action to default and kills the process.
#[test]
fn ignored_sigill_is_still_fatal() {
    let elf = tiny_elf(&[
        0xd280_0029,
        0xa9be_7fe9,
        0xa901_7fff,
        0xd280_0080,
        0x9100_03e1,
        0xd280_0002,
        0xd280_0103,
        0xd280_10c8,
        0xd400_0001, // rt_sigaction(SIGILL, {SIG_IGN}, NULL, 8)
        0x0000_0000, // udf #0
    ]);
    for tier in TIERS {
        let (r, _) = go(&elf, &["ign"], tier);
        assert!(matches!(r.stop, Stop::Failed { status: 132, .. }), "{tier:?}: {r:?}");
    }
}

/// `fault_signals.c`: a guest handler receives SIGILL (UDF, reserved word, EL1 MSR, unexposed MRS) with
/// `ILL_ILLOPC` and `si_addr` = pc, and SIGBUS (misaligned LDXR/LDAXR/LDXRH/STXR/LDXP/LDAXP/STXP) with `BUS_ADRALN`
/// and `si_addr` = the address; the handler skips the instruction and the guest continues. CLREX clears the monitor.
#[test]
fn guest_handlers_receive_sigill_and_sigbus_on_every_tier() {
    let elf = include_bytes!("../../tests/linux-elf/bin/fault_signals.elf");
    for tier in TIERS {
        let (r, out) = go(elf, &["fault_signals"], tier);
        assert_eq!(r.stop, Stop::Exited(0), "{tier:?}: {r:?}");
        assert_eq!(out, "SIGILL and SIGBUS delivery, siginfo and CLREX: ok\n", "{tier:?}");
    }
}

/// With no handler, a misaligned exclusive ends in SIGBUS (status 128 + 7). ounce-core does not advertise
/// `HWCAP_USCAT` (FEAT_LSE2), so it applies the pre-LSE2 rule: any access not aligned to its size faults, not only
/// one that crosses a 16-byte boundary (the Apple M3 and other LSE2 cores only fault on the crossing; see
/// docs/verify/sigill-sigbus/).
#[test]
fn misaligned_exclusives_without_a_handler_end_in_sigbus_on_every_tier() {
    let sp_plus = |n: u32| 0x9100_03E0 | (n << 10); // add x0, sp, #n
    for (name, code) in [
        ("ldxr x +1", vec![sp_plus(1), 0xc85f_7c01]),
        ("ldxr w +2", vec![sp_plus(2), 0x885f_7c01]),
        ("ldxrh +1", vec![sp_plus(1), 0x485f_7c01]),
        ("stxr x +4", vec![sp_plus(4), 0xc802_7c01]),
        ("ldxp x +8", vec![sp_plus(8), 0xc87f_0801]),
        ("ldaxp w +4", vec![sp_plus(4), 0x887f_8801]),
        ("stlxp w +4", vec![sp_plus(4), 0x8823_8801]),
    ] {
        for tier in TIERS {
            let (r, _) = go(&tiny_elf(&code), &["bus"], tier);
            let Stop::Failed { diagnosis, status } = &r.stop else {
                panic!("{name} on {tier:?}: {r:?}")
            };
            assert_eq!(*status, 135, "{name} on {tier:?}: {diagnosis}");
            assert!(diagnosis.contains("SIGBUS"), "{diagnosis}");
        }
    }
}

/// The local monitor tracks address and size. A store-exclusive of another size or to another address fails
/// (architecturally CONSTRAINED UNPREDICTABLE; the Apple M3 lets both succeed). An ordinary store in between
/// does not clear it (IMPLEMENTATION DEFINED; the M3 clears it for the same 16-byte granule): documented in LIMITS.md.
#[test]
fn exclusive_monitor_tracks_address_and_size_on_every_tier() {
    let exit_w2 = [0xaa02_03e0u32, 0xd280_0bc8, 0xd400_0001]; // mov x0, x2; mov x8, #94; svc #0
    let with = |body: &[u32]| {
        let mut code = vec![0xd100_83ff, 0x9100_03e0, 0x9100_23e4]; // sub sp, sp, #32; mov x0, sp; add x4, sp, #8
        code.extend_from_slice(body);
        code.extend_from_slice(&exit_w2);
        tiny_elf(&code)
    };
    for (name, body, status) in [
        ("ldxr x; stxr x", vec![0xc85f_7c01, 0xc802_7c01], 0),
        ("ldxr w; stxr x (size differs)", vec![0x885f_7c01, 0xc802_7c03], 1),
        ("ldxr x; stxr w (size differs)", vec![0xc85f_7c01, 0x8802_7c03], 1),
        (
            "ldxr [sp]; stxr [sp+8] (address differs)",
            vec![0xc85f_7c01, 0xc802_7c83],
            1,
        ),
        ("ldxr; clrex; stxr", vec![0xc85f_7c01, 0xd503_3f5f, 0xc802_7c01], 1),
        ("stxr alone", vec![0xc802_7c01], 1),
        ("ldxr; stxr; stxr", vec![0xc85f_7c01, 0xc802_7c01, 0xc802_7c01], 1),
        (
            "ldxr; str same address; stxr",
            vec![0xc85f_7c01, 0xf900_0001, 0xc802_7c01],
            0,
        ),
    ] {
        for tier in TIERS {
            let (r, _) = go(&with(&body), &["monitor"], tier);
            assert_eq!(r.stop, Stop::Exited(status), "{name} on {tier:?}: {r:?}");
        }
    }
}

#[test]
fn el0_system_register_access_is_explicitly_diagnosed_on_every_tier() {
    for (word, register, access) in [
        (0xd538_0000, "S3_0_C0_C0_0", "MRS"), // MIDR_EL1; HWCAP_CPUID is not advertised.
        (0xd518_1000, "S3_0_C1_C0_0", "MSR"), // SCTLR_EL1 is privileged.
    ] {
        let elf = tiny_elf(&[word]);
        for tier in TIERS {
            let (r, _) = go(&elf, &["sysreg"], tier);
            let Stop::Failed { diagnosis, status } = r.stop else {
                panic!("expected failure: {r:?}")
            };
            assert_eq!(status, 132, "no handler: SIGILL");
            assert!(diagnosis.contains(register), "{diagnosis}");
            assert!(diagnosis.contains(access), "{diagnosis}");
        }
    }
}

#[test]
fn linux_counter_is_deterministic_and_advances_on_every_tier() {
    // MRS x2,CNTVCT_EL0; MRS x3,CNTVCT_EL0; SUB x0,x3,x2; exit(x0).
    let elf = tiny_elf(&[0xd53b_e042, 0xd53b_e043, 0xcb02_0060, 0xd280_0ba8, 0xd400_0001]);
    for tier in TIERS {
        let mut cap = Capture::default();
        let opts = Options {
            tier,
            deterministic: true,
            ..Options::default()
        };
        let r = run(&elf, &["counter".into()], &opts, &mut cap, &mut |_| {});
        assert_eq!(r.stop, Stop::Exited(1), "{tier:?}: {r:?}");
    }
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
    let mut core = HELLO.to_vec();
    core[16] = 4; // ET_CORE
    reject(&core, "e_type 4");
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
    reject(&interp, "--root");
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
    proc.set_retired_instructions(1_234_567_890);
    for clock in [2, 3] {
        assert_eq!(
            proc.dispatch(SYS_CLOCK_GETTIME, [clock, 0x1000, 0, 0, 0, 0], &mut mem, &mut out),
            Outcome::Return(0)
        );
        assert_eq!(mem.read_u64(0x1000).unwrap(), 1);
        assert_eq!(mem.read_u64(0x1008).unwrap(), 234_567_890);
    }
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
fn sched_setaffinity_validates_and_accepts_cpu_zero() {
    use super::syscalls::{EFAULT, EINVAL, ESRCH, Outcome, PID, Process, SYS_SCHED_SETAFFINITY};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x1000, Permission::READ_WRITE);
    let mut out = Capture::default();
    let mut call = |pid, len, mask, mem: &mut Memory| {
        proc.dispatch(SYS_SCHED_SETAFFINITY, [pid, len, mask, 0, 0, 0], mem, &mut out)
    };
    // Setting CPU 0 succeeds (mask = 1).
    mem.write_u64(0x1000, 1).unwrap();
    assert_eq!(call(0, 8, 0x1000, &mut mem), Outcome::Return(0));
    assert_eq!(call(PID, 128, 0x1000, &mut mem), Outcome::Return(0));
    // Setting mask with CPU 0 + other bits also includes CPU 0 and succeeds.
    mem.write_u64(0x1000, 3).unwrap();
    assert_eq!(call(0, 8, 0x1000, &mut mem), Outcome::Return(0));
    // Mask without CPU 0 fails with EINVAL (no allowed CPU requested).
    mem.write_u64(0x1000, 2).unwrap();
    assert_eq!(call(0, 8, 0x1000, &mut mem), Outcome::Return(-EINVAL));
    mem.write_u64(0x1000, 0).unwrap();
    assert_eq!(call(0, 8, 0x1000, &mut mem), Outcome::Return(-EINVAL));
    // Invalid length or pid.
    assert_eq!(call(0, 4, 0x1000, &mut mem), Outcome::Return(-EINVAL));
    assert_eq!(call(0, 12, 0x1000, &mut mem), Outcome::Return(-EINVAL));
    assert_eq!(call(PID + 1, 8, 0x1000, &mut mem), Outcome::Return(-ESRCH));
    // Invalid pointer.
    assert_eq!(call(0, 8, 0, &mut mem), Outcome::Return(-EFAULT));
}

#[test]
fn getcpu_reports_cpu_zero_and_node_zero() {
    use super::syscalls::{EFAULT, Outcome, Process, SYS_GETCPU};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x1000, Permission::READ_WRITE);
    let mut out = Capture::default();
    let mut call =
        |cpu, node, tcache, mem: &mut Memory| proc.dispatch(SYS_GETCPU, [cpu, node, tcache, 0, 0, 0], mem, &mut out);
    mem.write_u32(0x1000, 0xdead_beef).unwrap();
    mem.write_u32(0x1004, 0xbadc_0ffe).unwrap();
    assert_eq!(call(0x1000, 0x1004, 0, &mut mem), Outcome::Return(0));
    assert_eq!(mem.read_u32(0x1000).unwrap(), 0);
    assert_eq!(mem.read_u32(0x1004).unwrap(), 0);
    // Null pointers are permitted and ignored.
    assert_eq!(call(0, 0, 0, &mut mem), Outcome::Return(0));
    assert_eq!(call(0x1000, 0, 0, &mut mem), Outcome::Return(0));
    assert_eq!(call(0, 0x1004, 0, &mut mem), Outcome::Return(0));
    // Unmapped pointer fails with EFAULT.
    assert_eq!(call(0x9000, 0, 0, &mut mem), Outcome::Return(-EFAULT));
    assert_eq!(call(0, 0x9000, 0, &mut mem), Outcome::Return(-EFAULT));
}

#[test]
fn fork_style_clone_and_clone3_return_enosys_with_one_time_warning() {
    // Threads are created by the runner (`threads.rs`) before dispatch; what reaches dispatch is a fork-style
    // clone (no CLONE_THREAD), which stays ENOSYS with one stderr note, and clone3, which is ENOSYS so glibc
    // falls back to clone.
    use super::syscalls::{CLONE_THREAD, ENOSYS, Outcome, Process, SYS_CLONE, SYS_CLONE3};
    use crate::mmu::{Memory, Permission};
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x1000, Permission::READ_WRITE);
    let mut out = Capture::default();

    let fork = 0x11; // SIGCHLD only, as fork(2) passes
    assert_eq!(
        proc.dispatch(SYS_CLONE, [fork, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOSYS)
    );
    let expected_msg = format!(
        "[run-elf] guest tried to start a process (clone flags {:#x}); fork is not supported, clone returned ENOSYS\n",
        fork
    );
    assert_eq!(String::from_utf8_lossy(&out.err), expected_msg);
    assert!(out.out.is_empty(), "stdout must remain clean");
    assert_eq!(
        proc.dispatch(SYS_CLONE, [fork, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOSYS)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.err),
        expected_msg,
        "the note is printed once"
    );

    mem.write_u64(0x1000, CLONE_THREAD).unwrap();
    assert_eq!(
        proc.dispatch(SYS_CLONE3, [0x1000, 88, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOSYS)
    );
    assert_eq!(String::from_utf8_lossy(&out.err), expected_msg, "clone3 adds no note");
}

#[test]
fn single_cpu_and_clone_syscalls_execute_across_all_tiers() {
    // AArch64 machine code that exercises:
    // 1. sched_getaffinity(0, 8, sp) -> returns 8, writes mask 1
    // 2. sched_setaffinity(0, 8, sp) -> returns 0
    // 3. getcpu(sp, sp + 4, 0) -> returns 0, writes cpu 0 and node 0
    // 4. clone(CLONE_THREAD, 0, 0, 0, 0) without CLONE_VM/CLONE_SIGHAND -> returns -22 (EINVAL, as on Linux)
    // 5. exit 0 (or exit 42 on any failure)
    let code: [u32; 42] = [
        0xd2800f68, 0xd2800000, 0xd2800101, 0x910003e2, 0xd4000001, 0xf100201f, 0x54000421, 0xf94003e3, 0xf100047f,
        0x540003c1, 0xd2800f48, 0xd2800000, 0xd2800101, 0x910003e2, 0xd4000001, 0xb5000300, 0xd2801508, 0x910003e0,
        0x910013e1, 0xd2800002, 0xd4000001, 0xb5000240, 0xb94003e4, 0x35000204, 0xb94007e5, 0x350001c5, 0xd2801b88,
        0xd2a00020, 0xd2800001, 0xd2800002, 0xd2800003, 0xd2800004, 0xd2800005, 0xd4000001, 0xb100581f, 0x54000081,
        0xd2800ba8, 0xd2800000, 0xd4000001, 0xd2800ba8, 0xd2800540, 0xd4000001,
    ];
    let elf = tiny_elf(&code);
    for tier in TIERS {
        let (r, _) = go(&elf, &["affinity_test"], tier);
        assert_eq!(r.stop, Stop::Exited(0), "{tier:?}: expected clean exit 0, got {r:?}");
    }
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
        ("unhandled_svc", 14, Some(280))
    );
    assert!(report.message.unwrap().contains("bpf"));
    assert_eq!(report.pc, r.detail.regs.map(|regs| regs.pc));
}

#[test]
fn non_linux_svc_reports_its_immediate_as_svc_id() {
    let r = go_t3(&tiny_elf(&[0xD400_00E1])); // svc #0x7
    assert_eq!((r.detail.stop_kind, r.detail.svc_id), ("unhandled_svc", Some(7)));
}

#[test]
fn unimplemented_instruction_reports_its_opcode() {
    let r = go_t3(&tiny_elf(&[RMIF]));
    let report = r.to_run_report(CpuTier::Tier3);
    assert_eq!(
        (report.stop_kind, report.exit_code, report.opcode),
        ("unimplemented_insn", 10, Some(RMIF))
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
        0x2257fb,
        "AT_HWCAP must expose verified bits: FP | ASIMD | AES | PMULL | SHA1 | SHA2 | CRC32 | ATOMICS | FPHP | ASIMDHP | ASIMDRDM | FCMA | SHA3 | SHA512"
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

/// Known answers for PMULL/PMULL2 (8B, 1D), CRC32{B,H,W,X}, CRC32C{B,H,W,X}, EOR3, BCAX, RAX1 and XAR,
/// three input sets each. `hwcap_kat.expected` was produced by the same source (`hwcap_kat_core.h`)
/// compiled natively with clang on an Apple M3, so a wrong result on any tier fails here.
#[test]
fn hwcap_instructions_match_native_known_answers_on_every_tier() {
    let elf = include_bytes!("../../tests/linux-elf/bin/hwcap_kat.elf");
    let expected = include_str!("../../tests/linux-elf/hwcap_kat.expected");
    for tier in TIERS {
        let (r, out) = go(elf, &["hwcap_kat"], tier);
        assert_eq!(r.stop, Stop::Exited(0), "{tier:?}: {r:?}");
        assert_eq!(out, expected, "{tier:?} results differ from native hardware");
    }
}

/// Known answers for LDXP/LDAXP/STXP/STLXP (W and X pairs, XZR operands, a consumed monitor, 128-bit
/// increment and CAS loops) and every FEAT_LSE form (`CAS{B,H,}`, `CASP`, `LD<op>`, `ST<op>`, `SWP`, `LDAPR`),
/// compiled natively with clang on an Apple M3 from the same source (`lse_kat_core.h`).
#[test]
fn exclusive_pairs_and_lse_match_native_known_answers_on_every_tier() {
    let elf = include_bytes!("../../tests/linux-elf/bin/lse_kat.elf");
    let expected = include_str!("../../tests/linux-elf/lse_kat.expected");
    for tier in TIERS {
        let (r, out) = go(elf, &["lse_kat"], tier);
        assert_eq!(r.stop, Stop::Exited(0), "{tier:?}: {r:?}");
        assert_eq!(out, expected, "{tier:?} results differ from native hardware");
    }
}

/// `LDXP x1, x1, [x0]` is UNDEFINED on the M3 (SIGILL), and an exclusive pair must be aligned to the whole
/// pair (16 bytes for X, 8 for W): these stop the run on every tier instead of loading wrong data.
#[test]
fn exclusive_pair_undefined_and_misaligned_forms_stop() {
    let mov_x0_sp = 0x9100_03E0u32;
    let add_x0_sp_8 = 0x9100_23E0u32;
    let ldxp_x1_x1 = 0xC87F_0401u32; // ldxp x1, x1, [x0]
    let ldxp_x1_x2 = 0xC87F_0801u32; // ldxp x1, x2, [x0]
    let ldaxp_w1_w2 = 0x887F_8801u32; // ldaxp w1, w2, [x0]
    let stxp_w3_x1_x2 = 0xC823_0801u32; // stxp w3, x1, x2, [x0]
    for (name, code) in [
        ("ldxp x1, x1", vec![mov_x0_sp, ldxp_x1_x1]),
        ("ldxp x, misaligned by 8", vec![add_x0_sp_8, ldxp_x1_x2]),
        ("stxp x, misaligned by 8", vec![add_x0_sp_8, stxp_w3_x1_x2]),
        ("ldaxp w, misaligned by 4", vec![0x9100_13E0, ldaxp_w1_w2]),
    ] {
        for tier in TIERS {
            let (r, _) = go(&tiny_elf(&code), &["pair"], tier);
            assert!(matches!(r.stop, Stop::Failed { .. }), "{name} on {tier:?}: {r:?}");
        }
    }
    // The aligned forms of the same instructions run.
    for tier in TIERS {
        let code = [mov_x0_sp, ldxp_x1_x2, 0xD280_0BA8, 0xD280_0000, 0xD400_0001];
        let (r, _) = go(&tiny_elf(&code), &["pair"], tier);
        assert_eq!(r.stop, Stop::Exited(0), "aligned ldxp on {tier:?}: {r:?}");
    }
}

/// FEAT_AES, FEAT_SHA1, FEAT_SHA2 and FEAT_SHA512 (`HWCAP_AES`, `HWCAP_SHA1`, `HWCAP_SHA2`, `HWCAP_SHA512`): a full
/// AES-128 encryption and decryption of the FIPS-197 appendix B block, SHA-1, SHA-256 and SHA-512 digests of "abc"
/// built only from the instructions, and a hash of 2048 pseudo-random inputs per instruction. `crypto_kat.expected`
/// was produced by the same source (`crypto_kat_core.h`) compiled natively with clang on an Apple M3.
#[test]
fn crypto_extensions_match_native_known_answers_on_every_tier() {
    let elf = include_bytes!("../../tests/linux-elf/bin/crypto_kat.elf");
    let expected = include_str!("../../tests/linux-elf/crypto_kat.expected");
    // FIPS-197 appendix B ciphertext and the FIPS-180 "abc" digests, independent of the hardware output.
    for known in [
        "3925841d02dc09fbdc118597196a0b32",
        "3243f6a8885a308d313198a2e0370734",
        "a9993e364706816aba3e25717850c26c9cd0d89d",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
    ] {
        assert!(expected.contains(known), "{known} missing from the expected output");
    }
    for tier in TIERS {
        let (r, out) = go(elf, &["crypto_kat"], tier);
        assert_eq!(r.stop, Stop::Exited(0), "{tier:?}: {r:?}");
        assert_eq!(out, expected, "{tier:?} results differ from native hardware");
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

#[test]
fn getrandom_is_deterministic_validated_and_capped() {
    use super::syscalls::{EFAULT, EINVAL, Outcome, Process};
    use crate::mmu::{Memory, Permission};
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x2000, Permission::READ_WRITE);
    let mut out = Capture::default();
    let mut run = || {
        let mut p = Process::new(0x10_0000);
        assert_eq!(
            p.dispatch(278, [0x1000, 16, 4, 0, 0, 0], &mut mem, &mut out),
            Outcome::Return(16)
        );
        let mut b = [0u8; 16];
        mem.read_bytes(0x1000, &mut b).unwrap();
        b
    };
    let (a, b) = (run(), run());
    assert_eq!(a, b, "fixed seed: identical bytes every run");
    assert!(a.iter().any(|&x| x != 0));
    let mut p = Process::new(0x10_0000);
    for flags in [0, 1, 2, 4, 7] {
        assert_eq!(
            p.dispatch(278, [0x1000, 8, flags, 0, 0, 0], &mut mem, &mut out),
            Outcome::Return(8)
        );
    }
    assert_eq!(
        p.dispatch(278, [0x1000, 8, 8, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EINVAL)
    );
    assert_eq!(
        p.dispatch(278, [0x9000_0000, 8, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-EFAULT)
    );
    assert_eq!(
        p.dispatch(278, [0x1000, 0x1000, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(256)
    );
}

/// `--root=DIR`: guest paths outside the scratch area name files under DIR, read-only, and nothing outside DIR
/// (lexical `..` stops at the root; a symlink that leads out is refused).
#[test]
#[allow(non_snake_case)]
fn host_root_exposes_a_read_only_folder_and_nothing_outside_it() {
    use super::syscalls::{Outcome, Process};
    use crate::mmu::{Memory, Permission};
    const AT_FDCWD: u64 = -100i64 as u64;
    let (OPENAT, READ, LSEEK, PREAD64, NEWFSTATAT, GETDENTS64) = (56u64, 63u64, 62u64, 67u64, 79u64, 61u64);
    let (O_WRONLY, O_CREAT, O_DIRECTORY) = (1u64, 0o100u64, 0o200000u64);
    let (ENOENT, EROFS) = (2i64, 30i64);
    let base = std::env::temp_dir().join(format!("ounce_root_test_{}", std::process::id()));
    let (root, outside) = (base.join("root"), base.join("outside"));
    std::fs::create_dir_all(root.join("etc")).unwrap();
    std::fs::create_dir_all(root.join("data")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(root.join("etc/motd"), b"hello from the host\n").unwrap();
    std::fs::write(root.join("data/n.bin"), [1u8, 2, 3, 4, 5]).unwrap();
    std::fs::write(outside.join("secret"), b"must not be readable").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.join("secret"), root.join("escape")).unwrap();

    let mut mem = Memory::new();
    mem.map_range(0x10000, 0x10000, Permission::READ_WRITE);
    let mut out = Capture::default();
    let path = |mem: &mut Memory, p: &str| {
        mem.write_bytes(0x10000, p.as_bytes()).unwrap();
        mem.write_bytes(0x10000 + p.len() as u64, &[0]).unwrap();
        0x10000u64
    };
    let mut proc = Process::new(0x1000);
    let p = path(&mut mem, "/etc/motd");
    assert_eq!(
        proc.dispatch(OPENAT, [AT_FDCWD, p, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(-ENOENT),
        "no root: host files stay hidden"
    );

    proc.set_host_root(&root).unwrap();
    let open = |proc: &mut Process, mem: &mut Memory, out: &mut Capture, p: &str, flags: u64| {
        mem.write_bytes(0x10000, p.as_bytes()).unwrap();
        mem.write_bytes(0x10000 + p.len() as u64, &[0]).unwrap();
        match proc.dispatch(OPENAT, [AT_FDCWD, 0x10000, flags, 0o644, 0, 0], mem, out) {
            Outcome::Return(v) => v,
            other => panic!("{p}: {other:?}"),
        }
    };
    let fd = open(&mut proc, &mut mem, &mut out, "/etc/motd", 0) as u64;
    assert!(fd >= 3);
    assert_eq!(
        proc.dispatch(READ, [fd, 0x11000, 64, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(20)
    );
    let mut got = [0u8; 20];
    mem.read_bytes(0x11000, &mut got).unwrap();
    assert_eq!(&got, b"hello from the host\n");
    assert_eq!(
        proc.dispatch(LSEEK, [fd, 0, 2, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(20),
        "SEEK_END"
    );
    assert_eq!(
        proc.dispatch(PREAD64, [fd, 0x11000, 4, 6, 0, 0], &mut mem, &mut out),
        Outcome::Return(4)
    );
    mem.read_bytes(0x11000, &mut got[..4]).unwrap();
    assert_eq!(&got[..4], b"from");
    // relative to the root (the guest's working directory is /), and `..` cannot climb above it
    for p in ["etc/motd", "/etc/../etc/motd", "/../../../etc/motd"] {
        assert!(open(&mut proc, &mut mem, &mut out, p, 0) >= 3, "{p}");
    }
    // read-only, nothing created, nothing outside, nothing missing
    assert_eq!(open(&mut proc, &mut mem, &mut out, "/etc/motd", O_WRONLY), -EROFS);
    assert_eq!(
        open(&mut proc, &mut mem, &mut out, "/etc/new", O_WRONLY | O_CREAT),
        -EROFS
    );
    assert_eq!(open(&mut proc, &mut mem, &mut out, "/nope", 0), -ENOENT);
    #[cfg(unix)]
    assert_eq!(
        open(&mut proc, &mut mem, &mut out, "/escape", 0),
        -ENOENT,
        "a symlink out of the root is refused"
    );
    // stat by path: a 5-byte regular file and a directory
    let p = path(&mut mem, "/data/n.bin");
    assert_eq!(
        proc.dispatch(NEWFSTATAT, [AT_FDCWD, p, 0x12000, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(
        (
            mem.read_u32(0x12010).unwrap() & 0o170000,
            mem.read_u64(0x12030).unwrap()
        ),
        (0o100000, 5)
    );
    let p = path(&mut mem, "/data");
    assert_eq!(
        proc.dispatch(NEWFSTATAT, [AT_FDCWD, p, 0x12000, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    assert_eq!(mem.read_u32(0x12010).unwrap() & 0o170000, 0o040000);
    // getdents64 lists the directory: linux_dirent64 records (d_ino, d_off, d_reclen, d_type, name)
    let dfd = open(&mut proc, &mut mem, &mut out, "/data", O_DIRECTORY) as u64;
    let Outcome::Return(n) = proc.dispatch(GETDENTS64, [dfd, 0x13000, 4096, 0, 0, 0], &mut mem, &mut out) else {
        panic!()
    };
    let mut names = Vec::new();
    let mut off = 0u64;
    while off < n as u64 {
        let reclen = mem.read_u16(0x13000 + off + 16).unwrap() as u64;
        let mut name = Vec::new();
        let mut i = 19;
        while let Ok(c) = mem.read_u8(0x13000 + off + i) {
            if c == 0 {
                break;
            }
            name.push(c);
            i += 1;
        }
        names.push(String::from_utf8(name).unwrap());
        off += reclen;
    }
    names.sort();
    assert_eq!(names, [".", "..", "n.bin"]);
    assert_eq!(
        proc.dispatch(GETDENTS64, [dfd, 0x13000, 4096, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0),
        "then end of directory"
    );
    // the scratch area is unchanged: /tmp stays writable
    assert!(open(&mut proc, &mut mem, &mut out, "/tmp/x", O_WRONLY | O_CREAT) >= 3);
    std::fs::remove_dir_all(&base).unwrap();
}

/// `Options::root` reaches the guest: a program reads a host file by its guest path, on every engine.
#[test]
fn run_elf_root_option_lets_a_guest_read_host_files() {
    let dir = std::env::temp_dir().join(format!("ounce_readfile_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("conf")).unwrap();
    std::fs::write(dir.join("conf/app.cfg"), "answer = 42\n").unwrap();
    for tier in TIERS {
        let mut cap = Capture::default();
        let opts = Options {
            tier,
            root: Some(dir.clone()),
            ..Options::default()
        };
        let argv = ["readfile".to_string(), "/conf/app.cfg".to_string()];
        let r = run(READFILE, &argv, &opts, &mut cap, &mut |_| {});
        assert_eq!(r.stop, Stop::Exited(0), "{tier:?}: {r:?}");
        assert_eq!(String::from_utf8_lossy(&cap.out), "answer = 42\n");
    }
    // without a root the same path does not exist
    let (r, out) = go(READFILE, &["readfile", "/conf/app.cfg"], CpuTier::Tier3);
    assert_eq!((r.stop, out.as_str()), (Stop::Exited(1), "open failed: -2\n"));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A static PIE (ET_DYN without PT_INTERP) is placed at a load base and runs there.
#[test]
fn pie_executables_load_at_a_base_address() {
    for t in TIERS {
        let (r, out) = go(PIE, &["pie"], t);
        assert_eq!(r.stop, Stop::Exited(0), "{t:?}: {r:?}");
        assert_eq!(out, "pie runs at its load base\n");
    }
}

/// PT_INTERP: the interpreter comes from `--root`, runs first at its own base with AT_BASE, AT_ENTRY and AT_PHDR
/// set, and jumps to the main program, as ld.so does.
#[test]
fn an_interpreter_from_the_root_starts_first_and_jumps_to_main() {
    let dir = std::env::temp_dir().join(format!("ounce_interp_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/fake-ld.so"), FAKELD).unwrap();
    for tier in TIERS {
        let mut cap = Capture::default();
        let opts = Options {
            tier,
            root: Some(dir.clone()),
            ..Options::default()
        };
        let r = run(DYNMAIN, &["dynmain".to_string()], &opts, &mut cap, &mut |_| {});
        assert_eq!(r.stop, Stop::Exited(0), "{tier:?}: {r:?}");
        assert_eq!(
            String::from_utf8_lossy(&cap.out),
            "ld: running at AT_BASE\nld: AT_ENTRY and AT_PHDR set\nmain: reached through the interpreter\n"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Without `--root` a dynamic executable cannot be started; the diagnosis names the interpreter it needs.
#[test]
fn a_dynamic_executable_without_root_names_its_interpreter() {
    let (r, _) = go(DYNMAIN, &["dynmain"], CpuTier::Tier3);
    let Stop::Failed { diagnosis, status } = r.stop else {
        panic!("{r:?}")
    };
    assert_eq!(status, 72);
    assert!(
        diagnosis.contains("/lib/fake-ld.so") && diagnosis.contains("--root"),
        "{diagnosis}"
    );
}

/// The guest is an unprivileged user with no network: it may only "change" to its own ids, it has no supplementary
/// groups, `socket` fails as for an unsupported family, and `sendfile` is EINVAL so callers copy with read/write.
#[test]
fn identity_socket_and_sendfile_answers() {
    use super::syscalls::{EAFNOSUPPORT, EINVAL, EPERM, GID, Outcome, Process, UID};
    use crate::mmu::Memory;
    let (mut proc, mut mem, mut out) = (Process::new(0x1000), Memory::new(), Capture::default());
    let mut call = |nr: u64, a: [u64; 6]| proc.dispatch(nr, a, &mut mem, &mut out);
    let keep = u32::MAX as u64;
    assert_eq!(call(146, [UID, 0, 0, 0, 0, 0]), Outcome::Return(0), "setuid to self");
    assert_eq!(call(146, [0, 0, 0, 0, 0, 0]), Outcome::Return(-EPERM), "setuid(0)");
    assert_eq!(call(144, [GID, 0, 0, 0, 0, 0]), Outcome::Return(0), "setgid to self");
    assert_eq!(
        call(147, [keep, UID, keep, 0, 0, 0]),
        Outcome::Return(0),
        "setresuid keeping two"
    );
    assert_eq!(
        call(149, [GID, 0, GID, 0, 0, 0]),
        Outcome::Return(-EPERM),
        "setresgid to root"
    );
    assert_eq!(call(158, [0, 0, 0, 0, 0, 0]), Outcome::Return(0), "getgroups: none");
    assert_eq!(
        call(198, [1, 0x80001, 0, 0, 0, 0]),
        Outcome::Return(-EAFNOSUPPORT),
        "socket(AF_UNIX)"
    );
    assert_eq!(call(71, [1, 3, 0, 1 << 24, 0, 0]), Outcome::Return(-EINVAL), "sendfile");
}

/// Two guest threads from a raw `clone` (musl's pthread flags plus CLONE_CHILD_SETTID): futex hand-off, 200,000
/// racing LDXR/STXR increments, TLS and gettid per thread, and a join through CLONE_CHILD_CLEARTID. The expected
/// output is what the same ELF prints under `qemu-aarch64` on Linux.
#[test]
fn guest_threads_share_memory_block_on_futexes_and_join() {
    let want = "distinct tids\nparent settid ok\nwait on a changed word: EAGAIN\njoined\ncounter=200000\ntls ok\n\
                gettid ok\nwake with no waiters: 0\n";
    for t in TIERS {
        let (r, out) = go(THREADS, &["threads"], t);
        assert_eq!(r.stop, Stop::Exited(0), "{t:?}: {r:?}\n{out}");
        assert_eq!(out, want, "{t:?}");
    }
}

/// File-backed `mmap` of scratch files (`tests/linux-elf/src/mapfile.c`): private copies, offsets, the zero tail,
/// PROT_NONE then `mprotect`, a read-only shared mapping, code run from a mapped file and replaced by a MAP_FIXED
/// mapping of another (the engines must drop the old translation), and the errors in Linux's order. The expected
/// output is what the same ELF prints under `qemu-aarch64` on Linux with stdout a pipe.
#[test]
fn mmap_of_a_file_maps_its_bytes_and_runs_its_code() {
    let want = [
        "private mapping has the file bytes",
        "tail page is zero past the end of the file",
        "a store to a private mapping leaves the file alone",
        "offset 4096 maps the second page",
        "PROT_NONE mapping keeps the bytes for mprotect",
        "read-only shared mapping",
        "writable shared mapping of a read-only fd: EACCES",
        "mapping a write-only fd: EACCES",
        "mapping write-only stdout: EACCES",
        "mapping a directory: ENODEV",
        "mapping a closed fd: EBADF",
        "unaligned offset: EINVAL",
        "neither MAP_SHARED nor MAP_PRIVATE: EINVAL",
        "code runs from an executable file mapping after its fd is closed",
        "a MAP_FIXED mapping of another file replaces the code",
    ]
    .map(|line| format!("{line}: ok\n"))
    .concat();
    for t in TIERS {
        let (r, out) = go(MAPFILE, &["mapfile"], t);
        assert_eq!(r.stop, Stop::Exited(0), "{t:?}: {r:?}\n{out}");
        assert_eq!(out, want, "{t:?}");
    }
}

/// A writable MAP_SHARED mapping of a file would need stores to reach the file: the run stops and says so rather
/// than handing out a private copy.
#[test]
fn writable_shared_file_mapping_is_reported_as_unsupported() {
    use super::syscalls::{Outcome, Process};
    use crate::mmu::{Memory, Permission};
    const AT_FDCWD: u64 = -100i64 as u64;
    let mut proc = Process::new(0x1000);
    let mut mem = Memory::new();
    mem.map_range(0x10000, 0x1000, Permission::READ_WRITE);
    mem.write_bytes(0x10000, b"/tmp/shared.bin\0").unwrap();
    let mut out = Capture::default();
    // openat(AT_FDCWD, path, O_RDWR | O_CREAT, 0644)
    let Outcome::Return(fd) = proc.dispatch(56, [AT_FDCWD, 0x10000, 0o102, 0o644, 0, 0], &mut mem, &mut out) else {
        panic!("openat")
    };
    assert!(fd >= 3, "{fd}");
    // mmap(0, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0)
    let Outcome::Unimplemented(why) = proc.dispatch(222, [0, 4096, 3, 1, fd as u64, 0], &mut mem, &mut out) else {
        panic!("a writable shared file mapping must not succeed")
    };
    assert!(why.contains("MAP_SHARED and PROT_WRITE"), "{why}");
    // Invalid len or MAP_FIXED address must return -EINVAL rather than aborting as Unimplemented.
    assert_eq!(
        proc.dispatch(222, [0, 0, 3, 1, fd as u64, 0], &mut mem, &mut out),
        Outcome::Return(-super::syscalls::EINVAL)
    );
    assert_eq!(
        proc.dispatch(222, [0x123, 4096, 3, 0x11, fd as u64, 0], &mut mem, &mut out),
        Outcome::Return(-super::syscalls::EINVAL)
    );
    // Negative off_t (-4096) overflows file offset; unknown MAP_SHARED_VALIDATE flag is EOPNOTSUPP.
    assert_eq!(
        proc.dispatch(222, [0, 4096, 1, 2, fd as u64, (-4096i64) as u64], &mut mem, &mut out),
        Outcome::Return(-super::syscalls::EOVERFLOW)
    );
    assert_eq!(
        proc.dispatch(222, [0, 4096, 1, 3 | 0x80000, fd as u64, 0], &mut mem, &mut out),
        Outcome::Return(-super::syscalls::EOPNOTSUPP)
    );
}

#[test]
fn file_mmap_populates_full_page_for_unaligned_len_handles_host_root_and_umask() {
    use super::syscalls::{Outcome, Process};
    use crate::mmu::{Memory, Permission};
    const AT_FDCWD: u64 = -100i64 as u64;
    let base = std::env::temp_dir().join(format!("ounce_mmap_host_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("lib")).unwrap();
    let mut payload = vec![0x5Au8; 4096];
    payload[0] = 0x11;
    payload[2000] = 0x77;
    std::fs::write(base.join("lib/sample.bin"), &payload).unwrap();

    let mut proc = Process::new(0x1000);
    proc.set_host_root(&base).unwrap();
    let mut mem = Memory::new();
    mem.map_range(0x10000, 0x2000, Permission::READ_WRITE);
    let mut out = Capture::default();

    // umask defaults to 0o022, returns previous mask, and masks newly created scratch file modes.
    assert_eq!(
        proc.dispatch(166, [0o077, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0o022)
    );
    assert_eq!(
        proc.dispatch(166, [0o027, 0, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0o077)
    );
    mem.write_bytes(0x10000, b"/tmp/masked.bin\0").unwrap();
    let Outcome::Return(sfd) = proc.dispatch(56, [AT_FDCWD, 0x10000, 0o102, 0o666, 0, 0], &mut mem, &mut out) else {
        panic!("openat scratch")
    };
    assert!(sfd >= 3);
    assert_eq!(
        proc.dispatch(80, [sfd as u64, 0x11000, 0, 0, 0, 0], &mut mem, &mut out),
        Outcome::Return(0)
    );
    let st_mode = mem.read_u32(0x11000 + 16).unwrap();
    assert_eq!(st_mode & 0o777, 0o666 & !0o027);

    // Mapping 100 bytes of a 4096-byte --root file must populate the full 4 KiB page (like Linux do_mmap).
    mem.write_bytes(0x10000, b"/lib/sample.bin\0").unwrap();
    let Outcome::Return(hfd) = proc.dispatch(56, [AT_FDCWD, 0x10000, 0, 0, 0, 0], &mut mem, &mut out) else {
        panic!("openat host")
    };
    assert!(hfd >= 3);
    let Outcome::Return(addr) = proc.dispatch(222, [0, 100, 1, 2, hfd as u64, 0], &mut mem, &mut out) else {
        panic!("mmap host")
    };
    assert!(addr > 0);
    let mut first = [0u8; 1];
    let mut mid = [0u8; 1];
    mem.read_bytes(addr as u64, &mut first).unwrap();
    mem.read_bytes(addr as u64 + 2000, &mut mid).unwrap();
    assert_eq!(first[0], 0x11);
    assert_eq!(mid[0], 0x77);
    let _ = std::fs::remove_dir_all(&base);
}

/// A futex wait that no thread can ever wake hangs on Linux; the runner stops it with a diagnosis instead.
#[test]
fn futex_wait_with_no_thread_left_to_wake_it_is_a_diagnosed_deadlock() {
    // ldr w2, [sp]; mov x0, sp; mov x1, #FUTEX_WAIT_PRIVATE; mov x8, #SYS_futex; svc #0
    let elf = tiny_elf(&[0xB940_03E2, 0x9100_03E0, 0xD280_1001, 0xD280_0C48, 0xD400_0001]);
    for t in TIERS {
        let (r, _) = go(&elf, &["deadlock"], t);
        let Stop::Failed { diagnosis, .. } = r.stop else {
            panic!("expected a diagnosed failure: {r:?}")
        };
        assert!(diagnosis.contains("deadlock"), "{t:?}: {diagnosis}");
    }
}

#[test]
fn nanosleep_moves_the_deterministic_clock_instead_of_waiting() {
    use super::syscalls::{EINVAL, Outcome, Process, SYS_CLOCK_NANOSLEEP, SYS_NANOSLEEP};
    use crate::mmu::{Memory, Permission};
    const SYS_CLOCK_GETTIME: u64 = 113;
    let mut proc = Process::new_deterministic(0x1000);
    let mut mem = Memory::new();
    mem.map_range(0x1000, 0x1000, Permission::READ_WRITE);
    let mut out = Capture::default();
    let mut now = |proc: &mut Process, mem: &mut Memory| {
        assert_eq!(
            proc.dispatch(SYS_CLOCK_GETTIME, [1, 0x1100, 0, 0, 0, 0], mem, &mut out),
            Outcome::Return(0)
        );
        mem.read_u64(0x1100).unwrap() * 1_000_000_000 + mem.read_u64(0x1108).unwrap()
    };
    let before = now(&mut proc, &mut mem);
    // 1.5 s: the monotonic clock jumps by at least that much, with no host wait
    mem.write_u64(0x1000, 1).unwrap();
    mem.write_u64(0x1008, 500_000_000).unwrap();
    let mut out2 = Capture::default();
    assert_eq!(
        proc.dispatch(SYS_NANOSLEEP, [0x1000, 0, 0, 0, 0, 0], &mut mem, &mut out2),
        Outcome::Return(0)
    );
    let after = now(&mut proc, &mut mem);
    assert!(after - before >= 1_500_000_000, "{before} -> {after}");
    // TIMER_ABSTIME in the past returns at once; a tv_nsec of 10^9 is EINVAL
    mem.write_u64(0x1000, 0).unwrap();
    mem.write_u64(0x1008, 0).unwrap();
    assert_eq!(
        proc.dispatch(SYS_CLOCK_NANOSLEEP, [1, 1, 0x1000, 0, 0, 0], &mut mem, &mut out2),
        Outcome::Return(0)
    );
    let past = now(&mut proc, &mut mem);
    assert!(past - after < 10_000_000, "an absolute time in the past does not sleep");
    mem.write_u64(0x1008, 1_000_000_000).unwrap();
    assert_eq!(
        proc.dispatch(SYS_NANOSLEEP, [0x1000, 0, 0, 0, 0, 0], &mut mem, &mut out2),
        Outcome::Return(-EINVAL)
    );
}
