# Real Linux ARM64 programs through the Mac app

Run on 2026-11-12, Apple Silicon Mac (macOS 26), the app built from this branch with `scripts/build_macos_app.sh`, driven with Orca computer use. Every
program is an unmodified pinned upstream source from [docs/verify/linux-elf/PROGRAMS.md](../verify/linux-elf/PROGRAMS.md), built with
`scripts/verify_linux_elf.py` (Zig 0.17.1, `aarch64-linux-musl`, `-O1`), then given to the app with `open -a` (Finder-style open), with
"Compare with the recorded reference" ticked. No binary is committed. Screenshots are of the app window only.

| # | Program (file name) | Verdict in the app | What was compared | Screenshot |
|---|---|---|---|---|
| 1 | LZ4 `lz4.elf` | Results match | Tier-3 vs Tier-4, and stdout vs `lz4-harness/expected-host.txt` (17 lines) | [01](01-lz4.png) |
| 2 | xxHash `xxhash.elf` | Results match | same, `xxhash-harness` | [02](02-xxhash.png) |
| 3 | cJSON `cjson.elf` | Results match | same, `cjson-harness` | [03](03-cjson.png) |
| 4 | zlib `zlib.elf` | Results match | same, `zlib-harness` (122 lines, about 10 s on Tier-3) | [04](04-zlib.png) |
| 5 | Lua `lua.elf` | Results match | same, `lua-harness` | [05](05-lua.png) |
| 6 | SQLite `sqlite.elf` | Results match | same, `sqlite-harness` (22 lines) | [06](06-sqlite.png) |
| 7 | wasm3 `wasm3.elf` | **Results differ** | stdout vs `wasm3-harness/expected-host.txt`: differs from line 2. Both engines exit 1 with `fib load memory allocation failed` | [07](07-wasm3.png) |
| 8 | xxHash built with `-O3 -mcpu=apple_m1` (`xxhash-apple-m1.elf`) | **Could not run this program** | both engines stop at `0x190D011F` (`STLURB`, FEAT_LRCPC2): [#414](https://github.com/hffgzktbcc-blip/ounce-core/issues/414) | [08](08-xxhash-apple-m1.png) |

A later command-line verification fixes the Apple M1 xxHash stop with FEAT_LRCPC2 and FEAT_SHA3: both engines exit 0 with `fails=0` and match the host. See [the reproducible SHA3 follow-up](../verify/linux-elf/SHA3-414.md); the screenshots above record the earlier app run.

The verdict for 1 to 6 was also read from the saved `result.json` of each check (`verdict.kind` `agrees`, `host_reference.status` `matches`, both engines `exited`).
tinf and jsmn also ran through the server API (not the window) and agreed on both engines; they have no recorded reference file.

## What "Results match" means for a Linux program

Tier-3 and Tier-4 ended the same way and printed byte-identical output; when the reference box is ticked and the file name is a known harness, that output also
equals the recorded output of a native build of the same source. It is **not** compared with this Mac: `run-elf` has no native engine, the page disables that
option for Linux programs and says why. The page's wording for a Linux program names this ("run on Ounce Core's engines only, not on this Mac").

## Rows 7 and 8 are real, not fixtures

* **wasm3** (known gap, [#366](https://github.com/hffgzktbcc-blip/ounce-core/issues/366)): the runner refuses the allocator's ~1 TiB `PROT_NONE` request. The guest then prints
  its own error and exits 1, with both engines in agreement, so only the recorded reference exposes it. Without the reference box the app would say *Results match*
  (two engines agreeing on a failure); the page does not look at a guest's exit code. That is existing verdict logic and was left unchanged.
* **`STLURB`**: found by rebuilding the pinned programs with `-O3 -mcpu=apple_m1`. Lua, cJSON, zlib, xxHash, KISS FFT and CoreMark stop on it; LZ4, tinf, jsmn and
  BLAKE3 built the same way ran. The word comes from the allocator code Zig links, not from the programs' own source. Filed as #414.
* The instruction gaps #400 to #404 (FMULX scalar, FCVTXN, FMOV half-precision immediate, SQRDMLAH, FCADD) were **not** reproduced from a real program: with
  `-O3 -ffast-math -mcpu=generic+rdm+complxnum+fullfp16` KISS FFT, Lua, cJSON, xxHash, zlib and CoreMark ran (no such instruction in their disassembly), and
  `-mcpu=neoverse_v1` output uses SVE, a different gap. No fixture was used for any row above.

## Dock and window drop

Tested with synthetic drags from a Finder window (Orca computer use has no Dock surface; the drag is a window-coordinate drag that I aimed at the Dock icon's screen position).

| Case | Result |
|---|---|
| Drag `lz4.elf` onto the Dock icon, app running | Worked: a new check (lz4, Results match) appeared within seconds; screenshot [09](09-dock-drop-app-running-lz4.png). Delivery was foreground and the tool marks it unverified (synthetic input); the new saved check is the evidence. |
| Drag onto the Dock icon, app not running | **Not tested.** A quit app has no Dock icon unless it is pinned, and pinning means changing the owner's Dock. A cold start with the file (`open -a ... file`, the same `application(_:openFiles:)` path) was run about a dozen times on fresh instances and always opened and checked it. |
| Drag onto the app window | **Not verified.** Four drags from the Finder window did not start a check, but the app window was mostly covered by another Finder window, so the drop point may not have been over it. No claim either way. |

## Report This Problem, then close or quit

Tried to provoke the earlier hang: open a failing program, click *Report This Problem*, then close the window (close button) or quit (Cmd+Q). Six runs each way on a
fresh instance: the app exited at once every time and no `ounce-core ui` server was left. The hang was **not reproduced**. Not covered: Help > Report a Problem through the menu
(the tool cannot drive menus), and quitting by an Apple Event. Note for anyone repeating this: the installed `~/Applications/Ounce Core.app` shares the bundle
ID with a build in another folder, and `--restore-window` of the tool can start or target the installed copy; address the build by `pid:`.

## Not tested

Intel Macs (none available here; the build is arm64-only, `native` and Tier-4 are Apple Silicon features). Any other macOS version. A second pair of eyes: no one else has reproduced these runs.
