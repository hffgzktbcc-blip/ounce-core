#!/usr/bin/env bash
# Prepare a Linux checkout for ounce-core builds. Idempotent; safe to re-run.
set -euo pipefail
cd "$(dirname "$0")/.."

export PKG_CONFIG_PATH="$HOME/.local/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LIBRARY_PATH="$HOME/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

if ! pkg-config --exists alsa libudev 2>/dev/null; then
  SUDO=""
  if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    SUDO="sudo"
  fi

  if [ "$(id -u)" -eq 0 ] || [ -n "$SUDO" ]; then
    if command -v apt-get >/dev/null 2>&1; then
      DEBIAN_FRONTEND=noninteractive $SUDO apt-get update -qq
      DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq libasound2-dev libudev-dev pkg-config
    elif command -v dnf >/dev/null 2>&1; then
      $SUDO dnf install -y -q alsa-lib-devel systemd-devel pkgconfig
    fi
  fi
fi

# Fallback for rootless cloud containers where runtime .so files exist without -dev packages
MULTIARCH_LIBDIR="/usr/lib/$(uname -m)-linux-gnu"
if ! pkg-config --exists alsa libudev 2>/dev/null && [ -e "$MULTIARCH_LIBDIR/libasound.so.2" ] && [ -e "$MULTIARCH_LIBDIR/libudev.so.1" ]; then
  mkdir -p "$HOME/.local/lib/pkgconfig"
  ln -sfn "$MULTIARCH_LIBDIR/libasound.so.2" "$HOME/.local/lib/libasound.so"
  ln -sfn "$MULTIARCH_LIBDIR/libudev.so.1" "$HOME/.local/lib/libudev.so"
  cat > "$HOME/.local/lib/pkgconfig/alsa.pc" <<EOF
prefix=/usr
exec_prefix=\${prefix}
libdir=$HOME/.local/lib
includedir=\${prefix}/include
Name: alsa
Description: Advanced Linux Sound Architecture (ALSA) - Library
Version: 1.2.8
Libs: -L\${libdir} -lasound
Cflags: -I\${includedir}
EOF
  cat > "$HOME/.local/lib/pkgconfig/libudev.pc" <<EOF
prefix=/usr
exec_prefix=\${prefix}
libdir=$HOME/.local/lib
includedir=\${prefix}/include
Name: libudev
Description: Library to access udev device information
Version: 252
Libs: -L\${libdir} -ludev
Cflags: -I\${includedir}
EOF
  sed -i '/case \$- in/,/esac/d' "$HOME/.bashrc" 2>/dev/null || true
  grep -qxF 'export PKG_CONFIG_PATH="$HOME/.local/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"' "$HOME/.bashrc" 2>/dev/null || \
    echo 'export PKG_CONFIG_PATH="$HOME/.local/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"' >> "$HOME/.bashrc"
  grep -qxF 'export LIBRARY_PATH="$HOME/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"' "$HOME/.bashrc" 2>/dev/null || \
    echo 'export LIBRARY_PATH="$HOME/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"' >> "$HOME/.bashrc"
fi

command -v cargo >/dev/null || { echo "Install Rust (rustup) first"; exit 1; }
python3 --version
cargo build --locked
scripts/next.sh
