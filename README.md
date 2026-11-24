<div align="center">

# Ounce-Core checks how ARM64 programs behave

Ounce-Core runs supported ARM64 programs through three CPU engines, compares their results with references recorded on real ARM64 hardware, and helps developers investigate differences.

[![CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml/badge.svg)](https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml)
[![First-frame oracles](https://img.shields.io/badge/first--frame_oracles-50%2F50_pass-84b89e?style=flat-square)](docs/DETAILS.md#legacy-switch-regression-corpus)
[![MIT](https://img.shields.io/badge/License-MIT-84b89e?style=flat-square)](LICENSE)

[![Ounce-Core in 90 seconds: one program, three engines, one hardware reference. Click to watch the full video.](docs/media/ounce-core-hero.gif)](docs/media/ounce-core-main.mp4)

<sub>[Watch the full 90-second video](docs/media/ounce-core-main.mp4)</sub>

**[Try it in your browser](https://hffgzktbcc-blip.github.io/ounce-core/playground/)** · **[Latest published release](https://github.com/hffgzktbcc-blip/ounce-core/releases/latest)**

</div>

## What it is

You ship a library built on x86. Will it give the same answer on ARM64? You may not own an ARM64 machine to find out.

Ounce-Core runs supported programs through three CPU engines and compares their outputs with results recorded on real ARM64 hardware. When results differ, its reports and bounded traces help developers find where the runs split. Ounce-Core is open source, written in Rust, and licensed under MIT. The [vision](docs/VISION.md) explains the aim.

**It is not only for homebrew.** The same engines run static Linux ARM64 ELFs (`ounce-core run-elf`) and unencrypted NRO binaries (`run-nro`): open-source C libraries and VMs such as MicroPython, QuickJS, Lua, SQLite, libpng, libdeflate, bzip2, brotli, PCRE2, LZ4, and xxHash, plus first-party workloads. You can also use it to check published results: a [paper check](docs/verify/papers/README.md) rebuilds a paper's algorithm as an ARM64 program, runs it and compares the output with the numbers the paper prints.

**Why `ounce-core` instead of `qemu-aarch64`?** `ounce-core` executes guest code inside deterministic `#![forbid(unsafe_code)]` Rust with built-in cross-engine differential comparison (Interpreter, Tier-3, Tier-4, and native hardware), zero-dependency ELF symbolication with frame-pointer backtraces, and a typed Rust library API (`ounce_core`). When runs diverge or fault, `--report-json=-` emits flat `ounce.run.v1` JSON reports alongside bounded `ounce.trace.v1` instruction traces rather than just a process exit code.

## A real bug in one minute

**Lua finished running, but it gave the wrong answer.** In a recorded November 2026 test, `math.sin(1)` produced about `1.1585` instead of the expected `0.84147`. Three tested execution paths agreed with each other, but a separately built Apple Silicon reference exposed the mistake.

Ounce-Core's floating-point checks isolated the bug to two instructions. The maintainers fixed them and recorded a matching rerun. **[See the before-and-after, the real evidence and how to rerun it](docs/community/LUA_FP_BUG_WALKTHROUGH.md)**. You can read the case in your browser without installing anything.

## What it has done

- **It catches real bugs across engines and hardware.** Differential testing against Apple Silicon and `qemu-aarch64` exposed prefetch register corruption, Lua `sin`/`cos`/`pow` sign bugs, vector `.4H`/`.8H` `FABS`/`FNEG` sNaN handling (`#543`), `LD2–LD4`/`ST2–ST4`/`LD2R–LD4R` structure loads/stores (`#547`), `tkill`/`tgkill` `128+signal` exits (`#549`), `newfstatat` path handling (`#551`), full scalar FP16 arithmetic, `FCVT`, `FMOV`, `FRINT*`, and `int<->half` conversions (`#553`, `#554`), and FEAT_LSE atomics (`LD<op>`, `SWP`, `CAS`, `CASP`, `LDAPR`) that ran as plain loads and stores, so programs built with `-mcpu=apple_m1` computed wrong atomics (`#559`) — all fixed with regression tests. Scalar FP16 `FRECPE`/`FRSQRTE`/`FRECPS`/`FRSQRTS`/`FABD` and the `FCM*`/`FAC*` compares match an Apple M3 bit for bit over 595,472 cases (`#558`).
- **It checks floating point, compilers, and real C/VM workloads (measured on `baec66b`).** Berkeley TestFloat 3e (`ARM-VFPv2`, tininess before rounding) passes `5,921,280` vectors across all 9 groups (`f16`/`f32`/`f64` arithmetic, FMA, `f16<->f32/f64`, `int<->half`, `float<->int`) with `0` real mismatches vs `qemu-aarch64` (`656` FMA NaN-priority cases match `qemu-aarch64`; 6 sampled confirmed addend-NaN on Apple M3), plus `144/144` `FPCR.FZ`/`FZ16`/`DN` cases and `908/908` scalar `f16` `FMOV`/`#imm`/fixed-point checks across Interpreter, Tier-3, and Tier-4. Differential runs vs `qemu-aarch64` also match on Csmith 2.4.0 (`2,815` compared across `-O1`–`-O3 -mcpu=apple_m1`, `0` differ; `385` timed out under QEMU), YARPGen v2 (`498/500`; 2 exceeded instruction budget), `gcc.c-torture/execute` (`1,586` at `-O0`, `1,602` at `-O2` and `-O3 -mcpu=apple_m1`; 3 temp-file tests differ), CompCert `test/c` (`69/72`; `knucleotide` needs `stdin`), `PolyBench/C` (`60/60` vs QEMU and native x86), `musl libc-test` (`276` pass, `198/199` math; 48 differ on unsupported threads/files/signals), MicroPython (`581/582` `tests/basics` vs QEMU), QuickJS (`5/5` at `-O3 -mcpu=apple_m1`), `libpng 1.6.59` (`168/168`), `libdeflate` (`1,800` round-trips, `0` failures; `1.25M` vs `17.8M` instructions proving `CRC32`/`PMULL`/`EOR3`), `bzip2` (`140/140`), `brotli` (`205/205`), `PCRE2` (`51/51` patterns, `78` matches), the 51-harness Linux ARM64 ELF CI matrix, and the <!-- corpus-headline -->50/50<!-- /corpus-headline --> first-frame NRO oracle corpus. These results describe bounded tests, not general compatibility.
- **It runs real crypto, JSON, image and numerical libraries (23 Nov 2026, Apple M3, [details and caveats](docs/verify/bigtech-2026-11-23.md)).** On the interpreter, Tier-3 and Tier-4 unless noted: BLAKE3 `822/822` and libmpdec `148/148` decTest files at `-O2` and `-O3`; s2n-bignum `384` passed, `0` failed, `13` skipped (they need `FEAT_AES`, not implemented); libwebp `57/57` (`-O3` interpreter not run); simdjson `6/6`; secp256k1 `119/119` on Tier-4 only (its Tier-3 and interpreter runs were stopped or killed before finishing, which is not a pass); SGP4 `962/1,222` lines bit-identical to a macOS-libm oracle, with the remaining 260 differing by 1 to 2 ulp (consistent with macOS vs musl libm, not classified).
- **It reproduces published results and numerical invariants.** `102/102` [paper checks](docs/verify/papers/README.md) rebuild algorithms from papers or specs as ARM64 programs, run them on `ounce-core`, and compare the ARM64 output against both the paper's published numbers and a native host build of the same reference code (some compare only a few values or secondary sources: each folder says which). Kahan's `Paranoia` passes `4/4` (*"No failures, defects nor flaws… Excellent!"*), Shewchuk adaptive predicates pass `90,000` near-degenerate cases (`0` mismatches vs exact/QEMU/x86), and physics/quantum invariants (Kepler, Crank-Nicolson tunnelling, 2D Ising, Grover, QFT) pass `9/9`.
- **It is safe Rust.** The runtime in `src/` forbids `unsafe` code, and the compiler enforces it. The two small crates that need `unsafe` are kept apart, and CI checks the boundary.

**Limit:** this is a testing tool, not a fast emulator. It runs static Linux ARM64 ELFs and unencrypted NRO binaries; running arbitrary or dynamically linked Linux applications is not established. Instruction coverage is partial, and a match is evidence for the inputs tested, not proof ([limits](docs/LIMITS.md)).

## Try it

Build, run the hello fixture, and write an `ounce.run.v1` report: [docs/QUICKSTART.md](docs/QUICKSTART.md).

**1. In your browser.** Step through ARM64 code one instruction at a time in the [playground](https://hffgzktbcc-blip.github.io/ounce-core/playground/). Nothing to install.

**2. On your machine.** Download a prebuilt binary from [Releases](https://github.com/hffgzktbcc-blip/ounce-core/releases) or build from source (pure-Rust headless default on Rust 1.89+ with zero C system libraries; pass `--features host-io` only if desktop audio/controller input is wanted, which needs `libasound2-dev libudev-dev` on Debian/Ubuntu; `--workspace` / `--features native-jit` requires current stable Rust):

```bash
cargo install --git https://github.com/hffgzktbcc-blip/ounce-core
ounce-core run-elf path/to/program.elf --present=headless --report-json=-
# Or from a local checkout, run an unencrypted NRO fixture:
cargo run --release -- run tests/fixtures/nro/hello/hello.nro fixture
```

`run-elf` executes a static Linux ARM64 ELF and emits pure `ounce.run.v1` JSON to `stdout` (diagnostics and the guest's own output on `stderr`) with `--report-json=-`; the `hello.nro` fixture prints `result=0x00000EEE`. The default engine is Tier-3 (`--report-json=run.json` or `OUNCE_REPORT_JSON` also writes to a file). To run your Rust tests on aarch64 Linux through Ounce-Core, see [`cargo test` as a runner](#run-cargo-test-through-ounce-core) below. To check your own program against a host build, see [Add a program](docs/ADD_A_PROGRAM.md) or the [Community Verification Guide](docs/COMMUNITY_VERIFICATION_GUIDE.md). To find where a run first goes wrong, see the [diagnosis guide](docs/DIAGNOSIS_CLI.md).

### Run `cargo test` through Ounce-Core

Cargo can use `ounce-core` as the runner for `aarch64-unknown-linux-musl` test binaries. Put this in `.cargo/config.toml`:

```toml
[target.aarch64-unknown-linux-musl]
runner = "ounce-core run-elf --"
```

On macOS the default link flags fail, and `ounce-core` rejects static-PIE, so build a static non-PIE binary with `rust-lld`:

```sh
rustup target add aarch64-unknown-linux-musl
RUSTFLAGS="-C linker=rust-lld -C target-feature=+crt-static -C relocation-model=static" \
  cargo test --target aarch64-unknown-linux-musl
```

Plain `cargo test` runs without `--test-threads=1`: libtest attempts to spawn a thread per test, `ounce-core` refuses `clone` thread flags with `ENOSYS` (logging a one-time note to `stderr`), and musl maps the error to `WouldBlock`, which libtest catches to run tests sequentially inline on the main thread (and `--test-threads=1` continues to work). `sched_getaffinity`, `sched_setaffinity`, and `getcpu` report an honest 1-CPU machine. A three-test crate passed this way on an Apple M3; the exact output is in the [Quick Start](docs/QUICKSTART.md#cargo-test-for-aarch64-linux-musl-through-ounce-core). Programs that require second threads fail with `WouldBlock` (`clone` threads and `futex` are not implemented; see [limits](docs/LIMITS.md)).

**3. Help out.** Have an ARM64 Mac or Linux machine, plus devkitPro/devkitA64? Follow the [independent rerun instructions](docs/REPRODUCE.md): the differential test needs `--nro` (or valid `--nro-dir` inputs) to actually compare guest engines. **Without guest binaries it can report skipped engine checks**, not a reproduction. Share matching, failing and blocked results in [issue #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248). No ARM64 machine? Try a [browser-only task](docs/community/TRY_THIS_WEEK.md) or a [good first issue](https://github.com/hffgzktbcc-blip/ounce-core/contribute). Read [CONTRIBUTING.md](CONTRIBUTING.md) before contributing.

## More

- [Quick start](docs/QUICKSTART.md): build, hello fixture, `--report-json`
- [Details](docs/DETAILS.md): subsystem status, how the proof works, test harness, full commands, roadmap, repository layout
- [Reproduce the results](docs/REPRODUCE.md), the [Big Tech library runs](docs/verify/bigtech-2026-11-23.md) and [what they do not show](docs/LIMITS.md)
- [Research paper](site/research.html) and [vision](docs/VISION.md)
- [Mac app](docs/MAC_APP.md) and [interactive lab](docs/INTERACTIVE_LAB.md)
- [Sources, provenance and legal notice](docs/DETAILS.md#responsible-research-and-legal)

Ounce-Core is not affiliated with Nintendo or NVIDIA. It runs open-source, homebrew and synthetic programs you provide; it is not designed to load commercial games, firmware or keys. Source and packaged downloads are published only through this repository and its [Releases](https://github.com/hffgzktbcc-blip/ounce-core/releases) page. Licensed under [MIT](LICENSE).
