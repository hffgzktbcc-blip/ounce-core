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

## External corpus programs (built in CI)

Sources are fetched at pinned commits and built unchanged with the `devkitpro/devkita64`
Docker image. Source and NROs stay out of the repository. CI transfers permitted NROs to the
native job in a one-day artifact. Build manifests are under `tests/corpus/`. Licences are as declared upstream at the
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

### Step 7 Switch examples intake

The 30 added cases use the same public `switchbrew/switch-examples` revision
`669786898205b7beb25ff1731e72982e6d0397d3` and its Public Domain Mark 1.0 README notice.
The paths are registered in `tests/corpus/oracle-builds/switch-examples.json`.
The CPU JIT sample uses the already pinned `jit` build in `tests/corpus/manifest.json`.
The new programs link libnx (ISC) and the devkitA64 standard runtime. CI retains each build's
source and binary hashes, compiler details, and the container package inventory.

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

## SASS encodings: sources read and cross-checked (`src/gpu/sass`, PLAN step 5)

Read 2026-11-03. Field positions and small semantic formulas were taken from these MIT sources and
re-written in our own code; their notices are in `THIRD_PARTY.md`.

| Source | Version read | Used for |
|---|---|---|
| Mesa NAK `src/nouveau/compiler/nak/sm70_encode.rs`, `nvdisasm_tests.rs`, `from_nir.rs`, `sph.rs`, `nak_nir_lower_fs_inputs.c` (MIT) | mesa `f4a706c5a218cbee80caef7f40cd511a6fa46b36` | Ampere `ALD`/`AST`/`IPA` fields, `IPA` frequency codes (PASS 0, CONSTANT 1, STATE 2; no PASS_MUL_W on SM70+), `PLOP3` negate bits and second table (checked by NAK against `nvdisasm`), `MUFU` function width and operand forms, `TEX`/`TLD` flag bits, SM70+ `MUFU.SIN`/`COS` input scaling, 32-word SPH v4 for SM 7.3+ |
| Mesa NAK `src/nouveau/compiler/nak/sm50.rs` (MIT) | same | independent check of the Maxwell fields already read from envytools; `RRO` encoding |
| envytools `envydis/gm107.c` (MIT) | `master`, fetched 2026-11-03 | Maxwell `XMAD` signedness/mode tables, `LOP`/`LOP3` predicate-output fields, `TEX` LOD modes |
| Ryujinx `Ryujinx.Graphics.Shader/Instructions/InstEmitIntegerArithmetic.cs` (MIT), read from an archive copy | as archived | `XMAD` semantics: `.CHI`/`.CLO` addends zero-extended regardless of operand signedness, `.CBCC`/`.MRG` use the unmodified B, `.CSFU` formula |

Unresolved disagreements (we keep the reading with real-word evidence): NAK's SM50 bound `TEX` opcode
`0x0380` against the CuAssembler-checked `0xc038`; the Maxwell `FSET` constant-buffer form's b negate bit
(NAK bit 6, envytools bit 53).

## Switch 2 homebrew SDK

The split-image loader follows the public MIT-licensed
[HBLauncher app builder](https://github.com/ravensclawx/HBLauncher/blob/18b4748ba98eeac7df17beb5d3418985eb74897c/sdk/tools/hbx-build.py)
at commit `18b4748ba98eeac7df17beb5d3418985eb74897c`. The build report supplies
segment addresses, sizes, entry point, and SHA-256 hashes. The first-frame
oracle uses the pinned outside Hello CPU sample's drawing calls.

## Ported code

MIT code ported into this repository under the "Sources" rules of `PLAN.md`. Notices are in `THIRD_PARTY.md`.

| Ported into | Upstream | Licence | What was taken |
|---|---|---|---|
| `crates/ounce-nce/src/patch.rs` | [seedds/nxnative](https://github.com/seedds/nxnative) `dm/tools/patch_nso.py` and `dm/tools/nso_asm.py`, read at commit `abd5ac2cce8e164f455188e4984281d14161d4cb` | MIT | The native-execution rewrite rules (svc and timer reads to host gates, `TPIDR_EL0`/`TPIDRRO_EL0` to a thread context, `x18` users to a scratch register backed by the context: `site_gate`, `site_sysreg_read`, `site_tpidr_write`, `site_plain`, `site_sp`, `site_cond`, `site_pcrel`, `site_blr`), the instruction encoders, and the 16 KiB host page sealing approach (`dm/host/loader.c`). Re-written in Rust; game-specific parts (registered cuts, spin gates, engine fixes) not taken. |

## Known provenance problem: git history

Commits before `76ce766` (including `c7f16f2` and `4be9640`) were previously identified by the project as containing firmware-derived or third-party-derived byte sequences and an external-payload loader; their contents have not been re-audited in this documentation change. On 2026-10-28, this checkout confirmed `c7f16f2` is reachable from `main`, tags `v0.3.0` through `v0.3.2`, and fetched pull-request refs `72`, `81`, `82`, and `83`. This limited check does not establish reachability from every branch, tag, fork, clone, or server-side ref. A history cleanup has been discussed, but no completion is verified here. Tracked in `AGENTS.md`.

## Audits

- Cargo licence metadata inventory: recorded 2026-10-28; it covers declared Cargo metadata, not source texts or fixture binaries. See `docs/DEPENDENCY_LICENSE_AUDIT.md`.
- Shader tooling (`uam` compiles deko3d test shaders; version, licence notices, and generated-output inventory are unrecorded).
- Fixture source authorship: compare each `main.c` with relevant libnx/deko3d examples and record the result.

## Controller color availability (2026-11-06)

The HID color attributes and shared-memory layout follow libnx 4.12.0
`include/switch/services/hid.h` (`HidColorAttribute`, `HidNpadFullKeyColorState`,
`HidNpadJoyColorState`). Connected devices without host-provided color metadata
report ReadError, disconnected devices report NoController, and supplied metadata
reports Ok. No runtime implementation code was copied.

The runtime regression workload is unchanged switchbrew/switch-examples
`hid/colors` at `669786898205b7beb25ff1731e72982e6d0397d3`, built locally with
devkitA64 GCC 16.1.0, newlib 4.6.0.20260123 and libnx 4.12.0. The existing registered
frame expectation is unchanged. Package digests were checked against the official
devkitPro repository databases; toolchain and binaries remain outside the repository.

### Local account and libnx audio device sessions (2026-11-07)

`src/kernel/account.rs` and `src/kernel/audio_device.rs` implement the message
layouts described by public switchbrew/libnx sources at commit
`b156b4774fb2df7b93402169509266da7547fc4c` in `nx/source/services/acc.c`,
`applet.c`, `audin.c`, `audout.c` and their public headers. These are protocol
references. The implementation uses ounce-core's existing CMIF and memory APIs.

The account is an offline runtime-local identity with the nickname Ounce User.
The headless audio input supplies silence. Device buffers complete synchronously,
output PCM enters the existing audio queue, and completion events track released
buffer tags. First-frame passes establish only the registered output behavior.
They do not establish microphone capture, real-time audio pacing or complete
account and savedata service coverage.

## Provenance remediation evidence update

The local corrective work and scoped history proposal are consolidated in
`PROVENANCE_REMEDIATION_REPORT.md`. All eleven committed NROs rebuilt
byte-for-byte from their tracked inputs. `tests/fixtures/provenance.json` now
registers every binary and build input, including input_multi, sdmc and deko3d.
`tests/fixtures/rebuild-evidence.json` retains exact build and linker evidence.
Reproducibility is verified. Source authorship and full redistribution
obligations remain unresolved. The fixture identity gate preserves this
explicit legacy baseline and requires review evidence for changes.
