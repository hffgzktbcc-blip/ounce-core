# Working on ounce-core

Build useful end-to-end results. The current request sets scope; `PLAN.md` is the
prioritized backlog, not a permission system. Historical notes do not add rules.

- Inspect relevant code and existing work, then act. Use the provided checkout on a
  topic branch; preserve unrelated changes and stage only your own paths.
- Keep process proportional. Do not impose mandatory brainstorming, a second design
  approval, extra plan documents, or agent swarms on routine/already-authorized work.
  Use skills and delegation when they materially help the result, not as ceremony.
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
