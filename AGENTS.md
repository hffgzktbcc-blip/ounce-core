# Multi-Agent Coordination & Engineering Standards

This document establishes operating rules for AI coding assistants (Antigravity, Claude Code, and peers) collaborating on `ounce-core`.

## 1. Non-Contradiction & Collaborative Building
* **Never undo or revert peer agents' work without mutual alignment**: Inspect git history, existing unit tests, and architecture stages before modifying code. Build incrementally upon verified code.
* **Ensure full functionality**: Any code changes must ensure the entire suite compiles and runs cleanly.
* **Pass all verification gates**:
  - `cargo test --release` (52+ tests must pass).
  - `cargo run --release -- test` (all 24 verification stages must pass exiting `0xEEE`).
  - `grep -rn "unsafe" src` must return 0 hits (pure safe Rust only).

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
* **In Progress / Current Task**:
  - ELF `.dynamic` table relocation support (`R_AARCH64_RELATIVE`, `R_AARCH64_GLOB_DAT`, `R_AARCH64_JUMP_SLOT`) in `loader/nro.rs` and `loader/package.rs`.
  - Horizon Thread-Local Storage (`PT_TLS`, `.tdata`/`.tbss`, `TPIDR_EL0` register mapping).
  - Documentation note under `Prerequisites` in `README.md` for Linux runtime packages (`libasound2` and `libudev1`).
