#!/usr/bin/env python3
"""Check preview bytes against their declared inventory, without granting release approval."""
import argparse
import hashlib
import json
from pathlib import Path


def check(directory):
    root = directory.resolve()

    def read(relative):
        path = (root / relative).resolve()
        if not path.is_relative_to(root):
            raise ValueError('Artifact path escapes its directory')
        return path.read_bytes()

    def sha(raw):
        return hashlib.sha256(raw).hexdigest()

    artifact = json.loads(read('artifact-manifest.json'))
    if artifact['schema'] != 'ounce-core/playground-artifact-v1':
        raise ValueError('Unsupported preview manifest')
    if sha(read('pkg/ounce_learn_bg.wasm')) != artifact['wasm_sha256']:
        raise ValueError('WebAssembly hash mismatch')
    name = artifact['notices_manifest']
    raw = read(name)
    if sha(raw) != artifact['notices_manifest_sha256']:
        raise ValueError('Notice manifest hash mismatch')
    notices = json.loads(raw)
    if notices['wasm_sha256'] != artifact['wasm_sha256']:
        raise ValueError('Notices identify another binary')
    base = Path(name).parent
    for relative, expected in notices['files_sha256'].items():
        if sha(read(base / relative)) != expected:
            raise ValueError('Notice file hash mismatch: ' + relative)
    for field in ('project_notices_sha256', 'preview_files_sha256'):
        for relative, expected in artifact[field].items():
            if sha(read(relative)) != expected:
                raise ValueError('Preview or project-notice hash mismatch: ' + relative)
    return artifact


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    result = check(args.directory)
    print('Preview bytes and retained notice hashes verified: ' + result['wasm_sha256'])
