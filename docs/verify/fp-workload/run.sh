#!/usr/bin/env bash
# #359 FP edge workload: native ARM64 host vs `ounce-core run-elf` on Tier-3 and Tier-4, one op per process,
# compared byte for byte. Verdicts: identical | UNSUPPORTED (engine stops on an unimplemented instruction) | MISMATCH | OTHER-FAIL.
# Usage: docs/verify/fp-workload/run.sh <new out dir>   (run from the repo root; needs clang, rust-lld/ld.lld, an ARM64 host)
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
out="${1:?usage: run.sh <new out dir>}"
[ -e "$out" ] && { echo "out dir exists: $out" >&2; exit 2; }
mkdir -p "$out/ops"
case "$(uname -m)" in arm64|aarch64) ;; *) echo "needs an ARM64 host for the reference" >&2; exit 2;; esac
cargo build --release --locked --features native-jit
bin=target/release/ounce-core
clang -O1 -march=armv8.5-a -fno-builtin -o "$out/fp_edge_host" "$here/fp_edge.c"
if command -v ld.lld >/dev/null 2>&1; then LD=(ld.lld); else
  sysroot="$(rustc --print sysroot)"; host="$(rustc -vV | sed -n 's/^host: //p')"
  LD=("$sysroot/lib/rustlib/$host/bin/rust-lld" -flavor gnu)
  export DYLD_FALLBACK_LIBRARY_PATH="$sysroot/lib" LD_LIBRARY_PATH="$sysroot/lib"
fi
clang --target=aarch64-linux-gnu -O1 -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-outline-atomics \
  -march=armv8.5-a -DFREESTANDING -c "$here/fp_edge.c" -o "$out/fp_edge.o"
"${LD[@]}" -static -nostdlib -no-pie -z norelro --build-id=none -e _start -o "$out/fp_edge.elf" "$out/fp_edge.o"
printf 'op\tcases\ttier3\ttier4\n' > "$out/summary.tsv"
rc=0
for op in $("$out/fp_edge_host" --list); do
  "$out/fp_edge_host" "$op" > "$out/ops/$op.host"
  line="$op\t$(wc -l < "$out/ops/$op.host" | tr -d ' ')"
  for tier in tier3 tier4; do
    st=0
    "$bin" run-elf "$out/fp_edge.elf" --cpu=$tier --max-insns=2000000000 -- "$op" > "$out/ops/$op.$tier" 2> "$out/ops/$op.$tier.err" || st=$?
    if [ "$st" = 0 ] && cmp -s "$out/ops/$op.host" "$out/ops/$op.$tier"; then v=identical
    elif [ "$st" = 71 ] && grep -q "Unimplemented instruction" "$out/ops/$op.$tier.err"; then v="UNSUPPORTED($(grep -o 'instruction word 0x[0-9a-f]*' "$out/ops/$op.$tier.err" | head -1 | cut -d' ' -f3))"
    elif [ "$st" = 0 ]; then v=MISMATCH; rc=1
    else v="OTHER-FAIL(exit $st)"; rc=1; fi
    line="$line\t$v"
  done
  printf "$line\n" >> "$out/summary.tsv"
done
column -t -s "$(printf '\t')" "$out/summary.tsv"
shasum -a 256 "$here/fp_edge.c" "$out/fp_edge.elf" "$out/fp_edge_host" > "$out/SHA256SUMS"
{ clang --version | head -1; uname -sm; sysctl -n machdep.cpu.brand_string 2>/dev/null || true; git rev-parse HEAD; } > "$out/toolchain.txt"
exit $rc
