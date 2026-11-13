# Improvement ideas (found 10 November 2026)

A hunt for outside resources that could make Ounce-Core better. Nothing here is built yet unless a
status line says so. Ranked by value for effort.

| # | Idea | Why | Status |
|---|---|---|---|
| 1 | Berkeley TestFloat as a floating-point oracle | Checks every operation across edge cases and rounding modes; would settle the FMAX/FMIN SNaN question. BSD-3 licence (SoftFloat 3e `COPYING.txt`), so it is fine as a test-time tool; do not vendor it without adding notices. | Built: `tools/testfloat/gen_trace.py` (see Results below) |
| 2 | Arm machine-readable spec (AARCHMRS, JSON) | Lists every encoding, so decoder coverage can be measured instead of guessed (the thin "QC not set" coverage). The full XML/ASL packages carry Arm's own licence: use at test time only, never commit them. | Not started |
| 3 | `diverge` for Tier-3 and Tier-4 | The JIT tiers are where silent errors are most likely; `diverge` only replays the interpreter today (see `docs/DIVERGE.md`). | Not started |
| 4 | `cargo-fuzz` on the decoder and the ELF/NRO loaders | There is only the instruction fuzzer, no parser fuzzing. | Not started |
| 4 | Miri on `ounce-nce` where it runs | `ounce-nce` holds the only unsafe code. | Not started |
| 4 | `cargo-mutants` on the decoder | Shows whether the tests notice a flipped bit. | Not started |
| 5 | QEMU `tests/tcg/aarch64` | Read for ideas only. The files are GPL; do not copy them into this MIT repository. | Decided: do not copy |

## Known gap found while planning

`fuzz-replay` compares the FPSR exception flags (`EXC = 0x9F` in `src/fuzz_replay.rs`) since the interpreter models
them (`src/cpu/softfp.rs`); TestFloat checks them too, against an independent oracle.

## TestFloat results (10 November 2026)

Method: `testfloat_gen -rnear_even -level 1` (SoftFloat 3e, ARM-VFPv2 build) for f32 and f64, converted by
`tools/testfloat/gen_trace.py` into `fuzz-replay` traces. Compared: the result register. Not compared:
FPSR exception flags (unmodelled). Skipped: cases with a NaN input or result.

| Operation | Cases (f32+f64) | Interpreter | Tier-3 |
|---|---|---|---|
| add | 86,631 | 0 mismatches | 0 mismatches |
| sub | 86,631 | 0 mismatches | 0 mismatches |
| mul | 86,619 | 0 mismatches | 0 mismatches |
| div | 86,619 | 0 mismatches | 0 mismatches |
| sqrt | 650 | 0 mismatches | 0 mismatches |
| fused multiply-add | 10,839,335 | 0 mismatches | 0 mismatches |

That run (above) used round-to-nearest-even only and compared results, not flags.

`tools/testfloat/gen_trace.py` now also emits the exception flags, all four FPCR rounding modes, invalid operations
on non-NaN inputs (positive default NaN), FCVT, SCVTF/UCVTF, FCVT*S/U (saturating, NaN gives 0), FRINT*, FCMP and
FCMPE (NaNs included). The `testfloat` job in `.github/workflows/fp-edge.yml` builds SoftFloat/TestFloat at pinned
commits and replays that through the interpreter and Tier-3. **Result (CI run 43180713127, commit 5ba3cd9, 2026-11-12, `--count 3000`):** 430,640 cases emitted (10,360 NaN-input cases
skipped), 20 instruction classes; the interpreter and Tier-3 each match all 430,640 on result, NZCV and QC with 0
mismatches and 0 FPSR flag mismatches. The local sandbox still cannot run TestFloat, so this is CI-only.

Still not covered: NaN *inputs* (TestFloat's propagation rules differ from Arm's), FMAX/FMIN, FZ and DN, half
precision, Tier-4 compiled FP.
