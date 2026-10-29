<p align="center">
  <img src="assets/banner.svg" alt="ounce-core: Hardware Research Runtime for NVIDIA Tegra T239-class ARMv8.5-A systems" width="100%" />
</p>

<p align="center">
  <a href="https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml"><img src="https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml/badge.svg" alt="Build Status" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/language-Rust%202024-orange.svg" alt="Language" /></a>
  <a href="#legal-notice"><img src="https://img.shields.io/badge/distribution-no%20keys%20%7C%20no%20firmware%20%7C%20no%20game%20data-success.svg" alt="Distribution" /></a>
  <a href="#implementation-status"><img src="https://img.shields.io/badge/status-experimental-yellow.svg" alt="Status" /></a>
</p>

---

## Abstract

`ounce-core` is an experimental AArch64 execution and high-level service research runtime written in Rust. It contains an interpreter, Tier-1 through Tier-3 execution engines, and an optional Tier-4 native recompiler, alongside a paged memory model, a subset of Horizon-style supervisor calls (SVCs) and services, and software models of audio, input, and graphics. The default build renders through a CPU rasterizer; `gpu-present` can replay selected draw output through `wgpu`. It is intended for systems research and makes no claim of commercial software compatibility.

- The root crate forbids unsafe code (`#![forbid(unsafe_code)]`). This is not a workspace-wide guarantee: the optional `ounce-recompiler` crate owns native code generation and executable-memory management, including the platform-specific unsafe boundary.
- Five runtime dependencies: `byteorder`, `cpal` (host audio playback), `gilrs` (gamepad input), `minifb` (host window), `thiserror`. The optional `gpu-present` feature adds `wgpu`, `winit` and `pollster` for GPU presentation.
- No network code, no telemetry. The only URL in the source is the project's own repository link in the CLI banner.

## How this project is built

The maintainer uses AI coding agents as part of development and reviews their changes. CI runs the default tests and 25-stage harness, plus separate feature-specific jobs described below. Tests cover only the behaviors they exercise.

The checked-in NRO fixtures target the original Nintendo Switch homebrew environment (`libnx`). Switch 2 game support is a long-term goal, not a current capability; these fixtures and tests do not establish Switch 2 compatibility or commercial-game support.

---

## Legal Notice

**What this repository does not contain**

- No firmware, system archives, or code or data derived from them.
- No encryption keys. The loader decrypts only containers built with the in-repo synthetic test key; retail keys are rejected (`LoadError::RestrictedCipher`). A `prod.keys`-format parser exists for unit tests of the key-file format only. The `.gitignore` excludes key files.
- No retail game content, ROMs, or dumps. Tests include generated synthetic programs and checked-in homebrew NRO fixtures. The `hello` fixture build links libnx and uses the devkitA64/newlib toolchain; see [`tests/fixtures/nro/hello/README.md`](tests/fixtures/nro/hello/README.md). Fixture source comparisons and binary component inventories are incomplete; see [`docs/PROVENANCE.md`](docs/PROVENANCE.md) and [`THIRD_PARTY.md`](THIRD_PARTY.md).
- Earlier revisions contained firmware-derived and third-party-derived byte sequences; they were removed from the current tree but remain in git history. See *Known Issues*.

**Trademarks.** "Nintendo", "Nintendo Switch", "Switch 2", "NVIDIA", "Tegra", "Horizon" and related names are trademarks of their respective owners. This project is not affiliated with, endorsed by, or sponsored by any of them. Names appear only to describe the hardware and ABI being modelled.

**No warranty; not legal advice.** The software is provided as is. Anyone using, modifying or distributing it is responsible for complying with the copyright, anti-circumvention and other laws that apply where they are. Consult a lawyer before any use beyond private research.

**License information.** The repository includes an MIT license ([`LICENSE`](LICENSE)) and third-party material notes ([`THIRD_PARTY.md`](THIRD_PARTY.md)). These documents do not determine the licensing status of historical commits or every component in the checked-in binaries.

---

## Security Advisory

> [!WARNING]
> Third parties may distribute executables labelled "ounce-core" that contain malware. The only source of this project is the repository above. Build from source with the standard Rust toolchain.

---

## Implementation Status

Status reflects what the code does and what the tests exercise. "Modelled" means the behaviour is simulated in software, not measured against hardware.

| Subsystem | Status | Notes |
| :--- | :--- | :--- |
| AArch64 CPU | Implemented (subset) | Interpreter, Tier-1 block cache, Tier-2 decoded blocks, Tier-3 fused blocks, and optional Tier-4 native recompilation in `crates/ounce-recompiler` (`native-jit`). Guest writes to translated code stop the active block and invalidate stale translations; code-patch and differential tests cover this path. |
| Memory | Implemented | 4 KB-page MMU with intra-page boundary alignment and per-page permissions. No DRAM timing model. |
| Kernel (SVC) | Implemented (subset) | Threads, events, mutexes, memory and process queries needed by the test programs. |
| IPC | Implemented (subset) | CMIF and TIPC message formats. Services are synthetic. |
| Services | Modelled | `sm:`, `set:sys` (Drake T239 model, language, nickname), `am`, `vi`, `hid`, `fsp-srv` (directory enumeration, persistent save/sdmc partitions), `pl:u` (placeholder font data), `time:u` / `time:s` (nanosecond steady clock, UTC epoch timestamp), `nvdrv`, `ro:1`. |
| GPU | Modelled; optional host-GPU replay | Selected `MAXWELL_B` and `AMPERE_B` 3D methods are decoded and rendered by a CPU rasterizer. `gpu-present` can replay supported draw output through `wgpu`. This is not a hardware driver or NVN implementation; many behaviors remain unsupported or unverified. |
| Shader translation | Partial | The older SASS path emits SPIR-V 1.5; the newer Ampere path translates a limited SASS subset through SSA IR to WGSL. The newer decoder documents unverified encodings; neither path is validated against a hardware driver. |
| Vulkan HAL (`gpu/vulkan.rs`) | Model only | Structures and SPIR-V module checks. It does not call a Vulkan loader or driver. |
| Host presentation (`gpu/present/`) | Implemented | Guest framebuffer and 3D pushbuffer geometry presented through a host GPU hardware swapchain (`wgpu`: Metal on macOS, Vulkan on Linux/Windows; feature `gpu-present`), software window (`minifb`) or headless mode. |
| Upscaler (`gpu/upscaler.rs`) | Modelled | 2× bilinear interpolation with sharpening. **Not DLSS**; there is no neural network. |
| Audio | Implemented | Software 64-voice mixer; continuous non-blocking ring-buffer streaming from `audout:u` and `audren:u` to 48 kHz stereo host audio sink via `cpal` with polyphonic resampling, underrun tracking and latency metrics. |
| Input (`hid/`) | Implemented | Controller state filled from host keyboard and physical USB/Bluetooth gamepads via `gilrs` (analog stick threshold synthesis to D-pad bitmasks). |
| Containers | Implemented (parsing) | PFS0/NSP, HFS0/XCI, NCA3, NSO0, NRO0, NPDM, RomFS, unencrypted loose directories. |
| Dynamic linking | Implemented (subset) | MOD0 `.dynamic` parsing; `R_AARCH64_RELATIVE`, `ABS64`, `GLOB_DAT`, `JUMP_SLOT`; imports resolved across modules in loose packages; `DT_REL`/`DT_RELA` tables; unresolved imports reported; `DT_GNU_HASH` symbol counts; TLS relocations (`TPREL64`, `DTPMOD64`, `DTPREL64`, `TLSDESC`) against a static TLS layout; `DT_INIT`/`INIT_ARRAY`/`FINI_ARRAY`/`DT_FINI` runner (opt-in, `OUNCE_RUN_INIT=1`). `PT_TLS` templates (`.tdata` copied, `.tbss` zeroed, `p_align` honoured) when supplied from an ELF; each `svcCreateThread` thread gets its own TLS area and `TPIDR_EL0`. Limits: NSO/NRO carry no `PT_TLS`, so for them TLS block sizes come from `STT_TLS` symbols, alignment is assumed 16, and blocks start zeroed. Guest SVCs inside initialisers are not serviced. |
| Crypto | Implemented | AES-128 checked against FIPS-197 vectors. AES-128-CTR is used for NCA sections when the user supplies keys. |

**Known limitations**

- Timing and MIPS figures are host measurements of small synthetic programs. They are not comparable to hardware and are not pass/fail criteria.
- Behaviour that is not in the test suite should be assumed unverified.

---

## Goals and Roadmap

**North star:** run Nintendo Switch 2 software through an accurate, inspectable execution runtime. This is a long-term goal; Ounce-Core does not currently run Switch 2 software. Original Switch homebrew results are foundation evidence only.

### Goals

- Improve CPU, memory, service and graphics correctness with deterministic regressions and reference comparisons.
- Build compatibility evidence from programs that were not written for Ounce-Core.
- Keep public capability claims tied to reproducible results, with fixture sources and build inputs recorded.

### Milestones

Status snapshot reviewed **2026-10-29**. This table describes the merged `main` baseline; ongoing, unmerged work is identified separately below. No calendar estimates are set for milestones that depend on toolchains or hardware access.

| Milestone | Status | Exit criteria |
| :--- | :--- | :--- |
| Code-cache invalidation regression gate | Complete | Regressions cover guest code writes and page unmap/remap across the interpreter, Tier-3 and Tier-4; the moved-code regression covers Tier-4. All nine configured CI checks passed for this milestone. |
| Independent original Switch homebrew corpus | In progress (unmerged PRs) | Pin independently authored source revisions and build steps, confirm fixture-use terms, record guest stops and oracle results under interpreter and Tier-3, verify graphics against actual submitted frame bytes, and obtain reproduction from an outside clean checkout. External startup/exit evidence exists on development branches, but no independent rendered-frame pass or broad compatibility claim is established. |
| Switch 2 vertical slice | Blocked | Requires an authorized Switch 2 toolchain and reference hardware. Then build a self-authored sample and match startup, one input-driven change, a fixed completion result and a captured test image on Ounce-Core and the reference system. Record source and build provenance. |
| Broader Switch 2 compatibility | Later | Expand one verified behavior or sample at a time, using target-specific results and updating the support claims to match the evidence. |

Original Switch tests can exercise shared loading, CPU, service and graphics foundations. They do not establish Switch 2 ABI, services, GPU behavior, commercial-software compatibility or performance.

### Active development evidence (29 October 2026; not yet released)

The following links are **open pull requests**, not features guaranteed to exist on `main`. Their CI runs check the submitted branches, not real-world compatibility.

| Workstream | Latest documented evidence | Remaining gate |
| :--- | :--- | :--- |
| [#89 — independent corpus preparation](https://github.com/hffgzktbcc-blip/ounce-core/pull/89) | Four pinned public-source trial builds and an external Linux diagnostic probe; current-head nine-job CI reported green. | Resolve merge conflict, validate provenance and require clean-checkout reproduction. |
| [#90 — interpreter/Tier-3 external runner](https://github.com/hffgzktbcc-blip/ounce-core/pull/90) | Four samples were exercised under both engines (eight executions). The recorded diagnostic runs reached `svcBreak` / libnx `BadGfxInit` (`0x2559`) with zero frames at that tested revision. | Reconcile stacked corpus changes, and get a meaningful guest-output oracle. |
| [#91 — bounded Binder parcel validation](https://github.com/hffgzktbcc-blip/ounce-core/pull/91) | Parser and object-lifetime regressions; nine-job CI reported green. | Resolve merge conflicts and coordinate with newer VI/producer implementation. |
| [#94 — host GPU replay corrections](https://github.com/hffgzktbcc-blip/ounce-core/pull/94) | Offscreen host replay regression for clear ordering, texture lifetime and inherited viewport; nine-job CI reported green. | Integration review; does not establish a guest-rendered frame. |
| [#95 — libnx graphics startup](https://github.com/hffgzktbcc-blip/ounce-core/pull/95) | On its tested branch, three externally sourced homebrew candidates reportedly reached normal `svcExitProcess` after added native-window, buffer-producer and GPU-submission plumbing; nine-job CI reported green. | Verify actual submitted frames and externally expected pixels; review integration and provenance. |
| [#96 — Ounce-Verify contract](https://github.com/hffgzktbcc-blip/ounce-core/pull/96) | Draft specification for versioned evidence records and strict oracle-based verdicts. | Implement the validator, its negative tests and CI consumer; design document alone is not a working tool. |

Successful host workflow completion, green CI, guest process exit and correct rendered output are **four different claims**. No PR above demonstrates a verified Switch 2 software run.

---

## Repository Structure

```
ounce-core/
├── .github/workflows/ # Cross-platform CI
├── assets/            # Vector artwork
├── crates/
│   └── ounce-recompiler/ # Optional Tier-4 native code and executable-memory boundary
├── docs/design/       # Architecture and implementation notes
├── src/               # Runtime, CPU, GPU, kernel, loader and host integrations
├── tests/fixtures/nro/ # Checked-in homebrew integration fixtures
├── AGENTS.md
├── Cargo.lock
├── Cargo.toml
├── CONTRIBUTING.md
├── LICENSE
└── README.md
```

---

## Verification

```bash
cargo test --release
cargo run --release -- test
```

The harness has 25 stages (with multiple checks within some stages). It runs synthetic AArch64 programs written for this repository through the CPU, kernel and service models. It is not an independently authored homebrew compatibility suite. Each stage passes when its guest program returns the expected exit code; host timings are informational only. Passing this harness does not establish compatibility with commercial software or general homebrew compatibility.

CI coverage is feature-specific: the default job runs unit tests and the 25-stage harness; `gpu-present` builds and runs unit tests but does not run that harness; `native-jit` tests the recompiler crate, builds the feature, runs the Tier-4 host self-test and feature-enabled unit tests, but does not run the 25-stage harness. These configured checks describe CI coverage, not the result of the latest run. Test totals are omitted because they can change between revisions.

| Stage | Subsystem | What it checks | Exit |
| :---: | :--- | :--- | :---: |
| 01 | AArch64 ALU & branching | Fibonacci `N=10` through the block cache | `55` |
| 02 | Debug SVC | `svcOutputDebugString` with a guest string | — |
| 03 | Calling convention | Stack frame, `BL` and `RET` | `42` |
| 04 | CMIF IPC & vector math | `sm:` lookup of `nvdrv`, IEEE-754 `FADD` | `D0 = 42.0` |
| 05 | RomFS & PCM audio | Reads `romfs:/title.txt`, queues audio to `audout:u` | — |
| 06 | AES-CTR & threads | NCA section decryption, SASS-subset decode, thread spawn | — |
| 07 | Package boot & rasterizer | Boots an NSP, futex arbitration, textured triangle | — |
| 08 | crt0 & depth | `svcGetInfo`, direct-chained JIT, Z-buffer occlusion | — |
| 09 | NEON & upscaler | 128-bit vector arithmetic, 2× upscaler, save sync | — |
| 10 | FS query payload | BTI landing pad, process/thread queries, challenge response | — |
| 11 | Dynamic linker | `RO` NSO0 module over TIPC, `NS2ROEXE` reply | — |
| 12 | Thread-affinity wrapper | `BL` into a leaf that returns `0xFF`, then `AND X0, X0, #7` | `0x07` |
| 13 | Timing & events | Synthetic event init, try-wait and tick frequency | `0x124F801` (19,200,001) |
| 14 | XCI container boot | Synthetic cartridge image → HFS0 → NCA3 → NSO0 | — |
| 15 | NVN command buffer | `SVC #0x105` (research range) submission, multi-section RomFS | — |
| 16 | audren & Vulkan model | Memory pools, voice mixing, SPIR-V module construction | — |
| 17 | Service handshake | `set:sys`, `am:appletOE`, `vi:m`, `hid`, `fsp-srv` | `0x777` |
| 18 | ro:1 & texture pools | `LoadNro`, BC7 descriptor binding | `0x888` |
| 19 | SDK-style HLE runtime | Standard allocator, `SleepThread`, `nn::fs::OpenFile` | `0x999` |
| 20 | Tier-3 / optional Tier-4 | Hot-loop benchmark; select with `--cpu=tier3|tier4` (`native-jit` required for Tier-4). Host MIPS is informational, not gated. | `0xAAA` |
| 21 | Loose packages | NPDM parse, multi-module link, loose RomFS | `0xBBB` |
| 22 | Shared font & clock | `pl:u` placeholder mapping, `time:u`, ASTC 4×4 | `0xCCC` |
| 23 | Applet & GPU model | `IApplicationFunctions`, `ISelfController`, ASTC 8×8 | `0xDDD` |
| 24 | FP16 SASS & NVN ring | FP16 SASS vector math (`HADD2`, `HMUL2`, `HFMA2`), dynamic NVN buffer cycling | `0xEEE` |
| 25 | Tensor core MMA | Warp-level FP16 matrix multiply-accumulate (`HMMA.1688`, `HMMA.16816`) with FP32 accumulate, checked against IEEE binary16 vectors | `0xEEE` |

---

## Getting Started

### Prerequisites

- Rust 1.85 or newer (edition 2024).
- Host: `aarch64` or `x86_64`, Linux, macOS or Windows.
- Prebuilt Linux binaries dynamically link to the system ALSA and udev libraries. On Debian/Ubuntu: `sudo apt install libasound2 libudev1`. On Fedora: `sudo dnf install alsa-lib systemd-libs`. On Arch: `sudo pacman -S alsa-lib systemd-libs`. Building from source also needs the development headers (`libasound2-dev libudev-dev` on Debian/Ubuntu).
- GPU presentation (optional): build with `--features gpu-present`. macOS uses Metal and needs nothing extra. Windows uses Vulkan or DX12 from the installed GPU driver. Linux needs a Vulkan driver plus `libvulkan1` and `libxkbcommon-x11-0` on Debian/Ubuntu (`vulkan-loader libxkbcommon-x11` on Fedora, `vulkan-icd-loader libxkbcommon-x11` on Arch); `mesa-vulkan-drivers` gives a software Vulkan driver for machines without a GPU. If the GPU path cannot start, the runtime falls back to the software window and then to headless.

### Commands

```bash
cargo run --release -- info                      # Show modelled hardware and engine status
cargo run --release -- test                      # Run the 25-stage harness
cargo run --release -- run tests/fixtures/nro/hello/hello.nro fixture  # Boot the libnx homebrew fixture (prints result=0x00000EEE)
cargo run --release -- inspect ./dumps/title     # Inspect a user-supplied unencrypted directory
cargo run --release -- run ./dumps/title --gui   # Boot an unencrypted loose directory in a window
cargo run --release -- demo-dir ./demo --gui     # Generate a synthetic loose package and boot it
cargo run --release -- play                      # Keyboard-driven controller and graphics demo
cargo run --release -- 3d 120                    # Software rasterizer benchmark
cargo run --release --features gpu-present -- gfx --present=gpu   # Present through the GPU swapchain
cargo run --release -- gfx 60 --present=headless # No window; OUNCE_DUMP_FRAMES=<dir> writes PPM frames
cargo run --release --features native-jit -- jit-selftest # Check whether this host can execute Tier-4 code
cargo run --release --features native-jit -- test --cpu=tier4 # Run Tier-4 for the Stage 20 benchmark
```

`--present=auto|gpu|minifb|headless` (or `OUNCE_PRESENT`) picks the presentation backend; `auto` tries the GPU first when the feature is compiled in. `WGPU_BACKEND` (`vulkan`, `metal`, `dx12`, `gl`) and `WGPU_ADAPTER_NAME` select the GPU, and `OUNCE_VSYNC=0` disables vsync. `--cpu=tier3|tier4` (or `OUNCE_CPU`) selects the Stage 20 CPU engine. Tier-4 requires `--features native-jit`; the native engine is not currently used for general `run` workloads.

---

## Known Issues

- **Git history.** A history cleanup has been discussed; completion is not verified. The project has previously identified commits before `76ce766` (including `c7f16f2` and `4be9640`) as containing firmware-derived or third-party-derived byte sequences and an external-payload loader. On 2026-10-28, this checkout confirmed `c7f16f2` is reachable from `main`, tags `v0.3.0` through `v0.3.2`, and the fetched pull-request refs `72`, `81`, `82`, and `83`. This is a dated, limited ref check, not an inventory of every branch, tag, fork, clone, or GitHub ref. See [`docs/PROVENANCE.md`](docs/PROVENANCE.md).
- **Commercial container decryption.** Decryption in `loader/nca.rs` is restricted by policy to synthetic test vectors (`SYNTHETIC_TEST_KEY`). Retail key decryption is disabled; research binaries must be supplied as unencrypted loose directories or homebrew packages.
- **Provenance status.** The provenance register and clean-room protocol are drafts and have not been legally reviewed. Known source comparisons and binary composition checks remain incomplete: [`docs/PROVENANCE.md`](docs/PROVENANCE.md), [`docs/CLEAN_ROOM.md`](docs/CLEAN_ROOM.md).

---

## Contributing

Changes must keep the test suite and the 25-stage harness passing. New test inputs must be synthetic or written by hand from the public AArch64 and SPIR-V specifications. Do not add firmware, keys, game content, or code or data copied from other projects.
