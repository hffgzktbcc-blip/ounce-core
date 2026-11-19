#!/usr/bin/env python3
"""Run the same programs through two or more Ounce tools and compare their `ounce.run.v1` reports.

    python3 scripts/family_check.py --tool core=target/release/ounce-core \\
                                    --tool runtime=../ounce-runtime/target/release/ounce-runtime

Every program in `--fixtures` (default `tests/fixtures/nro`, one `.nro` per folder) is run by each tool as
`<tool> run-nro <file> --present=headless --audio=null --deterministic --report-json=<file>`. The first `--tool` is the
reference; each other tool is compared to it.

Two kinds of keys are compared:

* **Strict** keys are the facts the tools already agree on. A difference, or a run that wrote no report, fails the check
  (exit 1).
* **Advisory** keys are reported but never fail the check: numbers that may not mean the same thing in every tool yet
  (`insns_executed`), and keys one tool does not write at all. They show where the tools can still learn from each
  other, and they move into the strict list only once the contract freezes them.

This script only runs the programs and reads the JSON reports. It shares no code between the tools. Exit codes:
0 all strict keys agree, 1 a strict difference or a missing report, 2 bad usage.
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

STRICT_KEYS = ("schema", "stop_kind", "exit_code", "frames_presented", "framebuffer_hash")
ADVISORY_KEYS = (
    "insns_executed",
    "audio_frames_mixed",
    "audio_hash",
    "guest_exit_status",
    "lr",
    "sp",
)
RUN_FLAGS = ("--present=headless", "--audio=null", "--deterministic")
TIMEOUT_SECONDS = 120


def compare(reference_name, reference, other_name, other):
    """Compare two reports; either may be None (no report written). Returns (strict, advisory) lists of strings."""
    if reference is None or other is None:
        who = reference_name if reference is None else other_name
        return [f"no report written by {who}"], []
    strict = []
    for key in STRICT_KEYS:
        a, b = reference.get(key), other.get(key)
        if a != b:
            strict.append(f"{key}: {reference_name}={a!r} {other_name}={b!r}")
    advisory = []
    for key in ADVISORY_KEYS:
        in_a, in_b = key in reference, key in other
        if in_a and not in_b:
            advisory.append(f"{key}: only {reference_name} writes it ({reference[key]!r})")
        elif in_b and not in_a:
            advisory.append(f"{key}: only {other_name} writes it ({other[key]!r})")
        elif in_a and reference[key] != other[key]:
            advisory.append(f"{key}: {reference_name}={reference[key]!r} {other_name}={other[key]!r}")
    return strict, advisory


def find_programs(fixtures):
    return sorted(p for p in Path(fixtures).glob("*/*.nro") if p.is_file())


def run_tool(binary, program, report):
    """Run one tool on one program. Returns the parsed report, or None if none was written or it did not parse."""
    report.unlink(missing_ok=True)
    cmd = [str(binary), "run-nro", str(program), *RUN_FLAGS, f"--report-json={report}"]
    try:
        subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=TIMEOUT_SECONDS, check=False)
        return json.loads(report.read_text(encoding="utf-8"))
    except (OSError, ValueError, subprocess.TimeoutExpired):
        return None


def parse_tools(specs):
    tools = []
    for spec in specs:
        name, sep, path = spec.partition("=")
        if not sep or not name or not path:
            raise ValueError(f"--tool wants NAME=PATH, got {spec!r}")
        if not Path(path).is_file():
            raise ValueError(f"{name}: no such binary: {path}")
        tools.append((name, path))
    if len(tools) < 2:
        raise ValueError("give at least two --tool NAME=PATH (the first is the reference)")
    if len({n for n, _ in tools}) != len(tools):
        raise ValueError("--tool names must be different")
    return tools


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--tool", action="append", default=[], metavar="NAME=PATH", help="a tool binary; repeat; first is the reference")
    ap.add_argument("--fixtures", default="tests/fixtures/nro", help="folder of <program>/<program>.nro (default %(default)s)")
    ap.add_argument("--work", required=True, help="new or empty folder for the JSON reports")
    ap.add_argument("--json", metavar="FILE", help="also write the full result as JSON")
    args = ap.parse_args(argv)
    try:
        tools = parse_tools(args.tool)
    except ValueError as err:
        print(err, file=sys.stderr)
        return 2
    programs = find_programs(args.fixtures)
    if not programs:
        print(f"no programs found under {args.fixtures}", file=sys.stderr)
        return 2
    work = Path(args.work)
    if work.exists() and any(work.iterdir()):
        print(f"{work} is not empty", file=sys.stderr)
        return 2
    work.mkdir(parents=True, exist_ok=True)

    ref_name, ref_bin = tools[0]
    results, failed = [], False
    for program in programs:
        label = program.stem
        reports = {}
        for name, binary in tools:
            reports[name] = run_tool(binary, program, work / f"{label}.{name}.json")
        row = {"program": label, "others": {}}
        for name, _ in tools[1:]:
            strict, advisory = compare(ref_name, reports[ref_name], name, reports[name])
            row["others"][name] = {"strict": strict, "advisory": advisory}
            failed = failed or bool(strict)
        results.append(row)

    for row in results:
        for name, diff in row["others"].items():
            verdict = "DIFFER" if diff["strict"] else "agree"
            print(f"{row['program']:<12} {ref_name} vs {name}: {verdict}")
            for line in diff["strict"]:
                print(f"    strict   {line}")
            for line in diff["advisory"]:
                print(f"    advisory {line}")
    total = len(results)
    bad = sum(1 for r in results if any(d["strict"] for d in r["others"].values()))
    print(f"\n{total - bad} of {total} programs agree on every strict key ({', '.join(STRICT_KEYS)}).")
    if args.json:
        Path(args.json).write_text(json.dumps({"reference": ref_name, "results": results}, indent=2) + "\n", encoding="utf-8")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
