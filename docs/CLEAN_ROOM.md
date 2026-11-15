# Sources and legal guardrails

Status: draft, not legally reviewed. This replaces the earlier clean-room protocol (owner decision 2026-11-03).
ounce-core is **no longer a clean-room project**: it may port MIT-licensed emulator code, with credit.

## Allowed sources
Cite each one in `docs/PROVENANCE.md` when used.
- **Arm Architecture Reference Manual** (ARMv8/9 A-profile): CPU semantics and encodings.
- **Mesa NAK** (MIT) and **Nouveau/envytools**: SASS encodings and GPU classes. MIT code may be ported with its
  notice kept in `THIRD_PARTY.md`.
- **libnx, deko3d, devkitPro examples** (public homebrew source).
- **nxnative and Ryujinx** (MIT): may be read and ported to Rust. Keep their notices in `THIRD_PARTY.md` and
  record each port in `docs/PROVENANCE.md`.
- **Libraries:** wgpu/naga, SPIRV-Cross, standard crates.
- **Observation** of outside homebrew running in our runtime, and reference runs of outside homebrew (never
  retail games) in other emulators, with emulator and version recorded in `docs/PROVENANCE.md`.

## Not allowed
- GPL emulator code (yuzu, Eden, Eglantine and similar).
- Code, logs or traces from non-public projects.
- Retail games, game profiles or game-specific patches, keys, firmware, system archives, dumps, leaked material,
  Nintendo SDK code, or anything derived from them.

## Repository boundaries

Do not commit keys, retail content, firmware, dumps, leaked/private material, or
incompatible licensed code. Do not distribute artifacts with unresolved rights.
Key handling and encrypted-container decryption are out of scope; `scripts/check_legal_guardrails.sh` fails if they reappear (see `docs/CAPABILITY_SCOPE.md`). The existing
legal and unsafe-boundary checks remain in CI; they do not require repeating a
full runtime build to save ordinary work.

Record source, commit/hash, licence and build recipe for new redistributed
fixtures. Preserve port notices in THIRD_PARTY.md. Focused synthetic tests and
permitted outside workloads are both valid; use evidence appropriate to the claim.

The historical provenance issues in docs/PROVENANCE.md remain unresolved until
specifically dispositioned. Apply those restrictions to the affected materials,
not unrelated tooling or ready tasks. See root AGENTS.md for the active workflow.
