# ounce-core: useful results, not component counts

Updated 2026-11-07 from the owner's direction to remove agent friction and deliver
community value beyond Switch/Switch 2 coverage. This file replaces the previous
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

- [x] **W1. Remove workflow loops and preserve useful checks.**
      Result: read-only local status; unique task IDs; blocked-task selection; cheap staged-only local checkpoints; separate PR submission; scope-aware CI with required statuses always emitted.
      Acceptance: disposable-Git regression tests show no fetch/revert/push at startup, no Cargo/GitHub requirement to save, no sweeping peer work, no automatic completion/merge, and correct blocked/duplicate handling. Hosted CI checks relevant code without requiring Rust for docs-only edits.
      Evidence: [PR #205](https://github.com/hffgzktbcc-blip/ounce-core/pull/205) and [hosted run 42408267822](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42408267822), verified 2026-11-07: all 25 jobs completed successfully, including tooling, platform/runtime, canonical corpus and interaction checks. Disposable-Git tests cover read-only startup, blocked/duplicate handling and staged-only local save without Cargo/GitHub. Integration remains separate; no PR was merged to record this acceptance.
- [x] **R1. Ship one failure-to-reproduction walkthrough.**
      Result: a developer follows one entry point to load two retained runs, see the first recorded divergence, identify comparable inputs, and obtain the exact replay command and prerequisites.
      Scope: reuse tools/ounce_verify.py, scripts/diagnose_corpus_run.py, scripts/compare_corpus_runs.py and the historical SDL2 failure; retain real before/after evidence with redistribution reviewed for those artifacts only. Start with local JSON and a CLI; no new service or GUI framework.
      Acceptance: on a fresh Python-only checkout, one documented command produces a diagnosis and a shareable reproduction bundle from permitted retained evidence. Corrupt/missing inputs give actionable errors and nonzero exits. Replay is actually executed in a supported Rust/runtime environment and checked against the recorded failure; offline diagnosis is not called a fresh execution.
      Evidence: [Python-only retained SDL2 walkthrough](docs/FAILURE_TO_REPRODUCTION.md) produces a portable diagnosis/replay dossier from original before/after report and log bytes; exact artifact identities and observation-only distribution scope are retained. Both engines first diverge at frame 1; no historical prior passes are lost. This is offline inspection, not fresh runtime execution.
      Execution acceptance (2026-11-07): exact-hash local NRO replayed from clean recorded revisions on macOS arm64 with isolated build directories. Both engines exactly match historical stop, PC, instruction count and frame observations: expected failure exits 101; reference passes with exit 0. Raw JSON, runtime logs and digests: [dated replay manifest](docs/reproduce/sdl2-failure/replay-2026-11-07/manifest.json). Offline error handling: all six walkthrough tests pass. No binary redistribution, independent-rerun or current-corpus claim.
      Next: R2 investigates a real developer input; R3 can package the demonstrated workflow.
- [ ] **R2. Solve one externally grounded developer issue.**
      Status: ready
      Result: an open-source developer has a reproducible answer to a real regression, execution divergence, or unusable failure report.
      Scope: inspect public issue reports or contributor feedback; record the issue URL, affected user, current workaround and smallest supported input. Do not assume market demand or contact maintainers without authorization.
      Acceptance: capture failure before a targeted fix, run the same input after it, show the actionable diagnosis/replay result, and preserve prior relevant passes. Record developer feedback separately; outside endorsement is not required to merge a useful fix.
      Evidence: [public issue shortlist](docs/verify/ISSUE_SHORTLIST.md), inspected 2026-11-07. Real reports exist, but none currently supplies a supported, permission-cleared matched execution pair; no outside developer outcome or demand is claimed.
      Next: investigate a minimal accessible input for the threading report or an incoming developer failure with retained runs. Require a recorded failure before choosing a fix; do not build a generic adapter from this shortlist alone.
- [x] **R3. Make the proven workflow installable and predictable.**
      Depends: R1
      Result: a new developer runs the demonstrated diagnosis without assembling internal scripts manually.
      Scope: one CLI front door, explicit versions/exit codes, examples and a small distribution using current Python/Rust tooling. Separate offline report inspection from execution dependencies. Support only the tested report/input formats.
      Acceptance: fresh-install smoke test, successful diagnosis, missing/corrupt input, timeout and interrupted execution scenarios; a shareable bundle opens without private paths or credentials. Record measured setup time and tested hosts rather than a speed promise.
      Evidence: [CLI install and exit-code guide](docs/DIAGNOSIS_CLI.md), [dated acceptance](docs/verify/diagnosis-cli-0.1.0/acceptance.json). The copied standard-library archive diagnosed the reviewed example outside the checkout on macOS arm64/Python 3.9.6; explicit replay matched both exact historical revisions. Packaged timeout and SIGTERM returned 124/130 with retained logs. Tests cover missing/corrupt inputs, output preservation, wrong hashes and real descendant cleanup on timeout/SIGINT/SIGTERM; 99 Python tests ran, zero failures, one opt-in infrastructure skip. No Windows or independent-user acceptance claimed.
      Next: keep the CLI scope fixed until R2 identifies a reproduced external developer failure; release remains R5.
- [ ] **R4. Turn one lagging component into an observed user result.**
      Status: ready
      Depends: R2
      Result: one existing CPU, JIT, GPU, service, trace or app component materially improves the selected developer's workflow.
      Scope: trace the selected issue through the real command path; repair the first unconnected or incorrect boundary. No blanket refactor or new subsystem because it appears on an architecture diagram.
      Acceptance: the user scenario crosses the relevant component boundary, produces the expected observable result, handles its failure path and retains a regression case. Compare latency only on matched inputs/hosts; compare correctness against a fixed expectation.
      Next: use R2's reproduced failure to select the component and bounded integration change.
- [ ] **R5. Publish the smallest useful community release and reassess.**
      Status: ready
      Depends: R3 R4
      Result: a versioned, documented tool that addresses the demonstrated issue with an example, limitations, checksums and a repeatable recipe.
      Acceptance: clean-environment installation and the selected end-to-end scenario pass; required CI passes; included materials have documented redistribution rights. Summarize actual feedback or lack of it. Do not claim adoption from stars or internal reruns.
      Next: release only the demonstrated workflow. Continue from concrete reported failures; add a generic evidence adapter, viewer, platform or service only when a real user needs it.

## External acceptance and publication dependencies

- [ ] **E1. Obtain an independent rerun of the existing reproduction experiment.**
      Status: blocked
      Blocked by: unaffiliated tester evidence through issue #190. Existing internal/CI RomFS results remain useful, but do not establish independence.
      Next: retain the recipe and accept outside evidence when available; no repeated internal reruns as a substitute.
- [ ] **E2. Disposition historical provenance for affected distribution artifacts.**
      Status: blocked
      Blocked by: specific owner/specialist decisions recorded in the provenance inventory and open PR #203. No destructive rewrite or blanket clearance is implied.
      Next: review existing inventory/PR instead of recreating it. Restrict disputed artifact distribution; continue work on clearly permitted tooling and inputs.

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
