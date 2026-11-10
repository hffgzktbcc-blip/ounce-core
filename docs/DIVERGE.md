# `ounce-core diverge`: the first instruction where Ounce-Core and your CPU disagree

```sh
ounce-core diverge program.nro [--budget N] [--engine interpreter,tier3,tier4] [--json report.json]
```

It runs the program on the interpreter, then on Tier-3 and Tier-4 (the default; `--engine` picks a subset), and records the registers before and after a sample of its
register-only instructions (integer, floating-point and SIMD data processing). A small C helper
(`tools/hwcheck/hwcheck.c`, built with the host's `cc`) then replays each recorded instruction on this
machine's real CPU from the same starting registers. Any difference in a register, the flags or the
saturation flag is reported with the program counter, the opcode and the two values.

Exit status: `0` no disagreement in what was checked, `1` a disagreement was found, `2` it could not run.

## What it shows
- A disagreement is a specific instruction, operands and register, not just "the output differs".
- Sampling is by opcode: the first four executions of every distinct opcode, then every power-of-two
  occurrence. The first execution of every opcode the program uses is therefore always checked.

## What it does not show
- Only AArch64 hosts can replay (an Apple Silicon Mac or a Linux ARM64 machine). The JSON report keeps the top-level `checked`/`mismatched`/`mismatches` and adds `engines` and `not_checked`.
- Loads, stores, branches, system registers, `x18`, `sp` operands and pointer authentication are not
  replayed, because the host cannot run them in isolation. A clean result says nothing about them.
- Tier-3 and Tier-4 run in one-instruction slices so each traced instruction goes through that engine's own
  translation. Multi-instruction block fusion and cross-instruction optimisation are not checked.
  When a tier executes more than the traced instruction in one slice (Tier-4 falls back to Tier-3 blocks
  for instructions it does not translate), that sample is skipped and counted in the `[Trace]` line.
- The default set reports Tier-4 as NOT CHECKED, not as a pass, if this build lacks `--features native-jit`
  or the host cannot run it. `--engine tier4` makes that an error (exit 2). Build with
  `--features native-jit` to cover Tier-4.
- One run, one input. It is not a proof that the emulator is correct.

The checker itself is tested by corrupting a recorded result and confirming it is caught.
