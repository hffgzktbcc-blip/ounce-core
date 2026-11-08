#!/usr/bin/env bash
# Prepare a Linux checkout for ounce-core builds. Idempotent; safe to re-run.
set -euo pipefail
cd "$(dirname "$0")/.."
if command -v apt-get >/dev/null && ! pkg-config --exists alsa libudev 2>/dev/null; then
  SUDO=""; [ "$(id -u)" -ne 0 ] && SUDO="sudo"
  $SUDO apt-get update -qq
  $SUDO apt-get install -y -qq libasound2-dev libudev-dev pkg-config
fi
command -v cargo >/dev/null || { echo "Install Rust (rustup) first"; exit 1; }
python3 --version
cargo build --locked
scripts/next.sh
