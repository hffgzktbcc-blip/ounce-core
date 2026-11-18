# Ounce Family 1-Year Roadmap (Nov 2026 – Nov 2027) — Committed Plan

*Vetted on 2026-11-14 across 4 independent reviews: Architecture & Contract Stability, Licensing & Clean-Room Legal, CI & Release Infrastructure, and Homebrew Developer Experience.*

**Verified release update (17 Nov 2026):** The Q1 family releases are **published**, not pending: [ounce-core v0.9.0](https://github.com/hffgzktbcc-blip/ounce-core/releases/tag/v0.9.0), [ounce-hb v0.1.0](https://github.com/hffgzktbcc-blip/ounce-hb/releases/tag/v0.1.0), and [ounce-runtime v0.6.0](https://github.com/hffgzktbcc-blip/ounce-runtime/releases/tag/v0.6.0). The older pre-release instructions below remain as historical acceptance criteria; do **not** recreate these release PRs or re-tag these versions. Continue the post-tag Q1 developer-experience, deterministic-reporting, real-world validation and DeckStream work after checking live PRs and actual evidence.

**Status: Done / In PR / Next (historical 14–16 Nov checklist; live GitHub wins on current state)**

| Status | Item |
| --- | --- |
| **Done** | Landed 14 Nov 2026: `ounce.run.v1` / `--report-json`; `ounce-hb` `doctor` / `--auto` / `repro`; `setup-ounce`; the Linux ARM64 ELF matrix workflow. |
| **Done** | Release infrastructure merged: `ounce-core` #442, `ounce-hb` #40, `ounce-runtime` #13. Linux ARM64 ELF matrix passed on core `main` at `6a4ccb8` ([job](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/43803024996/job/133087400644)): 28 harnesses / 76 rows. |
| **Done** | The Q1 release tags v0.9.0 / v0.1.0 / v0.6.0 are published (links above). Preserve the candidate notes as historical qualification evidence. |

**Source of truth:** This document supersedes `docs/roadmap/milestones.json` and the `ounce-hb` wiki roadmap. The education roadmap (`OUNCE_EDUCATION_ROADMAP.md`) was deferred until the Q1 tags existed; they now do (see the verified release update above), so scheduling it is an owner decision.

**North Star:** Make **`ounce-core`** (MIT), **`ounce-hb`** (MIT), **`ounce-runtime`** (GPL-3.0-or-later), and **`deckstream`** (MIT, SteamOS WebDAV launcher & Decky UI) the self-diagnosing, zero-AI-needed standard for **building, testing, running, and debugging open-source AArch64 homebrew (`.nro` / `.nso`) and static musl Linux AArch64 ELFs**, with strict license isolation (subprocess + CLI/JSON contracts only) and **zero retail/key/SDK material** (`ounce-hb`: strict clean-room per `docs/CLEAN_ROOM.md`; `ounce-core`: public specs + attributed MIT ports per `docs/CLEAN_ROOM.md`; `ounce-runtime`: GPL-3.0-or-later; `deckstream`: MIT).

---

## Q1 (Months 1–3 | Nov 2026 – Feb 2027): Release Alignment, Day-1 Homebrew DX & Visual Parity

**Theme:** Unify release/CI pipelines, ship `linux-arm64` binaries, and make real `appletMainLoop()` + RomFS/SDMC homebrew work on day 1.

1. **Pre-Release Legal & CI Infrastructure Alignment (single hard gate for synchronized `ounce-core` `v0.9.0` / `ounce-hb` `v0.1.0` / `ounce-runtime` `v0.6.0`):**
   - **Metadata & `cargo-deny`:** Add `license = "MIT"` to `ounce-core/Cargo.toml`; add `deny.toml` and PR `cargo-deny` checks to `ounce-core` and `ounce-runtime`.
   - **Fix `release.yml` across all 3 repos:**
     - Replace `if: matrix.os == 'ubuntu-latest'` with `if: runner.os == 'Linux'` in `ounce-core` & `ounce-runtime` `release.yml` so `ubuntu-24.04-arm` (`linux-arm64`) builds cleanly.
     - Replace hardcoded archive lists in `SHA256SUMS` with wildcard/4-target generation (`ounce-core`, `ounce-hb`).
     - Upgrade `ounce-runtime/.github/workflows/release.yml` to match `ounce-core`/`ounce-hb`: build with `--locked`, run `cargo test --release --locked`, bundle `LICENSE` (GPL-3.0-or-later) + consolidated `THIRD_PARTY_LICENSES.txt` / `THIRD_PARTY.md` + Corresponding Source notice (GPL-3.0 §§ 4, 6), generate `.sha256` files, use `actions/attest@v4`, and publish via a dedicated `needs: build-release` job. Standardize draft/publish behavior.
   - **`ounce-runtime` License & Provenance Pre-`v0.6.0` Gate:**
     - Refresh `docs/DEPENDENCY_LICENSE_AUDIT.md` against the current 321-package `Cargo.lock` (up from 190) and generate binary `THIRD_PARTY_LICENSES.txt` / `NOTICE` texts for compiled dependencies.
     - Add missing `input_multi` and `sdmc` entries to `docs/PROVENANCE.md` and `tests/fixtures/nro/TOOLCHAIN_VERSIONS.md`; rebuild `tests/fixtures/nro/deko3d/deko3d.nro` in a recorded toolchain env to capture `deko3d` + `uam` versions, SHA-256, and notices.
   - **Refresh `RELEASE_NOTICE_REVIEW.json`:** Run toolchain/native notice collection on `ubuntu-24.04-arm` and update `release_scope` before tagging `ounce-core` `v0.9.0`.
   - **Fix `setup-ounce` & tag collision:** Update `ounce-hb/release.yml` trigger from `'v*'` to `'v[0-9]+.[0-9]+.[0-9]+*'` so floating action tags (`setup-ounce@v1`) never trigger binary releases; update `.github/actions/setup-ounce/action.yml` to detect `RUNNER_ARCH` (`linux-arm64`), fix Windows `.exe` / `pwsh` extraction + CI verification, enable `verify_sha="true"` for `ounce-runtime`, and support `install-hb: 'false'` for dogfooding.
   - **Cut Synchronized Releases:** Verify a passing `linux-arm64-elf-matrix` run on `main`, then tag **`ounce-core` `v0.9.0`**, **`ounce-hb` `v0.1.0`**, and **`ounce-runtime` `v0.6.0`** (including `linux-arm64` assets). Nothing else in this quarter blocks these tags.

**November product goal (not a tag blocker):** The `ounce-core` #357 community sprint outcome — a second independent rerun of #191, or a first #248 report.

**After the tag** (one theme at a time):

1. **Day-1 Homebrew Ergonomics & Doc/CLI Sync:**
   - **Loop-exit controls (`--max-frames <N>` and inline `--press PLUS@60` DONE, #458 merged 17 Nov; `OUNCE_INPUT_SCRIPT=<path>` was not built):** so standard `while (appletMainLoop())` `libnx` apps exit cleanly instead of hitting the 60s timeout.
   - **Q1 `--sdmc` & RomFS support: DONE** (#459 merged 17 Nov: copy-on-write `--sdmc <dir>` overlay, `--sdmc-writeback` for write-through, `romfsInit()` verification). Do not redo.
   - **CLI & Doc parity:** Implement `--timeout <dur>` and `--json` on `ounce-hb run` (matching `HOMEBREW_DEVELOPER_GUIDE.md`), sync `ounce-hb/README.md` with `doctor`, `repro`, and `--auto`, and extend `ounce-hb doctor` to check `DEVKITPRO`/`devkitA64`/`cargo-nx` alongside engine binaries.
   - **Applet deadlock (`ounce-core` #387): DONE.** Issue closed, fixed by `0b4489b` (cancelled software-keyboard result); PR #459 adds a regression test. Do not redo.

2. **End-to-End Graphical & Determinism Parity (`ounce.run.v1` / `ounce.repro.v1`):**
   - **Prerequisites before comparing `framebuffer_hash`:**
     - Hash **pre-HUD** guest framebuffer pixels (disable `draw_hud_overlay` under `--present=headless` / `OUNCE_DETERMINISTIC=1`) in both `ounce-core` and `ounce-runtime` to prevent false-positive HUD divergences.
     - Wire `run_report::is_deterministic()` in `ounce-runtime` to match `ounce-core`'s deterministic clock/scheduler behavior.
   - **DONE (#460):** Emit `frames_presented` and `framebuffer_hash` (`fnv1a64:...`) from `ounce-core`'s `RunReport`. **3D frames (#486, unreleased):** headless and `--deterministic` runs draw 3D with the software rasterizer and hash it; only a `gpu-present` GPU window outside deterministic mode reports `null`.
   - Extend `ounce.repro.v1` (`format_repro_bundle` & `ReproManifest`) with `framebuffer_hash`, `frames_presented`, and `input_script`, and teach `ounce-hb run --auto` and `ounce-hb repro` to compare `framebuffer_hash`.

3. **Real-World Homebrew Validation (Metadata-Only Corpus):**
   - Run 5–10 real open-source `libnx` / SDL2 homebrew `.nro` projects through `ounce-hb run --auto` and `ounce-core diverge`; store **only** `ounce.repro.v1` JSON metadata/hashes in `tests/repro/` (never commit `.nro` or RomFS binaries).

4. **DeckStream Family Alignment & Security/CI Gate (`deckstream`, MIT):**
   - Complete `deckstream/PLAN.md` baseline gates (own-storage Steam Deck launch report on `#9`, `assertPublicHttpUrl` SSRF guard, CodeQL high findings, Decky TypeScript check, and `shellcheck` in CI).
   - Sync "The Growing Family" sibling table in `ounce-hb/README.md` and `ounce-runtime/README.md` with `deckstream`, and verify `ounce-hb` / `ounce-runtime` `linux-x86_64` release binaries run on SteamOS 3.5+.

---

## Q2 (Months 4–6 | Feb – May 2027): Deep Diagnostics, `bisect`, & Deterministic Telemetry

**Theme:** Replace guesswork in homebrew debugging with symbolicated faults, `git bisect` automation, and bounded cross-engine trace comparison.

1. **Hard Prerequisite — Safe Recursive-Descent JSON Parser in `ounce-hb`:**
   - Replace `find_field_value()`'s flat substring scan in `ounce-hb/src/report_v1.rs` with a zero-dependency, `#![forbid(unsafe_code)]` recursive-descent JSON parser (and signed `i32` handling for `exit_code`) *before* adding nested arrays/objects to `ounce.run.v1`.

2. **Zero-AI Diagnostic Tooling (`ounce-hb` + `ounce-core`):**
   - **Symbolicated fault telemetry:** Pure safe Rust DWARF/ELF symbolication (`#![forbid(unsafe_code)]`, permissive crate only, no GPL/LGPL C bindings) emitting `pc`, `lr`, `sp`, and top stack frames in `ounce.run.v1`. Exit `11` prints PC + faulting VA + symbol; exit `14` names exact `sm` service + command ID. **Partly done (#475, unreleased): `lr` and `sp` are in the report on `run`, `run-nro` and `run-elf`.** Still open: symbolication and stack frames, which need a DWARF/ELF parsing crate (a new dependency, so an owner decision), and naming the `sm` service + command ID for exit `14`.
   - **Live hang visibility & guest args:** Add `--stream` (or flush-on-timeout) so hung `.nro` runs show stdout/stderr immediately (**core side DONE** by #476, unreleased: `run`/`run-nro` flush guest debug output as it is written, so no flag is needed; `run-elf` already flushed each write), and lift the guest `-` argument restriction so homebrew test harnesses can pass flags like `--verbose` after `--` (**core side DONE** by the `ounce-core` #487 fix: `run`/`run-nro` pass everything after `--` through and refuse unknown options before it; #418/#420 fixed argv overflow, not this; `ounce-hb` still refuses dashed guest args until it drops that workaround; headless no-audio #419 DONE in #421).
   - **`ounce-hb bisect`:** Automated `git bisect run` wrapper around `ounce-hb repro`, mapping `ounce-hb` infrastructure exits (`2..5`) to `125` (skip untestable commit) and guest pass/fail to `0`/`1`.
   - **`ounce-hb diff-trace` via `ounce.trace.v1`:** Define a bounded first-divergence summary contract (`OUNCE_TRACE_SUMMARY=<path>`) implemented independently by both `ounce-core` and `ounce-runtime`, preserving license isolation. **Status (#477, 18 Nov 2026):** contract in [`docs/OUNCE_TRACE_V1_SCHEMA.md`](../OUNCE_TRACE_V1_SCHEMA.md); the `ounce-core` side (`OUNCE_TRACE_SUMMARY` writer, `ounce-core trace-diff`) is implemented and test-covered, not yet merged. `ounce-runtime` and `ounce-hb diff-trace` are NOT done, so nothing has been cross-checked between engines.

3. **Audio & Assertion Parity (`ounce.run.v1` additive fields):**
   - Add `audio_frames_mixed` / `audio_hash` for headless `audout` / SDL_mixer verification, plus a golden-output / `framebuffer_hash` assertion flag on `ounce-hb run`/`repro` for `Makefile` & CI test suites. **Core side DONE (#478, unreleased) for `audout` and `audren`:** `ounce.run.v1` carries both fields on every `run` / `run-nro` stop (`run-elf` writes `0` / `null`), counted as audio is mixed, so `--present=headless` and a run with a sound device agree. Tested against the audren fixture's documented waveform on both tiers. Still open: the `ounce-hb` assertion flag, and SDL_mixer, which is a guest library on top of `audout` and has no separate core path.

4. **Expand `run-elf` Bounded Envelope & `ounce.run.v1` Parity (Static musl AArch64):**
   - **DONE (#473, unreleased):** Wire `ounce.run.v1` (`--report-json` / `OUNCE_REPORT_JSON`) and `--dump-regs` (`X0`/`D0`/`NZCV` on stop) into `ounce-core`'s `run-elf` runner (`src/linux/runner.rs`) so ELF workloads share the same machine-readable contract as `.nro`. Adds the flat, additive `guest_exit_status` field.
   - Expand `run-elf` & `linux-arm64-elf-matrix` toward 50+ single-threaded/bounded-file/deterministic static musl AArch64 workloads fetched/built cleanly in CI (no vendored unclear/GPL binaries), keeping scope strictly bounded to static musl `ET_EXEC`.
     - **Status (18 Nov 2026, #474 in progress):** 45 of 50+ harnesses (28 at the v0.9.0 matrix run, +17 first-party `elf-*` workloads, see `docs/verify/elf-workloads/README.md`). The 17 new ones are verified locally on x86-64 (Tier-3, Tier-4 and `diverge`, O1/O2/O3_M1, against host builds) but have not run on the `ubuntu-24.04-arm` CI job yet. Not complete.

5. **DeckStream `.nro` Subprocess & `ounce.run.v1` Integration (`deckstream`, MIT):**
   - Add `.nro`/`.NRO` dispatch in `deckstream/daemon/game-launcher.sh` **before** Proton/umu checks (`DECKSTREAM_NRO_RUNNER=hb` runs `ounce-hb run --json "$EXE"`; default runs user-installed `ounce-runtime run "$EXE" --present=gpu` — strictly external subprocess, never bundled or auto-fetched in `deckstream` releases).
   - Wire `ounce-hb doctor --json` into `deckstream/daemon/doctor.py` and surface `ounce.run.v1` `stop_kind`, `exit_code`, and symbolicated fault lines in `launch_report.py`.
   - Map `--sdmc <dir>` to a per-game DeckStream directory compatible with `fuse-overlayfs` and `saves.py` (Ludusavi) backups.

---

## Q3 (Months 7–9 | May – Aug 2027): Ecosystem Adoption, CI Templates & JIT/NCE Hardening

**Theme:** Meet homebrew developers inside `cargo-nx` and `devkitA64` `Makefile`s, visualize CI regressions, and close `ounce-runtime` accuracy gaps.

1. **One-Line Build Tool & CI Integration:**
   - Publish copy-pasteable `.cargo/config.toml` (`runner = "ounce-hb run"`) snippets for `cargo-nx`, `devkitA64` `Makefile` `make test` targets, and GitHub Actions starter workflows using `setup-ounce@v1` (with nominative-use disclaimers; never redistributing `devkitPro`/`libnx` binaries).
   - Emit PNG screenshot / visual-diff artifacts alongside `ounce-repro.json` and rich `$GITHUB_STEP_SUMMARY` tables (`stop_kind`, `insns_executed`, `frames_presented`, `framebuffer_hash`) when CI detects divergence.
   - Ship a lightweight `tasks.json` + `problemMatcher` bundle for VS Code matching `ounce-hb`'s symbolicated crash lines (no heavy custom extension).

2. **One-Way JIT / NCE Hardening (`ounce-runtime` & `ounce-core`):**
   - Feed Q1–Q2 `ounce.repro.v1` bug reports into `ounce-runtime` Tier-3 / Tier-4 / NCE so fast execution matches `ounce-core` on FP/SIMD, NaN, and SVC edge cases.
   - **Strict One-Way Rule:** `ounce.repro.v1` JSON discoveries may inform GPL `ounce-runtime` fixes, **never** the reverse—no code or patterns from `ounce-runtime` may ever flow into MIT `ounce-core` or `ounce-hb`.
   - Track CI performance regressions strictly via deterministic `insns_executed` budgets (keep wall-time tracking advisory on shared runners). **Core side DONE (#479):** `tests/insn-budgets.json`, checked by `scripts/insn_budgets.py` in the Runtime CI job on three hosts; see `docs/INSN_BUDGETS.md`.
   - **#400–#404 (instruction gaps): ALL CLOSED 16 Nov 2026** (#400 `32e69ac`, #401 #449, #402 #444, #403 #450, #404 #451). Do not restart them.

3. **Quarterly Coordinated Releases & Supply-Chain Hardening:**
   - Cut Q2/Q3 releases (`ounce-core` `v0.10.0` → `v0.11.0`, `ounce-hb` `v0.2.0` → `v0.3.0`, `ounce-runtime` `v0.7.0` → `v0.8.0`) with SPDX/SBOM generation, SHA-pinned GitHub Actions / Dependabot across all 3 repos, and `SHA256SUMS`. **`ounce-core` side partly done (#480, unreleased):** `release.yml` generates an SPDX 2.3 SBOM per target (`scripts/generate_sbom.py`, also on a manual dry run) and `SHA256SUMS` covers it; every workflow `uses:` is pinned to a commit SHA except six listed in `scripts/check_action_pins.py` (their SHAs were not available), which a test keeps from growing. Not done: the six pins, a hosted release dry run, and the other two repositories.

4. **DeckStream SteamOS Gaming Mode Verification & Mock CI:**
   - Verify end-to-end `.nro` launch over WebDAV/rclone in SteamOS Gaming Mode via `ounce-runtime --present=gpu` (Steam Deck `gilrs` gamepad input, `MESA_SHADER_CACHE_DIR`, `DECKSTREAM_FPS_CAP`), and profile cold/warm RomFS range reads in `startup_profile.py`.
   - Add stub-binary regression tests in `deckstream`'s `run_all_tests.sh` covering `.nro` dispatch, `--sdmc` mapping, and `ounce.run.v1` exit-code handling without requiring `ounce-runtime` in `deckstream` CI.

---

## Q4 (Months 10–12 | Aug – Nov 2027): v1.0 Contract Freeze, Provenance Closure & Reproduction Dossier

**Theme:** Lock contracts, close every open provenance item, publish the 2027 Verification Dossier, and graduate all 3 repos to `v1.0.0`.

1. **Contract Freeze (`v1.0`):**
   - Freeze `ounce.run.v1` (additive-only compatibility), `ounce.repro.v1`, `ounce.trace.v1`, exit codes (`0`, `2–5`, `10–17`), and DX CLI flags (`--timeout`, `--sdmc`, `--max-frames`, `--json`). Tag `setup-ounce@v1`.
   - Lock `deckstream`'s `.nro` launcher and `doctor.py` integration against the frozen `ounce.run.v1` schema, exit codes, and `--present=gpu` / `--json` flags.
   - Guarantee C/C++ `devkitA64` users can install and run `ounce-hb` + `ounce-core` from prebuilt archives/Homebrew with zero Rust toolchain required.

2. **2027 Independent Reproduction & Benchmark Dossier:**
   - Publish a reproducible report covering Apple Silicon hardware-replay fuzzer results, `ubuntu-24.04-arm` multi-opt ELF matrix results, and headless graphical + audio + `libnx` parity results (with nominative-use disclaimer; opt-in external verification only, never unsolicited PRs).

3. **Provenance & Security Closure Gate for `v1.0.0`:**
   - **Must close before `v1.0.0` tags:**
     - `ounce-runtime/docs/PROVENANCE.md` & `LICENSE` hygiene: complete upstream comparisons + binary component inventories (`libnx`, `newlib`, `libgcc`, `libdeko3d`) for all 11 `tests/fixtures/nro/*` fixtures (including `romfs/greeting.txt`) and `uam` shader notices (or generate fixtures cleanly in CI), add SPDX headers (`GPL-3.0-or-later` / `MIT` in `crates/ounce-recompiler`), clarify `src/hid/gc_mac.m:7` (`pad_apple.h`), and remove draft status.
     - `ounce-core/docs/PROVENANCE.md` & `LICENSE_DISPOSITION.md`: close/disposition `ns2jb` git-history object, `refs/pull/1/head`, and specialist review of `RELEASE_NOTICE_REVIEW.json` (keep prebuilt Mac app bundle out of `v1.0.0` unless `#310` `objc2`/Apple SDK review completes).
     - `ounce-hb/docs/CLEAN_ROOM.md`: align or explicitly document contributor boundaries alongside `ounce-core`'s `docs/CLEAN_ROOM.md`.
   - Full `cargo-deny`, `RustSec`, `CodeQL`, and `#![forbid(unsafe_code)]` verification, then cut **`ounce-core` `v1.0.0`**, **`ounce-hb` `v1.0.0`**, **`ounce-runtime` `v1.0.0`**, and a qualified **`deckstream`** release (`decky/DeckStream.zip`, `daemon/manifest.json` SHA-256s, checksums, and `docs/LIMITS.md` scoped to measured SteamOS + WebDAV results).
