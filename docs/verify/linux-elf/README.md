# Linux ELF runner (`ounce-core run-elf`)

Vision step 1: run **static AArch64 Linux ELF executables** on the Tier-3 or Tier-4 engine, and when a
program cannot run, say exactly why.

```
ounce-core run-elf <elf> [--cpu=tier3|tier4] [--max-insns=N] [--trace-syscalls] [-- guest args...]
```

Exit status is the guest's `exit_group` code (low 8 bits). Failures use: 70 unimplemented syscall or
non-Linux SVC, 71 instruction/memory fault, 72 ELF rejected, 73 instruction budget exhausted. A
`run-elf: DIAGNOSIS: ...` line on stderr names the syscall number (with args) or the instruction word.

Maturity: **research-only**. Tested on five tiny freestanding programs, not on real libc binaries.

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

- **No libc programs verified.** Real static glibc/musl binaries will almost certainly stop early:
  they call `mprotect`, `getrandom`, `prlimit64`, `rseq`, `futex`, `set_robust_list`, `readlinkat`
  paths, etc. Each stops with a diagnosis naming the syscall; none is implemented yet.
- Dynamic executables, `ET_DYN`/static-PIE, 32-bit and big-endian ELF are rejected. `PT_TLS` is not
  set up (libc startup is expected to do it).
- No files, `read`, sockets, threads, `clone`, `execve`, signals delivery, or `fork`.
- Segment pages are RW or RWX only: read-only data is not write-protected, and no W^X.
- Memory is allocated eagerly: `brk` growth is capped at 512 MiB, one `mmap` at 512 MiB, total
  anonymous mappings at 1 GiB. Environment is empty; `AT_RANDOM` is fixed bytes (deterministic).
- Time: `clock_gettime` reads the host clock, so output depending on it is not reproducible.
- `--cpu=native` is rejected. Missing-instruction failures belong to the decoder/interpreter; this
  runner only reports them (instruction word plus the interpreter's error).
- Instruction budget default is 2 billion; the interpreter-class speed is about 20 MIPS.

Rebuild test ELFs: `tests/linux-elf/src/build.sh`. Run: `cargo test --release --bin ounce-core linux::`.
