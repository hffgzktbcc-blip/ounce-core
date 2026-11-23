#!/usr/bin/env bash
# Paper check: SHA-224/256/384/512 (FIPS 180-4, RFC 6234 test driver). Usage: docs/verify/papers/sha2/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" sha2.c yes
