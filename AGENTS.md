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
  - **Claude (Track B: Services)**: Homebrew service plumbing (`sm:`, `hid`, `fsp-srv` RomFS, `nvdrv`). Reserved files: `src/kernel/ipc.rs`, `src/kernel/sm.rs`, `src/kernel/hid.rs`, `src/kernel/nvdrv.rs`, `src/fs/romfs.rs`, test fixtures.
  - **Antigravity (Track A: Tier-4 Recompiler Enhancements)**: Tier-4 direct block chaining, Software TLB (Option B), and differential fuzzer in CI. Branch `feat/tier4-chaining-tlb-fuzzer`. Reserved files: `crates/ounce-recompiler/*`, `src/cpu/tier4.rs`, `.github/workflows/*`.
  - **Shared Boundary**: Antigravity avoids Claude's reserved paths (`src/kernel/*`, `src/fs/romfs.rs`, test fixtures). Claude avoids `crates/ounce-recompiler/*`, `src/cpu/tier4.rs`, `.github/workflows/*`. Neither touches `src/main.rs` without coordination.
* **Handoff: Tier-4 Software TLB, Fast Block Chaining & CI Fuzzer** (branch `feat/tier4-chaining-tlb-fuzzer`, 2026-10-27)
  - `crates/ounce-recompiler/src/exec.rs`: 512-entry direct-mapped `TlbEntry` software TLB (Option B). `mem_read` and `mem_write` trampolines fast-path cached page lookups in host memory without `catch_unwind` or HashMap overhead; writes to pages holding code bypass the fast path to preserve self-modifying code invalidation.
  - `crates/ounce-recompiler/src/lib.rs`: `PageMapping` metadata on `MemoryBus::query_page`; `Recompiler` owns a persistent software TLB across block runs with `flush_tlb()`.
  - `src/cpu/tier4.rs`: `MmuBus` implements `query_page` using `Memory::get_page`. `Tier4JitEngine` adds a 1024-entry `fast_cache` for 1-cycle block dispatch, direct block chaining counter (`chained_jumps`), and TLB flushing on `sync_code_writes`.
  - Tests: expanded differential fuzzer to 10,000 cases with boundary pages (`DATA + 0xFF8..0x1008`) and unmapped faults; added `fibonacci_loop_block_chaining` test verifying direct loop chaining. 163 workspace tests pass.
  - CI: added Tier-4 unit test & fuzzer step (`cargo test --release --features native-jit`) to `.github/workflows/ci.yml` across Ubuntu, macOS, and Windows.
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
* **Handoff: `run hello.nro` from the CLI** (branch `claude/libnx-cli-run-svc`, 2026-10-27)
  - `cargo run --release -- run tests/fixtures/nro/hello/hello.nro fixture` boots the libnx fixture and prints `ounce-fixture result=0x00000EEE`. Extra non-flag args after the path become homebrew `argv[1..]`.
  - `src/kernel/svc.rs`: Horizon numbers now mean the Horizon call: `0x1E` GetSystemTick (X0), `0x29` GetInfo (`HorizonKernel::info`, unknown types return `0xF001`), `0x02/0x03` Set{MemoryPermission,MemoryAttribute}, `0x0F` SetThreadCoreMask, `0x28` ReturnFromException (stops like Break), `0x2A/0x2B` cache flushes, `0x2C/0x2D` Map/UnmapPhysicalMemory. `svcQueryMemory` reports unmapped pages as Free. Region layout from GetInfo no longer overlaps (ASLR 0x0800_0000..0x80_0000_0000 holds code, heap, alias and the stack region).
  - ounce-core's own SVCs moved to `kernel::svc::ext` (`0x100` PRESENT_FRAME, `0x101` SUBMIT_GPU_PUSHBUFFER, `0x102` READ_ROMFS_FILE, `0x103` APPEND_AUDIO_BUFFER, `0x104` WRITE_SAVE_DATA_FILE, `0x105` NVN_SUBMIT_COMMAND_BUFFER, `0x106` NNSDK_DISPATCH; `ext::svc(n)` encodes `svc #n`). Harness guest programs updated; the two that used `0x1E` as GetInfo now use `0x29`. All 27 harness checks and every stage exit code unchanged; `demo-dir`, `package`, `play`, `3d`, `gfx` output identical to `main`.
  - `src/main.rs` `run`: NROs get `setup_homebrew_nro` (32 MB `OverrideHeap`, `ConfigEntry` list at 0x7FFF_0000, `X1` = -1, `X30` = `svcExitProcess` stub at 0x7FFE_0000) and a 50M instruction budget. The old `HomebrewEnvironment::setup(.., 0x7FFF_FE00, ..)` mapped two pages and zeroed the NRO's first page at 0x8000_0000, which is why `run` on any NRO died at PC 0x8000_0000.
  - Tests: `libnx_nro_runs_under_cli_setup_and_kernel` (CLI setup + real kernel dispatch), `horizon_numbers_and_research_range`, `exit_stub_calls_exit_process`.
  - Left: `0x18` is still treated as PresentFrame rather than svcWaitSynchronization (harness/display rely on it). Services (`sm`, `applet`, `hid`, `fs`) and threads are untested with libnx; the fixture skips them.
* **Handoff: libnx service handshakes, step 1 (sm + hid)** (branch `claude/libnx-services`, 2026-10-27)
  - `src/kernel/hipc.rs`: HIPC request parser (special header, PID, copy/move handles, X/A/B/W descriptors, receive list, CMIF at the 16-byte aligned data start) and response writer (special-header bit in word 1, copy and move handle lists). `IpcManager::write_cmif_response` now uses it, and `IpcManager::request` holds the parsed request for handlers. Zero-header requests still find CMIF at TLS + 0x10, so the harness programs are unaffected.
  - `src/kernel/ipc.rs`: `sm:` RegisterClient records the PID; `hid` IAppletResource `GetSharedMemoryHandle` returns a copy handle to a new `ServiceType::SharedMemory { size: 0x40000 }`; control requests (QueryPointerBufferSize, Clone) answered; ConvertCurrentObjectToDomain returns an error and logs (domains are next: libnx `fsInitialize` needs them).
  - `src/kernel/svc.rs`: `svcQueryMemory` returns the whole mapped or free run around the address (`Memory::region_at`), so libnx `virtmem` can place mappings (it gave up after 512 one-page answers before). `0x14` svcUnmapSharedMemory added (`Memory::unmap_range`).
  - `src/main.rs`: main thread handle is `0x1000`, not the `0xFFFF8000` pseudo-handle: libnx mutexes use bit 30 of the owner handle as their waiters flag, so `virtmemFindAslr` saw its own lock as not held and returned NULL.
  - Fixture `tests/fixtures/nro/services/` (smInitialize + hidInitialize + read shared memory + exits) prints `result=0x00000EEE` under `ounce-core run` and in `libnx_services_fixture_opens_sm_and_hid`.
  - Next: CMIF domains + `fsp-srv` (OpenSdCardFileSystem / OpenDataStorageByCurrentProcess, RomFS reads through B buffers), then `nvdrv`. HID input is not written into the guest-mapped shared memory yet (HidServer still uses its fixed address).
* **Claim: Track B, homebrew service plumbing (Claude)** (2026-10-27, split agreed with the user)
  - Reserved: `src/kernel/ipc.rs`, `src/kernel/sm.rs`, `src/kernel/hid.rs`, `src/kernel/nvdrv.rs`, `src/fs/romfs.rs`, service test fixtures (`tests/fixtures/nro/*`, `src/loader/fixture_tests.rs`). Track A (Antigravity) owns `crates/ounce-recompiler/*`, `src/cpu/tier4.rs`, `.github/workflows/*`; Claude does not touch those. `src/main.rs` changes are flagged to the user first.
  - Files outside that list changed by the Track B PRs opened before the split, for reviewers: #34 `src/kernel/hipc.rs` (new), `src/kernel/svc.rs` (QueryMemory regions, UnmapSharedMemory), `src/kernel/mod.rs`, `src/mmu/memory.rs` (additive `region_at`, `unmap_range`), `src/main.rs` (homebrew main thread handle constant only). #35 adds `src/cpu/{decoder,instructions,interpreter,simd}.rs` (new AdvSIMD ops; not Tier-4), `src/main.rs` (stage 17 uses fsp-srv cmd 200; `setup_homebrew_nro` puts the NRO on the virtual SD card).
* **Handoff: libnx service handshakes, step 2 (CMIF domains + fsp-srv RomFS)** (branch `claude/libnx-fsp-srv`, stacked on `claude/libnx-services` / #34, 2026-10-27)
  - Domains (`src/kernel/ipc.rs`, `src/kernel/hipc.rs`): `ConvertCurrentObjectToDomain` marks the session (`IpcManager::domains`) and returns its own handle as object ID 1st object; domain requests are routed by the domain header's object ID (IDs are entries in `handles`), Close removes the object; replies on a domain carry a domain out header and out object IDs after the data (`hipc::Response::domain_objects`). `IpcManager::reply` turns a handler's new session into a move handle (plain) or an out object (domain) and drops the legacy echo word on domains.
  - fsp-srv: `1` SetCurrentProcess now returns nothing (it used to return an IStorage; the harness's stage 17 and the unit test now use `200` OpenDataStorageByCurrentProcess). IFileSystem renumbered to Horizon (`8` OpenFile, `9` OpenDirectory, `10` Commit, `11` GetFreeSpaceSize, `12` GetTotalSpaceSize); paths come from the X buffer (`path_arg`). IFile `0` Read (offset/size, data into the B buffer). `VirtualFileSystem::handle_path` added.
  - `run`: the NRO is placed on the virtual SD card and `argv[0]` is `sdmc:/<file name>` (libnx `romfsInit` reopens it from there).
  - CPU: AdvSIMD modified immediate (`MOVI/MVNI/ORR/BIC/FMOV` vector), `DUP`/`INS` (element) and `INS` (general), `LD1/ST1` single structure + `LD1R`, `ZIP/UZP/TRN`, `LDR/STR` B/H unsigned offset.
  - Fixture `tests/fixtures/nro/romfs/` (sm, fs, sdmc, romfsInit, read `romfs:/greeting.txt`) -> `0x00000EEE`.
  - Next: `nvdrv` (open, ioctls, `nvmap`), HID input into the guest-mapped shared memory, IStorage Read for NSO-style `OpenDataStorageByCurrentProcess`.
* **Handoff: libnx service handshakes, step 3 (nvdrv + nvmap)** (branch `claude/libnx-nvdrv`, stacked on #35, 2026-10-27)
  - New `src/kernel/nvdrv.rs` (Track B reserved): device files by path, nvmap objects (`CREATE`, `FROM_ID`, `ALLOC`, `FREE`, `PARAM`, `GET_ID`, Linux Tegra nvmap UAPI layouts); other ioctls return `NV_NOT_IMPLEMENTED` and are recorded in `NvDrv::unhandled`.
  - `src/kernel/ipc.rs`: libnx-shaped nvdrv requests (data in X/A, out through C/B, plus Close/QueryEvent/SetClientPID) go to `IpcManager::handle_nvdrv`; the harness's inline-argument nvdrv form keeps the older handler. `ServiceType::TransferMemory` added.
  - Outside the reserved list (flagged): `src/kernel/svc.rs` `0x15` svcCreateTransferMemory; `src/kernel/mod.rs` registers the module.
  - Fixture `tests/fixtures/nro/nvdrv/` (nvInitialize, nvMapInit, nvMapCreate, close/exit) -> `0x00000EEE`.
  - Next: nvhost-ctrl / nvhost-as-gpu / nvhost-gpu ioctls for real GPU channels, QueryEvent events, HID input into the guest-mapped shared memory.
* **Autonomous Parallel Execution Agreement (Owner Approved, 2026-10-27)**:
  - PR #34, #35, #36 (Track B) and PR #37 (Track A) are all merged to `main` at `c6d37ef`.
  - Both agents operate autonomously in their dedicated lanes without requiring conversational round-trips for permission.
  - **Merge Protocol**: Each agent opens draft/ready PRs for their lane. Once all 9 cross-platform CI matrix jobs are 100% green and no files collide outside reserved lists, the author agent is authorized to squash-merge to `main` without admin bypass.
  - **Track B Lane (Claude)**:
    - Next Goal: Homebrew threads (`svcCreateThread`, thread-local storage, and libnx mutex arbitration via Horizon synchronization primitives) followed by audio mixing (`audren:u` 64-voice mixer).
    - Reserved files: `src/kernel/ipc.rs`, `src/kernel/sm.rs`, `src/kernel/hid.rs`, `src/kernel/nvdrv.rs`, `src/kernel/scheduler.rs`, `src/kernel/sync.rs`, `src/fs/romfs.rs`, `tests/fixtures/nro/*`, `src/loader/fixture_tests.rs`.
  - **Track A Lane (Antigravity)**:
    - Next Goal: Benchmarking and optimizing Tier-4 MIPS throughput on long workloads, Cranelift register optimization, and wiring `run --gui` interactive window presentation with live swapchain pacing.
    - Reserved files: `crates/ounce-recompiler/*`, `src/cpu/tier4.rs`, `src/gpu/present/*`, `.github/workflows/*`.
  - **Shared Files**: Avoid touching `src/main.rs` unless strictly necessary for minimal CLI flags, with changes kept completely disjoint.
* **Handoff: Track B step 4, GPU channels + QueryEvent** (branch `claude/libnx-gpu-channels`, 2026-10-27)
  - `src/kernel/nvdrv.rs`: `/dev/nvhost-ctrl` (syncpoint read/incr/wait, event wait/signal/register), `/dev/nvhost-ctrl-gpu` (GM20B characteristics as libnx documents them, zcull, TPC masks, ZBC slot mask, GPU time), `/dev/nvhost-as-gpu` (alloc/free space, MAP_BUFFER_EX, unmap, VA regions), `/dev/nvhost-gpu` (GPFIFO alloc, object contexts, settings, SUBMIT_GPFIFO with inline or Ioctl2 entries). Submitted GPU VAs are translated through the address space to guest addresses and run on `NvgpuChannel::process_pushbuffer` from `IpcManager::handle_nvdrv`. Work completes on submit: each submit advances the channel syncpoint and returns it as the fence.
  - `src/kernel/ipc.rs`: nvdrv `Ioctl2` (11) / `Ioctl3` (12) buffers; `QueryEvent` returns a copy handle to `ServiceType::NvEvent` (waiting on it is not modelled: `svcWaitSynchronization` 0x18 still doubles as frame present).
  - Outside Track B: `src/cpu/interpreter.rs` fix: extended-register ADD/SUB (not ADDS/SUBS) writes SP when Rd = 31 (`sub sp, sp, x12`, GCC frames over 4 KiB). The same one-line fix is in `src/cpu/tier4.rs` AddSubReg (a Track A file; see PR #39 for the discussion), because the Tier-4 differential fuzzer compares Tier-4 against the interpreter.
  - Fixture `tests/fixtures/nro/gpu/` -> `0x00000EEE`; test checks the GPU model processed the command and no ioctl went unhandled.
* **Handoff: Track B step 5, controller input** (branch `claude/libnx-hid-input`, stacked on `claude/libnx-gpu-channels`, 2026-10-27)
  - New `src/kernel/hid.rs` (Track B reserved): HID shared memory in libnx's `HidSharedMemory` layout (offsets from libnx `services/hid.h`). When the guest maps the block it got from `GetSharedMemoryHandle`, player 1 (full-key style) and Handheld are described as connected; every SVC appends the current `NpadInput` (buttons, sticks) to both npad LIFOs with the next sampling number.
  - `src/kernel/ipc.rs`: `IpcManager::hid`, `shared_memory_mapped` / `shared_memory_unmapped`.
  - Outside Track B (flagged): `src/kernel/svc.rs` (map/unmap hooks, one `hid.sync` per SVC), `src/main.rs` (`run_vm_internal`: on ordinary SVCs, every 4 ms while the HID block is mapped, poll the window keyboard and `HostGamepadManager` into `kernel.ipc.hid.input`; homebrew does not present frames, so the PresentFrame poll never runs for it).
  - Fixture `tests/fixtures/nro/input/` + `libnx_input_fixture_reads_host_controller` (held buttons and stick read back through `padUpdate`).
  - Not tested: live input with a physical gamepad or keyboard under `run --gui` (needs a person at the controls). Keyboard input under `--gui` also depends on the window pumping events, which only happens when frames are presented.
* **Handoff: Tier-4 AdvSIMD Vector & Floating-Point Lowering** (branch `feat/tier4-simd-fp-lowering`, 2026-10-27)
  - `crates/ounce-recompiler/src/lib.rs`: Added `pub v: [u128; 32]` to `GuestState`.
  - `crates/ounce-recompiler/src/ir.rs`: Added `Reg::VLo(u8)` and `Reg::VHi(u8)` (64-bit halves of 128-bit vector registers); added `FpWidth` (`F32`, `F64`), `FpBinOp` (`Fadd`, `Fsub`, `Fmul`, `Fdiv`, `Fmax`, `Fmin`), `Op::FpBin`, and `Op::FpCompare`.
  - `crates/ounce-recompiler/src/backend.rs`: Lowered `Reg::VLo`/`VHi` register loads/stores at exact struct offsets; lowered `Op::FpBin` and `Op::FpCompare` via Cranelift `fadd/fsub/fmul/fdiv/fmax/fmin` and `fcmp` with little-endian bitcast flags and branchless AArch64 NZCV condition generation (`0x3000_0000`, `0x6000_0000`, `0x8000_0000`, `0x2000_0000`).
  - `src/cpu/tier4.rs`: Lowered `FpMovGen`, `FpLoadStoreImm` (S/D), `SimdLoadStoreQ` (128-bit Q), `FpData2`, `FpCompare`, `SimdModImm` (`MOVI`/`MVNI`), `SimdDupGeneral`, `SimdMovToGeneral` (`UMOV`), `SimdThreeSame` (`AND`, `ORR`, `EOR`, `BIC`, `ORN`), and `SimdTwoMisc` (`NOT`). Wired full `v: [u128; 32]` preservation across native block runs.
  - Tests: Added `fp_and_simd_ops_match_all_tiers` test and expanded 10,000-case randomized differential fuzzer in CI to generate FP & SIMD opcode classes and verify all 32 128-bit vector registers match bit-for-bit between interpreter, Tier-3, and Tier-4. 190 workspace tests pass.
* **Claim: Track B, homebrew threads** (branch `claude/libnx-threads`, 2026-10-27, in progress)
  - Files: `src/kernel/scheduler.rs`, `src/kernel/sync.rs`, `src/kernel/svc.rs` (thread/sync SVCs 0x08-0x0B, 0x11, 0x16, 0x18 with handles, 0x1A-0x1D, 0x25), `tests/fixtures/nro/threads/`, `src/loader/fixture_tests.rs`.
  - Plan: cooperative scheduler that switches guest threads inside `dispatch_svc` by swapping `cpu.regs` (no `main.rs` change). Each thread gets its own zeroed 0x200-byte page at `TPIDRRO_EL0` (libnx keeps `ThreadVars` and copies `.tdata` itself). `svcStartThread` runs the new thread first; `svcArbitrateLock`/`svcWaitProcessWideKeyAtomic`/`svcWaitSynchronization` block and switch; unlock/signal/exit hand over and wake waiters. `svcWaitSynchronization` (0x18) waits for real only when given 1-64 known thread/event handles; otherwise it keeps acting as frame present for the existing demos.
