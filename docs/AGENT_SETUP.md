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
