# READ THIS FIRST (overrides everything below; owner decision 2026-11-03)

1. **The only plan is [`PLAN.md`](PLAN.md).** Read it before any work, do only its next unchecked step, tick it with
   one line of proof, then end the session. `HANDOFF.md` and older notes are history, not plans.
2. **Proof = a real outside homebrew program passing its pixel oracle in CI.** Unit tests guard code but are never
   a milestone or a "verified" claim.
3. **Follow PLAN.md's Legal guardrails on every commit:** never decrypt, no piracy pointers, no game names or game
   screenshots, no retail test inputs, nothing from the owner's non-public projects.
4. **Save the owner's usage:** one step per session, no re-exploring what PLAN.md already says, logs read as text,
   long jobs in the background, work only in `~/Projects` (`~/Documents` is iCloud-synced and stalls).
5. **Agents test themselves.** The owner is never the test harness.

---

# Agent rules for `ounce-core`

Short on purpose: every agent loads this file at session start. The plan is [`PLAN.md`](PLAN.md); finished milestones and past handoffs are history in `docs/HANDOFF_HISTORY.md` (grep it only when a step needs it).

**Outside samples with a verified frame: 4** (`simplegfx`, `deko_basic`, `romfs`, `startup`; oracles in `docs/*-oracle.md`).

## How we work
- The owner's goal is our goal: a working, high-performance runtime for open-source Switch homebrew, where every claim rests on a real program's frame.
- Focus strictly on real engineering: CPU execution, GPU method dispatch, kernel services, memory management, and getting software running.
- No artificial roadblocks: work directly on whatever code needs fixing to make software run.
- Keep the roots clean: `#![forbid(unsafe_code)]` in the core runtime, zero retail keys in the repo. Sources and porting rules are in PLAN.md ("Sources"): MIT code (nxnative, Ryujinx, Mesa NAK) may be ported with credit; no GPL emulator code, no retail material.

## Merge gate
- Before pushing to PR: `cargo test --release` passes; `cargo run --release -- test` passes; zero `unsafe` in `src`.
- CI must pass on the milestone PR before merging to `main`.
- Push directly and frequently on your branch; batch merge once milestone is complete.

## Legal boundary
- PLAN.md's "Legal guardrails" apply to every commit, and CI enforces them (`scripts/check_legal_guardrails.sh`). ounce-core is not a clean-room project (see README, "Sources").
- No proprietary keys (`prod.keys`) or retail copyrighted game assets committed to the git repository.
- Use open documentation and permissive code: ARM manuals, Mesa NVK/nouveau, NVIDIA open-gpu-doc, public SwitchBrew specifications, and MIT code as PLAN.md allows.
