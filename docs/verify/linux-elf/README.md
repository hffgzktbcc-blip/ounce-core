# Linux ELF runner (`ounce-core run-elf`)

Vision step 1: run **static AArch64 Linux ELF executables** on the Tier-3 or Tier-4 engine, and when a
program cannot run, say exactly why.

```
ounce-core run-elf <elf> [--cpu=tier3|tier4] [--max-insns=N] [--trace-syscalls] [--deterministic] [-- guest args...]
```

Exit status is the guest's `exit_group` code (low 8 bits). Failures use: 70 unimplemented syscall or
non-Linux SVC, 71 instruction/memory fault, 72 ELF rejected, 73 instruction budget exhausted. A
`run-elf: DIAGNOSIS: ...` line on stderr names the syscall number (with args) or the instruction word.

Maturity: **research-only** as a general Linux runner. Six tiny freestanding programs (including a deterministic clock fixture) and 18 unchanged pinned real programs linked with musl (LZ4, xxHash, cJSON, zlib and the ones in [PROGRAMS.md](PROGRAMS.md)) have been executed; wasm3 does not run yet (#366). Each result is bounded to its listed cases; it does not establish general libc compatibility.

## What runs

- ELF64 little-endian AArch64 `ET_EXEC`; `PT_LOAD` mapping; stack with argc, argv, empty envp, auxv
  (PHDR, PHENT, PHNUM, PAGESZ, ENTRY, UID/GID, HWCAP, CLKTCK, SECURE, RANDOM, PLATFORM, EXECFN).
- Syscalls (via `SVC #0`, number in x8): `write`, `writev` (fd 1 and 2 only), `exit`, `exit_group`,
  `brk`, `mmap` (anonymous only, including sparse reservations and `MAP_FIXED`), `mprotect`, `munmap`, `mremap` (anonymous mappings; shrink in place, grow only with `MREMAP_MAYMOVE`), `ioctl` (returns
  ENOTTY), `set_tid_address`, `rt_sigaction`/`rt_sigprocmask`, `sigaltstack`, `rt_sigreturn`, `getpid`/`gettid`,
  `getrlimit`/`prlimit64` (read-only stack limit),
  `clock_gettime` (realtime, monotonic), `uname`, `openat`/`readlinkat` (always ENOENT),
  `sched_getaffinity` (one CPU: an 8-byte mask with CPU 0 set).
- Test programs: `tests/linux-elf/src/*.c`, built by `tests/linux-elf/src/build.sh` (clang plus LLD,
  or rustup's `rust-lld`) into `tests/linux-elf/bin/` (each ELF is about 1-2 KB). Tests run each
  program on `tier3` and `tier4` (Tier-4 falls back to Tier-3 without `--features native-jit`).

## Honest limits

- **18 libc-linked programs verified so far** (LZ4, xxHash, cJSON, zlib; below, and [PROGRAMS.md](PROGRAMS.md)); wasm3 fails (#366). Other static glibc/musl binaries may stop early:
  they call `mprotect`, `getrandom`, `prlimit64`, `rseq`, `futex`, `set_robust_list`, `readlinkat`
  paths, etc. Each stops with a diagnosis naming the syscall; none is implemented yet.
- Dynamic executables, `ET_DYN`/static-PIE, 32-bit and big-endian ELF are rejected. `PT_TLS` is not
  set up (libc startup is expected to do it).
- No files, `read`, sockets, threads, `clone`, `execve`, asynchronous signals, or `fork`. Synchronous memory faults can deliver a guest
  SIGSEGV handler (including an alternate stack and `rt_sigreturn`).
- Segment pages are RW or RWX only: read-only data is not write-protected, and no W^X.
- Accessible memory is allocated eagerly: `brk` growth is capped at 512 MiB, one accessible
  `mmap`/`mprotect` at 512 MiB, and backed anonymous pages at 1 GiB. `PROT_NONE` reservations
  use virtual ranges without page buffers; `mprotect` commits bounded portions, and `munmap`
  or fixed anonymous mappings trim/replace them. Environment is empty; `AT_RANDOM` is fixed bytes (deterministic).
- Time: `clock_gettime` reads the host clock by default. `--deterministic` supplies a fixed virtual clock (below), not physical time.
- `--cpu=native` is rejected. Missing-instruction failures belong to the decoder/interpreter; this
  runner only reports them (instruction word plus the interpreter's error).
- Instruction budget default is 2 billion; the interpreter-class speed is about 20 MIPS.

Arguments after `--` are passed unchanged to the guest, including values that look
like host options (`--cpu=`, `--present=`, `--sdmc=`). Host options belong before
the separator.

Rebuild test ELFs: `tests/linux-elf/src/build.sh`. Run: `cargo test --release --bin ounce-core linux::`.

## LZ4 without a console SDK

The existing `lz4_cases.c` and host `main` run unchanged as a static AArch64 Linux
ELF. A fresh x86-64 Linux host build and both Tier-3 and actual Tier-4 produced
identical 406-byte output (`fails=0`), each exiting 0 after 3,862,822 instructions
and five syscalls. [Recorded report and logs](lz4-2026-11-08/report.json) identify
source files, pinned LZ4 commit, compilers, ELF and runtime hashes. The working
tree was dirty only for the new verification scripts; no runtime source repair
was required. This is an internal rerun, not outside reproduction.

Reproduce with Rust, Python, a host C compiler and Zig (tested: Zig 0.17.1, whose
Python distribution is `ziglang==0.17.1`):

```sh
cargo build --release --locked --features native-jit
python3 scripts/verify_linux_elf.py --work target/linux-lz4 \
  --elf-cc 'zig cc -target aarch64-linux-musl'
```

Alternatively use `--elf-cc 'python3 -m ziglang cc -target aarch64-linux-musl'`.
`--harness NAME` runs another harness from `docs/verify/differential.json` (default `lz4`).
The work directory must be new; it retains sources, binaries, compiler identities,
commands, execution output and `report.json`. Source fetch uses the existing
manifest commit. `--upstream-dir` accepts a clean local checkout of that exact
commit. Each subprocess has a deadline (`--seconds`, default 300); the guest also
has an instruction budget. Failure, byte differences and unconfirmed/fallback
engines produce nonzero exits with retained evidence. An existing work directory
is refused rather than overwritten.

The [three-host qualification](three-host-2026-11-08.json) passed on Linux, macOS and Windows: both actual engines match the fresh host build and all five CLI scenarios pass on each host. This covers separately built versions of the same pinned cases, not one transported ELF or arbitrary libc programs. Host stdout uses binary mode on Windows so byte comparisons retain LF rather than CRT CRLF translation.
No LZ4, libc or runtime binary is redistributed with the checked-in evidence.

## xxHash, cJSON and zlib (2026-11-09)

The same script with `--harness xxhash`, `cjson` and `zlib` runs the unchanged
pinned cases as static musl ELFs. Before this change, xxHash and cJSON stopped at startup on
syscall 123 (`sched_getaffinity`, asked by musl with a 128-byte buffer); the
[before reports](xxhash-2026-11-09/before/report.json) keep that diagnosis. With
the one-CPU `sched_getaffinity` reply, all three match a fresh x86-64 host build on
actual Tier-3 and Tier-4 and exit 0. zlib needs `--budget 2000000000` (it runs
about 314 million instructions; the script default is 100 million):

| harness | output bytes | Tier-3 instructions | syscalls | report |
|---|---|---|---|---|
| xxHash `2ec67ca` | 3,444 | 247,981 | 11 | [report](xxhash-2026-11-09/report.json) |
| cJSON `c859b25` (`-lm`) | 7,025 | 42,830,459 | 63 | [report](cjson-2026-11-09/report.json) |
| zlib `51b7f2a` | 5,190 | 313,690,936 | 787 | [report](zlib-2026-11-09/report.json) |

LZ4 was rerun as a control with the generalized script and still matched. Linux
x86-64 host only (Zig 0.17.1 musl); macOS and Windows were not rerun for these
three. Monocypher was not tried: its tarball host was not reachable from the test
machine. Internal run, not outside reproduction; no binaries are checked in.

## Deterministic clock mode

`run-elf ... --deterministic` makes `clock_gettime` reproducible. `CLOCK_REALTIME`
starts at Unix second 1700000000; MONOTONIC, MONOTONIC_RAW and BOOTTIME start at
zero. The clocks share a tick counter and advance by one millisecond after each
successful clock read. Invalid clocks and bad output pointers return EINVAL/EFAULT
without consuming a tick. Each process run starts over; instruction counts and
host scheduling do not affect virtual time. This is a test clock, not a model of
hardware time, asynchronous signal delivery or scheduler timing. Default mode keeps host time.

```sh
ounce-core run-elf tests/linux-elf/bin/clock.elf --deterministic --cpu=tier3
ounce-core run-elf tests/linux-elf/bin/clock.elf --deterministic --cpu=tier4
```

Both commands should print:

```text
realtime=1700000000:0
monotonic=0:1000000
raw=0:2000000
boottime=0:3000000
```

The fixture is project-owned freestanding C; rebuild with the fixture build script.
The implementation does not provide full record/replay or guarantee deterministic
behavior for arbitrary Linux applications.


## wasm3 guarded memory (#366)

See [the reproduction and evidence](wasm3-366.md). The unchanged pinned wasm3
sources reserve `(8 GiB + 2*4096) * 128 = 0x10000100000` bytes for guarded
Wasm memories, using `PROT_NONE | MAP_NORESERVE`; this is intentional virtual
address space, not a malloc size corruption. Linux ELF verification runs wasm3
in the Linux CI qemu-user matrix. Its `aarch64` reference policy uses a real ARM64
host directly or requires `--qemu` on other host architectures, preserving the
architecture's default-NaN sign rather than normalizing output.
