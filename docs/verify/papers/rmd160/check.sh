#!/usr/bin/env bash
# Paper check: RIPEMD-160 (Dobbertin, Bosselaers, Preneel, FSE 1996). Usage: docs/verify/papers/rmd160/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" rmd160.c yes
