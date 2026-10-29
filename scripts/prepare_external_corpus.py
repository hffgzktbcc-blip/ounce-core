#!/usr/bin/env python3
"""Prepare pinned external sources; optionally build them. Never tests compatibility."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

REPO = Path(__file__).resolve().parents[1]


def run(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd, text=True, stderr=subprocess.STDOUT).strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(manifest):
    if not isinstance(manifest, dict) or manifest.get("schema_version", 1) != 1:
        raise ValueError("manifest must be a schema_version 1 object")
    repository = manifest.get("repository", "")
    if not isinstance(repository, str) or not repository or repository.startswith("-"):
        raise ValueError("repository must be a nonempty URL or local git path")
    revision = manifest.get("revision", "")
    if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("revision must be an exact lowercase 40-hex commit")
    samples = manifest.get("samples", [])
    if not isinstance(samples, list) or not samples:
        raise ValueError("samples must be a nonempty list")
    ids = set()
    for sample in samples:
        if not isinstance(sample, dict):
            raise ValueError("each sample must be an object")
        sid, path = sample.get("id", ""), sample.get("path", "")
        if not isinstance(sid, str) or not re.fullmatch(r"[a-z0-9][a-z0-9_-]*", sid) or sid in ids:
            raise ValueError("sample ids must be unique safe names")
        ids.add(sid)
        if not isinstance(path, str) or not path or "\\" in path or any(
            p in ("", ".", "..") for p in path.split("/")
        ) or PurePosixPath(path).is_absolute():
            raise ValueError("sample paths must be plain relative directories")
    return manifest


def checked_source(source, revision):
    if run("git", "rev-parse", "HEAD", cwd=source) != revision:
        raise ValueError("checkout HEAD differs from pinned revision")
    if run("git", "status", "--porcelain", "--untracked-files=no", cwd=source):
        raise ValueError("tracked source files are modified")


def prepare(manifest_path, output, build=False):
    manifest_path = Path(manifest_path).resolve()
    manifest = validate(json.loads(manifest_path.read_text(encoding="utf-8")))
    output = Path(output).resolve()
    if output == REPO or REPO in output.parents:
        raise ValueError("output must be outside the Ounce-Core checkout")
    if output.exists() and any(output.iterdir()):
        raise ValueError("output must be a new or empty directory; existing files are never deleted")
    devkit = os.environ.get("DEVKITPRO")
    if build and (not devkit or not Path(devkit).is_dir()):
        raise ValueError("--build requires DEVKITPRO pointing to an installed toolchain")
    output.mkdir(parents=True, exist_ok=True)
    source = output / "source"
    run("git", "clone", "--no-checkout", "--", manifest["repository"], str(source))
    run("git", "checkout", "--detach", manifest["revision"], cwd=source)
    checked_source(source, manifest["revision"])
    # Inventory every tracked source/build input, including shared root Makefiles.
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=source).decode().split("\0")
    hashes = {}
    for name in filter(None, tracked):
        path = source / name
        if path.is_symlink() or not path.resolve().is_relative_to(source.resolve()):
            raise ValueError("source symlinks are not supported")
        if path.is_file():
            hashes[name] = digest(path)
    inventory = {
        "schema_version": 1,
        "status": "built-not-executed" if build else "prepared-not-built",
        "compatibility": "not-tested",
        "repository": manifest["repository"], "revision": manifest["revision"],
        "manifest_sha256": digest(manifest_path), "source_sha256": hashes,
        "tools": {"python": sys.version, "git": run("git", "--version")},
        "samples": [],
    }
    if build:
        inventory["tools"]["make"] = run("make", "--version").splitlines()[0]
        compiler = Path(devkit) / "devkitA64/bin/aarch64-none-elf-gcc"
        inventory["tools"]["compiler"] = run(str(compiler), "--version").splitlines()[0]
        inventory["tools"]["compiler_sha256"] = digest(compiler)
        inventory["toolchain_inputs_sha256"] = {}
        for relative in ("libnx/switch_rules", "libnx/switch.specs", "libnx/switch.ld", "libnx/lib/libnx.a"):
            path = Path(devkit) / relative
            if path.is_file():
                inventory["toolchain_inputs_sha256"][relative] = digest(path)
    for sample in manifest["samples"]:
        path = source / sample["path"]
        if not path.is_dir() or not path.resolve().is_relative_to(source.resolve()):
            raise ValueError("sample directory missing or outside source checkout")
        entry = {"id": sample["id"], "path": sample["path"], "nro_sha256": {}}
        if build:
            checked_source(source, manifest["revision"])
            if list(path.rglob("*.nro")):
                raise ValueError("preexisting NRO cannot be recorded as a fresh build")
            log = run("make", "-C", str(path))
            (output / (sample["id"] + ".build.log")).write_text(log + "\n", encoding="utf-8")
            checked_source(source, manifest["revision"])
            for nro in sorted(path.rglob("*.nro")):
                if nro.is_symlink() or not nro.resolve().is_relative_to(source.resolve()):
                    raise ValueError("build output escapes checkout")
                entry["nro_sha256"][nro.relative_to(source).as_posix()] = digest(nro)
            if not entry["nro_sha256"]:
                raise ValueError("build produced no NRO")
        inventory["samples"].append(entry)
    (output / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
    return inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=REPO / "tests/corpus/manifest.json")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--build", action="store_true", help="execute upstream Makefiles with installed DEVKITPRO")
    args = parser.parse_args()
    try:
        inventory = prepare(args.manifest, args.output, args.build)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Preparation failed: {error}\n")
    print(f"{inventory['status']}; compatibility not tested. Inventory: {args.output / 'inventory.json'}")


if __name__ == "__main__":
    main()
