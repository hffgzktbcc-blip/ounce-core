# Changelog

Notable changes to ounce-core. Evidence for each release lives under `docs/releases/`.

## Unreleased

### Fixed
- `run-nro` and `run` refuse a guest argument string over 62,855 bytes with a `[Loader Error]` and exit 1 before the
  program starts (#418). The string is the program path (`sdmc:/<file>`) and the guest arguments joined by spaces, so a
  longer path leaves less room for the guest arguments. Before, a longer string overwrote the program's first
  instruction. The limit is the 64 KiB between the homebrew config block and the NRO load address, less the 5 config
  entries and the 2,560 bytes of next-load buffers.

### Changed
- `--present=headless` (or `OUNCE_PRESENT=headless`) opens no host audio device, so a run on a machine with no audio output
  no longer prints ALSA or `[HostAudioSink]` errors to stderr (#419). Guest audio is discarded in that mode. The other
  modes (`auto`, `gpu`, `minifb`) still open the default output device as before. Documented in `docs/DETAILS.md` and
  `--help`.

## 0.8.0

### Removed
- Key handling and encrypted-container support (NCA/XCI, `prod.keys` parsing). Ounce-core is homebrew-only; a CI guard
  (`scripts/check_legal_guardrails.sh`) rejects reintroducing it.

### Added
- The workbench and the Mac app check static AArch64 Linux ELF programs as well as `.nro` files; real-program runs are
  recorded under `docs/mac-app-real-programs/`.
- Contributor bots: welcome, claim and gentle-nudge workflows, plus `scripts/outside_activity.py` for drafting replies.

### Changed
- Report This Problem, Copy details and Help > Report a Problem now fill in every field the app knows: program name,
  size and SHA-256, the engine, what was compared and the expected output fingerprint, the stop reason (an unimplemented
  instruction keeps its word and address), the command to reproduce, and the OS, chip and Ounce Core version and commit.
  The issue form says nothing is sent until you press Submit and asks you to search open issues first. See
  `docs/WORKBENCH_UI.md`.
- The Mac app is one window, the workbench. The older launcher window is gone. The menu bar is File (Check a Program, Open
  Recent, Open Saved Results), View (with Advanced inside it) and Help (Report a Problem, Documentation). Run in a
  Window, Graphics Demo, Run Settings, SD Card Folder and the session log are under View > Advanced. Verify Against Host
  Reference is gone: the page's "Compare with the recorded reference" does it. A program can be dropped anywhere on the
  window or on the Dock icon. Clicked through on a Mac; see `docs/MAC_APP.md` for what was and was not seen.
- The workbench page follows the approved mockup: heading "Check whether a program behaves the same everywhere", verdicts
  "Results match / Results differ / Could not run this program", a chosen-program row with "Choose another", a progress
  state, and Save Report….
- Saved results and the app's results window use the same words as the page (this Mac, Results match) instead of
  "Tier-3 and Tier-4 agree".
- Dialogs and file panels in the app are sheets, so Quit is never blocked by one.

### Added
- FPSR cumulative exception flags (IOC, DZC, OFC, UFC, IXC, IDC) are set by FP arithmetic, conversions, FRINT*, compares and FMAX/FMIN in the interpreter and Tier-3, scalar and vector (`src/cpu/softfp.rs`, `Cpu::step`). FP arithmetic now always goes through the soft-float path, so interpreter FP is slower than before (not measured). Tier-4 compiled FP does not set the flags until the program first accesses FPSR; after that FP arithmetic and compares run in the interpreter. The fp-edge gate now compares flags (0 mismatches over 294,156 cases per engine against qemu-user; hardware left to CI). `tools/testfloat/gen_trace.py` emits flags, all four rounding modes, conversions, FRINT* and compares, with a CI job (`testfloat` in `fp-edge.yml`) that has not run yet.

### Fixed
- FPCR rounding mode, flush-to-zero and default-NaN are now honoured by ordinary FP arithmetic (#363): scalar and vector `FADD/FSUB/FMUL/FDIV/FNMUL/FMULX/FABD`, `FMLA/FMLS` and `FMADD` family, `FSQRT`, `FRINT*`, `FCVT` single/double, `SCVTF/UCVTF/FCVTZ*` and FP compares, through a pure-integer soft-float (`src/cpu/softfp.rs`) used only when one of those FPCR fields is set. The fp-edge corpus shows 0 mismatches at all nine FPCR values against a qemu-user reference (was 36,941); hardware runs are left to CI. Tier-4 defers to the interpreter while those FPCR fields are set. FPSR exception flags and half-precision arithmetic are still not modelled.

## 0.7.0

### Changed
- `ounce-core diverge` now traces Tier-3 and Tier-4 as well as the interpreter (one-instruction slices), reports a tier
  it could not run as NOT CHECKED, and adds `--engine`; the JSON report gains `engines` and `not_checked`.
- CI: notices and provenance records under `tests/fixtures/` no longer trigger the Rust jobs; a run that fails only
  because hosted-runner jobs were cancelled is re-run once (`ci-rerun-infra.yml`, never for a real failure); an
  advisory job builds the Mac app bundle and checks a program through the bundled runtime.
- A long-running soak workload joins the differential suite (`docs/verify/soak-harness`), byte-identical to the host
  on Tier-3 and Tier-4.

### Added
- `run-elf` answers `sched_getaffinity`; the pinned xxHash, cJSON and zlib harnesses run as static musl ELFs and match a host build on Tier-3 and Tier-4 (joining LZ4).
- TestFloat vectors for `fuzz-replay`; every committed `.nro` replayed on the host CPU (22k instructions, 0 disagreements).
- LodePNG joins the differential suite (`docs/verify/lodepng-harness`): 15 colour formats, all filter strategies, Adam7, deflate settings, corrupt files, zlib and the colour converter; byte-identical to the host on Tier-3 and Tier-4.
- stb_image joins the differential suite (`docs/verify/stb-image-harness`): generated PNG (all filters and depths), BMP and JPEG inputs plus corrupt files, with the NEON JPEG path active; byte-identical to the host on Tier-3 and Tier-4.
- heatshrink joins the differential suite (`docs/verify/heatshrink-harness`): 7 window/lookahead settings, chunked and byte-at-a-time streaming, malformed streams; byte-identical to the host on Tier-3 and Tier-4.
- tiny-AES-c joins the differential suite (`docs/verify/tiny-aes-harness`): NIST AES-128 ECB/CBC/CTR vectors plus generated buffers; byte-identical to the host on Tier-3 and Tier-4.
- QOI joins the differential suite (`docs/verify/qoi-harness`): 360 generated images encoded, decoded and fuzzed with malformed streams; byte-identical to the host on Tier-3 and Tier-4.
- QuickJS joins the differential suite (`docs/verify/quickjs-harness`): 46 scripts covering numbers, BigInt, Unicode, regexps, JSON, classes, generators, typed arrays and error paths; byte-identical to the host on Tier-3 and Tier-4 (about 95 s per engine).
- The workbench shows an "Instruction check" card when "Compare with this computer" is on (AArch64 hosts): how many sampled register-only instructions were replayed on this CPU and the first disagreement, if any. Same method and limits as `ounce-core diverge`.
- `ounce-core diverge <program.nro>` replays a sample of a program's register-only instructions on this machine's CPU and names the first one where Ounce-Core disagrees: program counter, opcode, register, both values. AArch64 hosts only. See `docs/DIVERGE.md` for what it does not cover.
- The Mac app now opens the workbench in its window (`tools/macos-launcher/Workbench.swift`, a `WKWebView` around the
  local `ounce-core ui` server, started from the bundled runtime). Opening an `.nro` or "Check a Program…" (⌘O) sends it
  to the workbench, emulator-only; the Mac reference stays a choice made in the page. The server exits when the app does
  (`ui --parent-pipe`). The earlier launcher window is still there under "Show Classic Launcher" and
  "Run in a Window (Classic)…".
- `ounce-core ui` is now a real local workbench, not a printout. It serves a page on 127.0.0.1 (random port,
  per-session key, Host/Origin checks, 64 MiB upload cap). Drop a homebrew `.nro` and it runs on Tier-3 and
  Tier-4 in separate processes, and, only if you turn on "Compare with this Mac" and confirm, on this Mac's CPU
  as the hardware reference. It compares how each run stopped, its exit code, every file written to `sdmc:/` and
  the guest's console text, names the first differing line, and says plainly that this is one run, not a proof.
  Results are saved under `~/Library/Application Support/Ounce Core/Checks` (macOS) or `~/.ounce-core/checks`.
  `site/lab.html` stays a scripted design preview.
- Scalar `FADDP Dd, Vn.2D` / `Sd, Vn.2S` and scalar `FMUL/FMLA/FMLS/FMULX` by element; `run-elf` answers `mremap` for anonymous mappings.
- Ten more programs run as static Linux ELFs and match host, qemu-user, Tier-3 and Tier-4: BLAKE3 (NEON), crypto-algorithms, FastLZ, tinf, jsmn, SHA3, picohttpparser, inih, CoreMark, KISS FFT; Monocypher, Lua, QuickJS, SQLite and four more existing harnesses now also run as ELFs. `verify_linux_elf.py --qemu` adds qemu-user as an independent reference. Summary and limits (wasm3 does not run as an ELF yet): `docs/verify/linux-elf/PROGRAMS.md`.
- `docs/verify/fp-edge` and `.github/workflows/fp-edge.yml`: a fixed 389,556-case floating-point edge corpus (NaNs, denormals,
  signed zeros, infinities, rounding modes, FZ/DN, conversions, FMA, min/max) run on real AArch64 hardware and replayed through
  the interpreter, Tier-3 and Tier-4, with raw per-case results kept as CI artifacts. `fuzz_host` now also builds on Linux
  AArch64; `fuzz-replay` accepts `TF <fpcr> ...` trace lines and `--results FILE`.
- `FMAXP`/`FMINP`/`FMAXNMP`/`FMINNMP` (vector, 2S/4S/2D), `FMAXV`/`FMINV`/`FMAXNMV`/`FMINNMV` (4S) and the scalar pairwise `FMAXP` family (`Sd, Vn.2S` / `Dd, Vn.2D`) are decoded and honour FPCR flush-to-zero and default-NaN like `FMAX`/`FMIN`; before they were undefined instructions. Unit tests also pin the `.2D` forms under FZ/DN. The half-precision (FEAT_FP16) forms are decoded too: `.4H`/`.8H` three-same and pairwise, `FMAXV` family on `.4H`/`.8H`, scalar pairwise `.2H` and scalar `FMAX/FMIN/FMAXNM/FMINNM Hd, Hn, Hm`; halves flush denormals under FPCR.FZ16 (not FZ) and honour DN. Checked only against the ARM pseudocode (unit tests), not replayed on hardware.

### Fixed
- FPCR rounding mode, flush-to-zero and default-NaN are now honoured by ordinary FP arithmetic (#363): scalar and vector `FADD/FSUB/FMUL/FDIV/FNMUL/FMULX/FABD`, `FMLA/FMLS` and `FMADD` family, `FSQRT`, `FRINT*`, `FCVT` single/double, `SCVTF/UCVTF/FCVTZ*` and FP compares, through a pure-integer soft-float (`src/cpu/softfp.rs`) used only when one of those FPCR fields is set. The fp-edge corpus shows 0 mismatches at all nine FPCR values against a qemu-user reference (was 36,941). The bounded corpus also passed on Linux Arm and Apple Silicon hardware in CI run 42971358680; see docs/verify/fp-edge/README.md for supported and unsupported case counts. Tier-4 defers to the interpreter while those FPCR fields are set. FPSR exception flags and half-precision arithmetic are still not modelled.
- `FMAX`/`FMIN`/`FMAXNM`/`FMINNM` now honour FPCR flush-to-zero and default-NaN (scalar and vector). Before, 1,208 of 4,800 replayed operand sets disagreed with the host CPU when either was set; now none do. Tier-4 no longer compiles scalar FMAX/FMIN natively so every engine takes the same path.
- `ounce-nce`'s arena test no longer fails when another test thread claims the address it just released.
- Workbench on Windows keeps checks under `%LOCALAPPDATA%\Ounce Core\Checks` (it used a temp folder) and opens the browser with `start`. On Linux/Windows the wording says "this computer"; a closed stdout no longer kills the server. The Mac launcher records Stop Session in the session log.
- `run`/`run-nro --cpu=tier3|tier4` (or `OUNCE_CPU`) now runs the guest on the selected block engine. Before, the
  flag was ignored for homebrew programs: every run used the one-instruction interpreter, so the "Tier-3" and
  "Tier-4" columns of the NRO differentials had not exercised those engines. With no flag the default path is
  unchanged. Measured on a 200-iteration mixed integer/float/heap workload: interpreter 52 s, Tier-3 36 s,
  Tier-4 16 s, output byte-identical to the host. All 15 differential harnesses still match on every engine.
- CLI help and profile banners print the crate version instead of a hardcoded `v0.3.0`.
- The qc-flag harness masks FPSR[31:28], which are RES0 without FEAT_AA32 (resolves #257).
- `LDPSW` now sign-extends both loaded words (it was decoded as a 32-bit `LDP`); this made Monocypher's X25519 differ on every engine.
- FP results that depended on the host CPU or ignored Arm NaN rules, found by the new FP edge-case experiment: vector
  `FADD/FSUB/FMUL/FDIV` (.4S/.2D) now pick the NaN operand like Arm (signalling first, quieted); invalid operations
  (inf-inf, 0*inf, 0/0, sqrt of a negative) return the positive default NaN on every host (x86 gave the negative one) in
  scalar and vector arithmetic, FNMUL, fused multiply-add and FSQRT; scalar `FRINT*` and `FSQRT` quiet signalling NaNs. Tier-4 scalar `FADD/FSUB/FMUL/FDIV/FNMUL` follow the same NaN rules on non-AArch64 hosts.

## 0.6.1

### Fixed
- ASTC texture decoding: the bilinear weight `w00` underflowed in `u32` when
  `fs + ft > 16`, panicking debug builds (`compressed_formats_match_reference_vectors`).
- Tier-4: an instruction the lowerer rejects no longer leaves partially emitted IR in the
  block; the block is rolled back so the Tier-3 fallback sees no partial effect. Replaces a
  debug assertion that failed in `random_blocks_match_interpreter_and_tier3` and
  `random_linked_loops_match_interpreter_and_tier3` with `--features native-jit`.
- CI: `actions/setup-python` 5 → 7 and `actions/dependency-review-action` 4 → 5 (supersedes Dependabot #275 and #274).
- Default and `nce native-jit` builds are free of compiler warnings.

## 0.6.0
Default CLI release for Linux x86-64, macOS arm64 and Windows x86-64. See
`docs/releases/v0.6.0-publication-proof.json` and the
[release page](https://github.com/hffgzktbcc-blip/ounce-core/releases/tag/v0.6.0).
