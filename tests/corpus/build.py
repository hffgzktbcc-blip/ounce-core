#!/usr/bin/env python3
"""Build pinned unchanged upstream candidates in a fresh external directory."""
import argparse
import hashlib
import json
import os
import shlex
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('directory', type=Path, help='new external build directory')
parser.add_argument('--devkitpro', type=Path, default=Path(os.environ.get('DEVKITPRO', '/opt/devkitpro')))
args = parser.parse_args()
root = args.directory.resolve()
root.mkdir(parents=True, exist_ok=False)
manifest = json.loads(Path(__file__).with_name('sources.json').read_text())
report = {'schema_version': 1, 'runtime_pass_claim': False, 'builds': []}
checkouts = {}
env = dict(os.environ, DEVKITPRO=str(args.devkitpro.resolve()))

def run(command, cwd=None):
    return subprocess.check_output(command, cwd=cwd, env=env, text=True, stderr=subprocess.STDOUT)

for sample in manifest['candidates']:
    source, build = sample['upstream'], sample['build']
    key = (source['repository'], source['commit'])
    directory = root / build['checkout_directory']
    try:
        if key not in checkouts:
            directory.mkdir()
            run(['git', 'init', '-q'], directory)
            run(['git', 'fetch', '--depth=1', source['repository'], source['commit']], directory)
            run(['git', 'checkout', '--detach', 'FETCH_HEAD'], directory)
            checkouts[key] = directory
        directory = checkouts[key]
        revision = run(['git', 'rev-parse', 'HEAD'], directory).strip()
        if revision != source['commit']:
            raise RuntimeError('source revision differs from manifest')
        project = directory / build.get('working_directory', '.')
        mapping = '-ffile-prefix-map=' + str(root) + '=/ounce-corpus'
        compiler = args.devkitpro.resolve() / 'devkitA64/bin/aarch64-none-elf-'
        make_args = list(build['make_arguments']) + [
            'CC=' + shlex.quote(str(compiler) + 'gcc') + ' ' + shlex.quote(mapping),
            'CXX=' + shlex.quote(str(compiler) + 'g++') + ' ' + shlex.quote(mapping),
        ]
        log = run(['make', *make_args, 'clean'], project)
        log += run(['make', *make_args, 'all'], project)
        artifact = project / (project.resolve().name + '.nro')
        digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
        if run(['git', 'diff', '--exit-code'], directory):
            raise RuntimeError('tracked upstream sources changed during build')
        entry = {'id': sample['id'], 'revision': revision, 'artifact': str(artifact),
                 'path_normalization': 'checkout root mapped to /ounce-corpus via -ffile-prefix-map',
                 'sha256': digest, 'matches_recorded_build': digest == build['sha256']}
        report['builds'].append(entry)
        (root / (sample['id'] + '.log')).write_text(log)
    except subprocess.CalledProcessError as error:
        (root / (sample['id'] + '.log')).write_text(error.output)
        raise
    finally:
        (root / 'build-report.json').write_text(json.dumps(report, indent=2) + '\n')

print(json.dumps(report, indent=2))
if not all(item['matches_recorded_build'] for item in report['builds']):
    raise SystemExit('Build hashes differ from the recorded environment; inspect the report.')
