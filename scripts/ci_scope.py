#!/usr/bin/env python3
"""Classify changed paths for local checks and hosted CI, without network calls."""
import argparse
import json
import re
import subprocess


def classify(paths):
    runtime = False
    corpus = False
    for path in paths:
        # Notices and provenance records under tests/fixtures/ are checked by the tooling job, not built.
        fixture_record = path.startswith('tests/fixtures/') and (
            path.endswith('.md') or (path.count('/') == 2 and path.endswith('.json')))
        rust = ((path.startswith(('src/', 'crates/', 'tests/fixtures/', 'tools/macos-launcher/', 'tests/macos/', '.cargo/', 'assets/fonts/'))
                 and not fixture_record)
                or path in {'Cargo.toml', 'Cargo.lock', 'build.rs', 'rust-toolchain.toml',
                            '.github/workflows/ci.yml', 'scripts/ci_scope.py', 'scripts/build_macos_app.sh'}
                or path.endswith('.rs'))
        oracle = path.startswith('tests/corpus/')
        runtime |= rust or oracle
        corpus |= rust or oracle or path in {
            '.github/workflows/external-corpus.yml', 'tools/ounce_verify.py',
            'tools/corpus_oracle.py', 'tests/test_ounce_verify.py',
        } or (path.startswith('scripts/') and any(word in path for word in ('corpus', 'interaction_oracle')))
    return {'runtime': runtime, 'corpus': corpus}


def headline_changed(before, after):
    """Prose edits are cheap; altering/removing certified counts needs fresh evidence."""
    pattern = r'<!-- corpus-headline -->.*?<!-- /corpus-headline -->|first--frame_oracles-\d+%2F\d+_pass'
    return re.findall(pattern, before, re.S) != re.findall(pattern, after, re.S)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', required=True)
    parser.add_argument('--github-output')
    args = parser.parse_args()
    # Fail rather than quietly under-test if the supplied base is unavailable.
    paths = subprocess.check_output(['git', 'diff', '--name-only', '-z', args.base, 'HEAD']).decode().split('\0')[:-1]
    scope = classify(paths)
    if 'README.md' in paths:
        before = subprocess.run(['git', 'show', f'{args.base}:README.md'], capture_output=True, text=True)
        after = subprocess.run(['git', 'show', 'HEAD:README.md'], capture_output=True, text=True)
        scope['corpus'] |= before.returncode != 0 or after.returncode != 0 or headline_changed(before.stdout, after.stdout)
    print(json.dumps(scope, sort_keys=True))
    if args.github_output:
        with open(args.github_output, 'a') as output:
            output.writelines(f'{key}={str(value).lower()}\n' for key, value in scope.items())


if __name__ == '__main__':
    main()
