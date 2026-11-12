# Workbench page: flow, options and app hooks

The workbench is the page `ounce-core ui` serves (`assets/lab/index.html`, `app.js`, `app.css`). The Mac app
shows it in a `WKWebView` (`tools/macos-launcher/Workbench.swift`). This page is meant to be the only front
door: everything the older launcher window offered is an option here.

## The flow

1. **Choose a program.** A drop zone that is also a button; once a program is chosen it becomes the file name, its size and
   a "Choose another" link. One line says the file stays on this computer.
   A short "What happens next" list shows until the first check has been made.
2. **Pick how to check it.** A one-sentence summary of the standard check, and one **Advanced** section:
   * *Engines to use*: both (default), interpreter only, recompiler only.
   * *Step limit*: the instruction budget (the old "Run Settings" budget). Empty means the server default.
   * *Also run on this computer and compare*: the native reference run. Disabled with a reason when the
     build cannot do it.
   * *Compare with the recorded reference*: the old "Verify against Host". For the known test programs
     (matched by file name, like `scripts/verify_differential.py`), compares the program's result file with
     `<verify dir>/<harness>/expected-host.txt`. Disabled with a reason when the data is not installed.
3. **Result.** One large verdict, in plain words: **Results match**, **Results differ** or **Could not run this program**, then a
   headline, one paragraph, and "What to do next" with buttons. *Evidence* (per-engine outcomes, first
   difference, recorded-reference comparison, instruction check) and *Details* (hash, step limit, logs) are
   collapsed below.
4. **If it fails.** The headline is a diagnosis in plain words. **Report This Problem** opens the project's
   issue form with the known fields filled in; **Copy details** copies the same text. Nothing is sent
   without the person submitting the form.

Light and dark follow the system. The page works from 320 px wide, is fully keyboard-operable (the verdict
takes focus when a result arrives), and respects reduced motion. The page uses no inline script or style
(the smoke test enforces this).

## Server changes (additive; existing calls behave as before)

| Call | Notes |
| --- | --- |
| `POST /api/checks?name=NAME&native=0\|1` | Unchanged. New optional query: `engines=tier3,tier4` (any non-empty subset), `budget=N` (positive integer, overrides the server's `--budget`), `reference=1` (run the recorded-reference comparison). Bad values return `400` with a plain message. |
| `GET /api/checks`, `GET /api/checks/ID` | Unchanged. A check now also carries `budget` and, when requested, `host_reference` (`status` is `matches`, `differs`, `unknown-program`, `no-output`, `not-run` or `unavailable`). |
| `GET /api/checks/ID/program` | New. The `.nro` the check was started with (`application/octet-stream`); `404` for an unknown id. |
| `GET /api/config` | New. `{native, default_budget, timeout_seconds, engines, reference_data}`. Needs the session key like every `/api/` call. |

`ounce-core ui --verify-dir DIR` (or `OUNCE_VERIFY_DIR`) points at a folder laid out like `docs/verify`:
`differential.json` plus `<harness>/expected-host.txt`. Without the option the server uses `docs/verify` if it
exists in the working directory. **The app should pass `--verify-dir` with its bundled `Resources/Verify`.**

## Page-level actions for the app

Everything below is optional: the app keeps working if it calls nothing new.

* `pick('<id>')` (unchanged): show a check the app started itself through `POST /api/checks`.
* `OunceWorkbench.options()` returns `{engines, budget, native, reference}` as currently set in the page.
* `OunceWorkbench.queryString()` returns those settings as query text to append to `POST /api/checks?name=...`.
  If the app uploads a program itself (`Workbench.check(file:)`), append this instead of the fixed
  `native=0`, then call `pick(id)`.
* `OunceWorkbench.setOptions({engines, budget, native, reference})` sets any subset. The app can use this to
  restore its saved settings after load. The page also remembers settings in `localStorage`, but the origin
  changes with the random port, so that does not survive an app restart.
* `OunceWorkbench.stage(fileOrId)` stages a program for the next check, without starting one. Give it a `File` or the id of
  a check the server holds: the page then fetches the program from `GET /api/checks/ID/program` (needs the session key;
  404 for unknown ids). The app calls `stage(id)` after uploading a program itself, so the result's "Check again…"
  buttons work. A path is not accepted: a web view cannot read one, and the id is all the app has after an upload.
* `OunceWorkbench.reportRequest()` returns `{url, text}`, the same report *Report This Problem* sends, for the check on
  screen, or `null` when no check has finished. Help > Report a Problem in the app uses it, so both agree.
* Save Report…: with the message handler present the page posts `{action: "save", name, text}` (the app shows a save panel);
  in a browser it downloads a text file.
* `OunceWorkbench.choose()` opens the file picker (handled by `runOpenPanelWith`).
* `OunceWorkbench.reset()` returns to "Choose a program".
* Report: when `window.webkit.messageHandlers.ounceWorkbench` exists the page posts
  `{action: "report", url, text}` instead of opening the form: open `url` in the default browser and keep
  `text` for the clipboard. Without that handler the page uses `window.open`, then falls back to
  `location.assign`, which `decidePolicyFor` already sends to the default browser.
* Native comparison asks for confirmation with `confirm()`, which `Workbench.swift` already shows as a sheet.

Test only: `OUNCE_LAB_TEST_FIXTURES=<folder>` makes the server list every `*.json` in the folder as a finished check
(a crafted result, nothing is run). It exists so a state the machine cannot produce on demand, such as *Results differ*,
can be looked at in the real window. Fixture names should say so (`TEST-FIXTURE-…`).

## What the app can retire

With these options in the page, the launcher window's *Run Settings*, *Verify against Host*, *Report This
Problem*, *run in a window* and *Classic window* paths are duplicated by the page, except for the graphics demo
and visible (GUI) runs, which the page does not cover. The app has now retired *Verify against Host* and the old launcher window; *Run Settings* stays for
*Run in a Window* only.

## Not verified here

The page was exercised in a browser against a debug build (empty, failed and matched states; upload through
the file input; Advanced section). The embedded `WKWebView` behaviour, the `ounceWorkbench` message handler
and `--verify-dir` from the app bundle were not run. The "this computer" comparison is unavailable in the
build used (no `nce` feature, not AArch64 Linux), so its result states were not seen; the recorded-reference
comparison is covered by unit tests, not an end-to-end run with a real harness `.nro`.
