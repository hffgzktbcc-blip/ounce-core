#!/usr/bin/env bash
# Fails if tracked files include keys, retail containers or links to ROM/key/dump sites,
# or a ported file has no PROVENANCE.md entry.
# See docs/CLEAN_ROOM.md "Legal guardrails".
set -u
fail=0
bad_files=$(git ls-files | grep -iE '\.(keys|nsp|xci|nca|nsz|xcz)$|(^|/)dumps/|(^|/)(prod|title|dev)\.keys' || true)
if [ -n "$bad_files" ]; then
  echo "Key or retail container files are tracked:"; echo "$bad_files"; fail=1
fi
# Known ROM/key/dump site domains. Extend as needed; never link them anywhere else.
sites='nswdb\.com|nopaystation|hshop\.erista|nsw2u|switchroms|romsfun|romspure|cdromance|vimm\.net|nxbrew|ziperto|emuparadise|romsgames|nsp-?ddl|prodkeys\.net|keys\.lol'
hits=$(git grep -niE "$sites" -- . ':!scripts/check_legal_guardrails.sh' || true)
if [ -n "$hits" ]; then
  echo "Links to ROM/key/dump sites found:"; echo "$hits"; fail=1
fi
# Ported files carry a "Ported from ..." notice and need an entry in docs/PROVENANCE.md.
for f in $(git grep -lE '^[[:space:]]*//[/!]?[[:space:]]*Ported from' -- '*.rs' || true); do
  grep -qF "\`$f\`" docs/PROVENANCE.md || { echo "Ported file without a docs/PROVENANCE.md entry: $f"; fail=1; }
done
[ "$fail" -eq 0 ] && echo "Legal guardrails check passed."
exit "$fail"
