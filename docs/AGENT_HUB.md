# Running several agents from your phone

One session is your **hub**: the one you talk to from your phone (Claude Desktop with Remote Control on). Other agents (more Claude sessions, Codex, Antigravity) do the work. Everyone shares memory through git, so any of them can run out of usage and the next picks up.

## What is shared
| What | Where | Command |
|---|---|---|
| What each agent is doing and where it stopped, including unsaved work | `refs/handoff/<agent>/<branch>` | `scripts/handoff.sh save`, `watch`, `resume` |
| Who owns which task (expires after 4h) | `refs/handoff/claims/<task>` | `claim`, `release`, `status` |
| What was decided or learned (the shared memory) | `refs/handoff/journal` | `log "text"`, `journal` |
| One-screen summary for a phone | computed | `scripts/handoff.sh digest` |

All of it lives in the repository's git, so it works across machines. Nothing is stored outside your GitHub repo, and nothing needs a server.

## Every agent, at session start
```sh
scripts/handoff.sh resume          # where did earlier agents stop?
scripts/handoff.sh journal         # what do we already know?
scripts/handoff.sh claim <task>    # take a task (expires on its own)
scripts/handoff.sh watch --push &  # autosave unsaved work every minute
```
While working, `scripts/handoff.sh log "..."` for anything the next agent must not rediscover: a decision, a trap, a result. Keep entries to one sentence. Never put secrets or keys in notes or in the working tree; snapshots are not encrypted.

## The hub, from your phone
Ask it things like "what is everyone doing?" and it should:
1. run `scripts/handoff.sh digest` and reply with the result;
2. list sessions (`list_sessions`) and, for details, search or read a session's transcript (`search_session_transcripts`, `get_session`, `list_events`) instead of asking you to open it;
3. start a new session for an independent task (`start_session`) and tell it to claim the task first;
4. forward your instruction to a running session (`send_message`);
5. when an agent reports it is out of usage, run `scripts/handoff.sh resume --apply <name>` in a fresh session so the next agent continues from the unsaved state, then release the dead agent's claim if it has not expired.

Agents that cannot receive session messages (Codex, Antigravity) still read and write the journal and claims, because those are plain git refs.

## Limits
- Messages are polling, not push: an agent sees new journal entries when it runs `journal`, not instantly.
- Claims are advisory and race-safe on origin, but nothing stops an agent that ignores them.
- Transcripts are only visible to sessions on the same Claude Desktop; other tools share through the journal.

## Which tool reads what
Every tool lands on the same rules and the same git-based memory, so none needs to talk to another directly.

| Tool | Reads on start | Talks through |
|---|---|---|
| Claude Code / Desktop | `CLAUDE.md` → `AGENTS.md` | git refs (`handoff.sh`), plus session messaging between Claude sessions |
| Codex | `AGENTS.md` | git refs (it can run shell commands) |
| Antigravity (Gemini) | `GEMINI.md` → `AGENTS.md` | git refs; optionally the local mailbox MCP |
| GitHub Copilot (agent mode) | `.github/copilot-instructions.md` | git refs if it can run commands; otherwise the PR conversation |
| Copilot chat, no shell | the files in the repo | you relay: paste `scripts/handoff.sh digest` to it, and its answer back |

**Rule of thumb:** durable facts go in the journal (`log`), live ownership goes in claims, unsaved work goes in snapshots. Anything between two agents on the same Mac that needs a quick reply can use a local mailbox (an MCP server or a shared file); anything that must survive a session ends goes in git.
