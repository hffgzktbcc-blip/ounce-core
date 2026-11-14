#!/usr/bin/env bash
# One-command bootstrap for a fresh Oracle Cloud VM (Ubuntu/Debian or Oracle Linux)
# or any Linux cloud workstation. Installs system deps, Rust, GitHub CLI, clones
# the Ounce family repos if needed, syncs handoff state, and runs setup_env.sh.
set -euo pipefail

SUDO=""
if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1; then
  SUDO="sudo"
fi

# 1. System packages (Ubuntu/Debian apt-get or Oracle Linux/RHEL dnf)
if [ "$(id -u)" -eq 0 ] || [ -n "$SUDO" ]; then
  if command -v apt-get >/dev/null 2>&1; then
    DEBIAN_FRONTEND=noninteractive $SUDO apt-get update -qq
    DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq \
      build-essential pkg-config git curl tmux python3 \
      libasound2-dev libudev-dev
    if ! command -v gh >/dev/null 2>&1; then
      DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq gh || true
    fi
  elif command -v dnf >/dev/null 2>&1; then
    $SUDO dnf groupinstall -y -q "Development Tools" || true
    $SUDO dnf install -y -q pkgconfig git curl tmux python3 \
      alsa-lib-devel systemd-devel
    if ! command -v gh >/dev/null 2>&1; then
      $SUDO dnf config-manager --add-repo https://cli.github.com/packages/rpm/gh-cli.repo || true
      $SUDO dnf install -y -q gh || true
    fi
  fi
fi

# 2. Rust toolchain (rustup >= 1.85 for edition 2024)
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
fi

# 3. Clone Ounce family repositories if not already inside ounce-core
if [ -f "$(dirname "$0")/setup_env.sh" ]; then
  REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
  WORKSPACE_DIR="$(dirname "$REPO_ROOT")"
else
  WORKSPACE_DIR="${OUNCE_WORKSPACE:-$HOME/workspace}"
  mkdir -p "$WORKSPACE_DIR"
  REPO_ROOT="$WORKSPACE_DIR/ounce-core"
fi

OWNER="${OUNCE_GITHUB_OWNER:-hffgzktbcc-blip}"
for repo in ounce-core ounce-runtime ounce-hb deckstream; do
  if [ ! -d "$WORKSPACE_DIR/$repo" ]; then
    if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
      gh repo clone "$OWNER/$repo" "$WORKSPACE_DIR/$repo" || true
    else
      git clone "https://github.com/$OWNER/$repo.git" "$WORKSPACE_DIR/$repo" || true
    fi
  fi
done

cd "$REPO_ROOT"
./scripts/setup_env.sh
./scripts/handoff.sh digest || true
