#!/usr/bin/env bash
# Paper check: Streebog / GOST R 34.11-2012 (RFC 6986). Usage: docs/verify/papers/streebog/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" streebog.c yes
