#!/usr/bin/env python3
"""Validate Ounce-Verify v0 evidence without executing guest programs."""

from __future__ import annotations

import argparse
import hashlib
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


def verify_artifacts(record, root):
    """Verify local bytes without allowing paths outside the supplied artifact root."""
    artifacts = record.get("artifacts", [])
    if not isinstance(artifacts, list):
        raise EvidenceError("artifacts must be an array")
    binary_verified = False
    seen = set()
    root = root.resolve(strict=True)
    for artifact in artifacts:
        artifact = _object(artifact, "artifact")
        path = _text(artifact.get("path"), "artifact.path")
        relative = Path(path)
        if relative.is_absolute() or ".." in relative.parts or path in seen:
            raise EvidenceError("artifact paths must be unique relative paths without traversal")
        seen.add(path)
        _sha(artifact.get("sha256"), "artifact.sha256")
        resolved = (root / relative).resolve(strict=True)
        if not resolved.is_relative_to(root) or not resolved.is_file():
            raise EvidenceError("artifact must be a file within the artifact root")
        digest = hashlib.sha256()
        with resolved.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
        if digest.hexdigest() != artifact["sha256"]:
            raise EvidenceError(f"artifact hash mismatch: {path}")
        if artifact.get("role") == "binary":
            if artifact["sha256"] != record["source"]["binary_sha256"]:
                raise EvidenceError("binary artifact hash differs from source.binary_sha256")
            binary_verified = True
    if record["verdict"] == "pass" and not binary_verified:
        raise EvidenceError("passing evidence requires a verified binary artifact")
    return binary_verified


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
    if (not isinstance(execution.get("guest_instructions"), int)
            or isinstance(execution["guest_instructions"], bool) or execution["guest_instructions"] < 0):
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
    if execution.get("observation_missing") and computed == "pass":
        raise EvidenceError("missing observations cannot pass")
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
    normalized.pop("artifact_integrity", None)  # Never trust a caller-supplied integrity claim.
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


def canonical_report(records, artifact_root=None):
    normalized = validate_records(records)
    if artifact_root is not None:
        for record in normalized:
            record["artifact_integrity"] = (
                "verified" if verify_artifacts(record, artifact_root) else "incomplete"
            )
    return json.dumps(
        {"schema_version": SCHEMA, "records": normalized},
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ) + "\n"


def canonical_hbx_report(record, oracle):
    """Recompute SDK-path pixel verdicts against an independently registered oracle."""
    record = _object(record, "record")
    oracle = _object(oracle, "oracle")
    if (record.get("schema_version") != "ounce-verify/hbx-first-frame-v1"
            or record.get("target") != "switch2-homebrew" or oracle.get("platform") != "switch2"):
        raise EvidenceError("unknown HBX schema or target")
    if record.get("source") != oracle.get("source"):
        raise EvidenceError("HBX source differs from registered oracle")
    for key in ("code_sha256", "data_sha256", "report_sha256", "oracle_sha256"):
        _sha(record.get(key), key)
    pixels = oracle.get("pixels")
    if not isinstance(pixels, list) or not pixels:
        raise EvidenceError("HBX oracle needs registered pixels")
    runs = record.get("runs")
    if not isinstance(runs, list) or len(runs) != 2:
        raise EvidenceError("HBX evidence requires both engines")
    engines, hashes, statuses = set(), [], []
    for run in runs:
        run = _object(run, "run")
        engine = run.get("engine")
        if engine not in {"interpreter", "tier3"} or engine in engines:
            raise EvidenceError("duplicate or unknown HBX engine")
        engines.add(engine)
        success = not run.get("error")
        if success:
            success = run.get("width") == oracle.get("width") and run.get("height") == oracle.get("height")
            samples = run.get("pixels")
            if not isinstance(samples, list) or len(samples) != len(pixels):
                raise EvidenceError("HBX pixel coverage differs from registered oracle")
            for sample, expected in zip(samples, pixels):
                sample = _object(sample, "pixel")
                if any(sample.get(key) != expected.get(key) for key in ("x", "y")) or sample.get("expected") != expected.get("rgba"):
                    raise EvidenceError("HBX pixel expectation differs from registered oracle")
                observed = sample.get("observed")
                if observed is not None and (not isinstance(observed, list) or len(observed) != 4
                        or any(type(v) is not int or not 0 <= v <= 255 for v in observed)):
                    raise EvidenceError("HBX observed pixel must be RGBA bytes or null")
                matched = observed == expected["rgba"]
                if sample.get("pass") is not matched:
                    raise EvidenceError("HBX pixel verdict disagrees with observation")
                success = success and matched
            digest = run.get("rgba_fnv1a64")
            if not isinstance(digest, str) or not HEX64.fullmatch(digest):
                raise EvidenceError("invalid HBX frame hash")
            hashes.append(digest)
        if run.get("pass") is not success:
            raise EvidenceError("HBX engine verdict disagrees with observations")
        statuses.append(success)
    agreement = len(hashes) == 2 and hashes[0] == hashes[1]
    verdict = all(statuses) and agreement
    if record.get("engine_agreement") is not agreement or record.get("pass") is not verdict:
        raise EvidenceError("HBX overall verdict disagrees with observations")
    normalized = json.loads(json.dumps(record))
    normalized["rendering"] = "HBLauncher host UI callbacks; not a Switch 2 GPU-path validation"
    return json.dumps(normalized, sort_keys=True, separators=(",", ":")) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path, help="JSON evidence object or array")
    parser.add_argument("--output", type=Path, help="write canonical validation report")
    parser.add_argument("--artifact-root", type=Path, help="verify referenced artifact bytes under this directory")
    parser.add_argument("--hbx-oracle", type=Path, help="validate HBLauncher SDK-path evidence against this registered oracle")
    args = parser.parse_args(argv)
    try:
        data = json.loads(args.evidence.read_text(encoding="utf-8"))
        records = data if isinstance(data, list) else [data]
        if args.hbx_oracle:
            import hashlib
            oracle_bytes = args.hbx_oracle.read_bytes()
            if data.get("oracle_sha256") != hashlib.sha256(oracle_bytes).hexdigest():
                raise EvidenceError("HBX oracle artifact digest mismatch")
            output = canonical_hbx_report(data, json.loads(oracle_bytes))
        else:
            output = canonical_report(records, args.artifact_root)
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

