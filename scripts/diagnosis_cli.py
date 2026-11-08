#!/usr/bin/env python3
"""Offline retained SDL2/audio diagnosis, with explicit bounded SDL2 replay."""
import argparse
from contextlib import suppress
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys

from reproduce_failure import EXAMPLE, read_evidence, write_bundle
from audio_open_diagnosis import EXAMPLE as AUDIO_EXAMPLE, write_audio_bundle

VERSION = '0.2.0'
NRO = Path('switch-examples/build/source/graphics/sdl2/sdl2-simple/sdl2-simple.nro')
TEST = 'loader::fixture_tests::corpus_runner::corpus_first_frame_oracle'
FIELDS = ('engine', 'stop', 'pc', 'instructions', 'x0', 'queued', 'failures',
          'host_timeout_seconds', 'max_guest_instructions')


def seconds(value):
    number = float(value)
    if not math.isfinite(number) or number <= 0:
        raise argparse.ArgumentTypeError('timeout must be finite and positive')
    return number


def run_bounded(command, cwd, log, timeout, env=None):
    """Use the existing verifier's POSIX process-group kill/reap pattern."""
    if os.name != 'posix':
        raise ValueError('replay requires macOS or Linux; offline diagnosis is separate')
    process = None
    def terminate(signum, frame):
        raise KeyboardInterrupt
    previous = signal.signal(signal.SIGTERM, terminate)
    try:
        with log.open('xb') as stream:
            process = subprocess.Popen(command, cwd=cwd, stdout=stream, stderr=subprocess.STDOUT,
                                       env=env, start_new_session=True)
            try:
                code = process.wait(timeout=timeout)
                return {'status': 'exited', 'exit_code': code}
            except subprocess.TimeoutExpired:
                return {'status': 'timeout', 'exit_code': 124}
            except KeyboardInterrupt:
                return {'status': 'interrupted', 'exit_code': 130}
            finally:
                # Also stop descendants left behind by a normally exiting parent.
                with suppress(ProcessLookupError):
                    os.killpg(process.pid, signal.SIGKILL)
                process.wait()
    except OSError as error:
        raise ValueError(f'could not launch replay; check Cargo and runtime prerequisites: {error}') from error
    finally:
        signal.signal(signal.SIGTERM, previous)


def replay(args):
    manifest, files, records = read_evidence(EXAMPLE)
    expected = json.loads(files[manifest['runs'][args.side]['observation']])
    corpus = args.corpus_root.resolve()
    binary = corpus / NRO
    if hashlib.sha256(binary.read_bytes()).hexdigest() != expected['nro_sha256']:
        raise ValueError('NRO SHA-256 does not match the recorded input; use the exact authorized binary')
    checkout = args.checkout.resolve()
    commit = records[args.side]['sdl2-simple', 'interpreter']['execution']['core_commit']
    try:
        actual = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=checkout,
                                         text=True, stderr=subprocess.PIPE, timeout=10).strip()
        dirty = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=normal'],
                                        cwd=checkout, text=True, stderr=subprocess.PIPE, timeout=10)
    except (subprocess.SubprocessError, OSError) as error:
        raise ValueError('cannot inspect checkout; provide a clean Git checkout at the recorded revision') from error
    if actual != commit or dirty.strip():
        raise ValueError(f'replay requires a clean checkout at {commit}; see the bundle REPLAY.md')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith('OUNCE_') and key != 'GITHUB_SHA'}
    env.update(OUNCE_CORPUS_ROOT=str(corpus), OUNCE_ORACLE_RESULT=str(output / 'observation.json'),
               OUNCE_ORACLE_ID='sdl2-simple', OUNCE_ORACLE_SECONDS='180',
               CARGO_TARGET_DIR=str(output / 'build'))
    command = ['cargo', 'test', '--release', '--locked', TEST, '--', '--exact', '--ignored',
               '--nocapture', '--test-threads=1']
    result = {'schema_version': 'ounce-diagnose/replay-v1', 'side': args.side,
              'core_commit': commit, 'binary_sha256': expected['nro_sha256'],
              'timeout_seconds': args.timeout, 'matched': False}
    result.update(run_bounded(command, checkout, output / 'runtime.log', args.timeout, env))
    if result['status'] == 'exited':
        try:
            observed = json.loads((output / 'observation.json').read_bytes())
            by_engine = {run['engine']: run for run in observed['runs']}
            result['matched'] = (
                result['exit_code'] == (0 if expected['pass'] else 101)
                and observed['id'] == expected['id']
                and observed['nro_sha256'] == expected['nro_sha256']
                and observed['pass'] is expected['pass']
                and len(observed['runs']) == 2 and set(by_engine) == {'Interpreter', 'Tier3'}
                and all(by_engine[run['engine']][field] == run[field]
                        for run in expected['runs'] for field in FIELDS))
        except (OSError, ValueError, KeyError, TypeError):
            result['observation_error'] = 'missing or malformed observation; inspect runtime.log'
    result['files_sha256'] = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                              for path in (output / 'runtime.log', output / 'observation.json') if path.is_file()}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(f"Replay {result['status']}; recorded observations "
          f"{'match' if result['matched'] else 'not matched'}. Local logs: {output}")
    if result['status'] in {'timeout', 'interrupted'}:
        return result['exit_code']
    return 0 if result['matched'] else 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', action='version', version=f'ounce-diagnose {VERSION}')
    commands = parser.add_subparsers(dest='command', required=True)
    example = commands.add_parser('example', help='create the reviewed offline SDL2 dossier; no execution')
    example.add_argument('--output', type=Path, required=True)
    bundle = commands.add_parser('bundle', help='inspect a retained SDL2 evidence directory of the same format')
    bundle.add_argument('--evidence', type=Path, required=True)
    bundle.add_argument('--output', type=Path, required=True)
    audio = commands.add_parser('audio-case', help='inspect the recorded audio-padding before/after case; no execution')
    audio.add_argument('--evidence', type=Path, default=AUDIO_EXAMPLE)
    audio.add_argument('--output', type=Path, required=True)
    execute = commands.add_parser('replay', help='execute only the pinned SDL2 recipe (requires Rust and exact NRO)')
    execute.add_argument('--side', choices=('failure', 'reference'), required=True)
    execute.add_argument('--checkout', type=Path, required=True)
    execute.add_argument('--corpus-root', type=Path, required=True)
    execute.add_argument('--output', type=Path, required=True)
    execute.add_argument('--timeout', type=seconds, default=600, help='outer seconds, including build (default 600)')
    args = parser.parse_args(argv)
    try:
        if args.command == 'audio-case':
            report = write_audio_bundle(args.evidence, args.output)
            matched = report['recorded_repair_matches']
            print('Offline audio diagnosis: seven padded default-stereo opens rejected before; '
                  + ('seven stereo replies after on both engines.' if matched else 'recorded repair not matched.'))
            print(f'No guest executed. Bundle: {args.output} (read REPLAY.md).')
            return 0 if matched else 1
        if args.command == 'replay':
            return replay(args)
        evidence = EXAMPLE if args.command == 'example' else args.evidence
        diagnosis, comparison = write_bundle(evidence, args.output)
        print(f"Offline diagnosis: {len(diagnosis['cases'])} engine cases; first divergence at frame 1; "
              f"{len(comparison['regressions'])} lost prior passes. Bundle: {args.output}")
        print('No guest executed. Read REPLAY.md for the separate runtime prerequisites.')
        return 0
    except KeyboardInterrupt:
        print('ounce-diagnose: interrupted', file=sys.stderr)
        return 130
    except (OSError, ValueError, KeyError, TypeError, IndexError, AttributeError, StopIteration) as error:
        print(f'ounce-diagnose: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
