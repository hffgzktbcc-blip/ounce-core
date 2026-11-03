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

## Known provenance problem: git history

Commits before `76ce766` (including `c7f16f2` and `4be9640`) were previously identified by the project as containing firmware-derived or third-party-derived byte sequences and an external-payload loader; their contents have not been re-audited in this documentation change. On 2026-10-28, this checkout confirmed `c7f16f2` is reachable from `main`, tags `v0.3.0` through `v0.3.2`, and fetched pull-request refs `72`, `81`, `82`, and `83`. This limited check does not establish reachability from every branch, tag, fork, clone, or server-side ref. A history cleanup has been discussed, but no completion is verified here. Tracked in `AGENTS.md`.

## Audits

- Cargo licence metadata inventory: recorded 2026-10-28; it covers declared Cargo metadata, not source texts or fixture binaries. See `docs/DEPENDENCY_LICENSE_AUDIT.md`.
- Shader tooling (`uam` compiles deko3d test shaders; version, licence notices, and generated-output inventory are unrecorded).
- Fixture source authorship: compare each `main.c` with relevant libnx/deko3d examples and record the result.
