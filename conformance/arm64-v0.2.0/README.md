# ARM64 conformance 0.2.0

Twelve MIT-licensed, real Apple M3 hardware captures expand the published 0.1.0
seed without changing its bytes. These are measured single-instruction regression
inputs, not ISA certification, broad coverage, or independent validation.

## Capture and reproduction

Captured locally on 2026-11-13 on Apple M3. Exact OS build, compiler, UTC timestamp,
generator SHA-256 and command are recorded per case in `suite.json`. The source
is `docs/verify/insn-fuzzer/fuzz_host.c`; its deterministic named modes initialize
explicit register bits and print actual hardware output. No expected output is
synthesized. The original `default-nan-mul2d` input was recaptured here; 0.1.0
retains its original capture and provenance.

```sh
cc -O1 -Wall -Wextra -o /tmp/fuzz-host-v020 docs/verify/insn-fuzzer/fuzz_host.c
/tmp/fuzz-host-v020 --case default-nan-mul2d
# Replace the case name with any ID below to reproduce that trace.
```

Each instruction executes once inside the hardware MAP_JIT register-state
wrapper. Replay places that word at guest address 0x1800 followed by an SVC stop
sentinel. The hardware wrapper itself is dynamically mapped, not fixed at 0x1800.
Inputs use FPCR=0 and FPSR=0; compare X0–X30 except macOS-reserved X18, all 32 raw
128-bit V registers, all four NZCV bits, and FPSR.QC (mask 0x08000000).
Other FPSR exception flags are captured but excluded from matching. PC, SP,
memory, sequences, timing and other FPCR modes are outside this suite.

The manifest pins the exact SHA-256 of each one-line trace and its word encoding.
Preserve published versions; expand or change references in a new version.

## Cases

| ID | Word | Instruction | Measured edge |
|---|---|---|---|
| default-nan-mul2d | 6e62dc20 | FMUL V0.2D,V1.2D,V2.2D | zero × infinity in both lanes |
| snan-add-d | 1e622820 | FADD D0,D1,D2 | signaling NaN payload quieting |
| qnan-fmaxnm-d | 1e626820 | FMAXNM D0,D1,D2 | negative quiet NaN versus finite number |
| frintn-tie-d | 1e644020 | FRINTN D0,D1 | +2.5 rounds to even |
| frinta-negative-tie-d | 1e664020 | FRINTA D0,D1 | −2.5 rounds away from zero |
| fcmp-unordered-d | 1e622020 | FCMP D1,D2 | unordered NZCV |
| fcvtzs-negative-d | 9e780020 | FCVTZS X0,D1 | −3.75 truncates toward zero |
| sqadd-saturate-16b | 4e220c20 | SQADD V0.16B,V1.16B,V2.16B | positive/negative saturation sets QC |
| adds-signed-overflow | ab020020 | ADDS X0,X1,X2 | signed maximum plus one |
| adcs-carry-wrap | ba020020 | ADCS X0,X1,X2 | unsigned maximum plus carry |
| sbfm-sign-extend | 93483c20 | SBFM X0,X1,#8,#15 | negative byte sign extension |
| bfm-wrap-insert | b3781c20 | BFM X0,X1,#56,#7 | wraparound insertion preserves bits |

## Qualify locally

```sh
cargo build --release --features native-jit
python3 -m unittest discover -s scripts -p test_run_conformance.py
for version in 0.1.0 0.2.0; do
  for engine in interp tier3 tier4; do
    python3 scripts/run_conformance.py --suite conformance/arm64-v$version \
      --report /tmp/arm64-v$version-$engine.json -- \
      target/release/ounce-core fuzz-replay --engine $engine
  done
done
```

Report paths must be new. The runner validates every hash and instruction record
before invoking any adapter. Exit 0 alone is insufficient: each case must report
one executed and matched instruction with no unsupported instructions, errors,
mismatches or host-undefined words. See 0.1.0's README for the adapter count
contract and trace format; this version uses the same comparison contract.

All three named engines use their existing replay paths, including documented
fallbacks; an engine pass does not establish that every word was native compiled.
In particular, replay tracks FP flags and may fall back for FP arithmetic.
Outside-project execution and adoption remain outstanding.

Local qualification observations are retained in
[`docs/verify/conformance-v020/qualification.json`](../../docs/verify/conformance-v020/qualification.json):
both versions passed on all three engines (39 matched case executions total),
using the recorded runtime revision and native-jit release build. This records
only these fixed inputs, not general engine compatibility.
