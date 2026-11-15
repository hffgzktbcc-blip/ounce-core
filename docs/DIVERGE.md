# `ounce-core diverge`: find disagreements across engines and host CPUs

```sh
ounce-core diverge <program.nro|program.elf> [--budget N] [--engine interpreter,tier3,tier4] [--json report.json] [-- program_args...]
```

`diverge` auto-detects Switch homebrew `.nro` programs (`NRO0` magic at offset `0x10`) and static
64-bit AArch64 Linux ELFs (`\x7fELF` magic at offset `0`, with `.nro`/`.elf` extension fallback).

Exit status: `0` no disagreement in what was checked, `1` a disagreement was found, `2` it could not run.

## `.nro` mode (per-instruction hardware replay)

For `.nro` targets, `diverge` runs the program on the interpreter, then on Tier-3 and Tier-4 (the
default; `--engine` picks a subset), and records the registers before and after a sample of its
register-only instructions (integer, floating-point and SIMD data processing). A small C helper
(`tools/hwcheck/hwcheck.c`, built with the host's `cc`) then replays each recorded instruction on this
machine's real AArch64 CPU from the same starting registers. Any difference in a register, the flags,
or the saturation flag is reported with the program counter, the opcode, and the two values.

- **Sampling**: the first four executions of every distinct opcode, then every power-of-two occurrence.
  The first execution of every opcode the program uses is therefore always checked.
- **Host requirement**: only AArch64 hosts can replay (an Apple Silicon Mac or a Linux ARM64 machine);
  `.nro` mode exits with `2` on non-AArch64 hosts.

## Static AArch64 Linux ELF mode (cross-engine + optional Linux AArch64 host)

For static AArch64 Linux ELF targets, `diverge` drives `ounce-core run-elf` with `--deterministic`
and `--max-insns=<budget>` across `interpreter`, `Tier-3`, and `Tier-4` (plus any `[program_args...]`
passed after the ELF path or after `--`), and compares:

- Process exit codes
- Guest `stdout` and filtered guest `stderr` (with byte-offset diff reporting on mismatch)
- Stop kind and `[run-elf]` diagnosis (`exited`, `budget`, `unhandled-svc`, `unimplemented`, `fault`, `loader`)
- Silent Tier-4 fallback detection via `[run-elf] engine=<name>`

Unlike `.nro` mode, ELF mode does **not** perform per-instruction `OUNCE_TRACE` / `hwcheck` replay;
it checks end-to-end deterministic run equivalence across engines.

- **On a Linux AArch64 host**: `diverge` also executes the ELF natively as a host reference and
  checks exit code, `stdout`, and `stderr` against the emulated reference run.
- **On macOS or non-AArch64 Linux/other hosts**: macOS cannot directly execute Linux ELF binaries,
  so native host execution is never faked; instead `host (Linux AArch64)` is reported as
  `NOT CHECKED` in both stdout and the JSON `not_checked` array while `interpreter`, `Tier-3`, and
  (when available) `Tier-4` are still compared against one another. Because at least two executions
  are needed to compare for divergence, running with a single `--engine` on a non-Linux-AArch64 host
  exits with `2`.

## What `diverge` does not show

- The JSON report keeps top-level `checked`, `mismatched`, and `mismatches`, and adds `mode`,
  `engines`, and `not_checked` (plus `reference` in ELF mode).
- In `.nro` mode: loads, stores, branches, system registers, `x18`, `sp` operands, and pointer
  authentication are not replayed on `hwcheck`, and Tier-3/Tier-4 run in one-instruction slices so
  multi-instruction block fusion is not checked.
- The default set reports Tier-4 as `NOT CHECKED`, not as a pass, if this build lacks
  `--features native-jit` or the host cannot run it. Passing `--engine tier4` makes that an error
  (exit `2`).
- In ELF mode, `--deterministic` fixes guest clocks and `brk`/`mmap`/auxv randomness so engines agree
  deterministically; programs inspecting real wall-clock time may differ from a native Linux host run
  on Linux AArch64.
- One run, one input. It is not a proof that the emulator is correct.

The instruction checker itself is tested by corrupting a recorded result and confirming it is caught.
