#!/usr/bin/env bash
# Paper check: Keccak-f[1600] (Bertoni, Daemen, Peeters, Van Assche, Keccak reference 3.0). Usage: docs/verify/papers/keccak-f1600/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" keccak-f1600.c yes
