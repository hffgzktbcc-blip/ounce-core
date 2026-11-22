#!/usr/bin/env bash
# Paper check: Rabinowitz-Wagon spigot for pi. Usage: docs/verify/papers/rabinowitz-wagon-spigot/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" rabinowitz-wagon-spigot.c yes
