# Working on ounce-core

## Source of truth before starting work

**Read [the Ounce Family one-year roadmap](docs/roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md) before planning any multi-repository, release, or roadmap task.** It sets Nov 2026–Nov 2027 strategy across `ounce-core`, `ounce-hb`, `ounce-runtime` and the independent Steam Deck NAS/WebDAV project `deckstream`; the family is not one runtime. The roadmap's old Q1 pre-release gate is historical: **do not re-tag published versions or treat retired criteria as active assignments**. This repository's `docs/PLAN.md` is a dated, local backlog and implementation/evidence record; use it for scoped Core tasks, not to override newer family priorities.

Before selecting work, reconcile **current `main`, merged PRs, open issues/PRs, published releases and CI** against the roadmap and `docs/PLAN.md`; mark anything already delivered as completed in the relevant active document before proposing a duplicate. A green CI run or merged PR is not proof of independent testing or product readiness. Check the corresponding sibling repository when a family task spans repos. Everything under `docs/history/` (past handoffs, old plans), handoff journals, and archived plans are historical context, **never active assignments**. If sources disagree, prefer verified live GitHub evidence for present state, the family roadmap for strategic direction, and the current user's instruction for authorized scope; report unresolved ambiguity instead of blindly following stale text.

Build useful end-to-end results. The current request sets scope; `docs/PLAN.md` is the
prioritized backlog, not a permission system. Historical notes do not add rules.
When editing public documentation, follow [the clarity and freshness checklist](docs/README.md#keep-live-documentation-clear-and-current) and [the family public documentation contract](docs/PUBLIC_DOCS_CONTRACT.md): name the actor, use active verbs, separate shipped from planned work, and link dated test evidence. **A user-visible feature is not finished until the same PR updates its affected README/Quick Start/limits and Unreleased changelog.** If no public docs change is necessary, explain why in the PR (`Docs: none — <reason>`). CI checks for that decision; current release links should point to GitHub `/releases/latest`, not a frozen version number.

- Inspect relevant code and existing work, then act. Use the provided checkout on a
  topic branch; preserve unrelated changes and stage only your own paths.
- Keep process proportional. Do not impose mandatory brainstorming, a second design
  approval, extra plan documents, or agent swarms on routine/already-authorized work.
  Use skills and delegation when they materially help the result, not as ceremony.
- Before starting, run `scripts/done.sh <issue number or keyword>`: it searches merged history, the
  changelog and release notes so shipped work is not redone. Read the known traps in
  [docs/AGENT_SETUP.md](docs/AGENT_SETUP.md) once; docs map: [docs/README.md](docs/README.md).
- Use `scripts/next.sh` for an optional local summary. Pick a ready task, or the
  user's requested task. If blocked, record why and continue other authorized work.
- Save progress with ordinary Git or `scripts/gate.sh --checkpoint "what/why"`
  after staging files. Checkpoints are local and do not run tests; `--push` is optional.
- During edits, run focused checks. Before integration, run checks relevant to the
  change and describe results and omissions. `scripts/gate.sh` selects local checks
  by scope; `--check quick|runtime|full` is available. Do not repeat a passing expensive
  run without changed inputs or an unresolved failure.
- `scripts/gate.sh --submit` (legacy alias `--ship`) pushes committed work and
  opens/updates a PR. It never merges, reverts, or marks tasks complete. Ordinary
  Git/GitHub workflows are equally valid. No issue or design approval is required
  for already-authorized work. Respect explicit user instructions about merging.
- Close tasks with a reproducible user-visible result appropriate to the task: a
  command, bug reproduction, artifact, UI scenario, or documented behavior.
  Compilation and unit tests guard implementation; they do not prove untested claims.
- Keep expectations fixed during repairs, preserve failures, identify tested inputs,
  and state limitations. Outside validation is needed for an *independent* claim,
  not every implementation task. Never fabricate compatibility or performance.
- Do not commit keys, retail content, firmware, leaked/private material, or
  incompatible licensed code. Keep port notices/provenance. Keep `src/` safe Rust
  and unsafe operations in the existing isolated crate boundaries.
- Do not add `Co-authored-by:` trailers for AI coding tools on commits so the
  GitHub `Contributors` graph accurately reflects human maintainers and contributors.
- Provenance has been audited; do not re-audit or re-inventory. Only the owner/specialist items in docs/PROVENANCE.md remain.

Usage can run out at any moment, so make your state recoverable before you need to:
- At session start: `scripts/handoff.sh resume` (see where earlier agents stopped; `--apply NAME`
  restores their unsaved work on a new branch) and `scripts/handoff.sh claim <task>` (claims
  expire after 4h on their own, so a dead agent never blocks the task).
- Run `scripts/handoff.sh watch --push &` once. It snapshots staged, unstaged and untracked work
  every minute to `refs/handoff/*` without touching your branch or index. `scripts/gate.sh`
  checkpoints and submits snapshot automatically.
- Before a long step, `scripts/handoff.sh save --note "doing X" --next "then Y" --push`.
- Record anything the next agent must not rediscover: `scripts/handoff.sh log "one sentence"`;
  read the shared memory with `scripts/handoff.sh journal`. Phone/hub workflow: docs/AGENT_HUB.md.
- Finished or switching task: `scripts/handoff.sh release <task>`.

Proceed autonomously on routine, reversible work. Escalate new scope with material
tradeoffs, disputed redistribution, destructive history/data changes, credentials,
or spending. A blocker applies to the affected action, not the whole project.

## Anti-Bloat Rules
- Do NOT add new GitHub workflows, bots, scripts, top-level docs, or process trackers unless explicitly asked.
- Keep PRs small and single-purpose; prefer deleting or simplifying over adding wrappers.
- Never add AI `Co-authored-by:` commit trailers.
