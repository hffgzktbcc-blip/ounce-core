# AArch64 scalar floating-point differential test (found FNMADD/FNMSUB bug)

`fp_cases.c` runs each instruction through inline assembly on 27 operand values (zeros of both signs, subnormals, huge values, infinities, NaN, values at integer boundaries), prints the result bits, and is compiled twice: for the host (Apple Silicon, real ARM64) as the oracle, and as an NRO for Ounce-Core. 5,964 result lines cover `fmadd`, `fmsub`, `fnmadd`, `fnmsub`, `fadd/fsub/fmul/fdiv/fnmul`, `fmax/fmin/fmaxnm/fminnm`, `fabs/fneg/fsqrt`, all `frint*`, `fcvt{z,n,m,p,a}{s,u}` to 32 and 64-bit integers, `fcvt` between single and double, `scvtf/ucvtf`, and `fcmp/fcmpe` flags.

## Failure found
Before the fix, 533 of 5,964 lines differed, all `fnmadd` and `fnmsub`, on Tier-3, Tier-4 and native alike. The interpreter negated the product when `o0` was set; the architecture negates it when `o0 != o1`. `FMADD`/`FMSUB` were unaffected, which is why it went unnoticed. It made devkitA64's `sin`, `cos` and `pow` return wrong values (for example `sin(1) = 1.1585`, `2^53` negative), found via the Lua differential test (`docs/verify/lua-harness`).

## Result (2026-11-08, macOS arm64)
All three engines now match the host on every line. NRO sha256: `af2717e63bcec4fa08123a965bdb859237e23c3dd7c273cfac1b043b00c9c3fb`.
Not covered: single-precision arithmetic, `fccmp`, `fcsel`, vector floating point, exceptions and flags in `fpsr`.
