#!/usr/bin/env python3
"""Optional helpers: inspect locally, save staged work, check, submit a PR."""
import argparse
import os
from pathlib import Path
import re
import subprocess
import sys

from ci_scope import classify, headline_changed

TASK = re.compile(r'^- \[([ x])\] \*\*([A-Za-z0-9_-]+)\. (.+)')
FORBIDDEN = re.compile(r'\.(keys|nsp|xci|nca|nsz|xcz|tik)$|(^|/)dumps/|(^|/)ns2jb', re.I)
BUILD = re.compile(r'(^|/)(target|dist)/|\.(o|rlib|dylib|so|dll)$', re.I)


def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()


def tasks(path=Path('PLAN.md')):
    result = {}
    current = None
    for line in path.read_text().splitlines():
        match = TASK.match(line)
        if match:
            checked, identity, title = match.groups()
            if identity in result:
                raise ValueError(f'duplicate task ID {identity} in {path}; use unique IDs')
            current = {'id': identity, 'title': title.rstrip('*'), 'done': checked == 'x',
                       'status': 'ready', 'depends': [], 'details': []}
            result[identity] = current
        elif line.startswith('#'):
            current = None
        elif current and line.startswith('      '):
            detail = line.strip()
            current['details'].append(detail)
            if detail.startswith('Status:'):
                current['status'] = detail.split(':', 1)[1].strip()
                if current['status'] not in {'ready', 'active', 'blocked', 'parked'}:
                    raise ValueError(f'unknown status for {current["id"]}: {current["status"]}')
            if detail.startswith('Depends:'):
                current['depends'] = detail.split(':', 1)[1].split()
    for task in result.values():
        for dependency in task['depends']:
            if dependency not in result:
                raise ValueError(f'unknown dependency {dependency} for {task["id"]}')
    return result


def ready(task, plan):
    return (not task['done'] and task['status'] in {'ready', 'active'}
            and all(plan[identity]['done'] for identity in task['depends']))


def next_task(args):
    plan = tasks()
    if args.step:
        if args.step not in plan:
            raise ValueError(f'unknown task {args.step}; see PLAN.md')
        selected = plan[args.step]
    else:
        selected = next((task for task in plan.values() if ready(task, plan)), None)
    print('ounce-core — local status (no remote queries)')
    print(f'Branch: {git("branch", "--show-current") or "detached HEAD"}; commit: {git("rev-parse", "--short", "HEAD")}')
    print(git('status', '--short') or 'Worktree clean')
    if selected:
        state = 'done' if selected['done'] else selected['status']
        print(f'\n{selected["id"]}. {selected["title"]} [{state}]')
        for detail in selected['details']:
            print(f'  {detail}')
    else:
        print('\nNo ready task. Record the blocker or propose the next useful result; do not repeat completed work.')
    waiting = [t for t in plan.values() if not t['done'] and not ready(t, plan)]
    if waiting:
        print('Waiting: ' + ', '.join(f'{t["id"]} ({t["status"]}; dependencies: {" ".join(t["depends"]) or "none"})' for t in waiting))
    handoff = subprocess.run([sys.executable, '-B', str(Path(__file__).with_name('handoff.py')), '--offline', 'status', '--limit', '3'],
                             capture_output=True, text=True)
    if handoff.returncode == 0 and ('expires' in handoff.stdout or 'ago' in handoff.stdout):
        print('\n' + handoff.stdout.rstrip())
    print('\nUsage can end at any time: scripts/handoff.sh claim <task>; scripts/handoff.sh watch --push &  (see AGENTS.md)')
    print('Save: git add <owned paths>; scripts/gate.sh --checkpoint "what/why"')
    print('Check: scripts/gate.sh [--check quick|runtime|full]; submit: scripts/gate.sh --submit')


def staged_paths():
    data = subprocess.check_output(['git', 'diff', '--cached', '--name-only', '--diff-filter=ACMR', '-z'])
    return [os.fsdecode(path) for path in data.split(b'\0') if path]


def check_staged():
    # Only proposed commit contents matter. Ignored local inputs are not staged.
    for path in staged_paths():
        if FORBIDDEN.search(path) or BUILD.search(path):
            raise ValueError(f'refusing to commit restricted input or build output: {path}')
    # Reuse the domain policy, inspecting staged additions rather than the worktree.
    policy = Path('scripts/check_legal_guardrails.sh')
    if policy.is_file():
        match = re.search(r"^sites='(.+)'$", policy.read_text(), re.M)
        if match:
            sites = re.compile(match[1], re.I)
            for path in staged_paths():
                if path == str(policy):
                    continue
                data = subprocess.check_output(['git', 'show', f':{path}'])
                if b'\0' not in data and sites.search(data.decode('utf-8', errors='replace')):
                    raise ValueError(f'restricted download-site reference in staged file: {path}')


def branch_for_write():
    if git('branch', '--show-current') in {'', 'main'}:
        raise ValueError('create a working branch first: git switch -c <topic>')


def autosave(note, next_step=None, push=False):
    """Best-effort handoff snapshot (see scripts/handoff.py); never fails the calling command."""
    command = [sys.executable, '-B', str(Path(__file__).with_name('handoff.py')), '--offline', 'save', '--note', note]
    if next_step:
        command += ['--next', next_step]
    try:
        subprocess.run(command, capture_output=True, timeout=60)
        if push:
            subprocess.run([sys.executable, '-B', command[2], 'save', '--note', note, '--push'], capture_output=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        pass


def checkpoint(args):
    branch_for_write()
    if not git('diff', '--cached', '--name-only'):
        raise ValueError('nothing staged; git add <owned paths> first (other work is left alone)')
    check_staged()
    notes = ['Validation: not run by checkpoint; record focused checks in the PR.']
    for label, value in [('Task', args.step), ('Next', args.next), ('Tried', args.tried),
                         ('Decision', args.decision), ('Evidence', args.proof)]:
        if value:
            notes.append(f'{label}: {value}')
    subprocess.run(['git', 'commit', '-s', '-m', f'wip: {args.checkpoint}', '-m', '\n'.join(notes)], check=True)
    print(f'Checkpoint saved locally: {git("rev-parse", "--short", "HEAD")}. Validation not run.')
    autosave(args.checkpoint, args.next, push=args.push)
    if args.push:
        try:
            subprocess.run(['git', 'push', '-u', 'origin', 'HEAD'], check=True)
        except subprocess.CalledProcessError as error:
            raise ValueError('push failed; your checkpoint is saved locally. Retry git push when connected.') from error


def submit(args):
    branch_for_write()
    if git('diff', '--cached', '--name-only'):
        raise ValueError('staged changes are not committed; save them before submitting')
    subprocess.run(['git', 'push', '-u', 'origin', 'HEAD'], check=True)
    existing = subprocess.run(['gh', 'pr', 'view', '--json', 'url', '--jq', '.url'], capture_output=True, text=True)
    if existing.returncode == 0:
        print(existing.stdout.strip())
    else:
        body = ('## Result\n' + (args.proof or 'Describe the user-visible result and link its reproduction evidence.')
                + '\n\n## Validation\nSubmission does not run or certify checks. Record local validation and inspect hosted checks.\n')
        subprocess.run(['gh', 'pr', 'create', '--base', 'main', '--title', git('log', '-1', '--format=%s'), '--body', body], check=True)
    autosave('submitted: ' + git('log', '-1', '--format=%s'), push=True)
    print('PR submitted. No milestone was ticked and no merge was requested.')


def local_scope():
    baseline = subprocess.run(['git', 'rev-parse', '--verify', 'origin/main'], capture_output=True)
    base = git('merge-base', 'origin/main', 'HEAD') if baseline.returncode == 0 else 'HEAD'
    paths = subprocess.check_output(['git', 'diff', '--name-only', '-z', base]).split(b'\0')
    paths += subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard', '-z']).split(b'\0')
    paths = [os.fsdecode(path) for path in paths if path]
    scope = classify(paths)
    if 'README.md' in paths:
        before = subprocess.run(['git', 'show', f'{base}:README.md'], capture_output=True, text=True)
        after = Path('README.md')
        scope['corpus'] |= before.returncode != 0 or not after.is_file() or headline_changed(before.stdout, after.read_text())
    return scope


def check(args):
    mode = args.check or 'auto'
    tasks()
    commands = [
        ['bash', 'scripts/check_legal_guardrails.sh'],
        ['bash', 'scripts/check_unsafe_boundary.sh'],
        [sys.executable, '-m', 'unittest', 'discover', '-s', 'scripts', '-p', 'test_*.py'],
        [sys.executable, '-m', 'unittest', 'discover', '-s', 'tests', '-p', 'test_*.py'],
        ['git', 'diff', '--check'],
    ]
    runtime = mode in {'runtime', 'full'} or (mode == 'auto' and local_scope()['runtime'])
    if runtime:
        commands += [['cargo', 'test', '--release', '--locked'], ['cargo', 'run', '--release', '--locked', '--', 'test']]
    if mode == 'full':
        for feature in ('gpu-present', 'native-jit', 'nce'):
            commands.append(['cargo', 'test', '--release', '--locked', '--features', feature])
    for command in commands:
        print('+ ' + ' '.join(command), flush=True)
        subprocess.run(command, check=True)
    print('Selected local checks passed. Hosted platform/corpus evidence is separate.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    next_parser = sub.add_parser('next', help='read local status and the next ready task')
    next_parser.add_argument('step', nargs='?')
    gate = sub.add_parser('gate', help='validation, staged checkpoint, or PR submission')
    mode = gate.add_mutually_exclusive_group()
    mode.add_argument('--checkpoint', metavar='SUMMARY', help='commit staged files locally; no tests or push')
    mode.add_argument('--submit', '--ship', dest='submit', action='store_true', help='push committed work and open/update a PR; never merge or tick tasks')
    mode.add_argument('--check', choices=('auto', 'quick', 'runtime', 'full'), help='auto checks scope; quick omits Rust; full adds feature tests')
    gate.add_argument('--push', action='store_true', help='also push a local checkpoint')
    gate.add_argument('--step', help='optional task ID recorded in checkpoint message')
    gate.add_argument('--next', help='next action recorded in checkpoint message')
    gate.add_argument('--tried')
    gate.add_argument('--decision')
    gate.add_argument('--proof', help='evidence notes; does not certify or complete a milestone')
    gate.add_argument('--no-oracle', help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        os.chdir(git('rev-parse', '--show-toplevel'))
        if args.command == 'next':
            next_task(args)
        elif args.checkpoint:
            checkpoint(args)
        elif args.submit:
            submit(args)
        else:
            if args.push:
                raise ValueError('--push is only used with --checkpoint')
            check(args)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'workflow: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
