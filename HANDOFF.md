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
