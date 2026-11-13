#!/usr/bin/env python3
"""Resumable agent handoff: snapshots, expiring task claims, one-command resume.

An agent can run out of usage at any moment, so nothing here needs the agent to
finish anything. Everything is stored as git refs under refs/handoff/, so it never
touches the working tree, the index or any branch:

  save     snapshot the whole working state (staged, unstaged, untracked) as a commit
           reachable only from refs/handoff/<agent>/<branch>; optionally push it
  watch    save every N seconds when something changed (run once in the background)
  claim    reserve a task; claims expire on their own, so a dead agent never blocks work
  release  give a claim up
  log      add a note to the shared journal: decisions and findings every agent should know
  journal  read the shared journal (what other agents learned, newest first)
  digest   short markdown of claims, last-seen agents and the journal, for a phone
  status   list live claims and recent snapshots (local and origin)
  resume   show where each agent stopped; --apply NAME restores a snapshot on a new branch
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timedelta, timezone

NAMESPACE = 'refs/handoff'
MARK = 'handoff-state:'
FORBIDDEN = re.compile(r'\.(keys|nsp|xci|nca|nsz|xcz|tik)$|(^|/)dumps/|(^|/)ns2jb', re.I)
BUILD = re.compile(r'(^|/)(target|dist|node_modules)/|\.(o|rlib|dylib|so|dll)$', re.I)
SAFE = re.compile(r'[^A-Za-z0-9._-]+')


def run(*args, env=None, check=True, input=None):
    result = subprocess.run(['git', *args], capture_output=True, env=env, input=input)
    if check and result.returncode:
        raise ValueError(f'git {" ".join(args[:2])} failed: {result.stderr.decode(errors="replace").strip()}')
    return result


def out(*args, **kw):
    return run(*args, **kw).stdout.decode().strip()


def now():
    return datetime.now(timezone.utc)


def stamp(moment):
    return moment.strftime('%Y-%m-%dT%H:%M:%SZ')


def parse_stamp(text):
    return datetime.strptime(text, '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=timezone.utc)


def agent_name(args=None):
    name = (getattr(args, 'agent', None) or os.environ.get('AGENT_NAME') or os.environ.get('OUNCE_AGENT')
            or os.environ.get('USER') or 'agent')
    return SAFE.sub('-', name).strip('-') or 'agent'


def safe(text):
    return SAFE.sub('-', text).strip('-') or 'x'


def branch_name():
    return out('branch', '--show-current') or 'detached'


def encode(state, subject):
    return f'{subject}\n\n{MARK} {json.dumps(state, sort_keys=True)}\n'


def decode(message):
    for line in message.splitlines():
        if line.startswith(MARK):
            return json.loads(line[len(MARK):])
    return None


def snapshot_tree():
    """Tree of the working directory (tracked + untracked, minus ignored/forbidden/build), via a temp index."""
    with tempfile.TemporaryDirectory() as tmp:
        env = dict(os.environ, GIT_INDEX_FILE=os.path.join(tmp, 'index'))
        run('read-tree', 'HEAD', env=env, check=False)
        listed = out('ls-files', '-z', '--cached', '--others', '--exclude-standard').split('\0')
        wanted = [p for p in listed if p and not FORBIDDEN.search(p) and not BUILD.search(p) and os.path.lexists(p)]
        gone = [p for p in out('ls-files', '-z', '--cached').split('\0') if p and not os.path.lexists(p)]
        if wanted:
            run('update-index', '--add', '-z', '--stdin', env=env, input=('\0'.join(wanted) + '\0').encode())
        if gone:
            run('update-index', '--force-remove', '-z', '--stdin', env=env, input=('\0'.join(gone) + '\0').encode())
        return out('write-tree', env=env), len(wanted), len(gone)


def head_tree():
    return out('rev-parse', 'HEAD^{tree}')


def save(args, quiet=False):
    tree, files, removed = snapshot_tree()
    dirty = tree != head_tree()
    branch = branch_name()
    ref = f'{NAMESPACE}/{agent_name(args)}/{safe(branch)}'
    previous = run('rev-parse', '--verify', '-q', ref, check=False).stdout.decode().strip()
    if previous and out('rev-parse', f'{previous}^{{tree}}') == tree and not args.note and not args.next:
        if not quiet:
            print(f'unchanged since last snapshot ({ref})')
        return ref
    state = {'agent': agent_name(args), 'branch': branch, 'base': out('rev-parse', 'HEAD'), 'time': stamp(now()),
             'dirty': dirty, 'files': files, 'removed': removed, 'note': args.note or '', 'next': args.next or '',
             'task': args.task or ''}
    commit = out('commit-tree', tree, '-p', 'HEAD', '-m', encode(state, f'handoff: {agent_name(args)} on {branch}'))
    run('update-ref', ref, commit)
    if not quiet:
        print(f'saved {ref} @ {commit[:9]} ({files} files{", uncommitted work included" if dirty else ""})')
    if args.push:
        pushed = run('push', '--force', 'origin', f'{ref}:{ref}', check=False)
        if pushed.returncode and not quiet:
            print('push failed; the snapshot is saved locally (retry with --push)', file=sys.stderr)
        elif not quiet:
            print('pushed to origin')
    return ref


def watch(args):
    print(f'watching every {args.interval}s as {agent_name(args)}; Ctrl-C to stop', flush=True)
    try:
        while True:
            save(args, quiet=True)
            time.sleep(args.interval)
    except KeyboardInterrupt:
        pass


def fetch_remote():
    run('fetch', '--quiet', '--force', 'origin', f'+{NAMESPACE}/*:{NAMESPACE}/*', check=False)


def entries(prefix):
    lines = out('for-each-ref', '--format=%(refname) %(objectname)', f'{NAMESPACE}/{prefix}').splitlines()
    found = []
    for line in lines:
        ref, sha = line.split()
        state = decode(out('log', '-1', '--format=%B', sha))
        if state:
            found.append((ref, sha, state))
    return found


def live(state):
    return parse_stamp(state['expires']) > now()


def claim(args, attempt=0):
    task = safe(args.task_id)
    ref = f'{NAMESPACE}/claims/{task}'
    if not args.offline:
        fetch_remote()
    me = agent_name(args)
    existing = run('rev-parse', '--verify', '-q', ref, check=False).stdout.decode().strip()
    if existing:
        state = decode(out('log', '-1', '--format=%B', existing)) or {}
        if state.get('agent') != me and 'expires' in state and live(state):
            raise ValueError(f'{task} is claimed by {state["agent"]} until {state["expires"]} '
                             f'({state.get("note", "")}); take another task or wait for it to expire')
    state = {'agent': me, 'task': task, 'time': stamp(now()), 'expires': stamp(now() + timedelta(hours=args.hours)),
             'note': args.note or '', 'branch': branch_name(), 'local': bool(args.offline)}
    commit = out('commit-tree', head_tree(), '-m', encode(state, f'claim: {task} by {me}'))
    run('update-ref', ref, commit)
    if not args.offline:
        # compare-and-swap against what origin has right now, so two agents claiming at once cannot both win
        remote = out('ls-remote', 'origin', ref, check=False).split()
        pushed = run('push', f'--force-with-lease={ref}:{remote[0] if remote else ""}', 'origin', f'{ref}:{ref}', check=False)
        if pushed.returncode:
            detail = pushed.stderr.decode(errors='replace').strip().splitlines()[-1:] or ['no detail from git']
            if attempt < 3:        # a transient rejection or a race on origin: refetch, recheck ownership, try again
                time.sleep(0.5 * (attempt + 1))
                return claim(args, attempt + 1)
            raise ValueError(f'could not claim {task} after {attempt + 1} tries: {detail[0]} '
                             f'(retry, or use --offline for a local-only claim)')
    print(f'claimed {task} for {args.hours:g}h as {me} (expires {state["expires"]})')


def release(args):
    ref = f'{NAMESPACE}/claims/{safe(args.task_id)}'
    run('update-ref', '-d', ref, check=False)
    if not args.offline:
        run('push', 'origin', f':{ref}', check=False)
    print(f'released {safe(args.task_id)}')


JOURNAL = f'{NAMESPACE}/journal'


def log(args):
    """Append a note to the shared journal: what you decided or learned, for every other agent and session."""
    entry = {'agent': agent_name(args), 'time': stamp(now()), 'branch': branch_name(), 'text': args.text,
             'task': args.task or ''}
    for attempt in range(5):
        if not args.offline:
            fetch_remote()
        tip = run('rev-parse', '--verify', '-q', JOURNAL, check=False).stdout.decode().strip()
        parent = ['-p', tip] if tip else []
        commit = out('commit-tree', head_tree(), *parent, '-m', encode(entry, f'journal: {entry["agent"]}'))
        run('update-ref', JOURNAL, commit)
        if args.offline:
            break
        remote = out('ls-remote', 'origin', JOURNAL, check=False).split()
        pushed = run('push', f'--force-with-lease={JOURNAL}:{remote[0] if remote else ""}', 'origin', f'{JOURNAL}:{JOURNAL}',
                     check=False)
        if pushed.returncode == 0:
            break
        run('update-ref', JOURNAL, tip) if tip else run('update-ref', '-d', JOURNAL)   # lost a race: redo on the new tip
    else:
        raise ValueError('could not publish the journal entry after 5 tries; it is saved locally, retry later')
    print(f'logged: {args.text[:80]}')


def journal_entries(limit):
    lines = run('log', '-n', str(limit), '--format=%B%x00', JOURNAL, check=False).stdout.decode().split('\0')
    return [e for e in (decode(l) for l in lines if l.strip()) if e]


def journal(args):
    if not args.offline:
        fetch_remote()
    entries_ = journal_entries(args.limit)
    if not entries_:
        print('Journal is empty. Add to it with: scripts/handoff.sh log "what you decided or learned"')
    for e in entries_:
        task = f' [{e["task"]}]' if e.get('task') else ''
        print(f'{e["time"]} {e["agent"]}{task}: {e["text"]}')


def digest(args):
    """Short markdown for a phone: live claims, who is working on what, recent journal."""
    if not args.offline:
        fetch_remote()
    lines = ['# Agents right now', '']
    claims = [(st) for _, _, st in entries('claims') if 'expires' in st and live(st)]
    lines.append('**Working on:** ' + ('; '.join(f'{c["task"]} ({c["agent"]}, until {c["expires"][11:16]}Z)' for c in claims) or 'nobody has a live claim'))
    snaps = sorted([st for _, _, st in entries('') if 'expires' not in st], key=lambda st: st['time'], reverse=True)[:5]
    lines += ['', '**Last seen:**']
    lines += [f'- {st["agent"]} on `{st["branch"]}` {age(st["time"])}' + (f' — next: {st["next"]}' if st.get('next') else '') +
              (' (unsaved work saved)' if st.get('dirty') else '') for st in snaps] or ['- nobody yet']
    lines += ['', '**Journal:**']
    lines += [f'- {e["time"][5:16].replace("T", " ")} {e["agent"]}: {e["text"]}' for e in journal_entries(args.limit)] or ['- empty']
    print('\n'.join(lines))


def age(text):
    seconds = int((now() - parse_stamp(text)).total_seconds())
    return f'{seconds // 3600}h{seconds % 3600 // 60:02d}m ago' if seconds >= 3600 else f'{seconds // 60}m ago'


def listing(args):
    if not args.offline:
        fetch_remote()
    claims = [(r, s, st) for r, s, st in entries('claims') if 'expires' in st]
    if not args.offline:
        # a claim released on origin must disappear here too; fetch alone never deletes refs
        remote = {line.split()[1] for line in out('ls-remote', 'origin', f'{NAMESPACE}/claims/*', check=False).splitlines()}
        stale = [(r, s, st) for r, s, st in claims if r not in remote and not st.get('local')]
        for ref, _, _ in stale:
            run('update-ref', '-d', ref, check=False)
        claims = [c for c in claims if c not in stale]
    print('Claims:')
    shown = False
    for ref, sha, state in sorted(claims, key=lambda e: e[2]['time']):
        status = 'live' if live(state) else 'expired (free to take)'
        print(f'  {state["task"]:<18} {state["agent"]:<14} {status}; expires {state["expires"]}  {state.get("note", "")}')
        shown = True
    if not shown:
        print('  none')
    print('Snapshots (newest first):')
    snaps = [(r, s, st) for r, s, st in entries('') if 'expires' not in st]
    snaps.sort(key=lambda e: e[2]['time'], reverse=True)
    for ref, sha, state in snaps[:args.limit]:
        flag = 'uncommitted work' if state.get('dirty') else 'clean'
        print(f'  {ref[len(NAMESPACE) + 1:]:<40} {age(state["time"]):<12} {flag}, {state.get("files", 0)} files')
        if state.get('task') or state.get('note') or state.get('next'):
            print(f'      task={state.get("task", "") or "-"} | note: {state.get("note", "") or "-"} | next: {state.get("next", "") or "-"}')
    if not snaps:
        print('  none')
    return snaps


def resume(args):
    snaps = listing(args)
    if not args.apply:
        if snaps:
            print('\nTo continue where one stopped: scripts/handoff.sh resume --apply <name from the list above>')
        return
    match = [e for e in snaps if e[0][len(NAMESPACE) + 1:] == args.apply]
    if not match:
        raise ValueError(f'no snapshot named {args.apply}; names are listed above')
    if out('status', '--porcelain', '--untracked-files=all'):
        raise ValueError('working tree is not clean; commit or stash first (resume never overwrites local work)')
    ref, sha, state = match[0]
    branch = f'resume/{safe(state["branch"])}-{stamp(now())[:10]}'
    run('switch', '-c', branch, sha)
    run('reset', '--mixed', state['base'])
    print(f'restored {state["agent"]}\'s work as branch {branch}: base {state["base"][:9]}, '
          f'working tree has the unsaved changes. Review with git status, then commit.')
    if state.get('next'):
        print(f'They wrote as next step: {state["next"]}')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--agent', help='name recorded on snapshots and claims (default: $AGENT_NAME or $USER)')
    parser.add_argument('--offline', action='store_true', help='do not contact origin')
    sub = parser.add_subparsers(dest='command', required=True)
    p = sub.add_parser('save', help='snapshot the working state')
    p.add_argument('--note'); p.add_argument('--next'); p.add_argument('--task'); p.add_argument('--push', action='store_true')
    p = sub.add_parser('watch', help='save automatically while you work')
    p.add_argument('--interval', type=int, default=60); p.add_argument('--push', action='store_true')
    p.add_argument('--note'); p.add_argument('--next'); p.add_argument('--task')
    p = sub.add_parser('claim', help='reserve a task; expires on its own')
    p.add_argument('task_id'); p.add_argument('--hours', type=float, default=4); p.add_argument('--note')
    p = sub.add_parser('release', help='give a claim up'); p.add_argument('task_id')
    p = sub.add_parser('log', help='add a note to the shared journal (decisions, findings, warnings)')
    p.add_argument('text'); p.add_argument('--task')
    p = sub.add_parser('journal', help='read the shared journal'); p.add_argument('--limit', type=int, default=20)
    p = sub.add_parser('digest', help='short markdown summary for a phone'); p.add_argument('--limit', type=int, default=8)
    for name in ('status', 'resume'):
        p = sub.add_parser(name, help='list claims and snapshots' if name == 'status' else 'see where agents stopped; --apply restores')
        p.add_argument('--limit', type=int, default=10)
        if name == 'resume':
            p.add_argument('--apply', metavar='NAME')
    args = parser.parse_args(argv)
    try:
        os.chdir(out('rev-parse', '--show-toplevel'))
        {'save': save, 'watch': watch, 'claim': claim, 'release': release,
         'status': listing, 'resume': resume, 'log': log, 'journal': journal, 'digest': digest}[args.command](args)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'handoff: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
