#!/usr/bin/env python3
"""Validate Ounce-Verify v0 evidence without executing guest programs."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SCHEMA = "ounce-verify/v0"
VERDICTS = {"pass", "fail", "blocked", "not-assessed"}
ORACLE_SCOPES = {"application", "graphics", "regression", "comparison"}
SHA256 = re.compile(r"^[0-9a-f]{64}$")
GIT_SHA = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^0x[0-9a-fA-F]{16}$")


class EvidenceError(ValueError):
    pass


def _object(value, name):
    if not isinstance(value, dict):
        raise EvidenceError(f"{name} must be an object")
    return value


def _text(value, name):
    if not isinstance(value, str) or not value.strip():
        raise EvidenceError(f"{name} must be a non-empty string")
    return value


def _sha(value, name, nullable=False):
    if value is None and nullable:
        return
    if not isinstance(value, str) or not SHA256.fullmatch(value):
        raise EvidenceError(f"{name} must be 64 lowercase hexadecimal characters")


def _oracle_verdict(oracles):
    relevant = [o for o in oracles if o["required"] and o["scope"] in {"application", "graphics"}]
    if not relevant:
        return "not-assessed"
    statuses = {o["status"] for o in relevant}
    if "fail" in statuses:
        return "fail"
    if "blocked" in statuses:
        return "blocked"
    if statuses == {"pass"}:
        return "pass"
    return "not-assessed"


def validate_record(record):
    """Return a normalized record or raise EvidenceError."""
    record = _object(record, "record")
    if record.get("schema_version") != SCHEMA:
        raise EvidenceError(f"schema_version must be {SCHEMA!r}")
    _text(record.get("case_id"), "case_id")
    target = _text(record.get("target"), "target")
    if target not in {"original-switch-libnx", "switch2-homebrew", "synthetic-aarch64"}:
        raise EvidenceError("target is not recognized")

    source = _object(record.get("source"), "source")
    _text(source.get("origin"), "source.origin")
    _text(source.get("repository"), "source.repository")
    _text(source.get("commit"), "source.commit")
    _sha(source.get("source_tree_sha256"), "source.source_tree_sha256", nullable=True)
    _sha(source.get("binary_sha256"), "source.binary_sha256", nullable=True)
    _text(source.get("toolchain"), "source.toolchain")

    execution = _object(record.get("execution"), "execution")
    if not isinstance(execution.get("core_commit"), str) or not GIT_SHA.fullmatch(execution["core_commit"]):
        raise EvidenceError("execution.core_commit must be a 40-character lowercase git SHA")
    _text(execution.get("engine"), "execution.engine")
    _text(execution.get("runner"), "execution.runner")
    _sha(execution.get("input_script_sha256"), "execution.input_script_sha256", nullable=True)
    limits = _object(execution.get("limits"), "execution.limits")
    for field in ("max_guest_instructions", "host_timeout_seconds"):
        if not isinstance(limits.get(field), int) or isinstance(limits.get(field), bool) or limits[field] <= 0:
            raise EvidenceError(f"execution.limits.{field} must be a positive integer")
    _text(execution.get("host_status"), "execution.host_status")
    _text(execution.get("guest_stop"), "execution.guest_stop")
    for field in ("guest_pc", "guest_x0"):
        if not isinstance(execution.get(field), str) or not HEX64.fullmatch(execution[field]):
            raise EvidenceError(f"execution.{field} must be a 16-digit hexadecimal value")
    if not isinstance(execution.get("guest_instructions"), int) or execution["guest_instructions"] < 0:
        raise EvidenceError("execution.guest_instructions must be a non-negative integer")
    if not isinstance(execution.get("diagnostics_truncated"), bool):
        raise EvidenceError("execution.diagnostics_truncated must be boolean")
    if execution["diagnostics_truncated"]:
        raise EvidenceError("truncated evidence cannot be validated")

    raw_oracles = record.get("oracles")
    if not isinstance(raw_oracles, list):
        raise EvidenceError("oracles must be an array")
    names = set()
    oracles = []
    for index, raw in enumerate(raw_oracles):
        oracle = _object(raw, f"oracles[{index}]")
        name = _text(oracle.get("name"), f"oracles[{index}].name")
        if name in names:
            raise EvidenceError(f"duplicate oracle name {name!r}")
        names.add(name)
        scope = oracle.get("scope")
        status = oracle.get("status")
        if scope not in ORACLE_SCOPES:
            raise EvidenceError(f"oracle {name!r} has an unknown scope")
        if status not in VERDICTS:
            raise EvidenceError(f"oracle {name!r} has an unknown status")
        if not isinstance(oracle.get("required"), bool):
            raise EvidenceError(f"oracle {name!r} required must be boolean")
        if "expected" not in oracle or "observed" not in oracle:
            raise EvidenceError(f"oracle {name!r} must preserve expected and observed values")
        oracles.append(dict(oracle))

    computed = _oracle_verdict(oracles)
    declared = record.get("verdict")
    if declared not in VERDICTS:
        raise EvidenceError("verdict is unknown")
    if declared != computed:
        raise EvidenceError(f"declared verdict {declared!r} does not match computed verdict {computed!r}")
    if execution["guest_stop"] == "svc-break" and computed == "pass" and not any(
        o["scope"] == "application" and o["name"] != "guest-stop" and o["status"] == "pass"
        for o in oracles
    ):
        raise EvidenceError("svc-break cannot become an application pass from host status or stop alone")

    normalized = json.loads(json.dumps(record))
    normalized["verdict"] = computed
    normalized["reproducibility"] = (
        "content-addressed"
        if source.get("source_tree_sha256") and source.get("binary_sha256")
        else "incomplete"
    )
    return normalized


def validate_records(records):
    if not isinstance(records, list) or not records:
        raise EvidenceError("input must contain one record or a non-empty array of records")
    seen = set()
    normalized = []
    for record in records:
        item = validate_record(record)
        case_id = item["case_id"]
        if case_id in seen:
            raise EvidenceError(f"duplicate case_id {case_id!r}")
        seen.add(case_id)
        normalized.append(item)
    return sorted(normalized, key=lambda item: item["case_id"])


def canonical_report(records):
    return json.dumps(
        {"schema_version": SCHEMA, "records": validate_records(records)},
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path, help="JSON evidence object or array")
    parser.add_argument("--output", type=Path, help="write canonical validation report")
    args = parser.parse_args(argv)
    try:
        data = json.loads(args.evidence.read_text(encoding="utf-8"))
        records = data if isinstance(data, list) else [data]
        output = canonical_report(records)
        if args.output:
            args.output.write_text(output, encoding="utf-8")
        else:
            sys.stdout.write(output)
        return 0
    except (OSError, json.JSONDecodeError, EvidenceError) as error:
        print(f"ounce-verify: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
