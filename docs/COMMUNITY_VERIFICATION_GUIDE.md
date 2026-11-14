# Community Verification & Multi-Opt Guide

`ounce-core` is built to make **evidence-first AArch64 verification** accessible to open-source C, C++, and Rust library maintainers without requiring proprietary hardware or specialized lab setups.

Using `ounce-core`, you can compile any static AArch64 Linux ELF or unencrypted `.nro` homebrew binary and verify it across multiple optimization levels, CPU tiers, and Apple Silicon hardware replay.

---

## 1. Quick Start: Verify Your Own AArch64 Library

If you maintain an open-source C or Rust library (such as a parser, codec, rasterizer, or math kernel), you can run differential verification against `ounce-core` in minutes:

### Step 1: Compile a deterministic test harness
Compile your library + self-test harness as a static AArch64 Linux ELF across multiple compiler optimization flags (e.g. `-O1`, `-O2`, `-O3`, and `-O3 -mcpu=apple_m1`):

```sh
aarch64-linux-gnu-gcc -static -O2 harness.c -lm -o harness_O2.elf
aarch64-linux-gnu-gcc -static -O3 -mcpu=apple_m1 harness.c -lm -o harness_O3_m1.elf
```

### Step 2: Run Cross-Tier Divergence Checks (`ounce-core diverge`)
Run `ounce-core diverge` on your binary to execute it deterministically across the **Interpreter**, **Tier-3**, and **Tier-4 JIT** engines:

```sh
cargo run --release --features native-jit -- diverge harness_O3_m1.elf --json diverge_report.json
```

`diverge` automatically verifies that:
- Process exit codes match identically across all engines.
- Filtered `stdout` and `stderr` match byte-for-byte.
- Stop reasons (`exited`, `budget`, `fault`, etc.) match with zero silent Tier-4 fallback.
- On Linux AArch64 hosts, `diverge` also runs the ELF natively on host silicon as the ground-truth reference.

*(See [`docs/DIVERGE.md`](DIVERGE.md) for full details and options.)*

---

## 2. Multi-Opt Matrix Harness (`scripts/verify_linux_elf.py`)

In `ounce-core`, we maintain a **27-program differential verification gauntlet** (`59` optimization matrix rows) covering standard C libraries, compression codecs, parsers, and math kernels—including `yyjson`, `stb_truetype.h`, `zstd`, `cJSON`, `zlib`, `miniz`, `lz4`, `xxHash`, `sqlite3`, `sha256`, and `lua`.

You can run the full multi-optimization matrix locally:

```sh
# Run all 27 harnesses across -O1, -O2, -O3, and -O3 -mcpu=apple_m1
python3 scripts/verify_linux_elf.py --opt-matrix --diverge
```

Or target a single harness across all optimization tiers:

```sh
python3 scripts/verify_linux_elf.py --program yyjson --opt-matrix --diverge
```

---

## 3. Clean-Room & Safety Guarantees

- **Zero proprietary material**: All verification workloads are clean-room open-source C/Rust test harnesses or unencrypted open-source `.nro` homebrew.
- **Safe-Rust core**: `src/` enforces `#![forbid(unsafe_code)]`, keeping memory isolation and execution boundaries verifiable.
- **Reproducible JSON telemetry**: Every `diverge` run can output a structured `--json` report detailing checked engines, mismatch counts, and reference host status.

To contribute a new open-source library harness to the gauntlet, see [`docs/ADD_A_PROGRAM.md`](ADD_A_PROGRAM.md).
