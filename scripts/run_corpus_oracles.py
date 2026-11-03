#!/usr/bin/env python3
"""Run every pre-registered first-frame oracle (tests/corpus/oracles.json) and record each verdict.

Each oracle runs in its own process under an outer timeout, because a blocking guest SVC cannot be
interrupted from inside the runner. A failing oracle is recorded, not hidden: the job fails only when
an oracle that `tests/corpus/oracle-results.json` records as passing stops passing, or when a corpus
program is missing from the build (a workflow error, not a runtime result).
"""
import argparse
import json
import os
from pathlib import Path
import subprocess

REPO = Path(__file__).resolve().parents[1]
TEST = "loader::fixture_tests::corpus_runner::corpus_first_frame_oracle"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True, help="compiled ounce-core test binary")
    parser.add_argument("--builds", type=Path, required=True, help="directory holding <build>/build/source")
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--seconds", type=int, default=300, help="host time per engine")
    args = parser.parse_args()
    oracles = json.loads((REPO / "tests/corpus/oracles.json").read_text())["oracles"]
    recorded = json.loads((REPO / "tests/corpus/oracle-results.json").read_text())["results"]
    args.evidence.mkdir(parents=True, exist_ok=True)
    verdicts, errors = [], []
    for oracle in oracles:
        oid = oracle["id"]
        nro = args.builds / oracle["build"] / "build/source" / oracle["nro"]
        if not nro.is_file():
            errors.append(f"{oid}: corpus program missing from the build: {nro}")
            continue
        result = args.evidence / f"{oid}.oracle.json"
        log = args.evidence / f"{oid}.oracle.log"
        env = dict(os.environ, OUNCE_CORPUS_ROOT=str(args.builds), OUNCE_ORACLE_ID=oid,
                   OUNCE_ORACLE_RESULT=str(result), OUNCE_ORACLE_SECONDS=str(args.seconds))
        command = [str(args.executable), TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"]
        with log.open("wb") as out:
            try:
                subprocess.run(command, env=env, stdout=out, stderr=subprocess.STDOUT,
                               timeout=2 * args.seconds + 120, check=False)
                stop = None
            except subprocess.TimeoutExpired:
                stop = "outer host timeout"
        if result.is_file():
            verdict = json.loads(result.read_text())
        else:
            verdict = {"id": oid, "pass": False, "failures": [stop or "runner exited without a verdict (see log)"]}
        verdicts.append(verdict)
        print(f"{'PASS' if verdict['pass'] else 'FAIL'} {oid}: {'; '.join(verdict['failures'][:3])}", flush=True)
    passed = [v["id"] for v in verdicts if v["pass"]]
    (args.evidence / "oracle-verdicts.json").write_text(json.dumps(
        {"passed": len(passed), "total": len(oracles), "verdicts": verdicts}, indent=2) + "\n")
    lines = [f"### First-frame oracles: {len(passed)}/{len(oracles)} pass", "", "| oracle | verdict | first failure |", "|---|---|---|"]
    for v in verdicts:
        lines.append(f"| {v['id']} | {'pass' if v['pass'] else 'fail'} | {(v['failures'] or [''])[0][:160]} |")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
            summary.write("\n".join(lines) + "\n")
    print("\n".join(lines))
    regressions = [oid for oid, status in recorded.items() if status == "pass" and oid not in passed]
    if regressions:
        errors.append(f"recorded passes regressed: {regressions}")
    if errors:
        raise SystemExit("\n".join(errors))


if __name__ == "__main__":
    main()
