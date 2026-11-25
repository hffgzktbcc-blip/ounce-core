# Changelog

Notable changes to ounce-core. Evidence for each release lives under `docs/releases/`.

## Unreleased

- Added: `AESE`, `AESD`, `AESMC`, `AESIMC`; the SHA-1 and SHA-256 hash and schedule instructions; and `SHA512H`, `SHA512H2`, `SHA512SU0`, `SHA512SU1`. `AT_HWCAP` is now `0x2201fb` and advertises AES, SHA1, SHA2 and SHA512. A native Apple M3 oracle and all three engines agree bit for bit on full AES-128 encrypt/decrypt, SHA-1/SHA-256/SHA-512 of `abc`, and 2,048 deterministic pseudo-random inputs for each of the 18 instructions; Tier-3 and Tier-4 deliberately fall back to the verified interpreter implementation. s2n-bignum at `4685b5c` passes all `397/397` tests on each engine; the 13 formerly skipped AES tests now run and pass.
- Docs: a versioning and release policy in [CONTRIBUTING.md](CONTRIBUTING.md#versioning-and-releases) (what makes a minor or patch release, at most one minor a week, the release gate); [SECURITY.md](SECURITY.md) now supports the newest patch of the newest minor instead of naming `0.10.x`.
- Added: `LDXP`, `LDAXP`, `STXP` and `STLXP` (W and X pairs), so C `__int128` atomics and `portable-atomic` 128-bit loops run. Pairs must be aligned to the whole pair; `LDXP Rt, Rt` (including `XZR, XZR`) stops as undefined, as it raises `SIGILL` on the Apple M3. Known-answer tested against M3 output on the interpreter, Tier-3 and Tier-4 (`lse_kat`).
- Added: `AT_HWCAP` advertises `HWCAP_ATOMICS`, now `0x20193`. Rust and C programs built without `+lse` take the `FEAT_LSE` instructions through outline atomics; `lse_kat` value-checks every LSE form (`CAS{B,H,}`, `CASP`, `LD<op>`, `ST<op>`, `SWP`, `LDAPR`) against the M3.
- Fixed: `--report-json=-` (and `OUNCE_REPORT_JSON=-`) now prints only the `ounce.run.v1` JSON on stdout for `run-elf`, `run-nro` and `run`. Guest output (stdout and stderr) and the VM's progress lines go to stderr, so `json.load(stdout)` works; before, the guest's text came first and broke it. `--report-json=<file>` is unchanged.
- Fixed: `cargo test` through `ounce-core run-elf --` works without `--test-threads=1`. Rust's libtest attempts to spawn a worker thread per test; `ounce-core` refuses `clone`/`clone3` thread flags with `ENOSYS` and emits a one-time note to `stderr`. musl translates `ENOSYS` to `EAGAIN` (`WouldBlock`), which libtest catches to run tests sequentially inline on the main thread. Honest 1-CPU Linux scheduling is completed by `sched_getaffinity`, `sched_setaffinity`, and `getcpu`, while programs requiring real second threads (`std::thread::spawn`) fail clearly with `WouldBlock`.
- Changed: release downloads are built, tested and smoke-tested with `--features native-jit` on Linux x86-64, Linux arm64, macOS arm64 and Windows x86-64, so `--cpu=tier4` works in every download from v0.13.0 (v0.12.0 downloads lack it). The release workflow asserts `engine=Tier-4` with no fallback. The SBOMs, `THIRD_PARTY_NOTICES.txt` and the notice review now cover the 27 extra crates (Cranelift, regalloc2 and others); the owner accepted these licences on 2026-11-25, so `package_release.py --require-reviewed` can publish.
- Changed: `trace-diff --elf` now prints decoded instructions as GNU/LLVM-style AArch64 assembly while retaining the raw opcode and a diagnostic fallback for unsupported variants.
- Docs: fixed relative links in v0.12.0 release notes to absolute GitHub URLs and added absolute-link convention for release notes; added release download checksum verification instructions for macOS and Linux; documented prebuilt binary feature defaults (requesting Tier-4 without `native-jit` falls back to Tier-3) and build instructions; refreshed stale commit references in `QUICKSTART.md` and `LIMITS.md` to `18cf538` (`v0.12.0`); updated README research paper link to the published website.
- CLI: updated `--version` banner to neutral ARM64 Execution & Verification Runtime framing without Nintendo/NVIDIA affiliation, displaying built status of optional engines/features (`native-jit`, `nce`, `host-io`).
- Scripts: `scripts/verify_differential.py` honours `$DEVKITPRO` for the template Makefile, reports skipped engine checks when run without `--nro`, and pruned obsolete TinyExpr reference from `docs/REPRODUCE.md`.
- Fixed: verification docs now distinguish dated FP16/FZ16/FPSR and atomics results from current support, contract docs mark archived sibling repositories as historical context, and `help` / `info` describe Tier-3 and the single-threaded testing runtime accurately.

## 0.12.0 — 2026-11-24 (released)

- Added: `run-elf -- <elf> [args]`, so `[target.aarch64-unknown-linux-musl] runner = "ounce-core run-elf --"` runs `cargo test` binaries (`#557`). The Quick Start and README give the `RUSTFLAGS` macOS needs (`-C linker=rust-lld -C target-feature=+crt-static -C relocation-model=static`) and a recorded run of a three-test crate on an Apple M3.
- Added (Linux syscalls, `#557`): `ppoll`, deterministic `getrandom` (fixed stream, the default), `read`/`readv`/`pread64`/`pwrite64`/`lseek`/`fstat`/`newfstatat` (including `AT_EMPTY_PATH`)/`unlinkat`/`ftruncate`/`fcntl`; a bounded in-memory scratch file system at `/tmp` and `/var/tmp` (64 files, 16 MiB per file, 64 MiB total), `/dev/null` and `/proc/self/auxv`; opt-in `--stdin` and `--stdin=<path>` (descriptor 0 is end-of-file by default; 64 MiB limit); `AT_HWCAP` is `0x20093` (FP, ASIMD, PMULL, CRC32, SHA3), known-answer verified on the interpreter, Tier-3 and Tier-4. The bad-syscall test fixture now uses `bpf` (280).
- Added: scalar FP16 `FRECPE`/`FRSQRTE`/`FRECPS`/`FRSQRTS`/`FABD` and the `FCM*`/`FAC*` compare forms, bit-exact against an Apple M3 over 595,472 cases (`#558`).
- Added: `ounce-core trace-diff` prints a first-divergence readout after its headline: step, pc with `symbol+0xoff` (`--elf program.elf`), decoded instruction, differing registers side by side, and an NZCV/FPSR/FPCR split. JSON gains a `readout` object only with `--elf`; existing fields are unchanged ([docs/DIVERGE.md](docs/DIVERGE.md)).
- Fixed: FEAT_LSE atomics (`LD<op>`, `SWP`, `CAS`, `CASP`) and `LDAPR` were silently executed as plain loads and stores, so programs built with `-mcpu=apple_m1` or any LSE target computed wrong atomics. They are now implemented (`#559`). `LDXP`/`STXP` now stop as unimplemented instead of running wrong.
- Verified: [Big Tech and fintech library runs](docs/verify/bigtech-2026-11-23.md) (23 Nov 2026, Apple M3): BLAKE3 `822/822`, libmpdec `148/148`, s2n-bignum `384` passed with `13` skipped for missing `FEAT_AES`, libwebp `57/57`, simdjson `6/6`, secp256k1 `119/119` on Tier-4 only (Tier-3 and interpreter runs did not finish), SGP4 `962/1,222` lines bit-identical to a macOS-libm oracle. The page lists each engine, flag and caveat.
- Docs: `docs/LIMITS.md` lists the current Linux runner limits (no `FEAT_AES`/`SHA1`/`SHA2`, `HWCAP_ATOMICS` not advertised, `LDXP`/`STXP`, no `futex`) and drops limits fixed above. Headless builds need no ALSA or udev packages (only `--features host-io` does); `ounce-hb`, `ounce-runtime` and `Ounce-Spark` are archived and no longer linked as live tools; agents no longer start from the family roadmap, which is now marked historical.
  Post-release note: that limits summary was accurate when v0.12.0 shipped; `HWCAP_ATOMICS` and pair exclusives were added afterwards.

## 0.11.0 — 2026-11-23 (released)

- Added: typed Rust library API in `src/lib.rs` (`EngineKind`, `RunOptions`, `StopReport`, `StopClass`, `run_elf`, `run_nro`, `#539`) and `--report-json=-` pure JSON stdout output with diagnostics routed to `stderr` (`#537`), plus zero-dep ELF symbolication and frame-pointer backtraces (`#531`).
- Changed: pure-Rust headless default build (`default = []`) with zero C system library dependencies; host audio (`cpal`) and controller input (`gilrs`) are now gated behind opt-in `--features host-io` (`#550`). Enforced workspace `unsafe_op_in_unsafe_fn`, `undocumented_unsafe_blocks`, and `missing_safety_doc` lints (`#538`), and slimmed PR CI / pruned obsolete workflows (`#541`, `#548`).
- Added/Fixed (CPU & Linux syscalls): full scalar FP16 decode and execution (`FADD`/`FSUB`/`FMUL`/`FDIV`/`FNMUL`/`FSQRT`/`FMOV`/`FABS`/`FNEG`/`FCVT`/`FCSEL`/`FCMP`/`FCCMP`, `FMOV H<->W/X`, `#imm`, `FRINT* H`, and `int<->half` conversions with Apple Silicon-verified `FPConvert` `FZ`/`FZ16` behaviour, `#531`, `#553`, `#554`), vector `.4H`/`.8H` `FABS`/`FNEG` (`#543`), `LD2–LD4`/`ST2–ST4`/`LD2R–LD4R` (`#547`), `tkill`/`tgkill` (`128+signal` exit status, `#549`), and `newfstatat` path handling (`#551`).
- Verified (differential sweeps on `baec66b`):
  - Berkeley TestFloat 3e (`ARM-VFPv2`, tininess before rounding): `5,921,280` vectors across all 9 groups (`0` real mismatches vs `qemu-aarch64`; `656` FMA NaN-priority cases match `qemu-aarch64`, 6 sampled confirmed addend-NaN on Apple M3), plus `144/144` `FPCR.FZ`/`FZ16`/`DN` and `908/908` scalar `f16` `FMOV`/`#imm`/fixed-point checks across Interpreter, Tier-3, and Tier-4.
  - Compiler & numerical suites vs `qemu-aarch64` / native references: Csmith 2.4.0 (`2,815` compared across `-O1`–`-O3 -mcpu=apple_m1`, `0` differ; `385` timed out under QEMU), YARPGen v2 (`498/500`; 2 exceeded instruction budget), `gcc.c-torture/execute` (`1,586` at `-O0`, `1,602` at `-O2` and `-O3 -mcpu=apple_m1`; 3 temp-file tests differ), CompCert `test/c` (`69/72`; `knucleotide` needs `stdin`), `PolyBench/C` (`60/60` vs QEMU and native x86), `musl libc-test` (`276` pass, `198/199` math; 48 differ on unsupported threads/files/signals), Kahan `Paranoia` (`4/4`), Shewchuk predicates (`90,000` cases, `0` mismatches), and physics/quantum invariants (`9/9`).
  - Real libraries & VMs: MicroPython (`581/582` `tests/basics` vs QEMU), QuickJS (`5/5` at `-O3 -mcpu=apple_m1`), `libpng 1.6.59` (`168/168`), `libdeflate` (`1,800` round-trips, `0` failures; `1.25M` vs `17.8M` instructions proving `CRC32`/`PMULL`/`EOR3`), `bzip2` (`140/140`), `brotli` (`205/205`), and `PCRE2` (`51/51` patterns, `78` matches).
- Added: 100 more paper checks (102 in all) under `docs/verify/papers/`, with a shared harness (`common/ounce_paper.h`,
  `common/lib.sh`) and `run_all.sh`. They cover ciphers (Speck, Simon, AES, Serpent, Twofish, ChaCha, Ascon and others),
  hashes (SHA-1/2/3, BLAKE2, RIPEMD-160, Skein, xxHash, SipHash and others), random number generators (Mersenne Twister,
  PCG, Threefry and others), number theory, floating-point notes of Kahan and Goldberg, and data formats. Each has a
  `paper.json` listing its source, a negative control and what is not checked. Compares the ARM64 output against both the paper's published numbers and a native host build of the same reference code; Tier-3 only, not in CI. Sources differ in strength, and some compare only a few values: see `docs/verify/papers/README.md`.
- Security: documented the best-effort support policy for `main` and the latest `0.10.x` patch, and recorded the evidence-based review of ten CodeQL cleartext-logging alerts. The alerts are false positives caused by a synthetic offline account response sharing a generic IPC result path with numeric guest/test diagnostics; none of the sinks logs account payloads, host credentials or network identities.
- Repository metadata: declared Rust 1.89 as the package MSRV, added canonical repository/homepage/readme metadata, explicitly disabled crates.io publication, and corrected the README footer to point packaged downloads at GitHub Releases.

## 0.10.0 — 2026-11-20 (released)

- Added: half-precision FP3-source (`FMADD`/`FMSUB`/`FNMADD`/`FNMSUB H`), vector `FMLAL`/`FMLSL`/`FMLAL2`/`FMLSL2`
  (FEAT_FHM), and vector `FCVTN`/`FCVTN2`/`FCVTL`/`FCVTL2` (`4H <-> 4S`) with `softfp::F16` and `FPCR.FZ16` (#469, #515).
  On native Apple M3 silicon (100,000 random fuzzer words, seed 1), executed words increased from 36,345 to 37,909
  (+1,564 words) with 0 mismatches and 0 FPSR flag differences across Interpreter, Tier-3, and Tier-4; `docs/verify/fp-edge/run.sh`
  passes 359,136/359,136 cases with 0 value or FPSR flag mismatches across all 9 FPCR modes (including `FZ16` not raising `IDC`).
- Added: `ounce.run.v1` exit-14 (`unhandled_svc`) telemetry fields `sm_service` and `cmif_cmd_id` (#475, #513), tracking
  the last requested `sm:` service name / CMIF command ID and unimplemented HIPC `(iface, cmd)` in `IpcManager` and
  surfacing them in `ounce.run.v1` and stop diagnostics, pinned by `tests/contract/run-v1-keys.json` and `tests/contract_v1.rs`.
- Added: metadata-only `ounce.repro.v1` regression corpus (#472, #513): 10 JSON manifests in `tests/repro/` (zero binaries)
  covering `audren`, `deko3d`, `gpu`, `hello`, `input`, `input_multi`, `nvdrv`, `romfs`, `services`, and `threads`, verified
  deterministically in CI by `tests/repro_corpus.rs`.
- Added: 6 additional first-party deterministic static C workloads (`elf-graphs`, `elf-dp`, `elf-geometry`, `elf-compress`,
  `elf-automata`, `elf-linalg`) bringing the Linux ARM64 ELF matrix from 45 to 51 harnesses (23 `elf-*`, achieving the 50+
  workload goal, #474, #510), plus a Tier-4 fallback test for scalar-SIMD fixed-point converts.
- Verified: native Apple M3 hardware agreement across all 199 FP workload ops (185,328 cases, 199/199 `identical` on Tier-3
  and Tier-4; `docs/verify/fp-workload/result-2026-11-19-apple-m3-199ops/summary.tsv`) and `fjcvtzs_nzcv.c` (62/62 rows across
  FPCR `0` and `FZ`); all 199 ops now gate CI in `docs/verify/fp-workload/gated-ops.txt` (#488, #509).

- Added: scalar-SIMD fixed-point converts `SCVTF`/`UCVTF`/`FCVTZS`/`FCVTZU` `Sd, Sn, #fbits` and `Dd, Dn, #fbits` (#469). They share the
  vector arithmetic (one lane converted, the rest of Vd zeroed), run in the interpreter (Tier-3 and Tier-4 fall back to it), and
  are covered by decode, behaviour and engine-agreement tests. The FP workload gains 12 ops for them and for the unsigned vector
  forms; they are reported, not gated, and have not been compared with hardware yet. Half-precision forms are still undecoded.
- Paper checks (#503): `docs/verify/papers/` reproduces published results as freestanding ARM64 programs run with `run-elf`.
  `philox/` (Salmon et al., SC'11) matches the authors' three known-answer vectors for Philox4x32-10 and passes a statistical
  smoke battery that fails Philox with 1 to 6 rounds (not the paper's BigCrush claim). `accurate-sum/` (Ogita, Rump, Oishi,
  2005) checks Theorem 3.4 and Propositions 4.5, 4.10 and 5.5 on 240 generated cases with an exact long-integer reference
  and reproduces Table 6.2 within 1.0 digit; its output is identical to a native x86-64 build, fused multiply-add included.
  Each has a manifest listing what is not checked. Run on x86-64 and Tier-3 only; not in CI. A guide for adding more is in
  `docs/verify/papers/README.md`.
- Docs: the crate description, README, `docs/DETAILS.md` and the site footer no longer say ounce-core loads only homebrew. It
  also runs static Linux ARM64 programs and open-source C programs, as the README results already showed. The roadmap line for
  the action pins (done in #501) was brought up to date.
- Added: `docs/verify/fp-workload/fjcvtzs_gate.sh` builds `fjcvtzs_nzcv.c`, runs it natively and requires `run-elf` on Tier-3 and
  Tier-4 to print the same 62 lines (result, NZCV, FPSR for 31 inputs at FZ 0 and 1), with a negative control; the `fp-workload`
  job runs it on the ARM64 runner, so FJCVTZS is now compared against silicon in CI as well as by hand. Run by hand on an Apple
  M-series Mac, it settled the Z flag of a denormal flushed by FZ.
- Changed: the six action refs that were unpinned (`actions/attest`, `softprops/action-gh-release`, `actions/configure-pages`,
  `actions/upload-pages-artifact`, `actions/deploy-pages`, `taiki-e/install-action`) are now pinned to the commit their tag
  points to, read with `git ls-remote`, and `PENDING` in `scripts/check_action_pins.py` is empty. Only the pins changed:
  release, pages and coverage workflows have still not run with them.
- Family check: `scripts/family_check.py` runs the same programs (default `tests/fixtures/nro`) through two or more Ounce
  tools and compares their `ounce.run.v1` reports. Five keys (`schema`, `stop_kind`, `exit_code`, `frames_presented`,
  `framebuffer_hash`) must agree or the check fails; other keys (`insns_executed`, the audio keys, `lr`, `sp`,
  `guest_exit_status`) are only reported. Run on 2026-11-19 with ounce-core and ounce-runtime both built from `main`, 11 of 11
  programs agree on every strict key and neither tool is missing an advisory key. `insns_executed` still differs: ounce-core
  counts 15 more on 10 programs and 1 fewer on `threads`; that is unexplained. The first run (2026-11-18, before
  ounce-runtime wrote `lr`, `sp`, `guest_exit_status` and the audio keys) showed the same 11 of 11 on the strict keys. Data and
  CLI only; no code is shared between the tools.
- Contract (#481, DRAFT, not a freeze): `docs/CONTRACT_V1.md` states the additive-only rule and lists what `ounce.run.v1`,
  `ounce.trace.v1`, the exit codes and the flags `--max-frames`, `--sdmc`, `--timeout`, `--json` are today, with the
  questions the owner must answer before anything is frozen. `tests/contract_v1.rs` and `tests/contract/*.json` pin the
  exact report and trace keys and JSON types, the `exit_code` for every reachable `stop_kind`, the process exit
  statuses, and the flags, by running the binary. Findings: `--timeout` and `--json` are not `run` flags in ounce-core,
  ounce-core does not define the proposed infrastructure meanings for exit codes `2-5` (its own status 2 is the usage error) and has no `ounce.repro.v1`, and `cpu_halted` (15) cannot be produced.
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
- FP workload (`docs/verify/fp-workload/`): 151 more ops (199 total), the same 18 operand patterns x 8 FPCR values, one
  process per op: vector `.4s`/`.2d` arithmetic, FMLA/FMLS, compares, FRECPE/FRSQRTE, rounding, converts, FCVTN/FCVTL and
  reductions, plus the remaining scalar `FCVT*` forms, fixed-point converts, FJCVTZS and FCVTXN. The native ARM64
  host comparison has not been run locally (it runs in the `fp-workload` CI job, which gates only the original 48 ops); an informational Tier-3 versus qemu-user run
  on x86_64 found 147 of the 151 new ops (plus `fcvtzs.sw`, an original op) identical and 5 unimplemented
  (`FJCVTZS`, vector fixed-point `FCVTZS`/`SCVTF`) (#488).
- New instructions (#469): `FJCVTZS` (FEAT_JSCVT; truncate toward zero, wrap modulo 2^32, writes NZCV `0 Z 0 0`) and the
  vector fixed-point converts `SCVTF`/`UCVTF`/`FCVTZS`/`FCVTZU Vd.<2S|4S|2D>, Vn, #fbits`, the five ops the informational
  qemu run found stopping with exit 71. Unit tests use hand-derived expected values (saturation, NaN to 0, rounding,
  FPSR flags, FZ/RMode), a per-lane cross-check against the existing scalar fixed-point converts, and an interpreter vs
  Tier-3 vs Tier-4 agreement test. `run-elf --cpu=tier3` output for the five fp-workload ops is byte-identical to
  `qemu-aarch64` (informational, not hardware); the native ARM64 comparison has not been run. The scalar-SIMD fixed forms
  (`scvtf s0, s1, #n`) and half precision are still undecoded.
- Trace: `ounce.trace.v1`, a bounded step trace and first-divergence summary ([schema](docs/OUNCE_TRACE_V1_SCHEMA.md), #477).
  `OUNCE_TRACE_SUMMARY=<file>` makes `run` / `run-nro` write the first `OUNCE_TRACE_STEPS` (default 1024, max 65536)
  retired instructions (pc, opcode, changed registers); `ounce-core trace-diff <expected> <actual> [--window N] [--json out]`
  reports the first differing step, both values and the steps before it (exit 0 / 1 / 2). Registers only; no memory deltas
  recorded yet, and ounce-runtime does not implement the contract yet.
- Linux ARM64 ELF matrix: 17 new first-party, single-threaded, deterministic static musl workloads (`elf-*` harnesses
  in `docs/verify/elf-workloads/`: strings, sorts, hashes, bignum, printf, 64/128-bit integer math, in-memory file
  streams, allocator churn, floating point, word counts, codecs, a bytecode VM, puzzles, RNGs, calendar, control flow,
  trees) take the matrix from 28 to 45 harnesses toward the 50+ goal (#474, in progress). Each is original MIT source with
  a per-workload provenance entry; none is vendored or downloaded. Verified locally on x86-64 against host builds at
  O1/O2/O3_M1 on Tier-3, Tier-4 and `diverge`; not yet run on the native ARM64 CI job.

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
