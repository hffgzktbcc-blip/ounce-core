#!/usr/bin/env python3
"""Qualify a proposed repair against independent, retained corpus evidence."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from corpus_verify import ENGINES, REPO
from diagnose_corpus_run import diagnose, has_failure, load, required_oracles

ORACLE_PATH = "tests/corpus/oracles.json"
EVIDENCE_TOOLS = (
    ".github/workflows/external-corpus.yml",
    "scripts/corpus_verify.py",
    "scripts/diagnose_corpus_run.py",
    "scripts/prepare_external_corpus.py",
    "scripts/qualify_corpus_repair.py",
    "scripts/run_corpus_oracles.py",
    "scripts/verify_corpus_run.py",
    "src/loader/corpus_runner.rs",
    "tools/corpus_oracle.py",
    "tools/ounce_verify.py",
)


def revision(records):
    commits = {record["execution"]["core_commit"] for record in records.values()}
    if len(commits) != 1:
        raise ValueError("each report must identify one core revision")
    return commits.pop()


def registered_cases(manifest):
    ids = [oracle["id"] for oracle in json.loads(manifest)["oracles"]]
    if not ids or len(ids) != len(set(ids)):
        raise ValueError("the oracle manifest must register unique program IDs")
    return {(program, engine) for program in ids for engine in ENGINES.values()}


def clean_pass(record):
    return not has_failure(record) and record["execution"]["host_status"] == "exited-zero"


def evaluate(reference, candidate, program, expected):
    issues = []
    for side, records in (("reference", reference), ("candidate", candidate)):
        if records.keys() != expected:
            missing = sorted(expected - records.keys())
            extra = sorted(records.keys() - expected)
            issues.append(f"{side} coverage differs from the registered corpus: missing={missing}, extra={extra}")

    focused = {(program, engine) for engine in ENGINES.values()}
    if not focused <= expected:
        issues.append(f"{program}: target is not registered")
    if not focused <= reference.keys() & candidate.keys():
        return issues

    baseline = [reference[key] for key in sorted(focused)]
    if not any(record["verdict"] == "fail" for record in baseline):
        issues.append(f"{program}: reference has no observed oracle failure")
    hashes = {record["source"]["binary_sha256"] for record in baseline}
    if len(hashes) != 1 or None in hashes:
        issues.append(f"{program}: reference engines did not identify the same NRO bytes")

    for key in sorted(reference.keys() & candidate.keys()):
        before, after = reference[key], candidate[key]
        case = after["case_id"]
        before_checks = {key: oracle["expected"] for key, oracle in required_oracles(before).items()}
        after_checks = {key: oracle["expected"] for key, oracle in required_oracles(after).items()}
        if before_checks != after_checks:
            issues.append(f"{case}: required oracle expectations changed")

        was_passing = clean_pass(before)
        if key in focused or was_passing:
            digest = before["source"]["binary_sha256"]
            if not digest or digest != after["source"]["binary_sha256"]:
                issues.append(f"{case}: candidate did not execute the same NRO bytes")
            for field in ("limits", "input_script_sha256"):
                if before["execution"].get(field) != after["execution"].get(field):
                    issues.append(f"{case}: execution {field} changed")
            if not clean_pass(after):
                outcome = "target did not pass" if key in focused else "previously passing case regressed"
                issues.append(f"{case}: {outcome}")
    return issues


def manifest_at(commit):
    return subprocess.check_output(
        ["git", "show", f"{commit}:{ORACLE_PATH}"], cwd=REPO, timeout=30
    )


def qualify(reference_path, candidate_path, program, output):
    reference_bytes, candidate_bytes = reference_path.read_bytes(), candidate_path.read_bytes()
    reference, candidate = load(reference_bytes), load(candidate_bytes)
    before, after = revision(reference), revision(candidate)
    reference_manifest, candidate_manifest = manifest_at(before), manifest_at(after)
    issues = evaluate(reference, candidate, program, registered_cases(reference_manifest))
    if before == after:
        issues.append("reference and candidate must identify distinct core revisions")
    if reference_manifest != candidate_manifest:
        issues.append("candidate changed the registered oracle manifest")
    changed_tools = subprocess.check_output(
        ["git", "diff", "--name-only", before, after, "--", *EVIDENCE_TOOLS],
        cwd=REPO, text=True, timeout=30,
    ).splitlines()
    for path in changed_tools:
        issues.append(f"candidate changed evidence tooling: {path}")

    output.mkdir(parents=True)
    result = {
        "schema_version": "ounce-verify/repair-v1",
        "program": program,
        "qualified": not issues,
        "issues": issues,
        "reference": {"core_commit": before},
        "candidate": {"core_commit": after},
    }
    for side, report, manifest in (("reference", reference_bytes, reference_manifest),
                                   ("candidate", candidate_bytes, candidate_manifest)):
        report_name, manifest_name = f"{side}-report.json", f"{side}-oracles.json"
        (output / report_name).write_bytes(report)
        (output / manifest_name).write_bytes(manifest)
        result[side].update(report=report_name, report_sha256=hashlib.sha256(report).hexdigest(),
                            oracle_manifest=manifest_name, oracle_sha256=hashlib.sha256(manifest).hexdigest())
    (output / "diagnosis.json").write_text(json.dumps(diagnose(reference, candidate), indent=2) + "\n")
    (output / "repair.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path, help="canonical report with the original failure")
    parser.add_argument("candidate", type=Path, help="canonical report from the isolated repair revision")
    parser.add_argument("--program", required=True, help="registered program the repair targets")
    parser.add_argument("--output", type=Path, required=True, help="new directory for the repair evidence bundle")
    args = parser.parse_args()
    try:
        result = qualify(args.reference, args.candidate, args.program, args.output)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(2, f"ounce-verify repair: {error}\n")
    for issue in result["issues"]:
        print(issue)
    print(f"{'QUALIFIED' if result['qualified'] else 'REJECTED'} {args.program} ({args.output / 'repair.json'})")
    return 0 if result["qualified"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
