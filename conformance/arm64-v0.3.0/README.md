# ARM64 conformance 0.3.0

Twelve MIT-licensed, real Apple M3 hardware captures extending the published 0.1.0
and 0.2.0 suites to **non-default `FPCR` inputs** (`RMode=RP/RM/RZ`, `FZ`, `DN`,
`FZ+DN`, `FZ+DN+RZ`, `FZ16`, and `FZ+DN+FZ16+RZ`) without modifying any published
0.1.0 or 0.2.0 bytes (#470). These are measured single-instruction regression inputs,
not ISA certification, broad coverage, or independent validation.

## Capture and reproduction

Captured locally on 2026-11-20 on Apple M3 using `docs/verify/insn-fuzzer/fuzz_host.c`
with `--seed 1 --count 6000 --fpcr-mix`. Exact OS build, compiler, UTC timestamp,
generator SHA-256, trace line number, and command are recorded per case in `suite.json`.
No expected output is synthesized.

```sh
cc -O1 -Wall -Wextra -o /tmp/fuzz-host-v030 docs/verify/insn-fuzzer/fuzz_host.c
/tmp/fuzz-host-v030 --seed 1 --count 6000 --fpcr-mix
```

Each trace uses the `TF <fpcr> <word> ...` one-line register-state format where
`<fpcr>` is the non-zero 32-bit `FPCR` input value set before executing the instruction.
The manifest (`schema: "ounce-core/conformance-v2"`) pins each case's exact trace
SHA-256, 32-bit instruction word, and 8-hex `fpcr` input; `scripts/run_conformance.py`
validates all three before invoking any adapter.

## Cases

| ID | Word | FPCR (`mode`) | Instruction |
|---|---|---|---|
| `rmode-rp-6eb2ece0` | `6eb2ece0` | `00400000` (`rmode-rp`) | `facgt.4s v0, v7, v18` |
| `rmode-rm-0e21b979` | `0e21b979` | `00800000` (`rmode-rm`) | `fcvtms.2s v25, v11` |
| `rmode-rz-6ee1fb4d` | `6ee1fb4d` | `00c00000` (`rmode-rz`) | `fsqrt.2d v13, v26` |
| `fz-4ee18aec` | `4ee18aec` | `01000000` (`fz`) | `frintp.2d v12, v23` |
| `dn-6ea0daa8` | `6ea0daa8` | `02000000` (`dn`) | `fcmle.4s v8, v21, #0.0` |
| `fz-dn-1f43e4ef` | `1f43e4ef` | `03000000` (`fz-dn`) | `fmsub d15, d7, d3, d25` |
| `fz-dn-rz-4e217940` | `4e217940` | `03c00000` (`fz-dn-rz`) | `fcvtl2 v0.4s, v10.8h` |
| `fz16-1fc1b5bf` | `1fc1b5bf` | `00080000` (`fz16`) | `fmsub h31, h13, h1, h13` |
| `fz-dn-fz16-rz-1fca8937` | `1fca8937` | `03c80000` (`fz-dn-fz16-rz`) | `fmsub h23, h9, h10, h2` |
| `rmode-rp-2e3ffc7d` | `2e3ffc7d` | `00400000` (`rmode-rp`) | `fdiv.2s v29, v3, v31` |
| `fz-4e73c4cc` | `4e73c4cc` | `01000000` (`fz`) | `fmaxnm.2d v12, v6, v19` |
| `dn-6e21a80d` | `6e21a80d` | `02000000` (`dn`) | `fcvtnu.4s v13, v0` |

## Qualify locally

```sh
cargo build --release --locked --features native-jit
python3 -m unittest discover -s scripts -p test_run_conformance.py
for version in 0.1.0 0.2.0 0.3.0; do
  for engine in interp tier3 tier4; do
    python3 scripts/run_conformance.py --suite conformance/arm64-v$version \
      --report /tmp/arm64-v$version-$engine.json -- \
      target/release/ounce-core fuzz-replay --engine $engine
  done
done
```

Preserve published versions (`0.1.0`, `0.2.0`, `0.3.0`); expand or change references
only in a new version.
