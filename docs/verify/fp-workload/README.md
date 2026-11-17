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

## Results (Apple M3, macOS, Apple clang 21.0.0)

**Before** the scalar change (base 008e3dd): [summary.tsv](result-2026-11-17/summary.tsv). Tier-3 and Tier-4: 39 ops identical, 8 ops
UNSUPPORTED (engine stops with exit 71, `Unimplemented instruction`), 0 mismatches. The 8 were scalar `FRECPS`, `FRSQRTS`,
`FRECPE`, `FRSQRTE` (double: `0x5e61fc02`, `0x5ee1fc02`, `0x5ee1d802`, `0x7ee1d802`; single `FRECPE`/`FRSQRTE`: `0x5ea1d802`,
`0x7ea1d802`), `FRINT32Z` and `FRINT64X` (`0x1e684002`, `0x1e69c002`).

**After** implementing those scalar forms (this folder's PR; [summary.tsv](result-2026-11-17-after-scalar-recip/summary.tsv)):
all 47 ops are byte-identical to the host on Tier-3 and Tier-4, every case (value and FPSR); 0 unsupported, 0 mismatches.
The first attempt after the change showed single-precision `FRECPE`/`FRSQRTE` with matching values but a spurious
divide-by-zero flag (an unread lane evaluated as zero); that was fixed by replicating lane 0, and the table above is the
rerun. The instruction fuzzer agrees (host vs interpreter, 8 seeds x 400,000 words, all 8 FPCR values): scalar
FRECPE/FRSQRTE 3,200,000 executed, scalar FRECPS/FRSQRTS 3,200,000, scalar FRINT32Z/32X/64Z/64X 1,801,223 (the host
rejects the other words); 0 value mismatches, 0 flag-only differences, 0 unimplemented.

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
