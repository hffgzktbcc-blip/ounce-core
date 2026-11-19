# FP edge-case program workload (#359, extended by #488)

Complements [`../fp-edge/`](../fp-edge/README.md), the existing instruction-level experiment (one instruction per case, interpreter/Tier-3/Tier-4, CI on Linux and Apple Silicon). This folder runs a whole program: one pinned C source as a static ELF through `run-elf`, compared with the same source on the host.

One pinned C source, [fp_edge.c](fp_edge.c), built twice: natively on an ARM64 host (the reference) and as a
freestanding static AArch64 Linux ELF for `ounce-core run-elf` on Tier-3 and Tier-4. Both print one line per case with
raw hex bits: `op fpcr-nibble operands = result fpsr`. Every op runs as its own process (`argv[1]`), so an engine that
stops on one op cannot hide the others.

Cases: 18 fixed bit patterns per precision (signed zeros, smallest/largest subnormal, smallest normal, max finite,
infinities, quiet/signalling/negative-payload NaNs, 0.1, 2^63, 2^53) x 8 FPCR values (default, FZ, DN, FZ+DN, RMode
RP/RM/RZ, FZ+DN+RZ) x all operand pairs (2,592 cases per binary op, 144 per unary op). FPSR is cleared before each case
and printed masked to `0x9f` (IOC, DZC, OFC, UFC, IXC, IDC, QC). 211 ops (`fp_edge_host --list`):

- the original 48 (#359; the text and tables below that say 47 are the historical wording, the committed `summary.tsv` files
  have 48 rows): scalar arithmetic, min/max(NM), fused multiply-add, square root, rounding, converts to and from integer and
  between precisions, compare;
- 39 more scalar converts (#488): every `FCVT{N,M,P,Z,A}{S,U}` to integer in all four forms (`.dx` `.dw` `.sx` `.sw` =
  source d/s, destination x/w), four fixed-point `FCVTZS/FCVTZU ..., #fbits` forms (`_fx`), `FJCVTZS`, and scalar `FCVTXN`;
- 112 vector ops (#488) on `.4s` and `.2d`: `FADD FSUB FMUL FDIV FMULX FMAX FMIN FMAXNM FMINNM FABD FRECPS FRSQRTS FADDP
  FMAXP FMINP`, the compares `FCMEQ FCMGE FCMGT FACGE FACGT` (and `FCMEQ #0.0`), `FMLA FMLS`, the unary `FSQRT FRECPE
  FRSQRTE FABS FNEG FRINT{X,A,Z,I,M,N,P} FRINT32Z FRINT64X`, all ten `FCVT*` to integer, `SCVTF UCVTF`, fixed-point
  `FCVTZS`/`SCVTF` with `#fbits`, `FCVTN FCVTN2 FCVTXN FCVTXN2 FCVTL FCVTL2`, and the reductions `FMAXV FMINV FMAXNMV
  FMINNMV` (`.4s`). Names are `<insn>.4s` / `<insn>.2d`.

Vector operands are 128-bit, printed as 32 hex digits. Lane `k` of operand `a` at table index `i` is `table[(i+k) mod 18]`, of
`b` at index `j` is `table[(j+k) mod 18]`, and of the `FMLA/FMLS` accumulator is `table[(i+j+k) mod 18]`. Lane 0 therefore walks
the same 18 x 18 grid as the scalar ops (2,592 cases per binary or ternary vector op, 144 per unary) and the other lanes
carry different patterns. FPSR is the OR over the lanes.

```sh
docs/verify/fp-workload/run.sh <new out dir>      # from the repo root; ARM64 host, clang, rust-lld or ld.lld
```

`FP_WORKLOAD_OPS="fadd.d fsqrt.s"` runs only those ops. `FP_WORKLOAD_TAMPER=<op>` edits one result in that op's host
reference after it is written; that op must then come out `MISMATCH` on both tiers.

**CI (#485):** the `fp-workload` job in [`fp-edge.yml`](../../../.github/workflows/fp-edge.yml) runs this on a Linux ARM64
runner (`ubuntu-24.04-arm`) for pull requests that touch the CPU engines, the ELF runner or this folder. It fails unless
every op listed in [`gated-ops.txt`](gated-ops.txt) (all 199 ops, expanded from the original 48 once verified on Apple M3)
is `identical` on both tiers (so `UNSUPPORTED` fails too) and no op is `OTHER-FAIL`. It runs the negative control with
`FP_WORKLOAD_TAMPER=fadd.d`, and runs [`fjcvtzs_gate.sh`](fjcvtzs_gate.sh) on the same ARM64 runner. That runner is not Apple silicon, and its runs are the project's own, not independent.

Exit status is 1 if any op is `MISMATCH` or `OTHER-FAIL`; `UNSUPPORTED` alone does not fail the run.

## Results (Apple M3, macOS, Apple clang 21.0.0)

**Before** the scalar change (base 008e3dd): [summary.tsv](result-2026-11-17/summary.tsv). Tier-3 and Tier-4: 40 ops identical, 8 ops
UNSUPPORTED (engine stops with exit 71, `Unimplemented instruction`), 0 mismatches. The 8 were scalar `FRECPS`, `FRSQRTS`,
`FRECPE`, `FRSQRTE` (double: `0x5e61fc02`, `0x5ee1fc02`, `0x5ee1d802`, `0x7ee1d802`; single `FRECPE`/`FRSQRTE`: `0x5ea1d802`,
`0x7ea1d802`), `FRINT32Z` and `FRINT64X` (`0x1e684002`, `0x1e69c002`).

**After** implementing those scalar forms (this folder's PR; [summary.tsv](result-2026-11-17-after-scalar-recip/summary.tsv)):
all 48 ops are byte-identical to the host on Tier-3 and Tier-4, every case (value and FPSR); 0 unsupported, 0 mismatches.
The first attempt after the change showed single-precision `FRECPE`/`FRSQRTE` with matching values but a spurious
divide-by-zero flag (an unread lane evaluated as zero); that was fixed by replicating lane 0, and the table above is the
rerun. The instruction fuzzer agrees (host vs interpreter, 8 seeds x 400,000 words, all 8 FPCR values): scalar
FRECPE/FRSQRTE 3,200,000 executed, scalar FRECPS/FRSQRTS 3,200,000, scalar FRINT32Z/32X/64Z/64X 1,801,223 (the host
rejects the other words); 0 value mismatches, 0 flag-only differences, 0 unimplemented.

### All 199 ops and `fjcvtzs_nzcv` (2026-11-19, base 1553f9d)

**All 199 ops on Apple M3** ([summary.tsv](result-2026-11-19-apple-m3-199ops/summary.tsv),
[toolchain.txt](result-2026-11-19-apple-m3-199ops/toolchain.txt),
[SHA256SUMS](result-2026-11-19-apple-m3-199ops/SHA256SUMS)): `run.sh` ran across all 199 ops (64 two/three-operand ops x
2,592 cases + 135 one-operand ops x 144 cases = 185,328 total cases) on Apple M3 (`Darwin arm64`, Apple clang 21.0.0)
against `ounce-core run-elf` on `--cpu=tier3` and `--cpu=tier4`. **All 199 ops are `identical`** to the native Apple M3
host (every case, value and FPSR) on both Tier-3 and Tier-4: **199 identical, 0 UNSUPPORTED, 0 MISMATCH, 0 OTHER-FAIL.**
All 199 ops are now listed in [`gated-ops.txt`](gated-ops.txt). (This supersedes the earlier x86_64 `qemu-aarch64`
informational check in [result-2026-11-18-488-qemu-informational/summary.tsv](result-2026-11-18-488-qemu-informational/summary.tsv),
which had flagged 5 unimplemented ops fixed in #469.)

**`FJCVTZS` NZCV and FPSR after #499** ([fjcvtzs_nzcv.c](fjcvtzs_nzcv.c), [fjcvtzs_gate.sh](fjcvtzs_gate.sh)): the workload
does not print NZCV, which `FJCVTZS` writes (`0 Z 0 0`). `fjcvtzs_nzcv.c` prints result, NZCV and FPSR for 31 doubles at
FPCR `0` and FPCR.FZ (`1 << 24`, 62 rows). Compiled natively on Apple M3 (`clang -O1 -march=armv8.5-a` with a Darwin
`write`/`main` wrapper for stdout/exit) and as a freestanding static AArch64 Linux ELF run through
`target/release/ounce-core run-elf` on `--cpu=tier3` and `--cpu=tier4`, **all 62/62 rows agree byte-for-byte** across
native Apple M3, Tier-3 and Tier-4 (including denormals flushed under FZ giving `Z = 0` and FPSR `0x80` for either sign).
Running `fjcvtzs_gate.sh` with `FJCVTZS_NATIVE_REF` set to that native output passes on both tiers and flags the tampered
reference negative control.

## Checks on the harness itself

- Negative control: a host reference with one result edited is flagged by the comparison (`cmp`), and the reported line is the edited one (`FP_WORKLOAD_OPS=fadd.d FP_WORKLOAD_TAMPER=fadd.d` produces `MISMATCH` on both tiers on Apple M3, and CI repeats this on every run).
- The FPCR modes change the observable result, so they are exercised: `fadd.d` of the smallest subnormal and 1.0 gives
  FPSR `0x10` (inexact) at default FPCR and `0x80` (input denormal) under FZ, on host and both tiers.
- FPSR values seen on the host across all 199 ops (33 distinct byte values masked to `0x9f`): `00 01 02 03 08 09 0a 10 11 12 14 15 18 19 1a 1c 80 81 82 83 88 89 8a 8b 90 91 92 93 94 95 96 97 9c`.

## Limits (not claimed)

- One host (Apple M3), one run, by the maintainers' tooling: this is **not** an independent reproduction (#248 still open).
- The reference is the native instruction; "agree" means identical bits and FPSR for these inputs only, not general
  correctness, and not FP16/FZ16, trap enables, FPCR bits other than FZ/DN/RMode, or the interpreter engine
  (`run-elf` offers Tier-3 and Tier-4 only; interpreter coverage is the fuzzer's).
- The 12 ops added after the 199-op Apple M3 run (#469/#507: scalar-SIMD fixed-point `scvtf`/`ucvtf`/`fcvtzs`/`fcvtzu`
  `s2, s0, #7` and `d2, d0, #20`, and unsigned vector `ucvtf`/`fcvtzu` `.4s`/`.2d` with `#7`/`#20`) run all 144 cases on
  Tier-3 and Tier-4 and are reported outside the 199 gated ops in [`gated-ops.txt`](gated-ops.txt).
- Vector coverage is the `.4s` and `.2d` arrangements only. Not covered: `.2s` and other 64-bit forms, FP16 (`.4h`/`.8h`,
  `FCVT` to and from half, `FZ16`), by-element forms (`FMLA`/`FMUL`/`FMULX ... v.s[i]`), scalar-SIMD forms of the vector
  instructions (scalar `FADDP`/`FMAXP`, scalar `FMULX`/`FRECPS`/`FRSQRTS`; the scalar fixed-point converts are now covered), `FCADD`, `FCMLA`, `FMLAL`, `FRECPX`,
  `BFCVT`/`BFMLAL`, and scalar `FCVT` between half and single/double.
- Lane patterns are the rotation described above, not all 18^lanes combinations, so a bug that needs a lane-pair that the
  rotation never produces can be missed. Each fixed-point op uses one `#fbits` value (scalar 16, 5, 33, 7; vector `.4s` 7,
  `.2d` 20), not every shift amount. `FJCVTZS` also writes NZCV, which only the separate fjcvtzs gate prints.
- The earlier informational `qemu-aarch64` file uses a software model reference with a different failure profile; it is retained for history only.
- Per-op outputs (about 3 MB) are not committed; `SHA256SUMS` in the result folder pins `fp_edge.c`, the ELF, the host binary and every
  per-op host output so a rerun can be checked against them. Rebuilt binaries are not bit-for-bit pinned across toolchains.
