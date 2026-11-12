#!/usr/bin/env python3
"""Ounce-Verify adapter for the first-frame oracle corpus (docs/corpus-oracles.md).

  evidence  map run_corpus_oracles.py verdicts to `ounce-verify/v0` records, one per program and engine
  headline  derive the README headline count from a canonical report written by tools/ounce_verify.py
  snapshot  keep tests/corpus/oracle-results.json consistent with the README headline (and a report)

A program counts toward the headline only when its record for every engine has verdict `pass`.
The README count (the headline markers and the oracle badge) is never edited by hand:
`headline --write-readme` rewrites it from a report and `headline --check-readme` fails when the
README disagrees with the report.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

REPO = Path(__file__).resolve().parents[1]
README_MARK = re.compile(r"<!-- corpus-headline -->.*?<!-- /corpus-headline -->", re.S)
BADGE_MARK = re.compile(r"first--frame_oracles-\d+%2F\d+_pass")
ENGINES = {"Interpreter": "interpreter", "Tier3": "tier3"}
STOPS = (("FrameLimit", "frame-limit"), ("ExitProcess", "exit-process"), ("Break", "svc-break"),
         ("UnhandledSvc", "unhandled-svc"), ("CpuHalt", "cpu-halt"), ("Fault", "cpu-fault"),
         ("InstructionBudget", "instruction-budget"), ("HostTimeout", "host-timeout"),
         ("LoadError", "load-error"), ("MissingInput", "missing-input"))


def source_tree_sha256(inventory):
    """SHA-256 of the sorted `<sha256>  <path>` lines of every tracked file at the pinned commit."""
    listing = "".join(f"{sha}  {path}\n" for path, sha in sorted(inventory["source_sha256"].items()))
    return hashlib.sha256(listing.encode()).hexdigest()


def guest_stop(stop):
    return next((name for prefix, name in STOPS if stop.startswith(prefix)), "unknown")


def records_for(oracle, verdict, inventory, toolchain, core_commit):
    nro_hashes = {path: sha for sample in inventory["samples"] for path, sha in sample["nro_sha256"].items()}
    binary = verdict.get("nro_sha256")
    if binary is not None and nro_hashes.get(oracle["nro"]) != binary:
        raise SystemExit(f"{oracle['id']}: executed NRO hash differs from the build inventory")
    if binary is None:
        binary = nro_hashes.get(oracle["nro"])
    expected = [{k: c[k] for k in ("frames", "expect", "probes", "width", "height") if k in c} for c in oracle["checks"]]
    raw_runs = verdict.get("runs", [])
    runs = {run["engine"]: run for run in raw_runs}
    if len(runs) != len(raw_runs) or set(runs) - set(ENGINES):
        raise SystemExit(f"{oracle['id']}: duplicate or unknown engine")
    if runs and verdict.get("nro_sha256") is None:
        raise SystemExit(f"{oracle['id']}: executed NRO hash is missing")
    agree = not any("frames differ" in f for f in verdict.get("failures", []))
    out = []
    for engine, slug in ENGINES.items():
        run = runs.get(engine)
        observed = run is not None
        stop = guest_stop(run["stop"]) if observed else "not-observed"
        frame_status = ("pass" if not run["failures"] else "fail") if observed else "blocked"
        oracles = [
            {"name": "guest-stop", "scope": "application", "required": True,
             "status": ("pass" if stop == "frame-limit" else "fail") if observed else "blocked",
             "expected": "frame-limit", "observed": run["stop"] if observed else None},
            {"name": "first-frame", "scope": "graphics", "required": True, "status": frame_status,
             "expected": expected,
             "observed": {"queued": run["queued"], "failures": run["failures"]} if observed else None},
            {"name": "engine-agreement", "scope": "comparison", "required": False,
             "status": ("pass" if agree else "fail") if len(runs) == 2 else "not-assessed",
             "expected": "interpreter and Tier-3 frames identical", "observed": "identical" if agree else "differ"},
        ]
        statuses = {o["status"] for o in oracles if o["required"]}
        record = {
            "schema_version": "ounce-verify/v0",
            "case_id": f"corpus/{oracle['id']}/{slug}",
            "target": "original-switch-libnx",
            "source": {
                "origin": "external-public-homebrew",
                "repository": inventory["repository"],
                "commit": inventory["revision"],
                "source_tree_sha256": source_tree_sha256(inventory),
                "binary_sha256": binary,
                "toolchain": toolchain,
            },
            "execution": {
                "core_commit": core_commit,
                "engine": slug,
                "runner": "corpus_first_frame_oracle (scripts/run_corpus_oracles.py)",
                "input_script_sha256": None,
                "limits": {"max_guest_instructions": run["max_guest_instructions"] if observed else 4_000_000_000,
                           "host_timeout_seconds": run["host_timeout_seconds"] if observed else verdict["host_timeout_seconds"]},
                "host_status": verdict.get("host_status", "unknown"),
                "guest_stop": stop,
                "guest_pc": run["pc"] if observed else "0x0000000000000000",
                "guest_x0": run["x0"] if observed else "0x0000000000000000",
                "guest_instructions": run["instructions"] if observed else 0,
                "diagnostics_truncated": False,
            },
            "oracles": oracles,
            "verdict": "fail" if "fail" in statuses else "blocked" if "blocked" in statuses else "pass",
        }
        if not observed:
            record["execution"]["observation_missing"] = True  # pc, x0 and instructions were not observed
        out.append(record)
    return out


def evidence(args):
    oracles = json.loads((REPO / "tests/corpus/oracles.json").read_text())["oracles"]
    verdicts = {v["id"]: v for v in json.loads(args.verdicts.read_text())["verdicts"]}
    image = json.loads(args.toolchain_image.read_text())[0]["Id"]
    records = []
    for oracle in oracles:
        inventory = json.loads((args.builds / oracle["build"] / "build/inventory.json").read_text())
        tools = inventory["tools"]
        libnx = inventory.get("toolchain_inputs_sha256", {}).get("libnx/lib/libnx.a", "unrecorded")
        toolchain = f"{tools['compiler']}; devkitpro/devkita64 image {image}; libnx.a sha256 {libnx}"
        verdict = verdicts.get(oracle["id"]) or {"id": oracle["id"], "failures": ["not run"],
                                                 "host_status": "not-run", "host_timeout_seconds": 1}
        items = records_for(oracle, verdict, inventory, toolchain, args.core_commit)
        for item in items:
            item["artifacts"] = [{
                "role": "binary",
                "path": f"{oracle['build']}/build/source/{oracle['nro']}",
                "sha256": item["source"]["binary_sha256"],
            }] if item["source"]["binary_sha256"] else []
        records += items
    args.output.write_text(json.dumps(records, indent=2, ensure_ascii=False) + "\n")
    print(f"{len(records)} records for {len(oracles)} programs -> {args.output}")


def count(report, manifest=None):
    # Never trust a report's denominator or engine count to define coverage.
    sys.path.insert(0, str(REPO / "tools"))
    import ounce_verify

    data = json.loads(report.read_text())
    if data.get("schema_version") != ounce_verify.SCHEMA:
        raise SystemExit("not an ounce-verify/v0 canonical report")
    manifest = manifest or REPO / "tests/corpus/oracles.json"
    policy = json.loads(manifest.read_text())
    ids = [o["id"] for o in policy["oracles"]]
    required_engines = policy.get("required_engines", list(ENGINES.values()))
    if (not isinstance(required_engines, list) or any(not isinstance(e, str) for e in required_engines)
            or len(required_engines) != len(ENGINES)
            or set(required_engines) != set(ENGINES.values())):
        raise SystemExit("manifest must require each corpus engine exactly once")
    target = policy.get("target", "original-switch-libnx")
    if target != "original-switch-libnx":
        raise SystemExit("this headline is restricted to original-Switch homebrew")
    if not ids or len(set(ids)) != len(ids):
        raise SystemExit("oracle manifest must contain unique, non-empty coverage")
    programs = {oid: {} for oid in ids}
    try:
        records = ounce_verify.validate_records(data["records"])
    except (KeyError, ounce_verify.EvidenceError) as error:
        raise SystemExit(f"invalid canonical report: {error}") from error
    for record in records:
        parts = record["case_id"].split("/")
        if len(parts) != 3 or parts[0] != "corpus":
            raise SystemExit("invalid corpus case ID")
        _, program, engine = parts
        if program not in programs or engine not in ENGINES.values():
            raise SystemExit("report contains an unregistered program or engine")
        if record["target"] != target:
            raise SystemExit("report target differs from the registered corpus")
        if record["execution"]["engine"] != engine:
            raise SystemExit("case ID and execution engine disagree")
        programs[program][engine] = record["verdict"]
    required = set(required_engines)
    passed = sorted(p for p, runs in programs.items()
                    if set(runs) == required and set(runs.values()) == {"pass"})
    return passed, len(programs)


def render_readme(current, passed, total):
    """README text with every headline marker and the oracle badge set to `passed/total`."""
    if not README_MARK.search(current):
        raise SystemExit("README.md has no corpus-headline marker")
    if not BADGE_MARK.search(current):
        raise SystemExit("README.md has no first-frame oracle badge")
    text = README_MARK.sub(f"<!-- corpus-headline -->{passed}/{total}<!-- /corpus-headline -->", current)
    return BADGE_MARK.sub(f"first--frame_oracles-{passed}%2F{total}_pass", text)


def headline(args):
    passed, total = count(args.report)
    print(f"{len(passed)}/{total} programs pass on every engine: {', '.join(passed) or 'none'}")
    readme = REPO / "README.md"
    current = readme.read_text()
    expected = render_readme(current, len(passed), total)
    # docs/DETAILS.md repeats the headline in the corpus section and the roadmap table.
    details = REPO / "docs/DETAILS.md"
    details_now = details.read_text()
    details_expected = README_MARK.sub(f"<!-- corpus-headline -->{len(passed)}/{total}<!-- /corpus-headline -->", details_now)
    if args.write_readme:
        readme.write_text(expected)
        details.write_text(details_expected)
    elif args.check_readme and (current != expected or details_now != details_expected):
        raise SystemExit(f"README or docs/DETAILS.md headline differs from the report ({len(passed)}/{total})")


def snapshot_counts(snapshot):
    results = snapshot["results"]
    return sum(v == "pass" for v in results.values()), len(results)


def readme_counts(text):
    """(passed, total) from every headline marker and the badge; fails if they disagree."""
    found = {tuple(map(int, m.split("/"))) for m in
             re.findall(r"<!-- corpus-headline -->(\d+/\d+)<!-- /corpus-headline -->", text)}
    found |= {(int(a), int(b)) for a, b in re.findall(r"first--frame_oracles-(\d+)%2F(\d+)_pass", text)}
    if len(found) != 1:
        raise SystemExit(f"README headline markers and badge disagree or are missing: {sorted(found)}")
    return found.pop()


def snapshot(args):
    path = REPO / "tests/corpus/oracle-results.json"
    ids = [o["id"] for o in json.loads((REPO / "tests/corpus/oracles.json").read_text())["oracles"]]
    if args.write:
        if not args.report:
            raise SystemExit("--write needs --report")
        passed, _ = count(args.report)
        data = json.loads(path.read_text())
        data["results"] = {i: "pass" if i in passed else "fail" for i in ids}
        data["recorded_from"] = str(args.report.resolve().relative_to(REPO))
        data["core_commit"] = args.core_commit or data.get("core_commit")
        path.write_text(json.dumps(data, indent=2) + "\n")
    data = json.loads(path.read_text())
    problems = []
    if set(data["results"]) != set(ids):
        problems.append("snapshot programs differ from tests/corpus/oracles.json")
    got = snapshot_counts(data)
    want = readme_counts((REPO / "README.md").read_text())
    if got != want:
        problems.append(f"snapshot records {got[0]}/{got[1]} but README headline says {want[0]}/{want[1]}")
    if args.report:
        passed, total = count(args.report)
        reported = {i: "pass" if i in passed else "fail" for i in ids}
        if {k: ("pass" if v == "pass" else "fail") for k, v in data["results"].items()} != reported:
            problems.append(f"snapshot verdicts differ from the report ({len(passed)}/{total})")
    if problems:
        raise SystemExit("; ".join(problems))
    print(f"snapshot, README headline{' and report' if args.report else ''} agree: {got[0]}/{got[1]}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    e = sub.add_parser("evidence")
    e.add_argument("--verdicts", type=Path, required=True, help="oracle-verdicts.json from run_corpus_oracles.py")
    e.add_argument("--builds", type=Path, required=True)
    e.add_argument("--toolchain-image", type=Path, required=True, help="docker image inspect output")
    e.add_argument("--core-commit", required=True)
    e.add_argument("--output", type=Path, required=True)
    h = sub.add_parser("headline")
    h.add_argument("report", type=Path, help="canonical report from tools/ounce_verify.py")
    mode = h.add_mutually_exclusive_group()
    mode.add_argument("--check-readme", action="store_true")
    mode.add_argument("--write-readme", action="store_true")
    n = sub.add_parser("snapshot", help="check (default) or --write tests/corpus/oracle-results.json")
    n.add_argument("--report", type=Path, help="canonical report to compare with (required by --write)")
    n.add_argument("--write", action="store_true")
    n.add_argument("--core-commit", help="core commit the report was produced at (with --write)")
    args = parser.parse_args()
    {"evidence": evidence, "headline": headline, "snapshot": snapshot}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())

