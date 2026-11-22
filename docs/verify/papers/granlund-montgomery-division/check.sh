#!/usr/bin/env bash
# Paper check: Granlund-Montgomery division by invariant integers. Usage: docs/verify/papers/granlund-montgomery-division/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" granlund-montgomery-division.c yes
