#!/usr/bin/env python3
"""Find a first-parent transition for one unchanged Ounce-Verify oracle."""

import argparse
from contextlib import suppress
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile

from corpus_build_cache import digest
from run_corpus_oracles import REPO, TEST
from verify_corpus_run import positive_seconds


def git(*args, cwd=REPO):
    return subprocess.check_output(["git", *map(str, args)], cwd=cwd, text=True, timeout=60).strip()


def oracle_at(commit, program):
    manifest = json.loads(git("show", f"{commit}:tests/corpus/oracles.json"))
    return next((oracle for oracle in manifest["oracles"] if oracle["id"] == program), None)


def observed_state(result, program, nro_hash, returncode):
    if (result["id"] != program or result["nro_sha256"] != nro_hash
            or [run["engine"] for run in result["runs"]] != ["Interpreter", "Tier3"]
            or any(run["stop"] == "HostTimeout" for run in result["runs"])):
        return "unknown"
    if result["pass"] is True and returncode == 0:
        return "pass"
    if result["pass"] is False and returncode == 101:
        return "fail"
    return "unknown"


def narrow(commits, before, after, evaluate):
    if evaluate(commits[0]) != before or evaluate(commits[-1]) != after:
        raise ValueError("endpoint observations do not match the requested transition")
    low, high = 0, len(commits) - 1
    while high - low > 1:
        middle = (low + high) // 2
        state = evaluate(commits[middle])
        if state == before:
            low = middle
        elif state == after:
            high = middle
        else:
            break
    return commits[low], commits[high], high - low == 1


def run_trial(checkout, log, env, timeout):
    command = ["cargo", "test", "--release", "--locked", TEST, "--",
               "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    with log.open("wb") as stream:
        with subprocess.Popen(command, cwd=checkout, env=env, stdout=stream,
                              stderr=subprocess.STDOUT, start_new_session=True) as process:
            try:
                return process.wait(timeout=timeout)
            except (subprocess.TimeoutExpired, KeyboardInterrupt):
                with suppress(ProcessLookupError):
                    os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                raise


def run(args):
    start, end = (git("rev-parse", "--verify", f"{rev}^{{commit}}") for rev in (args.start, args.end))
    path = git("rev-list", "--first-parent", "--reverse", f"{start}..{end}").splitlines()
    if not path or git("rev-parse", f"{path[0]}^1") != start:
        raise ValueError("start must be a strict first-parent ancestor of end")
    oracle = oracle_at(start, args.program)
    if oracle is None or oracle != oracle_at(end, args.program):
        raise ValueError("endpoints must register the same program and oracle")
    builds = args.builds.resolve()
    nro = builds / oracle["build"] / "build/source" / oracle["nro"]
    nro_hash = digest(nro)
    output = args.output.resolve()
    output.mkdir(parents=True)
    report = {
        "schema_version": "ounce-verify/bisect-v1",
        "program": args.program, "nro_sha256": nro_hash,
        "start": start, "end": end, "before": args.before,
        "seconds_per_engine": args.seconds, "trial_timeout_seconds": args.trial_timeout,
        "trials": [],
    }

    def save():
        temp = output / "bisection.tmp"
        temp.write_text(json.dumps(report, indent=2) + "\n")
        temp.replace(output / "bisection.json")

    with tempfile.TemporaryDirectory(dir=output) as directory:
        checkout = Path(directory) / "checkout"
        git("worktree", "add", "--detach", checkout, start)
        try:
            def evaluate(commit):
                entry = {"commit": commit, "state": "unknown"}
                report["trials"].append(entry)
                try:
                    if oracle_at(commit, args.program) != oracle:
                        entry["reason"] = "oracle changed"
                        return entry["state"]
                    git("checkout", "--detach", commit, cwd=checkout)
                    observation = output / f"{commit}.oracle.json"
                    log = output / f"{commit}.log"
                    entry.update(log=log.name, observation=observation.name)
                    env = dict(os.environ, OUNCE_CORPUS_ROOT=str(builds),
                               OUNCE_ORACLE_ID=args.program, OUNCE_ORACLE_RESULT=str(observation),
                               OUNCE_ORACLE_SECONDS=str(args.seconds))
                    env.pop("OUNCE_CPU", None)
                    env.setdefault("CARGO_TARGET_DIR", str(Path(directory) / "cargo"))
                    code = run_trial(checkout, log, env, args.trial_timeout)
                    entry["returncode"] = code
                    entry["state"] = observed_state(json.loads(observation.read_text()),
                                                   args.program, nro_hash, code)
                    if entry["state"] == "unknown":
                        entry["reason"] = "incomplete or incomparable guest observation"
                except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
                    entry["reason"] = str(error)
                finally:
                    save()
                    print(f"{commit[:12]} {entry['state']}", flush=True)
                return entry["state"]

            after = "fail" if args.before == "pass" else "pass"
            left, right, adjacent = narrow([start, *path], args.before, after, evaluate)
            report.update(last_observed_before=left, first_observed_after=right, adjacent=adjacent)
            save()
            print(f"{left[:12]}..{right[:12]} ({'adjacent' if adjacent else 'unresolved interval'})")
            return 0 if adjacent else 2
        finally:
            git("worktree", "remove", "--force", checkout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--start", required=True, help="first-parent ancestor")
    parser.add_argument("--end", required=True)
    parser.add_argument("--before", choices=("pass", "fail"), default="pass")
    parser.add_argument("--program", required=True)
    parser.add_argument("--builds", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--seconds", type=positive_seconds, default=180)
    parser.add_argument("--trial-timeout", type=positive_seconds, default=1200)
    try:
        return run(parser.parse_args())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"ounce-verify bisect: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
