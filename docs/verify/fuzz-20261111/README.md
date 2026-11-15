# Seed 20261111 and scalar default NaNs (#391, #362)

## Root causes

### Issue #391 (`Nightly fuzz mismatch (seed 20261111)`)

The [#391 nightly artifact](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/43009804952)
from commit `c47fa1468e36316b61a5aff2d54029d98799f90d` reported **zero hardware
value mismatches** on both Tier-3 and Tier-4 (`339,720` implemented cases each).
Its failure was a segmentation fault in the preceding `--engine-vs-engine` run
(`--seed 20261111 --count 1000000`), triggered by `u64::MAX` (`0xffff_ffff_ffff_ffff`)
register inputs colliding with empty sentinel tags in Tier-4:

1. **Indirect link table null tail call (`crates/ounce-recompiler/src/backend.rs::Lowering::try_link`)**:
   `LinkEntry::default()` uses `guest_pc: u64::MAX, host_entry: 0` as the empty-slot
   sentinel. An indirect branch (`BR`/`BLR`/`RET`) to `u64::MAX` matched `entry_pc == pc`
   on an empty slot and tail-called `host_entry == 0`, causing `Segmentation fault: 11`.
   `try_link` now requires `host_entry != 0` (`populated`) before chaining.
2. **Tier-4 fast-cache sentinel collision (`src/cpu/tier4.rs::Tier4JitEngine::lookup_or_compile_fast`)**:
   `fast_cache` is initialized with `(u64::MAX, BlockId::default())`. Once `try_link`
   fell back cleanly, an indirect branch such as `0xd79f01c0` (`BR X14`) jumping to
   `u64::MAX` hit the empty `fast_cache[1023]` slot and looped on `BlockId(0)` until
   the 16-instruction budget instead of faulting at `0xffff_ffff_ffff_ffff`.
   `lookup_or_compile_fast` now returns `None` immediately when `pc & 3 != 0`,
   routing misaligned targets to fault through Tier-3/interpreter.

### Issue #362 (`Tier-4 on x86_64 returns host negative default NaN for scalar FADD/FSUB/FMUL/FDIV/FNMUL`)

[#362](https://github.com/hffgzktbcc-blip/ounce-core/issues/362) was fixed in `main`
by `de2b909db935240198c7b72cf39105b7c5a887c8`: non-AArch64 scalar Cranelift
arithmetic passes through `arm_nan`. It chooses signaling NaNs (quieted) before
quiet NaNs (`a` before `b`), preserves operand payload/sign, and replaces newly
generated host NaNs with Arm's positive default NaN (`0x7fc0_0000` /
`0x7ff8_0000_0000_0000`). `FNMUL` flips the sign afterward. Arm hosts retain their
native arithmetic fast path. Scalar `FMAX`/`FMIN`/`FMAXNM`/`FMINNM` use the
interpreter path in Tier-4, rather than Cranelift's min/max semantics.

## Regression coverage

- `crates/ounce-recompiler/src/lib.rs::tests::indirect_exit_links_through_runtime_pc`:
  verifies runtime dynamic exits miss cleanly on unlinked targets including `u64::MAX`.
- `src/cpu/tier4.rs::tests::indirect_branch_to_empty_link_sentinel_faults_all_tiers`:
  verifies `BR X0` (`0xd61f_0000`), `BLR X0` (`0xd63f_0000`), `RET X0` (`0xd65f_0000`),
  and `BR X14` (`0xd79f_01c0` from `seed 20261111`) targeting `u64::MAX` fault at
  `u64::MAX` across the interpreter, Tier-3, and native Tier-4.
- `src/cpu/tier4.rs::tests::scalar_arithmetic_nan_bits_all_tiers`:
  checks exact `S` and `D` encodings of `FADD`, `FSUB`, `FMUL`, `FDIV`, and `FNMUL`
  with `FPCR=0`, covering finite arithmetic, `inf + -inf`, `inf - inf`, `0 * inf`
  and `inf * 0`, `0 / 0`, `inf / inf`, quiet/signaling NaN priority, payloads,
  signs, and cleared upper lanes across the interpreter, Tier-3, and native Tier-4.
- `crates/ounce-recompiler/src/backend.rs::nan_tests::portable_nan_lowering_rewrites_negative_host_nan`:
  executes `arm_nan` directly with runtime operand/result loads, supplying an
  x86-style negative default NaN (`dn | sign`) and verifying both `I32` and `I64`
  rewrite invalid-operation results to Arm's positive default NaN while preserving
  finite results and NaN operand priority without altering the AArch64 production fast path.

## Seed 20261111 replay (macOS 26.6.2, arm64)

### Hardware trace replay (`1,000,000` words)

```sh
cc -O1 -o /tmp/fuzz_host_391 docs/verify/insn-fuzzer/fuzz_host.c
cargo build --release --locked --features 'native-jit nce'
/tmp/fuzz_host_391 --seed 20261111 --count 1000000 > /tmp/trace-391.txt
for engine in interp tier3 tier4; do
  target/release/ounce-core fuzz-replay --engine "$engine" --show 10 /tmp/trace-391.txt
done
```

Generated trace SHA-256:
`d916f7ebb77978f20677c732708444286b3ca1f8323c04f63df69a5496fcab41`
(not committed; 1.1 GB, regenerate on hardware).

All three engines (`interp`, `tier3`, `tier4`) report identical counts:

| Observation | Count |
| --- | ---: |
| Generated words | 1,000,000 |
| Hardware executed | 403,336 |
| Hardware SIGILL | 596,664 |
| Emulator unimplemented | 45,086 (687 classes) |
| Emulator other errors | 0 |
| Emulator executed / exact X,V,NZCV,QC matches | 358,250 |
| Distinct classes executed | 2,011 |
| Value mismatches | 0 |
| Emulator executes hardware-undefined instruction | 0 |
| Matching values with differing FP exception flags | 1,610 |

Tier-4 reports native instructions for all `358,250` implemented trace cases.
FP arithmetic may fall back under replay because replay sets `cpu.fp_flags_tracked = true`;
native scalar FP execution is covered separately by `scalar_arithmetic_nan_bits_all_tiers`.

### Engine-vs-engine comparison (`seed 20261111`, `1,000,000` words)

```sh
target/release/ounce-core fuzz-replay --engine-vs-engine --seed 20261111 --count 1000000 --show 10
```

| Observation | Count |
| --- | ---: |
| Words generated | 1,000,000 |
| Interpreter unimplemented/other (skipped) | 2 |
| Compared | 999,998 (45,935 distinct classes) |
| Interpreter exits (`budget` / `fault` / `svc`) | 1 / 367,553 / 632,444 |
| Tier-3 vs interpreter MISMATCH | 0 |
| Tier-4 vs interpreter MISMATCH | 0 |
| Words Tier-4 ran natively | 289,975 of 999,998 |

## Validation commands

```sh
cargo test --locked -p ounce-recompiler
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

Both `conformance/arm64-v0.1.0` (1 case) and `conformance/arm64-v0.2.0` (12 cases)
pass on `interp`, `tier3`, and `tier4`. Native x86_64 execution remains a CI
validation requirement because this machine is macOS arm64; locally `arm_nan` is
verified directly with simulated negative host NaNs via
`portable_nan_lowering_rewrites_negative_host_nan`, and the repository's Tier-4 CI
matrix runs on x86_64 Linux/Windows as well as AArch64.
