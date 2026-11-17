# Handoff: repo consolidation, 17 Nov 2026

**Historical record, not an assignment.** Live state is `scripts/next.sh`, `scripts/done.sh <issue or keyword>`,
[PLAN.md](../../../PLAN.md) and GitHub. Written because `scripts/handoff.sh log` could not publish to
`refs/handoff/journal` in this session (the push to a shared ref was denied); re-log the lines under
"Journal lines" when that is allowed.

## What was done (branch `claude/fervent-goodall-l76niz`, PR #462, draft)
- Past handoffs, old plans, outreach and recruitment drafts, pre-run corpus proposals and the E2 memo moved to `docs/history/`; `docs/README.md` indexes live vs history.
- `PLAN.md` compacted (done tasks are one-liners; evidence in `docs/history/PLAN-COMPLETED.md`; IDs and dependencies unchanged so `next.sh` works).
- Stale statements fixed after checking git, tags and code: CHANGELOG and v0.9.0 notes (v0.9.0 is tagged at `4b63f7a` and released 2026-11-16), `CITATION.cff` (0.9.0), `--report-json` ships in v0.9.0, wasm3 runs (#366 fixed by #429), 28 harnesses / 76 rows, FPSR flags raised by the interpreter and Tier-3, roadmap and `milestones.json` (#400-#404, #387, #418, #419, `--max-frames`/`--press` done).
- New tooling: `scripts/refresh_notice_review.py` (+ check in `gate.sh`), `scripts/done.sh`, recently-merged list in `next.sh`, CI gate message for cancelled runs, `docs/AGENT_SETUP.md` known traps.
- Merged: #461 (docs), #458 (`--max-frames`/`--press`). `main.rs` conflicts resolved and pushed on #460 and #459 (their first hosted CI pending when this was written).

## Not done / pending decisions
- Merge #460 and #459 only after their hosted CI is fully green. #459 changes the `--sdmc` default (guest writes no longer reach the host folder without `--sdmc-writeback`) and removed `applet-software-keyboard` from `tests/corpus/soak-known.json` without a soak re-run; check "Corpus soak smoke".
- Redo the `src/main.rs` split (4.2k lines) into `src/cli/{usage,vm,demos,run}.rs` after #459/#460 land. It was built and passed 640 unit tests (commit `68c1727`) but reverted (`a118721`) so those PRs would not need hand-porting. Method: extract by line range, make items `pub(crate)`, start each module with `use crate::*;`, keep `main()` and the statics in `main.rs`.
- CodeQL raised 9 "cleartext logging" alerts on the split (moved code gets new file locations). They look like false positives (offline stand-in account; VM status prints). Dismissal is the owner's call and was not done.
- Owner approval needed before closing fixed issue #383 (by #452) and obsolete-looking #359, #298, #211, #212.
- `site/status.html` does not mention v0.9.0. Workflow automation (23 workflows) and sibling repos (`ounce-hb`, `ounce-runtime`, `deckstream`) were not audited or pruned.

## Traps (full list: docs/AGENT_SETUP.md)
1. Editing `Cargo.toml` (even a comment), `Cargo.lock` or `THIRD_PARTY_NOTICES.txt` breaks "Release notice packaging"; run `python3 scripts/refresh_notice_review.py` or revert a comment-only edit.
2. A new push cancels older CI runs; failures on an older commit (`Artifact not found`, a red gate whose jobs say `cancelled`) are not real.
3. `milestones.json` renders into `README.md` and `docs/DETAILS.md`; edit the JSON, run `scripts/render_roadmap.py`.
4. Pre-registered oracle docs and design notes that code points at must not move.

## Journal lines (for `scripts/handoff.sh log`)
- Before starting any work run `scripts/done.sh <issue# or keyword>`; v0.9.0 is released and #387 #366 #362 #400-#404 #418 #419 #383 #458 are already done.
- `main.rs` split is built and verified but deferred until #459 and #460 merge; see this note for the method.
- Do not dismiss the 9 CodeQL alerts or close issues #383 #359 #298 #211 #212 without owner approval.
