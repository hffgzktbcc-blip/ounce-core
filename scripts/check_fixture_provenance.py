#!/usr/bin/env python3
"""Check fixture identities and require evidence for reviewed replacements.

The legacy baseline preserves explicitly unresolved fixtures. Passing this check
does not establish their authorship, licence compliance or legal clearance.
"""
import argparse
import hashlib
import json
from pathlib import Path


def check(root):
    manifest = json.loads((root / 'tests/fixtures/provenance.json').read_text())
    baseline = json.loads((root / 'tests/fixtures/provenance-baseline.json').read_text())
    rows = manifest['fixtures']
    registered = {row['path'] for row in rows}
    actual = {str(path.relative_to(root)) for path in (root / 'tests/fixtures').rglob('*.nro')}
    if len(registered) != len(rows) or registered != actual:
        raise ValueError('Fixture register must cover every NRO exactly once')
    unresolved = 0
    for row in rows:
        path = row['path']
        binary = (root / path).read_bytes()
        if hashlib.sha256(binary).hexdigest() != row['sha256']:
            raise ValueError(f'Binary hash mismatch: {path}')
        blob = hashlib.sha1(b'blob ' + str(len(binary)).encode() + b'\0' + binary).hexdigest()
        if blob != row['git_blob']:
            raise ValueError(f'Git object mismatch: {path}')
        folder = (root / path).parent
        inputs = {str(item.relative_to(root)): hashlib.sha256(item.read_bytes()).hexdigest()
                  for item in sorted(folder.rglob('*'))
                  if item.is_file() and item.suffix not in ('.nro', '.elf', '.o')}
        if inputs != row['inputs_sha256']:
            raise ValueError(f'Source/build input mismatch: {path}')
        if row['review_status'] == 'UNVERIFIED':
            fingerprint = hashlib.sha256(json.dumps(row, sort_keys=True).encode()).hexdigest()
            if baseline.get(path) != fingerprint:
                raise ValueError(f'New or changed fixture requires review evidence: {path}')
            unresolved += 1
        elif row['review_status'] == 'VERIFIED':
            for field in ('source_origin', 'licence_evidence', 'toolchain_evidence',
                          'binary_component_review', 'reviewer', 'review_date'):
                evidence = row.get(field)
                if not isinstance(evidence, str) or not evidence.strip():
                    raise ValueError(f'Missing {field}: {path}')
        else:
            raise ValueError(f'Unsupported review status: {path}')
    print(f'Fixture identity gate passed. {unresolved} legacy fixtures remain UNVERIFIED.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, KeyError, OSError) as error:
        parser.exit(1, f'Fixture provenance gate failed: {error}\n')
