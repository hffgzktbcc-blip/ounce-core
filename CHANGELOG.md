# Changelog

Notable changes to ounce-core. Evidence for each release lives under `docs/releases/`.

## Unreleased

### Fixed
- FP results that depended on the host CPU or ignored Arm NaN rules, found by the new FP edge-case experiment: vector
  `FADD/FSUB/FMUL/FDIV` (.4S/.2D) now pick the NaN operand like Arm (signalling first, quieted); invalid operations
  (inf-inf, 0*inf, 0/0, sqrt of a negative) return the positive default NaN on every host (x86 gave the negative one) in
  scalar and vector arithmetic, FNMUL, fused multiply-add and FSQRT; scalar `FRINT*` and `FSQRT` quiet signalling NaNs.

### Added
- `docs/verify/fp-edge` and `.github/workflows/fp-edge.yml`: a fixed 389,556-case floating-point edge corpus (NaNs, denormals,
  signed zeros, infinities, rounding modes, FZ/DN, conversions, FMA, min/max) run on real AArch64 hardware and replayed through
  the interpreter, Tier-3 and Tier-4, with raw per-case results kept as CI artifacts. `fuzz_host` now also builds on Linux
  AArch64; `fuzz-replay` accepts `TF <fpcr> ...` trace lines and `--results FILE`.
- `FMAXP`/`FMINP`/`FMAXNMP`/`FMINNMP` (vector, 2S/4S/2D), `FMAXV`/`FMINV`/`FMAXNMV`/`FMINNMV` (4S) and the scalar pairwise `FMAXP` family (`Sd, Vn.2S` / `Dd, Vn.2D`) are decoded and honour FPCR flush-to-zero and default-NaN like `FMAX`/`FMIN`; before they were undefined instructions. Unit tests also pin the `.2D` forms under FZ/DN. The half-precision (FEAT_FP16) forms are decoded too: `.4H`/`.8H` three-same and pairwise, `FMAXV` family on `.4H`/`.8H`, scalar pairwise `.2H` and scalar `FMAX/FMIN/FMAXNM/FMINNM Hd, Hn, Hm`; halves flush denormals under FPCR.FZ16 (not FZ) and honour DN. Checked only against the ARM pseudocode (unit tests), not replayed on hardware.

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

### Fixed
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
