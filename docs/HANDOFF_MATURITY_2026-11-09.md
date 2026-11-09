# Ounce Core maturity handoff — 2026-11-09

User scope: keep implementing all vision milestones, release the native app when
its concrete prerequisites are satisfied, and leave a recoverable handoff near
the usage limit. Continue autonomously; do not restart completed investigations.

## Integrated foundation and arithmetic repair

- PR #266 merged at `645f3fec3c95deffcc7703088c5b9bce41297977`.
- PR #267 merged at `2182aeb6fbf5832122da77e31b5ce03a545e21e8` after all 40
  hosted checks passed on candidate `5342324c1ea8efd77419368e7d7a344b120991a2`.
- V5/V6 diagnose and replay one actual independent-instruction discrepancy, not
  arbitrary-program divergence or recording. `fuzz-replay --first-difference
  NEWDIR` saves immutable content-checked state; `replay-instruction BUNDLE`
  distinguishes historical reproduction from matching the hardware reference.
- V9 fixes shared SIMD FMUL zero-times-infinity default-NaN signs. The exact
  Apple Silicon trace and pre-fix Linux dossier remain unchanged under
  `docs/verify/insn-fuzzer/default-nan-mul2d/`. Hardware capture job 128970464303
  passed; its containing run 42558815987 was later superseded/cancelled.
- Historical Linux failure reproduced on Windows before the fix; macOS matched
  hardware. Run 42559991291's replay steps passed but containing jobs failed on
  a cached LZ4 output directory. Fresh run-specific evidence outside Rust cache
  repairs that separate defect. The final PR #267 pipeline requalified it.
- Local full native suite: 565 passed, 10 existing opt-in ignores; 11 CLI tests.
  Unchanged prior LZ4 ELF still produces exactly 406 bytes on both engines.
  Tier-4 vector execution can fall back; one native sentinel is not native FMUL
  proof. FP exception flags and complete FPCR semantics remain unmodelled.

## Browser milestone

PR #269 merged after all 40 hosted checks passed. Merge commit
`888d5158bd862ec2dbeb19145ee941ed1c409504`; exact tested candidate
`37cfc2683948e6979b0357f9ed030dcf91f8fdae`. Pages run 42565903672 succeeded. Public Chromium acceptance passed; see
`docs/verify/playground/live-acceptance.json` for actual deployed Wasm hash, live
MOV/ADD, hardware FMUL and offline execution. V7 is complete.

- `crates/ounce-learn` is a standalone locked workspace reusing actual safe
  `src/cpu` and `src/mmu` source. Root Cargo manifest/lock stay unchanged.
- Bounded WebAssembly API; real load/step/run/reset, register snapshots, code
  decode, transactional invalid input, SVC stop, 10,000 total step budget.
- Local native 117 tests and real Chromium checks passed. Browser loaded actual
  Wasm, then executed offline: MOV/ADD, SVC, invalid input, hardware FMUL, reset,
  bounded loop and mobile viewport. Retained evidence: docs/verify/playground/.
- Hosted Browser interpreter run 42564695731 passed at the candidate above.
  Artifact 13650454864, digest
  `sha256:099cf1b5cb0a7da7294dce0751dfc172874ac3424153147c9cebba7f0e71ae56`.
  Local artifact SHA is recorded separately; don't assume hosted bytes identical
  without fetching/verifying its artifact.
- First hosted run 42564528833 failed because offline Cargo metadata needed a
  dependency not compiled for this target. Builder now fetches the entire locked
  graph before offline notice collection; corrected hosted build/browser passed.
- Browser artifacts retain actual locked-graph license files and installed Rust
  notices, bound to Wasm hashes. Checker verifies inventory/assets; manually
  corrupted notice was rejected. This doesn't clear native app distribution.
- Existing Pages workflow builds the artifact into `/ounce-core/playground/`.
  PR #269 and its Pages/live acceptance are complete.
  Existing destination: https://hffgzktbcc-blip.github.io/ounce-core/.
- Focused reviewer found no actionable browser or final Pages-wiring issue.

## Reusable conformance seed

PR #270: `feat/conformance-seed`, retargeted to main. Exact candidate
`c8d3d32b5ebe7054b63c11fcf48a009710158ed2`. Hosted differential run 42566067861
and CI run 42566068002 are qualifying it. Version 0.1.0 contains one unchanged hardware FMUL input, comparison
mask, provenance, MIT text and adapter contract under `conformance/`.

`scripts/run_conformance.py` validates exact hashes before executing an argument
vector, requires actual executed/matched counts, rejects unsupported instructions
and missing/duplicate/malformed counts, and preserves new JSON reports. Four
focused tests passed; full Python tooling passed 112 tests with one existing
skip. Actual interp/Tier-3/Tier-4 runs pass locally. Review caught a malformed
negative duplicate count; regression failed before c0bf7b6 and passes after it.
Reviewer verified correction and reported no further issue. Existing three-host
job now retains per-engine seed reports; hosted qualification still needs observing.
Outside-project execution/adoption remains outstanding; V8 stays open.

## Rebuild environment

```sh
cd /workspace/ounce-core
. /workspace/toolchains/env.sh
export WASM_BINDGEN=/workspace/toolchains/wasm-bindgen/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl/wasm-bindgen
scripts/build_playground.sh
python3 tools/playground/check_artifact.py target/playground
python3 tools/playground/check_browser.py --chromium /usr/bin/chromium --output /workspace/evidence/NEW-browser-run
python3 -m unittest discover -s scripts -p test_run_conformance.py -v
python3 scripts/run_conformance.py --report /workspace/evidence/NEW-seed.json -- target/release/ounce-core fuzz-replay --engine interp
```

Rust stable 1.99.1 and wasm32 target installed. Zig 0.17.1 and pinned LZ4 source
remain under /workspace/toolchains. Python Playwright 1.62.0 and system Chromium
156.0.8078.79 installed. Network/Git/gh and opening localhost sockets need sandbox
escalation; same browser command succeeds escalated. An EPERM without it is not
an interpreter bug. GH CLI works; don't ask for credentials on sandbox failures.
Evidence logs are under /workspace/evidence. No expensive passing suite reruns
without changed inputs or unresolved concerns. Use `gh pr checks` (this installed
CLI doesn't support its newer --json flag); `gh pr view --json` does work.

## Native app release still has concrete prerequisites

No native release tag or asset publication is claimed. Existing recorded review
is PENDING_SPECIALIST_REVIEW and excludes app features `gpu-present nce native-jit`.
`docs/MAC_APP.md` withholds prebuilt app pending the recorded objc2/Apple SDK
redistribution decision. Need actual optional-feature/final-bundle review, Mac
live GUI acceptance, app packaging in the release workflow (currently CLI only),
version/signing/install notes. Checklist: docs/releases/app-next-draft.md.
User already authorized publication; no repeat generic permission question is
needed. Do not fabricate a rights decision, weaken --require-reviewed, or treat
this native-distribution blocker as blocking ordinary development/browser work.

## Next actions

1. Browser #269 and public V7 acceptance are complete; do not redo them without
   changed inputs or an unresolved issue.
2. Observe #270 three-host seed reports and tooling, fix real failures, then
   integrate reviewed passing head. Keep outside adoption open.
3. Continue app prerequisites that can be done with available evidence; preserve
   the exact specialist-review/live-GUI blockers. Expand measured conformance
   inputs as useful, without claiming outside adoption or contacting maintainers.
4. Refresh this handoff and `/workspace/ounce-core-handoff.bundle` before stopping.

Final-additions review also found no actionable issue in the three-host seed
workflow or live browser evidence. `gh pr edit` failed in this installed CLI
because it requests deprecated classic-project GraphQL fields; a structured
REST PATCH to /repos/hffgzktbcc-blip/ounce-core/pulls/270 successfully changed
the base and body. Do not repeat the broken GraphQL edit command.
