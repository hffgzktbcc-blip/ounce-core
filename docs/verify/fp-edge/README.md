# FP edge-case experiment (issue #359)

Question: do the interpreter, Tier-3 and Tier-4 return the same bits as real AArch64 hardware on a fixed set of
floating-point edge cases (NaNs including signalling, denormals, signed zeros, infinities, rounding modes, conversions,
fused multiply-add, min/max)? This tests a bounded corpus. It says nothing about arbitrary programs.

## Method
1. `../insn-fuzzer/fuzz_host.c --corpus edge` runs the corpus one instruction at a time on the host CPU (now portable:
   Linux AArch64 or Apple Silicon) and prints `TF <fpcr> <word> ...` trace lines with the exact input and output
   registers (V0-V3, X0 hold the operands; FPSR is recorded). The enumeration is fixed code with literal bit
   patterns, no PRNG: 277 instruction words x 9 FPCR values (default, FZ, DN, FZ+DN, RMode RP/RM/RZ, FZ16, all) x operand
   combinations drawn from 18 core values (28 for one-operand words, 7 for fused multiply-add, 12 integers for
   SCVTF/UCVTF); vector forms rotate the table across lanes. `fuzz_host --corpus edge --list-words` prints the words.
2. `ounce-core fuzz-replay --engine interp|tier3|tier4 --results FILE -` replays the same trace through each engine and
   writes one row per case: `match`, `mismatch` (with the host and engine bit patterns), `unsupported` (the engine does
   not implement the word; never counted as a pass or a failure) or `host-undefined`.
3. `summarize.py` builds the table, the first divergence per engine (with operand bits, so it is a minimal reproducer: one
   instruction word, one FPCR value, V0-V3), and engine-vs-interpreter differences.

`run.sh OUTDIR` does all of it and keeps the evidence: `trace.gz` (the hardware trace, sha256 in `meta.json`),
`results-<engine>.tsv`, `<engine>.stdout`, `summary.json`/`summary.md`, and `meta.json` (CPU, OS, ounce-core SHA, rustc, compiler,
commands). `.github/workflows/fp-edge.yml` runs it on `ubuntu-24.04-arm` and `macos-latest` (Apple Silicon) and uploads the
directory as an artifact. Exit status 1 means the gate failed: a value mismatch at FPCR=0 on an engine that ran.

The comparison is X0-X30 (not x18), V0-V31, NZCV and the FPSR exception flags (IOC, DZC, OFC, UFC, IXC, IDC). A case
whose values match but whose flags differ is counted separately (`fpsr_exception_flags_differ_on_matching_results`) and
fails the gate like a value mismatch. Tininess is detected before rounding (what AArch64 does); FZ flushes raise UFC
without IXC and a flushed input raises IDC; half-precision flushes raise nothing.

## Reading the results
- Native replay on an ARM64 host checks the harness and the engines against that CPU; the host row only says the loader
  and trace format work. Only the interpreter/Tier-3/Tier-4 columns are claims about Ounce-Core.
- Under qemu-user the reference is qemu's softfloat, not silicon. Use it to debug the harness, not as evidence of hardware
  agreement. Run it with `HOST_CC='python3 -m ziglang cc -target aarch64-linux-musl' HOST_RUN=qemu-aarch64 run.sh OUT`.
- Words every engine reports `unsupported`: half-precision arithmetic, `FRECPE/FRECPS/FRSQRTE/FRSQRTS/FRECPX`, scalar
  `FMULX`, scalar and vector `FADDP`, vector `FRINT*`, vector `FCVT{N,M,A,P}{S,U}`, `FCVT` to/from half. The full list is
  `words_always_unsupported` in `summary.json`.
- FPCR RMode, FZ and DN are modelled for scalar and vector add/sub/mul/div/fused multiply-add/sqrt, FRINT*, FCVT between
  single and double, SCVTF/UCVTF/FCVTZ*, and compares (`src/cpu/softfp.rs`, #363), and the exception flags of all of
  those plus FMAX/FMIN/FMAXNM/FMINNM (also the pairwise, reduction and half-precision forms). A mismatch at any FPCR
  value in the table is a finding. Not modelled: FZ16 and half-precision arithmetic, and trap enables (FPCR.IOE..IDE).
- **Tier-4 and flags.** Compiled FP does not accumulate exception flags. Once the program touches FPSR (the first
  `MRS`/`MSR FPSR`, or the replay harness) Tier-4 drops its FP-containing translations and leaves FP arithmetic and
  compares to the interpreter, so a program that clears or reads FPSR sees correct flags from then on. Flags raised by
  compiled FP *before* that first access are lost, so a program that only ever reads FPSR sees too few. Fixing that
  needs flag accumulation in the generated code.

## Results
Only runs whose provenance is recorded count. This section lists what was actually run; hardware numbers come from the CI artifacts.

**Local qemu-user run (weaker reference, x86_64 sandbox; harness shake-out, not hardware evidence)**, corpus trace sha256
`ceab2b2c…aeb84fdd89`, 389,556 cases. Before any fix the first run showed, at default FPCR, 935 interpreter mismatches
(Tier-3 identical, Tier-4 identical plus 324 more). All were real emulator defects, not reference artefacts, because the
expected values follow the Arm pseudocode as well as qemu:
- vector `FADD/FSUB/FMUL/FDIV` (.4S and .2D) used host arithmetic: a quiet NaN in `n` beat a signalling NaN in `m`
  (Arm returns the signalling one, quieted);
- invalid operations (inf-inf, 0*inf, 0/0, sqrt(-1)) returned the host's NaN, which is the negative NaN on x86 (Arm:
  positive default NaN), in scalar and vector arithmetic, FNMUL, fused multiply-add and FSQRT;
- scalar `FRINT*` and `FSQRT` returned a signalling NaN operand unquieted.
These are fixed in this branch (unit test `fuzz_scalar_nan::invalid_operations_return_positive_default_nan_and_unary_ops_quiet_snan`).
After the fix, at default FPCR: interpreter and Tier-3 0 mismatches in 32,628 executed cases (and 10,656 unsupported).
Tier-4 on this x86_64 host first returned the host's negative default NaN and the host's NaN operand choice for scalar
`FADD/FSUB/FMUL/FDIV/FNMUL` (96 cases at default FPCR; this also broke the existing `random_linked_loops` test once the
interpreter was fixed). The Cranelift lowering now rewrites NaN results to the Arm rule on non-AArch64 hosts only (AArch64
uses the native instruction unchanged), and Tier-4 then matches the interpreter on all 389,556 cases. Non-default FPCR: 36,941 interpreter mismatches before #363.

**After #363 (same qemu-user setup, same corpus):** interpreter, Tier-3 and Tier-4 each show 0 mismatches over all nine
FPCR values (293,652 executed cases per engine, 95,904 unsupported). Tier-4 runs translated blocks only with the default
FPCR; with RMode, FZ or DN set it hands execution to the soft-float interpreter path, so its non-default column checks that
hand-off rather than separate compiled code.

**Real hardware, CI run 42877863247 on commit de2b909 (artifacts `fp-edge-evidence-ubuntu-24.04-arm` and `-macos-latest`):**
Linux Arm runner (aarch64, Azure) and Apple M1 (virtual) give the same table. Each engine ran 389,556 cases: default FPCR
32,628 match, 0 mismatch, 10,656 unsupported; FZ 3,391, DN 7,129, FZ+DN 9,820, RP 2,016, RM 2,498, RZ 1,796 and all-modes 10,291
mismatches (the unmodelled-mode gap, issue #363); FZ16 equals default because no supported word is half precision. Tier-3 and
Tier-4 differ from the interpreter in 0 cases on both. The gate passed on both. The two hardware traces have different
sha256 values (Linux `ceab2b2c…`, macOS `f82e577a…`), so the hardware outputs are not byte-identical across the two CPUs;
the engines agree with each on every compared field. The cause of the trace difference (likely FPSR flags or x18, which are not
compared) was not investigated. The qemu-user trace has the same hash as the Linux Arm trace, so on this corpus qemu and
that CPU produced identical output.

**After the FPSR exception flags (local qemu-user, same corpus, same trace sha256 `ceab2b2c…`, this branch, 389,556
cases):** interpreter, Tier-3 and Tier-4 each show 0 value mismatches and 0 flag mismatches over all nine FPCR values
(294,156 executed cases per engine, 95,400 unsupported); the gate passes. The reference is qemu, not silicon: hardware
agreement on the flags is what the `fp-edge` CI job on `ubuntu-24.04-arm` and `macos-latest` decides, and it is not yet
recorded here. The one place the first local run disagreed with qemu was half-precision `FMAX/FMIN/FMAXNM/FMINNM` with
FPCR.FZ16 and a denormal input (1,024 cases, interpreter raised IDC, qemu does not); the interpreter now raises nothing
for half-precision flushes, which is a choice made to match qemu until the hardware run confirms or refutes it.

## Not covered
Loads/stores, branches, integer operations, SVE/SME, FEAT_FP16 arithmetic (unsupported), FRINT32/64, FCMA, BF16, trapping
FPCR bits, FPCR.AH/NEP, flags from half-precision conversions and from words the interpreter reports unsupported, flags
raised by Tier-4 compiled FP before the first FPSR access. An independent external rerun (#248, `docs/REPRODUCE.md`) is still needed: a
maintainer or agent rerun does not count.
