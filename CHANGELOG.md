# Changelog

Notable changes to ounce-core. Evidence for each release lives under `docs/releases/`.

## Unreleased

- CLI: `--max-frames N` and a repeatable `--press BUTTON@FRAME` (for example `PLUS@60`) let scripted runs
  exit cleanly and press buttons (#458).
- Fuzzer: `--word-mask` / `--word-value` targeted generation, with hardware evidence for FCADD, SQRDMLAH/SQRDMLSH,
  FCVTXN and half-precision FMOV (#457).
- Docs: one roadmap entry point for agents (#461); past handoffs moved under `docs/history/`.
- CI: a `lint` job fails on any rustc or clippy warning (default, `native-jit,nce` and `gpu-present` features, pinned
  toolchain), and an `msrv` job builds default features with Rust 1.89. The tree now has zero warnings.
- Fixed: guest debug output (`svcOutputDebugString`) on `run` / `run-nro` waited in stdout's buffer until the next
  newline, so a hung program's last partial line was lost when an outer timeout killed the run (#476). It is now
  flushed as it is written; `run-elf` already did this.
- CI: the whole-program FP workload (`docs/verify/fp-workload`, 48 ops x 8 FPCR modes, native vs `run-elf` Tier-3
  and Tier-4) runs on a Linux ARM64 runner for pull requests that touch the engines (#485). Any op that is not
  identical on both tiers fails it, and a deliberately edited reference must fail it too.
- `framebuffer_hash` for 3D frames (#486): `--deterministic` / `OUNCE_DETERMINISTIC=1` on `run` / `run-nro` now draws
  3D with the software rasterizer even in a GPU window, so every presented frame is hashed, with the same value a
  headless run reports. Only a `gpu-present` GPU window outside deterministic mode still reports `null`; the schema
  doc says so. Fixed: a `gpu-present` build whose GPU window failed to open sent 3D draws to a GPU that was not
  there, so frames were presented without them; the backend is now opened before that choice is made.
- Fixed: `run` / `run-nro` reported `insns_executed` without the instructions that ran in the slice that ended in a
  fault, so a program that faulted early reported 0 (and the interpreter and Tier-3 could not be compared). A fault
  now counts the instructions that completed before it. A new test faults the fourth instruction of a minimal NRO and
  expects 3 on every tier.
- Supply chain (#480): `release.yml` writes an SPDX 2.3 SBOM for each release target (`scripts/generate_sbom.py`: the
  crates reachable through normal and build dependencies for that target, with declared licenses, crates.io download
  locations and lockfile SHA-256 checksums). It also runs on a manual dry run, is uploaded with the archives and is
  listed in `SHA256SUMS`. Every workflow `uses:` is now pinned to a commit SHA, with `# version` comments for
  Dependabot, and `scripts/check_action_pins.py` fails on a tag or branch; six refs used only by release, pages and
  coverage workflows could not be pinned yet and are listed there by name. `dtolnay/rust-toolchain` steps now say
  `toolchain:` explicitly, which the action requires once pinned.
- Fixed: in `--deterministic` mode on `run` / `run-nro`, guest time advanced only inside system calls, so a thread
  asleep on a timeout never woke while another thread spun on a shared flag without making any; the run ended on the
  instruction budget. Each scheduler time slice (20,000 instructions) now also advances guest time by 1 ms, the same
  on every run and host. Found by review of #479; the 14 instruction budgets did not change.
- `run` / `run-nro` refuse an empty guest argument (exit 2, before the program starts). libnx drops an empty argv
  token even inside quotes, so the program used to see a smaller `argc` without any message. The `--help` text no
  longer says `--deterministic` leaves the homebrew clock alone.
- CI scripts: `insn_budgets.py` rejects a run-elf program that exits non-zero even though it stopped cleanly (its
  short run would pass as an improvement or be recorded as a baseline); the FP workload's `FP_WORKLOAD_OPS` rejects a
  name `--list` does not know and an empty host reference, instead of reporting two empty files as identical.
- `ounce.run.v1` gains `lr` and `sp` (#475, first part): the link register and stack pointer at the stop, set exactly
  when `pc` is, on `run`, `run-nro` and `run-elf`. With `pc` they locate a fault without a debugger: the `BL` that
  made the innermost call is at `lr - 4`. Additive. Symbolication (function names from DWARF/ELF) is not done: it
  needs a new dependency, which is left for the owner to decide.
- `ounce.run.v1` gains `audio_frames_mixed` and `audio_hash` (#478): the stereo frames of audio the guest mixed through
  `audout` or `audren`, and `fnv1a64:` of every sample. They are counted as audio is mixed, so `--present=headless`
  (no sound device) and a playing run report the same values, and a fixed program gives the same hash on every run,
  host and CPU tier. A program with no audio reports `0` and `null`. Additive; `run-elf` writes `0` and `null`.
  Tested against the audren fixture's documented waveform (2,640 frames, and 96,000 in `long` mode).
- CI: instruction budgets (#479). The Runtime job runs 14 committed NRO and ELF fixtures with `--deterministic` on the
  interpreter and Tier-3 and fails when `insns_executed` goes over its budget (baseline + 2%), the stop changes, the
  tiers disagree or a fixture was rebuilt; wall time is shown but never fails. Budgets live in
  `tests/insn-budgets.json` (`scripts/insn_budgets.py --update` records new ones); see `docs/INSN_BUDGETS.md`.
- Fixed: `--deterministic` / `OUNCE_DETERMINISTIC=1` on `run` / `run-nro` left the homebrew kernel on the host clock,
  so a guest that waited on time executed a different number of instructions from run to run (`input.nro` gave
  7,602 or 8,207). The kernel now uses the same fixed clock as the corpus runner: guest time advances per SVC and the
  date is fixed. Tested by `deterministic_runs_execute_the_same_instructions_while_waiting_on_time` (#479).
- CI: `ci-rerun-infra.yml` re-runs a second time when every job that failed the run was cancelled without ever
  reaching a runner, as macOS jobs were on two attempts of run 44144034971. At most two automatic re-runs. A real
  failure is never re-run, and after the first attempt neither is a job that started and was then cancelled.
  Tested by `scripts/test_ci_rerun_infra.py`.
- Docs: the stated minimum Rust version was wrong. The committed lockfile and code need Rust 1.89 for the default
  build (not 1.85); `gpu-present` needs 1.90 and `native-jit` 1.97.
- `run-elf` writes the `ounce.run.v1` report (`--report-json` / `OUNCE_REPORT_JSON`), with `stop_kind`, `pc`, `opcode`,
  `fault_addr` and `svc_id` (the Linux syscall number) filled in, and `silent_fallback` true when Tier-4 fell back. New
  additive field `guest_exit_status` carries the program's own exit status; `exit_code` stays the engine's. `--dump-regs`
  prints PC, X0, D0 and NZCV at the stop (#473). Tests now check the emitted fields and exit-code mapping against
  `docs/OUNCE_RUN_V1_SCHEMA.md`.
- Fixed: `run` / `run-nro` silently dropped every guest argument starting with `-`, including after `--` (#487).
  Everything after `--` now reaches the program unchanged; an unknown option before `--` exits 2 with the corrected
  command; `--gui` after `--` is the guest's, not a window request. `ounce-core run-nro --help` documents this.
- Fixed: `scripts/done.sh '#N'` found nothing on macOS, because `\b` in `git log -E --grep` is a GNU extension (#484).

## 0.9.0 — released 2026-11-16

Published as [v0.9.0](https://github.com/hffgzktbcc-blip/ounce-core/releases/tag/v0.9.0), tagged at `4b63f7a`. The release-infrastructure
PRs are merged, and the Linux ARM64 ELF matrix passed on `main` at `6a4ccb8`
([run 43803024996](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/43803024996)).
The downloaded report records 28/28 harnesses and 76/76 optimization rows passing.
See [Q1 release readiness](docs/releases/v0.9.0.md) for the pre-tag
evidence record. The list below was drafted against `v0.8.2..6a4ccb8` and may not itemise every commit up to the
tag (for example #452 and #456); `git log v0.8.2..v0.9.0` is the full list.

### Added
- `run` and `run-nro` can write an `ounce.run.v1` JSON report with
  `--report-json <path>` or `--report-json=<path>`, or `OUNCE_REPORT_JSON`.
  Guest stdout stays on stdout. The file records schema, engine, version, tier,
  stop kind, exit code, PC, opcode, fault address, SVC id, break result,
  instructions executed, and an optional message (#439). `run-elf` does not
  write this file yet.
- `--audio=null` or `OUNCE_AUDIO=null` skips the host audio device even when a
  window is open. `--present=headless` already skipped it.
- `--deterministic` on `run-elf` uses a fixed guest clock. The same flag and
  `OUNCE_DETERMINISTIC=1` (or `true`) are accepted on every command. They do
  not switch the homebrew kernel clock; that path still follows the host clock
  unless a caller such as the corpus runner sets it itself.
- `scripts/run_linux_arm64_matrix.py` runs the Linux ELF harnesses from
  `docs/verify/differential.json` and writes one JSON report. CI job
  `linux-arm64-elf-matrix` on `ubuntu-24.04-arm` calls it (`differential.yml`,
  #440). `docs/verify/linux-elf/README.md` now calls `run-elf` bounded-verified
  for static musl AArch64 `ET_EXEC` programs.
- Family and education roadmaps under `docs/roadmap/` (#441, #443).

### Fixed
- `inspect` on a file that is not an NRO0 or NSO0 executable, or whose header
  does not load, prints the reason and exits 1. It used to report an unknown
  format and exit 0 (#385, #433). Inspecting a loose directory is unchanged.
- The software-keyboard applet returns a deterministic cancelled result instead
  of deadlocking in `swkbdShow` (#446); interactive text entry is not implemented.
- Scalar FMULX decoding (#445), FCVTXN/FCVTXN2 round-to-odd (#449), vector
  SQRDMLAH/SQRDMLSH (#450), FCADD (#451), and half-precision vector FMOV
  immediates (#444) now have interpreter coverage. These bounded changes do not
  establish general application compatibility.

### Release infrastructure
- The package declares MIT licensing and PR checks run `cargo-deny`.
- Release archives include Linux ARM64; Linux checksum generation covers the
  archives produced by all four release targets (#442).

### Known limits at release
- `silent_fallback` is always `false`. `frames_presented` and
  `framebuffer_hash` are not fields on ounce-core's report.
- Asking for Tier-4 without `native-jit` still runs Tier-3, and the report's
  `silent_fallback` field does not say so.
- This candidate does not publish or merge the synchronized family releases. The
  sibling version bumps and final-candidate CI remain separate publication steps.

## 0.8.2

The tag commit only bumped the version and the notice-review hashes. The
changes below landed between v0.8.1 and that tag and were not listed here at
the time. This section is reconstructed from that git history for review.

### Added
- Static AArch64 Linux ELFs can be compared with `diverge`, and the ELF loader
  and decoder have a fuzzer (#430).
- Twelve measured ARM64 v0.2.0 conformance cases (#427).
- A multi-optimisation matrix and the high-star C programs yyjson,
  stb_truetype, and zstd (#434), with the README and community guide updated
  to match (#436, #437).
- The Growing Family section and banner in the README (#423, #424, #425).

### Fixed
- Tier-4 no longer treats `u64::MAX` as a sentinel in a way that collided with
  a real value (#391). Scalar NaN regression tests were added with that fix
  (#362, #431).
- Decoder accepts the LRCPC2 unscaled atomic loads and stores `LDAPUR` and
  `STLUR` (#426).
- FEAT_SHA3 SIMD bitwise instructions used by the Apple M1 xxHash build (#428).
- Guarded wasm3 Linux ELF reservations and faults (#429).

## 0.8.1

### Removed
- The release workflow no longer builds or attaches a prebuilt Mac app (`ounce-core-macos-arm64-app.zip`). The app stays
  build-from-source. Releases carry the Linux, macOS and Windows command-line archives and `SHA256SUMS`.

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
