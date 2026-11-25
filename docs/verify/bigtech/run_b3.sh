#!/bin/bash
# Set BT (work dir holding the upstream checkouts; copy this directory to $BT/h) and ZIG (Zig 0.17.1 binary).
BT=${BT:-/tmp/bigtech-verify}; ZIG=${ZIG:-/tmp/zig/zig}
cd "$BT/h" || exit 1
B=../BLAKE3/c; SRC=(blake3_drv.c $B/blake3.c $B/blake3_dispatch.c $B/blake3_portable.c $B/blake3_neon.c)
clang -O2 -I$B -DBLAKE3_USE_NEON=1 "${SRC[@]}" -o b3_native || exit 1
"$ZIG" cc -target aarch64-linux-musl -static -no-pie -O2 -I$B -DBLAKE3_USE_NEON=1 "${SRC[@]}" -o b3_O2 || exit 1
"$ZIG" cc -target aarch64-linux-musl -static -no-pie -O3 -mcpu=apple_m1 -I$B -DBLAKE3_USE_NEON=1 "${SRC[@]}" -o b3_O3 || exit 1
./b3_native > b3.ref; wc -l b3.ref
