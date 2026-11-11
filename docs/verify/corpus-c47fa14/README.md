# Fresh corpus run at c47fa14 (2026-11-11)

Maintainer run of `scripts/verify_corpus_run.py --shards 2 --check-readme` on a clean shared clone of
`main` at `c47fa14`, macOS arm64 host, Docker in a Lima VM. Result: 50/50 programs pass on every engine.

- `canonical-corpus.json`: the canonical report (source of `tests/corpus/oracle-results.json`).
- `oracle-verdicts.json`, `ounce-verify-evidence.json`: per-oracle verdicts and the verifier record.
- `run-manifest.json`: stages and commands; the local run directory is replaced by `$RUN_ROOT`.
- `toolchain-image.json`, `toolchain-packages.txt`: devkitA64 image and package versions.

Per-program build and oracle logs were not committed (about 540 MB with builds). This is not an
independent reproduction (PLAN E1).
