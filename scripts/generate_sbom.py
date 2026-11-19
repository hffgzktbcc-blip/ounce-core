#!/usr/bin/env python3
"""SPDX 2.3 SBOM of the crates an ounce-core release binary is built from (issue #480).

Reads `cargo metadata --locked` (the resolved dependency graph, optionally for one `--target`) and Cargo.lock (the
registry checksums). It lists the root package and every crate reachable from it through normal or build
dependencies; dev-only crates are not part of a release binary and are left out. Output is deterministic for a given
lockfile and `--created` time. The document is checked before it is written, so a malformed SBOM fails the release job
instead of being published.

What it states: package names, versions, declared licenses, download locations, SHA-256 checksums and the dependency
edges. What it does not: it does not analyse source files (`filesAnalyzed` is false), does not conclude licenses
(`licenseConcluded` is NOASSERTION), and does not list system libraries or the Rust toolchain; those are covered by the
notice bundles in the release archive (scripts/collect_toolchain_notices.py, scripts/collect_native_notices.py).

Usage: python3 scripts/generate_sbom.py --output FILE [--target TRIPLE] [--created 2026-01-01T00:00:00Z]
"""
import argparse
import hashlib
import json
import re
import subprocess
import sys
import tomllib
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'https://github.com/hffgzktbcc-blip/ounce-core'
CRATES_REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'
TOKEN = re.compile(r'\s*(\(|\)|[A-Za-z0-9.+-]+)')


def lock_checksums(text):
    """{(name, version): sha256} for every registry package in a Cargo.lock."""
    return {(p['name'], p['version']): p['checksum'] for p in tomllib.loads(text).get('package', []) if 'checksum' in p}


def spdx_license(declared):
    """The SPDX expression for a Cargo `license` field, or NOASSERTION when it is not one.

    Cargo accepts the old `MIT/Apache-2.0` form, where `/` means OR. Anything that is not operands joined by AND / OR /
    WITH (with balanced parentheses) is not guessed at.
    """
    if not declared:
        return 'NOASSERTION'
    text = declared.replace('/', ' OR ').strip()
    position, depth, expect_operand = 0, 0, True
    while position < len(text):
        match = TOKEN.match(text, position)
        if not match:
            return 'NOASSERTION'
        token, position = match.group(1), match.end()
        if token == '(':
            if not expect_operand:
                return 'NOASSERTION'
            depth += 1
        elif token == ')':
            if expect_operand or depth == 0:
                return 'NOASSERTION'
            depth -= 1
        elif token in ('AND', 'OR', 'WITH'):
            if expect_operand:
                return 'NOASSERTION'
            expect_operand = True
        else:
            if not expect_operand:
                return 'NOASSERTION'
            expect_operand = False
    if depth or expect_operand or text[position:].strip():
        return 'NOASSERTION'
    return re.sub(r'\s+', ' ', text)


def spdx_id(name, version):
    return 'SPDXRef-Package-' + re.sub(r'[^A-Za-z0-9.-]', '-', f'{name}-{version}')


def reachable(metadata):
    """Package ids reachable from the root through normal or build dependencies, and the edges between them."""
    nodes = {n['id']: n for n in metadata['resolve']['nodes']}
    root = metadata['resolve']['root']
    seen, edges, stack = {root}, [], [root]
    while stack:
        current = stack.pop()
        for dep in nodes[current]['deps']:
            if not any(k.get('kind') in (None, 'build') for k in dep['dep_kinds']):
                continue
            edges.append((current, dep['pkg']))
            if dep['pkg'] not in seen:
                seen.add(dep['pkg'])
                stack.append(dep['pkg'])
    return root, seen, edges


def build_sbom(metadata, checksums, created, target=None):
    root_id, included, edges = reachable(metadata)
    by_id = {p['id']: p for p in metadata['packages']}
    packages = []
    for pid in sorted(included, key=lambda i: (by_id[i]['name'], by_id[i]['version'])):
        p = by_id[pid]
        entry = {
            'name': p['name'],
            'SPDXID': spdx_id(p['name'], p['version']),
            'versionInfo': p['version'],
            'supplier': 'NOASSERTION',
            'downloadLocation': (f"https://crates.io/api/v1/crates/{p['name']}/{p['version']}/download"
                                 if p.get('source') == CRATES_REGISTRY else 'NOASSERTION'),
            'filesAnalyzed': False,
            'licenseConcluded': 'NOASSERTION',
            'licenseDeclared': spdx_license(p.get('license')),
            'copyrightText': 'NOASSERTION',
        }
        checksum = checksums.get((p['name'], p['version']))
        if checksum:
            entry['checksums'] = [{'algorithm': 'SHA256', 'checksumValue': checksum}]
        entry['externalRefs'] = [{'referenceCategory': 'PACKAGE-MANAGER', 'referenceType': 'purl',
                                  'referenceLocator': f"pkg:cargo/{p['name']}@{p['version']}"}]
        packages.append(entry)
    ids = {pid: spdx_id(by_id[pid]['name'], by_id[pid]['version']) for pid in included}
    relationships = [{'spdxElementId': 'SPDXRef-DOCUMENT', 'relationshipType': 'DESCRIBES',
                      'relatedSpdxElement': ids[root_id]}]
    relationships += [{'spdxElementId': ids[a], 'relationshipType': 'DEPENDS_ON', 'relatedSpdxElement': ids[b]}
                      for a, b in sorted(set(edges), key=lambda e: (ids[e[0]], ids[e[1]]))]
    root = by_id[root_id]
    label = f"{root['name']}-{root['version']}" + (f'-{target}' if target else '')
    fingerprint = hashlib.sha256('\n'.join(f"{p['name']} {p['versionInfo']}" for p in packages).encode()).hexdigest()
    return {
        'spdxVersion': 'SPDX-2.3',
        'dataLicense': 'CC0-1.0',
        'SPDXID': 'SPDXRef-DOCUMENT',
        'name': label,
        'documentNamespace': f'{REPOSITORY}/spdx/{label}-{fingerprint[:16]}',
        'creationInfo': {'created': created, 'creators': ['Tool: ounce-core scripts/generate_sbom.py']},
        'packages': packages,
        'relationships': relationships,
    }


def problems(doc):
    out, ids = [], [p['SPDXID'] for p in doc['packages']]
    for duplicate in sorted({i for i in ids if ids.count(i) > 1}):
        out.append(f'duplicate SPDXID {duplicate}')
    known = set(ids) | {doc['SPDXID']}
    for rel in doc['relationships']:
        for end in (rel['spdxElementId'], rel['relatedSpdxElement']):
            if end not in known:
                out.append(f'relationship {rel["relationshipType"]} names {end}, which is not in the document')
    if not any(r['relationshipType'] == 'DESCRIBES' for r in doc['relationships']):
        out.append('no DESCRIBES relationship: the document does not say what it describes')
    return out


def commit_time():
    """The HEAD commit time in UTC, so a rebuild of the same commit gives the same document."""
    try:
        text = subprocess.run(['git', 'show', '-s', '--format=%cI', 'HEAD'], cwd=ROOT, capture_output=True, text=True,
                              check=True).stdout.strip()
        return datetime.fromisoformat(text).astimezone(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    except (subprocess.CalledProcessError, FileNotFoundError, ValueError):
        return datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n', 1)[0])
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target', help='target triple: only dependencies compiled for it are listed')
    parser.add_argument('--created', help='creation time (default: the HEAD commit time)')
    args = parser.parse_args(argv)
    command = ['cargo', 'metadata', '--locked', '--format-version', '1']
    if args.target:
        command += ['--filter-platform', args.target]
    metadata = json.loads(subprocess.run(command, cwd=ROOT, capture_output=True, encoding='utf-8', check=True).stdout)
    doc = build_sbom(metadata, lock_checksums((ROOT / 'Cargo.lock').read_text(encoding='utf-8')),
                     args.created or commit_time(), args.target)
    found = problems(doc)
    for line in found:
        print(f'FAIL {line}', file=sys.stderr)
    if found:
        return 1
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(doc, indent=2) + '\n', encoding='utf-8')
    print(f'{args.output}: {len(doc["packages"])} packages, {len(doc["relationships"])} relationships')
    return 0


if __name__ == '__main__':
    sys.exit(main())
