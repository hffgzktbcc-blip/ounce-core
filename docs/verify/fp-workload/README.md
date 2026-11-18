# FP edge-case program workload (#359, extended by #488)

Complements [`../fp-edge/`](../fp-edge/README.md), the existing instruction-level experiment (one instruction per case, interpreter/Tier-3/Tier-4, CI on Linux and Apple Silicon). This folder runs a whole program: one pinned C source as a static ELF through `run-elf`, compared with the same source on the host.

One pinned C source, [fp_edge.c](fp_edge.c), built twice: natively on an ARM64 host (the reference) and as a
freestanding static AArch64 Linux ELF for `ounce-core run-elf` on Tier-3 and Tier-4. Both print one line per case with
raw hex bits: `op fpcr-nibble operands = result fpsr`. Every op runs as its own process (`argv[1]`), so an engine that
stops on one op cannot hide the others.

Cases: 18 fixed bit patterns per precision (signed zeros, smallest/largest subnormal, smallest normal, max finite,
infinities, quiet/signalling/negative-payload NaNs, 0.1, 2^63, 2^53) x 8 FPCR values (default, FZ, DN, FZ+DN, RMode
RP/RM/RZ, FZ+DN+RZ) x all operand pairs (2,592 cases per binary op, 144 per unary op). FPSR is cleared before each case
and printed masked to `0x9f` (IOC, DZC, OFC, UFC, IXC, IDC, QC). 199 ops (`fp_edge_host --list`):

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
the 48 ops listed in [`gated-ops.txt`](gated-ops.txt) are `identical` on both tiers (so `UNSUPPORTED` fails too) and no op is `OTHER-FAIL`. The 151 ops added for #488 are printed with their verdicts but do not gate, so a real `MISMATCH` or an engine gap is a recorded result instead of a red job. It runs the negative control with
`FP_WORKLOAD_TAMPER=fadd.d`. That runner is not Apple silicon, and its runs are the project's own, not independent.

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

### #488 extension (151 new ops): status

**The native comparison for the 151 new ops has not been run.** The session that wrote them had only an x86_64 Linux
container, and `run.sh` needs an ARM64 host for the reference. So there is no `identical / UNSUPPORTED / MISMATCH` table from
`run.sh` for these ops yet. It needs one run on the Apple M3 (or an ARM64 Linux host), with the resulting `summary.tsv`
committed next to the others and this section replaced by it.

What was done instead, and what it is worth (informational, **not** the host reference):
[result-2026-11-18-488-qemu-informational/summary.tsv](result-2026-11-18-488-qemu-informational/summary.tsv). The same freestanding
ELF was run under `qemu-aarch64` 8.2.2 (user-mode emulation, a software model of AArch64, not hardware) and under `ounce-core
run-elf --cpu=tier3` from an x86_64 build of this branch, and the outputs were compared byte for byte per op. Tier-4 was not run
(it needs an ARM64 host). The summary has 152 rows: the 151 new ops plus `fcvtzs.sw`, an original op that the list order puts
first in that range. Result: 147 identical to qemu, 5 UNSUPPORTED, 0 differences, 0 OTHER-FAIL. The five unsupported ops stop with
exit 71 `Unimplemented instruction` on these words: `FJCVTZS` (`fjcvtzs.dw`, `0x1e7e0008`) and the vector fixed-point converts
`fcvtzs_fx.4s` (`0x4f39fc02`), `fcvtzs_fx.2d` (`0x4f6cfc02`), `scvtf_fx.4s` (`0x4f39e402`), `scvtf_fx.2d` (`0x4f6ce402`). Those
five are engine gaps this run found, independent of which reference is used. "Identical to qemu" says the engine and qemu's
softfloat agree on these inputs. It does not say either matches silicon, and a qemu-versus-hardware difference would show up as
a `MISMATCH` on the real run. Do not read the 147 as host-verified.

**Update (#469): the five engine gaps are filled.** `FJCVTZS` and the vector fixed-point `SCVTF`/`UCVTF`/`FCVTZS`/`FCVTZU`
(`.2s`, `.4s`, `.2d`) are now decoded and executed (interpreter, Tier-3; Tier-4 runs them through its interpreter fallback).
The same five ops were rerun the same informal way (x86_64 `run-elf --cpu=tier3` against `qemu-aarch64` 8.2.2): all five exit 0
and are byte-identical to qemu over their 144 cases each. That is again a software-model comparison, not hardware: the native
run of these five ops still has not happened, and a hardware difference would show up as a `MISMATCH` in the `fp-workload` job.
The workload does not print NZCV, which `FJCVTZS` writes (`0 Z 0 0`). [fjcvtzs_nzcv.c](fjcvtzs_nzcv.c) prints result, NZCV and
FPSR for 31 doubles at FPCR 0 and FZ, so it can be built like `fp_edge.c` and run natively against `run-elf` later. Under qemu and
Tier-3 it agrees on 61 of 62 rows; the one difference is a negative denormal under FZ: the engine follows the `FPToFixedJS`
pseudocode as read (the flushed input keeps its sign, so Z = 0) and qemu gives Z = 1. Which one silicon does is unverified.

Also checked: the file compiles without warnings for both targets with `-Wall -Wextra` (plus `-Wno-missing-field-initializers`,
since the older table rows leave the new `vfn` field null on purpose), links, and `fp_edge.elf --list` prints 199 names.

## Checks on the harness itself

- Negative control: a host reference with one result edited is flagged by the comparison (`cmp`), and the reported line is the edited one. CI repeats this on every run (`FP_WORKLOAD_TAMPER`).
- The FPCR modes change the observable result, so they are exercised: `fadd.d` of the smallest subnormal and 1.0 gives
  FPSR `0x10` (inexact) at default FPCR and `0x80` (input denormal) under FZ, on host and both tiers.
- FPSR values seen on the host across all ops: 00, 01, 10, 80, 81, 18, 14, 02 and others.

## Limits (not claimed)

- One host (Apple M3), one run, by the maintainers' tooling: this is **not** an independent reproduction (#248 still open).
- The reference is the native instruction; "agree" means identical bits and FPSR for these inputs only, not general
  correctness, and not FP16/FZ16, trap enables, FPCR bits other than FZ/DN/RMode, or the interpreter engine
  (`run-elf` offers Tier-3 and Tier-4 only; interpreter coverage is the fuzzer's).
- The 151 ops added for #488 have no host run at all yet (see the status section above).
- Vector coverage is the `.4s` and `.2d` arrangements only. Not covered: `.2s` and other 64-bit forms, FP16 (`.4h`/`.8h`,
  `FCVT` to and from half, `FZ16`), by-element forms (`FMLA`/`FMUL`/`FMULX ... v.s[i]`), scalar-SIMD forms of the vector
  instructions (scalar `FADDP`/`FMAXP`, `FCVTZS d2, d0`, scalar `FMULX`/`FRECPS`/`FRSQRTS`), `FCADD`, `FCMLA`, `FMLAL`, `FRECPX`,
  `BFCVT`/`BFMLAL`, and scalar `FCVT` between half and single/double.
- Lane patterns are the rotation described above, not all 18^lanes combinations, so a bug that needs a lane-pair that the
  rotation never produces can be missed. Each fixed-point op uses one `#fbits` value (scalar 16, 5, 33, 7; vector `.4s` 7,
  `.2d` 20), not every shift amount. `FJCVTZS` also writes NZCV, which is not printed.
- The qemu figures above use a different reference with a different failure profile; they are a smoke check only.
- Per-op outputs (about 3 MB) are not committed; `SHA256SUMS` in the result folder pins the ELF, the host binary and every
  per-op host output so a rerun can be checked against them. Rebuilt binaries are not bit-for-bit pinned across toolchains.
