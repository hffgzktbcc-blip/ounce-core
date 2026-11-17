# Quick start

Run one homebrew program and, if you want a machine-readable result, write an
`ounce.run.v1` report. This is the short path. Limits are in
[LIMITS.md](LIMITS.md). The family roadmap is in
[roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md](roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md).

You need Rust 1.85 or newer (edition 2024). On Debian or Ubuntu, install the
audio and device headers before building:

```sh
sudo apt-get install -y libasound2-dev libudev-dev
```

```sh
git clone https://github.com/hffgzktbcc-blip/ounce-core.git
cd ounce-core
cargo build --release --locked
cargo run --release -- run-nro tests/fixtures/nro/hello/hello.nro fixture --present=headless
```

`fixture` is a guest argument. With it, a clean run prints `result=0x00000EEE` and exits 0. Without a guest argument the same file still exits 0 and prints `result=0x0BAD0008`. The default engine is
Tier-3. `--cpu=interpreter` is one instruction at a time.
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

`run.json` is `ounce.run.v1`. Guest text stays on stdout. The same path can be
given as `OUNCE_REPORT_JSON`. `run` and `run-nro` write the file.
`run-elf` does not, yet.

`--audio=null` (or `OUNCE_AUDIO=null`) does not open a host audio device.
`--present=headless` already skips that device. On `run-elf`,
`--deterministic` uses a fixed guest clock. On `run` and `run-nro` the flag is
accepted and does not change the kernel clock.

Field list, stop kinds, and the limits of this report:
[OUNCE_RUN_V1_SCHEMA.md](OUNCE_RUN_V1_SCHEMA.md).

## A static Linux ELF

```sh
cargo run --release -- run-elf path/to/program.elf --cpu=tier3 --deterministic
```

That runner is for static musl AArch64 `ET_EXEC` programs. See
[verify/linux-elf/README.md](verify/linux-elf/README.md).

## Where the other tools live

They are separate repositories. ounce-core does not build or download them.

| Tool | What to run | Where |
| --- | --- | --- |
| ounce-hb `doctor` | `ounce-hb doctor` or `ounce-hb doctor --json` | [ounce-hb](https://github.com/hffgzktbcc-blip/ounce-hb) `main` (not in the v0.0.2 archive) |
| ounce-hb `repro` / `run --auto` | see that repo's quick start | same |
| `setup-ounce` | GitHub Action `hffgzktbcc-blip/ounce-hb/.github/actions/setup-ounce` | same repo; its `doctor` step needs a build that contains `doctor` |

`doctor` checks that the engine binaries are on `PATH` (or `OUNCE_CORE_BIN` /
`OUNCE_RUNTIME_BIN`). It does not install a compiler. `setup-ounce` installs
release archives. Linux arm64 archive selection and ounce-runtime checksum
verification were added in ounce-hb pull request 40, which is merged; check the
ounce-hb release you install includes it.
