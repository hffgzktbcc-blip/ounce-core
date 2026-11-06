"""Store only registered homebrew binaries and their build inventories."""

import hashlib
import json
from pathlib import Path
import shutil
import tempfile


def digest(path):
    sha256 = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            sha256.update(chunk)
    return sha256.hexdigest()


def cache_key(image_id, build_recipe, manifests, oracles):
    inputs = {
        "image_id": image_id,
        "recipe": build_recipe,
        "manifests": {name: digest(path) for name, path in manifests.items()},
        "outputs": sorted((oracle["build"], oracle["nro"]) for oracle in oracles),
    }
    return hashlib.sha256(json.dumps(inputs, sort_keys=True).encode()).hexdigest()


def regular_file(root, relative):
    relative = Path(relative)
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError(f"invalid corpus path: {relative}")
    path = root / relative
    if not path.is_file() or path.resolve() != path:
        raise ValueError(f"missing or linked corpus file: {relative}")
    return path


def copy_verified_builds(source, destination, manifests, oracles):
    shutil.copyfile(regular_file(source, "toolchain-packages.txt"),
                    destination / "toolchain-packages.txt")
    inventories = {}
    for oracle in oracles:
        build = oracle["build"]
        if build not in inventories:
            relative = Path(build) / "build/inventory.json"
            inventory_path = regular_file(source, relative)
            inventory_bytes = inventory_path.read_bytes()
            inventory = json.loads(inventory_bytes)
            manifest_path = manifests[build]
            manifest = json.loads(manifest_path.read_bytes())
            if (inventory["manifest_sha256"] != digest(manifest_path)
                    or inventory["revision"] != manifest["revision"]
                    or inventory["repository"] != manifest["repository"]
                    or inventory["status"] != "built-not-executed"):
                raise ValueError(f"cached inventory does not match build manifest: {build}")
            inventories[build] = {
                nro: sha for sample in inventory["samples"]
                for nro, sha in sample["nro_sha256"].items()
            }
            output = destination / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(inventory_bytes)

        relative = Path(build) / "build/source" / oracle["nro"]
        binary = regular_file(source, relative)
        output = destination / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(binary, output)
        if digest(output) != inventories[build].get(oracle["nro"]):
            raise ValueError(f"cached NRO hash mismatch: {relative}")


def restore(entry, builds, manifests, oracles):
    if not entry.exists():
        return "miss"
    try:
        with tempfile.TemporaryDirectory(dir=builds.parent) as directory:
            staging = Path(directory)
            copy_verified_builds(entry, staging, manifests, oracles)
            staging.replace(builds)
    except (OSError, ValueError, KeyError, TypeError) as error:
        return f"rejected: {error}"
    return "hit"


def store(entry, builds, manifests, oracles):
    entry.parent.mkdir(parents=True, exist_ok=True)
    if entry.exists():
        return
    with tempfile.TemporaryDirectory(dir=entry.parent) as directory:
        staging = Path(directory)
        copy_verified_builds(builds, staging, manifests, oracles)
        try:
            staging.rename(entry)
        except OSError:
            if not entry.is_dir():
                raise
