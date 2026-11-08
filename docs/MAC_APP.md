# Native Mac app (local preview)

A small AppKit front end (`tools/macos-launcher/`) around the command-line runtime. The runtime stays a
separate child process (`Ounce Runtime.app/Contents/MacOS/ounce-core`); the app never links the core, so
`#![forbid(unsafe_code)]` is untouched. The only `unsafe` in a build with the app's features is in the existing
`ounce-nce` and `ounce-recompiler` crates (`scripts/check_unsafe_boundary.sh`).

Build and install: `scripts/build_macos_app.sh "$HOME/Applications/Ounce Core.app"`. The script builds
`--features "gpu-present nce native-jit"`, so `tier4` and `native` work on Apple Silicon and degrade to `tier3`
(with a message the app recognises) elsewhere. The bundle is ad-hoc signed for local use. It is not notarized.
No prebuilt macOS binary is offered while the Apple-SDK-derived `objc2` redistribution question in
[PROVENANCE_OWNER_DECISIONS.md](PROVENANCE_OWNER_DECISIONS.md) is open.

## What it does

| Area | Behaviour |
|---|---|
| First run | Explains the product; accepts a dropped or chosen `.nro`. A dropped `.elf` gets an honest "not supported yet" (ELF loading is a separate change). |
| Engine and budget | `--cpu=tier3|tier4|native` is always passed explicitly; `OUNCE_CPU` is stripped from the child. A budget sets `OUNCE_NRO_BUDGET`; blank clears any inherited value. `native` ignores the budget (it has a time limit, `OUNCE_NRO_TIMEOUT_SECS`) and runs headless. |
| Diagnosis | `RunLogic.swift` parses the runtime's printed stop messages (see below) into a headline, an explanation and a next step, quoting the log lines it relied on. The first failure line wins. A silent engine fallback is reported as such, never as a pass. |
| Verify against Host | `HostVerify.swift` matches the NRO file name to a harness in `docs/verify/differential.json`, runs it headless with a 4e9 budget (as `verify_differential.py` does) in a temporary SD folder, and diffs the harness's result file against the bundled `expected-host.txt`. A harness whose reference is `arm64_reference` carries a caveat on non-ARM64 hosts. |

Recognised runtime messages: `Unimplemented instruction 0x… at PC: 0x…`, `Execution budget reached (N instructions)`,
`unhandled SVC`, `Hit svcBreak` (and `Break result`), `CPU Halted`, `CPU fault (native signal …)`, `[VM Error]` with
`Final PC`, `[Loader Error]`, `Failed to read file`, `Time limit reached`, and the engine-fallback notices. If the runtime changes these strings
the diagnosis degrades to "could not classify" and shows the last log lines; it never invents a cause.

## Tests

The logic has no AppKit dependency and is covered by `tests/macos/run-logic/main.swift` (budget parsing, arguments and
environment, every diagnosis kind, harness matching against the committed `differential.json`, the comparison and its
sample limit):

```sh
swiftc tools/macos-launcher/RunLogic.swift tools/macos-launcher/HostVerify.swift tests/macos/run-logic/main.swift -o /tmp/run-logic-tests && /tmp/run-logic-tests
```

CI compiles and runs it on macOS beside the saved-report tests.

## Not verified on a GUI

The windows, drop target, pop-up and text field, sheets and the diagnosis card were compiled and the logic tested, but
nobody has driven them in a live session. Also unverified: `tier4` in the windowed (`--gui`) path, and whether the
card layout fits at every window size. Verification used the real runtime headless against the fixture NROs in
`tests/fixtures/nro/` (exit codes and logs).

## Known limits

- Verify against Host needs a prebuilt harness NRO (devkitA64). The app does not build harnesses or fetch upstream sources.
- Matching is by file name. A renamed or modified NRO that writes the expected file is compared as that harness.
- It reports a byte comparison with the committed reference, not a proof; each harness README states its own bounds.
- The diagnosis reads the last 256 KiB of the log; a failure earlier in a very long run can be missed.
