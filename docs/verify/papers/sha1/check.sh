#!/usr/bin/env bash
# Paper check: SHA-1 (FIPS 180-1 / RFC 3174, RFC 6234 test driver). Usage: docs/verify/papers/sha1/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" sha1.c yes
