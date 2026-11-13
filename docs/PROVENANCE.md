# Fixture and source provenance register

Maintained by the project. This register records the source provenance, build evidence, and rights status for fixtures, corpus inputs, specifications, ported code, and repository history. Source and licensing guardrails are in [CLEAN_ROOM.md](CLEAN_ROOM.md); licensing and notice distribution details are in [LICENSE_DISPOSITION.md](LICENSE_DISPOSITION.md).

## Status

| Group | Items / Status |
|---|---|
| **Done** | Current tree and 16 release assets scan clean; 11 NROs rebuild byte-for-byte; fixture validator and CODEOWNERS merged; notice packaging lock merged. |
| **Owner items** | 1. GitHub `refs/pull/1/head` retains 23 historical objects: support ticket SENT by owner, awaiting GitHub.<br>2. 72-byte third-party program in wider history: owner decided remove, needs targeted history rewrite or written risk acceptance, deferred until open PRs merge. |
| **Specialist items** | objc2/Apple SDK and Rust runtime notices; `RELEASE_NOTICE_REVIEW.json` reviewer. |

---

## Test fixtures (`tests/fixtures/nro/*`)

All eleven committed NROs have registered binary and build-input identities and byte-identical rebuild evidence. See [the register](../tests/fixtures/provenance.json), [rebuilds and linker evidence](../tests/fixtures/rebuild-evidence.json), [component notices](../tests/fixtures/nro/NOTICES.md), and [toolchain records](../tests/fixtures/nro/TOOLCHAIN_VERSIONS.md).

| Fixtures | Identity/build evidence | Distribution |
| --- | --- | --- |
| `hello`, `services`, `romfs`, `nvdrv`, `gpu`, `input`, `input_multi`, `threads`, `audren`, `sdmc`, `deko3d` | All eleven rebuilds match committed NRO bytes exactly from tracked inputs; selected archive members retained | Present in repository source archives as reviewed test inputs; excluded from runtime binary release archives and the standalone Python toolkit |

`scripts/check_fixture_provenance.py` validates fixture identity against `tests/fixtures/provenance.json` in CI, rejecting unregistered NROs and requiring review evidence for changes.

The RomFS asset identity and all tracked fixture inputs are in the register. Outside RomFS programs and containers under `docs/reproduce/` have separate pinned-source review packets; they are not repository fixtures.

## External corpus programs (built in CI)

Sources are fetched at pinned commits and built unchanged using the `devkitpro/devkita64` Docker image. Both source and compiled NROs remain outside the repository. CI transfers permitted NROs to the native job in ephemeral artifacts. Build manifests are under `tests/corpus/` and `tests/corpus/oracle-builds/`.

| Program | Repository | Commit | Path | Licence observed |
|---|---|---|---|---|
| `simplegfx` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/simplegfx` | Public Domain Mark 1.0 (README notice; links libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `deko-basic` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/deko3d/deko_basic` | Public Domain Mark 1.0 (README notice; links libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `romfs` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `fs/romfs` | Public Domain Mark 1.0 (README notice; links libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `startup` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `templates/application` | Public Domain Mark 1.0 (README notice; links libnx ISC, newlib, deko3d zlib, SDL2 zlib, Mesa MIT) |
| `printing-hello-world` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/printing/hello-world` | Public Domain Mark 1.0 (README notice) |
| `vt52-demo` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/printing/vt52-demo` | Public Domain Mark 1.0 (README notice) |
| `read-controls` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `hid/read-controls` | Public Domain Mark 1.0 (README notice) |
| `touch-screen` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `hid/touch-screen` | Public Domain Mark 1.0 (README notice) |
| `sixaxis` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `hid/sixaxis` | Public Domain Mark 1.0 (README notice) |
| `audren-simple` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `audio/audren-simple` | Public Domain Mark 1.0 (README notice) |
| `get-system-language` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `settings/get_system_language` | Public Domain Mark 1.0 (README notice) |
| `barrier` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `misc/barrier` | Public Domain Mark 1.0 (README notice) |
| `user-events` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `misc/user_events` | Public Domain Mark 1.0 (README notice) |
| `user-timers` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `misc/user_timers` | Public Domain Mark 1.0 (README notice) |
| `deko-console` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/deko3d/deko_console` | Public Domain Mark 1.0 (README notice) |
| `deko-examples-menu` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/deko3d/deko_examples` | Public Domain Mark 1.0 (README notice) |
| `sdl2-simple` | switchbrew/switch-examples | `669786898205b7beb25ff1731e72982e6d0397d3` | `graphics/sdl2/sdl2-simple` | Public Domain Mark 1.0 (README notice) |
| `hybrid-app` | fincs/hybrid_app | `8c43d74b41740703b732783fa35f9d3fa63d1b2e` | `.` | zlib (`COPYING`, Copyright (c) 2019 fincs) |
| `nx-hbmenu` | switchbrew/nx-hbmenu | `089fdf85ff82ad7077527092f8615f2e57ffc985` | `.` | ISC (`LICENSE.md`, nx-hbmenu Authors) |
| `raufang-hello-cpp` | Raufang/switch-hello-world | `fbe545bad4eb9b188990f66b0315b697ad74371a` | `.` | MIT (`LICENSE`, Copyright (c) 2025 Raufang) |

Thirty additional Switch examples are pinned at `switchbrew/switch-examples` revision `669786898205b7beb25ff1731e72982e6d0397d3` and registered in `tests/corpus/oracle-builds/switch-examples.json`. The CPU JIT sample uses the pinned `jit` build in `tests/corpus/manifest.json`.

## Synthetic inputs in source

The test harness (`src/main.rs`) and unit tests use AArch64 instruction words written by hand from public Arm architecture documentation, alongside containers generated at runtime with in-repo synthetic keys. No proprietary or retail binaries are included.

## Specifications and public references

| Area | Public reference | Used for |
|---|---|---|
| AArch64 | Arm Architecture Reference Manual (public) | Decoder, interpreter, JIT |
| Horizon service and SVC behaviour | libnx public headers and source (ISC), SwitchBrew wiki | Homebrew ABI, CMIF message layouts, service protocol flows |
| GPU methods and layouts | Nouveau/Mesa class headers, NVIDIA public documentation | Maxwell and Ampere method tables |
| Shader encodings | Mesa NAK sources, envytools, public SASS research | Shader translator front-ends |
| Block-linear tiling | Nouveau and Tegra public documentation | `src/gpu/tiling` |

Rules for implementation: implement from behaviour described, written in project code. Do not copy binary data tables or proprietary source. Method numbers and hardware structure layouts matching public specifications cite the corresponding header in comments.

### SASS encodings: sources read and cross-checked (`src/gpu/sass`)

Field positions and arithmetic formulas were cross-checked against these MIT sources and re-written in project code. Attributions and license notices are in `THIRD_PARTY.md`.

| Source | Version read | Used for |
|---|---|---|
| Mesa NAK `src/nouveau/compiler/nak/` (MIT) | `f4a706c5a218cbee80caef7f40cd511a6fa46b36` | Ampere `ALD`/`AST`/`IPA` fields, `IPA` frequency codes, `PLOP3` negate bits and tables, `MUFU` function width and operands, `TEX`/`TLD` flags, SM70+ `MUFU.SIN`/`COS` scaling, 32-word SPH v4; Maxwell `XMAD` and `RRO` encodings |
| envytools `envydis/gm107.c` (MIT) | `master` (fetched 2026-11-03) | Maxwell `XMAD` signedness/mode tables, `LOP`/`LOP3` predicate-output fields, `TEX` LOD modes |
| Ryujinx `Ryujinx.Graphics.Shader/` (MIT) | Archive copy | `XMAD` semantics (`.CHI`/`.CLO` zero-extension, `.CBCC`/`.MRG`, `.CSFU` formula) |

### Switch 2 homebrew SDK

The split-image loader follows the public MIT-licensed [HBLauncher app builder](https://github.com/ravensclawx/HBLauncher/blob/18b4748ba98eeac7df17beb5d3418985eb74897c/sdk/tools/hbx-build.py) at commit `18b4748ba98eeac7df17beb5d3418985eb74897c`.

### Account, audio, and controller protocols

- `src/kernel/account.rs` and `src/kernel/audio_device.rs` follow the protocol layouts in public libnx sources (`nx/source/services/acc.c`, `applet.c`, `audin.c`, `audout.c` at commit `b156b4774fb2df7b93402169509266da7547fc4c`). Account is an offline local profile ("Ounce User"); headless audio returns silence and tracks buffer tags.
- Controller color attributes and shared memory layouts follow libnx 4.12.0 `include/switch/services/hid.h`. Workload `hid/colors` is built from pinned `switchbrew/switch-examples` in CI; toolchain and binaries remain outside the repository.

## Ported code

Ported MIT code adheres to the sources policy in [CLEAN_ROOM.md](CLEAN_ROOM.md). Full license notices are maintained in `THIRD_PARTY.md`.

| Ported into | Upstream | Licence | What was taken |
|---|---|---|---|
| `crates/ounce-nce/src/patch.rs` | [seedds/nxnative](https://github.com/seedds/nxnative) `dm/tools/patch_nso.py` and `dm/tools/nso_asm.py` at commit `abd5ac2cce8e164f455188e4984281d14161d4cb` | MIT | Native-execution rewrite rules (SVC and timer reads to host gates, `TPIDR_EL0`/`TPIDRRO_EL0` to thread context, `x18` users to scratch register: `site_gate`, `site_sysreg_read`, `site_tpidr_write`, `site_plain`, `site_sp`, `site_cond`, `site_pcrel`, `site_blr`), instruction encoders, and 16 KiB host page sealing (`dm/host/loader.c`). Re-implemented in safe Rust; game-specific patches omitted. |

## Historical objects and repository state

- **Current tree and release assets:** The current source tree and 16 release assets have been scanned clean for exact matches of suspect byte sequences. All release assets match their recorded SHA-256 digests.
- **`refs/pull/1/head`:** Retains 23 historical objects (21 historical source-file versions containing 12 firmware-labelled byte arrays, plus 2 standalone files). These objects are reachable solely from the host-managed `refs/pull/1/head` reference and are absent from all branches and tags. The repository owner has submitted a support ticket to GitHub requesting removal of this reference and garbage collection, and is awaiting GitHub's response.
- **72-byte third-party program:** A 72-byte compiled test function from `pepsipu/ns2jb` was historically committed in early revisions. The owner decided on 8 November 2026 to remove the program rather than seek an upstream grant. The bytes are absent from current code. Removing them from historical commits requires a targeted history rewrite (or written risk acceptance), which is deferred until open PRs merge to prevent disruptive commit rebases.
- **No global rewrite:** No branch or release tag history has been rewritten.

## Notice packaging and release controls

- **Dependencies:** 169 dependency notices are tracked and hash-bound.
- **DejaVu font:** Full notice bundled in release packaging (`assets/fonts/DejaVu-LICENSE.txt`).
- **Toolchain notices:** Captured from the active build toolchain by `scripts/collect_toolchain_notices.py`.
- **Linux shared libraries:** Package copyright notices captured by `scripts/collect_native_notices.py`.
- **Review gate:** `scripts/package_release.py` enforces reviewer sign-off in `docs/RELEASE_NOTICE_REVIEW.json`.
- **Specialist items:** The macOS build links `objc2` bindings derived from Apple SDKs. The terms and notices require specialist review before prebuilt macOS applications are attached to releases; the macOS app currently builds from source.
