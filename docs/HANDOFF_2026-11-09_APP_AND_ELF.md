# Handoff: Mac app preview, ELF coverage, v0.7.0 draft (2026-11-09)

State at the end of this session. Start from `main`; do not redo anything listed as done.

## Done and merged
- **v0.6.1** released (tag on `9bcb965`): Linux x86-64, macOS arm64, Windows x86-64 CLI archives. Archive hashes matched `SHA256SUMS`. Attestation signatures were not independently verified.
- **#287, #288, #289**: community docs, behavior-preserving clippy cleanups (114 warnings remain, intentionally), banners print the crate version, Mac app launch check recorded in `docs/MAC_APP.md`.
- **#272**: FADD/FMUL operand order pinned in the vector-fp harness; the quiet-NaN "difference" was a compiler swap. **#277**: `sched_getaffinity` in `run-elf`; xxHash, cJSON and zlib run as static musl ELFs (Linux evidence).
- **#290**: `scripts/package_macos_app.py` makes `.tar.gz`/`.zip`, `MANIFEST.json`, `SHA256SUMS`; CI keeps them as the 14-day `macos-app-preview` artifact. Nothing is published.
- **#292** (merged `4b656cc`): three-host ELF job for LZ4, xxHash, cJSON, zlib; `docs/releases/v0.7.0-draft.md`. The first hosted Windows run failed on xxHash because the host reference wrote text-mode `\r\n`; fixed in `713ecca` by switching stdout to binary on Windows (same block LZ4 already had). Check the merged run's artifacts `linux-elf-libs-evidence-<os>` for the retained reports.

## Mac app: what is and is not verified
Verified on an Apple Silicon Mac (macOS 26.6.2) with the CI-built preview: checksums, ad-hoc signature, bundled runtime self-test (36 checks), hello.nro on tier3 and tier4 (9,067 instructions), launch and first-run window, "Try the Demo" (graphics window, 498 frames; the demo draws only about 25 rows, so the rest is black by design).
**Not verified:** running a chosen program from the window, the failure-diagnosis card, Verify against Host from the window, cancel and relaunch, other window sizes, tier4 inside the GUI, first-open Gatekeeper experience. The file dialogs cannot be driven safely by script; a person at the Mac (or a screen-control tool) is needed.

## Decisions in force
- Mac app: ad-hoc-signed developer preview only. **No prebuilt macOS binary in GitHub release assets** (`docs/PROVENANCE.md` Specialist items, objc2/Apple SDK terms). Do not flip `RELEASE_NOTICE_REVIEW.json` status or infer clearance.
- `docs/releases/v0.7.0-draft.md` is named `-draft` so `release.yml` cannot pick it up by tag. Renaming it to `v0.7.0.md` is the publishing step.
- Tag pushes are blocked from the cloud container; they were pushed from the owner's Mac.

## Next, in order
1. Cut v0.7.0 if wanted: bump `Cargo.toml`/`Cargo.lock`, refresh the two hashes in `docs/RELEASE_NOTICE_REVIEW.json` (see PR #286), add a CHANGELOG section, rename the draft, tag from the Mac. Finish or drop the unchecked GUI items first.
2. ELF coverage: Monocypher, tinyexpr, lua, sqlite, wasm3 as ELFs; `getrandom`, `futex`, `clock_gettime` only when a program needs them.
3. #257: FPSR[31:28] needs the Arm ARM text; do not guess.
4. Open PRs by others: **#291** (lab guide; I reworded it to say the lab is a design preview, merge is the owner's call, it showed "blocked"), **#268** (site refresh; contains stale text, do not merge as is), **#285** (recruitment; merge only after the owner prunes the leads list).

## Blocked on outside input
Step 0 independent reproduction (#190, #248), Step 6 conformance adoption, V8, E1, E2, owner-decision drafts #210-#212, #281 external link check.

## Gotchas
- The designated branch `claude/admiring-turing-i3kp89` has a stale remote tip after each squash merge. A force push is denied here; merge the remote tip with `git merge -s ours origin/<branch>` and push normally.
- The Mac clone at `~/Projects/ounce-core` is a separate copy on `step3-sdl2` with an old tree; do not assume it matches `main`. Scratch files are in `/tmp/ounce-preview-check` on the Mac.
- CI event wakes can refer to an old head SHA; check the PR's current head before acting.
