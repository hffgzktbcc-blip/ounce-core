#!/usr/bin/env python3
"""Compare canonical corpus reports and retain replay instructions for failures."""

import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import sys

from corpus_verify import ENGINES, REPO
from run_corpus_oracles import TEST

sys.path.insert(0, str(REPO / "tools"))
import ounce_verify


def load(data):
    report = json.loads(data)
    if report["schema_version"] != ounce_verify.SCHEMA:
        raise ValueError("expected an Ounce-Verify v0 canonical report")
    records = {}
    for record in ounce_verify.validate_records(report["records"]):
        prefix, program, engine = record["case_id"].split("/")
        if (prefix != "corpus" or not program or engine not in ENGINES.values()
                or engine != record["execution"]["engine"]):
            raise ValueError(f"invalid corpus case ID or engine: {record['case_id']}")
        records[program, engine] = record
    return records


def required_oracles(record):
    return {(oracle["scope"], oracle["name"]): oracle for oracle in record["oracles"]
            if oracle["required"]}


def first_divergence(reference, candidate):
    """Find the first differing retained frame, then the terminal observation."""
    if reference is None or candidate is None:
        return {"kind": "coverage", "missing": "reference" if reference is None else "candidate"}
    before_hash, after_hash = (record["source"]["binary_sha256"] for record in (reference, candidate))
    if not before_hash or not after_hash:
        return {"kind": "missing-binary-hash", "reference": before_hash, "candidate": after_hash}
    if before_hash != after_hash:
        return {"kind": "different-binary", "reference": before_hash, "candidate": after_hash}
    left, right = required_oracles(reference), required_oracles(candidate)
    if {key: item["expected"] for key, item in left.items()} != {
        key: item["expected"] for key, item in right.items()
    }:
        return {"kind": "different-oracle"}
    graphics = ("graphics", "first-frame")
    observed = [oracles.get(graphics, {}).get("observed") for oracles in (left, right)]
    if any(value is None for value in observed):
        return {"kind": "missing-observation",
                "reference": observed[0] is not None, "candidate": observed[1] is not None}
    before, after = ({frame["index"]: frame for frame in value["queued"]} for value in observed)
    for index in sorted(before.keys() | after.keys()):
        if before.get(index) != after.get(index):
            return {"kind": "frame", "index": index,
                    "reference": before.get(index), "candidate": after.get(index)}
    for field in ("guest_stop", "guest_pc", "guest_x0", "guest_instructions", "host_status"):
        before, after = (record["execution"].get(field) for record in (reference, candidate))
        if before != after:
            return {"kind": field, "reference": before, "candidate": after}
    for key in sorted(left):
        before, after = (oracles[key]["observed"] for oracles in (left, right))
        if key == graphics:
            before, after = before.get("failures", []), after.get("failures", [])
        if before != after:
            return {"kind": "oracle-observation", "oracle": key[1],
                    "reference": before, "candidate": after}
    return None


def ownership(failing, failing_side, divergence):
    kind = (divergence or {}).get("kind")
    if kind in {"coverage", "missing-binary-hash", "different-binary", "different-oracle",
                "missing-observation"}:
        return "corpus inputs or evidence", "Check matching NRO bytes, oracle registration and runner logs."
    stop = failing["execution"]["guest_stop"]
    if stop in {"cpu-fault", "cpu-halt", "unhandled-svc", "svc-break"}:
        return "CPU or kernel", "Inspect the terminal PC and collect an instruction or SVC trace."
    if stop == "exit-process" and kind == "frame" and divergence[failing_side] is None:
        return "guest initialization or service emulation", "Trace startup IPC and applet, nvdrv and display initialization before guest exit."
    if kind == "frame":
        return "graphics or presentation", "Inspect the queue and capture the first differing frame."
    return "runner or runtime", "Inspect terminal observations and the raw oracle log."


def replay(program, record):
    if record is None or not record["source"]["binary_sha256"]:
        return None
    execution = record["execution"]
    seconds = execution["limits"]["host_timeout_seconds"]
    return {
        "checkout": execution["core_commit"],
        "nro_sha256": record["source"]["binary_sha256"],
        "argv": ["cargo", "test", "--release", "--locked", TEST, "--",
                 "--exact", "--ignored", "--nocapture", "--test-threads=1"],
        "env": {"OUNCE_ORACLE_ID": program, "OUNCE_ORACLE_SECONDS": str(seconds)},
        "required_env": ["OUNCE_CORPUS_ROOT", "OUNCE_ORACLE_RESULT"],
    }


def comparison_failures(record):
    return [oracle for oracle in (record["oracles"] if record else [])
            if oracle["scope"] == "comparison" and oracle["status"] == "fail"]


def has_failure(record):
    return record is None or record["verdict"] != "pass" or bool(comparison_failures(record))


def diagnose(reference, candidate):
    cases = []
    groups = defaultdict(list)
    for program, engine in sorted(reference.keys() | candidate.keys()):
        before, after = reference.get((program, engine)), candidate.get((program, engine))
        before_verdict = before["verdict"] if before else "missing"
        after_verdict = after["verdict"] if after else "missing"
        if not has_failure(before) and not has_failure(after):
            continue
        difference = first_divergence(before, after)
        failing_side = "candidate" if has_failure(after) else "reference"
        failing = before if failing_side == "reference" else after
        owner, next_check = ownership(failing, failing_side, difference)
        case_id = f"corpus/{program}/{engine}"
        groups[owner].append(case_id)
        cases.append({
            "case_id": case_id,
            "observations": {
                "reference_verdict": before_verdict, "candidate_verdict": after_verdict,
                "reference_comparison_failures": comparison_failures(before),
                "candidate_comparison_failures": comparison_failures(after),
                "reference_execution": before["execution"] if before else None,
                "candidate_execution": after["execution"] if after else None,
                "first_divergence": difference,
            },
            "hypothesis": {"likely_owner": owner, "next_check": next_check},
            "replay": {"reference": replay(program, before), "candidate": replay(program, after)},
        })
    return {"schema_version": "ounce-verify/diagnosis-v1", "groups": dict(groups), "cases": cases}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path, help="reference canonical report")
    parser.add_argument("candidate", type=Path, help="candidate canonical report")
    parser.add_argument("--output", type=Path, required=True, help="diagnosis and replay bundle JSON")
    args = parser.parse_args()
    try:
        reference, candidate = args.reference.read_bytes(), args.candidate.read_bytes()
        bundle = diagnose(load(reference), load(candidate))
        bundle["reports_sha256"] = {
            "reference": hashlib.sha256(reference).hexdigest(),
            "candidate": hashlib.sha256(candidate).hexdigest(),
        }
        args.output.write_text(json.dumps(bundle, indent=2) + "\n")
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, f"ounce-verify diagnosis: {error}\n")
    print(f"{len(bundle['cases'])} case(s) in {len(bundle['groups'])} group(s): {args.output}")


if __name__ == "__main__":
    main()
