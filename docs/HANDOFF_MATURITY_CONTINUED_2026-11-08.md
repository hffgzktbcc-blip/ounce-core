# Continued maturity work — handoff 2026-11-08

## Owner direction and scope

Keep advancing the vision and all milestones; stop with a handoff only when
usage is nearly exhausted. The earlier instruction to stop immediately was
clarified. A new app release is authorized. Do not ask again for ordinary
implementation, review, pushing or publication within that scope. Existing
artifact qualification and specific distribution decisions remain unresolved;
publication authorization is not evidence that those decisions were completed.

## Durable state and review

Repository: `/workspace/ounce-core`. This handoff lives on
`docs/maturity-handoff-2026-11-08`, based on `891ace1`; it also retains the completed three-host qualification.
All implementation commits are pushed. No PR was merged and no release/tag was
published in this session. PRs are cumulative; integrate in order or use the
latest cumulative candidate after its applicable checks/review pass. Preserve
history and do not blindly squash/rebase all branches.

| PR | Branch / head | Result |
|---|---|---|
| [262](https://github.com/hffgzktbcc-blip/ounce-core/pull/262) | docs/refresh-vision-milestones / cbb55a6 | Stale information corrected; Apple Silicon fuzzer CI. All applicable hosted checks passed; runtime jobs correctly skipped for docs scope. |
| [263](https://github.com/hffgzktbcc-blip/ounce-core/pull/263) | feat/linux-library-harness / a374c45 | Existing pinned LZ4 cases execute as static Linux ARM64 ELF on actual Tier-3/Tier-4; repeatable command and retained evidence. Linux hosted job passed. |
| [264](https://github.com/hffgzktbcc-blip/ounce-core/pull/264) | feat/linux-deterministic-clock / 87bf9ab | Opt-in virtual clock and real fix for host parser consuming guest arguments after `--`. |
| [265](https://github.com/hffgzktbcc-blip/ounce-core/pull/265) | feat/linux-elf-cross-platform / 10db735 | Three-host ELF qualification, Windows binary stdout and correct `.exe` paths. Library and all five CLI scenarios passed on Linux, macOS and Windows. |
| [266](https://github.com/hffgzktbcc-blip/ounce-core/pull/266) | fix/fuzzer-empty-input / 891ace1 | Reject empty/read-error/zero-count traces and fail on accepted host-UNDEFINED words. Full local suite passed; new hosted checks pending. |

Focused reviewer found one Windows runtime path blocker; fixed in `10db735`.
No other actionable issue was found. A second review of the fuzzer integrity
repair found no actionable correctness regression. Reviews were read-only.

## What actually passed

- Full native-jit runtime unit suite: **562 passed, 0 failed, 10 existing opt-in
  tests ignored**. Latest run took 32.53 seconds after compilation.
- All **9 CLI tests passed**. Includes clock repeatability, untouched guest
  option-like arguments after `--`, missing/malformed input and fuzzer negative
  paths. The argv test failed on the old runtime before the parser fix.
- **108 Python tooling tests passed**, one existing opt-in skip; includes five
  new ELF verdict tests. Incorrect exits, missing engine identity and Tier-4
  fallback cannot be certified by matching bytes.
- Linux LZ4 host/Tier-3/Tier-4 output matches byte-for-byte: **406 bytes,
  fails=0**. Registered command run used **3,862,822 instructions, five syscalls**
  on both actual engines. A preliminary manual build used a different layout
  and instruction count; the checked-in report is the canonical local evidence.
- Deterministic clock fixture repeats exactly on two runs of each actual engine;
  invalid reads do not consume ticks and the second rollover is tested.
- Generated roadmap, YAML parsing/job checks, retained log hashes and
  `git diff --check` passed. Only source/observation text and the project-owned
  clock fixture are committed; LZ4/libc/runtime binaries are not redistributed.

Retained evidence:
`docs/verify/linux-elf/lz4-2026-11-08/`,
`docs/verify/linux-elf/deterministic-2026-11-08.json`,
`docs/verify/insn-fuzzer/ci-2026-11-08.log`,
`docs/verify/insn-fuzzer/integrity-2026-11-08.json`.
Large local logs/binaries are under `/workspace/evidence/`; runtime build is
`target/release/ounce-core`. Hashes in acceptance reports identify each tested
binary; later builds naturally have different hashes.

## Hosted evidence and pending checks

- First fuzzer qualification: run **42521905867**, job **128857024677**; 60,000
  engine-comparison words and seed-1 20,000-word hardware traces through both
  tiers, zero unexpected differences. Artifact ID **13633916913** uploaded.
- First Linux ELF qualification: run **42523091575**, job **128860671137**, passed.
- Three-host candidate: run **42523628103** (head `10db735`). Linux job
  **128862296904** passed; macOS job **128862296842** passed; Windows job
  **128862296903** passed both CLI and library checks. All jobs in this
  differential workflow completed successfully. V4 is closed with
  `docs/verify/linux-elf/three-host-2026-11-08.json`. This is not an independent
  outside-user reproduction.
- PR #266 is the latest cumulative candidate. Inspect its final CI/differential
  results before integration. Do not treat a green narrower job as all-checks
  acceptance.
- Duplicate PR #264 runs **42523497384** (queued CI) and **42523497303**
  (differential) were deliberately cancelled because newer candidates include
  the same work. Do not diagnose those cancellation states as code failures.
- Artifact ZIP storage denied a CLI download. The GitHub connector successfully
  retrieved job logs; the retained fuzzer excerpt comes from those logs, not a
  claim of local ZIP-byte verification. Never copy signed artifact URLs/tokens.

## Environment is now capable of runtime work

This is a Debian Linux x86-64 Cloud Environment. Rust was initially absent, but
workspace-local stable Rust **1.99.1**, Zig **0.17.1** and ALSA/udev development
files are now installed. There is no sudo. Use:

```sh
cd /workspace/ounce-core
. /workspace/toolchains/env.sh
cargo test --release --locked --features native-jit --bin ounce-core --test cli_failures
python3 scripts/verify_linux_elf.py --work /workspace/evidence/NEW-RUN \
  --upstream-dir /workspace/toolchains/lz4 \
  --elf-cc '/workspace/toolchains/zig/ziglang/zig cc -target aarch64-linux-musl'
```

`/workspace/toolchains/env.sh` selects user-owned Rust caches, the local
pkg-config sysroot/library paths and Zig cache. LZ4 checkout is clean at
`d04dab3152ffdb8f8e8708e4d135f7293c0f6136`. `--work` must be new; prior evidence
is intentionally never overwritten. Windows/macOS CI use the pinned Python
`ziglang` distribution and a portable host compiler.

Default sandbox cannot reach proxy:8080; approved escalated ordinary Git, curl,
Cargo and gh commands work through the inherited proxy. Earlier `gh auth status`
reported invalid credentials because access was attempted in that sandbox;
subsequent escalated `gh` reads, pushes and workflow dispatch worked. Do not
replace credentials or ask for tokens. Purpose-built GitHub connector also works.

## Release remains a distinct unfinished deliverable

`docs/releases/app-next-draft.md` is a concrete candidate checklist, not a release.
`docs/RELEASE_NOTICE_REVIEW.json` still says `PENDING_SPECIALIST_REVIEW`; its scope
excludes optional features, while the Mac app builds `gpu-present nce native-jit`.
`docs/MAC_APP.md` explicitly withholds a prebuilt app pending the recorded
objc2/Apple SDK distribution decision. Do not mark that decision complete or
weaken `--require-reviewed` based solely on a generic publish request.

Finish the independent technical preparation: app-feature notice inventory,
final bundle qualification and checksums, live Mac GUI acceptance, app packaging
in the release workflow (currently CLI-only), version choice and truthful
installation/signing/notarization notes. User already authorized publishing once
these actual prerequisites are satisfied. Historical E1/E2 questions do not block
ordinary development or the separately reviewed Python toolkit.

## Next actions, without repeating completed work

1. Inspect latest #266 checks; fix actual failures. V4 already passed on all
   three hosts at 10db735; do not repeat that work as if still untested.
2. Integrate reviewed, qualified changes through normal repository workflow.
   All current PRs target main; earlier stacked base prevented workflow triggers,
   so #263 was retargeted and manually qualified. Avoid firing redundant full
   matrices on every documentation-only handoff update.
3. Continue **V5**: one real bounded instruction-level first-divergence dossier.
   Existing fuzzer has Case/State snapshots and exact before/expected values in
   `src/fuzz_replay.rs`; it reports register differences but does not retain a
   portable first-divergence artifact. A promising historical case is scalar
   FNMUL NaN ordering: `src/cpu/interpreter.rs::fuzz_scalar_nan` records exact
   Apple Silicon inputs/results, fixed by `fedcf2e` / `f432544`. Preserve a real
   failing reproduction; label injected probes as synthetic. Do not claim
   single-stepping through an interpreter fallback verifies optimized blocks.
4. **V6**: hash-checked portable execution recording/replay on another host;
   current LZ4 command retains useful building blocks but does not qualify this.
5. **V7**: real WebAssembly runtime execution behind the browser teaching UI;
   current lab is scripted. **V8**: reusable conformance suite and an actual
   outside-project result. External acceptance cannot be manufactured internally.
6. Keep working on the app release prerequisites in parallel where independent.

PLAN.md now has V1–V4 completed with bounded evidence and V5–V8 explicit remaining
acceptance. Do not relabel all vision milestones complete based on these results.
