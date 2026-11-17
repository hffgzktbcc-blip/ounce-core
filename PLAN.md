# ounce-core: useful results, not component counts

> **Agent orientation (17 Nov 2026):** For the four-repository strategy and upcoming-quarter priorities, consult the [Ounce Family one-year roadmap](docs/roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md) **first**. This 8 November file retains historical task evidence and the Core-local backlog; its old `Next:` notes and release references may describe work now shipped. Before beginning a task, verify the current repo state, closed PRs/issues, releases, and CI, then choose only genuinely unfinished work. Never treat a historical handoff as an open assignment.

Updated 2026-11-08 to reconcile vision progress with current main and continue
the owner's direction: remove agent friction and deliver community value beyond
Switch/Switch 2 coverage. This file replaces the previous
milestone order. [Previous plan and evidence](docs/history/PLAN-2026-11-07.md) remain
historical records; unfinished historical items are not implicit blockers.
[AGENTS.md](AGENTS.md) contains the active working rules.

## Direction

Help developers answer: **what changed, why did this run fail, and how can someone
else reproduce it?** Start with a small debugging/regression toolkit assembled
from the existing verifier, diagnosis, comparison, bounded execution and replay
code. Runtime/CPU/GPU/service components earn new work by unblocking that user flow.

This is a product hypothesis, not demonstrated community demand. Validate it with
one real maintainer problem before building a generalized platform. Current corpus
adapters remain platform-specific; no generic AArch64 executable support is claimed.
Existing homebrew samples stay as regression fixtures. Switch 2 coverage expansion,
more platform badges, speculative product families and standalone dashboards are
parked unless a concrete user problem needs them.

Considered directions: more platform coverage would extend the existing benchmark
but would not address the requested community value; building Debug/Trace/Learn/Bench
as four products would multiply unfinished surfaces. The chosen starting point is
one complete failure-to-reproduction workflow using the components already present.

## What counts as progress

A deliverable is usable when a documented entry point takes a real input through
execution or retained evidence to an actionable result, with failure handling and a
repeatable check. Name what is supported and what is missing. Do not relabel an
implemented component as an end-to-end product. Historical benchmark counts remain
valid only within their original boundaries; they are regression metrics, not the
product's sole success criterion.

For each task keep only: result, next action, evidence, or blocker. Use unique IDs.
`Status: ready|active|blocked|parked` and `Depends: ID ...` guide the optional selector.
Blocked outside review or tester recruitment never stops unrelated ready work.
Close tasks manually with an evidence link after acceptance; PR submission never
closes them. Checkpoints live in commit messages, not automatic edits to this file.

## Ready work, in order

Done tasks are one-liners; full evidence is in [docs/history/PLAN-COMPLETED.md](docs/history/PLAN-COMPLETED.md). Doc map: [docs/README.md](docs/README.md).

- [x] **W1. Remove workflow loops and preserve useful checks.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#w1).
- [x] **R1. Ship one failure-to-reproduction walkthrough.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#r1).
- [x] **R2. Solve one externally grounded developer issue.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#r2).
- [x] **R3. Make the proven workflow installable and predictable.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#r3).
- [x] **R4. Turn one lagging component into an observed user result.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#r4).
- [x] **R5. Publish the smallest useful community release and reassess.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#r5).

## Vision continuation (owner requested 2026-11-08)

- [x] **V1. Qualify the existing instruction fuzzer in ARM64 CI.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#v1).
- [x] **V2. Run one existing library harness as a static Linux ELF.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#v2).

- [x] **V3. Make the bounded Linux ELF workflow repeatable.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#v3).
- [x] **V4. Qualify the same Linux ELF workflow on Linux, macOS and Windows.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#v4).
- [x] **V5. Locate one instruction-level first divergence.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#v5).
- [x] **V6. Replay one recorded execution on another host.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#v6).
- [x] **V9. Repair the measured host-dependent vector multiply result.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#v9).
- [x] **V7. Replace the scripted browser teaching path with real runtime execution.** Evidence: [archive](docs/history/PLAN-COMPLETED.md#v7).
- [ ] **V8. Publish a reusable conformance suite and obtain an outside result.**
      Status: blocked
      Blocked by: an actual outside-project execution and result.
      Result: another emulator project runs a versioned suite and reports a result with exact inputs and reference scope.
      Progress: [version 0.1.0 seed](conformance/arm64-v0.1.0/README.md) packages the unchanged hardware FMUL trace, hashes, provenance, comparison scope and adapter contract. A runner rejects unsupported, missing/duplicate counts and tampered input. All three local runtime engines pass. PR #270 passed all 21 hosted checks, including each engine on Linux, macOS and Windows, and merged at `2e55a5e`; these remain internal observations.
      Next: preserve the published seed, expand from measured cases in a new version, and obtain an actual outside-project result. Outside adoption remains external acceptance and is not inferred from internal runs.

## External acceptance and publication dependencies

- [ ] **E1. Obtain an independent rerun of the existing reproduction experiment.**
      Status: blocked
      Blocked by: unaffiliated tester evidence through issue #190. Existing internal/CI RomFS results remain useful, but do not establish independence.
      Next: retain the recipe and accept outside evidence when available; no repeated internal reruns as a substitute.
      Evidence: [issue #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190) has two comments, both by one outside tester (@cyzcz, 2026-11-07; reports #191 and follow-ups). Reviewed 2026-11-11: that is one self-reported outside run, not an independent rerun, so it stays `self-reported, awaiting independent rerun`. It deviates from the pinned recipe and lacks the byte-checked canonical records. See [E2_DECISION_MEMO.md](docs/E2_DECISION_MEMO.md#issue-190-is-either-comment-an-independent-rerun-e1). E1 is not complete.
- [ ] **E2. Disposition historical provenance for affected distribution artifacts.**
      Status: blocked
      Blocked by: native-app/optional-feature distribution decisions and actual bundle notice coverage, the historical 72-byte program permission question, and separately approved historical/ref actions. The default CLI v0.6.0 decision and its actual three-platform notice coverage are complete. PR #203 is merged; its technical controls do not establish legal clearance.
      Next: use docs/LICENSE_DISPOSITION.md as the current disposition; earlier broad audit/checklist warnings are superseded. Objective-C full-text notices and binary-bound Rust toolchain notice collection are implemented; preserve the actual upstream SDK/contributor qualifications and specific historical permission case. The normal default-feature CLI was published as [v0.6.0](https://github.com/hffgzktbcc-blip/ounce-core/releases/tag/v0.6.0) after the documented owner-directed distribution decision, three-platform qualification and strict reviewed packaging. [Downloaded-asset proof](docs/releases/v0.6.0-publication-proof.json) verifies checksums/notices and a Linux smoke run. Optional/native-app distribution and historical permission questions remain separate; no specialist legal review or new third-party grant is claimed. The standalone Python toolkit is separately distributable. Do not repeat completed inventories or treat E2 as a blocker to ordinary development. The four open questions (#308 to #311) have recorded outcomes in [docs/LICENSE_DISPOSITION.md](docs/LICENSE_DISPOSITION.md#open-e2-items-recorded-outcomes--10-november-2026) and are closed.

## Delivery cadence and stop conditions

Use one small vertical result per PR. Submit partial useful work without waiting for
an entire research milestone. Prefer focused local checks, then one relevant hosted
qualification for the candidate. Preserve exact failures; retry transient infrastructure
only within existing bounded limits. If the same failure recurs with unchanged inputs,
record it and change the hypothesis or move to ready work, rather than rerunning it.

Track time to first successful diagnosis, replay success on supported environments,
real issues resolved, and setup/CI time. No new telemetry service is required: record
measurements in the result/PR. After the first community release, choose the next
work from these observations. No invented dates, token savings, user demand or adoption.
