# Ounce-Core — Agent Contract (active)

## Finish line: one independently verifiable outside homebrew frame
**Stop unrelated feature work until this is met.** The first target is unchanged upstream `switch-examples/graphics/simplegfx`; second is `deko_basic`. For simplegfx, build from the pinned upstream revision, record exact source/build/toolchain and sample hash, execute through the actual VI/Binder submission path with no `BadGfxInit`/`svcBreak`, capture the submitted guest buffer (metadata and RGBA PNG), compare to the **pre-registered** pixel oracle in PR #114 (or its reviewed merged successor), reproduce interpreter/Tier-3 agreement, add appropriate CI validation and a clean-machine one-command guide, and obtain an independent reproduction. A host-rendered placeholder or synthetic harness pass does not count. Record **N outside samples producing a correct, externally verifiable frame**; begin with N=0 unless independently demonstrated otherwise. Do not claim Switch 2 or commercial compatibility.

**Order:** (1) integrate/review oracle #114; (2) trace the exact failing IPC/SVC call, including `BadGfxInit`/`0x2559`; (3) implement only the necessary VI display/layer/native-window and Binder buffer-queue requests; (4) capture/detile/export guest-submitted RGBA PNG; (5) compare against the frozen oracle and engines, CI, clean-machine replication; (6) repeat on deko_basic. Fix security regressions before merge. #112 (surface overflow guard) is already merged; #103 remains open and must be reviewed against current main before merge.

## How we work — task claims, not turf
- **Owner instruction takes priority over old file ownership.** No agent may stand down solely because a historical lane or named agent once touched a file. Never disregard safety, legal, or CI requirements.
- **Goal lead:** the agent actively executing the first-frame task owns coordination *for that bounded session*; no permanent or exclusive file owner. Before changing a file, inspect current relevant PRs and claim the specific task in a short PR/issue comment with date, intended files and expected outcome. Claims expire after 24 hours without a pushed update; reassess rather than blindly overwriting active work.
- Prefer one implementer and at most one independent reviewer. Avoid duplicate attempts and background subagents unless the owner asks. Build on an existing compatible branch/PR when feasible; otherwise use an isolated branch and clearly identify dependencies.
- Do not ask the owner to resolve routine file conflicts or choose an available agent. Ask only for genuinely human-only inputs (credentials, physical hardware, legal permissions, or policy decisions). Communicate concise results in PR comments.
- Keep unrelated PRs (#99, #104, #105, #108 and successors) **triaged, not automatically merged/closed**. #108 is already closed without merge. Do not suppress alerts or force merges merely to empty the queue.

## Token and tool budget (all agents)
- Use only tools directly needed: git, focused file reads/edits, cargo and GitHub checks. Do **not** load unrelated plugins, skills or connectors (email, design, hosting, social, etc.). This is a task-scoping rule, not an instruction to bypass mandatory tooling.
- Start from this brief and the relevant PR/commit. Search targeted symbols and read narrow line ranges; do not dump the repository, this archive or long logs into context.
- Batch independent shell commands; run focused tests first, then required full gates. Report concise result lines and failing excerpts, not entire diffs/logs.
- Don't launch subagents or repeat completed experiments without a concrete reason. Prefer cheap/local models for bounded routine work, with independent validation of critical changes.
- Keep handoffs short: **changed / verified / remaining / blocker**. Historical notes live in `docs/HANDOFF_HISTORY.md` and are read only as needed.

## Non-negotiable engineering boundaries
- Follow the root crate's `#![forbid(unsafe_code)]`; preserve the separately bounded optional Tier-4 unsafe implementation. Never add unsafe elsewhere or unauthorized dependencies.
- No proprietary firmware, keys, retail assets or re-enabled retail decryption. Use self-authored/licensed homebrew and legal provenance; distinguish original Switch sample results from Switch 2 claims.
- Maintain reproducible baselines, preserve tests and evidence, never manipulate tests to manufacture a pass. Avoid reverting peer work without review.
- **No merge to main unless all nine CI matrix jobs and applicable security checks pass on the final head, required review is complete, and no known blocker remains. Never bypass protection.**
- Required checks as appropriate: `cargo test --release`, `cargo run --release -- test`, focused external sample replay, `git diff --check`, and feature-specific CI. Synthetic 25-stage harness success is not external compatibility.
- Preserve git authorship/provenance and the repo's clean-room terminology policy.

## Current factual boundaries (2026-11-01)
- PR #103: open raw-frame path; must verify current integration and the reported Binder/VI startup stop.
- PR #112: merged overflow guard; do not describe it as awaiting initial repair.
- PR #114: open **documentation-only** pixel oracle, not an implemented/verified pixel pass.
- The next decisive artifact is a trace pinpointing the failing call, followed by an unchanged-source guest-submitted frame checked against the pre-registered oracle.
