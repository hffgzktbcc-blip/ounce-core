#!/usr/bin/env bash
# Fails if tracked files include keys, retail containers or links to ROM/key/dump sites,
# a ported file has no PROVENANCE.md entry, or key-parsing / NCA-XCI decryption code reappears.
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
# Capability scope (docs/CAPABILITY_SCOPE.md): no console key handling and no encrypted-container
# decryption/loading in code, tools, scripts or the site. Docs may describe the policy.
code_files=$(git ls-files -- src crates tools scripts site assets tests .github ':!scripts/check_legal_guardrails.sh')
bad_names=$(printf '%s\n' $code_files | grep -iE '(^|/)(nca|xci|nsp|crypto|keys|keyset|prod_keys)\.(rs|py|swift|js)$' || true)
if [ -n "$bad_names" ]; then
  echo "Key-handling or container-decryption source files found:"; echo "$bad_names"; fail=1
fi
scope='prod\.keys|title\.keys|parse_prod_keys|\bKeySet\b|NcaImage|parse_and_decrypt|Pfs0Archive|Hfs0Archive|XciCartridge|NCA3|aes[-_ ]?xts|RestrictedCipher|SYNTHETIC_TEST_KEY|section_key|title_key|header_key|key_area_key|mod crypto;'
scope_hits=$(git grep -niE "$scope" -- src crates tools scripts site assets tests .github ':!scripts/check_legal_guardrails.sh' || true)
if [ -n "$scope_hits" ]; then
  echo "Key parsing or NCA/XCI/NSP decryption code reappeared (see docs/CAPABILITY_SCOPE.md):"; echo "$scope_hits"; fail=1
fi
# Ported files carry a "Ported from ..." notice and need an entry in docs/PROVENANCE.md.
for f in $(git grep -lE '^[[:space:]]*//[/!]?[[:space:]]*Ported from' -- '*.rs' || true); do
  grep -qF "\`$f\`" docs/PROVENANCE.md || { echo "Ported file without a docs/PROVENANCE.md entry: $f"; fail=1; }
done
[ "$fail" -eq 0 ] && echo "Legal guardrails check passed."
exit "$fail"
