# Agent rules for `ounce-core`

Short on purpose: every agent loads this file at session start. Read `HANDOFF.md` next (mission, next work, toolbox). Past handoffs are in `docs/HANDOFF_HISTORY.md`; grep it for the entry you need, don't read it whole.

## Finish line 1: MET 2026-11-01

An **unmodified upstream homebrew sample shows correct pixels**: switchbrew/switch-examples `graphics/simplegfx` (pinned in `tests/corpus/manifest.json`) passes all six criteria in `docs/first-frame-oracle.md`: reproducible build, no libnx fatal at `framebufferCreate`, frames 1 and 2 captured from the submitted VI buffer match the oracle hashes for all 921,600 pixels, frame 61 is all zero, interpreter and Tier-3 agree, and the guest stops cleanly.

## Finish line 2: MET 2026-11-01

An unmodified upstream 3D GPU deko3d sample shows correct rendered frames: `graphics/deko3d/deko_basic` (pinned in `tests/corpus/manifest.json`) passes all seven criteria in `docs/deko-basic-oracle.md`. Merged in PR #121 with all 13 CI checks passing, verified bit-exact on both Interpreter and Tier-3 JIT engines, rendering the interpolated triangle with correct block-linear detiling.

## Finish line 3: MET 2026-11-01

An unmodified upstream filesystem and console sample shows correct rendered text frames: `fs/romfs` (pinned in `tests/corpus/manifest.json`) passes all criteria in `docs/romfs-oracle.md`. Merged in PR #123 with all 13 CI checks passing, verifying RomFS Unicode/ASCII asset reading, libnx default console text display, R5G6B5Unorm surface decoding, and bit-exact agreement between Interpreter and Tier-3 JIT engines.

**Outside samples with a verified frame: 3** (`simplegfx`, `deko_basic`, `romfs`).

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
- **deko_basic oracle + bring-up:** unclaimed (claim it here with a date).
- **Queue:** #104 closed (red CodeQL check on a NIST test-vector IV; owner can dismiss the alert in code scanning, then reopen and merge; branch `opencode/mimo-tests` is current). #115 (texture mips) paused.

## Merge gate (owner rule)
- Never merge to `main` until all 9 CI matrix jobs (macOS, Ubuntu, Windows x standard test, `gpu-present`, `native-jit`) are green. No admin bypass, ever.
- Before pushing: `cargo test --release` passes; `cargo run --release -- test` exits 0 with `[Harness] All N checks passed across 25 stages.`; no `unsafe` code in `src` (`#![forbid(unsafe_code)]`; the only `grep -rn unsafe src` hits are comments and that attribute).
- Build on verified work; don't revert another agent's work without a reason in the PR.

### Batch work, merge once per milestone (owner rule)

- Work on one branch per milestone. Push frequently, but merge into main only once the milestone is complete.
- Do not wait for CI between development steps. Final CI must pass before merging.
- Use one PR per milestone. Include small documentation and coordination changes in that PR.
- Before every push, run `cargo test --release`, verify harness 27/27, and check for new unsafe code.
- Fix failing CI immediately before continuing development.
- Separate PR exceptions: broken main, security fixes or changes blocking another session.
- Do not implement CI path filters or weaken workflow checks without explicit owner approval.

## Legal and scope boundary
- Target: NVIDIA Tegra T239 (Ampere GA10B GPU), Horizon OS 23.0.1 ABI. Clean-room research runtime for synthetic test vectors and homebrew only.
- Retail game bootstrapping and retail key decryption stay disabled (`loader/nca.rs` returns `LoadError::RestrictedCipher`). No proprietary keys or firmware bytes, ever.
- Don't use the word "emulator" in descriptions, docs or metadata.
- Git author: `hffgzktbcc-blip <hffgzktbcc-blip@users.noreply.github.com>`.
- Mark anything not checked against a public source or hardware as UNVERIFIED; never guess an encoding silently.
