# FPSR.QC (cumulative saturation) and FPSR/FPCR state: host differential test

`qc_cases.c` clears FPSR with `msr fpsr, xzr`, runs one saturating NEON op in inline asm, and reads `mrs x, fpsr` back, printing the QC bit (27) with the result vector. 300 operand sets (random plus edge values) through 8 forms: `SQDMULL`, `SQDMLAL`, `SQDMLSL` (16 and 32-bit sources, low forms) and `SQDMULL2` (16 and 32-bit), plus a sticky/clear sequence, an FPSR write-mask check (`msr fpsr, 0xffffffff` then read) and an FPCR round trip. 2,403 lines. The same C builds with `cc -O1` on Apple Silicon (reference, `expected-host.txt`) and with devkitA64 as an NRO.

## What is modelled
- `FPSR` and `FPCR` live in `CpuRegisters` (`fpsr`, `fpcr`); `mrs`/`msr` for both (`S3_3_C4_C4_1`, `S3_3_C4_C4_0`) are handled in `Cpu::step`.
- FPSR keeps only the AArch64-defined bits (`0x0800009F`: QC, IDC, IXC, UFC, OFC, DZC, IOC). Matches the host (`msr fpsr, 0xffffffff` reads `0800009f`).
- `FPSR.QC` is set by `SQDMULL`/`SQDMLAL`/`SQDMLSL` (+ `2` forms) when any lane saturates, in the doubling multiply (min*min) or in the accumulate. It is sticky until software writes FPSR.
- `svcGetThreadContext3` now returns FPCR/FPSR from the registers.
- Tier-3 and Tier-4 do not lower `SystemReg` or `SimdSatLong`; both fall back to `Cpu::step`, so they share the interpreter behaviour (unit test `qc_flag_matches_interpreter_on_all_tiers`, interpreter vs Tier-3 vs Tier-4 incl. FPSR/FPCR in the compared state).

## What is not modelled
- FPCR is stored and read back verbatim (RES0 bits included, unlike hardware). Rounding mode, flush-to-zero, default-NaN, trap enables are NOT honoured by any FP instruction.
- No cumulative exception flags (IOC, DZC, OFC, UFC, IXC, IDC) are ever set by FP instructions; they are only writable and readable.
- Other saturating instructions set QC too: the narrows and shifts (see `saturating-shifts/`) and `SQADD`/`UQADD`/`SQSUB`/`UQSUB` (see `saturating-add-sub/`). Still undecoded, so no QC: `SQDMULH`/`SQRDMULH`, scalar `SQDMULL`, `SQABS`/`SQNEG`, register-shift `SQSHL`/`UQSHL`.
- Native (`--cpu=native`) runs on real hardware; FPSR is not saved/restored across SVC entry/exit in the context struct (guest and host share the hardware register in between).

## Result (2026-11-08, macOS arm64)
| Engine | Output vs host reference |
|---|---|
| Tier-3 | byte-identical (2,403 lines; 392 with qc=1) |
| Tier-4 JIT | byte-identical |
| Native | byte-identical (checks the harness only) |

NRO sha256: `58f523792ef62281dd7a3efde92635d1ef296197cb81482649c3108e7bd2fae6`. Before this change every `qc=` was 0 on the emulator.

## Reproduce
```
cc -O1 -o qc_host host/host_main.c qc_cases.c && ./qc_host > expected-host.txt
export DEVKITPRO=/opt/devkitpro DEVKITA64=/opt/devkitpro/devkitA64
# copy examples/switch/templates/application, set SOURCES := ., put qc_cases.c and main_nx.c in the project root, make
OUNCE_NRO_BUDGET=4000000000 target/release/ounce-core run-nro qc.nro --present=headless --sdmc=DIR --cpu=tier3   # or tier4 / native
cmp DIR/results.txt expected-host.txt
```
