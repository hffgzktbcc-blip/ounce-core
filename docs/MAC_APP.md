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

### Local preview archive

`python3 scripts/package_macos_app.py --app "dist/Ounce Core.app" --out dist/macos-preview` (after the build above)
writes a `.tar.gz` and a `.zip` of the bundle, `MANIFEST.json` (version, git revision, whether the tree was dirty,
cargo features, rustc and swiftc versions, per-file SHA-256) and `SHA256SUMS`. It refuses an incomplete bundle (a
missing license notice, helper or reference file) and an existing output folder. CI runs it on macOS and keeps the
result as the 14-day `macos-app-preview` artifact.

This is a developer preview, not a release. It uploads nothing, and the manifest says so: the bundle is ad-hoc
signed and not notarized (Gatekeeper warns on first open), and it is not cleared for GitHub release assets while
item 4 of [PROVENANCE_OWNER_DECISIONS.md](PROVENANCE_OWNER_DECISIONS.md) is open. Share it only with people you
choose to, and tell them it is unsigned by an Apple developer identity.

## The workbench window (front door)

On launch the app starts `ounce-core ui --no-open --port 0 --parent-pipe` from the bundled runtime and shows the page in a
`WKWebView` (`tools/macos-launcher/Workbench.swift`; the pure logic is in `WorkbenchLogic.swift` and is covered by
`tests/macos/workbench-logic/main.swift`). The window holds no verification logic: see
[INTERACTIVE_LAB.md](INTERACTIVE_LAB.md) for what the page does and does not show.

- The window loads only the local workbench; any other link opens in your browser.
- "Check a Program…" (⌘O) and opening a `.nro` from Finder send the program to the workbench, emulator-only
  (Tier-3 and Tier-4). Comparing with this Mac's CPU is a checkbox in the page, and it asks for confirmation first.
- The server is a child process on 127.0.0.1 with a per-session key. It exits when the app does: the app holds a pipe to
  it, so even a crash or a force-quit doesn't leave it running.
- Results are saved in `~/Library/Application Support/Ounce Core/Checks`.
- The earlier launcher (run a program in a window, graphics demo, "Verify against Host") is still available from the
  menu: "Run in a Window (Classic)…" and "Show Classic Launcher".

Swift logic tests: `swiftc tools/macos-launcher/WorkbenchLogic.swift tests/macos/workbench-logic/main.swift -o /tmp/wb-tests && /tmp/wb-tests`.

## What it does

| Area | Behaviour |
|---|---|
| First run | Explains the product; accepts a dropped or chosen `.nro`. A dropped `.elf` or ELF-headed file directs the user to `ounce-core run-elf <file>` in Terminal. |
| Engine and budget | `--cpu=tier3|tier4|native` is always passed explicitly; `OUNCE_CPU` is stripped from the child. A budget sets `OUNCE_NRO_BUDGET`; blank clears any inherited value. `native` ignores the budget (it has a time limit, `OUNCE_NRO_TIMEOUT_SECS`) and runs headless. |
| Diagnosis | `RunLogic.swift` parses the runtime's printed stop messages (see below) into a headline, an explanation and a next step, quoting the log lines it relied on. The first failure line wins. A silent engine fallback is reported as such, never as a pass. |
| Finished run | The diagnosis card shows `Program output:` (what the program printed, last 8 lines) above the runtime evidence, so the result is visible without opening session details. |
| Report This Problem… | Shown after a failed run. Copies a report (file name only, home folder replaced by `~`, engine, command, OS, evidence) and opens `.github/ISSUE_TEMPLATE/emulation_bug.yml` in the browser with fields prefilled. The app submits nothing. |
| View Check Results… | `EvidenceReport.swift` decodes saved `ounce-verify` reports (`v0`, `diagnosis-v1`, `comparison-v1`, `repair-v1`, `run-v1`, `hbx-first-frame-v1`), `ounce-diagnose` audio (`audio-padding-v1`, `audout-padding-v1`) and replay (`replay-v1`) reports, and Workbench `result.json` files (`workbench-check-v1`). Selecting a dossier `manifest.json` explains which report file in that folder to open instead. |
| Classic window | Not shown at launch. Every classic action (Run in a Window, Verify, View Results, any diagnosis or error) brings it forward first. |
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

## GUI verification status

Checked live on 9 November 2026 on an Apple Silicon Mac (macOS 26.6.2) with the app built from `main` by
`scripts/build_macos_app.sh`:

- The app launches, shows its first-run window (headline, drop-target card, Open Program / Verify against Host /
  Try the Demo buttons, engine pop-up, budget field, diagnosis panel, Show Session Details) and quits cleanly. The
  layout fits at the default window size.
- The bundled runtime (`Ounce Runtime.app`) runs `tests/fixtures/nro/hello/hello.nro` headless under both `tier3` and
  `tier4` and exits through `svcExitProcess` with the same instruction count (9,067).
- The pure logic tests (`tests/macos/run-logic/main.swift`) pass there.

Still not verified: a run started from the window (dropping or choosing an NRO), the diagnosis card for a failing
run, Verify against Host, `tier4` in the windowed (`--gui`) path, and layout at other window sizes. The window was
observed in one screenshot and not clicked through; these were not exercised.

## Known limits

- Verify against Host needs a prebuilt harness NRO (devkitA64). The app does not build harnesses or fetch upstream sources.
- Matching is by file name. A renamed or modified NRO that writes the expected file is compared as that harness.
- It reports a byte comparison with the committed reference, not a proof; each harness README states its own bounds.
- The diagnosis reads the last 256 KiB of the log; a failure earlier in a very long run can be missed.
