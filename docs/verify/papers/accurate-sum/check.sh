#!/usr/bin/env bash
# Paper check: build accurate_sum.c as a static aarch64 ELF and run it on ounce-core (`run-elf`, Tier-3, about 25 s); require
# PASS and stdout equal to expected.txt byte for byte. If a host C compiler is present, also build the same source natively
# (-ffp-contract=off, so nothing is fused behind the algorithm's back) and require the native output to be identical, which
# compares ounce-core's floating point (including fused multiply-add) with the machine this runs on.
# Exit codes: 0 reproduced, 1 not reproduced or harness failure.
# Usage: docs/verify/papers/accurate-sum/check.sh <new out dir> [path to ounce-core]   (run from the repo root; needs clang, ld.lld)
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
  -ffp-contract=off -c "$here/accurate_sum.c" -o "$out/accurate_sum.o"
"${LD[@]}" -static -nostdlib -e _start "$out/accurate_sum.o" -o "$out/accurate_sum.elf"

status=0
"$bin" run-elf "$out/accurate_sum.elf" --cpu=tier3 --report-json="$out/report.json" > "$out/stdout.txt" 2> "$out/stderr.txt" || status=$?
if [ "$status" -ne 0 ]; then echo "NOT REPRODUCED: program exited $status" >&2; cat "$out/stdout.txt" >&2; exit 1; fi
if ! diff -u "$here/expected.txt" "$out/stdout.txt" > "$out/diff.txt"; then
  echo "NOT REPRODUCED: output differs from expected.txt" >&2; cat "$out/diff.txt" >&2; exit 1
fi

native="not compared (no host compiler)"
if clang -O1 -ffp-contract=off -DHOST "$here/accurate_sum.c" -o "$out/accurate_sum_host" -lm 2>/dev/null; then
  "$out/accurate_sum_host" > "$out/host_stdout.txt"
  cmp -s "$out/host_stdout.txt" "$out/stdout.txt" || { echo "NOT REPRODUCED: native output differs from ounce-core's" >&2; diff "$out/host_stdout.txt" "$out/stdout.txt" >&2; exit 1; }
  native="identical to a native $(uname -m) build"
fi
echo "REPRODUCED: Ogita-Rump-Oishi checks and Table 6.2 within 1.0 digit, Tier-3, output $native (report: $out/report.json)"
