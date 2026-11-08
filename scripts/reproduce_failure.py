#!/usr/bin/env python3
"""Inspect the retained SDL2 failure and write a portable offline reproduction dossier."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import tempfile

from compare_corpus_runs import compare
from diagnose_corpus_run import REPO, diagnose, load


EXAMPLE = REPO / "docs/reproduce/sdl2-failure"


def read_evidence(root):
    manifest = json.loads((root / "manifest.json").read_bytes())
    if manifest["schema_version"] != "ounce-verify/retained-example-v1":
        raise ValueError("unsupported retained-example schema")
    files = {}
    for name, identity in manifest["files"].items():
        path = root / name
        if Path(name).is_absolute() or not path.resolve().is_relative_to(root.resolve()):
            raise ValueError(f"evidence path escapes its directory: {name}")
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != identity["sha256"]:
            raise ValueError(f"SHA-256 mismatch for {name}; restore the retained evidence")
        files[name] = data
    records = {}
    for side in ("reference", "failure"):
        run = manifest["runs"][side]
        for role in ("report", "observation", "log"):
            if run[role] not in files:
                raise ValueError(f"{side}: missing retained {role} in manifest")
        records[side] = load(files[run["report"]])
        raw = json.loads(files[run["observation"]])
        if raw["id"] != manifest["program"]:
            raise ValueError(f"{side}: observation identifies a different program")
        if {r["engine"] for r in raw["runs"]} != {"Interpreter", "Tier3"}:
            raise ValueError(f"{side}: observations must cover both engines")
        for observation in raw["runs"]:
            engine = {"Interpreter": "interpreter", "Tier3": "tier3"}[observation["engine"]]
            record = records[side][manifest["program"], engine]
            execution = record["execution"]
            graphics = next(o for o in record["oracles"] if o["name"] == "first-frame")
            if (raw["nro_sha256"] != record["source"]["binary_sha256"]
                    or observation["pc"] != execution["guest_pc"]
                    or observation["instructions"] != execution["guest_instructions"]
                    or observation["queued"] != graphics["observed"]["queued"]):
                raise ValueError(f"{side}: raw observations disagree with canonical {engine} record")
    return manifest, files, records


def write_bundle(evidence, output):
    manifest, files, records = read_evidence(evidence)
    program = manifest["program"]
    selected = {side: {key: record for key, record in values.items() if key[0] == program}
                for side, values in records.items()}
    expected = {(program, engine) for engine in ("interpreter", "tier3")}
    if any(set(values) != expected for values in selected.values()):
        raise ValueError("retained example must cover both engines on both sides")
    for key in expected:
        before, after = (selected[side][key] for side in ("reference", "failure"))
        if (before["source"] != after["source"]
                or before["execution"]["limits"] != after["execution"]["limits"]
                or before["execution"]["input_script_sha256"] != after["execution"]["input_script_sha256"]):
            raise ValueError("retained example has different source identity or execution inputs")
    diagnosis = diagnose(selected["reference"], selected["failure"])
    if len(diagnosis["cases"]) != 2 or any(
            not isinstance(case["observations"]["first_divergence"], dict)
            or case["observations"]["first_divergence"].get("kind") != "frame"
            or case["observations"]["first_divergence"].get("index") != 1
            or case["observations"]["first_divergence"].get("candidate") is not None
            or case["observations"]["reference_verdict"] != "pass"
            or case["observations"]["candidate_verdict"] != "fail"
            for case in diagnosis["cases"]):
        raise ValueError("evidence does not reproduce the registered missing-first-frame diagnosis")
    diagnosis["reports_sha256"] = {
        side: hashlib.sha256(files[manifest["runs"][name]["report"]]).hexdigest()
        for side, name in (("reference", "reference"), ("candidate", "failure"))}
    revisions = {side: {"core_commit": selected[side][program, "interpreter"]["execution"]["core_commit"]}
                 for side in selected}
    comparison = compare((records["failure"], revisions["failure"], {}),
                         (records["reference"], revisions["reference"], {}),
                         set(records["failure"]) | set(records["reference"]))
    if output.exists():
        raise ValueError(f"output already exists: {output}; choose a fresh directory")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".ounce-bundle-", dir=output.parent) as temp:
        staging = Path(temp) / "bundle"
        staging.mkdir()
        for name, data in files.items():
            path = staging / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        shutil.copyfile(evidence / "manifest.json", staging / "manifest.json")
        shutil.copyfile(evidence / "NOTICE.md", staging / "NOTICE.md")
        shutil.copyfile(REPO / "LICENSE", staging / "LICENSE")
        for name, value in (("diagnosis.json", diagnosis), ("comparison.json", comparison)):
            (staging / name).write_text(json.dumps(value, indent=2) + "\n")
        lines = ["# Retained SDL2 diagnosis", "",
                 "Offline inspection only; this command did not execute a guest or verify NRO bytes.",
                 "Candidate in diagnosis.json means the historical failure; reference means the passing run.",
                 "comparison.json uses the chronological failure-to-passing direction and retains all 40 records.",
                 "", "Both engines first differ at queued frame 1. The failure exits at 0x000000007ffe0004.",
                 "Startup IPC/display initialization is a hypothesis, not a proven root cause.", "",
                 "## Replay prerequisites", "",
                 "Use Linux or macOS with Rust/Cargo and the runtime's documented native build dependencies.",
                 "Use separate clean checkouts at the exact recorded revisions below. Do not replay on today's main.",
                 "Provide the exact NRO under OUNCE_CORPUS_ROOT at", 
                 "switch-examples/build/source/graphics/sdl2/sdl2-simple/sdl2-simple.nro.",
                 "Obtain it from an authorized local build/archive; this bundle contains no executable.",
                 "Rebuilding with newer packages may change bytes: a different SHA-256 is not this replay.",
                 "Verify the NRO hash before executing. Set OUNCE_ORACLE_RESULT to a fresh absolute JSON path.",
                 "The test runs both engines and checks the unchanged registered pixel expectations.", ""]
        for side in ("reference", "candidate"):
            replay = diagnosis["cases"][0]["replay"][side]
            lines += [f"## {side}", "", f"Core checkout: `{replay['checkout']}`",
                      f"NRO SHA-256: `{replay['nro_sha256']}`", "", "```sh",
                      "export OUNCE_CORPUS_ROOT=/absolute/path/to/builds",
                      "export OUNCE_ORACLE_RESULT=/absolute/path/to/fresh-observation.json",
                      " ".join(f"{key}={value}" for key, value in replay["env"].items()) + " \\",
                      "  " + " ".join(replay["argv"]), "```", ""]
        lines += ["The historical failing test is expected to exit nonzero (101); a compiler error or missing NRO",
                  "is not a reproduced failure. Compare its raw JSON against failure/sdl2-simple.oracle.json:",
                  "both engines must have zero queued frames, ExitProcess, PC 0x000000007ffe0004,",
                  "9,104,054 instructions and the same NRO digest. Preserve the new log and JSON.",
                  "A prior replay was reported in PR #158; its raw replay files were not retained here.",
                  "Fresh replay acceptance remains pending. See NOTICE.md for artifact identity and limitations.", ""]
        (staging / "REPLAY.md").write_text("\n".join(lines))
        staging.rename(output)
    return diagnosis, comparison


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path, default=EXAMPLE, help="retained example directory")
    parser.add_argument("--output", type=Path, required=True, help="new portable bundle directory")
    args = parser.parse_args()
    try:
        diagnosis, comparison = write_bundle(args.evidence, args.output)
    except (OSError, ValueError, KeyError, TypeError, IndexError, AttributeError, StopIteration) as error:
        parser.exit(2, f"ounce-verify walkthrough: {error}\n")
    print("Offline diagnosis: 2 engines, same recorded NRO and expectations; first divergence: frame 1.")
    print(f"Full retained comparison: {len(comparison['regressions'])} regressions from failure to reference.")
    print(f"Bundle: {args.output} (read REPLAY.md; fresh guest execution is still required).")


if __name__ == "__main__":
    main()
