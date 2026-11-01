# Agent rules for `ounce-core`

Short on purpose: every agent loads this file at session start. Past handoffs are in `docs/HANDOFF_HISTORY.md`; grep it for the entry you need, don't read it whole.

## Finish line (feature work stops until this is met)

An **unmodified upstream homebrew sample shows correct pixels**: switchbrew/switch-examples `graphics/simplegfx` (pinned in `tests/corpus/manifest.json`) passes all six criteria in `docs/first-frame-oracle.md`: reproducible build, no libnx fatal at `framebufferCreate`, frames 1 and 2 captured from the submitted VI buffer match the oracle hashes for all 921,600 pixels, frame 61 is all zero, interpreter and Tier-3 agree, and the guest stops cleanly.

Also required: the oracle step passes in CI (`external-corpus.yml`), and someone other than the author reruns it from `docs/REPRODUCE_SIMPLEGFX.md` and gets the same result. Synthetic harness passes and host-rendered placeholders don't count. No Switch 2 or commercial-compatibility claims.

**Outside samples with a verified frame: 0** (simplegfx passes locally in #103; it counts after CI, merge and an independent rerun). Next: `deko_basic` (needs the skipped 2D class `0x902D` methods).

## Token budget rules (all agents)
- Use only what the current task needs: git, cargo, the GitHub MCP tools, and file read/edit.
- Don't load plugins, skills or connectors (Figma, Slack, Gmail, Drive, Vercel, Netlify, Unity, design, etc.). None of them are needed for this repo.
- Don't spawn subagents unless the owner asks.
- Read only the lines you need (grep, then sed -n ranges). Don't dump whole files or long logs.
- Don't paste full diffs or test output into PR bodies or chat; give the result line and the failing part only.
- Don't re-read `docs/HANDOFF_HISTORY.md` for context; the finish line and your current claim are enough.
- Batch independent commands into one call.
- Keep PR bodies and handoff entries short: what changed, what's verified, what's left.
- No work outside the finish line until it is met.

## How we work (replaces lanes)
- The owner's instruction wins. If the owner asks you to do something, do it, even in files another agent touched. Lanes never justify standing down.
- One goal at a time: the finish line above. Every agent works on it or stops.
- One lead per goal, named below. The lead decides who does what next.
- Claims are short-lived: claim a task (not a directory) with a date. A claim older than 24 h with no push is void; anyone may take it.
- To avoid collisions: check open PRs touching the same file before starting; if one exists, build on it or comment, don't wait.
- Ask the owner only for things only a human can do (credentials, hardware, permissions). Otherwise act.
- Communicate through this file and PR comments, not through the owner.

## Lead and current claims
- **Lead (finish line): Claude.** (2026-11-01)
- **VI/Binder + simplegfx oracle: Claude** (2026-11-01). #103 (#112 merged into it). Oracle passes locally; 9 CI jobs green, oracle CI step running.
- **Open for a second session: independent rerun** of `docs/REPRODUCE_SIMPLEGFX.md` on a clean machine; post the result on #103.
- **Queue:** #114, #105, #99 ready and green, waiting for the owner to merge (agent merges are blocked by the session's permission check). #104 needs a rebase after #105. #108 closed (infra, off-goal). #116 and #118 closed as duplicates of this file (#118's stricter points are folded in above).
- **Paused (feature work, not on the finish line):** #115 texture mips.

## Merge gate (owner rule)
- Never merge to `main` until all 9 CI matrix jobs (macOS, Ubuntu, Windows x standard test, `gpu-present`, `native-jit`) are green. No admin bypass, ever.
- Before pushing: `cargo test --release` passes; `cargo run --release -- test` exits 0 with `[Harness] All N checks passed across 25 stages.`; no `unsafe` code in `src` (`#![forbid(unsafe_code)]`; the only `grep -rn unsafe src` hits are comments and that attribute).
- Build on verified work; don't revert another agent's work without a reason in the PR.

## Legal and scope boundary
- Target: NVIDIA Tegra T239 (Ampere GA10B GPU), Horizon OS 23.0.1 ABI. Clean-room research runtime for synthetic test vectors and homebrew only.
- Retail game bootstrapping and retail key decryption stay disabled (`loader/nca.rs` returns `LoadError::RestrictedCipher`). No proprietary keys or firmware bytes, ever.
- Don't use the word "emulator" in descriptions, docs or metadata.
- Git author: `hffgzktbcc-blip <hffgzktbcc-blip@users.noreply.github.com>`.
- Mark anything not checked against a public source or hardware as UNVERIFIED; never guess an encoding silently.
