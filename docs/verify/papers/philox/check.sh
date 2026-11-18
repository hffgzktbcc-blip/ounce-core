#!/usr/bin/env bash
# Paper check: build philox4x32.c as a static aarch64 ELF, run it on ounce-core (`run-elf`, Tier-3), and require the program to
# report PASS (its output equals the published vectors in paper.json) and its stdout to equal expected.txt byte for byte.
# Exit codes: 0 reproduced, 1 not reproduced or harness failure.
# Usage: docs/verify/papers/philox/check.sh <new out dir> [path to ounce-core]   (run from the repo root; needs clang, ld.lld)
set -euo pipefail
out="${1:?usage: check.sh <new out dir> [ounce-core binary]}"
bin="${2:-target/release/ounce-core}"
here="$(cd "$(dirname "$0")" && pwd)"
[ ! -e "$out" ] || { echo "$out already exists" >&2; exit 1; }
mkdir -p "$out"

if command -v ld.lld >/dev/null 2>&1; then LD=(ld.lld); else
  sysroot="$(rustc --print sysroot)"; host="$(rustc -vV | sed -n 's/^host: //p')"
  LD=("$sysroot/lib/rustlib/$host/bin/rust-lld" -flavor gnu)
  export DYLD_FALLBACK_LIBRARY_PATH="$sysroot/lib" LD_LIBRARY_PATH="$sysroot/lib"
fi
clang --target=aarch64-linux-gnu -O1 -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-outline-atomics \
  -c "$here/philox4x32.c" -o "$out/philox.o"
"${LD[@]}" -static -nostdlib -e _start "$out/philox.o" -o "$out/philox.elf"

status=0
"$bin" run-elf "$out/philox.elf" --cpu=tier3 --report-json="$out/report.json" > "$out/stdout.txt" 2> "$out/stderr.txt" || status=$?
if [ "$status" -ne 0 ]; then echo "NOT REPRODUCED: program exited $status" >&2; cat "$out/stdout.txt" >&2; exit 1; fi
if ! diff -u "$here/expected.txt" "$out/stdout.txt" > "$out/diff.txt"; then
  echo "NOT REPRODUCED: output differs from expected.txt" >&2; cat "$out/diff.txt" >&2; exit 1
fi
echo "REPRODUCED: philox4x32-10, 3 of 3 published vectors, Tier-3 (report: $out/report.json)"
