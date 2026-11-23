#!/usr/bin/env bash
# Paper check: SM3 (GB/T 32905-2016 via draft-sca-cfrg-sm3). Usage: docs/verify/papers/sm3/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" sm3.c yes
