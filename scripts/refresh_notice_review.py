#!/usr/bin/env python3
"""Check or refresh the pinned file hashes in docs/RELEASE_NOTICE_REVIEW.json.

Release packaging refuses to run when Cargo.toml, Cargo.lock or THIRD_PARTY_NOTICES.txt
no longer match the hashes recorded by the notice review. That is deliberate: a changed
dependency set needs a licence look. Typical causes are a dependency bump or a manifest edit.

    python3 scripts/refresh_notice_review.py --check   # exit 1 and list stale files
    python3 scripts/refresh_notice_review.py           # rewrite only the three hashes

Refreshing asserts that the change added no dependency or licence needing new notices
(run `cargo deny check` and look at the Cargo.lock diff first). It never edits any other
field, so reviewer, status and notice sources stay as the owner recorded them.
"""
import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REVIEW = Path('docs/RELEASE_NOTICE_REVIEW.json')
PINNED = [('Cargo.lock', 'cargo_lock_sha256'),
          ('Cargo.toml', 'cargo_manifest_sha256'),
          ('THIRD_PARTY_NOTICES.txt', 'notices_sha256')]


def digest(root, name):
    return hashlib.sha256((root / name).read_bytes()).hexdigest()


def stale(root):
    review = json.loads((root / REVIEW).read_text())
    return [(name, field, review[field], digest(root, name))
            for name, field in PINNED if review[field] != digest(root, name)]


def refresh(root):
    path = root / REVIEW
    text = path.read_text()
    changed = stale(root)
    for _name, field, old, new in changed:
        text, count = re.subn(rf'("{field}"\s*:\s*"){old}(")', rf'\g<1>{new}\g<2>', text)
        if count != 1:
            raise ValueError(f'could not rewrite {field} in {REVIEW}')
    path.write_text(text)
    json.loads(text)
    return changed


def main(argv=None, root=ROOT):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', action='store_true', help='report stale hashes, change nothing')
    args = parser.parse_args(argv)
    changed = stale(root) if args.check else refresh(root)
    if args.check:
        for name, *_ in changed:
            print(f'stale: {name} no longer matches {REVIEW}')
        if changed:
            print('Fix: python3 scripts/refresh_notice_review.py (after checking the diff added no new licence)')
        return 1 if changed else 0
    for name, *_ in changed:
        print(f'refreshed hash for {name}')
    print('Nothing to refresh.' if not changed else f'Updated {REVIEW}; review the diff before committing.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
