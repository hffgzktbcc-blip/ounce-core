# Agent rules for `ounce-core`

Short on purpose: every agent loads this file at session start. Read `HANDOFF.md` next (mission, next work, toolbox). Past handoffs are in `docs/HANDOFF_HISTORY.md`; grep it for the entry you need, don't read it whole.

## Finish line 1: MET 2026-11-01

An **unmodified upstream homebrew sample shows correct pixels**: switchbrew/switch-examples `graphics/simplegfx` (pinned in `tests/corpus/manifest.json`) passes all six criteria in `docs/first-frame-oracle.md`: reproducible build, no libnx fatal at `framebufferCreate`, frames 1 and 2 captured from the submitted VI buffer match the oracle hashes for all 921,600 pixels, frame 61 is all zero, interpreter and Tier-3 agree, and the guest stops cleanly.

## Finish line 2: MET 2026-11-01

An unmodified upstream 3D GPU deko3d sample shows correct rendered frames: `graphics/deko3d/deko_basic` (pinned in `tests/corpus/manifest.json`) passes all seven criteria in `docs/deko-basic-oracle.md`. Merged in PR #121 with all 13 CI checks passing, verified bit-exact on both Interpreter and Tier-3 JIT engines, rendering the interpolated triangle with correct block-linear detiling.

## Finish line 3: MET 2026-11-01

An unmodified upstream filesystem and console sample shows correct rendered text frames: `fs/romfs` (pinned in `tests/corpus/manifest.json`) passes all criteria in `docs/romfs-oracle.md`. Merged in PR #123 with all 13 CI checks passing, verifying RomFS Unicode/ASCII asset reading, libnx default console text display, R5G6B5Unorm surface decoding, and bit-exact agreement between Interpreter and Tier-3 JIT engines.

## Finish line 4: MET 2026-11-01

An unmodified upstream startup application sample shows correct rendered text frames: `templates/application` (pinned in `tests/corpus/manifest.json`) passes all criteria in `docs/startup-oracle.md`. Merged in PR #125 with all CI checks passing, verifying default text console display and bit-exact agreement between Interpreter and Tier-3 JIT engines.

**Outside samples with a verified frame: 4** (`simplegfx`, `deko_basic`, `romfs`, `startup`).

## How we work
- The owner's goal is our goal: build a working, high-performance, clean-room research runtime.
- Focus strictly on real engineering: CPU execution, GPU method dispatch, kernel services, memory management, and getting software running.
- No artificial roadblocks: work directly on whatever code needs fixing to make software run.
- Keep the roots clean: `#![forbid(unsafe_code)]` in the core runtime, zero retail keys in the repo, clean-room implementation only.

## Merge gate
- Before pushing to PR: `cargo test --release` passes; `cargo run --release -- test` passes; zero `unsafe` in `src`.
- CI must pass on the milestone PR before merging to `main`.
- Push directly and frequently on your branch; batch merge once milestone is complete.

## Legal and clean-room boundary
- Clean-room implementation only. Target: NVIDIA Tegra T239 (Ampere GA10B GPU), Horizon OS ABI.
- No proprietary keys (`prod.keys`) or retail copyrighted game assets committed to the git repository.
- Use open documentation: ARM manuals, Mesa NVK/nouveau, NVIDIA open-gpu-doc, public SwitchBrew specifications.
