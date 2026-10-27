# Multi-Agent Coordination & Engineering Standards

This document establishes operating rules for AI coding assistants (Antigravity, Claude Code, and peers) collaborating on `ounce-core`.

## 1. Non-Contradiction & Collaborative Building
* **Never undo or revert peer agents' work without mutual alignment**: Inspect git history, existing unit tests, and architecture stages before modifying code. Build incrementally upon verified code.
* **Strict Green CI Policy (Rule from the Owner)**: **Never merge to `main` until all CI checks are green.** Never use admin bypass under any circumstances. Every pull request must wait for all 9 cross-platform matrix jobs (macOS, Ubuntu, Windows across standard test, `gpu-present`, and `native-jit`) to succeed before merging.
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
    - Note (2026-10-26, libnx public source check): libnx `switch.ld` wraps `__tdata_lma`, `__tdata_lma_end`, `__tls_start`, `__tls_end` and `__tls_align` in `PROVIDE_HIDDEN`, and `switch.specs` links `-pie` without `--export-dynamic`, so real libnx binaries never put them in `.dynsym`. The fallback above stays (it serves synthetic modules and any toolchain that does export them) but never fires for libnx. No loader fix is needed: libnx copies `.tdata` itself (`newlibSetup` for the main thread, `threadCreate` for others) and reads its thread pointer from `ThreadVars.tls_tp` at `TPIDRRO_EL0 + 0x1F8`, not `TPIDR_EL0`. Do not add MOD0/layout heuristics to recover `.tdata`: `.main.tls` mixes `.tdata` and `.tbss` sizes and they cannot be split without the hidden symbols. What libnx does need from the runtime is a writable 0x200-byte TLS page at `TPIDRRO_EL0` per thread.
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
* **Completed: Harness stage gating** (branch `claude/project-thread-jvrok0`, 2026-10-26)
  - `run_vm` / `run_vm_internal` return a `VmOutcome { stop, x0, d0 }`; `VmStop` records why the guest stopped (`ExitProcess`, `Break`, `UnhandledSvc`, `Halt`, `Fault`, `Budget`, `WindowClosed`, `FrameLimit`). Non-harness callers ignore it.
  - `StageGate` in `src/main.rs`: a stage passes only if the guest stopped through `svcExitProcess` with the expected `X0`. Success lines ("Verified", "Handshake completed", ...) now print only on pass; `gate.finish()` exits 1 on any failure, so CI goes red.
  - Expected `X0` per stage: documented codes from the README table where one exists (01 `55`, 03 `42`, 12 `0x7`, 13 `0x124F801`, 17–25 `0x777`..`0xEEE`). Stages the README lists as "—" are pinned to their current deterministic exit value (02 `0`, 05 `0x3840`, 06 `0x1E61`, 07 `0xFF40C0FF`, 08 `0xFF00FFFF`, 09 `0x10`, 10 `0x606A00`, 11 `0`, 14 `0`, 15 `0x239`) as regression gates. Stage 04 also checks `D0 = 42.0`. Stage 16 (no guest exit) gates on the Tier-2 guest's Fib(10) = 55 in `X2` via `svc #7` and the 20-byte SaveData write. Stage 20 gates on the Tier-3 guest's `X0 = 0xAAA`.
  - If a stage's guest program is changed on purpose, update its expected value in the `gate.expect_exit` call next to `run_vm`.
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
* **Completed: MMU unaligned multi-page range mapping fix** (PR #20 on `main` @ 882b853, closes #14)
  - `Memory::map_range` now computes `num_pages` with intra-page offset `(base_addr as usize) & (PAGE_SIZE - 1)`, ensuring ranges that cross page boundaries allocate all intersecting pages.
  - Zero-size allocation guards and checked addition for 64-bit address overflow safety.
  - 8 synthetic unit tests covering intra-page boundaries, permission enforcement, and unmapped access.
* **Completed: Code-cache invalidation on guest code writes** (PR #24 on `main`, 2026-10-26)
  - Implements the self-modifying-code rule from the M4 design (section 5) for the safe tiers only; no JIT code.
  - `Memory` (`src/mmu/memory.rs`): pages carry `holds_code` + `code_write_gen`. A write (or `map_range` remap) to a code page bumps the global `Memory::code_write_gen()`. API: `mark_code_range(start, end)`, `code_written_since(start, end, gen)`. Writes to pages no cache has compiled from cost one bool check.
  - Tier-2 / Tier-3 engines mark pages they compile and run `sync_code_writes` before each lookup: if the generation moved, they drop only blocks/super-blocks whose pages were written (`blocks_invalidated` / `super_blocks_invalidated` counters). `get_or_compile_block` / `get_or_compile_super_block` now take `&mut Memory`.
  - `Tier2Block::execute` ends the block right after an interpreter-fallback store that hit cached code, so a block that patches its own later instructions sees the new code.
  - `SuperBlock::execute` returns to the engine when the PC leaves the fused path. Fixes a pre-existing bug: traces follow `BL` to the return address, but execution jumps to the callee, so the next fused block ran at the wrong PC (callee skipped).
  - Interpreter `JitBlockCache` was already safe: it re-reads the raw word every step and recompiles on mismatch. Test added to pin that.
  - Tests: Tier-2 guest patches a called function, Tier-2 same-block patch, data-page writes keep blocks, Tier-3 hot super-block patch, Tier-3 `BL` into callee, interpreter patch. 107 unit tests pass (default features); harness 25/25.
  - Not done: `IC IVAU`/`DC CVAU` are still decoded as no-ops (invalidation is write-driven, which is stricter). Tier-4 must call the same `code_written_since` check once it exists.
* **Completed: Milestone 4 / Host GPU accelerated 3D translation via wgpu** (PR #26 on `main` @ f6456db)
  - Tegra T239 NVN/Maxwell 3D pushbuffer translation (`METHOD_DRAW_ARRAYS`, `METHOD_DRAW_INDEXED`) into host GPU pipelines via `wgpu` (Metal on macOS, Vulkan on Linux/Windows).
  - Compact sparse index deduplication and bounded remapping in `src/gpu/nvgpu.rs`, guarding host memory against extreme 32-bit index spans (`0xFFFFFFFF`).
  - Primitive restart index filtering (`0xFFFF`, `0xFFFFFFFF`) discarding degenerate triangles.
  - `Wgpu3DRenderer` dynamic uniform buffer management with 256-byte aligned offsets (`has_dynamic_offset: true`), per-draw texture caching and isolated bind groups, and dynamic depth-test pipeline switching.
  - Integrated into `DisplayServer::present_frame_3d` in `--present=gpu` mode while preserving 100% CPU software rasterizer fallback for headless testing and `minifb`.
  - 14 adversarial unit tests added, bringing suite to 125 tests passed.
* **Verification & Quality Gates**:
  - `cargo test --release --features gpu-present,native-jit`: **133 passed in ounce-core, 5 passed in ounce-recompiler (138 total passed, 0 failed)**.
  - `cargo run --release --features gpu-present -- test`: **25/25 architecture stages passing (exit code 0xEEE)**.
  - Host GPU hardware swapchain verified on Metal (`--present=gpu 3d 20`).
  - Safe Rust provenance: `grep -rn "unsafe" src` returns **0 hits**, strictly preserving `#![forbid(unsafe_code)]`.
* **M4 step 2, Tier-4 IR + Cranelift backend** (Claude, branch `claude/m4-step2`, PR #28, 2026-10-27; draft until reviewed)
  - Files: `crates/ounce-recompiler/src/{ir,backend}.rs` (new), `exec.rs` (memory trampolines + block call, still the only `unsafe`), `lib.rs` (`Recompiler`, `GuestState`, `MemoryBus`); `src/cpu/tier4.rs` (new, `native-jit`), `src/cpu/engine.rs` (new: `CpuTier`, `CpuEngine`), `src/cpu/mod.rs`; `src/main.rs` only `--cpu=` parsing, usage line, and Stage 20 using `CpuEngine` (default Tier-3, output unchanged).
  - Works: `--cpu=tier4` (or `OUNCE_CPU=tier4`) with `--features native-jit` runs Stage 20 natively; 25/25 stages pass on both tiers. Details and numbers: `docs/design/m4-native-jit.md` section 12, step 2.
  - Rules for the next agent: the interpreter (`Cpu::step`) is the reference; Tier-4 lowering copies its semantics, quirks included. Change both together or neither. New IR ops need a backend lowering plus a crate test. Don't add `unsafe` outside `exec.rs`.
  - Random differential test now also checks Tier-3 (after #29).
  - Left: (3) differential fuzzer in CI, (5) software TLB, (6) direct block chaining and FP/SIMD; `run_vm` is still interpreter-only; code arena (one `ExecBuffer` per block today, page-sized); `SystemReg`/`TPIDR*` not translated. macOS/Windows run Tier-4 only through CI.
* **Active Agent Threads & File Ownership**:
  - **Claude Task 1 (`claude/m4-step2`)**: M4 Step 2 Tier-4 Native JIT recompiler. Reserved files: `crates/ounce-recompiler/src/*`, `src/cpu/tier4.rs`, `--cpu` flag in `src/main.rs`, differential tests.
  - **Claude Task 2 (`claude/devkita64-fixture`)**: devkitA64 homebrew fixture. Reserved files: `src/loader/*`, test fixtures.
  - **Antigravity (`fix/tier2-addsubreg-shift-correctness`)**: Closes Issue #25 (Tier-2 ADD/SUB shifted registers & signed overflow flags). Touches `src/cpu/tier2_jit.rs` only.
  - **Shared Boundary**: Antigravity avoids Claude's reserved paths (`crates/ounce-recompiler/src/*`, `src/cpu/tier4.rs`, `src/loader/*`, `src/main.rs`). Claude avoids `src/gpu/*`, `.github/workflows/*`, and `src/cpu/tier2_jit.rs`.
* **Next**:
  - Merge Issue #25 fix PR (`fix/tier2-addsubreg-shift-correctness`).
  - Coordinate review and CI verification as Claude pushes `claude/m4-step2` and `claude/devkita64-fixture`.
* **Handoff: real libnx homebrew NRO fixture** (branch `claude/devkita64-fixture`, 2026-10-27)
  - `tests/fixtures/nro/hello/`: homebrew-only test program built with devkitA64 + libnx (`main.c`, `Makefile`, checked-in `hello.nro`, README with rebuild steps). Reproducible: rebuilding gives the same SHA-1.
  - `src/loader/homebrew.rs`: `HomebrewConfig` writes the homebrew ABI `ConfigEntry` list (`X0` = list, `X1` = -1). `HomebrewEnvironment::setup` (the "ARGV" block used by `run`) is left as is; libnx `crt0` does not understand that block, so `run` should move to `HomebrewConfig` (main.rs is outside this branch).
  - `src/loader/nro.rs`: `DT_RELR` parsed (`DynamicInfo::relr`) and `NroLoader::apply_relr` added. Deliberately NOT applied by `load_into_memory`: adding the base twice is wrong, and libnx `crt0` applies `DT_RELR` itself. Call it only for images that will not self-relocate.
  - `src/loader/fixture_tests.rs`: header/MOD0/RELR tests on the real NRO pass. (Boot test now passes on `claude/cpu-libnx-insns`, see below.) `libnx_nro_runs_to_main_return` boots `crt0` with a test-local Horizon SVC shim (kernel `0x29` is not `svcGetInfo`) and is `#[ignore]`d: the interpreter faults on AdvSIMD `DUP` in `memset`. Static scan of the fixture's `.text` also finds AdvSIMD `CMEQ/UMINP/ADDP/AND/SHRN/LD1/MOV` and scalar `REV/CLZ/RBIT/UMULH/LDUR/STUR` undecoded. Next: implement those in `src/cpu`, then drop the `#[ignore]`.
* **Handoff: CPU instructions for libnx startup** (branch `claude/cpu-libnx-insns`, 2026-10-27; builds on `claude/devkita64-fixture`)
  - Files: `src/cpu/decoder.rs` (`decode_extra`, LDP/STP V-bit, DC ZVA), `src/cpu/instructions.rs`, `src/cpu/interpreter.rs`, new `src/cpu/simd.rs` (lane helpers), fixture + `src/loader/fixture_tests.rs`. Tier-2/3 untouched: they fall back to the interpreter for the new variants. Tier-4/`crates/ounce-recompiler` untouched.
  - Added: `RBIT/REV16/REV32/REV/CLZ/CLS`, `UMULH/SMULH`, `UMADDL/SMADDL/UMSUBL/SMSUBL`; SIMD&FP `LDUR/STUR` + pre/post-index + register offset (B/H/S/D/Q); `LDP/STP/LDNP/STNP` on S/D/Q; `LD1/ST1` 1-4 regs (+post-index); `DUP` (general), `UMOV`/`MOV`; three-same (`AND/BIC/ORR/ORN/EOR/BSL/BIT/BIF/ADD/SUB/CMEQ/CMTST/CMHI/CMHS/CMGT/CMGE/[US]MAX/[US]MIN/ADDP/[US]MAXP/[US]MINP`); two-misc (`CMEQ/CMGT/CMGE/CMLE/CMLT #0`, `CNT`, `NOT`, `ABS`, `NEG`); `SSHR/USHR/SHL/SHRN/SHRN2`.
  - Fixed two existing decode bugs found by the fixture: `STP/LDP Qt` were run as general-register pairs (V bit ignored; libnx `memset` wrote X0 instead of Q0), and `DC ZVA` was a no-op while `DCZID_EL0` read 0 (= ZVA allowed), so large `memset(0)` could leave memory unchanged. `DC ZVA` now zeroes 64 bytes and `DCZID_EL0` reports that size.
  - Result: `libnx_nro_runs_to_main_return` runs the real NRO through libnx `crt0`, `main` and `exit` back to the loader; the fixture reports `0xEEE` via `svcOutputDebugString` (libnx always hands the loader 0). Not ignored any more.
  - Not done: LD2/LD3/LD4, `MOVI`/modified-immediate, `INS`/`DUP` (element), `SMOV`, FP vector ops beyond `SimdVec4S`. Kernel SVC numbering (`0x29` etc.) and `run` still using `HomebrewEnvironment` are unchanged (outside this branch).
