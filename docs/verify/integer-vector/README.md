# Integer vector and BRK forms: host differential test

`intvec_cases.c` uses NEON intrinsics, so the same C compiles to the real instructions on Apple Silicon (the reference, `cc -O1`) and, with devkitA64, as an NRO for Ounce-Core. 400 operand sets (random plus edge values: 0, 1, all ones, sign bits, +/-0x8000 boundaries, 0x7F7F..., 0x80808080) through 172 forms, 68,800 result lines. `otool -tv` of the host binary confirms the instructions are really emitted (not constant-folded).

## Covered (new decoder/executor in this change)
- Saturating narrow: `SQXTUN`, `SQXTN`, `UQXTN` (+ `2` forms, 16/32/64-bit sources) and scalar `SQXTN/UQXTN/SQXTUN Bd/Hd/Sd`.
- Narrowing shifts: `RSHRN`, `SQSHRN`, `UQSHRN`, `SQRSHRN`, `UQRSHRN`, `SQSHRUN`, `SQRSHRUN` (+ `2` forms and scalar).
- Accumulate/round: `SSRA`, `USRA`, `SRSHR`, `URSHR`, `SRSRA`, `URSRA` (all lane sizes, 64-bit D forms, scalar D forms; shift == lane width included).
- Insert: `SRI`, `SLI`. Saturating left shift by immediate: `SQSHL`, `UQSHL`, `SQSHLU` (vector and scalar).
- Pairwise long: `SADDLP`, `UADDLP`, `SADALP`, `UADALP`; scalar `ADDP Dd, Vn.2D`.
- Across lanes `ADDV`, `UADDLV`, `SADDLV`, `UMAXV`, `SMAXV`, `UMINV`, `SMINV` already decoded; included here as a regression check (all forms match).
- `BRK #imm`: stops the guest with `BRK #0x3e8 breakpoint hit at PC: 0x...` (an `Err(CpuError::Breakpoint)`, PC left on the BRK). Checked with `__builtin_trap()` on Tier-3 and Tier-4 (message above); native takes the real trap (SIGTRAP, reported as a native CPU fault). Unit test `brk_stops_the_guest`.

The census word `0x4ea0b800` class is not ADDV (ADDV already decoded): it is the `FCVTZS`/`FCVTZU` vector form (and ABS/NEG), left to the vector-FP work.

## Result (2026-11-08, macOS arm64, Apple Silicon host)
| Engine | Output vs host reference |
|---|---|
| Tier-3 | byte-identical (68,800 lines) |
| Tier-4 JIT | byte-identical |
| Native (`nce`, runs on the real CPU) | byte-identical |

Native executes the guest instructions on real hardware, so it is the evidence that the host reference and the test program agree; Tier-3 and Tier-4 are the emulator evidence. Before the change the NRO stopped at `[VM Error] Unimplemented instruction` on SQXTUN/RSHRN/etc.; two extra words (`ADDP Dd,Vn.2D` `0x5EF1B8..`, `UADDLP` `0x2EA028..`) were found by this run (compiler output for `vaddvq_u64`, `vaddlv_u32`) and added. No silent mismatch was found.

NRO sha256: `def0b2f6aefe21a4409534183a4725489f467015a9e3efef17b1a9779554f607` (devkitA64 gcc 16.1.0, template `application`, `SOURCES := src`).

## Reproduce
```
cc -O1 -o host/host intvec_cases.c host/host_main.c && ./host/host > expected-host.txt
# NRO: copy intvec_cases.c + main_nx.c into <template>/src, SOURCES := src, make
cargo build --release --features "nce native-jit"
docs/verify/integer-vector/run_engines.sh X.nro WORKDIR
```

## Not covered
- The cumulative saturation flag (`FPSR.QC`) is not modelled: saturating ops produce correct values but QC is never set. All saturation goes through `simd::saturate` (returns the value and a "saturated" bool) called from `simd::shift_ext`; wiring QC means collecting that bool there.
- Not tested by the differential: `SQXTN/SQSHRN` etc. with the `2` form on every size, scalar `SQSHL` register forms, by-element forms, `SQRSHL/UQRSHL/SQSHL` (register), `SQABS/SQNEG`, `SUQADD/USQADD`, `UQADD/SQADD` etc., `SHLL/SHLL2` (shift by lane size), `USHLL`/`SSHLL` fixed-point corner cases beyond existing tests.
- Vector `FCVTZS/FCVTZU`, `FCMxx`, float across-lanes (`FMAXV`) and pairwise float are vector-FP work, not done here.
- Tier-2 and the interpreter share the same executor and are not run separately by the NRO test.
