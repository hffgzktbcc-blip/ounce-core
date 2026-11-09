#!/usr/bin/env python3
"""Retain the browser graph's actual license texts and binary-bound Rust notices.

This is an engineering inventory for a browser research preview, not a legal-review decision
or an override of the native app's release controls.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from collect_toolchain_notices import collect

ROOT = Path(__file__).resolve().parents[1]


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def package(output):
    wasm = output / 'pkg/ounce_learn_bg.wasm'
    lock = ROOT / 'crates/ounce-learn/Cargo.lock'
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--offline', '--locked', '--format-version', '1',
        '--manifest-path', str(ROOT / 'crates/ounce-learn/Cargo.toml')], text=True))
    entries, packages = {}, []
    for item in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if item['source'] is None:  # Project license is retained at the preview root.
            continue
        directory = Path(item['manifest_path']).parent
        notices = [p for p in sorted(directory.iterdir()) if p.is_file() and p.name.startswith(('LICENSE', 'COPYING', 'NOTICE'))]
        if not notices:
            raise ValueError('No actual license text found for ' + item['name'])
        packages.append({k: item[k] for k in ('name', 'version', 'license')})
        for notice in notices:
            name = f"packages/{item['name']}-{item['version']}/{notice.name}"
            entries[name] = notice.read_bytes()
    with tempfile.TemporaryDirectory(prefix='.playground-notices-', dir=output) as tmp:
        staging = Path(tmp) / 'notices'
        staging.mkdir()
        toolchain = collect(wasm, 'wasm32-unknown-unknown', staging / 'rust')
        for name, raw in entries.items():
            path = staging / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        inventory = {
            'schema': 'ounce-core/playground-notices-v1',
            'scope': 'browser research preview; inventory is not a native-app distribution approval',
            'wasm_sha256': digest(wasm.read_bytes()), 'cargo_lock_sha256': digest(lock.read_bytes()),
            'rustc_version': toolchain['rustc_version'], 'packages': packages,
            'files_sha256': {p.relative_to(staging).as_posix(): digest(p.read_bytes())
                             for p in sorted(staging.rglob('*')) if p.is_file()},
        }
        raw = (json.dumps(inventory, indent=2) + '\n').encode()
        (staging / 'manifest.json').write_bytes(raw)
        identity = digest(raw)[:24]
        destination = output / 'licenses' / identity
        destination.parent.mkdir(exist_ok=True)
        if destination.exists():
            # Rebuilding the same artifact is allowed only with intact notices.
            for path in staging.rglob('*'):
                if path.is_file() and (destination / path.relative_to(staging)).read_bytes() != path.read_bytes():
                    raise ValueError('Existing notice artifact differs; preserve and inspect ' + str(destination))
        else:
            staging.rename(destination)
        index = {'schema': 'ounce-core/playground-artifact-v1',
                 'notices_manifest': f'licenses/{identity}/manifest.json',
                 'notices_manifest_sha256': digest(raw), 'wasm_sha256': inventory['wasm_sha256'],
                 'project_notices_sha256': {name: digest((ROOT / name).read_bytes()) for name in ('LICENSE', 'THIRD_PARTY.md', 'THIRD_PARTY_NOTICES.txt')},
                 'preview_files_sha256': {p.relative_to(output).as_posix(): digest(p.read_bytes()) for p in [output / 'index.html', *sorted((output / 'pkg').glob('*'))] if p.is_file()},
                 'source_files_sha256': {p.relative_to(ROOT).as_posix(): digest(p.read_bytes())
                     for p in [ROOT / 'crates/ounce-learn/src/lib.rs', *sorted((ROOT / 'src/cpu').glob('*.rs')), *sorted((ROOT / 'src/mmu').glob('*.rs'))]}}
        (output / 'artifact-manifest.json').write_text(json.dumps(index, indent=2) + '\n')
    for name in ('LICENSE', 'THIRD_PARTY.md', 'THIRD_PARTY_NOTICES.txt'):
        shutil.copyfile(ROOT / name, output / name)
    return index


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = package(args.output)
    print('Retained browser-graph and Rust notices for ' + result['wasm_sha256'])
