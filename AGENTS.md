# Agent contract (owner decision 2026-11-07)

PLAN.md is the single source of truth. Everything else — handoffs, memory, old
docs — is history, not instructions.

## Non-negotiables

- **Step IDs are unique.** Never reuse a PLAN.md step number. `gate.sh` refuses
  to checkpoint an ambiguous ID.
- **Agents never merge and never arm auto-merge.** The owner merges after
  reviewing evidence and required CI.
- **No self-replanning.** Do not create replacement plans, branches or
  sub-agents without a demonstrated blocker. Two substantially different repair
  attempts per blocker, then report and stop.
- Guardrails (legal, `unsafe` boundary, provenance) are enforced by CI and by
  `gate.sh`. No retail content, keys, firmware or piracy pointers, ever.

## Session sequence

1. `scripts/next.sh` — fast, local; shows the one active step and its
   checkpoint. (`--full` adds PR and canonical-oracle status via GitHub.)
2. Check branch, dirty tree and existing PRs for this step before editing.
   One cohesive milestone branch/PR at a time.
3. Work the step's tasks as one batch on a `step<N>-<topic>` branch.
4. `scripts/gate.sh --checkpoint "<what/why>"` saves state at meaningful
   batches (lightweight: guardrails + commit + push). Add `--validate` when the
   batch changes code and you need RED/GREEN evidence recorded.
5. `scripts/gate.sh` (no arguments) runs the full local validation without
   committing. Run it once before shipping, and again whenever relevant code
   changes.
6. `scripts/gate.sh --ship` re-validates, ticks the step with proof and opens
   the PR. Then **report and stop** — merging is the owner's action.

## Environment

Cloud agents: do not assume `~/Projects`, Docker, a logged-in `gh`, or the
owner's macOS toolchain. Use the pinned container recipes in `docs/` when a
toolchain is needed. Local agents on the owner's Mac: same contract; the
machine has Docker and the native toolchain.

