#!/usr/bin/env python3
"""Retain licence files from the locked macOS app dependency graph (including optional features)."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def collect(output):
    root = Path(__file__).resolve().parents[1]
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--locked', '--format-version', '1',
        '--filter-platform', 'aarch64-apple-darwin', '--features', 'gpu-present nce native-jit',
        '--manifest-path', str(root / 'Cargo.toml')], text=True))
    selected = {node['id'] for node in metadata['resolve']['nodes']}
    inventory = []
    for package in metadata['packages']:
        if package['id'] not in selected or package['source'] is None:
            continue
        source = Path(package['manifest_path']).parent
        files = {}
        for path in sorted(source.rglob('*')):
            if not path.is_file() or path.is_symlink():
                continue
            name = path.name.lower()
            if not (name.startswith(('license', 'licence', 'copying', 'copyright', 'notice'))
                    or (package.get('license_file') and path == source / package['license_file'])):
                continue
            relative = path.relative_to(source)
            target = output / (package['name'] + '-' + package['version']) / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            raw = path.read_bytes()
            target.write_bytes(raw)
            files[relative.as_posix()] = hashlib.sha256(raw).hexdigest()
        # objc2 workspace crates omit the shared licence from published crates.
        # The repository retains the upstream overview and supplemental MIT grant.
        if not files and package.get('repository') == 'https://github.com/madsmtm/objc2':
            for name in ['THIRD_PARTY_NOTICES.txt', 'licenses/objc2-MIT.txt', 'licenses/objc2-upstream-MIT.txt']:
                raw = (root / name).read_bytes()
                target = output / (package['name'] + '-' + package['version']) / Path(name).name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
                files[target.name] = hashlib.sha256(raw).hexdigest()
        if not files and package['name'].startswith(('cranelift-', 'wasmtime-internal-')):
            shared = next(p for p in metadata['packages'] if p['name'] == 'cranelift-codegen'
                          and p['license'] == package['license'])
            raw = (Path(shared['manifest_path']).parent / 'LICENSE').read_bytes()
            target = output / (package['name'] + '-' + package['version']) / 'LICENSE'
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
            files['LICENSE'] = hashlib.sha256(raw).hexdigest()
        reviewed = json.loads((root / 'docs/RELEASE_NOTICE_REVIEW.json').read_text())
        if not files and any(p['package'] == package['name'] and p['version'] == package['version']
                             for p in reviewed['notice_sources']):
            raw = (root / 'THIRD_PARTY_NOTICES.txt').read_bytes()
            target = output / (package['name'] + '-' + package['version']) / 'THIRD_PARTY_NOTICES.txt'
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
            files[target.name] = hashlib.sha256(raw).hexdigest()
        retained = root / 'licenses/macos-app' / (package['name'] + '-' + package['version'])
        if not files and retained.is_dir():
            for path in retained.iterdir():
                raw = path.read_bytes()
                target = output / retained.name / path.name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
                files[path.name] = hashlib.sha256(raw).hexdigest()
        if not files:
            raise ValueError('No licence text found for ' + package['id'])
        inventory.append({'package': package['name'], 'version': package['version'],
                          'license': package['license'], 'files_sha256': files})
    (output / 'manifest.json').write_text(json.dumps({'features': 'gpu-present nce native-jit',
        'target': 'aarch64-apple-darwin', 'packages': inventory}, indent=2) + '\n')
    print(f'Retained app dependency notices for {len(inventory)} packages')


if __name__ == '__main__':
    collect(Path(sys.argv[1]))
