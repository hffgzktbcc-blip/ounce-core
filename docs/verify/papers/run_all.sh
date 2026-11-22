#!/usr/bin/env bash
# Run every paper check (each folder's check.sh) and print one line per paper plus a total.
# Usage: docs/verify/papers/run_all.sh <new out dir> [ounce-core binary] [name-filter]   (from the repo root)
# Exit 0 only if every check reproduced.
set -uo pipefail
out="${1:?usage: run_all.sh <new out dir> [binary] [filter]}"; bin="${2:-target/release/ounce-core}"; filter="${3:-}"
here="$(cd "$(dirname "$0")" && pwd)"
[ ! -e "$out" ] || { echo "$out already exists" >&2; exit 1; }
mkdir -p "$out"; pass=0; fail=0; failed=()
for c in "$here"/*/check.sh; do
  name="$(basename "$(dirname "$c")")"
  [ "$name" = common ] && continue
  [ -z "$filter" ] || [[ "$name" == *$filter* ]] || continue
  if "$c" "$out/$name" "$bin" > "$out/$name.log" 2>&1; then pass=$((pass+1)); echo "ok    $name"; else fail=$((fail+1)); failed+=("$name"); echo "FAIL  $name"; fi
done
echo "$pass reproduced, $fail not reproduced"
[ "$fail" -eq 0 ] || { printf 'failed: %s\n' "${failed[*]}"; exit 1; }
