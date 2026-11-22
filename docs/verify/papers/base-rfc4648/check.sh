#!/usr/bin/env bash
# Paper check: RFC 4648 Base16/32/64. Usage: docs/verify/papers/base-rfc4648/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" base_rfc4648.c yes
