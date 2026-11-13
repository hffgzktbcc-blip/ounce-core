# Native Mac app (local preview)

A small AppKit front end (`tools/macos-launcher/`) around the command-line runtime. The runtime stays a
separate child process (`Ounce Runtime.app/Contents/MacOS/ounce-core`); the app never links the core, so
`#![forbid(unsafe_code)]` is untouched. The only `unsafe` in a build with the app's features is in the existing
`ounce-nce` and `ounce-recompiler` crates (`scripts/check_unsafe_boundary.sh`).

Build and install: `scripts/build_macos_app.sh "$HOME/Applications/Ounce Core.app"`. The script builds
`--features "gpu-present nce native-jit"`, so `tier4` and `native` work on Apple Silicon and degrade to `tier3`
(with a message the app recognises) elsewhere. The bundle is ad-hoc signed for local use. It is not notarized.
No prebuilt macOS binary is offered while the Apple-SDK-derived `objc2` redistribution question in
[PROVENANCE.md](PROVENANCE.md) (Specialist items) is open.

### Local preview archive

`python3 scripts/package_macos_app.py --app "dist/Ounce Core.app" --out dist/macos-preview` (after the build above)
writes a `.tar.gz` and a `.zip` of the bundle, `MANIFEST.json` (version, git revision, whether the tree was dirty,
cargo features, rustc and swiftc versions, per-file SHA-256) and `SHA256SUMS`. It refuses an incomplete bundle (a
missing license notice, helper or reference file) and an existing output folder. CI runs it on macOS and keeps the
result as the 14-day `macos-app-preview` artifact.

This is a developer preview, not a release. It uploads nothing, and the manifest says so: the bundle is ad-hoc
signed and not notarized (Gatekeeper warns on first open), and it is not cleared for GitHub release assets while
the specialist review in [PROVENANCE.md](PROVENANCE.md) (Specialist items) is open. Share it only with people you
choose to, and tell them it is unsigned by an Apple developer identity.

## One app, one door

Ounce Core opens one window: the workbench. There is no second launcher window. Everything you do to check a program
happens in that window; the menus only start it or open its results.

On launch the app starts `ounce-core ui --no-open --port 0 --parent-pipe` from the bundled runtime and shows the page in a
`WKWebView` (`tools/macos-launcher/Workbench.swift`; the pure logic is in `WorkbenchLogic.swift` and is covered by
`tests/macos/workbench-logic/main.swift`). The window holds no verification logic: see
[INTERACTIVE_LAB.md](INTERACTIVE_LAB.md) for what the page does and does not show.

| Menu | Items |
|---|---|
| File | Check a Program… (⌘O), Open Recent, Open Saved Results… (⇧⌘O), Close Window |
| View | Actual Size, Zoom In, Zoom Out, Advanced ▸, Enter Full Screen |
| View ▸ Advanced | Run in a Window…, Graphics Demo, Run Settings…, SD Card Folder…, Show Last Session Log, Stop Running Program |
| Help | Report a Problem…, Documentation |

There is no Edit menu. ⌘X, ⌘C, ⌘V, ⌘A and ⌘Z are routed to whatever has focus (the page, a text field in a dialog) by
`OunceApplication` in `main.swift`, so copy and paste work without a menu that would only repeat them. Every dialog and
file panel is a sheet on the window rather than `runModal()`, so an Apple Event quit is never blocked by one.

- **Ways to give it a program:** File > Check a Program, File > Open Recent, drop a `.nro` or a Linux ARM64 program anywhere on the window,
  drop one on the Dock icon, or open one from Finder. All of them send the program to the workbench, emulator-only
  (Tier-3 and Tier-4). Comparing with this Mac's CPU is a checkbox in the page, and it asks for confirmation first.
  A Linux ARM64 program (a static AArch64 ELF: `.elf` or no extension, recognised by content) is checked on Tier-3 and
  Tier-4 only: `run-elf` has no native engine, so the app never offers or confirms a Mac run for it. The Open dialog
  lists `.nro`, `.elf` and executables; Info.plist registers both kinds (`org.ounce-core.homebrew`,
  `org.ounce-core.linux-arm64-program` and `public.unix-executable`, rank Alternate). A file that is neither (an x86-64 ELF, a
  32-bit ELF, anything else) gets a dialog that says what it is and what to do.
- **Results:** the workbench saves each check in `~/Library/Application Support/Ounce Core/Checks`. File > Open Saved
  Results… reads one of those `result.json` files (or an Ounce-Verify report) in a read-only window
  (`EvidenceReport.swift`). It runs nothing.
- **Report a Problem:** Help > Report a Problem… asks the page for the same prefilled report as its *Report This Problem*
  button (`OunceWorkbench.reportRequest()`: engine, program name and SHA-256, the result's headline) for the check on
  screen, and opens it in the browser. With no finished check it opens the plain form with the app and macOS versions.
  The home folder is replaced by `~`. Nothing is sent until you submit the form. A failed Advanced run's dialog also
  offers it and copies the full report text.
- **Save Report…:** on a result, writes a plain-text report (verdict, what to do next, per-engine outcomes, program name
  and hash) through a save panel. It never contains the program.
- **Closing the window quits the app.** The server is a child process on 127.0.0.1 with a per-session key. It exits when
  the app does: the app holds a pipe to it, so even a crash or a force-quit doesn't leave it running.
- The window loads only the local workbench; any other link opens in your browser. The page hooks are documented in
  [WORKBENCH_UI.md](WORKBENCH_UI.md). The app uses these:
  - `--verify-dir <bundle>/Contents/Resources/Verify` is passed to the server when that folder exists, so the page's
    "Compare with the recorded reference" option works.
  - When the app hands over a program itself (Finder, Dock, drag-drop, File menu, Open Recent), it appends
    `OunceWorkbench.queryString()` (the engines, step limit and checkboxes currently shown in the page) to
    `POST /api/checks`, then calls `pick(id)`. If the page does not answer, it falls back to `native=0`. If the settings ask
    for the Mac reference run, the app asks for the same confirmation the page would, once per session.
  - The page's *Report This Problem* posts `{action: "report", url, text}` to the `ounceWorkbench` message handler. The app
    opens `url` in the default browser only if it is the project's issue form (anything else is ignored), with the home
    folder replaced by `~`. It does not touch the clipboard; *Copy details* in the page does that.
  - After the app uploads a program it calls `OunceWorkbench.stage('<id>')` as well as `pick`, so the page fetches the
    program back from the server (`GET /api/checks/ID/program`) and "Check again…" works for it.
  - *Save Report…* posts `{action: "save", name, text}`; the app shows a save panel (plain file name only).

### Advanced

The old launcher window is retired. Its functions that the workbench does not cover remain as items under View > Advanced with no
window of their own (`Advanced.swift`). The outcome of each is shown in a dialog that says what happened and what to try.

| Item | Behaviour |
|---|---|
| Run in a Window… | Runs a `.nro` in the runtime's own graphics window (`run-nro --gui`) on the engine chosen under Run Settings. A run that ends normally shows nothing; a failed one shows the diagnosis with Report a Problem… and Show Log. |
| Graphics Demo | Runs the built-in graphics demo on the interpreter. |
| Run Settings… | Engine (`--cpu=tier3|tier4|native`, always passed explicitly; `OUNCE_CPU` is stripped from the child) and instruction budget (`OUNCE_NRO_BUDGET`; blank clears it). `native` ignores the budget, has a time limit (`OUNCE_NRO_TIMEOUT_SECS`) and runs headless. These do not change the workbench. |
| SD Card Folder… | The folder these runs see as their SD card (default `~/Library/Application Support/Ounce Core/SD Card`). |
| Show Last Session Log | Reveals the newest log in `~/Library/Application Support/Ounce Core/Logs` (8 MiB cap per log). |
| Stop Running Program | Ends the run. Only enabled while one is running; the Dock icon shows "Running". |

Screenshots of a click-through of this build are in [mac-app-one-door/](mac-app-one-door/).

Swift logic tests: `swiftc tools/macos-launcher/WorkbenchLogic.swift tests/macos/workbench-logic/main.swift -o /tmp/wb-tests && /tmp/wb-tests`.

## How a run is explained

| Area | Behaviour |
|---|---|
| Diagnosis | `RunLogic.swift` parses the runtime's printed stop messages (see below) into a headline, an explanation and a next step, quoting the log lines it relied on. The first failure line wins. A silent engine fallback is reported as such, never as a pass. This is used by the Advanced runs; the workbench has its own verdicts. |
| Saved results | `EvidenceReport.swift` decodes saved `ounce-verify` reports (`v0`, `diagnosis-v1`, `comparison-v1`, `repair-v1`, `run-v1`, `hbx-first-frame-v1`), `ounce-diagnose` audio (`audio-padding-v1`, `audout-padding-v1`) and replay (`replay-v1`) reports, and Workbench `result.json` files (`workbench-check-v1`). Selecting a dossier `manifest.json` explains which report file in that folder to open instead. |

Recognised runtime messages: `Unimplemented instruction 0x… at PC: 0x…`, `Execution budget reached (N instructions)`,
`unhandled SVC`, `Hit svcBreak` (and `Break result`), `CPU Halted`, `CPU fault (native signal …)`, `[VM Error]` with
`Final PC`, `[Loader Error]`, `Failed to read file`, `Time limit reached`, and the engine-fallback notices. If the runtime changes these strings
the diagnosis degrades to "could not classify" and shows the last log lines; it never invents a cause.

## Tests

The logic has no AppKit dependency and is covered by `tests/macos/run-logic/main.swift` (budget parsing, arguments and
environment, every diagnosis kind and the issue report):

```sh
swiftc tools/macos-launcher/RunLogic.swift tests/macos/run-logic/main.swift -o /tmp/run-logic-tests && /tmp/run-logic-tests
```

CI compiles and runs it on macOS beside the saved-report tests.

## GUI verification status

Clicked through on a Mac (Apple Silicon, macOS 26) with Orca computer use against a build of this branch, screenshots in
[mac-app-one-door/](mac-app-one-door/): first launch; a program opened from Finder (`open -a`) with its progress state,
then *Results match*; *Check again and compare with this Mac* after that app-side upload (the confirmation sheet, then
*Results match* on this Mac); Save Report… (the file was written); Help > Report a Problem (same URL as the page's
button, for a match and for a differ); File > Open Recent; File > Open Saved Results; Advanced > Run in a Window; the
menu bar (Ounce Core, File, View, Help); ⌘A and ⌘C without an Edit menu; and an Apple Event quit while a file sheet was
open. **Results differ** was seen only through a test fixture (`OUNCE_LAB_TEST_FIXTURES`, a crafted result named
`TEST-FIXTURE-…`; nothing was run), because no program in the repo diverges on demand.

Not seen in that pass: Linux ARM64 programs, the Dock-icon drop, a real *differ*. Those are covered in
[mac-app-real-programs/](mac-app-real-programs/README.md). Still not seen: the app on an Intel Mac (none was available);
Graphics Demo in this pass; a failed *Run in a Window* dialog in this pass.

## Known limits

- The page's "Compare with the recorded reference" needs a prebuilt harness NRO (devkitA64) and matches by file name. The app does not build harnesses or fetch upstream sources. It is a byte comparison with the committed reference, not a proof.
- Linux ARM64 programs are accepted (static AArch64 ELF only; dynamic, PIE, 32-bit and x86-64 files are refused or stop with a diagnosis). They are never run on this Mac's CPU, so *Results match* for one means Tier-3 and Tier-4 agreed (and, for a known test program, matched the recorded output), not that it matched this Mac.
- The diagnosis reads the last 256 KiB of the log; a failure earlier in a very long run can be missed.
