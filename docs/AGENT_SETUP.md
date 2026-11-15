# Agent setup (Claude Code, Codex, Antigravity)

One rule source: root [AGENTS.md](../AGENTS.md). Backlog: [PLAN.md](../PLAN.md).
`docs/CODEX_HANDOFF.md` and `docs/HANDOFF_HISTORY.md` are historical records only.

| Agent | Reads | Notes |
|---|---|---|
| Claude Code | `CLAUDE.md` (imports AGENTS.md) | |
| Codex | `AGENTS.md` | branch prefix `codex/` |
| Antigravity / Gemini | `AGENTS.md`, `GEMINI.md` | branch prefix `antigravity/` |

## Environment
`scripts/setup_env.sh` (Linux: installs ALSA/udev dev headers, runs `cargo build --locked`,
prints `scripts/next.sh`). Python 3.9+ needed for tools; Rust edition 2024 (cargo >= 1.85).

## Coordination
- One agent per topic branch; stage only your own paths; never rewrite another agent's branch.
- Claim a task by naming the branch after its PLAN.md ID (e.g. `codex/E1-...`).
- Save with `scripts/gate.sh --checkpoint`; submit with `scripts/gate.sh --submit` (never merges).
