#!/usr/bin/env python3
"""Package a binary with recorded notices and verify the archive contents."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile
import zipfile


def package(binary, output, require_reviewed):
    root = Path(__file__).resolve().parents[1]
    review = json.loads((root / 'docs/RELEASE_NOTICE_REVIEW.json').read_text())
    for filename, field in [('Cargo.lock', 'cargo_lock_sha256'),
                            ('Cargo.toml', 'cargo_manifest_sha256'),
                            ('THIRD_PARTY_NOTICES.txt', 'notices_sha256')]:
        if hashlib.sha256((root / filename).read_bytes()).hexdigest() != review[field]:
            raise ValueError(f'Notice review identity is stale for {filename}')
    if require_reviewed:
        if (review['status'] != 'REVIEWED' or not review.get('reviewer')
                or not review.get('decision_evidence') or review.get('open_items')):
            raise ValueError('Release publication requires completed notice review')
    entries = {binary.name: binary.read_bytes()}
    for filename in ['LICENSE', 'THIRD_PARTY.md', 'THIRD_PARTY_NOTICES.txt',
                     'assets/fonts/DejaVu-LICENSE.txt', 'docs/RELEASE_NOTICE_REVIEW.json']:
        entries['licenses/' + Path(filename).name] = (root / filename).read_bytes()
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.name.endswith('.tar.gz'):
        with tarfile.open(output, 'w:gz') as archive:
            for name, data in entries.items():
                item = tarfile.TarInfo(name)
                item.size = len(data)
                item.mode = 0o755 if name == binary.name else 0o644
                archive.addfile(item, io.BytesIO(data))
        with tarfile.open(output) as archive:
            observed = {item.name: archive.extractfile(item).read()
                        for item in archive if item.isfile()}
    elif output.suffix == '.zip':
        with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for name, data in entries.items():
                archive.writestr(name, data)
        with zipfile.ZipFile(output) as archive:
            observed = {name: archive.read(name) for name in archive.namelist()}
    else:
        raise ValueError('Expected a .tar.gz or .zip output')
    if observed != entries:
        raise ValueError('Archive contents differ from the packaging inputs')
    print(f'Archive contents verified. Notice review status {review["status"]}.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--require-reviewed', action='store_true')
    args = parser.parse_args()
    try:
        package(args.binary, args.output, args.require_reviewed)
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f'Packaging failed: {error}\n')
