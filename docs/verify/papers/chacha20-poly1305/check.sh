#!/usr/bin/env bash
# Paper check: ChaCha20-Poly1305 (RFC 8439). Usage: docs/verify/papers/chacha20-poly1305/check.sh <new out dir> [path to ounce-core]   (from the repo root)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "$here/../common/lib.sh"
paper_check "$here" "${1:?usage: check.sh <new out dir> [ounce-core binary]}" "${2:-target/release/ounce-core}" chacha20poly1305.c yes
