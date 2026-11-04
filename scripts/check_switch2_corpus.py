#!/usr/bin/env python3
"""Check the Switch 2 homebrew corpus (PLAN step 8, docs/switch2-corpus.md) before any run.

  tests/corpus/switch2/oracles.json            outside programs only; each names a build manifest
  tests/corpus/switch2/oracle-builds/*.json    pinned source, licence and toolchain of each build
  tests/corpus/switch2/oracle-results.json     observed verdicts, keyed by oracle id
  tests/corpus/switch2/synthetic-oracles.json  our own harness programs; never counted

Fails on a synthetic program in the outside set (or the reverse), an id shared with another set,
a missing or unpinned build manifest, a manifest no oracle uses, an oracle without a spec or
expected value, or a result for an unknown oracle. It never runs or builds anything.
"""
import json
from pathlib import Path
import re
import sys

REPO = Path(__file__).resolve().parents[1]
CORPUS = REPO / "tests/corpus/switch2"
FIXTURES = REPO / "tests/fixtures/switch2-synthetic"
SAFE_ID = re.compile(r"[a-z0-9][a-z0-9_-]*")
FORMATS = {"nro"}  # src/loader/switch2.rs ProgramFormat
RESULTS = {"pass", "fail", "not-run"}


def plain_relative(path):
    return isinstance(path, str) and bool(path) and not path.startswith("/") and "\\" not in path and all(
        part not in ("", ".", "..") for part in path.split("/"))


def check_oracle(oracle, origin, errors):
    oid = oracle.get("id")
    if not isinstance(oid, str) or not SAFE_ID.fullmatch(oid):
        errors.append(f"oracle id {oid!r} is not a safe name")
        return
    if oracle.get("platform") != "switch2":
        errors.append(f"{oid}: platform must be \"switch2\"")
    if oracle.get("format", "nro") not in FORMATS:
        errors.append(f"{oid}: format {oracle.get('format')!r} is not supported by the loader ({sorted(FORMATS)})")
    if (oracle.get("build") == "synthetic") != (origin == "synthetic"):
        errors.append(f"{oid}: build \"synthetic\" is for synthetic programs only, and they need it")
    for field in ("nro", "source"):
        if not plain_relative(oracle.get(field)):
            errors.append(f"{oid}: {field} must be a plain relative path")
    frames = oracle.get("queued_frames")
    if not isinstance(frames, int) or isinstance(frames, bool) or frames < 1:
        errors.append(f"{oid}: queued_frames must be a positive integer")
        frames = 0
    checks = oracle.get("checks")
    if not isinstance(checks, list) or not checks:
        errors.append(f"{oid}: needs at least one check")
        checks = []
    for check in checks:
        listed = check.get("frames")
        if not isinstance(listed, list) or not listed or not all(isinstance(f, int) and 1 <= f <= frames for f in listed):
            errors.append(f"{oid}: check frames must be within 1..queued_frames")
        if "spec" not in check:
            errors.append(f"{oid}: every check needs the spec it was written from")
        expect = check.get("expect", {})
        if "rgba_fnv1a64" not in expect and not check.get("probes"):
            errors.append(f"{oid}: check has no expected hash or probes (run tools/corpus_oracle.py)")
    if not isinstance(oracle.get("assumptions"), list):
        errors.append(f"{oid}: assumptions must be a list (may be empty)")


def check_manifest(name, manifest, errors):
    if manifest.get("platform") != "switch2":
        errors.append(f"oracle-builds/{name}.json: platform must be \"switch2\"")
    if not isinstance(manifest.get("revision"), str) or not re.fullmatch(r"[0-9a-f]{40}", manifest["revision"]):
        errors.append(f"oracle-builds/{name}.json: revision must be an exact 40-hex commit")
    for field in ("repository", "upstream_terms", "toolchain_image"):
        if not isinstance(manifest.get(field), str) or not manifest[field].strip():
            errors.append(f"oracle-builds/{name}.json: {field} is required")
    if not isinstance(manifest.get("samples"), list) or not manifest["samples"]:
        errors.append(f"oracle-builds/{name}.json: samples must be a nonempty list")


def check(corpus=CORPUS, fixtures=FIXTURES, step2=REPO / "tests/corpus/oracles.json"):
    errors = []
    load = lambda path: json.loads(path.read_text(encoding="utf-8"))
    outside, synthetic = load(corpus / "oracles.json"), load(corpus / "synthetic-oracles.json")
    results = load(corpus / "oracle-results.json").get("results")
    for label, data, origin in (("oracles.json", outside, "outside"), ("synthetic-oracles.json", synthetic, "synthetic")):
        if data.get("schema_version") != 1 or data.get("platform") != "switch2" or data.get("origin") != origin:
            errors.append(f"{label}: header must be schema_version 1, platform switch2, origin {origin}")
        for oracle in data.get("oracles", []):
            check_oracle(oracle, origin, errors)
    step2_ids = {o["id"] for o in load(step2)["oracles"]}
    seen = set()
    for oracle in outside.get("oracles", []) + synthetic.get("oracles", []):
        oid = oracle.get("id")
        if oid in seen or oid in step2_ids:
            errors.append(f"{oid}: id is used twice across the corpus sets")
        seen.add(oid)
    manifests = {path.stem: path for path in sorted((corpus / "oracle-builds").glob("*.json"))}
    used = {o.get("build") for o in outside.get("oracles", [])}
    for build in sorted(b for b in used if isinstance(b, str)):
        if build not in manifests:
            errors.append(f"build {build!r} has no oracle-builds/{build}.json")
    for name, path in manifests.items():
        check_manifest(name, load(path), errors)
        if name not in used:
            errors.append(f"oracle-builds/{name}.json is used by no oracle")
    for oracle in synthetic.get("oracles", []):
        if plain_relative(oracle.get("nro")) and not (fixtures / oracle["nro"]).is_file():
            errors.append(f"{oracle['id']}: synthetic program {oracle['nro']} is not committed")
    outside_ids = {o.get("id") for o in outside.get("oracles", [])}
    if not isinstance(results, dict):
        errors.append("oracle-results.json: results must be an object")
        results = {}
    for oid, status in results.items():
        if oid not in outside_ids:
            errors.append(f"oracle-results.json: {oid!r} is not an outside Switch 2 oracle")
        if status not in RESULTS:
            errors.append(f"oracle-results.json: {oid}: status must be one of {sorted(RESULTS)}")
    return errors


def main():
    errors = check()
    if errors:
        raise SystemExit("Switch 2 corpus check failed:\n" + "\n".join(f"  {e}" for e in errors))
    outside = json.loads((CORPUS / "oracles.json").read_text())["oracles"]
    synthetic = json.loads((CORPUS / "synthetic-oracles.json").read_text())["oracles"]
    print(f"Switch 2 corpus check passed: {len(outside)} outside programs, {len(synthetic)} synthetic (never counted).")


if __name__ == "__main__":
    sys.exit(main())
