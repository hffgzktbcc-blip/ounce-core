# Multi-Agent Coordination & Engineering Standards

This document establishes operating rules for AI coding assistants (Antigravity, Claude Code, and peers) collaborating on `ounce-core`.

## 1. Non-Contradiction & Collaborative Building
* **Never undo or revert peer agents' work without mutual alignment**: Inspect git history, existing unit tests, and architecture stages before modifying code. Build incrementally upon verified code.
* **Strict Green CI Policy (Rule from the Owner)**: **Never merge to `main` until all CI checks are green.** Never use admin bypass under any circumstances. Every pull request must wait for all 9 cross-platform matrix jobs (macOS, Ubuntu, Windows across standard test, `gpu-present`, and `native-jit`) to succeed before merging.
* **Direct Inter-Agent Communication Protocol**: Any collaborating agents (Antigravity, Claude Code, and peers) communicate exclusively through `AGENTS.md` and GitHub PR comments / descriptions. Do not route routine status, handoffs, or technical notes through the user. The user is contacted solely for operations requiring human intervention (credentials, sudo, physical device input, or GitHub permissions).
* **Ensure full functionality**: Any code changes must ensure the entire suite compiles and runs cleanly.
* **Pass all verification gates**:
  - `cargo test --release` (88+ tests must pass).
  - `cargo run --release -- test` must exit 0 and print `[Harness] All N checks passed across 25 stages.` Every stage asserts its own guest exit code (see README stage table); any failure prints `[Gate] FAIL` and the process exits 1.
  - `grep -rn "unsafe" src` must return 0 hits (pure safe Rust only).

## 2. Core Architectural & Legal Boundary
* **Target Architecture**: NVIDIA Tegra T239 ("Drake" SoC / Ampere GA10B GPU) and Horizon OS 23.0.1 ABI.
* **Scope**: Clean-room systems research runtime and compatibility layer for synthetic test vectors and homebrew applications.
* **Legal Policy**: Commercial retail game bootstrapping and retail key decryption are permanently disabled by policy (`loader/nca.rs` throws `LoadError::RestrictedCipher`). Do not re-enable retail decryption or introduce proprietary keys or firmware bytes.
* **Terminology**: Avoid the word "emulator" in repository descriptions, documentation, and metadata.
* **Git Author**: Retain `hffgzktbcc-blip <hffgzktbcc-blip@users.noreply.github.com>`.

## Finish line (set 2026-11-01)
**One unmodified, independently built homebrew sample (pinned switch-examples, see `docs/independent-corpus.md`) boots under `run`, gets past VI/Binder startup (no `BadGfxInit`), presents at least one frame whose pixels are checked, and exits cleanly. CI runs it on all nine jobs.**
Why this one: every current passing fixture is project-authored, so it proves nothing about outside software. This is the first independent evidence and it unblocks the GPU, presentation and Tier-3/4 work already built.
Order: (1) VI/Binder bootstrap per `docs/CODEX_HANDOFF.md`; (2) frame descriptor into `frame_from_surface`; (3) sample passes in CI. Anything not on this path waits.

## 3. Active Handoff Log
* **Completed State**:
  - Release v0.3.0 was published. History cleanup is not verified complete; see the dated, limited ref check in `README.md` and `docs/PROVENANCE.md`.
  - Phase 2 implemented: Physical gamepad input via `gilrs` and 48 kHz stereo host audio sink via `cpal`.
  - Multi-platform CI workflows verified on Linux, macOS, and Windows.
* **Deliberate deviations and limits (do not "fix" by guessing)**:
  - `DT_INIT_ARRAYSZ` is 27 per the ELF spec (26 is `DT_FINI_ARRAY`).
  - NSO/NRO images carry no program headers, so `PT_TLS` is only available for ELF input (`NroLoader::parse_pt_tls`) or resolved via `__tdata_lma` devkitA64 linker script fallback symbols.
  - `TPIDRRO_EL0` stays the Horizon IPC thread-local region; only `TPIDR_EL0` points at the ELF TLS area.
  - Initialisers are not run by default because a program's own `crt0` normally runs them; the runner does not service SVCs and reports them instead.
  - Validated on synthetic test modules only (homebrew and synthetic workloads are the scope).
* **Verification & Quality Gates** (see section 1): `cargo test --release` (and `--features native-jit`, `gpu-present`) all pass; `cargo run --release -- test` prints all 27 checks across 25 stages; `grep -rn "unsafe" src` returns 0 hits (`unsafe` lives only in `crates/ounce-recompiler/src/exec.rs`). Last recorded: 392 tests with native-jit, 379 with gpu-present.
* **Working model (2026-11-01)**: One lead agent owns the finish line and any file it needs; no per-agent file reservations or lanes. At most one helper, for side work such as review or CI triage, on a separate branch with a draft PR. Coordinate through PR descriptions/comments, not this log. Older lane/ownership text: `docs/HANDOFF_HISTORY.md`. Rules in sections 1 and 2 still apply.
* **Pointers**: Corpus and VI/Binder work: [`docs/CODEX_HANDOFF.md`](docs/CODEX_HANDOFF.md). SVC status: `docs/svc-coverage.md`. Design notes: `docs/design/`. Per-feature history and each subsystem's "Left / Not done" list: `docs/HANDOFF_HISTORY.md`.
* **Open items** (details in history): independent homebrew still fails at VI/Binder startup (`BadGfxInit`), so no corpus sample passes; Tier-4 BTI landing-pad checks and aarch64 numbers; loops, memory ops and `SHFL` in the shader translator; Ampere MME; multi-level textures; several GPU behaviours marked UNVERIFIED against hardware.
