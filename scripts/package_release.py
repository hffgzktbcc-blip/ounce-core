#!/usr/bin/env python3
"""Package a binary with recorded notices and verify the archive contents."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tarfile
import zipfile


def toolchain_entries(directory, binary):
    if directory.is_symlink():
        raise ValueError('Linked toolchain notice directory is not allowed')
    manifest = directory / 'manifest.json'
    if manifest.is_symlink():
        raise ValueError('Linked toolchain notice manifest is not allowed')
    data = json.loads(manifest.read_text())
    if data.get('schema_version') != 'ounce-core/toolchain-notices-v1':
        raise ValueError('Unsupported toolchain notice manifest')
    if data.get('binary_sha256') != hashlib.sha256(binary.read_bytes()).hexdigest():
        raise ValueError('Toolchain binary hash mismatch')
    if not data.get('rustc_version') or not data.get('target'):
        raise ValueError('Missing toolchain identity')
    files = data['files_sha256']
    required = {'LICENSE-MIT', 'LICENSE-APACHE'}
    copyrights = {'COPYRIGHT.html', 'COPYRIGHT-library.html'}
    extra = set(files) - required - copyrights
    def reuse_name(name):
        path = PurePosixPath(name)
        return ('\\' not in name and ':' not in name and len(path.parts) == 2
                and path.parts[0] == 'licenses' and path.suffix == '.txt'
                and path.name not in {'.', '..'})
    if (not required.issubset(files) or not copyrights.intersection(files)
            or not all(reuse_name(name) for name in extra)):
        raise ValueError('Incomplete or invalid toolchain notice coverage')
    entries = {'licenses/rust/manifest.json': manifest.read_bytes()}
    for name, digest in files.items():
        source = directory / name
        if source.is_symlink():
            raise ValueError('Linked toolchain notice is not allowed')
        raw = source.read_bytes()
        if not raw or hashlib.sha256(raw).hexdigest() != digest:
            raise ValueError('Toolchain notice hash mismatch: ' + name)
        entries['licenses/rust/' + name] = raw
    return entries


def native_entries(directory, binary):
    if directory.is_symlink() or (directory / 'manifest.json').is_symlink():
        raise ValueError('Linked native notice inventory is not allowed')
    data = json.loads((directory / 'manifest.json').read_text())
    if data.get('schema_version') != 'ounce-core/native-notices-v1' or not data.get('libraries'):
        raise ValueError('Missing native library inventory')
    if data.get('binary_sha256') != hashlib.sha256(binary.read_bytes()).hexdigest():
        raise ValueError('Native notice binary hash mismatch')
    entries = {'licenses/native/manifest.json': (directory / 'manifest.json').read_bytes()}
    if 'common/LGPL-2.1.txt' not in data.get('files_sha256', {}):
        raise ValueError('Missing native LGPL-2.1 text')
    for library in data['libraries']:
        if not library.get('soname') or not library.get('package') or not library.get('version'):
            raise ValueError('Incomplete native library identity')
        package_name = library['package'].split(':')[0]
        if 'packages/' + package_name + '/copyright.txt' not in data['files_sha256']:
            raise ValueError('Missing native package copyright')
    for name, digest in data['files_sha256'].items():
        relative = PurePosixPath(name)
        if ('\\' in name or ':' in name or relative.is_absolute()
                or any(part in {'.', '..'} for part in relative.parts)
                or not relative.parts or relative.parts[0] not in {'packages', 'common'} or relative.suffix != '.txt'):
            raise ValueError('Invalid native notice path')
        source = directory / name
        if source.is_symlink() or not source.resolve().is_relative_to(directory.resolve()):
            raise ValueError('Linked or escaping native notice path')
        raw = source.read_bytes()
        if not raw or hashlib.sha256(raw).hexdigest() != digest:
            raise ValueError('Native notice hash mismatch')
        entries['licenses/native/' + name] = raw
    return entries


def package(binary, output, require_reviewed, toolchain_notices=None, native_notices=None):
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
        if toolchain_notices is None:
            raise ValueError('Release publication requires actual toolchain notices')
        if binary.read_bytes().startswith(b'\x7fELF') and native_notices is None:
            raise ValueError('Linux release publication requires actual native library notices')
    entries = {binary.name: binary.read_bytes()}
    for filename in ['LICENSE', 'THIRD_PARTY.md', 'THIRD_PARTY_NOTICES.txt',
                     'assets/fonts/DejaVu-LICENSE.txt', 'docs/RELEASE_NOTICE_REVIEW.json']:
        entries['licenses/' + Path(filename).name] = (root / filename).read_bytes()
    for item in review.get('notice_supplements', []):
        relative = Path(item['path'])
        if (relative.is_absolute() or len(relative.parts) != 2
                or relative.parts[0] != 'licenses' or relative.suffix != '.txt'):
            raise ValueError('Invalid supplemental notice path')
        source = root / relative
        if source.is_symlink():
            raise ValueError('Linked supplemental notice is not allowed')
        raw = source.read_bytes()
        if not raw or hashlib.sha256(raw).hexdigest() != item['sha256']:
            raise ValueError('Supplemental notice hash mismatch: ' + str(relative))
        entries['licenses/' + relative.name] = raw
    if toolchain_notices is not None:
        entries.update(toolchain_entries(toolchain_notices, binary))
    if native_notices is not None:
        entries.update(native_entries(native_notices, binary))
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
    parser.add_argument('--toolchain-notices', type=Path,
                        help='notices collected from the Rust toolchain used for this binary')
    parser.add_argument('--native-notices', type=Path,
                        help='installed Linux shared-library copyright and license texts')
    args = parser.parse_args()
    try:
        package(args.binary, args.output, args.require_reviewed, args.toolchain_notices, args.native_notices)
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f'Packaging failed: {error}\n')
