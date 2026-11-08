# Next app release — candidate notes

Draft only: no version selected, tag created, assets published or checks claimed.

Ounce Core now has a native macOS workbench around the ARM64 runtime. It accepts
NRO inputs, exposes CPU engine and execution-budget settings, explains recognized
runtime failures, and compares supported harness output with bundled references.
The browser lab is a teaching prototype, not the Rust runtime compiled to WebAssembly.

## Before publishing

- Resolve and record the existing redistribution decision in
  `docs/RELEASE_NOTICE_REVIEW.json`. Its current default-runtime scope excludes the
  optional `gpu-present nce native-jit` features used by the app; review the actual
  app dependency graph and final bundle, not just the default CLI graph.
- Build on Apple Silicon with `scripts/build_macos_app.sh`; retain exact revision,
  toolchain, feature set, notices and SHA-256 of the packaged bundle.
- Run Swift logic/report tests, runtime smoke tests and a live GUI walkthrough:
  first launch, choose/drop a permitted NRO, engine/budget settings, failure
  diagnosis, reference comparison, cancellation and relaunch. Check window sizing.
- Decide explicitly between an ad-hoc-signed development preview and notarized
  distribution; explain the installation experience accurately in release notes.
- Extend release packaging to include the app. The current tag workflow publishes
  CLI archives, not the app bundle. Preserve checksums and notice verification.
- Choose a version, update matching package/tag/release notes, run required hosted
  checks, then publish the verified assets. User authorized a new release on
  2026-11-08; no repeat publication permission is needed within that scope.

Known limitations: no ELF input in the Mac app, filename-based harness matching,
committed-reference comparison rather than independent proof, and no demonstrated
live GUI acceptance yet. See `docs/MAC_APP.md` for current behavior.
