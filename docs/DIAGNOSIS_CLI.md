# Install the retained-failure CLI

`ounce-diagnose 0.1.0` packages the demonstrated SDL2 failure-to-reproduction
workflow. It is an experimental tool for this retained evidence format, not a
general executable debugger. Offline inspection needs only Python 3.9 or newer;
replay additionally needs macOS/Linux, Git, Cargo, the runtime's native build
dependencies and an authorized exact-hash NRO.

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
| 1 | Replay finished but did not match (including build/test failure without the expected observations) |
| 2 | Invalid arguments, missing/corrupt inputs, wrong revision/hash, occupied output or launch prerequisite error |
| 124 | Replay exceeded the outer timeout; inspect local logs |
| 130 | Interrupted; available replay logs retained |

## Tested scope

On 2026-11-07, the copied 42,187-byte archive was run with isolated Python 3.9.6
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
