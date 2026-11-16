# Ounce Family 1-Year Roadmap (Nov 2026 – Nov 2027) — Committed Plan

*Vetted on 2026-11-14 across 4 independent reviews: Architecture & Contract Stability, Licensing & Clean-Room Legal, CI & Release Infrastructure, and Homebrew Developer Experience.*

**North Star:** Make **`ounce-core`** (MIT), **`ounce-hb`** (MIT), **`ounce-runtime`** (GPL-3.0-or-later), and **`deckstream`** (MIT, SteamOS WebDAV launcher & Decky UI) the self-diagnosing, zero-AI-needed standard for **building, testing, running, and debugging open-source AArch64 homebrew (`.nro` / `.nso`) and static musl Linux AArch64 ELFs**, with strict license isolation (subprocess + CLI/JSON contracts only) and **zero retail/key/SDK material** (`ounce-hb`: strict clean-room per `docs/CLEAN_ROOM.md`; `ounce-core`: public specs + attributed MIT ports per `docs/CLEAN_ROOM.md`; `ounce-runtime`: GPL-3.0-or-later; `deckstream`: MIT).

---

## Q1 (Months 1–3 | Nov 2026 – Feb 2027): Release Alignment, Day-1 Homebrew DX & Visual Parity

**Theme:** Unify release/CI pipelines, ship Stage 1–4 + `linux-arm64` binaries, and make real `appletMainLoop()` + RomFS/SDMC homebrew work on day 1.

1. **Pre-Release Legal & CI Infrastructure Alignment (Hard Gate Before `v0.9.0`/`v0.1.0`/`v0.6.0`):**
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
   - **Cut Synchronized Releases:** Verify `linux-arm64-elf-matrix` (`43630557848`) passes on `ounce-core` `main`, then tag **`ounce-core` `v0.9.0`**, **`ounce-hb` `v0.1.0`**, and **`ounce-runtime` `v0.6.0`** (including `linux-arm64` assets).

2. **Day-1 Homebrew Ergonomics & Doc/CLI Sync:**
   - **Loop-exit controls:** Add `--max-frames <N>` and inline `--press PLUS@60` (plus `OUNCE_INPUT_SCRIPT=<path>`) so standard `while (appletMainLoop())` `libnx` apps exit cleanly instead of hitting the 60s timeout.
   - **Q1 `--sdmc` & RomFS support:** Support ephemeral/copy-on-write `--sdmc <dir>` staging and RomFS (`romfsInit()`) verification in Q1 rather than deferring to Q2.
   - **CLI & Doc parity:** Implement `--timeout <dur>` and `--json` on `ounce-hb run` (matching `HOMEBREW_DEVELOPER_GUIDE.md`), sync `ounce-hb/README.md` with `doctor`, `repro`, and `--auto`, and extend `ounce-hb doctor` to check `DEVKITPRO`/`devkitA64`/`cargo-nx` alongside engine binaries.

3. **End-to-End Graphical & Determinism Parity (`ounce.run.v1` / `ounce.repro.v1`):**
   - **Prerequisites before comparing `framebuffer_hash`:**
     - Hash **pre-HUD** guest framebuffer pixels (disable `draw_hud_overlay` under `--present=headless` / `OUNCE_DETERMINISTIC=1`) in both `ounce-core` and `ounce-runtime` to prevent false-positive HUD divergences.
     - Wire `run_report::is_deterministic()` in `ounce-runtime` to match `ounce-core`'s deterministic clock/scheduler behavior.
   - Emit `frames_presented` and `framebuffer_hash` (`fnv1a64:...`) from `ounce-core`'s `RunReport`.
   - Extend `ounce.repro.v1` (`format_repro_bundle` & `ReproManifest`) with `framebuffer_hash`, `frames_presented`, and `input_script`, and teach `ounce-hb run --auto` and `ounce-hb repro` to compare `framebuffer_hash`.

4. **Real-World Homebrew Validation (Metadata-Only Corpus):**
   - Run 5–10 real open-source `libnx` / SDL2 homebrew `.nro` projects through `ounce-hb run --auto` and `ounce-core diverge`; store **only** `ounce.repro.v1` JSON metadata/hashes in `tests/repro/` (never commit `.nro` or RomFS binaries).

5. **DeckStream Family Alignment & Security/CI Gate (`deckstream`, MIT):**
   - Complete `deckstream/PLAN.md` baseline gates (own-storage Steam Deck launch report on `#9`, `assertPublicHttpUrl` SSRF guard, CodeQL high findings, Decky TypeScript check, and `shellcheck` in CI).
   - Sync "The Growing Family" sibling table in `ounce-hb/README.md` and `ounce-runtime/README.md` with `deckstream`, and verify `ounce-hb` / `ounce-runtime` `linux-x86_64` release binaries run on SteamOS 3.5+.

---

## Q2 (Months 4–6 | Feb – May 2027): Deep Diagnostics, `bisect`, & Deterministic Telemetry

**Theme:** Replace guesswork in homebrew debugging with symbolicated faults, `git bisect` automation, and bounded cross-engine trace comparison.

1. **Hard Prerequisite — Safe Recursive-Descent JSON Parser in `ounce-hb`:**
   - Replace `find_field_value()`'s flat substring scan in `ounce-hb/src/report_v1.rs` with a zero-dependency, `#![forbid(unsafe_code)]` recursive-descent JSON parser (and signed `i32` handling for `exit_code`) *before* adding nested arrays/objects to `ounce.run.v1`.

2. **Zero-AI Diagnostic Tooling (`ounce-hb` + `ounce-core`):**
   - **Symbolicated fault telemetry:** Pure safe Rust DWARF/ELF symbolication (`#![forbid(unsafe_code)]`, permissive crate only, no GPL/LGPL C bindings) emitting `pc`, `lr`, `sp`, and top stack frames in `ounce.run.v1`. Exit `11` prints PC + faulting VA + symbol; exit `14` names exact `sm` service + command ID.
   - **Live hang visibility & guest args:** Add `--stream` (or flush-on-timeout) so hung `.nro` runs show stdout/stderr immediately, and lift the guest `-` argument restriction (`ounce-core #418`) so homebrew test harnesses can pass flags like `--verbose` after `--`.
   - **`ounce-hb bisect`:** Automated `git bisect run` wrapper around `ounce-hb repro`, mapping `ounce-hb` infrastructure exits (`2..5`) to `125` (skip untestable commit) and guest pass/fail to `0`/`1`.
   - **`ounce-hb diff-trace` via `ounce.trace.v1`:** Define a bounded first-divergence summary contract (`OUNCE_TRACE_SUMMARY=<path>`) implemented independently by both `ounce-core` and `ounce-runtime`, preserving license isolation.

3. **Audio & Assertion Parity (`ounce.run.v1` additive fields):**
   - Add `audio_frames_mixed` / `audio_hash` for headless `audout` / SDL_mixer verification, plus a golden-output / `framebuffer_hash` assertion flag on `ounce-hb run`/`repro` for `Makefile` & CI test suites.

4. **Expand `run-elf` Bounded Envelope & `ounce.run.v1` Parity (Static musl AArch64):**
   - Wire `ounce.run.v1` (`--report-json` / `OUNCE_REPORT_JSON`) and `--dump-regs` (`X0`/`D0`/`NZCV` on stop) into `ounce-core`'s `run-elf` runner (`src/linux/runner.rs`) so ELF workloads share the same machine-readable contract as `.nro`.
   - Expand `run-elf` & `linux-arm64-elf-matrix` toward 50+ single-threaded/bounded-file/deterministic static musl AArch64 workloads fetched/built cleanly in CI (no vendored unclear/GPL binaries), keeping scope strictly bounded to static musl `ET_EXEC`.

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
   - Track CI performance regressions strictly via deterministic `insns_executed` budgets (keep wall-time tracking advisory on shared runners).

3. **Quarterly Coordinated Releases & Supply-Chain Hardening:**
   - Cut Q2/Q3 releases (`ounce-core` `v0.10.0` → `v0.11.0`, `ounce-hb` `v0.2.0` → `v0.3.0`, `ounce-runtime` `v0.7.0` → `v0.8.0`) with SPDX/SBOM generation, SHA-pinned GitHub Actions / Dependabot across all 3 repos, and `SHA256SUMS`.

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
