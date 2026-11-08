# Narrowing and saturating-doubling NEON ops: host differential test

`narrow_cases.c` uses NEON intrinsics so the same C compiles to the real instructions on Apple Silicon (the reference) and, with devkitA64, as an NRO for Ounce-Core. 400 operand sets (random plus edge values: 0, 1, all ones, sign bits, +/-32768 boundaries) through 19 forms: `ADDHN`, `SUBHN`, `RADDHN`, `RSUBHN` (16, 32, 64-bit sources, and the `2` forms that preserve the low half), and `SQDMULL`, `SQDMLAL`, `SQDMLSL` (16 and 32-bit, low and `2` forms). 7,600 result lines.

## Before
All of these stopped with `[VM Error] Unimplemented instruction` (undecoded).

## Result (2026-11-08, macOS arm64)
| Engine | Output vs host reference |
|---|---|
| Tier-3 | byte-identical |
| Tier-4 JIT | byte-identical |
| Native | byte-identical |

NRO sha256: `13c8cbf860627f8104a2400b63e41c19440901f93ba93752544e1270ab2557da`.

## Not covered
The cumulative saturation flag (QC) was not modelled when this check was made; it is now (see `docs/verify/qc-flag`). Saturated values themselves match. Scalar forms (`SQDMULL Sd, Hn, Hm`), by-element forms and `SQDMULH`/`SQRDMULH` are separate instructions and still undecoded.
