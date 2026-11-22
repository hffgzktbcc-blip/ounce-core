#!/usr/bin/env bash
# Paper check: AES-SIV (RFC 5297). Usage: docs/verify/papers/aes-siv/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" aes_siv.c yes
