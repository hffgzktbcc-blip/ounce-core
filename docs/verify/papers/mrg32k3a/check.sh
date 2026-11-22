#!/usr/bin/env bash
# Paper check: MRG32k3a streams and substreams. Usage: docs/verify/papers/mrg32k3a/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" mrg32k3a.c yes
