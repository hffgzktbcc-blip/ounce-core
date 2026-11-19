#!/usr/bin/env python3
"""Deterministic instruction budgets for committed guest programs (issue #479).

Each workload in tests/insn-budgets.json is a committed NRO or ELF fixture that stops on its own. The check runs it
with `--deterministic --report-json` on every listed CPU tier and reads `insns_executed` from the `ounce.run.v1`
report. It fails when a workload:

  * executes more guest instructions than its budget (the regression this guards against: the HLE kernel or a
    service makes the same guest program do more work, for example by spinning longer in a wait loop);
  * stops differently than recorded (a count from a run that crashed early is not comparable);
  * gets different counts on different tiers (every tier must count the same instructions);
  * no longer matches the recorded fixture SHA-256 (a rebuilt fixture needs a new baseline).

The guest instruction count of a fixed binary does not depend on the host, so a budget is exact and needs no
tolerance for runner noise. The headroom over the baseline lets small, intended changes through; a larger increase
needs the baseline updated (`--update`) and a reason in the PR. Wall time is printed for information only: shared CI
runners are too noisy to fail on it.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BUDGETS = ROOT / 'tests' / 'insn-budgets.json'
SCHEMA = 'ounce.insn-budgets.v1'
TIMEOUT_SECONDS = 300


def default_binary():
    name = 'ounce-core.exe' if os.name == 'nt' else 'ounce-core'
    return ROOT / 'target' / 'release' / name


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def budget_for(baseline, headroom_percent):
    return baseline + max(1, math.ceil(baseline * headroom_percent / 100))


def run_once(binary, command, tier, workdir):
    """Run one workload on one tier. Returns (report dict or None, wall seconds, failure text or None)."""
    report_path = Path(workdir) / f'report-{tier}.json'
    report_path.unlink(missing_ok=True)
    args = [str(binary), *command, f'--cpu={tier}', '--deterministic', f'--report-json={report_path}']
    start = time.monotonic()
    try:
        proc = subprocess.run(args, cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                              timeout=TIMEOUT_SECONDS, check=False)
    except subprocess.TimeoutExpired:
        return None, time.monotonic() - start, (
            f'did not stop within {TIMEOUT_SECONDS} s; a workload that used to stop on its own now runs on, which '
            'is the largest possible instruction regression')
    wall = time.monotonic() - start
    if not report_path.exists():
        tail = proc.stderr.decode('utf-8', 'replace').strip().splitlines()[-5:]
        return None, wall, f'wrote no report (exit status {proc.returncode}): ' + ' | '.join(tail)
    return json.loads(report_path.read_text()), wall, None


def measure(binary, spec, tiers):
    """Run `spec` on every tier. Returns (insns, stop_kind, wall seconds, list of problems)."""
    problems, results, wall = [], {}, 0.0
    with tempfile.TemporaryDirectory(prefix='ounce-insn-budget-') as workdir:
        for tier in tiers:
            report, seconds, failure = run_once(binary, spec['command'], tier, workdir)
            wall += seconds
            if failure:
                problems.append(f'{tier}: {failure}')
                continue
            status = report.get('guest_exit_status')
            if status not in (None, 0):
                # A run-elf program that fails its own checks still stops with clean_exit, often after fewer
                # instructions: its count must not pass as an improvement or be recorded as a baseline.
                problems.append(f'{tier}: the program exited with status {status}, so it did not do its work and its '
                                'instruction count is not comparable')
                continue
            results[tier] = (report['insns_executed'], report['stop_kind'])
    if len(set(results.values())) > 1:
        seen = ', '.join(f'{tier} {insns:,} ({stop})' for tier, (insns, stop) in results.items())
        problems.append(f'tiers disagree: {seen}; every tier must execute and count the same guest instructions')
    insns, stop = next(iter(results.values()), (None, None))
    return insns, stop, wall, problems


def check(binary, data, only, update):
    tiers = data['tiers']
    rows, failures, notices = [], [], []
    for name, spec in data['workloads'].items():
        if only and name not in only:
            continue
        fixture = ROOT / spec['command'][1]
        if sha256(fixture) != spec['sha256']:
            if not update:
                failures.append(f'{name}: {spec["command"][1]} no longer matches the recorded SHA-256. A rebuilt '
                                'fixture executes different code, so re-record its baseline with --update.')
                continue
            spec['sha256'] = sha256(fixture)
        insns, stop, wall, problems = measure(binary, spec, tiers)
        if insns is not None and stop != spec['stop_kind']:
            problems.append(f'stopped with {stop}, recorded {spec["stop_kind"]}: an instruction count from a '
                            'different stop is not comparable')
        if problems:
            failures.extend(f'{name}: {p}' for p in problems)
            rows.append((name, stop or '-', insns, spec['budget'], spec['baseline'], wall))
            continue
        if update:
            spec['baseline'] = insns
            spec['budget'] = budget_for(insns, data['headroom_percent'])
        elif insns > spec['budget']:
            failures.append(
                f'{name}: {insns:,} guest instructions is over its budget of {spec["budget"]:,} '
                f'({percent(insns, spec["baseline"])} vs the baseline of {spec["baseline"]:,}). The same committed '
                'program now does more work before it stops. If the increase is intended, run '
                '`python3 scripts/insn_budgets.py --update` and say why in the pull request.')
        elif insns < spec['baseline']:
            notices.append(f'{name}: {insns:,} guest instructions, below its baseline of {spec["baseline"]:,} '
                           f'({percent(insns, spec["baseline"])}). Run --update to lock the improvement in.')
        rows.append((name, stop, insns, spec['budget'], spec['baseline'], wall))
    return rows, failures, notices


def percent(value, baseline):
    return f'{(value - baseline) / baseline * 100:+.1f}%' if baseline else 'n/a'


def table(rows, tiers):
    lines = [f'Tiers: {", ".join(tiers)}. Wall time is advisory and never fails the check.', '',
             '| Workload | Stop | Guest instructions | Budget | vs baseline | Wall time (advisory) |',
             '|---|---|---:|---:|---:|---:|']
    for name, stop, insns, budget, baseline, wall in rows:
        count = f'{insns:,}' if insns is not None else '-'
        change = percent(insns, baseline) if insns is not None else '-'
        lines.append(f'| `{name}` | {stop} | {count} | {budget:,} | {change} | {wall:.2f} s |')
    return '\n'.join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n', 1)[0])
    parser.add_argument('--binary', type=Path, default=default_binary(), help='ounce-core build to measure')
    parser.add_argument('--budgets', type=Path, default=DEFAULT_BUDGETS, help='budget file (default: %(default)s)')
    parser.add_argument('--only', action='append', default=[], metavar='NAME', help='check only this workload')
    parser.add_argument('--summary', type=Path, help='append the markdown table to this file (GITHUB_STEP_SUMMARY)')
    parser.add_argument('--update', action='store_true',
                        help='record measured counts as baselines and recompute budgets instead of checking')
    args = parser.parse_args(argv)

    data = json.loads(args.budgets.read_text())
    if data.get('schema') != SCHEMA:
        parser.error(f'{args.budgets}: expected schema {SCHEMA}')
    unknown = set(args.only) - set(data['workloads'])
    if unknown:
        parser.error('unknown workload: ' + ', '.join(sorted(unknown)))
    if not args.binary.exists():
        parser.error(f'{args.binary} does not exist; build it with `cargo build --release` or pass --binary')

    rows, failures, notices = check(args.binary, data, set(args.only), args.update)
    report = table(rows, data['tiers'])
    print(report)
    if args.summary:
        with args.summary.open('a') as summary:
            summary.write('### Instruction budgets (deterministic `insns_executed`, #479)\n\n' + report + '\n\n')
            for line in failures:
                summary.write(f'- **Over budget or not comparable:** {line}\n')
    annotate = os.environ.get('GITHUB_ACTIONS') == 'true'
    for line in notices:
        print(f'::notice title=Instruction budget::{line}' if annotate else f'note: {line}')
    if failures:
        print()
        for line in failures:
            print(f'::error title=Instruction budget::{line}' if annotate else f'FAIL {line}')
        return 1
    if args.update:
        args.budgets.write_text(json.dumps(data, indent=2) + '\n')
        print(f'\nRecorded {len(rows)} baselines in {args.budgets}.')
    else:
        print(f'\nAll {len(rows)} workloads within budget on every tier.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
