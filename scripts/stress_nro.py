#!/usr/bin/env python3
"""Stress the runtime with NRO programs: run every program on each engine, or fuzz the loader.

  scripts/stress_nro.py samples DIR [--engines tier3,tier4,native]
      run each *.nro in DIR headless with an instruction budget; report vm-errors, panics, hangs
  scripts/stress_nro.py fuzz DIR [--count 300] [--seed 1]
      corrupt copies of the programs (truncate, header bytes, body bytes, zeroed runs) and check the
      loader and CPU fail gracefully: no Rust panic, no host signal, no hang

Build first (`cargo build --release`, add `--features "nce native-jit"` for those engines). The binary
is target/release/ounce-core unless --binary is given. Failing fuzz inputs are saved to --keep.
Native runs the guest on the host CPU with no instruction budget, so interactive programs that wait
for input show as hangs there (reported as 'hang', not as a bug).
"""
import argparse, collections, concurrent.futures as cf, glob, os, random, shutil, subprocess, sys, tempfile, time


def run(binary, nro, cpu, budget, limit):
    work = tempfile.mkdtemp()
    env = dict(os.environ, OUNCE_NRO_BUDGET=str(budget))
    start = time.time()
    try:
        done = subprocess.run([binary, 'run-nro', nro, '--present=headless', f'--sdmc={work}', f'--cpu={cpu}'],
                              capture_output=True, text=True, timeout=limit, env=env)
        out, rc = done.stdout + done.stderr, done.returncode
    except subprocess.TimeoutExpired:
        out, rc = '', None
    shutil.rmtree(work, ignore_errors=True)
    if rc is None:
        tag = 'hang'
    elif rc < 0:
        tag = f'signal{-rc}'
    elif 'panicked' in out:
        tag = 'panic'
    elif '[VM Error]' in out:
        tag = 'vm-error'
    elif 'Execution budget' in out:
        tag = 'budget'
    else:
        tag = 'exit'
    line = next((l for l in out.splitlines() if 'VM Error' in l or 'panicked' in l), '')
    return tag, round(time.time() - start, 1), line[:120]


def samples(args):
    programs = sorted(glob.glob(os.path.join(args.dir, '*.nro')))
    if not programs:
        sys.exit(f'no .nro files in {args.dir}')
    tally = collections.Counter()
    for nro in programs:
        for cpu in args.engines.split(','):
            tag, seconds, line = run(args.binary, nro, cpu, args.budget, args.limit)
            tally[(cpu, tag)] += 1
            if tag in ('vm-error', 'panic', 'hang') or tag.startswith('signal'):
                print(f'{os.path.basename(nro)} [{cpu}] {tag} after {seconds}s {line}')
    print(dict(tally))
    return 1 if any(t in ('vm-error', 'panic') or t.startswith('signal') for _, t in tally) else 0


def mutate(data, kind, rnd):
    buf = bytearray(data)
    if kind == 'trunc':
        return bytes(buf[:rnd.randrange(1, len(buf))])
    if kind == 'head':
        for _ in range(rnd.randint(1, 6)):
            buf[rnd.randrange(0, min(0x100, len(buf)))] = rnd.randrange(256)
    elif kind == 'body':
        for _ in range(rnd.randint(1, 40)):
            buf[rnd.randrange(len(buf))] = rnd.randrange(256)
    else:
        start = rnd.randrange(len(buf))
        length = min(rnd.randint(1, 4096), len(buf) - start)
        buf[start:start + length] = b'\0' * length
    return bytes(buf)


def fuzz(args):
    programs = sorted(glob.glob(os.path.join(args.dir, '*.nro')))
    if not programs:
        sys.exit(f'no .nro files in {args.dir}')
    rnd = random.Random(args.seed)
    jobs = []
    for i in range(args.count):
        src, kind = rnd.choice(programs), rnd.choice(['trunc', 'head', 'body', 'zero'])
        jobs.append((i, src, kind, mutate(open(src, 'rb').read(), kind, rnd)))
    os.makedirs(args.keep, exist_ok=True)

    def one(job):
        i, src, kind, data = job
        work = tempfile.mkdtemp()
        path = os.path.join(work, 'm.nro')
        open(path, 'wb').write(data)
        tag, seconds, line = run(args.binary, path, args.engine, args.budget, args.limit)
        if tag in ('hang', 'panic') or tag.startswith('signal'):
            keep = os.path.join(args.keep, f'fuzz-{args.seed}-{i}.nro')
            open(keep, 'wb').write(data)
            line = f'{line} saved {keep}'
        shutil.rmtree(work, ignore_errors=True)
        return i, os.path.basename(src), kind, tag, line

    with cf.ThreadPoolExecutor(args.jobs) as pool:
        results = list(pool.map(one, jobs))
    print(dict(collections.Counter(r[3] for r in results)))
    bad = [r for r in results if r[3] in ('hang', 'panic') or r[3].startswith('signal')]
    for r in bad:
        print(r)
    return 1 if bad else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--binary', default='target/release/ounce-core')
    sub = parser.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('samples'); p.add_argument('dir'); p.add_argument('--engines', default='tier3,tier4')
    p.add_argument('--budget', type=int, default=3_000_000); p.add_argument('--limit', type=int, default=40)
    p = sub.add_parser('fuzz'); p.add_argument('dir'); p.add_argument('--count', type=int, default=300)
    p.add_argument('--seed', type=int, default=1); p.add_argument('--engine', default='tier3')
    p.add_argument('--budget', type=int, default=500_000); p.add_argument('--limit', type=int, default=30)
    p.add_argument('--jobs', type=int, default=4); p.add_argument('--keep', default='fuzz-failures')
    args = parser.parse_args()
    return {'samples': samples, 'fuzz': fuzz}[args.cmd](args)


if __name__ == '__main__':
    sys.exit(main())
