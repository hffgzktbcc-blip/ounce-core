# Contributing to ounce-core

Thank you for your interest in contributing to `ounce-core`. 

`ounce-core` is an experimental clean-room research runtime and architecture compatibility layer. Because our primary objective is maintaining strict legal and technical provenance, the provenance bar is high and the scope is intentionally focused.

Please review these guidelines before submitting an issue or pull request.

---

## 1. Provenance: The Hard Requirement

This is a strict clean-room project. Every line of code, test case, and documentation must adhere to the following rules:

* **Public Documentation Only**: Contributions must be derived strictly from publicly available technical documentation:
  - The [ARM Architecture Reference Manual](https://developer.arm.com/documentation/ddi0487/latest/) (ARMv8-A / ARMv8.5-A).
  - The [Vulkan](https://registry.khronos.org/vulkan/) and [SPIR-V](https://registry.khronos.org/SPIR-V/) specifications.
  - Published NVIDIA GPU hardware documentation and architecture whitepapers.
  - Open specifications and appropriately licensed (MIT / Apache-2.0 / BSD) open-source reference implementations.
* **Zero Proprietary Material**: Absolutely nothing derived from leaked, dumped, decompiled, or reverse-engineered proprietary binaries, firmware, system archives, or SDKs.
* **No Keys or Game Content**: No encryption keys (`prod.keys`, `title.keys`), firmware dumps, or game data may be committed, even in unit tests. All test inputs must be synthetic or hand-written directly in source.
* **Developer Certificate of Origin (DCO)**: Sign off all commits using `git commit -s` to certify you have the right to submit the code under the project's MIT license.
* **Specification References**: Include a one-line reference in your PR description indicating where the implemented behavior or instruction encoding is documented in the public specification.
* **Prior Exposure Disclosure**: If you have previously worked with proprietary material for this platform (an SDK, dumped firmware, leaked documentation, or a decompilation project), say so in an issue before contributing. It does not rule out documentation or tooling work, but it does rule you out of the areas that material touched.

---

## 2. Project Scope

* **In Scope**:
  - AArch64 CPU instruction decoding and interpreter execution driven by ARM specifications.
  - Modular dynamic loaders (MOD0, ELF `.dynamic`, relocation tables, Thread-Local Storage).
  - High-level Horizon OS service models (`ipc`, `time:u`, `pl:u`, `hid`, `fsp-srv`).
  - GPU pipeline models (NVN command buffer, SASS transpilation to SPIR-V, texture decompression).
  - Software audio renderer models (`audren:u`, `audout:u`) and host sink integration.
  - Homebrew and synthetic research workloads.
  - Test coverage, synthetic test generators, documentation, CI, and host portability fixes.
* **Out of Scope (By Policy)**:
  - Bootstrapping or compatibility work for commercial retail games.
  - Re-enabling or bypassing retail container decryption (commercial key decryption is disabled by policy in `loader/nca.rs`).
  - Circumventing digital rights management or copyright protection mechanisms.

**Terminology**: Describe `ounce-core` only as a research runtime or compatibility layer, in code, comments, documentation, commit messages, and PR text. Do not describe it as a console or game-playing product.

---

## 3. Verification Gates

Before submitting a pull request, ensure your branch passes all required gates:

1. **Clean Release Build**:
   ```bash
   cargo build --release
   ```
   Must compile with zero errors and zero warnings.
2. **Passing Unit Tests**:
   ```bash
   cargo test --release
   ```
   All unit tests must pass.
3. **24-Stage Integration Harness**:
   ```bash
   cargo run --release -- test
   ```
   Must complete all 24 stages and exit with status code `0xEEE`.
4. **Zero Unsafe Guarantee**:
   ```bash
   grep -rn "unsafe" src
   ```
   Must return zero matches. All code within `src/` must remain pure safe Rust.
   If work genuinely needs `unsafe`, it belongs in a separate, isolated crate under `crates/`, and that must be agreed in an issue first.

---

## 4. Best First Step

Before opening a pull request, **open an issue describing what you want to work on**. This ensures the work is in-scope, avoids duplicate effort across contributors and automated agents, and aligns on implementation design before code is written.

Good starting areas:
- Additional AArch64 instruction decoders in `src/cpu/decoder.rs` from the ARM ARM.
- Synthetic test modules and edge-case unit tests for the dynamic loader and memory manager.
- Platform portability fixes and documentation enhancements.
