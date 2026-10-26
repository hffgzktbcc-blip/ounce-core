# Multi-Agent Coordination & Engineering Standards

This document establishes operating rules for AI coding assistants (Antigravity, Claude Code, and peers) collaborating on `ounce-core`.

## 1. Non-Contradiction & Collaborative Building
* **Never undo or revert peer agents' work without mutual alignment**: Inspect git history, existing unit tests, and architecture stages before modifying code. Build incrementally upon verified code.
* **Ensure full functionality**: Any code changes must ensure the entire suite compiles and runs cleanly.
* **Pass all verification gates**:
  - `cargo test --release` (52+ tests must pass).
  - `cargo run --release -- test` (all 24 verification stages must pass exiting `0xEEE`).
  - `grep -rn "unsafe" src` must return 0 hits (pure safe Rust only). `ounce-core` itself is `#![forbid(unsafe_code)]` (attribute lands with M3A step 1).

## 1a. Unsafe Boundary (user decision, 2026-10-26)
* `ounce-core` (`src/`) never contains `unsafe`. Use safe crates first (e.g. `wgpu` for graphics).
* Only exception path: a separate workspace crate under `crates/`, documented and audited, optional via a Cargo feature, with a safe fallback in `ounce-core`.
  - Graphics: raw `ash` only inside `crates/ounce-vulkan`, and only if `wgpu` proves insufficient. Not planned for M3A.
  - Milestone 4 native JIT: executable pages (`mprotect`/`VirtualProtect`) and calling emitted code cannot be safe, so the emitter (own or Cranelift) lives in `crates/ounce-recompiler`. The safe Tier-3 superblock engine (~100 MIPS) stays the default fallback when that crate is disabled.

## 2. Core Architectural & Legal Boundary
* **Target Architecture**: NVIDIA Tegra T239 ("Drake" SoC / Ampere GA10B GPU) and Horizon OS 23.0.1 ABI.
* **Scope**: Clean-room systems research runtime and compatibility layer for synthetic test vectors and homebrew applications.
* **Legal Policy**: Commercial retail game bootstrapping and retail key decryption are permanently disabled by policy (`loader/nca.rs` throws `LoadError::RestrictedCipher`). Do not re-enable retail decryption or introduce proprietary keys or firmware bytes.
* **Terminology**: Avoid the word "emulator" in repository descriptions, documentation, and metadata.
* **Git Author**: Retain `hffgzktbcc-blip <hffgzktbcc-blip@users.noreply.github.com>`.

## 3. Active Handoff Log
* **Completed State**:
  - History rewrite and release v0.3.0 complete; dirty tags purged.
  - Phase 2 implemented: Physical gamepad input via `gilrs` and 48 kHz stereo host audio sink via `cpal`.
  - Multi-platform CI workflows verified on Linux, macOS, and Windows.
* **Merged: ELF dynamic linking and TLS** (PR #3 + PR #4 on `main` @ 629cd87, 2026-10-26)
  - PR #3 (`claude/elf-dynamic-linking`): MOD0 `.dynamic` parsing; `R_AARCH64_RELATIVE`, `ABS64`, `GLOB_DAT`, `JUMP_SLOT`; cross-module imports/exports in loose packages; unresolved imports reported in `LooseBootInfo`; README Linux runtime note.
  - PR #4 (`claude/project-thread-1yu255`, stacked on #3; merge #3 first): `DT_REL`/`DT_RELSZ`/`DT_RELENT`/`DT_PLTREL`; `DT_GNU_HASH`; TLS relocations (`TPREL64`, `DTPMOD64`, `DTPREL64`, `TLSDESC`) on a static AArch64 variant I layout; `PT_TLS` parsing with `.tdata` copy, `.tbss` zero-fill and `p_align`; per-thread TLS area and `TPIDR_EL0` for every `svcCreateThread` thread; init/fini runner (`src/loader/init.rs`, opt-in via `OUNCE_RUN_INIT=1`); README Fedora/Arch package names.
  - Verified on PR #4 head: `cargo build --release` no warnings, `cargo test --release` 69 passed, `cargo run --release -- test` 24 stages `0xEEE`, no `unsafe` in `src/`.
  - CI (`ci.yml`) only runs on PRs targeting `main`, so stacked PR #4 has no CI until #3 merges and #4 is retargeted.
* **Deliberate deviations and limits (do not "fix" by guessing)**:
  - `DT_INIT_ARRAYSZ` is 27 per the ELF spec (26 is `DT_FINI_ARRAY`).
  - NSO/NRO images carry no program headers, so `PT_TLS` is only available for ELF input (`NroLoader::parse_pt_tls`). For NSO/NRO modules, TLS block size comes from `STT_TLS` symbols, alignment is assumed 16, and blocks start zeroed.
  - `TPIDRRO_EL0` stays the Horizon IPC thread-local region; only `TPIDR_EL0` points at the ELF TLS area.
  - Initialisers are not run by default because a program's own `crt0` normally runs them; the runner does not service SVCs and reports them instead.
  - Validated on synthetic test modules only (homebrew and synthetic workloads are the scope).
* **In Design: Milestone 3A GPU presentation** (2026-10-26)
  - Design doc: `docs/design/m3a-presentation.md`. Backend = `wgpu` + `winit` behind feature `gpu-present` (user-approved). Headless + `minifb` stay; default build unchanged.
  - Host presentation goes in new `src/gpu/present/`; `src/gpu/vulkan.rs` stays the guest-side model (do not rewrite it).
  - Plan: (1) refactor to `PresentBackend` trait + headless/minifb backends + `HostKey`, no behavior change; (2) wgpu backend; (3) CI feature job with lavapipe + docs. No code until the user approves the design PR.
* **Next**:
  - Implement M3A step 1 after design approval.
  - Exercise TLS and init with a multi-module homebrew build (libnx `crt0`, C++ `thread_local` / `std::thread`).
  - Build tooling note: building from source on Debian/Ubuntu needs `libasound2-dev libudev-dev`.
