# Fixture and source provenance register

Maintained by the project. Status: **draft, not legally reviewed.** Where a fact is unknown it says so.

## Test fixtures (`tests/fixtures/nro/*`)

Each listed fixture directory has a `main.c`, a `Makefile`, and a prebuilt `.nro`. The source-to-upstream comparison has not been completed, so authorship is unverified.

| Fixture | Source authorship | Build information recorded | Binary contents verified | Ships in repo |
|---|---|---|---|---|
| hello, services, romfs, nvdrv, gpu, input, threads, audren | unverified; upstream example comparison pending | devkitA64 r30 / GCC 16.1.0-1, newlib 4.6.0.20260123-4, libnx 4.12.0-1 (see toolchain register) | no; Makefiles link libnx, and toolchain metadata identifies newlib; binary-level component inventory is pending | yes (binary and source) |
| deko3d | unverified; upstream example comparison pending | devkitA64 + libnx + deko3d per fixture Makefile; shader compiler/version record pending | no; binary-level component inventory is pending | yes (binary and source) |

Notes:
- The fixture Makefiles explicitly link libnx; the recorded devkitA64 toolchain includes newlib. This does not establish the complete contents of each prebuilt binary. No libstdc++ linkage has been verified. See `THIRD_PARTY.md` for upstream notice references and open checks.
- `romfs.nro` embeds `romfs/greeting.txt`; its source authorship has not been separately checked.
- The project records these fixtures as synthetic homebrew test programs; source-to-upstream comparisons and binary-content checks remain pending.
- Exact per-fixture binary IDs and hashes are recorded in `tests/fixtures/nro/TOOLCHAIN_VERSIONS.md`.

## External corpus programs (PLAN step 2; built in CI, never committed)

The 20 outside programs whose first frames are oracles. Sources are fetched at the pinned commit and
built unchanged with the `devkitpro/devkita64` Docker image; neither source nor NRO binaries enter
this repository or its CI artifacts. Manifests: `tests/corpus/manifest.json` (original 4) and
`tests/corpus/oracle-builds/*.json` (16 added in step 2). Licences are as declared upstream at the
pinned commit; not legally reviewed. The oracles' glyph model reads libnx 4.12.0's
`nx/data/default_font.bin` (ISC) at generation time; the font is not committed.

| program | repository | commit | path | licence observed |
|---|---|---|---|---|
| `simplegfx` (original 4) | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/simplegfx` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `deko-basic` (original 4) | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/deko3d/deko_basic` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `romfs` (original 4) | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `fs/romfs` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `startup` (original 4) | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `templates/application` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `printing-hello-world` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/printing/hello-world` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `vt52-demo` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/printing/vt52-demo` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `read-controls` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `hid/read-controls` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `touch-screen` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `hid/touch-screen` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `sixaxis` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `hid/sixaxis` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `audren-simple` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `audio/audren-simple` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `get-system-language` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `settings/get_system_language` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `barrier` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `misc/barrier` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `user-events` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `misc/user_events` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `user-timers` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `misc/user_timers` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `deko-console` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/deko3d/deko_console` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `deko-examples-menu` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/deko3d/deko_examples` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `sdl2-simple` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/sdl2/sdl2-simple` | Public Domain Mark 1.0 (README notice; linked libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `hybrid-app` | fincs/hybrid_app | `8c43d74b41740703b732783fa35f9d3fa63d1b2e` | `.` | zlib (`COPYING`, Copyright (c) 2019 fincs) |
| `nx-hbmenu` | switchbrew/nx-hbmenu | `089fdf85ff82ad7077527092f8615f2e57ffc985` | `.` | ISC (`LICENSE.md`, nx-hbmenu Authors) |
| `raufang-hello-cpp` | Raufang/switch-hello-world | `fbe545bad4eb9b188990f66b0315b697ad74371a` | `.` | MIT (`LICENSE`, Copyright (c) 2025 Raufang) |

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

Commits before `76ce766` (including `c7f16f2` and `4be9640`) were previously identified by the project as containing firmware-derived or third-party-derived byte sequences and an external-payload loader; their contents have not been re-audited in this documentation change. On 2026-10-28, this checkout confirmed `c7f16f2` is reachable from `main`, tags `v0.3.0` through `v0.3.2`, and fetched pull-request refs `72`, `81`, `82`, and `83`. This limited check does not establish reachability from every branch, tag, fork, clone, or server-side ref. A history cleanup has been discussed, but no completion is verified here. Tracked in `AGENTS.md`.

## Audits

- Cargo licence metadata inventory: recorded 2026-10-28; it covers declared Cargo metadata, not source texts or fixture binaries. See `docs/DEPENDENCY_LICENSE_AUDIT.md`.
- Shader tooling (`uam` compiles deko3d test shaders; version, licence notices, and generated-output inventory are unrecorded).
- Fixture source authorship: compare each `main.c` with relevant libnx/deko3d examples and record the result.
