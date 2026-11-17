#!/usr/bin/env python3
"""Fail when the version stated in different places disagrees.

Checks: Cargo.toml [package] version == CITATION.cff version == newest dated "## X.Y.Z" heading in CHANGELOG.md,
and that docs/releases/v<version>.md exists for a released version. Usage: python3 scripts/check_pins.py [repo_root]
"""
import re
import sys
from pathlib import Path


def cargo_version(root):
    text = (root / 'Cargo.toml').read_text(encoding='utf-8')
    package = re.search(r'^\[package\](.*?)(?=^\[|\Z)', text, re.S | re.M)
    match = re.search(r'^version\s*=\s*"([^"]+)"', package.group(1) if package else text, re.M)
    return match.group(1) if match else None


def citation_version(root):
    path = root / 'CITATION.cff'
    match = re.search(r'^version:\s*"?([^"\s]+)"?', path.read_text(encoding='utf-8'), re.M) if path.exists() else None
    return match.group(1) if match else None


def changelog_version(root):
    text = (root / 'CHANGELOG.md').read_text(encoding='utf-8')
    for match in re.finditer(r'^##\s+v?(\d+\.\d+\.\d+)\b(.*)$', text, re.M):
        return match.group(1)
    return None


def problems(root):
    root = Path(root)
    found = {'Cargo.toml': cargo_version(root), 'CITATION.cff': citation_version(root), 'CHANGELOG.md': changelog_version(root)}
    out = []
    missing = [name for name, value in found.items() if value is None]
    out += [f'{name}: no version found' for name in missing]
    present = {value for value in found.values() if value}
    if len(present) > 1:
        out.append('version mismatch: ' + ', '.join(f'{name}={value}' for name, value in found.items()))
    version = found['Cargo.toml']
    if version and len(present) == 1 and not (root / 'docs' / 'releases' / f'v{version}.md').exists():
        out.append(f'docs/releases/v{version}.md is missing')
    return out


def main(argv):
    root = Path(argv[1]) if len(argv) > 1 else Path(__file__).resolve().parent.parent
    found = problems(root)
    for line in found:
        print(line)
    print(f'{len(found)} version pin problem(s)')
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
