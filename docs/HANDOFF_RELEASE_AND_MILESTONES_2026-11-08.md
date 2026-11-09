# Release and milestone handoff — 2026-11-08

## Owner direction

Publish a new release for the newly added app, then advance the vision milestones.
Owner explicitly requested stopping with a handoff because usage is low. Continue
from this state; do not redo the audit or require another design approval for the
already authorized release/milestone work.

## Saved work

Checkout: `/workspace/ounce-core`; branch: `docs/refresh-vision-milestones`.
Commit `5ee7696` refreshes stale README, vision, limits, reproduction instructions
and generated milestone registry. It adds an independent Apple Silicon fuzzer
job to `.github/workflows/differential.yml` (20,000 words per seed, existing script,
logs retained on failure). Hosted acceptance remains pending.
Candidate release notes/checklist: `docs/releases/app-next-draft.md`.
This handoff and the candidate notes are committed in the next local commit.
Nothing from this session has been pushed, tagged or published.

## Checks actually run

16 Python workflow/scope tests passed with
`PYTHONPATH=scripts python3 -m unittest scripts.test_ci_scope scripts.test_agent_workflow`.
Workflow YAML parses; job assertions, `bash -n` for the existing fuzz script,
`python3 scripts/render_roadmap.py --check` and `git diff --check` pass.
No runtime, Mac GUI or hardware-fuzzer execution was performed here.

## Environment and blockers

Managed Cloud Environment is connected; status observed current, network enforced,
unrestricted; no configured secrets or identities. No Rust/cargo toolchain is
installed. GitHub CLI reports invalid GH_TOKEN; do not print its value. Clone
worked via an approved escalated git command because the default sandbox cannot
reach proxy:8080. Purpose-built GitHub connector reads work and may offer a route
to submitting the saved changes; discover relevant capabilities before asking
for credentials. No remote write was attempted.

Public app distribution is currently blocked by the existing
`PENDING_SPECIALIST_REVIEW` in `docs/RELEASE_NOTICE_REVIEW.json`, not by lack of
user publication authorization. `docs/MAC_APP.md` explicitly withholds a prebuilt
Mac app pending the objc2/Apple SDK redistribution decision. The review scope
covers default binaries only, while the app builds optional GPU/NCE/JIT features.
Do not flip review status or infer clearance from the owner's release request.
Collect an actual documented decision and qualify final artifacts. Historical E1/E2
questions do not block ordinary runtime development.

## Continue in this order

1. Submit saved branch changes and inspect hosted checks. Qualify V1 by running
   the new arm64 fuzzer job; preserve unexpected differences rather than changing
   expectations. V1 remains active until hosted evidence is retained.
2. Prepare the app release using `docs/releases/app-next-draft.md`. Current
   release workflow publishes CLI archives; app packaging is still needed. Do
   independent technical preparation while the distribution decision is pending.
3. V2 in PLAN.md: execute one existing pinned library harness as a static Linux
   ELF without console SDK; compare identical shared C cases with a fresh host
   reference on Tier-3/Tier-4. Existing run-elf handles five freestanding fixtures,
   not real libc acceptance. Extend only observed required syscalls.
4. Vision step 2 further work: per-family and multi-instruction fuzz coverage.
5. Step 3: real instruction-level first divergence (existing retained frame
   diagnosis does not supply this). Step 4: portable execution record/replay.
6. Step 5: real WebAssembly runtime playground (site/lab.html is scripted).
7. Step 6: external emulator conformance-suite adoption. Outside reproduction
   remains unestablished; do not count internal or CI runs as independent.

Keep one checkable user result per change. PLAN.md and docs/VISION.md now distinguish
implemented components, bounded evidence and unmet milestone acceptance.
