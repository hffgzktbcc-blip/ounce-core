#!/bin/sh
# Usage: run_engines.sh X.nro WORKDIR   (from the repo root, after
#   cargo build --release --features "nce native-jit")
# Runs the NRO on tier3, tier4 and native, then compares results.txt with expected-host.txt byte for byte.
NRO=$1
WORK=$2
HERE=$(cd "$(dirname "$0")" && pwd)
for t in tier3 tier4 native; do
  mkdir -p "$WORK/sd_$t"
  rm -f "$WORK/sd_$t/results.txt"
  OUNCE_NRO_BUDGET=4000000000 target/release/ounce-core run-nro "$NRO" --present=headless --sdmc="$WORK/sd_$t" --cpu=$t > "$WORK/log_$t.txt" 2>&1
  if cmp -s "$WORK/sd_$t/results.txt" "$HERE/expected-host.txt"; then echo "$t: IDENTICAL ($(wc -l < "$WORK/sd_$t/results.txt") lines)"; else echo "$t: DIFFERENT"; grep -iE "error|unimpl" "$WORK/log_$t.txt" | head -3; fi
done
