#!/usr/bin/env python3
"""Run every pre-registered first-frame oracle (tests/corpus/oracles.json) and record each verdict.

Each oracle runs in its own process under an outer timeout, because a blocking guest SVC cannot be
interrupted from inside the runner. A failing oracle is recorded, not hidden: the job fails only when
an oracle that `tests/corpus/oracle-results.json` records as passing stops passing, or when a corpus
program is missing from the build (a workflow error, not a runtime result).
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

from corpus_build_cache import digest

REPO = Path(__file__).resolve().parents[1]
TEST = "loader::fixture_tests::corpus_runner::corpus_first_frame_oracle"


def checkpoint_for(identity, result, log, host_status):
    return {"identity": identity, "result_sha256": digest(result),
            "log_sha256": digest(log), "host_status": host_status}


def completed_case(verdict, identity, status):
    engines = [run["engine"] for run in verdict.get("runs", [])]
    return (status in ("exited-zero", "exited-101")
            and verdict["pass"] is (status == "exited-zero")
            and verdict["id"] == identity["oracle"]["id"]
            and verdict.get("nro_sha256") == identity["nro_sha256"]
            and engines in (["Interpreter", "Tier3"], ["Tier3", "Native"]))


def resume_case(checkpoint, identity, result, log):
    try:
        saved = json.loads(checkpoint.read_text())
        status = saved["host_status"]
        if status not in ("exited-zero", "exited-101"):
            return None
        if saved != checkpoint_for(identity, result, log, status):
            return None
        verdict = json.loads(result.read_text())
        if not completed_case(verdict, identity, status):
            return None
        verdict.update(host_status=status, host_timeout_seconds=identity["seconds"])
        return verdict
    except (OSError, ValueError, KeyError, TypeError):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True, help="compiled ounce-core test binary")
    parser.add_argument("--builds", type=Path, required=True, help="directory holding <build>/build/source")
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--seconds", type=int, default=300, help="host time per engine")
    parser.add_argument("--shards", type=int, default=1, help="number of concurrent corpus shards")
    parser.add_argument("--resume", action="store_true", help="reuse matching completed cases in the same evidence directory")
    args = parser.parse_args()
    if args.shards < 1:
        parser.error("--shards must be at least 1")
    args.evidence.mkdir(parents=True, exist_ok=True)
    if args.resume:
        # The OS releases this lock after an interruption. A file alone is not a lock.
        import fcntl
        with (args.evidence / ".resume.lock").open("a") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                parser.error("a resumable run is already using this evidence directory")
            run(args, parser)
    else:
        run(args, parser)


def run(args, parser):
    oracle_manifest = REPO / "tests/corpus/oracles.json"
    oracles = json.loads(oracle_manifest.read_text())["oracles"]
    if len({oracle["id"] for oracle in oracles}) != len(oracles):
        parser.error("the oracle manifest contains duplicate IDs")
    recorded = json.loads((REPO / "tests/corpus/oracle-results.json").read_text())["results"]
    aggregate = args.evidence / "oracle-verdicts.json"
    aggregate.unlink(missing_ok=True)
    resumed = {}
    if args.resume:
        resumed = {
            "executable_sha256": digest(args.executable),
            "runner_sha256": digest(Path(__file__)),
            "host_sha256": hashlib.sha256(f"{platform.node()} {platform.platform()}".encode()).hexdigest(),
            "manifest_sha256": digest(oracle_manifest),
            "build_root": str(args.builds.resolve()),
            "seconds": args.seconds,
            "shards": args.shards,
            "environment": {k: v for k, v in os.environ.items() if k.startswith("OUNCE_")},
        }
        resumed["environment"] = hashlib.sha256(
            json.dumps(resumed["environment"], sort_keys=True).encode()).hexdigest()

    def run_case(oracle):
        oid = oracle["id"]
        nro = args.builds / oracle["build"] / "build/source" / oracle["nro"]
        if not nro.is_file():
            return None, f"{oid}: corpus program missing from the build: {nro}"
        result = args.evidence / f"{oid}.oracle.json"
        log = args.evidence / f"{oid}.oracle.log"
        checkpoint = args.evidence / f"{oid}.checkpoint.json"
        if args.resume:
            identity = dict(resumed, oracle=oracle, nro_sha256=digest(nro))
            saved = resume_case(checkpoint, identity, result, log)
            if saved is not None:
                print(f"REUSED {oid}", flush=True)
                return saved, None
        checkpoint.unlink(missing_ok=True)
        # A failed rerun must never inherit a verdict from an earlier execution.
        result.unlink(missing_ok=True)
        env = dict(os.environ, OUNCE_CORPUS_ROOT=str(args.builds), OUNCE_ORACLE_ID=oid,
                   OUNCE_ORACLE_RESULT=str(result), OUNCE_ORACLE_SECONDS=str(args.seconds))
        command = [str(args.executable), TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"]
        with log.open("wb") as out:
            try:
                done = subprocess.run(command, env=env, stdout=out, stderr=subprocess.STDOUT,
                                      timeout=2 * args.seconds + 120, check=False)
                stop, host_status = None, "exited-zero" if done.returncode == 0 else f"exited-{done.returncode}"
            except subprocess.TimeoutExpired:
                stop, host_status = "outer host timeout", "outer-timeout"
        evidence_error = None
        if result.is_file():
            try:
                verdict = json.loads(result.read_text())
                if (not isinstance(verdict, dict) or verdict.get("id") != oid
                        or not isinstance(verdict.get("pass"), bool)
                        or not isinstance(verdict.get("failures"), list)
                        or not all(isinstance(item, str) for item in verdict["failures"])
                        or not isinstance(verdict.get("runs", []), list)
                        or not all(isinstance(item, dict) for item in verdict.get("runs", []))):
                    raise ValueError("invalid verdict identity or fields")
            except (OSError, ValueError) as error:
                evidence_error = f"{oid}: invalid oracle evidence: {error}"
                # Preserve the raw file for diagnosis. Never checkpoint or reuse it.
                verdict = {"id": oid, "pass": False, "failures": [evidence_error]}
        else:
            verdict = {"id": oid, "pass": False, "failures": [stop or "runner exited without a verdict (see log)"]}
        if host_status != "exited-zero" and verdict["pass"]:
            verdict["pass"] = False
            verdict["failures"].append(f"runner did not complete successfully: {host_status}")
        verdict.update(host_status=host_status, host_timeout_seconds=args.seconds)
        if args.resume and evidence_error is None and result.is_file() and completed_case(verdict, identity, host_status):
            pending = checkpoint.with_suffix(".tmp")
            pending.write_text(json.dumps(checkpoint_for(identity, result, log, host_status)) + "\n")
            pending.replace(checkpoint)
        print(f"{'PASS' if verdict['pass'] else 'FAIL'} {oid}: {'; '.join(verdict['failures'][:3])}", flush=True)
        return verdict, evidence_error

    def run_shard(shard):
        return [run_case(oracle) for oracle in shard]

    shard_count = min(args.shards, len(oracles)) if oracles else 1
    shards = [oracles[index::shard_count] for index in range(shard_count)]
    if shard_count == 1:
        results = run_shard(shards[0])
    else:
        with ThreadPoolExecutor(max_workers=shard_count) as executor:
            completed = list(executor.map(run_shard, shards))
        # Restore registration order, independent of which shard completed first.
        results = [completed[index % shard_count][index // shard_count]
                   for index in range(len(oracles))]
    verdicts = [verdict for verdict, _ in results if verdict is not None]
    errors = [error for _, error in results if error is not None]
    passed = [v["id"] for v in verdicts if v["pass"]]
    pending_aggregate = aggregate.with_suffix(".tmp")
    pending_aggregate.write_text(json.dumps(
        {"passed": len(passed), "total": len(oracles), "verdicts": verdicts}, indent=2) + "\n")
    pending_aggregate.replace(aggregate)
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
