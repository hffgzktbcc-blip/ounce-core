#!/usr/bin/env bash
# Fails if `unsafe` appears outside the modules docs/DETAILS.md "Safe Rust by construction" names.
# Keep this list and that section in step.
set -u
allowed='^(crates/ounce-recompiler/src/exec\.rs|crates/ounce-nce/src/(arena|exec)\.rs):'
fail=0
hits=$(git grep -nwI unsafe -- '*.rs' | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' | grep -vE "$allowed" || true)
if [ -n "$hits" ]; then echo "\`unsafe\` outside the allowed modules:"; echo "$hits"; fail=1; fi
allows=$(git grep -n 'allow(unsafe_code)' -- '*.rs' | grep -vE '^crates/(ounce-recompiler|ounce-nce)/src/lib\.rs:' || true)
if [ -n "$allows" ]; then echo "allow(unsafe_code) outside the two boundary crates:"; echo "$allows"; fail=1; fi
grep -q '^#!\[forbid(unsafe_code)\]' src/main.rs || { echo "src/main.rs lost #![forbid(unsafe_code)]"; fail=1; }
[ "$fail" -eq 0 ] && echo "Unsafe boundary check passed."
exit "$fail"
