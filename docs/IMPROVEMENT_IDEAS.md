# Improvement ideas (found 10 November 2026)

A hunt for outside resources that could make Ounce-Core better. Nothing here is built yet unless a
status line says so. Ranked by value for effort.

| # | Idea | Why | Status |
|---|---|---|---|
| 1 | Berkeley TestFloat as a floating-point oracle | Checks every operation across edge cases and rounding modes; would settle the FMAX/FMIN SNaN question. BSD-3 licence (SoftFloat 3e `COPYING.txt`), so it is fine as a test-time tool; do not vendor it without adding notices. | Not built yet: fetching and compiling TestFloat is awaiting the owner's go-ahead |
| 2 | Arm machine-readable spec (AARCHMRS, JSON) | Lists every encoding, so decoder coverage can be measured instead of guessed (the thin "QC not set" coverage). The full XML/ASL packages carry Arm's own licence: use at test time only, never commit them. | Not started |
| 3 | `diverge` for Tier-3 and Tier-4 | The JIT tiers are where silent errors are most likely; `diverge` only replays the interpreter today (see `docs/DIVERGE.md`). | Not started |
| 4 | `cargo-fuzz` on the decoder and the ELF/NRO loaders | There is only the instruction fuzzer, no parser fuzzing. | Not started |
| 4 | Miri on `ounce-nce` where it runs | `ounce-nce` holds the only unsafe code. | Not started |
| 4 | `cargo-mutants` on the decoder | Shows whether the tests notice a flipped bit. | Not started |
| 5 | QEMU `tests/tcg/aarch64` | Read for ideas only. The files are GPL; do not copy them into this MIT repository. | Decided: do not copy |

## Known gap found while planning

`fuzz-replay` masks the FPSR exception flags (`EXC = 0x9F` in `src/fuzz_replay.rs`) because the
interpreter does not model them. A program that reads FPSR after a floating-point operation sees
different flags from real hardware. TestFloat checks flags, so it will make this visible.
