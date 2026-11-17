# Agent setup (Claude Code, Codex, Antigravity, Copilot)

One rule source: root [AGENTS.md](../AGENTS.md). Backlog: [PLAN.md](../PLAN.md). Doc map:
[README.md](README.md). Everything under `docs/history/` (including past handoffs) is
historical record only: do not take tasks or rules from it.

| Agent | Reads | Branch prefix |
|---|---|---|
| Claude Code | `CLAUDE.md` (imports AGENTS.md) | `claude/` |
| Codex | `AGENTS.md` | `codex/` |
| Antigravity / Gemini | `GEMINI.md` -> `AGENTS.md` | `antigravity/` |
| Copilot agent | `.github/copilot-instructions.md` -> `AGENTS.md` | `copilot/` |

## Environment
`scripts/setup_env.sh` (Linux: installs ALSA/udev dev headers, runs `cargo build --locked`,
prints `scripts/next.sh`). Python 3.9+ for tools; Rust edition 2024 (cargo >= 1.85).

## Coordination
- One agent per topic branch; stage only your own paths; never rewrite another agent's branch.
- Claim a task with `scripts/handoff.sh claim <PLAN.md ID>` (expires after 4h); release with `release`.
- Save with `scripts/gate.sh --checkpoint`; submit with `scripts/gate.sh --submit` (never merges).
- Shared memory, snapshots and the phone-hub workflow: [AGENT_HUB.md](AGENT_HUB.md).

## Known traps (each one has cost real time)
1. **Pinned notice hashes.** Editing `Cargo.toml` (even a comment), `Cargo.lock` or
   `THIRD_PARTY_NOTICES.txt` makes the "Release notice packaging" CI job fail with
   `Notice review identity is stale`. `scripts/gate.sh` now catches it locally. If the change is
   intended, look at the `Cargo.lock` diff for new licences, then run
   `python3 scripts/refresh_notice_review.py` and commit the updated `docs/RELEASE_NOTICE_REVIEW.json`.
   For comment-only edits, revert instead.
2. **Pushing again cancels CI.** Each branch keeps only its newest CI run. Failures on an older
   commit after a new push (including `Artifact not found: corpus-nros` and a red `Test (...)`
   gate whose jobs all say `cancelled`) are not real. Read the check list for the head commit.
3. **Where the CLI lives.** `src/main.rs` is now only `main()` and the command dispatch (about
   240 lines). Command code is in `src/cli/`: `usage.rs` (banner/help/specs), `vm.rs` (VM setup,
   `StageGate`, `run_vm_internal`), `demos.rs` (built-in test suite and demos), `run.rs`
   (`run`, `run-nro`, directory packages, NRO helpers). Add new commands in `src/cli/` or their own
   module, not in `main.rs`, so parallel PRs do not collide. Each module starts with `use crate::*;`.
4. **Branch names.** Use the prefix in the table above. Other prefixes already in use are
   `feat/`, `fix/`, `docs/`, `cursor/`; the claim is the task ID in the name or `handoff.sh claim`.
5. **Do not take work from `docs/history/`.** Handoffs and old plans there are records; the live
   backlog is `PLAN.md` plus open issues and PRs.
6. **Reading CI.** `scripts/gate.sh` is the fast local equivalent. A hosted job that needs
   artifacts from another job fails as a cascade when that job fails; fix the first red job.
7. **Version and link drift is checked in CI.** `python3 scripts/check_pins.py` (Cargo.toml, CITATION.cff, CHANGELOG
   and `docs/releases/` must agree) and `python3 scripts/check_links.py` (no broken relative Markdown links) run as
   tests. When you cut a release or move a doc, run them first.
