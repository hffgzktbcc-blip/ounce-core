# Metadata-Only `ounce.repro.v1` Regression Corpus

This directory stores **metadata-only** `ounce.repro.v1` JSON manifests for deterministic homebrew regression checking (#472).

## Rules

- **Zero binaries:** Never commit `.nro`, `.romfs`, `.nsp`, `.xci`, or any other binary artifact in `tests/repro/`. Only JSON manifests and this `README.md` belong here.
- **Provenance recorded:** Every manifest records `source_repo`, `source_path`, `source_commit`, `toolchain`, `fixture_path`, and the pinned `nro_sha256` from [`tests/fixtures/provenance.json`](../fixtures/provenance.json) and [`tests/fixtures/nro/TOOLCHAIN_VERSIONS.md`](../fixtures/nro/TOOLCHAIN_VERSIONS.md).
- **Deterministic replay:** `tests/repro_corpus.rs` runs automatically under `cargo test`, verifies that `tests/repro/` contains no binary files, validates every manifest's metadata, and replays each referenced fixture through `ounce-core run-nro --present=headless --deterministic` checking exact match on `stop_kind`, `exit_code`, `insns_executed`, `frames_presented`, `framebuffer_hash`, `audio_frames_mixed`, and `audio_hash`.
