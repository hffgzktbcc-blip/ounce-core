#!/usr/bin/env python3
"""Repeated, oracle-checked end-to-end throughput on registered corpus NROs.

Timing comes from the existing runner, including load, kernel and frame capture;
it excludes process startup. Cold engines are created for every sample. No downloads.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import statistics
import subprocess

REPO = Path(__file__).resolve().parents[1]
TEST = 'loader::fixture_tests::corpus_runner::corpus_first_frame_oracle'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def summarize(samples):
    signatures = {(s['instructions'], json.dumps(s['queued'], sort_keys=True)) for s in samples}
    if len(signatures) != 1:
        raise ValueError('guest instruction counts or frames changed between repetitions')
    rates = [s['instructions'] / s['wall_seconds'] for s in samples]
    frames = [len(s['queued']) / s['wall_seconds'] for s in samples]
    return {'samples': samples, 'instructions_per_second_median': statistics.median(rates),
            'instructions_per_second_min': min(rates), 'instructions_per_second_max': max(rates),
            'frames_per_second_median': statistics.median(frames)}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--executable', required=True, type=Path, help='release cargo test executable')
    p.add_argument('--baseline', type=Path, help='optional baseline test executable, interleaved A/B')
    p.add_argument('--builds', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path, help='new directory; preserves raw results/logs')
    p.add_argument('--only', nargs='+', default=['printing-hello-world', 'simplegfx'])
    p.add_argument('--repeats', type=int, default=5)
    p.add_argument('--warmups', type=int, default=1)
    p.add_argument('--seconds', type=int, default=300)
    a = p.parse_args()
    if a.repeats < 1 or a.warmups < 0 or a.seconds < 1:
        p.error('repeats/seconds must be positive and warmups nonnegative')
    manifest = REPO / 'tests/corpus/oracles.json'
    registered = {o['id'] for o in json.loads(manifest.read_text())['oracles']}
    if len(set(a.only)) != len(a.only) or set(a.only) - registered:
        p.error('--only requires unique registered oracle IDs')
    a.output.mkdir(parents=True, exist_ok=False)
    exe = a.executable.resolve()
    executables = {'candidate': exe}
    if a.baseline:
        executables = {'baseline': a.baseline.resolve(), **executables}
    report = {'complete': False, 'schema': 'ounce-core/corpus-benchmark-v1', 'host': platform.platform(),
              'machine': platform.machine(), 'runtime_sha256': {label: digest(path) for label, path in executables.items()},
              'manifest_sha256': digest(manifest), 'repeats': a.repeats, 'warmups': a.warmups,
              'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(),
              'dirty': bool(subprocess.check_output(['git', 'diff', 'HEAD'], cwd=REPO)), 'cases': {}}
    try:
        for case in a.only:
            samples = {label: {'Interpreter': [], 'Tier3': []} for label in executables}
            binary = None
            cpu_samples = {label: [] for label in executables}
            for n in range(a.warmups + a.repeats):
                for label, executable in executables.items():
                    stem = a.output / f'{case}-{n}-{label}'
                    result = stem.with_suffix('.json').resolve()
                    env = dict(os.environ, OUNCE_CORPUS_ROOT=str(a.builds.resolve()),
                               OUNCE_ORACLE_ID=case, OUNCE_ORACLE_RESULT=str(result),
                               OUNCE_ORACLE_SECONDS=str(a.seconds))
                    env.pop('OUNCE_CPU', None)
                    cmd = [str(executable), TEST, '--exact', '--ignored', '--nocapture', '--test-threads=1']
                    cpu_start = resource.getrusage(resource.RUSAGE_CHILDREN)
                    with stem.with_suffix('.log').open('w') as log:
                        subprocess.run(cmd, cwd=REPO, env=env, stdout=log, stderr=subprocess.STDOUT,
                                       timeout=2 * a.seconds + 30, check=True)
                    cpu_end = resource.getrusage(resource.RUSAGE_CHILDREN)
                    if n >= a.warmups:
                        cpu_samples[label].append(cpu_end.ru_utime + cpu_end.ru_stime - cpu_start.ru_utime - cpu_start.ru_stime)
                    stem.with_suffix('.timing.json').write_text(json.dumps({
                        'cpu_seconds': cpu_end.ru_utime + cpu_end.ru_stime - cpu_start.ru_utime - cpu_start.ru_stime,
                        'warmup': n < a.warmups}) + '\n')
                    verdict = json.loads(result.read_text())
                    if not verdict['pass'] or verdict['id'] != case or [r['engine'] for r in verdict['runs']] != ['Interpreter', 'Tier3']:
                        raise ValueError(f'{case}: oracle failed or wrong engines')
                    if binary is not None and binary != verdict['nro_sha256']:
                        raise ValueError(f'{case}: NRO changed')
                    binary = verdict['nro_sha256']
                    for r in verdict['runs']:
                        if r['failures'] or r['stop'] != 'FrameLimit' or r['wall_seconds'] <= 0:
                            raise ValueError(f'{case}: incomplete execution')
                        if n >= a.warmups:
                            samples[label][r['engine']].append(r)
            entry = {'nro_sha256': binary, 'combined_engine_cpu_seconds': cpu_samples, 'runtimes': {
                label: {e: summarize(s) for e, s in engines.items()} for label, engines in samples.items()}}
            if a.baseline:
                cpu_changes = [100 * (b / c - 1) for b, c in zip(cpu_samples['baseline'], cpu_samples['candidate'])]
                entry['paired_combined_cpu_speedup_percent'] = {'samples': cpu_changes, 'median': statistics.median(cpu_changes),
                                                               'min': min(cpu_changes), 'max': max(cpu_changes)}
                entry['paired_speedup_percent'] = {}
                for engine in ('Interpreter', 'Tier3'):
                    before, after = samples['baseline'][engine], samples['candidate'][engine]
                    if any((b['instructions'], b['queued']) != (c['instructions'], c['queued'])
                           for b, c in zip(before, after)):
                        raise ValueError(f'{case}/{engine}: baseline and candidate guest work differs')
                    changes = [100 * (b['wall_seconds'] / c['wall_seconds'] - 1) for b, c in zip(before, after)]
                    entry['paired_speedup_percent'][engine] = {'samples': changes, 'median': statistics.median(changes),
                                                              'min': min(changes), 'max': max(changes)}
            report['cases'][case] = entry
            print(case, entry.get('paired_speedup_percent', {
                e: round(s['instructions_per_second_median']) for e, s in entry['runtimes']['candidate'].items()}), flush=True)
        report['complete'] = True
    finally:
        (a.output / 'benchmark.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
