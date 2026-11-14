# Seed 20261111 and scalar default NaNs (#391, #362)

## What the issues actually reported

The [#391 nightly artifact](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/43009804952)
from commit `c47fa1468e36316b61a5aff2d54029d98799f90d` contains **zero hardware
value mismatches** on both Tier-3 and Tier-4 (339,720 implemented cases each).
Its failure is a segmentation fault in the preceding `--engine-vs-engine` run.
The issue's automatic title, “Nightly fuzz mismatch”, does not distinguish a
crash from a mismatch. There are no diverging instruction encodings in that
artifact to repair.

[#362](https://github.com/hffgzktbcc-blip/ounce-core/issues/362) was already
fixed by `de2b909db935240198c7b72cf39105b7c5a887c8` before this work: non-AArch64
scalar Cranelift arithmetic passes through `arm_nan`. It chooses signaling NaNs
before quiet NaNs, preserves operand payload/sign, and replaces a newly generated
host NaN with Arm's positive default NaN. FNMUL flips the sign afterward. Arm
hosts retain their native arithmetic fast path. Scalar FMAX/FMIN/FMAXNM/FMINNM
use the interpreter path in Tier-4, rather than Cranelift's min/max semantics.
The earlier vector FMUL regression (#267) likewise distinguishes invalid
zero-times-infinity from input NaN propagation; scalar correction must preserve
that distinction as well.

## Regression coverage

`cpu::tier4::tests::scalar_arithmetic_nan_bits_all_tiers` checks the exact S/D
encodings of FADD, FSUB, FMUL, FDIV and FNMUL with FPCR=0, finite arithmetic,
inf + -inf, inf - inf, zero * inf in either operand order, zero / zero,
inf / inf, quiet/signaling NaN priority, payloads, signs and cleared upper lanes.
Each case executes on the interpreter, Tier-3 and native Tier-4; the test requires
nonzero native instruction counts.

`backend::nan_tests::portable_nan_lowering_rewrites_negative_host_nan` executes
the same portable lowering on either host architecture, loading operand/result
bits at runtime. It supplies an x86-style negative default NaN and checks the
positive Arm result in both widths, plus finite results and NaN operand priority.
The lowering helper was extracted without changing the production host gate;
this makes the x86 correction testable on Apple Silicon without adding overhead
to the Arm arithmetic fast path.

## Hardware replay

Measured on macOS 26.6.2, arm64, starting from main `1e9c4d6`:

```sh
cc -O1 -o /tmp/fuzz_host_391 docs/verify/insn-fuzzer/fuzz_host.c
cargo build --release --locked --features 'native-jit nce'
/tmp/fuzz_host_391 --seed 20261111 --count 1000000 > /tmp/trace-391.txt
for engine in interp tier3 tier4; do
  target/release/ounce-core fuzz-replay --engine "$engine" --show 10 /tmp/trace-391.txt
done
```

Generated trace SHA-256:
`d916f7ebb77978f20677c732708444286b3ca1f8323c04f63df69a5496fcab41`.
The trace is intentionally not committed (1.1 GB); regenerate it on hardware.

All three engines report identical totals:

| Observation | Count |
| --- | ---: |
| Generated words | 1,000,000 |
| Hardware executed | 403,336 |
| Hardware SIGILL | 596,664 |
| Emulator unimplemented | 45,086 |
| Emulator other errors | 0 |
| Emulator executed / exact X,V,NZCV,QC matches | 358,250 |
| Value mismatches | 0 |
| Emulator executes hardware-undefined instruction | 0 |
| Matching values with differing FP exception flags | 1,610 |

Tier-4 reports native instructions for all 358,250 implemented trace cases;
FP arithmetic may still fall back because replay requests FPSR tracking. The
native scalar unit test above separately proves those arithmetic paths. This
replay does not establish support for the unimplemented instructions or full
FP exception-flag accuracy. Hardware instruction support differs from the
original runner, which executed 400,326 words.

## Commands for validation

```sh
cargo test --locked -p ounce-recompiler portable_nan_lowering
cargo test --locked --features native-jit scalar_arithmetic_nan_bits_all_tiers
cargo test --locked
cargo test --locked --features native-jit
bash scripts/check_legal_guardrails.sh
git diff --check
for version in 0.1.0 0.2.0; do
  for engine in interp tier3 tier4; do
    python3 scripts/run_conformance.py \
      --suite "conformance/arm64-v$version" \
      --report "/tmp/conformance-$version-$engine.json" -- \
      target/release/ounce-core fuzz-replay --engine "$engine"
  done
done
```

Both versioned conformance suites pass on all three engines. The portable
negative-NaN lowering test and scalar native test pass locally. Native x86_64
execution remains a CI validation requirement; this Mac is arm64. The existing
Tier-4 CI matrix runs both the recompiler tests and native-jit core tests on
x86_64 Linux/Windows and AArch64 Linux/macOS.
