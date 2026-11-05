# ounce-core handoff

## 1. Mission
Build a clean, inspectable, and high-performance AArch64 research runtime that executes real software.

## 2. Philosophy & Engineering Standard
- Execution-driven engineering: let real software execution uncover what to build and fix next.
- Zero fake compatibility, zero stubbing away real crashes.
- Clean-room only: clean Rust, `#![forbid(unsafe_code)]` in core, no proprietary retail keys committed to the repository.

## 3. Workflow
- Test locally before push: `cargo test --release` and `cargo run --release -- test`.
- Push directly on milestone feature branches; merge to `main` once passing CI.

## Mac launcher preview (2026-11-05)
Native AppKit launcher and local packaging script added on `codex/macos-launcher`.
The ordinary GUI now presents decoded VI surfaces, including crop, without the
synthetic framebuffer overlay. The existing simplegfx oracle also checks this GUI
route against the frame-2 hash (regression failed before the fix).
Local checks: 467 default tests, 536 all-feature workspace tests, 36 harness checks.
Installed app opened the outside simplegfx NRO through its file picker on Apple M3
Metal; guest 1280x720 frames observed, Stop restored the ready controls. The bundled
graphics diagnostic remains a basic strip/gradient display, not a compatibility demo.
Compatibility remains 20/20; corpus expansion is still the next PLAN milestone.
