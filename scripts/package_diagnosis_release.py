#!/usr/bin/env python3
"""Export a clean, allowlisted Python toolkit snapshot and checksummed release assets."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

from build_diagnosis_cli import MODULES
from diagnosis_cli import VERSION

REPO = Path(__file__).resolve().parents[1]
README = '''# Ounce diagnosis toolkit {version}

This standalone Python tool inspects two retained failure/reproduction cases:
SDL2 startup and the external libnx #685 audio-padding interaction. It does not
contain or distribute a runtime, homebrew executable, upstream source, firmware,
keys, fonts or frame pixels. Project-authored tooling and observation data are
covered by LICENSE; observation-specific notices are included in generated bundles.

Python 3.9+ is required. No pip package, Cargo, Docker or network is needed for
recorded diagnosis. From this standalone source snapshot:

```sh
python3 -B scripts/build_diagnosis_cli.py --output dist/ounce-diagnose.pyz
python3 -I dist/ounce-diagnose.pyz --version
python3 -I dist/ounce-diagnose.pyz example --output sdl2-dossier
python3 -I dist/ounce-diagnose.pyz audio-case --output audio-dossier
python3 -B -m unittest discover -s scripts -p test_diagnosis_cli.py
```

For the downloaded archive, run the same commands with its path. Verify SHA256SUMS
first. Checksums detect changed files; they do not authenticate a publisher.
The audio command explains both engines' seven rejected-before and stereo-after
replies. It checks recorded identities/digests and reply consistency; it does not
execute the guest, re-verify absent binaries, certify new code, or authenticate
input manifests. Custom dossiers need review before sharing; no credential scrubber
is provided. See each generated NOTICE.md and REPLAY.md.

Exit codes: 0 = dossier written or explicit SDL2 replay matched; 1 = recorded audio
repair/replay not matched; 2 = invalid input/prerequisite/output; 124 = explicit
SDL2 replay timeout; 130 = interruption. `replay` is SDL2-only and requires a
separate clean full-runtime checkout, Git/Cargo and the authorized exact-hash NRO.
Audio replay uses the separate generated Cargo recipe. The runtime is absent from
this source snapshot. Retained examples are internal evidence; no outside user
endorsement, independent rerun, six-channel playback or general debugger support
is claimed. No maintainer was contacted and no adoption feedback is available.

SOURCE.json identifies the original repository commit and every exported input.
This snapshot has its own standalone release history. It excludes the runtime
fixtures and historical source tree whose separate provenance review remains open.
The runtime distribution and its release assets are unchanged.
'''


def git(repo, *args):
    return subprocess.check_output(['git', *args], cwd=repo, text=True, stderr=subprocess.PIPE).strip()


def package(repo, output):
    repo = repo.resolve()
    if output.exists() or output.is_symlink():
        raise ValueError('output already exists; choose a fresh directory')
    if git(repo, 'status', '--porcelain', '--untracked-files=normal'):
        raise ValueError('release packaging requires a clean committed source checkout')
    commit = git(repo, 'rev-parse', 'HEAD')
    paths = [f'scripts/{name}.py' for name in MODULES]
    paths += ['scripts/build_diagnosis_cli.py', 'scripts/test_diagnosis_cli.py',
              'tools/ounce_verify.py', 'LICENSE', '.github/workflows/diagnosis-release.yml']
    sdl = 'docs/reproduce/sdl2-failure'
    manifest = json.loads((repo / sdl / 'manifest.json').read_bytes())
    sdl_files = {'reference/canonical-report.json', 'reference/sdl2-simple.oracle.json',
                 'reference/sdl2-simple.oracle.log', 'failure/canonical-report.json',
                 'failure/sdl2-simple.oracle.json', 'failure/sdl2-simple.oracle.log'}
    if set(manifest['files']) != sdl_files:
        raise ValueError('unexpected SDL2 release inputs')
    paths += [f'{sdl}/{name}' for name in sorted(sdl_files)] + [f'{sdl}/manifest.json', f'{sdl}/NOTICE.md']
    audio = 'docs/verify/audout-padding'
    manifest = json.loads((repo / audio / 'manifest.json').read_bytes())
    audio_files = {'before.json', 'after.json', 'before.log', 'after.log', 'canonical-corpus.json'}
    if set(manifest['files_sha256']) != audio_files:
        raise ValueError('unexpected audio release inputs')
    paths += [f'{audio}/{name}' for name in sorted(audio_files)] + [f'{audio}/manifest.json']
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.diagnosis-release-', dir=output.parent) as temp:
        staging = Path(temp) / 'release'
        source = staging / 'source'
        source.mkdir(parents=True)
        hashes = {}
        for name in paths:
            original = repo / name
            if original.is_symlink() or not original.resolve().is_relative_to(repo):
                raise ValueError(f'linked release input is not allowed: {name}')
            git(repo, 'ls-files', '--error-unmatch', name)
            data = original.read_bytes()
            target = source / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            hashes[name] = hashlib.sha256(data).hexdigest()
        if git(repo, 'rev-parse', 'HEAD') != commit or git(repo, 'status', '--porcelain', '--untracked-files=normal'):
            raise ValueError('source changed during packaging; a clean checkout is required')
        (source / 'README.md').write_text(README.format(version=VERSION))
        (source / '.gitattributes').write_text('* text=auto eol=lf\n')
        provenance = {'schema_version': 'ounce-diagnose/source-snapshot-v1', 'version': VERSION,
                      'original_repository': 'https://github.com/hffgzktbcc-blip/ounce-core',
                      'original_commit': commit, 'files_sha256': hashes,
                      'generated_files_sha256': {name: hashlib.sha256((source / name).read_bytes()).hexdigest()
                                                 for name in ['README.md', '.gitattributes']},
                      'distribution_scope': 'Project-authored Python tooling and observation data only.'}
        (source / 'SOURCE.json').write_text(json.dumps(provenance, indent=2) + '\n')
        binary = staging / f'ounce-diagnose-{VERSION}.pyz'
        subprocess.run([sys.executable, '-B', str(source / 'scripts/build_diagnosis_cli.py'),
                        '--output', str(binary)], check=True, capture_output=True, text=True)
        archive = staging / f'ounce-diagnose-{VERSION}-source.zip'
        with zipfile.ZipFile(archive, 'x', compression=zipfile.ZIP_DEFLATED) as bundle:
            for path in sorted(source.rglob('*')):
                if path.is_file() and '__pycache__' not in path.parts:
                    info = zipfile.ZipInfo(path.relative_to(source).as_posix(), (1980, 1, 1, 0, 0, 0))
                    info.external_attr = 0o100644 << 16
                    bundle.writestr(info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED)
        checksums = ''.join(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n'
                            for path in [binary, archive])
        (staging / 'SHA256SUMS').write_text(checksums)
        staging.rename(output)
    return commit


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=REPO)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        commit = package(args.repo, args.output)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(2, f'diagnosis release: {error}\n')
    print(f'Exported toolkit {VERSION} from {commit}; source snapshot and SHA256SUMS in {args.output}')


if __name__ == '__main__':
    main()
