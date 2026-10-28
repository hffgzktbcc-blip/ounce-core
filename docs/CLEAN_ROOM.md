# Clean-room protocol

Status: draft, not legally reviewed. This describes how the project works today so that contributors and a
reviewing lawyer can check it against the repository.

## What we are
An independent implementation of an AArch64 execution environment and Horizon-style services, tested on
homebrew programs and synthetic test vectors. It is a research runtime. It does not claim to run commercial
games, and no such compatibility has been established.

## Permitted inputs
1. Public specifications and documentation (Arm manuals, SPIR-V, public NVIDIA and Mesa/Nouveau headers).
2. Open-source homebrew libraries and their public documentation (libnx, deko3d), used under their licences.
3. Programs we write ourselves and build with public toolchains.
4. Observations of our own runtime's behaviour.

## Prohibited inputs
- Firmware, system archives, bootloader or trusted-environment code, or anything derived from them.
- Encryption keys of any kind, including in tests (tests use the in-repo synthetic key only).
- Game content, ROMs, dumps, or assets, including as test data.
- Source code or tables copied from other emulator projects or from leaked or proprietary material.
- Output from tools run against material we are not permitted to hold.

## Working rules
- Every new fixture must have a row in `docs/PROVENANCE.md` before merge: who wrote it, how it was built,
  what third-party code is inside.
- Behaviour that is not publicly documented is implemented only after it is observed from our own test
  programs, and the commit message says so. If something can only be learned from prohibited material, we do
  not implement it.
- Contributors state in the PR that the change follows this protocol. Reviewers (including AI agents) reject
  changes that import prohibited material.
- Retail decryption stays disabled (`LoadError::RestrictedCipher`).

## Honest limits
- A protocol does not make an activity lawful. Interoperability and anti-circumvention law differ by country,
  and this project is published internationally. Obtain advice from a qualified IP lawyer before wider
  promotion or any attempt at commercial-title compatibility.
- The git history problem in `docs/PROVENANCE.md` is not resolved by this document.
