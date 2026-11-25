#!/bin/bash
# Set BT (work dir holding the upstream checkouts; copy this directory to $BT/h) and ZIG (Zig 0.17.1 binary).
BT=${BT:-/tmp/bigtech-verify}; ZIG=${ZIG:-/tmp/zig/zig}
cd $BT/h; S=../sgp4/libsgp4; SRC=$(ls $S/*.cpp | grep -v CsvTleLoader)
clang++ -O2 -std=c++17 -I$S sgp4_drv.cpp $SRC -o sgp4_native 2>&1 | grep error | head -3
"$ZIG" c++ -target aarch64-linux-musl -static -no-pie -O2 -std=c++17 -I$S sgp4_drv.cpp $SRC -o sgp4_O2 2>&1 | grep error | head -3
"$ZIG" c++ -target aarch64-linux-musl -static -no-pie -O3 -mcpu=apple_m1 -std=c++17 -I$S sgp4_drv.cpp $SRC -o sgp4_O3 2>&1 | grep error | head -3
./sgp4_native < ../sgp4/SGP4-VER.TLE > sgp4.ref; tail -1 sgp4.ref; grep -c EXC sgp4.ref
