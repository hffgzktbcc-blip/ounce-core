#!/bin/bash
# Set BT (work dir holding the upstream checkouts; copy this directory to $BT/h) and ZIG (Zig 0.17.1 binary).
BT=${BT:-/tmp/bigtech-verify}; ZIG=${ZIG:-/tmp/zig/zig}
S=$BT/s2n-bignum
cd $BT/h; rm -rf s2n_o && mkdir -p s2n_o
SFILES=$(find $S/arm -name '*.S' -not -path '*/proofs/*' -not -path '*/tutorial/*' -not -path '*/unopt/*' | sort)
for cfg in "O2:-O2 -mcpu=generic+sha3+aes+crypto" "O3:-O3 -mcpu=apple_m1"; do
  n=${cfg%%:*}; fl=${cfg#*:}
  mkdir -p s2n_o/$n
  for f in $SFILES; do "$ZIG" cc -target aarch64-linux-musl $fl -I$S/include -c $f -o s2n_o/$n/$(echo $f | sed "s#$S/arm/##; s#/#_#g").o 2>s2n_o/$n.err || { echo "ASM FAIL $f"; head -3 s2n_o/$n.err; }; done
  "$ZIG" cc -target aarch64-linux-musl -static -no-pie $fl -std=gnu99 -w -I$S/include -I$S/tests $S/tests/test.c s2n_o/$n/*.o -lm -o s2n_$n 2>&1 | tail -5
done
ls -la s2n_O2 s2n_O3
