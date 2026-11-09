#!/usr/bin/env python3
"""Retain installed Debian/Ubuntu shared-library notices for a trusted build.

Libraries themselves are not copied. ldd must only be used on our own build.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


def collect(binary, output):
    if output.exists() or output.is_symlink():
        raise ValueError('Choose a fresh native notice directory')
    if not binary.read_bytes().startswith(b'\x7fELF'):
        raise ValueError('Native notice collector requires our Linux ELF build')
    linked = subprocess.check_output(['ldd', str(binary.resolve())], text=True)
    if 'not found' in linked:
        raise ValueError('Unresolved native dependency')
    libraries, entries = [], {}
    for line in linked.splitlines():
        match = re.search(r'(?:=>\s+)?(/\S+)\s+\(', line)
        if not match:
            continue  # vDSO has no filesystem object or package.
        library = Path(match[1])
        candidates = [str(library.resolve()), str(library)]
        candidates += [p[4:] if p.startswith('/usr/') else '/usr' + p for p in list(candidates)]
        owner = None
        for candidate in dict.fromkeys(candidates):
            result = subprocess.run(['dpkg-query', '-S', candidate], capture_output=True, text=True)
            if result.returncode == 0:
                owner = result.stdout.splitlines()[0].split(': ', 1)[0]
                break
        if not owner:
            raise ValueError('No installed package owner for ' + library.name)
        package = owner.split(':')[0]
        copyright_file = Path('/usr/share/doc') / package / 'copyright'
        raw = copyright_file.read_bytes()
        if not raw:
            raise ValueError('Missing package copyright: ' + package)
        entries[f'packages/{package}/copyright.txt'] = raw
        version = subprocess.check_output(['dpkg-query', '-W', '-f=${Version}', owner], text=True)
        libraries.append({'soname': library.name, 'package': owner, 'version': version,
                          'library_sha256': hashlib.sha256(library.read_bytes()).hexdigest()})
    if not libraries:
        raise ValueError('No native library inventory was collected')
    # Debian copyright records reference these files. Keep the actual installed
    # common texts conservatively rather than infer a minimal subset.
    for path in sorted(Path('/usr/share/common-licenses').iterdir()):
        if path.is_file():
            entries['common/' + path.name + '.txt'] = path.read_bytes()
    if 'common/LGPL-2.1.txt' not in entries:
        raise ValueError('Missing installed LGPL-2.1 text')
    manifest = {'schema_version': 'ounce-core/native-notices-v1',
                'scope': 'Build-host shared-library notices; libraries are not bundled',
                'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                'libraries': libraries,
                'files_sha256': {name: hashlib.sha256(raw).hexdigest() for name, raw in entries.items()}}
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.native-notices-', dir=output.parent) as tmp:
        staging = Path(tmp) / 'notices'
        staging.mkdir()
        for name, raw in entries.items():
            path = staging / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        (staging / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        staging.rename(output)
    return manifest


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        report = collect(args.binary, args.output)
        print('Retained native notices for', len(report['libraries']), 'installed shared libraries')
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, 'Native notice collection failed: ' + str(error) + '\n')
