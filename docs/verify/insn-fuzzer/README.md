# Single-instruction fuzzer: Ounce-Core interpreter vs real Apple Silicon

Runs one A64 instruction at a time on the host CPU and in Ounce-Core's Tier-3 interpreter (`Cpu::step`) with identical
register inputs, then compares every X register, V register, NZCV and FPSR. Host: Apple Silicon, macOS (arm64 only).

## Method
- `fuzz_host.c` (this directory). A seeded xoshiro256** PRNG picks a word, then random inputs for X0-X30, NZCV and
  V0-V31. The word goes into a `MAP_JIT` page (`pthread_jit_write_protect_np`, `sys_icache_invalidate`) behind a
  position-independent template that loads all registers, runs the word, and stores all registers back. FPCR is the
  host default (0) and FPSR is cleared before each case. Words that raise SIGILL are reported as `U` lines.
- Words are drawn from the Advanced SIMD and scalar FP data-processing space (bits 27:25 = 111), so there are no
  loads/stores, branches or system instructions. Each case picks one of 16 families (scalar FP, three-same, two-reg misc, across
  lanes, three-different, by-element, shift-immediate, scalar variants, permute/extract/copy, modified immediate) and fixes that
  family's top-level bits; the rest are random. Register values are biased: FP special values (NaN/sNaN/inf/denormal/max/
  min) as f16, f32 or f64 lanes, small and edge integers per lane width, random bits, and aliased registers.
- `ounce-core fuzz-replay <trace|->` (`src/fuzz_replay.rs`, safe Rust) reads the trace from a pipe and replays each case in
  the interpreter. Compared: X0-X30 (except x18, below), V0-V31, NZCV, and FPSR.QC. FPSR exception flags
  (IOC/DZC/OFC/UFC/IXC/IDC) are counted separately because the interpreter does not model them (see qc-flag/README.md).
- Run: `cargo build --release && docs/verify/insn-fuzzer/run.sh <seed> <words>`. Nothing is stored on disk (about 1.6 KB per
  case); the same seed and word count reproduce the same trace on the same host.

## Skipped or excluded
- UNDEFINED encodings (host SIGILL) are not compared. They are replayed only to count words the interpreter runs but the
  host rejects ("emulator runs a word the host calls UNDEFINED").
- x18 is not compared: macOS reserves it and may clear it, so the input is fixed at 0 and the output ignored.
  (This showed up as a false mismatch in the first 1M-word run: 5 mismatches, all x18.)
- Architecturally UNPREDICTABLE / implementation-defined results: none were excluded individually. The covered space
  has none that Apple Silicon deviates from in these runs; if one appears it would show as a mismatch to triage by hand.
  The estimate instructions (FRECPE/FRSQRTE) are fully specified and were not excluded.
- Words the interpreter does not decode (`Unimplemented`) are counted as coverage gaps, not mismatches.

## Results (origin/main b2f54c5 plus the fixes in this branch)
Final state, seeds 1, 2, 3, 11, 12, 20261108, 21, 22, 1,000,000 words each:

| | words |
|---|---|
| generated | 8,000,000 |
| host SIGILL (UNDEFINED) | 4,777,660 |
| host executed | 3,222,340 |
| interpreter `Unimplemented` | 359,291 (about 690 encoding classes per seed; was 1,288,396 before the class work below) |
| interpreter executed and compared | 2,863,049 (about 2,010 distinct classes per seed) |
| exact match (X, V, NZCV, QC) | 2,863,049 |
| mismatches | 0 |
| executed words whose FPSR exception flags differ from the host | 367,032 (not modelled, not counted as mismatches) |
| words the interpreter runs but the host calls UNDEFINED | 0 |

### Mismatches found and fixed
On origin/main, seed 20261108, 400,000 words (161,160 host-executed, 97,499 interpreter-executed): 505 mismatches and 783
words accepted by the decoder that the host rejects. Triage (all were emulator bugs, none a fuzzer or ISA subtlety):

1. **Half-precision scalar FP decoded as single precision** (about 500 mismatches). FCSEL, FCMP, FP one-source, FRINT and FP/int
   conversions accepted ftype 3 and computed in single precision. Now `Unimplemented`. Commit "Decoder: do not decode
   half-precision ...", test `fuzz_half_precision`.
2. **Decoder accepts UNDEFINED scalar FP encodings** (783 words): M/S bits ignored on FADD/FMUL/..., FCMP's op field ignored, SCVTF/
   UCVTF/FCVTA* with nonzero rmode, FMOV general<->FP with mismatched sf/type. Commit "Decoder: reject scalar FP encodings ...",
   test `fuzz_fixed_bits`.
3. **FMUL/FMLA/FMLS by element picked the wrong NaN operand** (3 mismatches in 400k). Operand order was left to the compiler.
   Fixed with explicit Arm FPProcessNaNs order shared with the three-same code. Test `fuzz_nan_order`.
4. **Scalar FNMUL picked the wrong NaN operand** (3 mismatches in seed 11, 2 in seed 21 after fixes 1 to 3). Same root cause.
   Scalar FADD/FSUB/FMUL/FNMUL/FDIV/FMAX/FMIN/FMADD family now use explicit NaN order, FMADD addend first with the 0*inf default
   NaN case. Test `fuzz_scalar_nan` (expected values are what the host returned).

## Classes implemented from fuzzer coverage gaps
Ranked by `Unimplemented` word count over seeds 1 to 3 (481,591 words): integer three-same 41%, two-register misc 20%, by-element
16%, scalar FP16 about 7%, copy/other, shift. `fuzz-replay --filter MASK:VALUE` (repeatable) counts executed and exactly matching
words in a class.

1. **Integer three-same** (SHADD/UHADD, SRHADD/URHADD, SHSUB/UHSUB, SQADD/UQADD, SQSUB/UQSUB, S/URSHL, S/UQSHL, S/UQRSHL,
   S/UABD, S/UABA, MLA/MLS, PMUL, SQDMULH, SQRDMULH) plus the scalar three-same forms (all sizes of the saturating add/sub/shift,
   SQDMULH/SQRDMULH, D-sized compare/add/shift). QC is modelled. Seeds 1 to 8, 1M words each: 222,144 executed words of the new
   vector opcodes; scalar forms (new opcodes only) 221,708 words over 96 seeds. 0 mismatches. Test
   `fuzz_insn_tests::integer_three_same_classes`.
2. **Integer two-register misc**: SUQADD, USQADD, CLS, CLZ, RBIT, SQABS, SQNEG (vector and the scalar saturating forms), SHLL/SHLL2.
   Seeds 1 to 24: 269,877 executed vector words, 0 mismatches. The scalar SUQADD/USQADD/SQABS/SQNEG forms are rare in the
   fuzzer's output (220 executed words in 8M), so they are lightly tested. Test `fuzz_insn_tests::integer_two_reg_misc_classes`.
3. **By-element integer, dot product, FMULX**: MUL, MLA, MLS, SQDMULH, SQRDMULH, S/UMULL, S/UMLAL, S/UMLSL, SQDMULL, SQDMLAL,
   SQDMLSL by element, scalar SQDMULH/SQRDMULH/SQDMULL/SQDMLAL/SQDMLSL by element, SDOT/UDOT (by element and vector), vector
   FMULX (three-same and by element). Seeds 1 to 16: 266,727 executed vector words, 0 mismatches; scalar by element: 214,000
   words (seeds 17-24 and 301-380), 0 mismatches. Test `fuzz_insn_tests::by_element_dot_and_fmulx_classes`.
   Scalar FP by-element (FMUL/FMLA/FMLS/FMULX Sd/Dd) and scalar FMULX are not implemented.
4. **FP round, convert and estimate two-register ops** (vector, f32 and f64 lanes): FRINTN/P/M/Z/A/X/I, FRINT32Z/32X/64Z/64X,
   FCVTNS/NU/MS/MU/AS/AU/PS/PU, FRECPE, FRSQRTE, URECPE, URSQRTE. Seeds 1 to 16: about 314,000 executed words of the group,
   0 mismatches (the estimates, FRECPE/FRSQRTE/URECPE/URSQRTE, are about 19,000 of those words in 8M generated, so they are the
   most lightly tested part). The default-FPCR path was fuzzed; under non-default FPCR the ops flush denormal inputs under FZ and
   FRINTX/FRINTI/FRINT32X/FRINT64X honour RMode, but those paths and DN were not fuzzed. Scalar forms (FCVTNS Sd, FRECPE Sd, ...)
   and half precision are not implemented. Test `fuzz_insn_tests::fp_round_convert_estimate_classes`.
   Decoder gap found on the way: FRINTA (opcode 11000, U=1) was missing.
5. **FRECPS and FRSQRTS** (vector, f32 and f64). The first operand is negated before NaN processing (so its NaN returns with
   the sign flipped), and FRSQRTS halves before rounding so `(3 - a*b) / 2` does not overflow early; the fuzzer found both.
   Seeds 1 to 96: 209,840 executed words, 0 mismatches after those two fixes (3,707 mismatches before). Under non-default FPCR
   they take the default-rounding path (untested). Test `fuzz_insn_tests::fp_recip_step_classes`. FMAXP/FMINP/FMAXNMP/FMINNMP were
   already implemented on main.

## Not covered (explicit)
- Loads/stores, branches, system, integer scalar data-processing, SVE/SME, crypto, atomics.
- Features the interpreter lacks: half-precision scalar and vector FP (FEAT_FP16), BF16/I8MM, FCMA, RDM, and
  every encoding class it reports as unimplemented (top classes by count: 0x4f00fc00, 0x0f00fc00, 0x6e20f800; see the summary the tool prints).
  That is about 11% of the words the host executes in this space (was 40%). The largest remaining groups are scalar FP16
  (about a third of it), the FEAT_FHM widening FMLAL/FMLSL(2), FCVTN/FCVTL/FCVTXN half-precision and round-to-odd forms, scalar
  FP by-element and scalar two-register FP ops, and some shift-immediate forms.
- FPCR is always 0 (default rounding, no flush-to-zero, no default-NaN, no trapping). Other FPCR modes are untested.
- FPSR exception flags (IOC, DZC, OFC, UFC, IXC, IDC) are not modelled by the interpreter and are not compared.
- Tier-3 and Tier-4 are covered by the section "Tier-3 and Tier-4" below.
- Other vector FP paths in `interpreter.rs` (`SimdVec4S`, the 2D paths) still use plain Rust `+`/`*` on NaN inputs; they
  matched in these runs but depend on the compiler keeping operand order. The default-NaN result of `0 * inf` relies on
  the host being AArch64 (x86 hosts produce a different default NaN).
- Random inputs reach rare cases (exact ties, overflow boundaries, saturation edges) only by chance; the biased values help but
  are not exhaustive. 0 mismatches over 1.9M executed words is evidence, not proof.

## Tier-3 and Tier-4

`fuzz-replay --engine interp|tier3|tier4` runs each word through the chosen engine and compares it with the host trace.
`fuzz-replay --engine-vs-engine --seed N --count M` needs no host: it compares Tier-3 and Tier-4 with the interpreter on random
words, so it runs on any OS. `run_tiers.sh [words]` does both (host replay only on Apple Silicon macOS) and exits non-zero on
any difference.

Results (release build):
- Engine vs interpreter: 2M words (8 seeds x 250k) and 3 x 200k in the script run, 0 mismatches for Tier-3 and Tier-4.
  Tier-4 ran about 75% of the words natively; the rest fell back.
- Host replay, 200k words per seed, seeds 1 to 4 through interp, tier3 and tier4: 0 mismatches, 0 emulator errors.
- Harness check: a deliberate bug in the Tier-3 add/sub-immediate overflow flag was caught, then reverted.

Same limits as above: only the instruction classes the interpreter implements, FPCR 0, FPSR flags not compared.

## CI qualification

`.github/workflows/differential.yml` runs `run_tiers.sh 20000` in an independent
Apple Silicon job on runtime/verification changes and the weekly schedule. Seeds
1–3 compare Tier-3/Tier-4 with the interpreter; seed 1 also compares Tier-3/Tier-4
with the hardware trace. The job requires arm64, preserves pipeline failures,
and uploads the run log and per-engine replay summaries even on failure.

The first [hosted job](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42521905867/job/128857024677) passed on 2026-11-08; the [execution excerpt](ci-2026-11-08.log) retains the seed counts and zero-mismatch summaries.
It covers single instructions with the exclusions above, not generated sequences
or exhaustive coverage of every family. Reproduce with the same script locally.

## Failure exits and incomplete inputs

`fuzz-replay` returns 2 for an empty trace, a trace read/parse error, an unavailable
engine, or zero requested generated cases. It returns 1 for value mismatches,
execution errors, or an encoding accepted by the engine that the reference marks
UNDEFINED. Zero mismatches from zero input is not a passing run. Coverage gaps
remain separately counted; a successful run does not imply all encodings executed.
The contradictory-UNDEFINED regression test intentionally supplies a synthetic
reference to verify the failure exit; it is not a hardware observation.

## Retained first differences and replay

`fuzz-replay --engine tier3 --first-difference NEW-DIR reference.trace` retains the
first value mismatch with exact before/reference/actual state and replay bytes.
`replay-instruction NEW-DIR` validates those bytes and reports reproduction
separately from agreement with the reference. See the [command guide and real
hardware case](../../INSTRUCTION_DIAGNOSIS.md). This is bounded single-instruction
diagnosis, not an arbitrary program's first divergent instruction.
