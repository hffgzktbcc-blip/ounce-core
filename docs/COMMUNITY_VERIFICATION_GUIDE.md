# Check an ARM64 program against a reference

Ounce-Core runs **supported static ARM64 Linux ELF programs and unencrypted homebrew**, compares results across CPU engines, and records differences. You can use it to investigate a library test without console firmware, retail keys, or a proprietary console SDK.

**Scope:** Not every static ELF or homebrew program runs. A recorded native-hardware result requires a real ARM64 host or an independently captured reference; engine agreement on one machine is not proof of hardware agreement. Check [supported limits](LIMITS.md) before treating a match as evidence.

---

## 1. Quick Start: Verify Your Own AArch64 Library

If you maintain an open-source C or Rust library (such as a parser, codec, rasterizer, or math kernel), start with a small, deterministic test harness. Use an AArch64 host when you need a native-hardware comparison:

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

## 2. Test a pinned library across optimisation levels

`scripts/verify_linux_elf.py` checks **one named harness** at a time against a host build. It accepts `--harness`, `--work`, `--elf-cc` and a comma-separated value for `--opt-matrix`; these arguments are not optional shortcuts. The work directory must be new for each run.

Build Ounce-Core first, then use a static AArch64 compiler (for example, Zig installed with `pip install ziglang==0.17.1`):

```sh
cargo build --release --locked --features "nce native-jit"
python3 scripts/verify_linux_elf.py \
  --harness yyjson \
  --work /tmp/ounce-yyjson-new-run \
  --elf-cc "python3 -m ziglang cc -target aarch64-linux-musl" \
  --opt-matrix O1,O2,O3_M1 \
  --diverge
```

A run may fail because the program uses an unsupported instruction or system call. Record failures instead of treating matching stdout alone as a pass.

**To check the broader Linux ARM64 matrix**, use `scripts/run_linux_arm64_matrix.py` and [its first-party workload guide](verify/elf-workloads/README.md). The **51-harness matrix passed in CI at the v0.10.0 tag** ([exact job](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/44746386566/job/136322674982)). That count does **not** mean every harness is compatible with every host, optimisation flag, or engine.

---

## 3. Source and safety boundaries

- **No retail material:** Use open-source and first-party tests. Do not submit firmware, console keys, proprietary SDK code, or retail binaries. Ounce-Core is **not entirely clean-room**: attributed ports from permissively licensed sources are documented in [CLEAN_ROOM.md](CLEAN_ROOM.md) and [PROVENANCE.md](PROVENANCE.md).
- **Safe-Rust boundary:** `src/` forbids `unsafe` code; the project isolates necessary unsafe operations in separate crates. This does not automatically establish a security sandbox.
- **Recorded evidence:** A `diverge --json` report documents which engines ran, how outputs compared, and whether a native reference was used. A result is only as broad as the programs and inputs checked.

To contribute a new open-source library harness to the gauntlet, see [`docs/ADD_A_PROGRAM.md`](ADD_A_PROGRAM.md).
