#!/usr/bin/env python3
"""Build a self-contained, standard-library-only Python zip application."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import zipapp

REPO = Path(__file__).resolve().parents[1]
MODULES = ('diagnosis_cli', 'reproduce_failure', 'compare_corpus_runs', 'diagnose_corpus_run',
           'corpus_verify', 'run_corpus_oracles', 'corpus_build_cache')
BOOTSTRAP = '''import sys
if sys.version_info < (3, 9):
    raise SystemExit("ounce-diagnose requires Python 3.9 or newer")
import tempfile, zipfile
from pathlib import Path
with tempfile.TemporaryDirectory(prefix="ounce-diagnose-") as directory:
    with zipfile.ZipFile(sys.argv[0]) as archive:
        archive.extractall(directory)
    root = Path(directory)
    sys.path.insert(0, str(root / "tools"))
    sys.path.insert(0, str(root / "scripts"))
    from diagnosis_cli import main
    raise SystemExit(main())
'''


def build(output):
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists() or output.with_suffix(output.suffix + '.sha256').exists():
        raise ValueError('output or checksum already exists; choose a fresh path')
    with tempfile.TemporaryDirectory(prefix='ounce-cli-build-') as temp:
        root = Path(temp)
        (root / 'scripts').mkdir()
        (root / 'tools').mkdir()
        for name in MODULES:
            shutil.copyfile(REPO / 'scripts' / (name + '.py'), root / 'scripts' / (name + '.py'))
        shutil.copyfile(REPO / 'tools/ounce_verify.py', root / 'tools/ounce_verify.py')
        shutil.copyfile(REPO / 'LICENSE', root / 'LICENSE')
        source = REPO / 'docs/reproduce/sdl2-failure'
        target = root / 'docs/reproduce/sdl2-failure'
        # Only the six reviewed observation files and their manifest/notice.
        # No historical binary inputs, local logs, caches or build outputs.
        manifest = json.loads((source / 'manifest.json').read_bytes())
        for name in [*manifest['files'], 'manifest.json', 'NOTICE.md']:
            relative = Path(name)
            if relative.is_absolute() or '..' in relative.parts:
                raise ValueError('invalid retained evidence path')
            destination = target / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / relative, destination)
        (root / '__main__.py').write_text(BOOTSTRAP)
        with output.open('xb') as stream:
            zipapp.create_archive(root, stream, interpreter='/usr/bin/env python3', compressed=True)
    output.chmod(0o755)
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    output.with_suffix(output.suffix + '.sha256').write_text(f'{digest}  {output.name}\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='fresh .pyz path')
    args = parser.parse_args()
    try:
        build(args.output)
    except (OSError, ValueError) as error:
        parser.exit(2, f'ounce-diagnose build: {error}\n')
    print(f'Built {args.output} with SHA-256 checksum; requires Python 3.9+, no third-party packages.')


if __name__ == '__main__':
    main()
