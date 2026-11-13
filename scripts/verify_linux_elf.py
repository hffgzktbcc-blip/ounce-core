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
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys

from verify_differential import REPO, VERIFY, fetch_upstream, upstream_complete


def execution_verdict(expected, actual, returncode, stderr, engine):
    if returncode != 0:
        return 'execution-failed'
    selected = {'interpreter': 'Interpreter', 'tier3': 'Tier-3', 'tier4': 'Tier-4'}[engine]
    summaries = re.findall(r'^\[run-elf\] engine=(\S+) instructions=\d+ syscalls=\d+$', stderr, re.M)
    if summaries != [selected]:
        return 'engine-not-confirmed'
    return 'identical' if expected == actual else 'different'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--work', required=True, type=Path, help='new evidence directory; never overwrites a run')
    ap.add_argument('--elf-cc', required=True, help='compiler command, split as arguments; never run through a shell')
    ap.add_argument('--host-cc', default='cc')
    ap.add_argument('--ounce', type=Path, default=REPO / 'target/release' / ('ounce-core.exe' if sys.platform == 'win32' else 'ounce-core'))
    ap.add_argument('--harness', default='lz4', help='harness name in docs/verify/differential.json')
    ap.add_argument('--upstream-dir', type=Path, help='optional clean checkout at the manifest commit')
    ap.add_argument('--engines', nargs='+', choices=['interpreter', 'tier3', 'tier4'], default=['tier3', 'tier4'])
    ap.add_argument('--seconds', type=int, default=300, help='deadline per build/execution step')
    ap.add_argument('--budget', type=int, default=100_000_000)
    ap.add_argument('--qemu', help='optional qemu-user command (for example qemu-aarch64): run the same ELF as an independent AArch64 reference. '
                    'A harness with "elf_reference": "qemu" in differential.json is compared against qemu output instead of the host build '
                    '(for results that legitimately differ between x86-64 and AArch64, such as the sign of a default NaN)')
    a = ap.parse_args()
    if a.seconds <= 0 or a.budget <= 0:
        ap.error('--seconds and --budget must be positive')
    compilers = [shlex.split(a.host_cc), shlex.split(a.elf_cc)]
    if not all(compilers):
        ap.error('compiler commands must not be empty')
    work = a.work.resolve()
    try:
        work.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        ap.error('--work already exists; choose a new directory to preserve prior evidence')
    report = {'schema': 'ounce-core/linux-library-v1', 'harness': a.harness, 'failed': True, 'steps': [], 'checks': {}}

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
        h = next((h for h in json.loads((VERIFY / 'differential.json').read_text())['harnesses']
                  if h['name'] == a.harness), None)
        if h is None:
            raise RuntimeError(f'unknown harness {a.harness!r} in differential.json')
        report['upstream'] = h['upstream']
        if a.upstream_dir:
            root = a.upstream_dir.resolve()
            if not upstream_complete(h['upstream'], root):
                raise RuntimeError('upstream checkout is missing files or is not at the manifest commit')
            dirty = subprocess.check_output(['git', '-C', str(root), 'status', '--porcelain', '--untracked-files=no'])
            if dirty:
                raise RuntimeError('upstream checkout has tracked modifications; use the clean pinned source')
        else:
            cache = work / 'upstream'
            cache.mkdir()
            root = fetch_upstream(h['upstream'], cache)
        sources = work / 'sources'
        sources.mkdir()
        pairs = [(VERIFY / h['dir'] / f, Path(f).name) for f in h['files'] + [h['host_main']]]
        pairs += [(root / src, dest) for src, dest in h['upstream']['copy'].items()]
        for src, name in pairs:
            shutil.copyfile(src, sources / name)
        report['sources_sha256'] = {p.name: digest(p) for p in sorted(sources.iterdir())}
        ounce = a.ounce.resolve()
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
        cfiles = sorted(str(p.relative_to(work)) for p in sources.glob('*.c') if p.name != 'main_nx.c')
        for name, compiler, extra in [('host', compilers[0], []), ('elf', compilers[1], ['-static', '-no-pie'])]:
            output = 'host-ref.exe' if name == 'host' else elf
            r = step(name + '-build', compiler + ['-O1', '-Isources', *build_flags, *extra, *cfiles, '-o', output, *libs])
            if r.returncode:
                raise RuntimeError(f'{name} build failed; inspect {name}-build.stderr')
            report[name + '_sha256'] = digest(work / output)
        reference = step('host-run', [str(work / 'host-ref.exe')])
        if reference.returncode:
            raise RuntimeError('host reference returned failure; inspect host-run.stderr')
        expected = reference.stdout
        if a.qemu:
            q = step('qemu', shlex.split(a.qemu) + [str(work / elf)])
            report['checks']['qemu-vs-host'] = 'identical' if q.returncode == 0 and q.stdout == reference.stdout else 'different'
            if h.get('elf_reference') == 'qemu':
                if q.returncode:
                    raise RuntimeError('qemu reference returned failure; inspect qemu.stderr')
                expected = q.stdout
                report['reference'] = 'qemu'
                report['checks'].pop('qemu-vs-host')
                report['host_vs_qemu_note'] = 'host and qemu outputs ' + ('match' if q.stdout == reference.stdout else 'differ (architecture-dependent results; engines compared with qemu)')
        elif h.get('elf_reference') == 'qemu':
            raise RuntimeError('this harness is compared with qemu-user output; pass --qemu')
        for engine in a.engines:
            r = step(engine, [str(ounce), 'run-elf', str(work / elf), f'--cpu={engine}', f'--max-insns={a.budget}', '--trace-syscalls'])
            report['checks'][engine] = execution_verdict(expected, r.stdout, r.returncode,
                                                         r.stderr.decode('utf-8', 'replace'), engine)
        report['failed'] = any(v != 'identical' for v in report['checks'].values())
    except (OSError, RuntimeError, subprocess.SubprocessError, SystemExit) as e:
        report['error'] = str(e)
    finally:
        (work / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'failed': report['failed'], 'checks': report['checks'], 'error': report.get('error'),
                      'report': str(work / 'report.json')}))
    return 1 if report['failed'] else 0


if __name__ == '__main__':
    sys.exit(main())
