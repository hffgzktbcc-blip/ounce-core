#!/usr/bin/env python3
"""Interleaved A/B benchmark of an already-verified pinned Linux library harness.

First run verify_linux_elf.py with --engines interpreter tier3. This benchmark
requires its successful report, checks input hashes, compares every run's stdout
with that reference, and retains logs. Timing includes CLI startup and ELF loading.
"""
import argparse
import json
from pathlib import Path
import platform
import re
import resource
import statistics
import subprocess
import time

from benchmark_corpus import digest


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--verification', type=Path, required=True, help='verify_linux_elf report.json')
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--candidate', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True, help='new evidence directory')
    p.add_argument('--repeats', type=int, default=9)
    p.add_argument('--warmups', type=int, default=1)
    p.add_argument('--seconds', type=int, default=300)
    a = p.parse_args()
    if a.repeats < 1 or a.warmups < 0 or a.seconds < 1:
        p.error('repeats/seconds must be positive and warmups nonnegative')
    verified = json.loads(a.verification.read_text())
    if verified['failed'] or not verified['checks'] or any(x != 'identical' for x in verified['checks'].values()):
        p.error('verification must pass')
    root = a.verification.resolve().parent
    elf = root / (verified['harness'] + '.elf')
    expected = root / ('qemu.stdout' if verified.get('reference') == 'qemu' else 'host-run.stdout')
    reference_step = 'qemu' if verified.get('reference') == 'qemu' else 'host-run'
    expected_hash = next(s['stdout']['sha256'] for s in verified['steps'] if s['name'] == reference_step)
    if digest(elf) != verified['elf_sha256'] or digest(expected) != expected_hash:
        p.error('ELF/reference no longer matches verification')
    a.output.mkdir(parents=True, exist_ok=False)
    executables = {'baseline': a.baseline.resolve(), 'candidate': a.candidate.resolve()}
    report = {'complete': False, 'schema': 'ounce-core/linux-benchmark-v1', 'host': platform.platform(),
              'verification_sha256': digest(a.verification), 'elf_sha256': digest(elf),
              'reference_sha256': digest(expected), 'harness': verified['harness'],
              'runtime_sha256': {k: digest(v) for k, v in executables.items()},
              'repeats': a.repeats, 'warmups': a.warmups, 'engines': {}}
    try:
        for engine, identity in [('interpreter', 'Interpreter'), ('tier3', 'Tier-3')]:
            samples = {label: [] for label in executables}
            for n in range(a.warmups + a.repeats):
                for label, exe in executables.items():
                    cmd = [str(exe), 'run-elf', str(elf), '--cpu=' + engine, '--deterministic']
                    cpu_start = resource.getrusage(resource.RUSAGE_CHILDREN)
                    start = time.perf_counter()
                    r = subprocess.run(cmd, capture_output=True, timeout=a.seconds)
                    seconds = time.perf_counter() - start
                    cpu_end = resource.getrusage(resource.RUSAGE_CHILDREN)
                    cpu_seconds = (cpu_end.ru_utime + cpu_end.ru_stime - cpu_start.ru_utime - cpu_start.ru_stime)
                    stem = a.output / f'{engine}-{n}-{label}'
                    stem.with_suffix('.timing.json').write_text(json.dumps({
                        'wall_seconds': seconds, 'cpu_seconds': cpu_seconds, 'warmup': n < a.warmups,
                        'argv': cmd}) + '\n')
                    stem.with_suffix('.stdout').write_bytes(r.stdout)
                    stem.with_suffix('.stderr').write_bytes(r.stderr)
                    matches = re.findall(rb'^\[run-elf\] engine=(\S+) instructions=(\d+) syscalls=(\d+)$', r.stderr, re.M)
                    if r.returncode or r.stdout != expected.read_bytes() or len(matches) != 1 or matches[0][0].decode() != identity:
                        raise ValueError(f'{stem}: failed execution, engine identity or reference mismatch')
                    if n >= a.warmups:
                        samples[label].append({'seconds': seconds, 'cpu_seconds': cpu_seconds, 'instructions': int(matches[0][1]),
                                               'syscalls': int(matches[0][2])})
            if len({s['instructions'] for runs in samples.values() for s in runs}) != 1:
                raise ValueError('guest instruction counts changed')
            changes = [100 * (b['seconds'] / c['seconds'] - 1)
                       for b, c in zip(samples['baseline'], samples['candidate'])]
            cpu_changes = [100 * (b['cpu_seconds'] / c['cpu_seconds'] - 1)
                           for b, c in zip(samples['baseline'], samples['candidate'])]
            entry = {'paired_cpu_speedup_percent': {'samples': cpu_changes, 'median': statistics.median(cpu_changes),
                                                    'min': min(cpu_changes), 'max': max(cpu_changes)}, 'runtimes': {label: {'samples': runs, 'instructions_per_second_median': statistics.median(
                s['instructions'] / s['seconds'] for s in runs),
                'cpu_instructions_per_second_median': statistics.median(s['instructions'] / s['cpu_seconds'] for s in runs)} for label, runs in samples.items()},
                'paired_speedup_percent': {'samples': changes, 'median': statistics.median(changes),
                                          'min': min(changes), 'max': max(changes)}}
            report['engines'][engine] = entry
            print(engine, {'wall': entry['paired_speedup_percent'], 'cpu': entry['paired_cpu_speedup_percent']}, flush=True)
        report['complete'] = True
    finally:
        (a.output / 'benchmark.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
