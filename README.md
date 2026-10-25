<p align="center">
  <img src="assets/banner.svg" alt="ounce-core: Clean-Room Hardware Research Runtime for NVIDIA Tegra T239" width="100%" />
</p>

<p align="center">
  <a href="https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml"><img src="https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml/badge.svg" alt="Build Status" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/language-Rust%202024-orange.svg" alt="Language" /></a>
  <a href="#clean-room-legal-disclaimer--compliance-protocol"><img src="https://img.shields.io/badge/methodology-Clean--Room%20Research-blue.svg" alt="Methodology" /></a>
  <a href="https://www.nvidia.com/"><img src="https://img.shields.io/badge/target-Tegra%20T239%20%7C%20ARMv8.5--A-76b900.svg" alt="Target Platform" /></a>
  <a href="#clean-room-legal-disclaimer--compliance-protocol"><img src="https://img.shields.io/badge/keys-Zero%20Proprietary%20Keys-success.svg" alt="Zero Keys" /></a>
</p>

---

## Abstract

`ounce-core` is an open-source, clean-room systems research runtime and architecture compatibility layer for the **NVIDIA Tegra T239 ("Drake") SoC** and **Horizon OS 64-bit ABI**. Written entirely in pure safe Rust, the project models the hardware subsystems, virtual memory management, instruction execution, and microkernel supervisor interfaces of next-generation ARMv8-A platforms.

> **Project Scope**: `ounce-core` is developed strictly as a systems architecture research platform and interoperability compatibility layer. It provides clean-room binary execution, container auditing, and hardware emulation primitives for educational and diagnostic research. It is not an end-user video game distribution software.

---

## Clean-Room Legal Disclaimer & Compliance Protocol

Development of `ounce-core` strictly follows established clean-room reverse-engineering protocols to ensure complete legal compliance and freedom from proprietary intellectual property:

- **Independent Clean-Room Architecture**: All components are developed purely from published architectural documentation, publicly accessible interface specifications, and clean-room observation of hardware execution. No proprietary source code, internal SDK headers, or leaked documentation were utilized in this project.
- **Zero Proprietary Cryptographic Keys**: This repository contains **NO cryptographic keys** (`prod.keys`, `title.keys`, `header.key`, or master keys). All cryptographic self-tests use public reference standards (NIST FIPS 197) and synthetic test values.
- **Zero Copyrighted Assets or Binary Dumps**: This repository contains **NO copyrighted game assets, ROM dumps, or proprietary system firmware archives**. All test suites execute against clean-room synthetic binaries generated at runtime.
- **Legal Precedent for Interoperability**: Reverse engineering for the purpose of software interoperability is well-established and protected under applicable international copyright law, including *Sony Computer Entertainment, Inc. v. Connectix Corp.*, 203 F.3d 596 (9th Cir. 2000), *Sony Computer Entertainment America, Inc. v. Bleem, LLC*, 214 F.3d 1022 (9th Cir. 2000), and *Sega Enterprises Ltd. v. Accolade, Inc.*, 977 F.2d 1510 (9th Cir. 1992).

---

## Security Advisory & Authentic Source Verification

> [!WARNING]
> **BEWARE OF THIRD-PARTY MALWARE BUILDS**: Due to external media coverage, unauthorized parties and scam websites may distribute malicious executables (`.exe`, `.zip`) labeled as "Ounce Core". These third-party downloads often contain infostealers, Trojans, or adware.
>
> - **Sole Official Repository**: The only authentic source for `ounce-core` is: [`https://github.com/hffgzktbcc-blip/ounce-core`](https://github.com/hffgzktbcc-blip/ounce-core).
> - **Source-Only Distribution**: `ounce-core` is 100% open-source safe Rust with zero proprietary binary blobs, zero network telemetry, and zero obfuscation. Always audit and build directly from source using standard Cargo toolchains.

---

## Hardware Architecture & Subsystem Specification

<p align="center">
  <img src="assets/architecture.svg" alt="NVIDIA Tegra T239 & Horizon OS Hardware Architecture Diagram" width="100%" />
</p>

`ounce-core` implements an authentic, clean-room representation of the NVIDIA Tegra T239 SoC across six fundamental hardware domains:

### 1. Central Processing Unit (CPU) & Vector Execution Engine
```
   ┌─────────────────────────────────────────────────────────────────────────────┐
   │                    8× ARM Cortex-A78C Micro-Core Cluster                     │
   │  [Core 0: Main]   [Core 1: Worker]  [Core 2: Worker]  [Core 3: Worker]       │
   │  [Core 4: Audio]  [Core 5: Worker]  [Core 6: Worker]  [Core 7: Horizon OS]   │
   ├─────────────────────────────────────────────────────────────────────────────┤
   │  • ARMv8.3-A PAC: PACIBSP, PACIASP, AUTIBSP, RETAA, RETAB (Pointer Auth)     │
   │  • ARMv8.5-A BTI: BTI c, BTI j, BTI jc (Branch Target Identification)       │
   │  • 128-bit NEON: FADD, FMUL, FSUB, FMA Vector SIMD & IEEE-754 Precision     │
   │  • Execution HAL: Tier-1 Basic-Block Cache + Tier-2 Chained JIT (>7.5 MIPS) │
   └─────────────────────────────────────────────────────────────────────────────┘
```
- **Octa-Core Topology**: Models 8× ARM Cortex-A78C cores running an authentic preemptive thread scheduler with SMP core affinity masks (`0xFF`).
- **ISA Security Features**: Emulates ARMv8.3-A Pointer Authentication (PAC) preventing return-oriented programming (ROP) and ARMv8.5-A Branch Target Identification (BTI) enforcing landing-pad validation.
- **Dual-Tier Execution**: Tier-1 Basic-Block Translation Cache maps guest basic blocks into pre-decoded fast-paths; Tier-2 Direct-Chained JIT eliminates interpreter dispatch loop overhead for hot jump targets.

### 2. Graphics Processing Unit (GPU) & NVN Driver HLE
```
   ┌─────────────────────────────────────────────────────────────────────────────┐
   │                NVIDIA Ampere GA10B/GA10F Custom SoC Derivative              │
   │   12 Streaming Multiprocessors (~1,536 CUDA Cores, SM 8.7 Feature Set)     │
   ├─────────────────────────────────────────────────────────────────────────────┤
   │  • NVN Driver HLE: NvnCommandBuffer, NvnMemoryPool, NvnTexturePool          │
   │  • Fast-Path Submission: SVC #0x2D (svcNvnSubmitCommandBuffer)              │
   │  • SASS Transpiler: Ampere SM87 Machine ISA -> Vulkan SPIR-V 1.5 (0x07230203)│
   │  • Extended Opcodes: LOP3.LUT, ISETP/FSETP, MUFU (Sin/Cos/Rcp), SEL         │
   │  • AI Super-Resolution: Hardware Tensor Core Engine (Tensor DLSS 2× Scaling)│
   │  • Graphics Pipeline: Double-Buffered 32-bit RGBA8888 VRAM + Z-Buffer Depth │
   └─────────────────────────────────────────────────────────────────────────────┘
```
- **Official NVN Driver HLE**: Provides complete high-level emulation for Nintendo Switch 2's primary graphics API (`NvnCommandBuffer`, `NvnMemoryPool`, `NvnTexturePool`), communicating via fast-path `SVC #0x2D` (`svcNvnSubmitCommandBuffer`) and `nvdrv` `IOCTL_NVN_SUBMIT` (`0xC018_4E01`).
- **Ampere SASS Transpiler**: Decompiles native Ampere SM87 shader bytecode into validated Vulkan SPIR-V 1.5 binaries (`0x07230203`) with hardware LUT evaluation (`LOP3.LUT`), condition code predicates (`ISETP`/`FSETP`), and mathematical function units (`MUFU`).
- **Hardware Tensor DLSS**: Embedded edge-directed tensor reconstruction upscaling 540p handheld renders to 1080p and 1080p docked renders to 4K.

### 3. Memory Subsystem & Sparse Virtual MMU
```
   ┌─────────────────────────────────────────────────────────────────────────────┐
   │              12 GB LPDDR5X DRAM (128-bit Bus @ 6400 MT/s, 102.4 GB/s)       │
   ├─────────────────────────────────────────────────────────────────────────────┤
   │  Virtual Regions:                                                           │
   │  • Code / RoData / Data : Base: 0x8000_0000 (Executable guest text & data)  │
   │  • Dynamic User Heap    : Base: 0x1_0000_0000 (svcSetHeapSize arena)        │
   │  • Thread Stack / TLS   : Base: 0x7FFF_FE0000 (1MB stacks with guard pages) │
   │  • VRAM Framebuffer     : Base: 0x9000_0000 (1280×720 handheld / 1080p dock)│
   │  • Asynchronous Ring    : Base: 0x9400_0000 (GPFIFO command buffer queue)   │
   │  • Memory Protection    : Strict W^X (Write XOR Execute) page permissions   │
   └─────────────────────────────────────────────────────────────────────────────┘
```
- **Physical Memory Model**: Simulates the 12GB LPDDR5X memory architecture across a 128-bit wide bus providing ~102.4 GB/s theoretical bandwidth.
- **Sparse 4KB Virtual MMU**: 39-bit and 48-bit Horizon virtual address space management with cross-process memory mirroring (`svcMapProcessMemory`, `svcMapProcessCodeMemory`) and page permission enforcement.

### 4. Audio Digital Signal Processor (DSP) & Joy-Con 2 Input
```
   ┌─────────────────────────────────────────────────────────────────────────────┐
   │                 Audio DSP Mixer & Controller Peripheral HAL                 │
   ├─────────────────────────────────────────────────────────────────────────────┤
   │  • audren:u DSP Mixer  : Multi-channel voice matrix & wave buffer queues    │
   │  • audout:u PCM Sink   : 48kHz Stereo 16-bit linear PCM audio stream        │
   │  • Joy-Con 2 Interface : Rail Attached, Wireless BT, and Dual-Grip modes    │
   │  • Shared Memory (hid) : Handle 0x200 polling with radial deadzone filters  │
   └─────────────────────────────────────────────────────────────────────────────┘
```
- **audren:u Audio Renderer**: Clean-room DSP voice mixer performing real-time multi-channel voice interpolation, volume matrix ramping, and circular DMA buffer transfers.
- **Joy-Con 2 Controller Framework**: Subsystem polling over shared memory handle `0x200` with hardware button bitmasking, analog stick radial deadzone filtering, and support for attached rail and wireless Bluetooth configurations.

### 5. Storage Containers & Cryptographic Engine
```
   ┌─────────────────────────────────────────────────────────────────────────────┐
   │                  Storage Formats & Cryptographic Engine                     │
   ├─────────────────────────────────────────────────────────────────────────────┤
   │  • GameCard Media (.xci) : 64GB High-Speed GameCard, HEAD at 0x100, HFS0    │
   │  • Package Archive (.nsp): Nintendo Submission Package (PFS0 Archive)       │
   │  • Cryptography (NCA3)   : Hardware AES-128-CTR multi-section decryptor     │
   │  • Filesystem (RomFS)    : Level 3 hierarchical directory/file hash tables  │
   └─────────────────────────────────────────────────────────────────────────────┘
```
- **Universal Container Parser**: Ingests, validates, and unpacks `.xci`, `.nsp`, `.nca`, `.nso`, and `.nro` containers.
- **AES-128-CTR Cryptographic Engine**: Multi-section decryptor simultaneously processing Section 0 (`ExeFS` code) and Section 1 (`RomFS` data) with automated IV calculation.
- **Level 3 RomFS Engine**: Implements authentic hierarchical directory and file hash buckets, mounting directly into the Horizon `VirtualFileSystem` under `romfs:/...`.

### 6. Horizon OS 64-Bit Microkernel & HLE Services
```
   ┌─────────────────────────────────────────────────────────────────────────────┐
   │             Horizon OS 64-bit Microkernel (Target FW 23.0.1 ABI)            │
   ├─────────────────────────────────────────────────────────────────────────────┤
   │  • IPC Protocols : Switch 2 Tipc (0x10/0x12) & Horizon CMIF (SFCI/SFCO)     │
   │  • set:sys       : Firmware Version 23.0.1 reporting & device locale        │
   │  • am:appletOE   : Applet proxy lifecycle, focus state & foreground rights  │
   │  • vi:m          : Display management, vsync timing & stray layer creation  │
   │  • fsp-srv       : Data storage access & Program RomFS filesystem mounts    │
   │  • ro:1          : Dynamic Module Linker (LoadNro/LoadNrr MOD0 relocation)  │
   │  • nvdrv         : GPU driver device nodes (/dev/nvhost-ctrl, /dev/nvmap)   │
   └─────────────────────────────────────────────────────────────────────────────┘
```
- **TIPC & CMIF Dual-Protocol IPC**: Handles both legacy Horizon CMIF (`SFCI`/`SFCO`) and native Switch 2 `sm:` TIPC (`0x10` `RegisterClient`, `0x12` `RegisterService`).
- **Complete Big 5 Initialization Sequence**: Fulfills the mandatory retail bootstrap sequence (`set:sys`, `am:appletOE`, `vi:m`, `hid`, `fsp-srv`) verified with guest exit code `0x777`.
- **Dynamic Module Linker (`ro:1`)**: Resolves `MOD0` relocations (`R_AARCH64_RELATIVE`), allocates guest memory for secondary NRO modules, and configures homebrew ABI ARGV blocks.

---

## Repository Structure

```
ounce-core/
├── assets/           # Vector branding artwork (logo.svg, banner.svg, architecture.svg)
├── src/
│   ├── audio/        # audren:u DSP voice mixer & audout:u PCM output sinks
│   ├── cpu/          # AArch64 decoder, interpreter, Tier-1/2 JIT, and Tier-3 Super-Block engine
│   ├── crypto/       # Clean-room AES-128-CTR multi-section cryptographic engine
│   ├── fs/           # Virtual File System (VFS), Level 3 RomFS parser & persistent storage
│   ├── gpu/          # Ampere GPU rasterizer, NVN driver HLE, SASS decompiler, and DLSS engine
│   ├── hid/          # Joy-Con 2 physical controller state, button bitmasks, and deadzone filters
│   ├── kernel/       # Horizon OS microkernel, SVC dispatcher, TIPC/CMIF IPC, and nnSdk C++ runtime
│   ├── loader/       # Container parsers (XCI, NSP, PFS0, HFS0, NCA3, NSO0, NRO0)
│   ├── mmu/          # Sparse 4KB page-table memory management unit & virtual address regions
│   ├── ui/           # Headless Package Inspector and CLI visual telemetry
│   └── main.rs       # Entry point, CLI dispatcher, and 20-stage integration verification harness
├── Cargo.toml        # Minimal pure-Rust dependency configuration
└── README.md         # Architecture specification and technical documentation
```

---

## Verified Integration Verification Suite

The repository includes a comprehensive 20-stage full-stack integration verification harness (`cargo run --release -- test`) and 41 granular unit tests (`cargo test --release`):

| Stage | Subsystem Under Verification | Verification Vector / Exit Assertion |
| :---: | :--- | :--- |
| **01** | AArch64 ALU & Branching | Computes Fibonacci `N=10` (`X0 = 55`) via JIT basic-block cache |
| **02** | Horizon OS Debug Subsystem | Dispatches `svcOutputDebugString` with guest memory string pointer |
| **03** | Subroutine Calling Convention | Stack frame push/pop (`STP`, `LDP`), `BL`, and `RET` (`X0 = 42`) |
| **04** | CMIF IPC & Vector Math | Dispatches `sm:` -> `nvdrv` service lookup & IEEE-754 `FADD` (`D0 = 42.0`) |
| **05** | RomFS & Linear PCM Audio | Reads `romfs:/title.txt` and queues 48kHz audio to `audout:u` (Peak: 14,400) |
| **06** | NCA3 Cryptography & Multi-Threading | AES-128-CTR decryption, Ampere SASS decompiler, and multi-thread spawning |
| **07** | Unified Package Boot & Rasterizer | Boots `.nsp` container, arbitrates futex locks, and rasterizes 2D-textured polygon |
| **08** | Horizon crt0 & Depth Testing | Queries `svcGetInfo`, executes direct-chained JIT, and verifies Z-buffer depth occlusion |
| **09** | NEON SIMD & Tensor DLSS | Evaluates 128-bit vector arithmetic, DLSS 2x upscaling, and persistent save synchronization |
| **10** | Native FS Payload Security | Executes clean-room synthetic payload verifying `BTI c`, `PACIBSP`, `svcGetProcessId`, and `RETAB` |
| **11** | Dynamic Linker & TIPC IPC | Mounts `RO` `NSO0` module via TIPC (0x10/0x12) and validates `"NS2ROEXE"` challenge response |
| **12** | Horizon OS 23.0.1 SDK Stubs | Executes captured Firmware 23.0.1 `nnSdk` CoreMask and `nnCompatThin` wrappers |
| **13** | Hardware Timing & Event Arbiters | Tests `nn::os::InitializeEvent`, `TryWaitEvent`, and `CNTPCT_EL0` system tick timers |
| **14** | 64GB GameCard (.XCI) Boot | Walks `HFS0` secure partition, decrypts `NCA3`, and boots `NSO0` executable |
| **15** | NVN 3D Command Buffer Execution | Executes fast-path `SVC #0x2D` (`svcNvnSubmitCommandBuffer`) and mounts multi-section RomFS |
| **16** | audren:u DSP & Vulkan Pipeline | Sub-allocates `nvnMemoryPool` VRAM, renders 48kHz audio DSP voice, and loads SPIR-V 1.5 |
| **17** | Horizon Big 5 Retail Handshake | Executes retail handshake across `set:sys`, `am:appletOE`, `vi:m`, `hid`, and `fsp-srv` (Exit: `0x777`) |
| **18** | ro:1 Dynamic Linker & Texture Pools | Links secondary NRO via `ro:1` `LoadNro` and binds `NvnTexturePool` BC7 descriptors (Exit: `0x888`) |
| **19** | Nintendo SDK (`nnSdk`) C++ Runtime | Initializes `nn::mem::StandardAllocator`, `nn::os::SleepThread`, and `nn::fs::OpenFile` (Exit: `0x999`) |
| **20** | Tier-3 JIT Super-Block Execution | Multi-block trace unrolling with fused execution achieving >100 MIPS (Exit: `0xAAA`) |

---

## Getting Started

### Prerequisites

- [Rust Toolchain](https://rustup.rs/) (edition 2024 / rustc 1.80+)
- Supported Host Architectures: `aarch64` (Apple Silicon, ARM64 Linux), `x86_64` (Linux, macOS, Windows)

### Compilation & Execution

```bash
# Clone the official clean-room repository
git clone https://github.com/hffgzktbcc-blip/ounce-core.git
cd ounce-core

# Execute all 41 unit tests
cargo test --release

# Display hardware architecture specifications and engine status
cargo run --release -- info

# Execute the 20-stage full-stack integration verification suite
cargo run --release -- test

# Audit package headers, ISA security features, and extract RomFS assets
cargo run --release -- inspect package.nsp

# Launch interactive Joy-Con 2 input and graphics demonstration
cargo run --release -- play

# Run the Tegra Ampere 3D GPU pushbuffer rasterizer & DLSS benchmark
cargo run --release -- 3d 120

# Load and execute an AArch64 executable or container
cargo run --release -- run payload.nro
```
