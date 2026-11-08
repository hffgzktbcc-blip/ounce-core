#!/usr/bin/env bash
# Usage: run.sh [seed] [words]   (Apple Silicon macOS only; run from the repo root after `cargo build --release`)
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
seed="${1:-1}"; words="${2:-200000}"
cc -O1 -Wall -o "${TMPDIR:-/tmp}/fuzz_host" "$here/fuzz_host.c"
"${TMPDIR:-/tmp}/fuzz_host" --seed "$seed" --count "$words" | target/release/ounce-core fuzz-replay -
