#!/usr/bin/env python3
"""Ounce-Verify adapter for the first-frame oracle corpus (docs/corpus-oracles.md).

  evidence  map run_corpus_oracles.py verdicts to `ounce-verify/v0` records, one per program and engine
  headline  derive the README headline count from a canonical report written by tools/ounce_verify.py

A program counts toward the headline only when its record for every engine has verdict `pass`.
The README count is never edited by hand: `headline --write-readme` rewrites it from a report and
`headline --check-readme` fails when the README disagrees with the report.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

REPO = Path(__file__).resolve().parents[1]
README_MARK = re.compile(r"<!-- corpus-headline -->.*?<!-- /corpus-headline -->", re.S)
ENGINES = {"Interpreter": "interpreter", "Tier3": "tier3"}
# Oracle sets: file and case-id prefix. Each set's header names its platform (default original Switch).
SETS = {"step2": ("tests/corpus/oracles.json", "corpus"), "switch2": ("tests/corpus/switch2/oracles.json", "switch2")}
TARGETS = {"switch": "original-switch-libnx", "switch2": "switch2-homebrew"}
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


def records_for(oracle, verdict, inventory, toolchain, core_commit, target="original-switch-libnx", prefix="corpus"):
    nro_hashes = {path: sha for sample in inventory["samples"] for path, sha in sample["nro_sha256"].items()}
    binary = verdict.get("nro_sha256")
    if binary is not None and nro_hashes.get(oracle["nro"]) != binary:
        raise SystemExit(f"{oracle['id']}: executed NRO hash differs from the build inventory")
    if binary is None:
        binary = nro_hashes.get(oracle["nro"])
    expected = [{k: c[k] for k in ("frames", "expect", "probes", "width", "height") if k in c} for c in oracle["checks"]]
    runs = {run["engine"]: run for run in verdict.get("runs", [])}
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
            "case_id": f"{prefix}/{oracle['id']}/{slug}",
            "target": target,
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
    oracles_file, prefix = SETS[args.set]
    oracle_set = json.loads((REPO / oracles_file).read_text())
    if oracle_set.get("origin", "outside") != "outside":
        raise SystemExit(f"{oracles_file}: only outside programs produce Ounce-Verify evidence")
    target = TARGETS[oracle_set.get("platform", "switch")]
    oracles = oracle_set["oracles"]
    verdicts = {v["id"]: v for v in json.loads(args.verdicts.read_text())["verdicts"]}
    image = json.loads(args.toolchain_image.read_text())[0]["Id"]
    records = []
    for oracle in oracles:
        inventory = json.loads((args.builds / oracle["build"] / "build/inventory.json").read_text())
        tools = inventory["tools"]
        if target == "original-switch-libnx":
            libnx = inventory.get("toolchain_inputs_sha256", {}).get("libnx/lib/libnx.a", "unrecorded")
            toolchain = f"{tools['compiler']}; devkitpro/devkita64 image {image}; libnx.a sha256 {libnx}"
        else:
            toolchain = f"{tools['compiler']}; toolchain image {image}"
        verdict = verdicts.get(oracle["id"]) or {"id": oracle["id"], "failures": ["not run"],
                                                 "host_status": "not-run", "host_timeout_seconds": 1}
        records += records_for(oracle, verdict, inventory, toolchain, args.core_commit, target, prefix)
    args.output.write_text(json.dumps(records, indent=2, ensure_ascii=False) + "\n")
    print(f"{len(records)} records for {len(oracles)} programs -> {args.output}")


def count(report):
    data = json.loads(report.read_text())
    if data.get("schema_version") != "ounce-verify/v0":
        raise SystemExit("not an ounce-verify/v0 canonical report")
    programs = {}
    for record in data["records"]:
        _, program, _ = record["case_id"].split("/")
        programs.setdefault(program, []).append(record["verdict"])
    passed = sorted(p for p, verdicts in programs.items() if len(verdicts) == len(ENGINES) and set(verdicts) == {"pass"})
    return passed, len(programs)


def headline(args):
    passed, total = count(args.report)
    text = f"<!-- corpus-headline -->{len(passed)}/{total}<!-- /corpus-headline -->"
    print(f"{len(passed)}/{total} programs pass on every engine: {', '.join(passed) or 'none'}")
    readme = REPO / "README.md"
    current = readme.read_text()
    if not README_MARK.search(current):
        raise SystemExit("README.md has no corpus-headline marker")
    if args.write_readme:
        readme.write_text(README_MARK.sub(text, current))
    elif args.check_readme and README_MARK.search(current).group(0) != text:
        raise SystemExit(f"README headline {README_MARK.search(current).group(0)} differs from the report ({text})")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    e = sub.add_parser("evidence")
    e.add_argument("--verdicts", type=Path, required=True, help="oracle-verdicts.json from run_corpus_oracles.py")
    e.add_argument("--builds", type=Path, required=True)
    e.add_argument("--toolchain-image", type=Path, required=True, help="docker image inspect output")
    e.add_argument("--core-commit", required=True)
    e.add_argument("--output", type=Path, required=True)
    e.add_argument("--set", choices=sorted(SETS), default="step2", help="oracle set (default step2)")
    h = sub.add_parser("headline")
    h.add_argument("report", type=Path, help="canonical report from tools/ounce_verify.py")
    mode = h.add_mutually_exclusive_group()
    mode.add_argument("--check-readme", action="store_true")
    mode.add_argument("--write-readme", action="store_true")
    args = parser.parse_args()
    evidence(args) if args.command == "evidence" else headline(args)


if __name__ == "__main__":
    sys.exit(main())
