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

`ounce-core` is an experimental, non-commercial AArch64 execution and high-level-emulation (HLE) research runtime written in Rust. It contains an interpreter, three execution tiers, a paged memory model, a subset of Horizon-style supervisor calls (SVCs) and services, and software models of graphics, audio and input. It is a test harness for studying these subsystems. It is not a console emulator and makes no claim of compatibility with any commercial software.

- Safe Rust only: `grep -rn unsafe src` returns no matches.
- Five runtime dependencies: `byteorder`, `cpal` (host audio playback), `gilrs` (gamepad input), `minifb` (host window), `thiserror`.
- No network code, no telemetry. The only URL in the source is the project's own repository link in the CLI banner.

---

## Legal Notice

**What this repository does not contain**

- No firmware, system archives, or code or data derived from them.
- No encryption keys. Code that handles keys reads only files the user supplies at runtime (`prod.keys`, `title.keys`). The `.gitignore` excludes these files.
- No game content, ROMs, or dumps. Test inputs are synthetic: they are generated at runtime or written as AArch64 instructions in source.
- No code copied from third-party projects. Earlier revisions contained third-party-derived byte sequences; they were removed. See *Known Issues* for the git history.

**Trademarks.** "Nintendo", "Nintendo Switch", "Switch 2", "NVIDIA", "Tegra", "Horizon" and related names are trademarks of their respective owners. This project is not affiliated with, endorsed by, or sponsored by any of them. Names appear only to describe the hardware and ABI being modelled.

**No warranty; not legal advice.** The software is provided as is. Anyone using, modifying or distributing it is responsible for complying with the copyright, anti-circumvention and other laws that apply where they are. Consult a lawyer before any use beyond private research.

**License.** MIT. See [`LICENSE`](LICENSE). The license covers the code in this repository only. It does not cover any third-party material that may appear in earlier commits (see *Known Issues*).

---

## Security Advisory

> [!WARNING]
> Third parties may distribute executables labelled "ounce-core" that contain malware. The only source of this project is the repository above. Build from source with the standard Rust toolchain.

---

## Implementation Status

Status reflects what the code does and what the tests exercise. "Modelled" means the behaviour is simulated in software, not measured against hardware.

| Subsystem | Status | Notes |
| :--- | :--- | :--- |
| AArch64 CPU | Implemented (subset) | Interpreter plus Tier-1 block cache, Tier-2 chained blocks and Tier-3 super-blocks. Not the full ISA. |
| Memory | Implemented | 4 KB-page MMU with per-page permissions. No DRAM timing model. |
| Kernel (SVC) | Implemented (subset) | Threads, events, mutexes, memory and process queries needed by the test programs. |
| IPC | Implemented (subset) | CMIF and TIPC message formats. Services are synthetic. |
| Services | Modelled | `sm:`, `set:sys`, `am`, `vi`, `hid`, `fsp-srv`, `pl:u` (placeholder font data), `time:u`, `nvdrv`, `ro:1`. |
| GPU | Modelled | Command-buffer model, software rasterizer, texture and ASTC/BC7 block decoders. No GPU API is called. |
| Shader translation | Partial | A SASS-subset decoder that emits SPIR-V 1.5 bytes. Not validated against a driver. |
| Vulkan HAL (`gpu/vulkan.rs`) | Model only | Structures and SPIR-V module checks. It does not call a Vulkan loader or driver. |
| Upscaler (`gpu/dlss.rs`) | Modelled | 2× bilinear interpolation with sharpening. **Not DLSS**; there is no neural network. |
| Audio | Implemented | Software 64-voice mixer; real-time 48 kHz stereo host audio sink via `cpal`. |
| Input (`hid/`) | Implemented | Controller state filled from host keyboard and physical USB/Bluetooth gamepads via `gilrs`. |
| Containers | Implemented (parsing) | PFS0/NSP, HFS0/XCI, NCA3, NSO0, NRO0, NPDM, RomFS, unencrypted loose directories. |
| Dynamic linking | Implemented (subset) | MOD0 `.dynamic` parsing; `R_AARCH64_RELATIVE`, `ABS64`, `GLOB_DAT`, `JUMP_SLOT`; imports resolved across modules in loose packages; unresolved imports reported. Not implemented: TLS relocations, `DT_GNU_HASH`, running `DT_INIT`/`DT_FINI`. |
| Crypto | Implemented | AES-128 checked against FIPS-197 vectors. AES-128-CTR is used for NCA sections when the user supplies keys. |

**Known limitations**

- Timing and MIPS figures are host measurements of small synthetic programs. They are not comparable to hardware and are not pass/fail criteria.
- Behaviour that is not in the test suite should be assumed unverified.

---

## Repository Structure

```
ounce-core/
├── assets/           # Vector artwork (banner, logo, architecture diagram)
├── src/
│   ├── audio/        # Audio renderer model and PCM output sink
│   ├── cpu/          # AArch64 decoder, interpreter, Tier-1/2 JIT, Tier-3 super-block engine
│   ├── crypto/       # AES-128 and AES-128-CTR (user-supplied keys)
│   ├── fs/           # Virtual filesystem, RomFS parser, save-data storage
│   ├── gpu/          # Command-buffer model, rasterizer, shader translator, upscaler model
│   ├── hid/          # Controller state model and deadzone filters
│   ├── kernel/       # SVC dispatcher, IPC, scheduler, synchronization, nnSdk-style runtime
│   ├── loader/       # Container parsers and synthetic image builders
│   ├── mmu/          # Paged memory model
│   ├── ui/           # Headless package inspector and CLI telemetry
│   └── main.rs       # CLI and the 24-stage integration harness
├── Cargo.toml
└── README.md
```

---

## Verification

```bash
cargo test --release            # unit tests
cargo run --release -- test     # 24-stage integration harness
```

The harness runs synthetic AArch64 programs through the CPU, kernel and service models. Each stage passes when its guest program returns the expected exit code. Host timings are printed for information only.

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
| 15 | NVN command buffer | `SVC #0x2D` submission, multi-section RomFS | — |
| 16 | audren & Vulkan model | Memory pools, voice mixing, SPIR-V module construction | — |
| 17 | Service handshake | `set:sys`, `am:appletOE`, `vi:m`, `hid`, `fsp-srv` | `0x777` |
| 18 | ro:1 & texture pools | `LoadNro`, BC7 descriptor binding | `0x888` |
| 19 | nnSdk-style runtime | Standard allocator, `SleepThread`, `nn::fs::OpenFile` | `0x999` |
| 20 | Tier-3 super-blocks | Hot-loop fusion (host MIPS printed, not gated) | `0xAAA` |
| 21 | Loose packages | NPDM parse, multi-module link, loose RomFS | `0xBBB` |
| 22 | Shared font & clock | `pl:u` placeholder mapping, `time:u`, ASTC 4×4 | `0xCCC` |
| 23 | Applet & GPU model | `IApplicationFunctions`, `ISelfController`, ASTC 8×8 | `0xDDD` |
| 24 | FP16 SASS & NVN ring | FP16 SASS vector math (`HADD2`, `HMUL2`, `HFMA2`), dynamic NVN buffer cycling | `0xEEE` |

---

## Getting Started

### Prerequisites

- Rust 1.85 or newer (edition 2024).
- Host: `aarch64` or `x86_64`, Linux, macOS or Windows.
- Prebuilt Linux binaries dynamically link to `libasound2` and `libudev1`. On Debian/Ubuntu: `sudo apt install libasound2 libudev1`.

### Commands

```bash
cargo run --release -- info                      # Show modelled hardware and engine status
cargo run --release -- test                      # Run the 24-stage harness
cargo run --release -- inspect ./dumps/title     # Inspect a user-supplied unencrypted directory
cargo run --release -- run ./dumps/title --gui   # Boot an unencrypted loose directory in a window
cargo run --release -- demo-dir ./demo --gui     # Generate a synthetic loose package and boot it
cargo run --release -- play                      # Keyboard-driven controller and graphics demo
cargo run --release -- 3d 120                    # Software rasterizer benchmark
```

---

## Known Issues

- **Git history.** Repository Git history was rewritten across all commits to remove historical third-party byte sequences and test binary assets; remote GitHub backend cache and pull request ref purge is pending support ticket resolution.
- **Commercial container decryption.** Decryption in `loader/nca.rs` is restricted by policy to synthetic test vectors (`SYNTHETIC_TEST_KEY`). Retail key decryption is disabled; research binaries must be supplied as unencrypted loose directories or homebrew packages.
- **License scope.** The MIT license applies to this tree. Rewritten history removes prior external code sequences.

---

## Contributing

Changes must keep the test suite and the 24-stage harness passing. New test inputs must be synthetic or written by hand from the public AArch64 and SPIR-V specifications. Do not add firmware, keys, game content, or code or data copied from other projects.
