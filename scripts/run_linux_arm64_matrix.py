#!/usr/bin/env python3
"""Run scripts/verify_linux_elf.py across the Linux ELF harnesses in docs/verify/differential.json.

Intended primarily for native Linux ARM64 hosts (e.g. ubuntu-24.04-arm in CI),
writing an aggregated JSON report and exiting non-zero if any row fails.
"""
import argparse
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys

from verify_differential import REPO, VERIFY
from verify_linux_elf import parse_opt_matrix, uses_qemu_reference

HEAVY_HARNESSES = frozenset({
    'lodepng',
    'lua',
    'monocypher',
    'quickjs',
    'sqlite',
    'stb-image',
    'wasm3',
    'zlib',
})

DEFAULT_HOST_CC = 'cc'
DEFAULT_ELF_CC = 'python3 -m ziglang cc -target aarch64-linux-musl'
DEFAULT_FAST_OPT_MATRIX = 'O1,O2,O3_M1'
DEFAULT_HEAVY_OPT_MATRIX = 'O1,O2'
DEFAULT_FAST_BUDGET = 100_000_000
DEFAULT_HEAVY_BUDGET = 4_000_000_000
DEFAULT_SECONDS = 600


def is_linux_elf_harness(harness):
    """Return True if a differential.json harness entry targets the Linux ELF matrix."""
    if harness.get('nx_only') or not harness.get('host_main'):
        return False
    return bool(
        harness.get('linux_elf_only')
        or harness.get('upstream')
        or harness.get('name') == 'pmull'
    )


def select_harnesses(manifest, only=None):
    """Select Linux ELF harnesses from a loaded differential.json manifest."""
    selected = [h for h in manifest.get('harnesses', []) if is_linux_elf_harness(h)]
    if only is None:
        return selected
    by_name = {h['name']: h for h in selected}
    unknown = [name for name in only if name not in by_name]
    if unknown:
        raise ValueError(f'unknown or non-Linux-ELF harness(es): {", ".join(unknown)}')
    wanted = set(only)
    return [h for h in selected if h['name'] in wanted]


def harness_profile(
    harness_name,
    fast_opt_matrix=DEFAULT_FAST_OPT_MATRIX,
    heavy_opt_matrix=DEFAULT_HEAVY_OPT_MATRIX,
    fast_budget=DEFAULT_FAST_BUDGET,
    heavy_budget=DEFAULT_HEAVY_BUDGET,
    heavy_harnesses=HEAVY_HARNESSES,
):
    """Return optimization matrix and instruction budget for a harness."""
    is_heavy = harness_name in heavy_harnesses
    opt_matrix = heavy_opt_matrix if is_heavy else fast_opt_matrix
    levels = parse_opt_matrix('O1', opt_matrix)
    return {
        'heavy': is_heavy,
        'opt_matrix': ','.join(levels),
        'levels': levels,
        'budget': heavy_budget if is_heavy else fast_budget,
    }


def build_verify_cmd(
    harness,
    work_dir,
    *,
    ounce,
    host_cc=DEFAULT_HOST_CC,
    elf_cc=DEFAULT_ELF_CC,
    profile,
    diverge=True,
    seconds=DEFAULT_SECONDS,
    machine=None,
    qemu=None,
    verify_script=None,
):
    """Build the command-line invocation of scripts/verify_linux_elf.py for one harness."""
    script = Path(verify_script) if verify_script is not None else Path(__file__).resolve().with_name('verify_linux_elf.py')
    cmd = [
        sys.executable,
        str(script),
        '--harness', harness['name'],
        '--work', str(work_dir),
        '--ounce', str(ounce),
        '--host-cc', host_cc,
        '--elf-cc', elf_cc,
        '--opt-matrix', profile['opt_matrix'],
        '--budget', str(profile['budget']),
        '--seconds', str(seconds),
    ]
    if diverge:
        cmd.append('--diverge')
    if machine is None:
        machine = platform.machine()
    if uses_qemu_reference(harness, machine):
        qemu_cmd = qemu if qemu is not None else shutil.which('qemu-aarch64')
        if qemu_cmd:
            cmd.extend(['--qemu', qemu_cmd])
    elif qemu is not None:
        cmd.extend(['--qemu', qemu])
    return cmd


def aggregate_harness_result(harness_name, profile, sub_work, returncode, sub_report):
    """Build per-harness summary and row entries from a verify_linux_elf.py run."""
    rows = []
    if isinstance(sub_report, dict) and isinstance(sub_report.get('runs'), dict):
        runs = sub_report['runs']
        for opt in profile['levels']:
            run_entry = runs.get(opt)
            if isinstance(run_entry, dict):
                row_failed = bool(run_entry.get('failed', True))
                rows.append({
                    'harness': harness_name,
                    'opt': opt,
                    'heavy': profile['heavy'],
                    'budget': profile['budget'],
                    'failed': row_failed,
                    'checks': run_entry.get('checks', {}),
                    'host_sha256': run_entry.get('host_sha256'),
                    'elf_sha256': run_entry.get('elf_sha256'),
                    'error': run_entry.get('error'),
                    'report': run_entry.get('report'),
                })
            else:
                rows.append({
                    'harness': harness_name,
                    'opt': opt,
                    'heavy': profile['heavy'],
                    'budget': profile['budget'],
                    'failed': True,
                    'checks': {},
                    'error': f'missing run entry for opt {opt}',
                    'report': str(sub_work / 'report.json'),
                })
        harness_failed = returncode != 0 or bool(sub_report.get('failed', True)) or any(r['failed'] for r in rows)
        error = sub_report.get('error')
    else:
        harness_failed = True
        error = f'missing or invalid report.json (exit {returncode})'
        for opt in profile['levels']:
            rows.append({
                'harness': harness_name,
                'opt': opt,
                'heavy': profile['heavy'],
                'budget': profile['budget'],
                'failed': True,
                'checks': {},
                'error': error,
                'report': str(sub_work / 'report.json'),
            })

    summary = {
        'harness': harness_name,
        'heavy': profile['heavy'],
        'opt_matrix': profile['levels'],
        'budget': profile['budget'],
        'returncode': returncode,
        'failed': harness_failed,
        'error': error,
        'report': str(sub_work / 'report.json'),
        'checks': sub_report.get('checks', {}) if isinstance(sub_report, dict) else {},
    }
    return summary, rows


def run_matrix(
    *,
    work,
    manifest_path=None,
    ounce=None,
    host_cc=DEFAULT_HOST_CC,
    elf_cc=DEFAULT_ELF_CC,
    fast_opt_matrix=DEFAULT_FAST_OPT_MATRIX,
    heavy_opt_matrix=DEFAULT_HEAVY_OPT_MATRIX,
    fast_budget=DEFAULT_FAST_BUDGET,
    heavy_budget=DEFAULT_HEAVY_BUDGET,
    diverge=True,
    seconds=DEFAULT_SECONDS,
    qemu=None,
    only=None,
    machine=None,
    runner=None,
):
    """Execute the matrix across selected Linux ELF harnesses and aggregate results."""
    manifest_file = Path(manifest_path) if manifest_path is not None else VERIFY / 'differential.json'
    manifest = json.loads(manifest_file.read_text(encoding='utf-8'))
    harnesses = select_harnesses(manifest, only=only)
    if not harnesses:
        raise ValueError('no Linux ELF harnesses selected')

    ounce_bin = Path(ounce) if ounce is not None else REPO / 'target/release' / ('ounce-core.exe' if sys.platform == 'win32' else 'ounce-core')
    ounce_bin = ounce_bin.resolve()
    host_machine = machine if machine is not None else platform.machine()
    exec_fn = runner if runner is not None else (lambda cmd, _h, _w: subprocess.run(cmd, check=False))

    harness_summaries = {}
    all_rows = []
    for h in harnesses:
        name = h['name']
        profile = harness_profile(
            name,
            fast_opt_matrix=fast_opt_matrix,
            heavy_opt_matrix=heavy_opt_matrix,
            fast_budget=fast_budget,
            heavy_budget=heavy_budget,
        )
        sub_work = work / name
        cmd = build_verify_cmd(
            h,
            sub_work,
            ounce=ounce_bin,
            host_cc=host_cc,
            elf_cc=elf_cc,
            profile=profile,
            diverge=diverge,
            seconds=seconds,
            machine=host_machine,
            qemu=qemu,
        )
        res = exec_fn(cmd, h, sub_work)
        returncode = getattr(res, 'returncode', res)
        sub_report_path = sub_work / 'report.json'
        sub_report = None
        if sub_report_path.is_file():
            try:
                sub_report = json.loads(sub_report_path.read_text(encoding='utf-8'))
            except (OSError, ValueError):
                sub_report = None
        summary, rows = aggregate_harness_result(name, profile, sub_work, returncode, sub_report)
        harness_summaries[name] = summary
        all_rows.extend(rows)

    failed_rows = sum(1 for r in all_rows if r['failed'])
    passed_rows = len(all_rows) - failed_rows
    failed_harnesses = sum(1 for s in harness_summaries.values() if s['failed'])
    passed_harnesses = len(harness_summaries) - failed_harnesses
    overall_failed = failed_rows > 0 or failed_harnesses > 0 or len(all_rows) == 0

    return {
        'schema': 'ounce-core/linux-arm64-matrix-v1',
        'machine': host_machine,
        'host_cc': host_cc,
        'elf_cc': elf_cc,
        'diverge': diverge,
        'fast_opt_matrix': parse_opt_matrix('O1', fast_opt_matrix),
        'heavy_opt_matrix': parse_opt_matrix('O1', heavy_opt_matrix),
        'fast_budget': fast_budget,
        'heavy_budget': heavy_budget,
        'total_harnesses': len(harness_summaries),
        'passed_harnesses': passed_harnesses,
        'failed_harnesses': failed_harnesses,
        'total_rows': len(all_rows),
        'passed_rows': passed_rows,
        'failed_rows': failed_rows,
        'failed': overall_failed,
        'harnesses': harness_summaries,
        'rows': all_rows,
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--work', required=True, type=Path, help='new evidence directory; never overwrites a run')
    ap.add_argument('--report', type=Path, help='optional extra path to write the aggregated JSON report')
    ap.add_argument('--manifest', type=Path, default=VERIFY / 'differential.json')
    ap.add_argument('--ounce', type=Path, default=REPO / 'target/release' / ('ounce-core.exe' if sys.platform == 'win32' else 'ounce-core'))
    ap.add_argument('--host-cc', default=DEFAULT_HOST_CC)
    ap.add_argument('--elf-cc', default=DEFAULT_ELF_CC)
    ap.add_argument('--fast-opt-matrix', default=DEFAULT_FAST_OPT_MATRIX)
    ap.add_argument('--heavy-opt-matrix', default=DEFAULT_HEAVY_OPT_MATRIX)
    ap.add_argument('--fast-budget', type=int, default=DEFAULT_FAST_BUDGET)
    ap.add_argument('--heavy-budget', type=int, default=DEFAULT_HEAVY_BUDGET)
    ap.add_argument('--seconds', type=int, default=DEFAULT_SECONDS)
    ap.add_argument('--qemu', help='optional qemu-user command for harnesses requiring a qemu reference')
    ap.add_argument('--only', nargs='+', help='optional subset of harness names to run')
    ap.add_argument('--no-diverge', action='store_true', help='skip ounce-core diverge checks')
    a = ap.parse_args(argv)

    if a.seconds <= 0 or a.fast_budget <= 0 or a.heavy_budget <= 0:
        ap.error('--seconds, --fast-budget, and --heavy-budget must be positive')

    work = a.work.resolve()
    try:
        work.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        ap.error('--work already exists; choose a new directory to preserve prior evidence')

    report = run_matrix(
        work=work,
        manifest_path=a.manifest,
        ounce=a.ounce,
        host_cc=a.host_cc,
        elf_cc=a.elf_cc,
        fast_opt_matrix=a.fast_opt_matrix,
        heavy_opt_matrix=a.heavy_opt_matrix,
        fast_budget=a.fast_budget,
        heavy_budget=a.heavy_budget,
        diverge=not a.no_diverge,
        seconds=a.seconds,
        qemu=a.qemu,
        only=a.only,
    )

    report_text = json.dumps(report, indent=2) + '\n'
    report_path = work / 'report.json'
    report_path.write_text(report_text, encoding='utf-8')
    if a.report is not None:
        extra_report = a.report.resolve()
        extra_report.parent.mkdir(parents=True, exist_ok=True)
        extra_report.write_text(report_text, encoding='utf-8')

    print(json.dumps({
        'failed': report['failed'],
        'total_harnesses': report['total_harnesses'],
        'failed_harnesses': report['failed_harnesses'],
        'total_rows': report['total_rows'],
        'failed_rows': report['failed_rows'],
        'report': str(report_path),
    }))
    return 1 if report['failed'] else 0


if __name__ == '__main__':
    sys.exit(main())
