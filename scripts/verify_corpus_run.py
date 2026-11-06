#!/usr/bin/env python3
"""Build the registered homebrew corpus and retain bounded, replayable oracle evidence."""

import argparse
from contextlib import suppress
from math import ceil
import hashlib
import json
import os
import platform
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import uuid

import corpus_build_cache
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


# Docker reports both transient registry failures and permanent errors as exit 1.
TRANSIENT_PULL_ERRORS = (
    b"connection refused", b"connection reset by peer", b"i/o timeout",
    b"temporary failure in name resolution", b"tls handshake timeout",
    b"502 bad gateway", b"503 service unavailable", b"504 gateway timeout",
)


def pull_image(stage, image, seconds, evidence, *, delay=time.sleep):
    command = ["docker", "pull", image]
    for attempt in range(1, 4):
        name = "pull" if attempt == 1 else f"pull-retry-{attempt - 1}"
        try:
            if stage(name, command, seconds) == 0:
                return
        except subprocess.TimeoutExpired:
            pass  # stage has already killed and reaped the pull process.
        else:
            with (evidence / f"{name}.stderr.log").open("rb") as log:
                log.seek(max(0, os.fstat(log.fileno()).st_size - 65536))
                diagnostic = log.read().lower()
            if not any(message in diagnostic for message in TRANSIENT_PULL_ERRORS):
                break
        if attempt < 3:
            delay(2 ** attempt)
    raise ValueError(f"pull failed; see {evidence}")


def positive_seconds(value):
    seconds = int(value)
    if seconds < 1:
        raise argparse.ArgumentTypeError("limit must be a positive number of seconds")
    return seconds


def timing_host():
    cpuinfo = Path("/proc/cpuinfo")
    image = os.environ.get("ImageOS")
    if not image or not cpuinfo.is_file():
        return None
    model = next((line.partition(":")[2].strip() for line in cpuinfo.read_text().splitlines()
                  if line.startswith("model name")), None)
    if model is None:
        return None
    return {"image": image, "architecture": platform.machine(), "cpu_model": model,
            "logical_cpus": os.cpu_count()}


def run(args):
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=args.resume)
    builds, evidence = root / "builds", root / "evidence"
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
        "oracle_shards": args.shards,
        "timing_host": timing_host(),
        "image_reference": args.image,
        "runner_sha256": corpus_build_cache.digest(Path(__file__)),
        "resumable": args.resume,
        "stages": [],
    }
    manifest_path = evidence / "run-manifest.json"

    continuing = args.resume and manifest_path.is_file()
    attempt = 1
    if continuing:
        previous = json.loads(manifest_path.read_text())
        for key, value in manifest.items():
            if key != "stages" and previous.get(key) != value:
                raise ValueError(f"cannot resume after changing {key}")
        toolchain_hash = previous.get("prepared_toolchain_sha256")
        if not toolchain_hash or corpus_build_cache.digest(evidence / "toolchain-image.json") != toolchain_hash:
            raise ValueError("no complete build snapshot to resume")
        manifest = previous
        attempt = manifest.get("attempt", 1) + 1
    else:
        builds.mkdir()
        evidence.mkdir()
    manifest["attempt"] = attempt

    def save_manifest():
        temp = manifest_path.with_suffix(".tmp")
        temp.write_text(json.dumps(manifest, indent=2) + "\n")
        temp.replace(manifest_path)

    def stage(name, command, seconds, output=None):
        if attempt > 1:
            name = f"{name}-attempt-{attempt}"
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

    toolchain = evidence / "toolchain-image.json"
    build_manifests = {"original": corpus / "manifest.json"}
    build_manifests.update({path.stem: path for path in (corpus / "oracle-builds").glob("*.json")})
    if continuing:
        with tempfile.TemporaryDirectory(dir=root) as directory:
            corpus_build_cache.copy_verified_builds(builds, Path(directory), build_manifests, oracles)
        for name in ("oracle-verdicts.json", "ounce-verify-evidence.json", "canonical-report.json"):
            (evidence / name).unlink(missing_ok=True)
        save_manifest()
        print(f"Ounce-Verify: resuming oracle execution (attempt {attempt})", flush=True)
    else:
        pull_image(stage, args.image, args.build_seconds, evidence)
        require("inspect", ["docker", "image", "inspect", args.image], 30, toolchain)
        image_id = json.loads(toolchain.read_text())[0]["Id"]
        entry = None
        if args.build_cache is not None:
            recipe = [BUILD, corpus_build_cache.digest(REPO / "scripts/prepare_external_corpus.py"),
                      corpus_build_cache.digest(Path(corpus_build_cache.__file__).resolve())]
            key = corpus_build_cache.cache_key(image_id, recipe, build_manifests, oracles)
            entry = args.build_cache.resolve() / key
            result = corpus_build_cache.restore(entry, builds, build_manifests, oracles)
            manifest["build_cache"] = {"key": key, "result": result}
            save_manifest()
            print(f"Ounce-Verify: build cache {result} ({key})", flush=True)
        if manifest.get("build_cache", {}).get("result") == "hit":
            (evidence / "toolchain-packages.txt").write_bytes((builds / "toolchain-packages.txt").read_bytes())
            for build in {oracle["build"] for oracle in oracles}:
                (evidence / f"{build}.inventory.json").write_bytes(
                    (builds / build / "build/inventory.json").read_bytes())
        else:
            container = f"ounce-verify-{uuid.uuid4().hex}"
            try:
                require("build", ["docker", "run", "--rm", "--name", container,
                                  "-v", f"{REPO}:/repo:ro", "-v", f"{builds}:/corpus",
                                  "-v", f"{evidence}:/evidence", image_id, "bash", "-c", BUILD],
                        args.build_seconds)
            except (subprocess.TimeoutExpired, KeyboardInterrupt):
                stage("cleanup", ["docker", "rm", "-f", container], 30)
                raise
            if entry is not None:
                try:
                    (builds / "toolchain-packages.txt").write_bytes((evidence / "toolchain-packages.txt").read_bytes())
                    corpus_build_cache.store(entry, builds, build_manifests, oracles)
                except (OSError, ValueError, KeyError, TypeError) as error:
                    manifest["build_cache"]["store_error"] = str(error)
                    save_manifest()
                    print(f"Ounce-Verify: could not store build cache: {error}", flush=True)
        if args.resume:
            (builds / "toolchain-packages.txt").write_bytes((evidence / "toolchain-packages.txt").read_bytes())
            manifest["prepared_toolchain_sha256"] = corpus_build_cache.digest(toolchain)
            save_manifest()

    messages = evidence / ("cargo-artifacts.jsonl" if attempt == 1 else f"cargo-artifacts-{attempt}.jsonl")
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
    run_seconds = ceil(len(oracles) / args.shards) * (2 * args.engine_seconds + 120) + 60
    oracle_command = [python, "scripts/run_corpus_oracles.py", "--executable", paths.pop(),
                      "--builds", builds, "--evidence", evidence,
                      "--seconds", args.engine_seconds, "--shards", args.shards]
    if args.resume:
        oracle_command.append("--resume")
    runner_status = stage("oracles", oracle_command, run_seconds)
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
    headline_log = "headline.log" if attempt == 1 else f"headline-attempt-{attempt}.log"
    summary = (evidence / headline_log).read_text()
    print(summary, end="")
    if summary and os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as stream:
            stream.write(summary)
    return runner_status or headline_status


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="directory for builds and evidence")
    parser.add_argument("--resume", action="store_true", help="resume oracle execution after builds have completed")
    parser.add_argument("--build-cache", type=Path, help="optional directory for verified NRO build snapshots")
    parser.add_argument("--image", default=IMAGE, help="devkitA64 tag or previously recorded image ID")
    parser.add_argument("--build-seconds", type=positive_seconds, default=1800, help="limit per Docker stage")
    parser.add_argument("--compile-seconds", type=positive_seconds, default=1200)
    parser.add_argument("--engine-seconds", type=positive_seconds, default=180)
    parser.add_argument("--shards", type=positive_seconds, default=1, help="concurrent oracle shards")
    parser.add_argument("--check-readme", action="store_true")
    args = parser.parse_args()
    try:
        if args.resume:
            import fcntl
            args.output.mkdir(parents=True, exist_ok=True)
            with (args.output / ".run.lock").open("a") as lock:
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    parser.error("another resumable invocation is using this output directory")
                return run(args)
        return run(args)
    except subprocess.TimeoutExpired as error:
        parser.exit(1, f"ounce-verify: stage timed out after {error.timeout}s; see {args.output / 'evidence'}\n")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"ounce-verify: {error}; see {args.output / 'evidence'}\n")


if __name__ == "__main__":
    sys.exit(main())
