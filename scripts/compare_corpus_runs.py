#!/usr/bin/env python3
"""Compare correctness and performance evidence from two complete corpus runs."""

import argparse
import json
import math
from pathlib import Path

from corpus_verify import ENGINES, REPO
from diagnose_corpus_run import diagnose, has_failure, load


def read_run(directory):
    data = (directory / "canonical-report.json").read_bytes()
    records = load(data)
    if any(record.get("artifact_integrity") != "verified" for record in json.loads(data)["records"]):
        raise ValueError(f"{directory}: canonical report contains unverified artifacts")
    manifest = json.loads((directory / "run-manifest.json").read_bytes())
    if any(record["execution"]["core_commit"] != manifest["core_commit"] for record in records.values()):
        raise ValueError(f"{directory}: report and run manifest identify different revisions")
    raw = json.loads((directory / "oracle-verdicts.json").read_bytes())["verdicts"]
    timing = {}
    for verdict in raw:
        for run in verdict.get("runs", []):
            key = verdict["id"], ENGINES[run["engine"]]
            record = records[key]
            if (key in timing or verdict["nro_sha256"] != record["source"]["binary_sha256"]
                    or run["instructions"] != record["execution"]["guest_instructions"]):
                raise ValueError(f"{directory}: raw observations disagree with {key}")
            timing[key] = run["wall_seconds"]
    return records, manifest, timing


def compare(reference, candidate, expected):
    before, before_run, before_time = reference
    after, after_run, after_time = candidate
    host = before_run.get("timing_host")
    comparable_host = bool(host and host == after_run.get("timing_host")
                           and before_run.get("oracle_shards") == after_run.get("oracle_shards")
                           and before_run.get("seconds_per_engine") == after_run.get("seconds_per_engine"))
    regressions, changes, performance = [], [], []
    for key in sorted(before.keys() | after.keys()):
        left, right = before.get(key), after.get(key)
        if has_failure(left) != has_failure(right):
            change = {"case": "/".join(key), "before": left["verdict"] if left else "missing",
                      "after": right["verdict"] if right else "missing"}
            changes.append(change)
            if has_failure(right):
                regressions.append(change)
        if has_failure(left) or has_failure(right):
            continue
        if (left["source"]["binary_sha256"] != right["source"]["binary_sha256"]
                or left["oracles"] != right["oracles"]
                or left["execution"]["limits"] != right["execution"]["limits"]):
            continue
        old_instructions, new_instructions = (record["execution"]["guest_instructions"]
                                               for record in (left, right))
        metric = {"case": "/".join(key)}
        if old_instructions != new_instructions:
            metric["instructions"] = [old_instructions, new_instructions]
        if comparable_host and key in before_time and key in after_time:
            old, new = before_time[key], after_time[key]
            if (math.isfinite(old) and math.isfinite(new) and min(old, new) >= 0
                    and max(old, new) >= 0.1 and abs(old - new) >= 0.25 * max(old, new)):
                metric["wall_seconds"] = [old, new]
        if len(metric) > 1:
            performance.append(metric)
    return {
        "schema_version": "ounce-verify/comparison-v1",
        "baseline": before_run["core_commit"], "candidate": after_run["core_commit"],
        "coverage": {
            "reference": len(before), "candidate": len(after), "expected": len(expected),
            "reference_missing": ["/".join(key) for key in sorted(expected - before.keys())],
            "reference_extra": ["/".join(key) for key in sorted(before.keys() - expected)],
            "candidate_missing": ["/".join(key) for key in sorted(expected - after.keys())],
            "candidate_extra": ["/".join(key) for key in sorted(after.keys() - expected)],
        },
        "wall_times_comparable": comparable_host,
        "verdict_changes": changes, "regressions": regressions, "performance_changes": performance,
        "diagnosis": diagnose(before, after),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="baseline evidence directory")
    parser.add_argument("candidate", type=Path, help="candidate evidence directory")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        manifest = json.loads((REPO / "tests/corpus/oracles.json").read_bytes())["oracles"]
        expected = {(oracle["id"], engine) for oracle in manifest for engine in ENGINES.values()}
        report = compare(read_run(args.baseline), read_run(args.candidate), expected)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
    except (OSError, ValueError, TypeError, KeyError) as error:
        parser.exit(2, f"ounce-verify comparison: {error}\n")
    print(f"Corpus comparison: {report['coverage']['candidate']}/{len(expected)} engine records, "
          f"{len(report['regressions'])} regressions, {len(report['performance_changes'])} performance changes")
    for item in report["regressions"]:
        print(f"- {item['case']}: {item['before']} → {item['after']}")
    coverage = report["coverage"]
    return int(bool(report["regressions"] or any(coverage[key] for key in (
        "reference_missing", "reference_extra", "candidate_missing", "candidate_extra"))))


if __name__ == "__main__":
    raise SystemExit(main())
