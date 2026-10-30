# Rust quality tools — adoption and Codex handoff (2026-10-29)

Status: proposed workflows on a separate draft pull request. CodeQL Rust and RustSec completed successfully on exact PR head `69d1569c23d7e880ad34b8456778c94284fe7e46`; the manual coverage workflow remains unproven until it can run from the default branch. Existing `.github/workflows/ci.yml` and its nine required platform/feature jobs are unchanged.

## Implemented in this proposal

- `rust-security.yml`: CodeQL Rust analysis on source or manifest changes, and a RustSec `cargo audit --file Cargo.lock` scan. A weekly schedule catches newly disclosed advisories. Both jobs may surface findings that require review, not blind dependency upgrades.
- `rust-coverage.yml`: manually triggered LLVM source-coverage measurement on Ubuntu using `cargo-llvm-cov`; uploads `coverage.lcov` as a short-lived GitHub Actions artifact. A line-coverage count is not a correctness or external compatibility claim.
- Neither workflow consumes external sample binaries, guest keys, firmware, or game content. Neither alters guest runtime behavior or test verdict semantics.

## Owner/agent coordination
`AGENTS.md` reserves `.github/workflows/*` for Track A / Antigravity, so these additions **must receive the workflow owner's acknowledgement** before landing. No existing workflow or agent-owned file was edited. Changes are isolated to two new workflow files and this note. Keep PR draft until approval.

## Validation checklist for Codex
1. Read AGENTS.md and notify the workflow owner in the PR discussion before merge. Review YAML semantics and action versions; check that CodeQL Rust scanning is supported in the current GitHub environment and the repository has `security-events: write` access.
2. Check the PR CI run: all **nine existing** macOS/Ubuntu/Windows standard, gpu-present and native-jit jobs must pass. Do not use bypass, or interpret a successful security scan as application compatibility.
3. Inspect `Rust security checks` results: CodeQL build completes with ALSA/udev headers, and `cargo-audit` installs and scans the lockfile successfully. RustSec findings need an explicit security triage; do not auto-ignore or silence them.
4. Once the workflow is on main, manually invoke `Rust coverage (on demand)`; verify the report artifact contains legitimate source lines, that there are no unhandled native dependency failures, and record an initial baseline. Avoid invented percentages or thresholds.
5. If GitHub fails to trigger the new workflows on the draft PR, review the event/path filters and GitHub security settings. Note that workflow_dispatch is generally exposed for workflows on the default branch after merge.
6. Keep these jobs separate until reliable. Do not add them to required branch protection checks or change nine-job policy without owner approval.

## Later enhancements (not added here)
- `cargo-fuzz` on bounded parsers in a dedicated CI or scheduled fuzzing process after the parser owners acknowledge the exact targets and time budget; avoid duplicate in-repo synthetic fuzz tests.
- `cargo-nextest` performance comparison only after parity with the existing test harness is demonstrated.
- Renovate/Dependabot should produce **small, reviewable** dependency PRs only after the integration backlog is reduced. No automatic merges.
- Frontend/Tauri is independent of security workflows and should consume versioned Ounce-Verify results; do not duplicate runtime compatibility decisions inside the UI.
