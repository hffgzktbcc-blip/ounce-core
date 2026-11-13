# Install the retained-failure CLI

`ounce-diagnose 0.2.0` packages the demonstrated SDL2 failure-to-reproduction
workflow and the recorded audio-padding case from libnx #685. It is an experimental tool for this retained evidence format, not a
general executable debugger. Offline inspection needs only Python 3.9 or newer;
replay additionally needs macOS/Linux, Git, Cargo, the runtime's native build
dependencies and an authorized exact-hash NRO.

To diagnose your own `.nro` or `.elf` program rather than the retained SDL2/audio dossiers:

- `cargo run --release -- run-nro <program.nro>` (or `--cpu=tier3|tier4|native`) runs the program and prints the exact stop reason (`Unimplemented instruction`, `Stopping on unhandled SVC`, `Hit svcBreak` with `Break PC` and `Break result`, `[VM Error]` with faulting address and `Final PC`, or `[Loader Error]`).
- `cargo run --release -- ui` opens the local Verification Workbench (`docs/INTERACTIVE_LAB.md`), which runs your `.nro` on Tier-3, Tier-4, and optionally this computer's CPU (`native`), compares outputs, and saves `result.json` under `~/Library/Application Support/Ounce Core/Checks/<id>/` (openable in the Mac app's **File > Open Saved Results…**).
- `cargo run --release -- diverge <program.nro>` (`docs/DIVERGE.md`) replays sampled register-only instructions on this computer's CPU to isolate the first instruction that disagrees.
- `cargo run --release -- run-elf <program.elf>` runs a static AArch64 Linux ELF binary.
- When a run exposes a missing instruction, missing SVC, or engine divergence in Ounce Core, file an issue with `.github/ISSUE_TEMPLATE/emulation_bug.yml` including the `run-nro` command, host OS, `git rev-parse HEAD`, and the printed stop line.

From a source checkout, build the single-file distribution and its checksum:

```sh
python3 -B scripts/build_diagnosis_cli.py --output target/ounce-diagnose.pyz
```

Copy `ounce-diagnose.pyz` and `ounce-diagnose.pyz.sha256` to the destination machine.
Check the checksum there with `shasum -a 256 -c ounce-diagnose.pyz.sha256` (or
`sha256sum -c` on Linux). The checksum detects accidental changes; it is not a
signed publisher identity. No pip installation or third-party Python package is
needed. The archive includes the MIT license and the reviewed observation notices.
It does not include runtime binaries, NROs or upstream linked libraries.

Run from any directory, without the checkout:

```sh
python3 /path/to/ounce-diagnose.pyz --version
python3 /path/to/ounce-diagnose.pyz example --output ./sdl2-dossier
```

The second command validates the retained observations and produces a portable
dossier with `diagnosis.json`, `comparison.json`, original evidence and `REPLAY.md`.
It reports the first recorded frame divergence and exact inputs. It does not
execute the guest or independently re-verify absent binary artifacts. Output must
be a new directory; existing files are preserved. For another copy of the same
SDL2 retained-example format, use `bundle --evidence DIRECTORY --output NEW_DIRECTORY`.
This command is deliberately limited to the demonstrated SDL2 layout and semantic
checks. It is not a general report-pair importer. The built-in example's included
files have an observation-only distribution review; review your own evidence
before sharing a custom dossier. There is no automatic credential scrubber.

## Inspect the external audio-channel case

```sh
python3 /path/to/ounce-diagnose.pyz audio-case --output ./audio-dossier
```

This offline command explains the observed manager boundary from R2: both engines
rejected seven `0x00010000` channel words with `0xF601` before the repair, and
returned stereo for all seven after it. The low channel halfword is zero
(default stereo); the high halfword is padding. `diagnosis.json` reports the
first rejected manager open (index 1, after the startup open), raw and split
parameter values, per-engine counts, and whether the recorded after-replies
match the scoped expectation.

The output preserves the raw before/after reports, their manifest, runtime text
and historical corpus evidence. `REPLAY.md` names the exact binary/source hashes,
revisions, build image, input checkpoint and Cargo command. It contains no NRO,
upstream source, linked library or frame pixels. The reviewed default bundle has
no private checkout paths. The command checks digests and derives status from the
replies; changing only a pass flag cannot turn failed replies into success.

For a local copy of this specific dossier format, use `audio-case --evidence DIR
--output NEW_DIR`. The supported source/binary, input, engine set and expected
protocol are fixed. Missing/corrupt files, changed expectations, duplicate engines
and contradictory reply claims fail with exit 2. A consistent after-report that
still fails produces a dossier and exits 1. File hashes are relative to the
supplied manifest, not an authenticated attestation. Preserve the repository
revision and review custom evidence before sharing; there is no credential
scrubber. This command neither executes a guest nor independently re-verifies
absent NRO bytes, validates new runtime code, or grants repair qualification.
Six-channel playback and upstream libnx session replacement remain unestablished.

The `replay` command below executes only the SDL2 recipe. For audio, explicitly
follow the separate generated Cargo recipe with your authorized local binary and
an outer process deadline. [The audio case record](verify/AUDIO_CHANNEL_CASE.md)
contains the original guest execution acceptance.

## Explicit runtime replay

First run `example` and follow its `REPLAY.md` to obtain separate clean checkouts
at the recorded revisions and an authorized binary with the exact recorded hash.
The archive never downloads an executable or checks out revisions for you.

```sh
python3 /path/to/ounce-diagnose.pyz replay \
  --side failure \
  --checkout /path/to/clean-failing-revision \
  --corpus-root /path/to/exact-builds \
  --output /path/to/new-local-replay \
  --timeout 600
```

Use `--side reference` with the separate passing checkout to check the other side.
The CLI hashes the NRO and checks the clean Git revision before starting Cargo.
Each replay output gets its own `build/` target directory, preventing cross-revision
build reuse. Inherited `OUNCE_*` settings are removed; the registered SDL2 input,
180-second per-engine limit and fixed test command are set explicitly. The outer
`--timeout` includes compilation and both engines, and must be positive and finite.
No command from an input report is executed as shell text.

`result.json` records the Cargo exit, match status and hashes of the raw
`observation.json` and `runtime.log`. Stop, PC, instruction count, X0, frames and
execution limits must match the historical observations for both engines. A
compiler failure or missing observation cannot count as a match. This is exact
historical replay, not qualification of today's runtime. A match of the failing
revision deliberately includes Cargo exit 101 and zero frames. Logs on timeout or
Ctrl-C/SIGTERM are retained; the spawned process group is killed and reaped.
Runtime build logs can contain local checkout paths or tool output. They are local
diagnostics and are not automatically included in the reviewed shareable dossier;
review them before distribution.

| CLI exit | Meaning |
| --- | --- |
| 0 | Offline dossier created, or explicit replay matched its selected historical observation |
| 1 | Replay did not match, or a consistent audio after-report still fails the selected scenario |
| 2 | Invalid arguments, missing/corrupt inputs, wrong revision/hash, occupied output or launch prerequisite error |
| 124 | Replay exceeded the outer timeout; inspect local logs |
| 130 | Interrupted; available replay logs retained |

## Tested scope

The [0.2.0 acceptance record](verify/diagnosis-cli-0.2.0/acceptance.json) retains a
2026-11-08 macOS arm64/Python 3.9.6 smoke run: the 63,310-byte copied archive
ran audio diagnosis outside the checkout, with no external tools on PATH, in
0.1462 seconds after a 0.0004-second local file copy. These are single local
measurements, excluding download and Python installation. The new input/error
tests and prior offline/replay lifecycle tests pass. Windows is still unqualified.

The earlier 0.1.0 SDL2 baseline remains historical evidence. On 2026-11-07, the copied 42,187-byte archive was run with isolated Python 3.9.6
on macOS 26.6.2 arm64 outside the source checkout. Local file-copy installation
measured 0.0011 seconds; the first offline diagnosis measured 0.179 seconds.
These are one local observation, excluding source download, Python installation
and build time. They are not cross-machine performance promises. No network,
Rust or Docker was used for that offline invocation.

The Python tests exercise installed-archive diagnosis, missing/corrupt evidence,
existing-output preservation, wrong-binary rejection and real subprocess trees
on timeout, Ctrl-C and SIGTERM. They do not establish Windows replay support.
Replay is limited to POSIX hosts; other offline hosts remain unqualified until
separately tested. The [executed historical replay](FAILURE_TO_REPRODUCTION.md)
records the exact runtime baseline. Outside developer feedback and the community
release remain separate plan tasks.
