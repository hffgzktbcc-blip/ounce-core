#!/usr/bin/env bash
# Paper check: 64-bit Mersenne Twister MT19937-64. Usage: docs/verify/papers/mt19937-64/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" mt19937-64.c yes
