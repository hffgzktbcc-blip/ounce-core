#!/usr/bin/env bash
# Paper check: UTF-8, UTF-16 and UTF-7 RFC examples. Usage: docs/verify/papers/unicode-utf/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" utf.c yes
