#!/usr/bin/env python3
"""Build the registered homebrew corpus and retain bounded, replayable oracle evidence."""

import argparse
from contextlib import suppress
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import uuid

from run_corpus_oracles import REPO

IMAGE = "devkitpro/devkita64:latest"
BUILD = r"""
set -euo pipefail
apt-get update
apt-get install -y python3 git make zip
dkp-pacman -Q > /evidence/toolchain-packages.txt
for manifest in /repo/tests/corpus/manifest.json /repo/tests/corpus/oracle-builds/*.json; do
    id=$(basename "$manifest" .json)
    if [ "$id" = manifest ]; then id=original; fi
    python3 /repo/scripts/prepare_external_corpus.py --manifest "$manifest" \
        --output "/corpus/$id/build" --build > "/evidence/$id.prepare.log" 2>&1 \
        || echo "build of $id failed (see $id.prepare.log)"
    inventory="/corpus/$id/build/inventory.json"
    if [ -f "$inventory" ]; then cp "$inventory" "/evidence/$id.inventory.json"; fi
    for log in /corpus/$id/build/*.build.log; do
        if [ -f "$log" ]; then cp "$log" /evidence/; fi
    done
done
"""


def positive_seconds(value):
    seconds = int(value)
    if seconds < 1:
        raise argparse.ArgumentTypeError("limit must be a positive number of seconds")
    return seconds


def run(args):
    root = args.output.resolve()
    root.mkdir(parents=True)
    builds, evidence = root / "builds", root / "evidence"
    builds.mkdir()
    evidence.mkdir()
    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=REPO, text=True, timeout=10
    ).strip()
    corpus = REPO / "tests/corpus"
    inputs = [corpus / name for name in ("oracles.json", "oracle-results.json", "manifest.json")]
    inputs.extend(sorted((corpus / "oracle-builds").glob("*.json")))
    oracles = json.loads(inputs[0].read_text())["oracles"]
    manifest = {
        "schema_version": "ounce-verify/run-v1",
        "core_commit": commit,
        "cwd": str(REPO),
        "inputs_sha256": {
            str(path.relative_to(REPO)): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs
        },
        "seconds_per_engine": args.engine_seconds,
        "stages": [],
    }
    manifest_path = evidence / "run-manifest.json"

    def save_manifest():
        temp = manifest_path.with_suffix(".tmp")
        temp.write_text(json.dumps(manifest, indent=2) + "\n")
        temp.replace(manifest_path)

    def stage(name, command, seconds, output=None):
        output = output or evidence / f"{name}.log"
        command = list(map(str, command))
        entry = {
            "name": name, "argv": command, "timeout_seconds": seconds,
            "stdout": str(output.relative_to(evidence)), "stderr": f"{name}.stderr.log",
            "status": "running",
        }
        manifest["stages"].append(entry)
        save_manifest()
        print(f"Ounce-Verify: {name} (limit {seconds}s)", flush=True)
        try:
            with output.open("wb") as out, (evidence / entry["stderr"]).open("wb") as err:
                with subprocess.Popen(command, cwd=REPO, stdout=out, stderr=err,
                                      start_new_session=True) as process:
                    try:
                        status = process.wait(timeout=seconds)
                    except (subprocess.TimeoutExpired, KeyboardInterrupt):
                        with suppress(ProcessLookupError):
                            os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
                        raise
            entry.update(status="exited", returncode=status)
            return status
        except subprocess.TimeoutExpired:
            entry["status"] = "timeout"
            raise
        except KeyboardInterrupt:
            entry["status"] = "interrupted"
            raise
        except OSError:
            entry["status"] = "launch-error"
            raise
        finally:
            save_manifest()

    def require(name, command, seconds, output=None):
        if stage(name, command, seconds, output):
            raise ValueError(f"{name} failed; see {evidence}")

    require("pull", ["docker", "pull", args.image], args.build_seconds)
    toolchain = evidence / "toolchain-image.json"
    require("inspect", ["docker", "image", "inspect", args.image], 30, toolchain)
    image_id = json.loads(toolchain.read_text())[0]["Id"]
    container = f"ounce-verify-{uuid.uuid4().hex}"
    try:
        require("build", ["docker", "run", "--rm", "--name", container,
                          "-v", f"{REPO}:/repo:ro", "-v", f"{builds}:/corpus",
                          "-v", f"{evidence}:/evidence", image_id, "bash", "-c", BUILD],
                args.build_seconds)
    except (subprocess.TimeoutExpired, KeyboardInterrupt):
        stage("cleanup", ["docker", "rm", "-f", container], 30)
        raise

    messages = evidence / "cargo-artifacts.jsonl"
    require("compile", ["cargo", "test", "--release", "--locked", "corpus_first_frame_oracle",
                        "--no-run", "--message-format=json"], args.compile_seconds, messages)
    paths = set()
    for line in messages.read_text().splitlines():
        item = json.loads(line)
        if (item.get("reason") == "compiler-artifact" and item.get("profile", {}).get("test")
                and item.get("target", {}).get("name") == "ounce-core" and item.get("executable")):
            paths.add(item["executable"])
    if len(paths) != 1:
        raise ValueError(f"expected one oracle executable, found {sorted(paths)}")

    python = sys.executable
    verdicts = evidence / "oracle-verdicts.json"
    # Leave time for every per-case outer timeout plus runner bookkeeping.
    run_seconds = len(oracles) * (2 * args.engine_seconds + 120) + 60
    runner_status = stage("oracles", [python, "scripts/run_corpus_oracles.py", "--executable", paths.pop(),
                                     "--builds", builds, "--evidence", evidence,
                                     "--seconds", args.engine_seconds], run_seconds)
    if not verdicts.is_file():
        raise ValueError(f"oracle runner produced no observations; see {evidence}")

    records, report = evidence / "ounce-verify-evidence.json", evidence / "canonical-report.json"
    require("evidence", [python, "scripts/corpus_verify.py", "evidence", "--verdicts", verdicts,
                         "--builds", builds, "--toolchain-image", toolchain,
                         "--core-commit", commit, "--output", records], 60)
    require("verify", [python, "tools/ounce_verify.py", records, "--artifact-root", builds,
                       "--output", report], 60)
    headline = [python, "scripts/corpus_verify.py", "headline", report]
    if args.check_readme:
        headline.append("--check-readme")
    headline_status = stage("headline", headline, 60)
    summary = (evidence / "headline.log").read_text()
    print(summary, end="")
    if summary and os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as stream:
            stream.write(summary)
    return runner_status or headline_status


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="new directory for builds and evidence")
    parser.add_argument("--image", default=IMAGE, help="devkitA64 tag or previously recorded image ID")
    parser.add_argument("--build-seconds", type=positive_seconds, default=1800, help="limit per Docker stage")
    parser.add_argument("--compile-seconds", type=positive_seconds, default=1200)
    parser.add_argument("--engine-seconds", type=positive_seconds, default=180)
    parser.add_argument("--check-readme", action="store_true")
    args = parser.parse_args()
    try:
        return run(args)
    except subprocess.TimeoutExpired as error:
        parser.exit(1, f"ounce-verify: stage timed out after {error.timeout}s; see {args.output / 'evidence'}\n")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"ounce-verify: {error}; see {args.output / 'evidence'}\n")


if __name__ == "__main__":
    sys.exit(main())
