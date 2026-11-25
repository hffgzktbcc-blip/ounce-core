#!/bin/bash
# Set OCDIR (an ounce-core checkout built with --release --features native-jit).
# usage: cmp.sh <elf> <ref> [extra run-elf args after --]
OC=${OCDIR:-/tmp/bigtech-verify/oc}/target/release/ounce-core
elf=$1; ref=$2; shift 2
for cpu in interpreter tier3 tier4; do
  s=$(date +%s)
  $OC run-elf $elf --cpu=$cpu --max-insns=400000000000 "$@" > out.$cpu 2> err.$cpu; rc=$?
  e=$(date +%s)
  tot=$(wc -l < $ref); pass=$(comm -12 <(sort $ref) <(sort out.$cpu) | wc -l)
  echo "$elf $cpu rc=$rc pass=$pass/$tot time=$((e-s))s stderr: $(head -c 200 err.$cpu | tr '\n' '|')"
done
