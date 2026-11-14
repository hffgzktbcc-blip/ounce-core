#!/usr/bin/env python3
"""Run an existing pinned library harness (default LZ4) as a static Linux ARM64 ELF, without a console SDK.

Requires a host C compiler, a static AArch64 Linux C toolchain (for example
--elf-cc 'zig cc -target aarch64-linux-musl'), and ounce-core (native-jit is required only when selecting Tier-4).
Writes source/binary identities, commands, stdout/stderr and report.json to a NEW
work directory. A byte match with a failing exit or engine fallback is not a pass.
"""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys

from verify_differential import REPO, VERIFY, fetch_upstream, upstream_complete

OPT_CHOICES = ('O1', 'O2', 'O3', 'O3_M1')


def opt_flags(opt):
    """Map an optimization tier to (host_flags, elf_flags).

    O3_M1 passes -mcpu=apple_m1 ONLY to the AArch64 ELF compiler; host compilers
    on x86-64 Linux/Windows CI reject Apple M1 target CPUs.
    """
    if opt == 'O1':
        return (['-O1'], ['-O1'])
    if opt == 'O2':
        return (['-O2'], ['-O2'])
    if opt == 'O3':
        return (['-O3'], ['-O3'])
    if opt == 'O3_M1':
        return (['-O3'], ['-O3', '-mcpu=apple_m1'])
    raise ValueError(f'unknown opt level {opt!r}')


def normalize_elf_build_flags(build_flags, elf_opt):
    """Adapt manifest cflags (e.g. -march=armv8-a+crypto) for zig cc ELF builds."""
    has_mcpu = any(f.startswith('-mcpu=') for f in elf_opt)
    out = []
    for flag in build_flags:
        if flag == '-march=armv8-a+crypto':
            if not has_mcpu:
                out.append('-mcpu=generic+crypto')
        else:
            out.append(flag)
    return out


def parse_opt_matrix(opt, opt_matrix):
    """Parse --opt / --opt-matrix into an ordered, deduplicated list of optimization levels."""
    if opt_matrix is None:
        if opt not in OPT_CHOICES:
            raise ValueError(f'unknown opt level {opt!r}')
        return [opt]
    levels = []
    for raw in opt_matrix.split(','):
        item = raw.strip()
        if not item:
            continue
        if item not in OPT_CHOICES:
            raise ValueError(f'unknown opt level {item!r} (expected one of {", ".join(OPT_CHOICES)})')
        if item not in levels:
            levels.append(item)
    if not levels:
        raise ValueError('--opt-matrix must specify at least one optimization level')
    return levels


def execution_verdict(expected, actual, returncode, stderr, engine):
    if returncode != 0:
        return 'execution-failed'
    selected = {'interpreter': 'Interpreter', 'tier3': 'Tier-3', 'tier4': 'Tier-4'}[engine]
    summaries = re.findall(r'^\[run-elf\] engine=(\S+) instructions=\d+ syscalls=\d+$', stderr, re.M)
    if summaries != [selected]:
        return 'engine-not-confirmed'
    return 'identical' if expected == actual else 'different'


def _normalize_diverge_engine(name):
    if not isinstance(name, str):
        return None
    low = name.strip().lower().replace('-', '')
    if low in ('interpreter', 'interp', 'tier1'):
        return 'interpreter'
    if low in ('tier3', '3'):
        return 'tier3'
    if low in ('tier4', '4'):
        return 'tier4'
    return low


def diverge_verdict(returncode, diverge_report, expected_engines=('interpreter', 'tier3', 'tier4')):
    """Evaluate `ounce-core diverge <elf> --json ...` into a verification verdict."""
    if isinstance(diverge_report, (str, bytes)):
        try:
            diverge_report = json.loads(diverge_report)
        except (ValueError, TypeError):
            diverge_report = None
    if isinstance(diverge_report, dict):
        mismatched = diverge_report.get('mismatched') or 0
        mismatches = diverge_report.get('mismatches') or []
        if returncode == 1 or mismatched > 0 or len(mismatches) > 0:
            return 'different'
        if returncode != 0:
            return 'execution-failed'
        if diverge_report.get('mode') != 'elf':
            return 'execution-failed'
        engines = {}
        for entry in diverge_report.get('engines') or []:
            key = _normalize_diverge_engine(entry.get('engine'))
            if key:
                engines[key] = entry
        for exp in expected_engines:
            if exp not in engines:
                return 'engine-not-confirmed'
        for exp in expected_engines:
            entry = engines[exp]
            if entry.get('exit_code') != 0 or entry.get('stop') != 'exited':
                return 'execution-failed'
        if (diverge_report.get('checked') or 0) < max(1, len(expected_engines) - 1):
            return 'engine-not-confirmed'
        return 'identical'
    return 'execution-failed' if returncode != 0 else 'engine-not-confirmed'


def uses_qemu_reference(harness, machine):
    policy = harness.get('elf_reference')
    return policy == 'qemu' or (policy == 'aarch64' and machine.lower() not in ('arm64', 'aarch64'))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def resolve_upstream(h, work, upstream_dir):
    up = h.get('upstream')
    if not up:
        return None
    if upstream_dir:
        root = upstream_dir.resolve()
        if not upstream_complete(up, root):
            raise RuntimeError('upstream checkout is missing files or is not at the manifest commit')
        dirty = subprocess.check_output(['git', '-C', str(root), 'status', '--porcelain', '--untracked-files=no'])
        if dirty:
            raise RuntimeError('upstream checkout has tracked modifications; use the clean pinned source')
        return root
    cache = work / 'upstream'
    cache.mkdir(parents=True, exist_ok=True)
    return fetch_upstream(up, cache)


def run_single_opt(a, h, compilers, ounce, work, opt, upstream_root=None):
    host_opt, elf_opt = opt_flags(opt)
    report = {
        'schema': 'ounce-core/linux-library-v1',
        'harness': a.harness,
        'opt': opt,
        'failed': True,
        'steps': [],
        'checks': {},
    }

    def step(name, argv):
        entry = {'name': name, 'argv': [str(x) for x in argv], 'timeout_seconds': a.seconds}
        report['steps'].append(entry)
        try:
            r = subprocess.run(entry['argv'], cwd=work, capture_output=True, timeout=a.seconds)
            entry['returncode'] = r.returncode
            out, err = r.stdout, r.stderr
        except subprocess.TimeoutExpired as e:
            out, err = e.stdout or b'', e.stderr or b''
            entry['timeout'] = True
            r = None
        for kind, data in [('stdout', out), ('stderr', err)]:
            path = work / f'{name}.{kind}'
            path.write_bytes(data)
            entry[kind] = {'path': path.name, 'sha256': digest(path)}
        if r is None:
            raise RuntimeError(f'{name} timed out; inspect {name}.stderr')
        return r

    try:
        up = h.get('upstream')
        report['upstream'] = up
        root = upstream_root if upstream_root is not None else resolve_upstream(h, work, a.upstream_dir)
        sources = work / 'sources'
        sources.mkdir()
        pairs = [(VERIFY / h['dir'] / f, Path(f).name) for f in h['files'] + [h['host_main']]]
        if up and root is not None:
            pairs += [(root / src, dest) for src, dest in up['copy'].items()]
        for src, name in pairs:
            dest = sources / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(src, dest)
        report['sources_sha256'] = {
            str(p.relative_to(sources)): digest(p)
            for p in sorted(sources.rglob('*'))
            if p.is_file()
        }
        report['runtime_sha256'] = digest(ounce)
        report['revision'] = subprocess.check_output(['git', '-C', str(REPO), 'rev-parse', 'HEAD'], text=True).strip()
        report['working_tree_dirty'] = bool(subprocess.check_output(['git', '-C', str(REPO), 'status', '--porcelain']))
        for label, compiler in zip(('host-compiler', 'elf-compiler'), compilers):
            r = step(label, compiler + ['--version'])
            if r.returncode:
                raise RuntimeError(f'{label} --version failed')
        elf = f'{a.harness}.elf'
        libs = shlex.split(h.get('libs', ''))
        build_flags = [f'-D{d}' for d in h.get('defines', [])] + shlex.split(h.get('cflags', ''))
        host_build_flags = build_flags
        elf_build_flags = normalize_elf_build_flags(build_flags, elf_opt)
        cfiles = sorted(str(p.relative_to(work)) for p in sources.glob('*.c') if p.name != 'main_nx.c')
        for name, compiler, opt_fl, flags, extra in [
            ('host', compilers[0], host_opt, host_build_flags, []),
            ('elf', compilers[1], elf_opt, elf_build_flags, ['-static', '-no-pie']),
        ]:
            output = 'host-ref.exe' if name == 'host' else elf
            r = step(name + '-build', compiler + [*opt_fl, '-Isources', *flags, *extra, *cfiles, '-o', output, *libs])
            if r.returncode:
                raise RuntimeError(f'{name} build failed; inspect {name}-build.stderr')
            report[name + '_sha256'] = digest(work / output)
        reference = step('host-run', [str(work / 'host-ref.exe')])
        if reference.returncode:
            raise RuntimeError('host reference returned failure; inspect host-run.stderr')
        expected = reference.stdout
        qemu_reference = uses_qemu_reference(h, platform.machine())
        if a.qemu:
            q = step('qemu', shlex.split(a.qemu) + [str(work / elf)])
            report['checks']['qemu-vs-host'] = 'identical' if q.returncode == 0 and q.stdout == reference.stdout else 'different'
            if qemu_reference:
                if q.returncode:
                    raise RuntimeError('qemu reference returned failure; inspect qemu.stderr')
                expected = q.stdout
                report['reference'] = 'qemu'
                report['checks'].pop('qemu-vs-host')
                report['host_vs_qemu_note'] = 'host and qemu outputs ' + ('match' if q.stdout == reference.stdout else 'differ (architecture-dependent results; engines compared with qemu)')
        elif qemu_reference:
            raise RuntimeError('this harness is compared with qemu-user output; pass --qemu')
        for engine in a.engines:
            r = step(engine, [str(ounce), 'run-elf', str(work / elf), f'--cpu={engine}', f'--max-insns={a.budget}', '--trace-syscalls'])
            report['checks'][engine] = execution_verdict(expected, r.stdout, r.returncode,
                                                         r.stderr.decode('utf-8', 'replace'), engine)
        if a.diverge:
            div_json_path = work / 'diverge.json'
            r = step('diverge', [str(ounce), 'diverge', str(work / elf), '--budget', str(a.budget), '--json', str(div_json_path)])
            div_data = div_json_path.read_text() if div_json_path.is_file() else None
            report['checks']['diverge'] = diverge_verdict(r.returncode, div_data)
        report['failed'] = any(v != 'identical' for v in report['checks'].values())
    except (OSError, RuntimeError, subprocess.SubprocessError, SystemExit) as e:
        report['error'] = str(e)
    finally:
        (work / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--work', required=True, type=Path, help='new evidence directory; never overwrites a run')
    ap.add_argument('--elf-cc', required=True, help='compiler command, split as arguments; never run through a shell')
    ap.add_argument('--host-cc', default='cc')
    ap.add_argument('--ounce', type=Path, default=REPO / 'target/release' / ('ounce-core.exe' if sys.platform == 'win32' else 'ounce-core'))
    ap.add_argument('--harness', default='lz4', help='harness name in docs/verify/differential.json')
    ap.add_argument('--upstream-dir', type=Path, help='optional clean checkout at the manifest commit')
    ap.add_argument('--engines', nargs='+', choices=['interpreter', 'tier3', 'tier4'], default=['tier3', 'tier4'])
    ap.add_argument('--opt', choices=OPT_CHOICES, default='O1', help='optimization level for host and ELF builds (default: O1)')
    ap.add_argument('--opt-matrix', help='comma-separated optimization levels (e.g. O1,O2,O3_M1); runs each in <work>/<opt>/ and aggregates <work>/report.json')
    ap.add_argument('--diverge', action='store_true', help='also run ounce-core diverge on the built ELF across interpreter, tier3, and tier4')
    ap.add_argument('--seconds', type=int, default=300, help='deadline per build/execution step')
    ap.add_argument('--budget', type=int, default=100_000_000)
    ap.add_argument('--qemu', help='optional qemu-user command (for example qemu-aarch64): run the same ELF as an independent AArch64 reference. '
                    'A harness with "elf_reference": "qemu" (or "aarch64" on a non-ARM64 host) is compared against qemu output instead of the host build '
                    '(for results that legitimately differ between x86-64 and AArch64, such as the sign of a default NaN)')
    a = ap.parse_args()
    if a.seconds <= 0 or a.budget <= 0:
        ap.error('--seconds and --budget must be positive')
    try:
        levels = parse_opt_matrix(a.opt, a.opt_matrix)
    except ValueError as e:
        ap.error(str(e))
    compilers = [shlex.split(a.host_cc), shlex.split(a.elf_cc)]
    if not all(compilers):
        ap.error('compiler commands must not be empty')
    work = a.work.resolve()
    try:
        work.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        ap.error('--work already exists; choose a new directory to preserve prior evidence')

    h = next((h for h in json.loads((VERIFY / 'differential.json').read_text())['harnesses']
              if h['name'] == a.harness), None)
    if h is None:
        report = {'schema': 'ounce-core/linux-library-v1', 'harness': a.harness, 'failed': True, 'steps': [], 'checks': {},
                  'error': f'unknown harness {a.harness!r} in differential.json'}
        (work / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'failed': True, 'checks': {}, 'error': report['error'], 'report': str(work / 'report.json')}))
        return 1

    ounce = a.ounce.resolve()
    if a.opt_matrix is None:
        report = run_single_opt(a, h, compilers, ounce, work, levels[0])
        print(json.dumps({'failed': report['failed'], 'checks': report['checks'], 'error': report.get('error'),
                          'report': str(work / 'report.json')}))
        return 1 if report['failed'] else 0

    report = {
        'schema': 'ounce-core/linux-library-matrix-v1',
        'harness': a.harness,
        'opt_matrix': levels,
        'failed': True,
        'runs': {},
        'checks': {},
    }
    try:
        upstream_root = resolve_upstream(h, work, a.upstream_dir)
        for opt in levels:
            sub_work = work / opt
            sub_work.mkdir()
            sub = run_single_opt(a, h, compilers, ounce, sub_work, opt, upstream_root=upstream_root)
            report['runs'][opt] = {
                'failed': sub['failed'],
                'checks': sub['checks'],
                'host_sha256': sub.get('host_sha256'),
                'elf_sha256': sub.get('elf_sha256'),
                'error': sub.get('error'),
                'report': str(sub_work / 'report.json'),
            }
            for k, v in sub['checks'].items():
                report['checks'][f'{opt}:{k}'] = v
        report['failed'] = any(sub['failed'] for sub in report['runs'].values())
    except (OSError, RuntimeError, subprocess.SubprocessError, SystemExit) as e:
        report['error'] = str(e)
    finally:
        (work / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'failed': report['failed'], 'checks': report['checks'], 'error': report.get('error'),
                      'report': str(work / 'report.json')}))
    return 1 if report['failed'] else 0


if __name__ == '__main__':
    sys.exit(main())
