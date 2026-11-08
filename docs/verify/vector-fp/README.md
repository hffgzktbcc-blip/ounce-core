# Vector floating-point (2S/4S/2D): host differential test

`fp_cases.c` uses NEON intrinsics so the same C compiles to the real instructions on Apple Silicon (the reference, `cc -O1 -ffp-contract=off`) and, with devkitA64, as an NRO for Ounce-Core. 400 operand sets through 67 forms (26,800 result lines). Operands mix 27 edge patterns per width (0, -0, +/-1, +/-inf, quiet NaN with and without payload, signalling NaN, -NaN, smallest and largest subnormal, MIN, MAX, 2^31, 2^32, int boundaries, .5/1.5/2.5) with random finite values near 1.0 and fully random bit patterns.

## Covered (decoded, implemented, tested)
Single (2S, 4S) and double (2D) lanes:
- `FCMEQ`, `FCMGE`, `FCMGT`, `FACGE`, `FACGT` (register), and the `#0.0` forms `FCMEQ/FCMGE/FCMGT/FCMLE/FCMLT`
- `FADD`, `FSUB`, `FMUL`, `FDIV`, `FABD`, `FADDP`
- `FMLA`, `FMLS` (fused, `mul_add`)
- `FMAX`, `FMIN` (NaN propagates, signalling NaN wins, -0 < +0), `FMAXNM`, `FMINNM` (quiet NaN against a number returns the number)
- `FABS`, `FNEG`, `FSQRT`
- `FCVTZS`, `FCVTZU` vector (the vector form was undecoded; the scalar form already existed)
- `FCVTN`, `FCVTN2`, `FCVTL`, `FCVTL2`, double/single only

Implementation: `SimdFpThreeSame` / `SimdFpTwoMisc` in `src/cpu/instructions.rs`, decoder sections 27d and 29b, `simd::fp_three_same` / `simd::fp_two_misc`. Tier-4 translates none of these, so it falls back to the shared Tier-3 interpreter path for them (as for the other FP/SIMD forms it does not lower).

## Not covered
- Half precision (4H/8H, the FP16 three-same and two-misc encodings, `FCVTN/FCVTL` to/from half). Still undecoded.
- By-element FP forms were already present and are not part of this test; `FMULX`, `FMAXP/FMINP/FMAXNMP/FMINNMP`, `FRECPE/FRSQRTE/FRECPS/FRSQRTS`, `FRINT*`, other `FCVT*` rounding modes, `FCVTXN`, scalar FP compares in the SIMD scalar encoding.
- FPCR modes (`DN`, `FZ`, rounding) and FPSR cumulative exception flags are not modelled; results are the host's default-mode IEEE results.
- `FDIV`, `FADD`, `FSUB`, `FMUL`, `FSQRT` NaN handling is inherited from the host FPU (Rust `f32/f64` operators), correct on Apple Silicon and aarch64 hosts but not independently checked on x86 hosts.

## Result (2026-11-08, macOS arm64)
| Engine | Output vs host reference |
|---|---|
| Tier-3 | byte-identical (26,800 lines) |
| Tier-4 JIT | byte-identical |
| Native | byte-identical |

Native runs the NRO's own instructions on real hardware, so it is not evidence about the emulator; Tier-3 and Tier-4 are. NRO sha256: `a82701034765aa7338b5f3386aa0d2606ed31b0915bd3517982b9fb48bf860f4` (devkitA64 gcc 16.1, `-O1 -ffp-contract=off`; `-O2` hits an internal compiler error (segfault) on this large function).

## Findings from the differential
1. Real bug in the first implementation, caught before commit: `FMAX/FMIN/FMAXNM/FMINNM` returned the first NaN operand quieted. The architecture (FPProcessNaNs) prefers a signalling NaN over a quiet NaN even when it is the second operand: `fmax2d` case 248 (quiet default NaN against `0x7ff0000000000001`) differed on Tier-3 and Tier-4 (lane `7ff8000000000000` vs `7ff8000000000001`). Fixed in `simd.rs` with a unit test. The scalar `FMAX/FMIN` in `FpOp2` was not checked here.
2. Not an emulator bug, a harness note: `vfmaq_*`/`vfmsq_*` leave operand order to the compiler. clang (host) and gcc (devkitA64) put the two multiplicands in opposite `Vn`/`Vm` slots, so when both are NaN (or the NaN sits in the negated slot for `FMLS`) the result NaN's payload/sign legitimately differs, host vs native, on identical hardware. FMLA/FMLS therefore use inline `asm` in `fp_cases.c` so both sides execute the same instruction.

## Reproduce
```
cc -O1 -ffp-contract=off -o host_fp fp_cases.c host_main.c && ./host_fp > expected-host.txt
# NRO: devkitA64 template/application, SOURCES := ., CFLAGS -O1 -ffp-contract=off, files fp_cases.c + main_nx.c
OUNCE_NRO_BUDGET=4000000000 target/release/ounce-core run-nro nro.nro --present=headless --sdmc=DIR --cpu=tier3   # also tier4, native
cmp DIR/results.txt expected-host.txt
```
