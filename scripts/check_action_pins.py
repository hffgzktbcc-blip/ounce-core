#!/usr/bin/env python3
"""Fail when a GitHub Actions `uses:` is not pinned to a full commit SHA (issue #480).

A tag such as `actions/checkout@v4` can be moved to different code after it was reviewed, and a workflow that builds
release archives runs that code with write access. Every `uses:` in `.github/workflows` and `.github/actions` must be
`owner/repo[/path]@<40 hex digits>`, with the version in a trailing comment (`# v4`) so Dependabot's `github-actions`
updates keep proposing newer pins. Local actions and reusable workflows (`./...`) are exempt.

PENDING lists the few refs that could not be pinned yet, each with the reason. An entry is allowed only while that ref
is still used unpinned: once it is pinned or removed the check fails until the entry is deleted, so the list only
shrinks. Usage: python3 scripts/check_action_pins.py [repo_root]
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
USES = re.compile(r'^\s*(?:-\s*)?uses:\s*([^\s#]+)')
PINNED = re.compile(r'^[^@\s]+@[0-9a-f]{40}$')

# Refs still unpinned. The commit a tag resolves to has to be read from the action's own repository or from a CI log
# that ran it; these are used only by workflows (release, pages, coverage) that have not run since #480, so neither
# source was available when the other pins were made.
PENDING = {
    'actions/attest@v4': 'used only by release.yml, which has not run since the pins were made',
    'softprops/action-gh-release@v3': 'used only by release.yml, which has not run since the pins were made',
    'actions/configure-pages@v5': 'used only by pages.yml, which has not run since the pins were made',
    'actions/upload-pages-artifact@v5': 'used only by pages.yml, which has not run since the pins were made',
    'actions/deploy-pages@v5': 'used only by pages.yml, which has not run since the pins were made',
    'taiki-e/install-action@cargo-llvm-cov': 'used only by rust-coverage.yml, which has not run since the pins were made',
}


def references(root):
    """(relative path, line number, ref) for every non-local `uses:` under .github."""
    root = Path(root)
    files = sorted([*(root / '.github' / 'workflows').glob('*.y*ml'), *(root / '.github' / 'actions').glob('**/*.y*ml')])
    for path in files:
        for number, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
            match = USES.match(line)
            if match and not match.group(1).startswith('./'):
                yield path.relative_to(root).as_posix(), number, match.group(1).strip('\'"')


def problems(root, pending=PENDING):
    found, out = set(), []
    for path, number, ref in references(root):
        found.add(ref)
        if not PINNED.match(ref) and ref not in pending:
            out.append(f'{path}:{number}: `uses: {ref}` is not pinned to a 40-character commit SHA. Pin it as '
                       '`owner/repo@<sha> # <version>`; the SHA is the commit the tag points to.')
    pinned_bases = {ref.split('@')[0] for ref in found if PINNED.match(ref)}
    for ref, why in sorted(pending.items()):
        if ref in found:
            continue
        if ref.split('@')[0] in pinned_bases:
            out.append(f'PENDING lists {ref}, which is no longer unpinned: delete the entry.')
        else:
            out.append(f'PENDING lists {ref} but it is not used any more: delete the entry ({why}).')
    return out


def main(argv=None):
    root = Path((argv or sys.argv[1:] or [ROOT])[0])
    found = problems(root)
    for line in found:
        print(f'FAIL {line}')
    if not found:
        total = sum(1 for _ in references(root))
        print(f'All {total} action references are pinned to a commit SHA except {len(PENDING)} listed as pending.')
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main())
