#!/usr/bin/env bash
# FJCVTZS native-vs-engine gate (#469). Builds fjcvtzs_nzcv.c as a static freestanding aarch64 Linux ELF, runs it natively
# on an ARM64 host that has FEAT_JSCVT, and requires `ounce-core run-elf` on Tier-3 and Tier-4 to print the same 62 lines
# (31 inputs x FPCR.FZ 0/1: result, NZCV, FPSR). A negative control then edits one native line and requires the compare to
# notice. Exit codes: 0 identical, 1 mismatch or harness failure, 77 not run (host is not ARM64 or has no jscvt).
# Usage: docs/verify/fp-workload/fjcvtzs_gate.sh <new out dir> [path to ounce-core]   (run from the repo root; needs clang, ld.lld)
# FJCVTZS_NATIVE_REF=<file> uses that file as the native output instead of running the ELF (for checking this script off ARM64).
set -euo pipefail
out="${1:?usage: fjcvtzs_gate.sh <new out dir> [ounce-core binary]}"
bin="${2:-target/release/ounce-core}"
here="$(cd "$(dirname "$0")" && pwd)"
[ ! -e "$out" ] || { echo "$out already exists" >&2; exit 1; }
mkdir -p "$out"

LD=ld.lld
command -v "$LD" >/dev/null 2>&1 || LD="$(find "$(rustc --print sysroot)" -name rust-lld -type f | head -1)"
clang --target=aarch64-linux-gnu -O1 -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-outline-atomics \
  -march=armv8.5-a -c "$here/fjcvtzs_nzcv.c" -o "$out/fjcvtzs.o"
"$LD" -static -nostdlib -e _start "$out/fjcvtzs.o" -o "$out/fjcvtzs.elf"

if [ -n "${FJCVTZS_NATIVE_REF:-}" ]; then
  cp "$FJCVTZS_NATIVE_REF" "$out/native.txt"
elif [ "$(uname -m)" = aarch64 ] && grep -qw jscvt /proc/cpuinfo 2>/dev/null; then
  "$out/fjcvtzs.elf" > "$out/native.txt"
else
  echo "not run: this host is $(uname -m) and has no jscvt (needs an ARM64 Linux host with FEAT_JSCVT)"
  exit 77
fi
echo "native: $(wc -l < "$out/native.txt") lines"

rc=0
for tier in tier3 tier4; do
  "$bin" run-elf "$out/fjcvtzs.elf" --cpu=$tier > "$out/$tier.txt" 2> "$out/$tier.err"
  if cmp -s "$out/native.txt" "$out/$tier.txt"; then
    echo "$tier: identical to native"
  else
    echo "$tier: MISMATCH vs native"; diff "$out/native.txt" "$out/$tier.txt" | head -20 || true; rc=1
  fi
done

# Negative control: flip one hex digit in the first native line; the compare must flag it on both tiers.
sed '1s/ 00$/ 01/' "$out/native.txt" > "$out/native-tampered.txt"
if cmp -s "$out/native-tampered.txt" "$out/native.txt"; then echo "negative control did not change the file" >&2; exit 1; fi
for tier in tier3 tier4; do
  if cmp -s "$out/native-tampered.txt" "$out/$tier.txt"; then echo "negative control: $tier matched a tampered reference" >&2; exit 1; fi
done
echo "negative control: a tampered reference is flagged on both tiers"
exit $rc
