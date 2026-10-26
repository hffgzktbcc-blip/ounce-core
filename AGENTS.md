# Multi-Agent Coordination & Engineering Standards

This document establishes operating rules for AI coding assistants (Antigravity, Claude Code, and peers) collaborating on `ounce-core`.

## 1. Non-Contradiction & Collaborative Building
* **Never undo or revert peer agents' work without mutual alignment**: Inspect git history, existing unit tests, and architecture stages before modifying code. Build incrementally upon verified code.
* **Ensure full functionality**: Any code changes must ensure the entire suite compiles and runs cleanly.
* **Pass all verification gates**:
  - `cargo test --release` (85+ tests must pass).
  - `cargo run --release -- test` (all 25 verification stages must pass exiting `0xEEE`).
  - No `unsafe` code in `src/`. `src/main.rs` carries `#![forbid(unsafe_code)]`, so the compiler enforces it; the grep gate is now `grep -rnE "unsafe[ ({]" src` (0 hits), because a plain `grep unsafe` matches the attribute itself.

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
* **Stage 25 Ampere tensor core HMMA** (PR #9, branch `claude/project-thread-jvrok0`, 2026-10-26)
  - `src/gpu/tensor.rs` (new): IEEE 754-2008 binary16 conversion with round-to-nearest-even, the two FP16 MMA shapes (`m16n8k8`, `m16n8k16`), the per-lane warp fragment layouts, `mma_f16_f32`, `WarpFragments` distribute/execute/collect, and a `TensorCoreUnit` issue counter.
  - `src/gpu/shader.rs`: `SassOp::Hmma1688` / `SassOp::Hmma16816` with encode, decode, `hmma_shape()`, GLSL and SPIR-V emission.
  - `src/main.rs`: Stage 25 decodes both HMMA ops, distributes documented binary16 matrices across 32 warp lanes, executes both shapes plus an FP32-accumulate case, writes the FP32 results into guest memory, and runs a guest AArch64 program that compares all three against their IEEE-754 bit patterns (`0xEEE` on match, `0xBAD` on mismatch). Harness is now 25 stages; README and the `info` banner updated.
  - **Sourcing (important, do not overstate)**: no public NVIDIA specification documents the real `HMMA` SASS encoding. The *semantics* come from the public PTX ISA (`mma.sync.aligned.m16n8k8.row.col.f32.f16.f16.f32` and `...m16n8k16...`) and the per-lane fragment layouts documented with them (`groupID = laneid >> 2`, `threadID_in_group = laneid % 4`), plus IEEE 754-2008 binary16. The opcode bytes `0x0E` / `0x0F` are this repository's own synthetic 64-bit SASS test encoding, the same scheme as `SassOp` `0x01`..`0x0D`, and are not Ampere instruction words. Do not relabel any of this as a hardware-accurate decoder.
  - Verified on this branch: `cargo build --release` no warnings, `cargo test --release` 85 passed (rebased on main a95c9d4), `cargo run --release -- test` 25 stages `0xEEE`, `grep -rn unsafe src` 0 hits. Negative control checked by hand: corrupting one published result makes the Stage 25 guest exit `0xBAD`.
  - Limits: products are exact in `f32` and accumulation runs in increasing `k` order, so bit-exactness against silicon is not claimed for long reductions; only `.row.col` with `f32` accumulate is modelled (no `f16` accumulate, sparsity, `bf16`/`tf32`/integer shapes); GLSL and SPIR-V cannot express a cross-lane reduction without a cooperative-matrix extension, so they carry the lane-local partial plus a comment, and the real MMA runs in `gpu::tensor`; the stage's closing "Verified" line is unconditional, like every other stage, because `run_vm` returns `()` — the guest exit code in the VM summary above it is the actual assertion.
  - Touches `src/gpu/` alongside the M3A presentation thread: new work is confined to `tensor.rs` plus additive `SassOp` variants, so nothing in `src/gpu/vulkan.rs`, `nvn.rs` or the planned `src/gpu/present/` layer is changed.
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
  - Step 2 done (branch `claude/m3a-wgpu-backend`, stacked on step 1): `src/gpu/present/wgpu_window.rs` behind feature `gpu-present` (wgpu 30, winit 0.30 with `pump_app_events`, pollster 0.4). `FrameBlitter` uploads the RGBA8 frame and draws a fullscreen triangle, letterboxed (`viewport_rect`). `--present=auto|gpu|minifb|headless` flag (wins over `OUNCE_PRESENT`); `auto` tries GPU first when compiled in. Adapter override through wgpu's own `WGPU_BACKEND` / `WGPU_ADAPTER_NAME`; `OUNCE_VSYNC=0` disables vsync. winit init panics (missing `libxkbcommon-x11`) are caught and fall back to minifb.
  - Verified locally: offscreen pixel-exact blit test on lavapipe (`mesa-vulkan-drivers`), and `gfx 60 --present=gpu` under Xvfb presenting through Vulkan (llvmpipe). Not yet run on real macOS/Metal or Windows hardware.
  - Step 3 done (branch `claude/m3a-ci-docs`, stacked on step 2): `ci.yml` job `gpu-present` builds and tests `--features gpu-present` on Linux/macOS/Windows (Linux installs `mesa-vulkan-drivers` so the offscreen blit test runs on lavapipe); README documents the backend, flags and runtime packages.
  - Merge order: #5 (design) and #6 (step 1) into `main`, then retarget #8 (step 2) and the step 3 PR to `main` one after the other. Stacked PRs get no CI until retargeted.
* **Next**:
  - Manual check of `--present=gpu` on a Mac (Metal) and a Windows machine; nobody has run it on real hardware yet. (CI's offscreen blit test does pass on the macOS runner's Metal device.)
  - Later milestone (not M3A): translate NVN draws into host GPU work. Needs its own design.
  - M4 (Tier-4 native JIT in `crates/ounce-recompiler`) is on hold until the user asks for it.
  - Exercise TLS and init with a multi-module homebrew build (libnx `crt0`, C++ `thread_local` / `std::thread`).
  - Build tooling note: building from source on Debian/Ubuntu needs `libasound2-dev libudev-dev`.
