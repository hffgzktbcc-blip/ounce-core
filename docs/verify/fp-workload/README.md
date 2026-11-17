# FP edge-case program workload (#359)

Complements [`../fp-edge/`](../fp-edge/README.md), the existing instruction-level experiment (one instruction per case, interpreter/Tier-3/Tier-4, CI on Linux and Apple Silicon). This folder runs a whole program: one pinned C source as a static ELF through `run-elf`, compared with the same source on the host.

One pinned C source, [fp_edge.c](fp_edge.c), built twice: natively on an ARM64 host (the reference) and as a
freestanding static AArch64 Linux ELF for `ounce-core run-elf` on Tier-3 and Tier-4. Both print one line per case with
raw hex bits: `op fpcr-nibble operands = result fpsr`. Every op runs as its own process (`argv[1]`), so an engine that
stops on one op cannot hide the others.

Cases: 18 fixed bit patterns per precision (signed zeros, smallest/largest subnormal, smallest normal, max finite,
infinities, quiet/signalling/negative-payload NaNs, 0.1, 2^63, 2^53) x 8 FPCR values (default, FZ, DN, FZ+DN, RMode
RP/RM/RZ, FZ+DN+RZ) x all operand pairs (2,592 cases per binary op, 144 per unary op). FPSR is cleared before each case
and printed masked to `0x9f` (IOC, DZC, OFC, UFC, IXC, IDC, QC). 47 ops: arithmetic, min/max(NM), fused multiply-add,
square root, rounding, converts to and from integer and between precisions, compare.

```sh
docs/verify/fp-workload/run.sh <new out dir>      # from the repo root; ARM64 host, clang, rust-lld or ld.lld
```

## Result (2026-11-17, Apple M3, macOS, ounce-core 008e3dd, Apple clang 21.0.0)

[summary.tsv](result-2026-11-17/summary.tsv), [toolchain and hashes](result-2026-11-17/).

| | Tier-3 | Tier-4 |
|---|---|---|
| ops byte-identical to the host (value and FPSR, every case) | 39 | 39 |
| ops UNSUPPORTED (engine stops on an unimplemented instruction) | 8 | 8 |
| ops that ran and differed (MISMATCH) | 0 | 0 |

Unsupported scalar instructions (exit 71, `Unimplemented instruction`), identical on both tiers: `FRECPS`, `FRSQRTS`,
`FRECPE`, `FRSQRTE` (double: `0x5e61fc02`, `0x5ee1fc02`, `0x5ee1d802`, `0x7ee1d802`; single `FRECPE`/`FRSQRTE`:
`0x5ea1d802`, `0x7ea1d802`) and `FRINT32Z`, `FRINT64X` (`0x1e684002`, `0x1e69c002`). These are reported as
unsupported, not failed and not passed. [`../fp-edge/`](../fp-edge/README.md) already lists the `FRECPE/FRECPS/FRSQRTE/FRSQRTS`
family as unsupported on every engine; this run adds the scalar `FRINT32Z` and `FRINT64X` forms and shows the same gap at whole-program level.

## Checks on the harness itself

- Negative control: a host reference with one result edited is flagged by the comparison (`cmp`), and the reported line is the edited one.
- The FPCR modes change the observable result, so they are exercised: `fadd.d` of the smallest subnormal and 1.0 gives
  FPSR `0x10` (inexact) at default FPCR and `0x80` (input denormal) under FZ, on host and both tiers.
- FPSR values seen on the host across all ops: 00, 01, 10, 80, 81, 18, 14, 02 and others.

## Limits (not claimed)

- One host (Apple M3), one run, by the maintainers' tooling: this is **not** an independent reproduction (#248 still open).
- The reference is the native instruction; "agree" means identical bits and FPSR for these inputs only, not general
  correctness, and not FP16/FZ16, trap enables, FPCR bits other than FZ/DN/RMode, or the interpreter engine
  (`run-elf` offers Tier-3 and Tier-4 only; interpreter coverage is the fuzzer's).
- Per-op outputs (about 3 MB) are not committed; `SHA256SUMS` in the result folder pins the ELF, the host binary and every
  per-op host output so a rerun can be checked against them. Rebuilt binaries are not bit-for-bit pinned across toolchains.
