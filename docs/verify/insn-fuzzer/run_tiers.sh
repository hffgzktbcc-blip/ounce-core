#!/usr/bin/env bash
# CI-friendly tier check: interpreter vs Tier-3 vs Tier-4 on random words, no host needed (any OS).
# Usage: run_tiers.sh [words-per-seed]   (default 20000; run from the repo root)
# On Apple Silicon macOS it also replays host traces through tier3 and tier4 (needs fuzz_host.c).
# Exit status is non-zero if any engine differs. Takes about 15 s for the defaults.
set -eu -o pipefail
here="$(cd "$(dirname "$0")" && pwd)"
words="${1:-20000}"
cargo build --release --features native-jit
bin=target/release/ounce-core
rc=0
for seed in 1 2 3; do
  "$bin" fuzz-replay --engine-vs-engine --seed "$seed" --count "$words" --show 5 | tail -n 8 || rc=1
done
if [ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ]; then
  cc -O1 -Wall -o "${TMPDIR:-/tmp}/fuzz_host" "$here/fuzz_host.c"
  for engine in tier3 tier4; do
    "${TMPDIR:-/tmp}/fuzz_host" --seed 1 --count "$words" | "$bin" fuzz-replay --engine "$engine" - > "${TMPDIR:-/tmp}/tiers_$engine.txt" || rc=1
    grep -E "engine:|  MISMATCH|other errors" "${TMPDIR:-/tmp}/tiers_$engine.txt"
  done
fi
exit "$rc"
