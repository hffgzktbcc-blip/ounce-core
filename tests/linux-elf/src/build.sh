#!/usr/bin/env bash
# Builds the freestanding AArch64 Linux test ELFs into ../bin. Needs clang (any host) and an LLD
# (ld.lld on PATH, or the rust-lld that ships with rustup). Source is the only input; ELFs are
# reproducible-enough to rebuild, not bit-for-bit pinned.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
bin="$here/../bin"
mkdir -p "$bin"
if command -v ld.lld >/dev/null 2>&1; then
  LD=(ld.lld)
else
  sysroot="$(rustc --print sysroot)"
  host="$(rustc -vV | sed -n 's/^host: //p')"
  LD=("$sysroot/lib/rustlib/$host/bin/rust-lld" -flavor gnu)
  export DYLD_FALLBACK_LIBRARY_PATH="$sysroot/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
  export LD_LIBRARY_PATH="$sysroot/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
for src in "$here"/*.c; do
  name="$(basename "$src" .c)"
  clang --target=aarch64-linux-gnu -O1 -ffreestanding -fno-builtin -fno-stack-protector \
    -fno-pic -mgeneral-regs-only -mno-outline-atomics -c "$src" -o "$tmp/$name.o"
  "${LD[@]}" -static -nostdlib -no-pie -z norelro --build-id=none -e _start -o "$bin/$name.elf" "$tmp/$name.o"
  echo "built $bin/$name.elf ($(wc -c <"$bin/$name.elf") bytes)"
done
