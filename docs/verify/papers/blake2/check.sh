#!/usr/bin/env bash
# Paper check: BLAKE2 (Aumasson, Neves, Wilcox-O'Hearn, Winnerlein, ACNS 2013). Usage: docs/verify/papers/blake2/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" blake2.c yes
