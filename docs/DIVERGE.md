# `ounce-core diverge`: the first instruction where Ounce-Core and your CPU disagree

```sh
ounce-core diverge program.nro [--budget N] [--json report.json]
```

It runs the program once on the interpreter and records the registers before and after a sample of its
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
- Only AArch64 hosts can replay (an Apple Silicon Mac or a Linux ARM64 machine).
- Loads, stores, branches, system registers, `x18`, `sp` operands and pointer authentication are not
  replayed, because the host cannot run them in isolation. A clean result says nothing about them.
- It follows the one-instruction interpreter. Tier-3 and Tier-4 are compared by their output, as before.
- One run, one input. It is not a proof that the emulator is correct.

The checker itself is tested by corrupting a recorded result and confirming it is caught.
