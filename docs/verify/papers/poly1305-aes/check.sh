#!/usr/bin/env bash
# Paper check: Poly1305-AES. Usage: docs/verify/papers/poly1305-aes/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" poly1305aes.c yes
