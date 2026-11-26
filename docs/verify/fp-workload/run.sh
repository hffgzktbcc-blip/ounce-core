#!/usr/bin/env bash
# #359 FP edge workload: native ARM64 host vs `ounce-core run-elf` on Tier-3 and Tier-4, one op per process,
# compared byte for byte. Verdicts: identical | UNSUPPORTED (engine stops on an unimplemented instruction) | MISMATCH | OTHER-FAIL.
# Usage: docs/verify/fp-workload/run.sh <new out dir>   (run from the repo root; needs clang, rust-lld/ld.lld, an ARM64 host)
# FP_WORKLOAD_OPS="op ..." runs only those ops. FP_WORKLOAD_TAMPER=<op> edits one result in that op's host reference
# after it is written: the negative control, which must come out MISMATCH on both tiers (CI checks this).
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
out="${1:?usage: run.sh <new out dir>}"
[ -e "$out" ] && { echo "out dir exists: $out" >&2; exit 2; }
mkdir -p "$out/ops"
case "$(uname -m)" in arm64|aarch64) ;; *) echo "needs an ARM64 host for the reference" >&2; exit 2;; esac
# nce is part of the build so this rebuild does not drop it from target/release (run-elf itself has no native engine).
cargo build --release --locked --features native-jit,nce
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
known="$("$out/fp_edge_host" --list)"
# An unknown name would make both programs skip every op and "compare" two empty files, so a typo must fail.
for op in ${FP_WORKLOAD_OPS:-}; do
  case " ${known//$'\n'/ } " in *" $op "*) ;; *) echo "run.sh: unknown op '$op' in FP_WORKLOAD_OPS; --list names: ${known//$'\n'/ }" >&2; exit 2;; esac
done
for op in ${FP_WORKLOAD_OPS:-$known}; do
  "$out/fp_edge_host" "$op" > "$out/ops/$op.host"
  [ -s "$out/ops/$op.host" ] || { echo "run.sh: the host reference produced no cases for '$op'" >&2; exit 2; }
  if [ "${FP_WORKLOAD_TAMPER:-}" = "$op" ]; then
    # Flip the last hex digit (the FPSR) of the first result.
    awk 'NR == 1 { d = substr($0, length($0)); $0 = substr($0, 1, length($0) - 1) (d == "0" ? "1" : "0") } { print }' \
      "$out/ops/$op.host" > "$out/ops/$op.host.tampered" && mv "$out/ops/$op.host.tampered" "$out/ops/$op.host"
  fi
  line="$op\t$(wc -l < "$out/ops/$op.host" | tr -d ' ')"
  for tier in tier3 tier4; do
    st=0
    "$bin" run-elf "$out/fp_edge.elf" --cpu=$tier --max-insns=2000000000 -- "$op" > "$out/ops/$op.$tier" 2> "$out/ops/$op.$tier.err" || st=$?
    # A tier4 column must be Tier-4: a build or host without it would run Tier-3 and label it tier4.
    if [ "$tier" = tier4 ] && grep -q '^\[Tier-4\] Not available' "$out/ops/$op.$tier.err"; then v="OTHER-FAIL(tier4 fell back to tier3: $(grep -m1 '^\[Tier-4\]' "$out/ops/$op.$tier.err"))"; rc=1
    elif [ "$st" = 0 ] && cmp -s "$out/ops/$op.host" "$out/ops/$op.$tier"; then v=identical
    elif [ "$st" = 71 ] && grep -q "Unimplemented instruction" "$out/ops/$op.$tier.err"; then v="UNSUPPORTED($(grep -o 'instruction word 0x[0-9a-f]*' "$out/ops/$op.$tier.err" | head -1 | cut -d' ' -f3))"
    elif [ "$st" = 0 ]; then v=MISMATCH; rc=1
    else v="OTHER-FAIL(exit $st)"; rc=1; fi
    line="$line\t$v"
  done
  printf '%b\n' "$line" >> "$out/summary.tsv"
done
column -t -s "$(printf '\t')" "$out/summary.tsv" 2>/dev/null || cat "$out/summary.tsv"
shasum -a 256 "$here/fp_edge.c" "$out/fp_edge.elf" "$out/fp_edge_host" > "$out/SHA256SUMS"
{ clang --version | head -1; uname -sm; sysctl -n machdep.cpu.brand_string 2>/dev/null || true; git rev-parse HEAD; } > "$out/toolchain.txt"
exit $rc
