#!/usr/bin/env bash
# Rerun the BLAKE3 row of docs/verify/bigtech-2026-11-23.md and check it.
#
# Fetches BLAKE3 at the pinned commit, builds the native oracle and the -O2 / -O3 guest ELFs with
# docs/verify/bigtech/run_b3.sh, checks the oracle output against the committed
# docs/verify/blake3-bigtech/expected.txt, then runs both ELFs on the interpreter, Tier-3 and Tier-4
# and requires each output to equal expected.txt byte for byte. Exits 0 only if everything matches.
#
#   ZIG=/path/to/zig-0.17.1/zig OUNCE_CORE=target/release/ounce-core scripts/verify/blake3.sh
#
# OUNCE_CORE must be built with `cargo build --release --features native-jit` (Tier-4).
# WORK (optional) is the scratch directory; by default a new temporary one, removed on success.
# The native-oracle build line in run_b3.sh uses BLAKE3's NEON code, so it needs an AArch64 host.
# See docs/verify/blake3-bigtech/PINS.txt for the pins and an x86-64 oracle line.
set -euo pipefail

PIN=657284b1fa4c4378aa7422b4450dcb5c2c9052fc
URL=https://github.com/BLAKE3-team/BLAKE3
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
EXPECTED="$ROOT/docs/verify/blake3-bigtech/expected.txt"
OC=${OUNCE_CORE:-$ROOT/target/release/ounce-core}
ZIG=${ZIG:?set ZIG to a Zig 0.17.1 binary}
KEEP=${WORK:+1}
WORK=${WORK:-$(mktemp -d "${TMPDIR:-/tmp}/blake3-verify.XXXXXX")}

[ -x "$OC" ] || { echo "ounce-core not found at $OC (set OUNCE_CORE)"; exit 2; }
"$ZIG" version | grep -qx '0.17.1' || { echo "ZIG must be Zig 0.17.1 (got $("$ZIG" version))"; exit 2; }
echo "host: $(uname -sm); ounce-core: $("$OC" --version 2>/dev/null | grep -o 'v[0-9.]*' | head -1); zig: $("$ZIG" version); work: $WORK"

mkdir -p "$WORK"
if [ ! -d "$WORK/BLAKE3/.git" ]; then
  git init -q "$WORK/BLAKE3"
  git -C "$WORK/BLAKE3" fetch -q --depth 1 "$URL" "$PIN"
  git -C "$WORK/BLAKE3" checkout -q FETCH_HEAD
fi
got=$(git -C "$WORK/BLAKE3" rev-parse HEAD)
[ "$got" = "$PIN" ] || { echo "BLAKE3 checkout is $got, expected $PIN"; exit 1; }
echo "BLAKE3 at $got"

rm -rf "$WORK/h"; cp -R "$ROOT/docs/verify/bigtech" "$WORK/h"
BT="$WORK" ZIG="$ZIG" bash "$WORK/h/run_b3.sh"
cd "$WORK/h"

fail=0
if cmp -s b3.ref "$EXPECTED"; then
  echo "native oracle: $(wc -l < b3.ref | tr -d ' ') lines, identical to expected.txt"
else
  echo "native oracle differs from expected.txt"; fail=1
fi
total=$(wc -l < "$EXPECTED" | tr -d ' ')
for elf in b3_O2 b3_O3; do
  for cpu in interpreter tier3 tier4; do
    s=$(date +%s); rc=0
    "$OC" run-elf "$elf" --cpu=$cpu --max-insns=400000000000 > "out.$elf.$cpu" 2> "err.$elf.$cpu" || rc=$?
    e=$(date +%s)
    same=$(comm -12 <(sort "$EXPECTED") <(sort "out.$elf.$cpu") | wc -l | tr -d ' ')
    if [ "$rc" -eq 0 ] && cmp -s "out.$elf.$cpu" "$EXPECTED"; then verdict=match; else verdict=MISMATCH; fail=1; fi
    echo "$elf $cpu rc=$rc lines=$same/$total time=$((e-s))s $verdict $(tail -n 1 "err.$elf.$cpu")"
    [ "$verdict" = match ] || head -c 300 "err.$elf.$cpu"
  done
done

if [ "$fail" -eq 0 ]; then
  echo "RESULT: BLAKE3 $PIN: all 6 runs (2 builds x 3 engines) match the native oracle"
  [ -n "$KEEP" ] || rm -rf "$WORK"
else
  echo "RESULT: BLAKE3 $PIN: FAILED (work dir kept: $WORK)"
fi
exit "$fail"
