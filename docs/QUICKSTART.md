# Quick start

Run one homebrew program and, if you want a machine-readable result, write an
`ounce.run.v1` report. This is the short path. Limits are in
[LIMITS.md](LIMITS.md).

You need Rust 1.89 or newer (edition 2024). Default headless builds require zero C system libraries; pass `--features host-io` only if you want desktop audio output and physical controller input. Only that opt-in feature needs `libasound2-dev libudev-dev` on Debian or Ubuntu.

```sh
git clone https://github.com/hffgzktbcc-blip/ounce-core.git
cd ounce-core
cargo build --release --locked
cargo run --release -- run-nro tests/fixtures/nro/hello/hello.nro fixture --present=headless
```

`fixture` is a guest argument. With it, a clean run prints `result=0x00000EEE` and exits 0. Without a guest argument the same file still exits 0 and prints `result=0x0BAD0008`. The default engine is
Tier-3. `--cpu=interpreter` is one instruction at a time.
Guest arguments that start with `-` go after `--`, for example `... hello.nro -- --verbose`; before `--`,
an unknown option is refused with exit status 2 rather than dropped (`ounce-core run-nro --help`).
`--cpu=tier4` needs `cargo build --release --features native-jit`.
`--cpu=native` runs a homebrew NRO on an AArch64 host and needs
`--features nce`; without that feature the run stays on the interpreter and
says so.

## A JSON report

`--report-json` ships in v0.9.0 and later (added 14 Nov 2026, `#439`).

```sh
cargo run --release -- run-nro tests/fixtures/nro/hello/hello.nro fixture \
  --present=headless --report-json=run.json
```

`run.json` is `ounce.run.v1` (pass `--report-json=-` to emit pure JSON to stdout while routing diagnostics to stderr). Guest text stays on stdout and is flushed as the
guest writes it (with `--report-json=-` the guest's stdout and stderr both go to stderr, so stdout is only the JSON), so a hung program's output shows before a timeout stops it. The same path can be
given as `OUNCE_REPORT_JSON`. `run`, `run-nro` and `run-elf` write the file.

`--audio=null` (or `OUNCE_AUDIO=null`) does not open a host audio device.
`--present=headless` already skips that device. `--deterministic` fixes the
guest clock for supported `run`, `run-nro` and `run-elf` workloads rather than
reading host wall-clock time. It makes tested fixture runs repeatable, but it
does **not** guarantee that every program follows the same instruction path on
every host. On `run` and `run-nro`, supported 3D frames use the software
rasterizer in deterministic mode so they can be hashed. Outside that mode, a
`gpu-present` window may not produce a `framebuffer_hash`.

Field list, stop kinds, and the limits of this report:
[OUNCE_RUN_V1_SCHEMA.md](OUNCE_RUN_V1_SCHEMA.md).

## See a real wrong-answer bug

The [Lua floating-point case study](community/LUA_FP_BUG_WALKTHROUGH.md) explains how a
program returned an incorrect answer even though Ounce-Core's engines initially
agreed. It shows the dated evidence and a correctly flagged optional rerun.

## A static Linux ELF

```sh
cargo run --release -- run-elf path/to/program.elf --cpu=tier3 --deterministic
```

That runner is for static musl AArch64 `ET_EXEC` programs. See
[verify/linux-elf/README.md](verify/linux-elf/README.md). Put the ELF after `--` to pass it
guest arguments without host option parsing: `ounce-core run-elf -- program.elf arg1 arg2`.
Guest file descriptor 0 is at end-of-file unless you pass `--stdin` (read the host's stdin) or
`--stdin=<path>` (read a file); at most 64 MiB is read. `getrandom` returns a fixed,
reproducible byte stream, not host entropy.

## `cargo test` for aarch64 Linux (musl) through Ounce-Core

Cargo can run test binaries built for another target through a *runner*. Install the target
(`rustup target add aarch64-unknown-linux-musl`), put `ounce-core` on your `PATH`, and create
`.cargo/config.toml` in your project:

```toml
[target.aarch64-unknown-linux-musl]
runner = "ounce-core run-elf --"
```

On macOS the default flags fail to link (Apple's linker does not understand the GNU options
rustc passes), and `ounce-core` rejects static-PIE (`ET_DYN`). Link with `rust-lld` and a
static, non-PIE layout:

```sh
RUSTFLAGS="-C linker=rust-lld -C target-feature=+crt-static -C relocation-model=static" \
  cargo test --target aarch64-unknown-linux-musl -- --test-threads=1
```

`--test-threads=1` is needed because the runner executes single-threaded guests only (no
`futex`, no `clone`). The result below is from a three-test library crate on an Apple M3 Mac
(Rust 1.98, `ounce-core` built from `main` at `14b3c4d`); the tests ran on the Tier-3 engine:

```text
running 3 tests
test tests::adds ... ok
test tests::formats_floats ... ok
test tests::sums_a_vec ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

[run-elf] engine=Tier-3 instructions=164740 syscalls=74
```

This shows that small test binaries run; it does not show that every crate's tests do. Tests
that spawn threads or need AES (`FEAT_AES` is not implemented) or other unsupported system
calls stop with a diagnosis ([limits](LIMITS.md)).

## Where the other tools live

`ounce-hb`, `ounce-runtime` and `Ounce-Spark` are archived on GitHub (checked 2026-11-24).
This repository does not build, download or recommend them; the Quick Start above needs only
`ounce-core`.
