# Multi-Agent Coordination & Engineering Standards

This document establishes operating rules for AI coding assistants (Antigravity, Claude Code, and peers) collaborating on `ounce-core`.

## 1. Non-Contradiction & Collaborative Building
* **Never undo or revert peer agents' work without mutual alignment**: Inspect git history, existing unit tests, and architecture stages before modifying code. Build incrementally upon verified code.
* **Ensure full functionality**: Any code changes must ensure the entire suite compiles and runs cleanly.
* **Pass all verification gates**:
  - `cargo test --release` (88+ tests must pass).
  - `cargo run --release -- test` (all 25 verification stages must pass exiting `0xEEE`).
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
* **Completed: ELF dynamic linking and TLS** (PR #3 + PR #4 on `main` @ 629cd87, updated 2026-10-26)
  - PR #3 (`claude/elf-dynamic-linking`): MOD0 `.dynamic` parsing; `R_AARCH64_RELATIVE`, `ABS64`, `GLOB_DAT`, `JUMP_SLOT`; cross-module imports/exports in loose packages; unresolved imports reported in `LooseBootInfo`; README Linux runtime note.
  - PR #4 (`claude/project-thread-1yu255`): `DT_REL`/`DT_RELSZ`/`DT_RELENT`/`DT_PLTREL`; `DT_GNU_HASH`; TLS relocations (`TPREL64`, `DTPMOD64`, `DTPREL64`, `TLSDESC`) on a static AArch64 variant I layout; `PT_TLS` parsing with `.tdata` copy, `.tbss` zero-fill and `p_align`; per-thread TLS area and `TPIDR_EL0` for every `svcCreateThread` thread; init/fini runner (`src/loader/init.rs`, opt-in via `OUNCE_RUN_INIT=1`); README Fedora/Arch package names.
  - End-to-end homebrew multi-module and TLS lifecycle verified:
    - Fixed `LoosePackageLoader` template discovery to locate companion `.elf` files (`main.elf`, `<name>.elf`) and parse `PT_TLS` headers into the static TLS layout.
    - Added devkitA64 / libnx linker script fallback for bare NSO binaries via `__tdata_lma`/`__tdata_lma_end` symbols so non-zero `.tdata` initial values are preserved.
    - Added `THREAD_RETURN_SENTINEL` (0xFFFF_0000_0000_E717) in `ThreadScheduler`: initialized in `X30` (`LR`) on `create_thread` to gracefully catch thread completion when entry points terminate via standard `RET`.
    - Added integration tests `test_homebrew_multimodule_tls_and_thread_lifecycle` and `test_homebrew_devkita64_linker_script_tls_fallback` covering `crt0`, non-zero `thread_local`, worker threads, cross-module calls from `subsdk0` to `main`, and thread memory isolation.
* **Completed: Stage 25 Ampere tensor core HMMA** (PR #9 on `main` @ ce883d6, 2026-10-26)
  - `src/gpu/tensor.rs`: IEEE 754-2008 binary16 conversion with round-to-nearest-even, the two FP16 MMA shapes (`m16n8k8`, `m16n8k16`), the per-lane warp fragment layouts, `mma_f16_f32`, `WarpFragments` distribute/execute/collect, and a `TensorCoreUnit` issue counter.
  - `src/gpu/shader.rs`: `SassOp::Hmma1688` / `SassOp::Hmma16816` with encode, decode, `hmma_shape()`, GLSL and SPIR-V emission.
  - `src/main.rs`: Stage 25 decodes both HMMA ops, distributes documented binary16 matrices across 32 warp lanes, executes both shapes plus an FP32-accumulate case, writes the FP32 results into guest memory, and runs a guest AArch64 program that compares all three against their IEEE-754 bit patterns (`0xEEE` on match, `0xBAD` on mismatch). Harness is now 25 stages.
* **Completed: Milestone 3A GPU presentation** (PR #5, #6, #8, #10 on `main` @ fcd85fa, 2026-10-26)
  - Design doc: `docs/design/m3a-presentation.md`. Backend = `wgpu` + `winit` behind feature `gpu-present` (Vulkan on Linux/Windows, native Metal on macOS). Headless + `minifb` stay; default build unchanged.
  - `src/gpu/present/`: `PresentBackend` trait implemented for `HeadlessBackend`, `MinifbBackend`, and `WgpuPresentBackend` with letterboxing and vsync configuration.
  - CI: `gpu-present` job runs on Linux/macOS/Windows with Mesa Lavapipe on Linux.
* **Deliberate deviations and limits (do not "fix" by guessing)**:
  - `DT_INIT_ARRAYSZ` is 27 per the ELF spec (26 is `DT_FINI_ARRAY`).
  - NSO/NRO images carry no program headers, so `PT_TLS` is only available for ELF input (`NroLoader::parse_pt_tls`) or resolved via `__tdata_lma` devkitA64 linker script fallback symbols.
  - `TPIDRRO_EL0` stays the Horizon IPC thread-local region; only `TPIDR_EL0` points at the ELF TLS area.
  - Initialisers are not run by default because a program's own `crt0` normally runs them; the runner does not service SVCs and reports them instead.
  - Validated on synthetic test modules only (homebrew and synthetic workloads are the scope).
* **Completed: Milestone 3B controller input mappings & Metal verification** (2026-10-26)
  - PR #11, #12, #13 merged to `main`.
  - Verified `--present=gpu` on real Apple Silicon (Apple M3 Metal integrated GPU) executing both `3d` and `play` benchmark modes.
  - Added physical gamepad analog stick threshold synthesis to D-pad directional buttons in `HostGamepadManager::poll` so physical thumbsticks seamlessly drive games reading `NpadButton` bitmasks.
  - Added `HostGamepadManager::active_gamepad_name` and attached gamepad detection logging to interactive VM runner.
* **Completed: Milestone 3B live audio streaming & ring-buffer pacing** (2026-10-26)
  - Continuous non-blocking audio streaming from `audout:u` and `audren:u` to `HostAudioSink` via sample queue with capacity management (`DEFAULT_MAX_CAPACITY = 192_000` samples / 2 seconds) and graceful eviction to preserve low latency.
  - Real-time latency tracking (`queue_latency_ms`), atomic underrun tracking, played/dropped sample counters, and snapshot metrics in `HostAudioSink`.
  - Added waveform synthesizer (`Sine`, `Square`, `Sawtooth`, `Triangle`) in `audout` producing 48 kHz stereo 16-bit PCM for diagnostic testing and interactive demos.
  - Multi-voice polyphonic mixing in `audren` DSP renderer with fractional sample rate conversion (interpolated resampling to 48 kHz stereo) and per-voice pitch/gain scaling.
* **Completed: Milestone 3C GPU indexed rendering & NVN elements** (2026-10-26)
  - Added `METHOD_SET_INDEX_ARRAY_ADDR` (`0x05F2`) and `METHOD_DRAW_INDEXED` (`0x05F4`) to Tegra T239 `NvgpuChannel` pushbuffer command execution.
  - Added 16-bit and 32-bit index buffer reading with vertex fetching and barycentric triangle rasterization in `rasterize_indexed_triangles`.
  - Added `NVN_CMD_BIND_INDEX_BUFFER` (`0x0D`) and `NVN_CMD_DRAW_ELEMENTS` (`0x0E`) opcodes to `NvnCommandBuffer`, stream encoder/decoder, and GPFIFO compiler.
* **Completed: Milestone 5 Horizon OS filesystem & time/settings services** (2026-10-26)
  - `VirtualFileSystem`: directory iteration & enumeration (`list_directory`) returning sorted directory entries across in-memory and disk hierarchies (`sdmc:/`, `save:/`).
  - Added recursive directory creation, file deletion, and directory deletion with free/total virtual disk space queries.
  - `fsp-srv` CMIF IPC handling for `IFileSystem` (`CreateFile`, `DeleteFile`, `CreateDirectory`, `DeleteDirectory`, `OpenFile`, `OpenDirectory`, `GetFreeSpaceSize`, `GetTotalSpaceSize`) and `IDirectory` (`ReadDirectory`, `GetEntryCount`).
  - `time:u` / `time:s` high-resolution monotonic clock matching host `Instant` with nanosecond precision, and UTC epoch timestamp calculation via `ISystemClock::GetCurrentTime`.
  - `set:sys` queries for system language, console model (`0x02` Drake / Tegra T239), and device nickname (`Ounce-T239`).
* **Verification & Quality Gates**:
  - `cargo test --release --features gpu-present`: **103 passed, 0 failed**.
  - `cargo run --release --features gpu-present -- test`: **25/25 architecture stages passing (exit code 0xEEE)**.
  - Safe Rust provenance: `grep -rn "unsafe" src` returns **0 hits**, strictly preserving `#![forbid(unsafe_code)]`.
* **Next**:
  - Later milestone: translate NVN draws into host GPU work.
  - M4 (Tier-4 native JIT in `crates/ounce-recompiler`) is on hold until the user asks for it.
  - Build tooling note: building from source on Debian/Ubuntu needs `libasound2-dev libudev-dev`.
