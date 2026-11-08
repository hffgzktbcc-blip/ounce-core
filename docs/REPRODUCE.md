# Reproduce the differential results in 10 minutes

This is the path for someone outside the project, on an ARM64 Mac or an ARM64 Linux box, to
rerun the differential checks and see whether they match. Read [LIMITS.md](LIMITS.md) first so you know what a
match does and does not mean.

> **Dependency.** This page documents `scripts/verify_differential.py` and
> `docs/verify/differential.json`. They are being landed separately; if they are not on `main` yet,
> the steps below will not run until they are. Everything else here already exists.

## What you are reproducing

For each harness listed in `docs/verify/differential.json` (floating-point and integer-vector
instruction tests, the QC-flag tests, and LZ4, xxHash, SQLite, Lua, plus the ones marked in review such as zlib, cJSON, TinyExpr, Monocypher), the script:

1. fetches the pinned upstream source (a git commit, or a tarball whose SHA-256 is checked);
2. compiles the shared C cases for your host with `cc -O1`. This is the **reference**;
3. with `--nro`, compiles the same cases for AArch64 with devkitA64 into an NRO;
4. runs that NRO on each Ounce-Core engine (`tier3`, `tier4`, `native`);
5. compares each engine's `results.txt` with the host output **byte for byte**;
6. compares the committed `expected-host.txt` with your fresh host output. Harnesses marked
   `arm64_reference` only produce a meaningful reference on an ARM64 host, so on x86 that check is
   reported as skipped.

## Prerequisites

- An **ARM64** host (Apple Silicon macOS, or aarch64 Linux). The `native` engine needs one, and the
  host reference for the instruction tests must run on real ARM64 hardware.
- Rust (stable, via [rustup](https://rustup.rs)), `git`, a C compiler (`cc`), `make`, `python3`.
- devkitA64 from devkitPro, installed under `/opt/devkitpro` (the script looks there, or at `$DEVKITPRO`):
  install [devkitPro pacman](https://devkitpro.org/wiki/Getting_Started) for your OS, then

  ```sh
  sudo dkp-pacman -S switch-dev      # `sudo pacman -S switch-dev` on devkitPro's Linux packages
  export DEVKITPRO=/opt/devkitpro
  ```
- Network access (upstream sources are downloaded and hash-checked).

## Steps

```sh
git clone https://github.com/hffgzktbcc-blip/ounce-core && cd ounce-core
git rev-parse HEAD                      # write this down; you will report it

cargo build --release --features "nce native-jit"

python3 scripts/verify_differential.py --nro --json differential-result.json
```

The build is the slow part. Useful options: `--only NAME ...` to run a subset (try `--only fp-instructions qc-flag` first),
`--engines tier3 tier4` to skip native, `--nro-dir DIR` to use prebuilt NROs named `<harness>.nro`
instead of building them, `--ounce PATH` for a different binary (default `target/release/ounce-core`),
`--work DIR` for scratch space (default `target/differential`).

## Reading the result

Per harness the script prints one line, for example:

```
qc-flag: committed-vs-fresh-host=identical, tier3=identical, tier4=identical, native=identical
```

and finishes with either `all compared outputs identical` (exit status 0) or `FAILED` (exit status 1).
Each check is `identical`, `DIFFERENT`, or `skipped (...)`.

| Check | Meaning if `DIFFERENT` |
|---|---|
| `committed-vs-fresh-host` | Your host produces different reference output from the committed `expected-host.txt`. Either your CPU/compiler differs from the one used when the file was recorded, or the test is not deterministic on your machine. The engine comparisons are still against your fresh host output. |
| `tier3`, `tier4` | An emulator engine disagrees with your hardware. This is the finding the project exists to produce. |
| `native` | Native runs on your CPU, so a difference points at the harness, toolchain or loader, not the emulator. |

`differential-result.json` records the host architecture, an overall `failed` flag, per-check results and
the SHA-256 of each NRO built. The NRO hashes depend on your devkitA64 version; a different hash is
expected if your toolchain differs and is worth recording. A `skipped (no NRO)` or `skipped (needs ARM64 host)` is
not a pass.

## Report your result

Post on the tracking issue ([Independent reproduction wanted](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+%22Independent+reproduction+wanted%22)) using this template.

```
Host: (CPU, OS and version, `uname -m`)
Commit: (git rev-parse HEAD)
devkitA64 / Rust versions: (`aarch64-none-elf-gcc --version`, `rustc --version`)
Command: python3 scripts/verify_differential.py --nro --json differential-result.json
Result: all identical / FAILED
Mismatches: (harness, engine, and the first differing line, if any)
differential-result.json: (attach)
```

- **Reproduction confirmed:** every compared check is `identical` on your host and none was skipped. Say so
  and attach the JSON. A confirmed outside rerun is the evidence required to move a claim to
  *independently reproduced* (see [LIMITS.md](LIMITS.md#maturity-labels-and-where-each-claim-sits)); a maintainer will
  update the label, not you.
- **Mismatch:** open an issue with the template above plus the output of
  `diff <(cat target/differential/NAME/host-output.txt) target/differential/NAME/sd_ENGINE/results.txt | head`.
  A mismatch is a useful result, not a failed run, and a `committed-vs-fresh-host` mismatch alone does not
  indicate an emulator bug.
- **Could not run:** report the failing step. A missing prerequisite is documentation we should fix.
