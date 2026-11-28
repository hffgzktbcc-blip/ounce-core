#!/usr/bin/env bash
# Rerun the SGP4 row of docs/verify/bigtech-2026-11-23.md and check it.
#
# Fetches dnwrnr/sgp4 at the pinned commit (and checks the sha256 of its SGP4-VER.TLE input), builds
# the native oracle and the -O2 / -O3 guest ELFs with docs/verify/bigtech/run_sgp4.sh, checks the
# oracle output against the committed docs/verify/sgp4-bigtech/expected.txt, then runs both ELFs on
# the interpreter, Tier-3 and Tier-4 with SGP4-VER.TLE on stdin.
#
# Pass/fail: the oracle must equal expected.txt, every run must exit 0, and all six outputs must be
# byte-identical to each other. Lines identical to the oracle are counted and reported but do not
# decide the result: the oracle uses the host (macOS) libm and the guest uses musl's libm, and the
# 23 Nov page already records that gap (962/1,222 lines). Set SGP4_MIN_ORACLE_LINES to also fail
# when fewer lines than that match the oracle.
#
#   ZIG=/path/to/zig-0.17.1/zig OUNCE_CORE=target/release/ounce-core scripts/verify/sgp4.sh
#
# OUNCE_CORE must be built with `cargo build --release --features native-jit` (Tier-4).
# WORK (optional) is the scratch directory; by default a new temporary one, removed on success.
set -euo pipefail

PIN=b915de601a4af0a9c1c61c54efaafb168ba3defb
URL=https://github.com/dnwrnr/sgp4
TLE_SHA256=42566494ca3b11313570dbf2c493a2dd13a93f86aa8f03d57a54e46d9db72c01
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
EXPECTED="$ROOT/docs/verify/sgp4-bigtech/expected.txt"
OC=${OUNCE_CORE:-$ROOT/target/release/ounce-core}
ZIG=${ZIG:?set ZIG to a Zig 0.17.1 binary}
MIN=${SGP4_MIN_ORACLE_LINES:-0}
KEEP=${WORK:+1}
WORK=${WORK:-$(mktemp -d "${TMPDIR:-/tmp}/sgp4-verify.XXXXXX")}

sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
[ -x "$OC" ] || { echo "ounce-core not found at $OC (set OUNCE_CORE)"; exit 2; }
"$ZIG" version | grep -qx '0.17.1' || { echo "ZIG must be Zig 0.17.1 (got $("$ZIG" version))"; exit 2; }
echo "host: $(uname -sm); zig: $("$ZIG" version); work: $WORK"

mkdir -p "$WORK"
if [ ! -d "$WORK/sgp4/.git" ]; then
  git init -q "$WORK/sgp4"
  git -C "$WORK/sgp4" fetch -q --depth 1 "$URL" "$PIN"
  git -C "$WORK/sgp4" checkout -q FETCH_HEAD
fi
got=$(git -C "$WORK/sgp4" rev-parse HEAD)
[ "$got" = "$PIN" ] || { echo "sgp4 checkout is $got, expected $PIN"; exit 1; }
TLE="$WORK/sgp4/SGP4-VER.TLE"
[ "$(sha256 "$TLE")" = "$TLE_SHA256" ] || { echo "SGP4-VER.TLE sha256 differs from the pin"; exit 1; }
echo "sgp4 at $got; SGP4-VER.TLE sha256 $TLE_SHA256 ($(wc -l < "$TLE" | tr -d ' ') lines)"

rm -rf "$WORK/h"; cp -R "$ROOT/docs/verify/bigtech" "$WORK/h"
BT="$WORK" ZIG="$ZIG" bash "$WORK/h/run_sgp4.sh"
cd "$WORK/h"

fail=0
if cmp -s sgp4.ref "$EXPECTED"; then
  echo "native oracle: $(wc -l < sgp4.ref | tr -d ' ') lines, identical to expected.txt"
else
  echo "native oracle differs from expected.txt"; fail=1
fi
total=$(wc -l < "$EXPECTED" | tr -d ' ')
first=""
for elf in sgp4_O2 sgp4_O3; do
  for cpu in interpreter tier3 tier4; do
    out="out.$elf.$cpu"; s=$(date +%s); rc=0
    "$OC" run-elf "$elf" --cpu=$cpu --max-insns=400000000000 --stdin="$TLE" > "$out" 2> "err.$elf.$cpu" || rc=$?
    e=$(date +%s)
    same=$(comm -12 <(sort "$EXPECTED") <(sort "$out") | wc -l | tr -d ' ')
    [ -n "$first" ] || first="$out"
    if cmp -s "$out" "$first"; then agree=yes; else agree=NO; fail=1; fi
    [ "$rc" -eq 0 ] || fail=1
    [ "$same" -ge "$MIN" ] || fail=1
    echo "$elf $cpu rc=$rc oracle-identical=$same/$total same-as-$first=$agree time=$((e-s))s $(tail -n 1 "err.$elf.$cpu")"
  done
done

if [ "$fail" -eq 0 ]; then
  echo "RESULT: sgp4 $PIN: all 6 runs (2 builds x 3 engines) exit 0 and are byte-identical to each other; $same/$total lines identical to the macOS-libm oracle"
  [ -n "$KEEP" ] || rm -rf "$WORK"
else
  echo "RESULT: sgp4 $PIN: FAILED (work dir kept: $WORK)"
fi
exit "$fail"
