# Linux ELF runner (`ounce-core run-elf`)

Vision step 1: run **static AArch64 Linux ELF executables** on the Tier-3 or Tier-4 engine, and when a
program cannot run, say exactly why.

```
ounce-core run-elf <elf> [--cpu=tier3|tier4] [--max-insns=N] [--trace-syscalls] [--deterministic] [-- guest args...]
```

Exit status is the guest's `exit_group` code (low 8 bits). Failures use: 70 unimplemented syscall or
non-Linux SVC, 71 instruction/memory fault, 72 ELF rejected, 73 instruction budget exhausted. A
`run-elf: DIAGNOSIS: ...` line on stderr names the syscall number (with args) or the instruction word.

Maturity: **research-only** as a general Linux runner. Six tiny freestanding programs (including a deterministic clock fixture) and the unchanged pinned LZ4 harness linked with musl have been executed. The LZ4 result is bounded to its listed cases; it does not establish general libc compatibility.

## What runs

- ELF64 little-endian AArch64 `ET_EXEC`; `PT_LOAD` mapping; stack with argc, argv, empty envp, auxv
  (PHDR, PHENT, PHNUM, PAGESZ, ENTRY, UID/GID, HWCAP, CLKTCK, SECURE, RANDOM, PLATFORM, EXECFN).
- Syscalls (via `SVC #0`, number in x8): `write`, `writev` (fd 1 and 2 only), `exit`, `exit_group`,
  `brk`, `mmap` (anonymous only, `MAP_FIXED` allowed in the mmap window), `munmap`, `ioctl` (returns
  ENOTTY), `set_tid_address`, `rt_sigaction`/`rt_sigprocmask` (accept and ignore), `getpid`,
  `clock_gettime` (realtime, monotonic), `uname`, `openat`/`readlinkat` (always ENOENT).
- Test programs: `tests/linux-elf/src/*.c`, built by `tests/linux-elf/src/build.sh` (clang plus LLD,
  or rustup's `rust-lld`) into `tests/linux-elf/bin/` (each ELF is about 1-2 KB). Tests run each
  program on `tier3` and `tier4` (Tier-4 falls back to Tier-3 without `--features native-jit`).

## Honest limits

- **Only one libc-linked library harness verified so far.** The LZ4/musl case below runs; other static glibc/musl binaries may stop early:
  they call `mprotect`, `getrandom`, `prlimit64`, `rseq`, `futex`, `set_robust_list`, `readlinkat`
  paths, etc. Each stops with a diagnosis naming the syscall; none is implemented yet.
- Dynamic executables, `ET_DYN`/static-PIE, 32-bit and big-endian ELF are rejected. `PT_TLS` is not
  set up (libc startup is expected to do it).
- No files, `read`, sockets, threads, `clone`, `execve`, signals delivery, or `fork`.
- Segment pages are RW or RWX only: read-only data is not write-protected, and no W^X.
- Memory is allocated eagerly: `brk` growth is capped at 512 MiB, one `mmap` at 512 MiB, total
  anonymous mappings at 1 GiB. Environment is empty; `AT_RANDOM` is fixed bytes (deterministic).
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
The work directory must be new; it retains sources, binaries, compiler identities,
commands, execution output and `report.json`. Source fetch uses the existing
manifest commit. `--upstream-dir` accepts a clean local checkout of that exact
commit. Each subprocess has a deadline (`--seconds`, default 300); the guest also
has an instruction budget. Failure, byte differences and unconfirmed/fallback
engines produce nonzero exits with retained evidence. An existing work directory
is refused rather than overwritten.

The [first Linux hosted job](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42523091575) passed. The differential workflow now has a Linux/macOS/Windows matrix; macOS and Windows qualification remains pending until its actual runs are inspected. Host stdout uses binary mode on Windows so byte comparisons retain LF rather than CRT CRLF translation.
No LZ4, libc or runtime binary is redistributed with the checked-in evidence.

## Deterministic clock mode

`run-elf ... --deterministic` makes `clock_gettime` reproducible. `CLOCK_REALTIME`
starts at Unix second 1700000000; MONOTONIC, MONOTONIC_RAW and BOOTTIME start at
zero. The clocks share a tick counter and advance by one millisecond after each
successful clock read. Invalid clocks and bad output pointers return EINVAL/EFAULT
without consuming a tick. Each process run starts over; instruction counts and
host scheduling do not affect virtual time. This is a test clock, not a model of
hardware time, signal delivery or scheduler timing. Default mode keeps host time.

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
