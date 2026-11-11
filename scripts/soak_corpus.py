#!/usr/bin/env python3
"""Soak registered corpus programs well past the first frame, headless, on the interpreter and Tier-3.

Each program runs in its own process (`corpus_soak`, src/loader/corpus_soak.rs) under an outer
timeout, because a blocking guest SVC cannot be interrupted from inside the runner. A program is a
finding when either engine crashes or hangs, host RSS / guest pages / the Tier-3 cache keep growing
after warm-up, or the two engines produce different frames. Known, filed problems are listed in
`tests/corpus/soak-known.json` (id -> issue URL and the finding text it covers); only a finding
that is not listed fails the run.

  scripts/soak_corpus.py --executable <ounce_core test binary> --builds <dir> --evidence <dir> [--ids a,b]
  scripts/soak_corpus.py ... --smoke        # CI-sized: a few programs, a few hundred frames
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import subprocess
import sys

REPO = Path(__file__).resolve().parents[1]
TEST = "loader::fixture_tests::corpus_runner::corpus_soak::corpus_soak"
# One console program, one SDL program, one threaded/audio program and one GPU program.
SMOKE_IDS = ["printing-hello-world", "sdl2-simple", "user-timers", "deko-basic"]
SMOKE_FRAMES, SMOKE_SECONDS = 300, 90


def known_findings(path):
    if not path.is_file():
        return {}
    return json.loads(path.read_text()).get("known", {})


def split_findings(oid, findings, known):
    """Findings not covered by a recorded issue for this program. Matching is by prefix."""
    covered = [entry["finding"] for entry in known.get(oid, [])]
    new = [f for f in findings if not any(f.startswith(prefix) for prefix in covered)]
    return new, [f for f in findings if f not in new]


def soak_one(args, oracle, known):
    oid = oracle["id"]
    result = args.evidence / f"{oid}.soak.json"
    log = args.evidence / f"{oid}.soak.log"
    result.unlink(missing_ok=True)
    env = dict(os.environ, OUNCE_CORPUS_ROOT=str(args.builds), OUNCE_ORACLE_ID=oid,
               OUNCE_SOAK_RESULT=str(result), OUNCE_SOAK_FRAMES=str(args.frames),
               OUNCE_SOAK_SECONDS=str(args.seconds), OUNCE_SOAK_INPUT=str(args.input_seed),
               OUNCE_SOAK_SAMPLE=str(args.sample))
    command = [str(args.executable), TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    with log.open("wb") as out:
        try:
            done = subprocess.run(command, env=env, stdout=out, stderr=subprocess.STDOUT,
                                  timeout=2 * args.seconds + 120, check=False)
            host = "exited-zero" if done.returncode == 0 else f"exited-{done.returncode}"
        except subprocess.TimeoutExpired:
            host = "outer-timeout"
    if result.is_file():
        verdict = json.loads(result.read_text())
        findings = list(verdict["findings"])
        summary = [{k: r[k] for k in ("engine", "outcome", "frames_reached", "frames_per_second",
                                      "rss_growth_kib", "guest_page_growth", "jit_block_growth")}
                   for r in verdict["runs"]]
        compared = verdict["compared_frames"]
    else:
        # A crash of the whole test process (panic, abort) leaves no verdict: that is a finding too.
        findings = [f"no verdict ({host}); see {log.name}"]
        summary, compared = [], 0
    new, listed = split_findings(oid, findings, known)
    return {"id": oid, "host": host, "compared_frames": compared, "runs": summary,
            "findings": findings, "known_findings": listed, "new_findings": new}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--executable", type=Path, required=True, help="compiled ounce-core test binary")
    parser.add_argument("--builds", type=Path, required=True, help="directory holding <build>/build/source")
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--ids", help="comma-separated oracle ids (default: every registered program)")
    parser.add_argument("--frames", type=int, default=2000, help="queued frames to reach")
    parser.add_argument("--seconds", type=int, default=600, help="host time per engine")
    parser.add_argument("--sample", type=int, default=100, help="frames between samples")
    parser.add_argument("--input-seed", type=int, default=0, help="non-zero: scripted pseudo-random buttons")
    parser.add_argument("--shards", type=int, default=1, help="programs soaked concurrently (timings shift with load)")
    parser.add_argument("--smoke", action="store_true", help="CI-sized variant")
    parser.add_argument("--known", type=Path, default=REPO / "tests/corpus/soak-known.json")
    args = parser.parse_args()
    if args.smoke:
        args.frames, args.seconds, args.sample = SMOKE_FRAMES, SMOKE_SECONDS, 50
        args.ids = args.ids or ",".join(SMOKE_IDS)
    oracles = json.loads((REPO / "tests/corpus/oracles.json").read_text())["oracles"]
    if args.ids:
        wanted = args.ids.split(",")
        missing = [i for i in wanted if i not in {o["id"] for o in oracles}]
        if missing:
            parser.error(f"not registered oracles: {missing}")
        oracles = [o for o in oracles if o["id"] in wanted]
    args.evidence.mkdir(parents=True, exist_ok=True)
    known = known_findings(args.known)
    def run_case(oracle):
        if not (args.builds / oracle["build"] / "build/source" / oracle["nro"]).is_file():
            return {"id": oracle["id"], "host": "missing-nro", "compared_frames": 0, "runs": [], "findings": [],
                    "known_findings": [], "new_findings": [f"corpus program missing from the build: {oracle['nro']}"]}
        entry = soak_one(args, oracle, known)
        outcomes = "/".join(r["outcome"] for r in entry["runs"]) or entry["host"]
        print(f"{'OK  ' if not entry['new_findings'] else 'FAIL'} {entry['id']}: {outcomes}"
              f"{' ' + '; '.join(entry['new_findings'][:2]) if entry['new_findings'] else ''}", flush=True)
        return entry

    with ThreadPoolExecutor(max_workers=max(1, args.shards)) as pool:
        results = list(pool.map(run_case, oracles))
    (args.evidence / "soak-summary.json").write_text(json.dumps(
        {"frames": args.frames, "seconds_per_engine": args.seconds, "input_seed": args.input_seed,
         "results": results}, indent=2) + "\n")
    lines = [f"### Corpus soak ({args.frames} frames, {args.seconds}s per engine, input seed {args.input_seed})", "",
             "| program | interpreter | tier-3 | compared frames | findings |", "|---|---|---|---|---|"]
    for entry in results:
        cell = {r["engine"]: f"{r['outcome']} {r['frames_reached']}f @ {r['frames_per_second']:.0f}/s" for r in entry["runs"]}
        lines.append(f"| {entry['id']} | {cell.get('Interpreter', '-')} | {cell.get('Tier3', '-')} | "
                     f"{entry['compared_frames']} | {'; '.join(entry['findings'])[:200] or 'none'} |")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
            summary.write("\n".join(lines) + "\n")
    print("\n".join(lines))
    failed = [e["id"] for e in results if e["new_findings"]]
    if failed:
        sys.exit(f"soak findings not covered by tests/corpus/soak-known.json: {failed}")


if __name__ == "__main__":
    main()
