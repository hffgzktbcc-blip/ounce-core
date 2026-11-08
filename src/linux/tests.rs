use super::runner::{run, Options, Report, Stop};
use super::syscalls::Output;
use crate::cpu::CpuTier;

const HELLO: &[u8] = include_bytes!("../../tests/linux-elf/bin/hello.elf");
const EXITCODE: &[u8] = include_bytes!("../../tests/linux-elf/bin/exitcode.elf");
const ARGS: &[u8] = include_bytes!("../../tests/linux-elf/bin/args.elf");
const MEMORY: &[u8] = include_bytes!("../../tests/linux-elf/bin/memory.elf");
const BADSYS: &[u8] = include_bytes!("../../tests/linux-elf/bin/badsyscall.elf");

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

const TIERS: [CpuTier; 2] = [CpuTier::Tier3, CpuTier::Tier4];

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
