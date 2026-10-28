# Fixture and source provenance register

Maintained by the project. Status: **draft, not legally reviewed.** Where a fact is unknown it says so.

## Test fixtures (`tests/fixtures/nro/*`)

Each fixture directory has a `main.c` written for this project, a `Makefile`, and a prebuilt `.nro`.

| Fixture | Source written by | Built with | Third-party code inside the `.nro` | Ships in repo |
|---|---|---|---|---|
| hello, services, romfs, nvdrv, gpu, input, threads, audren | this project (`main.c`) | devkitA64 r30 (GCC 16.1.0-1, newlib 4.6.0.20260123-4) + libnx 4.12.0-1 | **libnx (ISC licence), newlib (BSD-style licences), libgcc/libstdc++ runtime pieces (GCC runtime exception)** | yes (binary and source) |

Notes:
- The `.nro` files statically link libnx and newlib. The README previously said "no code copied from
  third-party projects"; that was inaccurate for these binaries. Licence texts for libnx and newlib must be
  shipped or linked alongside them (see `THIRD_PARTY.md`).
- `romfs.nro` embeds `romfs/greeting.txt`, written for this project.
- None of the fixtures contain firmware, keys, retail code or game data. Anyone can rebuild them from `main.c`.
- Exact per-fixture binary IDs and hashes are recorded in `tests/fixtures/nro/TOOLCHAIN_VERSIONS.md`.

## Synthetic inputs in source

The 25-stage harness (`src/main.rs`) and unit tests use AArch64 instruction words written by hand from the
public Arm architecture documentation, plus containers built at runtime with an in-repo synthetic test key.
No external binaries are read.

## Specifications and public references used for implementation

| Area | Public reference | Used for |
|---|---|---|
| AArch64 | Arm Architecture Reference Manual (public) | decoder, interpreter, JIT |
| Horizon service/SVC behaviour | libnx public headers and source (ISC), SwitchBrew wiki | what homebrew calls and expects |
| GPU methods and layouts | public Nouveau/Mesa class headers, NVIDIA public documentation | Maxwell/Ampere method tables |
| Shader encodings | public SASS research and Mesa sources | translator front ends |
| Block-linear tiling | public Nouveau/Tegra documentation | `src/gpu/tiling` |

Rule: implement from the *behaviour* described, in our own code. Do not paste code or data tables from these
sources. Where a table must match a public layout (for example method numbers), cite the header in a comment.

## Known provenance problem: git history

Commits before `76ce766` (for example `c7f16f2`, `4be9640`) contain firmware-derived and third-party-derived
byte sequences and a loader for an external payload. They are still reachable from `main`, every branch and
tag, forks and clones, and GitHub pull request refs. The current tree is clean; the history is not.
Resolution requires a rewrite or a fresh repository plus a request to GitHub to remove cached commits, and
legal advice on the material already published. Tracked in `AGENTS.md`.

## Audits

- Cargo dependency licences: completed 2026-10-28. See `docs/DEPENDENCY_LICENSE_AUDIT.md`.
- Shader tooling (`uam` compiles deko3d test shaders; its licence and output status are unrecorded).
- Any fixture source derived from libnx or deko3d example code (compare `main.c` files against upstream
  examples and record the outcome).
