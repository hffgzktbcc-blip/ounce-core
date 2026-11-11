#!/usr/bin/env python3
"""Retain installed Rust licence/copyright files with a binary-bound inventory."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def collect(binary, target, output):
    if output.exists() or output.is_symlink():
        raise ValueError('Output already exists; choose a fresh notice directory')
    version = subprocess.check_output(['rustc', '-Vv'], text=True).strip()
    sysroot = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    sources = {}
    for name in ['LICENSE-MIT', 'LICENSE-APACHE', 'COPYRIGHT-library.html', 'COPYRIGHT.html']:
        candidates = [sysroot / folder / name
                      for folder in ['share/doc/rust', 'share/doc/rust/html']]
        reuse = {'LICENSE-MIT': 'MIT.txt', 'LICENSE-APACHE': 'Apache-2.0.txt'}
        if name in reuse:
            candidates.append(sysroot / 'share/doc/rust/licenses' / reuse[name])
        for path in candidates:
            if path.is_file() and path.stat().st_size:
                sources[name] = path.read_bytes()
                break
    # Recent Rust distributions install REUSE-managed licence texts here instead
    # of the older top-level LICENSE-MIT/APACHE files (dist.rs, rustc 1.99).
    # Preserve all supplied texts and their relative paths, not only two labels.
    for path in sorted((sysroot / 'share/doc/rust/licenses').glob('*.txt')):
        if path.is_file() and path.stat().st_size:
            sources['licenses/' + path.name] = path.read_bytes()
    if not {'LICENSE-MIT', 'LICENSE-APACHE'}.issubset(sources) or not any(
            name.startswith('COPYRIGHT') for name in sources):
        raise ValueError('Rust toolchain notices missing; install the matching rust-docs component')
    # Copyright for the whole toolchain is conservative if the library-only file
    # is absent; neither is replaced by a generic MIT/Apache metadata label.
    data = {'schema_version': 'ounce-core/toolchain-notices-v1',
            'rustc_version': version, 'target': target,
            'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
            'files_sha256': {name: hashlib.sha256(raw).hexdigest()
                             for name, raw in sources.items()}}
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.rust-notices-', dir=output.parent) as temp:
        staging = Path(temp) / 'notices'
        staging.mkdir()
        for name, raw in sources.items():
            (staging / name).parent.mkdir(parents=True, exist_ok=True)
            (staging / name).write_bytes(raw)
        (staging / 'manifest.json').write_text(json.dumps(data, indent=2) + '\n')
        staging.rename(output)
    return data


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--target', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        data = collect(args.binary, args.target, args.output)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f'Toolchain notice collection failed: {error}\n')
    print(f"Retained {len(data['files_sha256'])} toolchain notice files for {data['target']}")
