# UX flow findings: install, open, see a result, diagnose a failure, report it

Date: 2026-11-11. Base: `origin/main` at c47fa14. Host: macOS 26 on Apple Silicon.
Related open work: PR #389 (`hffgzktbcc-blip/diagnose-real-user`) fixes loader and fault message detail, ELF guidance
and saved-report decoding. This change does not touch those lines; the findings below are the ones #389 leaves open.

Method: built the release binary (`cargo build --release`) and the app (`scripts/build_macos_app.sh`), then walked
the CLI path as a first-time user. The app steps (L1, L2, L4) come from reading the launcher code and from the Swift
logic tests; they were not clicked through in a live window. Failing programs were synthesised as 4 KiB NROs whose first code word at `0x80` is
`0x00000000` (undecodable), `0xD4000FE1` (`svc #0x7f`, unhandled) or `0x14000000` (`b .`, never exits). The builder is in
"Repro inputs" at the end. Each repro runs from the repository root after `cargo build --release`.

Status key: **Fixed** in this change, **Open** not addressed here.

## Friction points

| # | Step | Friction | Status |
|---|---|---|---|
| C1 | Open (CLI) | A bare `ounce-core` prints a banner, a 55-line manual, then starts the 25-stage demo. | Fixed |
| C2 | Open (CLI) | An unknown command prints the whole manual and exits 0. Scripts see success. | Fixed |
| C3 | Open (CLI) | `run`, `run-nro`, `inspect` with no path exit 0 (`run`, `inspect`) or print the banner before the error. | Fixed |
| C4 | Open (CLI) | The manual tells installed users to type `cargo run --release --`, opens with an emoji, and lists the first-use commands in the middle. | Fixed |
| C5 | Diagnose (CLI) | A run that stops early prints a stop line and exits 1, but never says what to do next or where to report it. | Fixed |
| C6 | Open (CLI) | A missing file prints the banner, then `Failed to read file: No such file or directory (os error 2)` with no path and no advice. | Fixed |
| L1 | See result (app) | A finished run says "The program ran to completion" and hides what the program printed (for example `ounce-fixture result=0x0BAD0008`) inside the collapsed session log. | Fixed |
| L2 | Report (app) | A failed run shows a diagnosis, but reporting needs the user to find the issue template, copy the command, OS and stop line by hand. The diagnosis only names a file path in the repository. | Fixed |
| L3 | Install | `build_macos_app.sh` fails with a raw shell error when Rust or the Xcode tools are missing, and ends with `Built <path>` and no next action. | Fixed |
| L4 | Open, see result (app) | The classic launcher window is not shown at launch (only the workbench is). **Run in a Window (Classic)…** starts the run, but the status line, the diagnosis card and the error alerts belong to that hidden window, so a finished or failed run shows nothing on screen. | Fixed |
| L5 | Report (app) | Failures from Verify against Host and from the workbench have no report action. | Open |
| L6 | Install | There is no prebuilt app, and the app is not notarized. First open needs Control-click then Open. Policy: PROVENANCE_OWNER_DECISIONS.md. | Open (documented in the script's closing text) |

## Exact repro steps

### C1. Bare command runs a demo

```sh
target/release/ounce-core
```

Before: banner, the full manual, then the built-in 25-stage demo starts and runs for several seconds.
After: eight lines (three starting commands, a pointer to `help`), exit 0, nothing runs.

### C2. Unknown command exits 0

```sh
target/release/ounce-core frobnicate; echo "exit=$?"
```

Before: the whole manual, `exit=0`. After: `error: unknown command 'frobnicate'.` plus the pointer to `help`, `exit=2`.
A near miss is corrected: `ounce-core run-nr x` answers `Did you mean 'run-nro'?`.

### C3. Missing path

```sh
target/release/ounce-core run-nro;  echo "exit=$?"
target/release/ounce-core run;      echo "exit=$?"
target/release/ounce-core inspect;  echo "exit=$?"
```

Before: `Error: Please provide ...`; `run` and `inspect` exit 0, `run-nro` exits 1 after four banner lines.
After: `error: 'run-nro' needs the path of a homebrew .nro file.`, an example line, exit 2 for all three, no banner.

### C4. Manual wording and order

```sh
target/release/ounce-core help | head -12
```

Before: header `🚀 OUNCE-CORE ... COMMAND-LINE INTERFACE (CLI) & WORKFLOW GUIDE`, usage `cargo run --release -- <COMMAND>`,
first group "CORE EXECUTION COMMANDS" opening with `run`, with `ui` far below.
After: `ounce-core <COMMAND>` first (source-checkout form on the next line), then "RUN AND CHECK A PROGRAM (start here)"
opening with `ui`, `run-nro`, `run-elf`, `diverge`. Options for runs are grouped, and the last line is the report link.
No emoji.

### C5. A stopped run ends without a next action

```sh
python3 docs/verify/ux-flow/make_failing_nros.py "$TMPDIR/ux"      # see "Repro inputs"
OUNCE_NRO_BUDGET=1000 target/release/ounce-core run-nro "$TMPDIR/ux/halt.nro" --present=headless; echo "exit=$?"
```

Before: output ends at the `[VM Summary]` and `-> Final X0` lines, exit 1.
After: the output ends with a `Run stopped early.` paragraph naming the cause, one command to try, and the issue form
URL with what to include (command with the engine used, OS, version, stop line). The text depends on the stop:

| Input | Stop | Next step printed |
|---|---|---|
| `halt.nro` | undecodable instruction | `ounce-core diverge <file>`, report link |
| `svc.nro` | unhandled SVC | look up the call in `docs/svc-coverage.md`, report link |
| `loop.nro` with `OUNCE_NRO_BUDGET=1000` | budget | `OUNCE_NRO_BUDGET=500000000 ounce-core run-nro <file>`, no report link (not a bug) |
| a program calling `svcBreak` | break | read the lines above the break, `ounce-core ui`, no report link |

### C6. Missing file

```sh
target/release/ounce-core run-nro /nope/x.nro
```

Before: banner, `[Loader] Reading package/executable: /nope/x.nro`, `Failed to read file: No such file or directory (os error 2)`.
After: `[Loader Error] Failed to read file '/nope/x.nro': not found.` and a line saying to check the path, exit 1, no banner.
The phrase `Failed to read file` is unchanged because the app's diagnosis and `tests/cli_failures.rs` match on it.

### L1. A finished run hides its result

1. Build the app: `scripts/build_macos_app.sh "$HOME/Applications/Ounce Core.app"`.
2. Open the app, choose **Run in a Window (Classic)…**, then **Open Program…** and pick `tests/fixtures/nro/hello/hello.nro`.
3. Wait for "Session finished."

Before: the diagnosis card reads "The program ran to completion" with one runtime line; `ounce-fixture result=0x0BAD0008`
appears only after **Show Session Details**. After: the card has a "Program output:" block with that line.
Covered by `tests/macos/run-logic/main.swift` (`program output is extracted`).

### L2. Reporting a failed run

1. Open the app, **Run in a Window (Classic)…**, **Open Program…**, pick `halt.nro` from the C5 inputs.
2. Read the diagnosis: "The program used an ARM64 instruction Ounce Core does not implement yet."

Before: the next step names `.github/ISSUE_TEMPLATE/emulation_bug.yml`; the user must locate it in a checkout, then
retype the command, engine, OS and stop line. After: the card has **Report This Problem…**. One click copies a report
(file name only, home folder replaced by `~`, engine, command, OS, evidence) and opens the issue form in the browser with
the program, engine, observed and command fields filled in. Nothing is submitted by the app.

### L3. Install

```sh
PATH=/usr/bin:/bin scripts/build_macos_app.sh "$TMPDIR/App/Ounce Core.app"     # without cargo on PATH
scripts/build_macos_app.sh "$TMPDIR/App/Ounce Core.app"
```

Before: with Rust missing the script stops at `cargo: command not found` after creating a staging folder; a good build
ends with `Built <path>` only. After: a list of what is missing with the install command for each, before any work;
a good build ends with `Next: open "<path>"`, the first-open instruction for an unnotarized app, and the command-line
path inside the bundle.

### L4. Classic window stays hidden

1. Launch **Ounce Core.app**. Only the workbench window appears.
2. Application menu, **Run in a Window (Classic)…** (Command-R), pick `halt.nro`.

Before: `RunLogic`'s diagnosis, status and report button are written into the classic window, which was never ordered
front; the code path that shows it is only the separate **Show Classic Launcher** item. After: every classic action
(`chooseHomebrew`, `chooseVerification`, `chooseReport`, `showDiagnosisText`, `showError`) calls `revealClassic()` first.
Limit: found by reading `tools/macos-launcher/main.swift` (the window is only ordered front in `showClassic`) and
not confirmed by clicking through the live window; there is no GUI test harness for AppKit here.

### L5, L6 (open)

- L5: in the classic window choose **Verify against Host…**, pick any non-harness `.nro`. The diagnosis explains the
  mismatch and offers no report action.
- L6: `docs/MAC_APP.md` and the README state the unsigned status. No prebuilt app is offered.

## Repro inputs

`docs/verify/ux-flow/make_failing_nros.py` writes `halt.nro`, `svc.nro` and `loop.nro` into the folder given as its
first argument. They contain no third-party code: a 0x1000-byte image with an `NRO0` header and one instruction word.

## Before and after, summary

| Fix | Before | After |
|---|---|---|
| Bare command | demo runs | 8-line start screen |
| Unknown command | manual, exit 0 | one-line error with suggestion, exit 2 |
| Missing path | exit 0 or 1, banner first | message with example, exit 2 |
| Manual | emoji, `cargo run`, `ui` buried | plain, `ounce-core`, workflow order |
| Early stop | stop line only | cause, command to try, report link |
| Missing file | OS error text | path, advice, no banner |
| Finished run (app) | result in hidden log | result in the diagnosis card |
| Failed run (app) | find template by hand | one-click copied report and prefilled form |
| Classic window (app) | results written to a hidden window | window brought forward before any result or error |
| Install | raw failure, no next step | prerequisite list, next-step text |
