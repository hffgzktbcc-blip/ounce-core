#!/usr/bin/env python3
"""Package a built "Ounce Core.app" as a local, checksummed developer preview.

Writes into a fresh output directory:
  Ounce-Core-<version>-macos-preview.tar.gz
  Ounce-Core-<version>-macos-preview.zip     (macOS only; made with ditto)
  MANIFEST.json   revision, version, features, toolchains, per-file SHA-256
  SHA256SUMS

This is not a release. It never uploads anything, and the manifest records that
the bundle is ad-hoc signed, not notarized, and not cleared for public
distribution (docs/PROVENANCE.md, Specialist items).
"""
import argparse
import hashlib
import json
import platform
import re
import subprocess
import sys
import tarfile
from pathlib import Path

FEATURES = "gpu-present nce native-jit"
REQUIRED = [
    "Contents/Info.plist",
    "Contents/MacOS/OunceCore",
    "Contents/Helpers/Ounce Runtime.app/Contents/MacOS/ounce-core",
    "Contents/Resources/Licenses/LICENSE",
    "Contents/Resources/Licenses/THIRD_PARTY.md",
    "Contents/Resources/Licenses/DejaVu-LICENSE.txt",
    "Contents/Resources/Verify/differential.json",
]


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(*cmd, root):
    try:
        return subprocess.check_output(cmd, cwd=root, text=True, stderr=subprocess.STDOUT).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def crate_version(root):
    match = re.search(r'^version\s*=\s*"([^"]+)"', (root / "Cargo.toml").read_text(), re.M)
    if not match:
        raise ValueError("Cargo.toml has no package version")
    return match[1]


def bundle_files(app):
    files = {}
    for path in sorted(app.rglob("*")):
        if path.is_file() and not path.is_symlink():
            files[path.relative_to(app).as_posix()] = sha256(path)
    return files


def check_bundle(app):
    if app.suffix != ".app" or not app.is_dir():
        raise ValueError(f"Not an .app bundle: {app}")
    missing = [name for name in REQUIRED if not (app / name).is_file()]
    if missing:
        raise ValueError("Bundle is missing: " + ", ".join(missing))


def package(app, out, root):
    check_bundle(app)
    if out.exists():
        raise ValueError(f"Choose a fresh output directory: {out}")
    out.mkdir(parents=True)
    version = crate_version(root)
    stem = f"Ounce-Core-{version}-macos-preview"
    archives = []

    tar_path = out / f"{stem}.tar.gz"
    with tarfile.open(tar_path, "w:gz") as tar:
        tar.add(app, arcname=app.name)
    archives.append(tar_path)

    if platform.system() == "Darwin":
        zip_path = out / f"{stem}.zip"
        subprocess.check_call(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(zip_path)])
        archives.append(zip_path)

    manifest = {
        "schema_version": 1,
        "kind": "local developer preview, not a release",
        "version": version,
        "git_revision": run("git", "rev-parse", "HEAD", root=root),
        "git_dirty": run("git", "status", "--porcelain", root=root) != "",
        "cargo_features": FEATURES,
        "rustc": run("rustc", "--version", root=root),
        "swiftc": run("swiftc", "--version", root=root).splitlines()[0] if platform.system() == "Darwin" else "not run",
        "host": platform.platform(),
        "signing": "ad-hoc (codesign --sign -); not notarized; Gatekeeper will warn on first open",
        "distribution": "Not cleared for public release or GitHub release assets: "
                        "docs/PROVENANCE.md Specialist items (objc2/Apple SDK terms) is open.",
        "archives": {a.name: sha256(a) for a in archives},
        "bundle_files": bundle_files(app),
    }
    (out / "MANIFEST.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    sums = [(sha256(p), p.name) for p in archives + [out / "MANIFEST.json"]]
    (out / "SHA256SUMS").write_text("".join(f"{h}  {n}\n" for h, n in sums))
    return manifest


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--app", type=Path, default=Path("dist/Ounce Core.app"))
    parser.add_argument("--out", type=Path, default=Path("dist/macos-preview"))
    args = parser.parse_args(argv)
    root = Path(__file__).resolve().parent.parent
    try:
        manifest = package(args.app, args.out, root)
    except (ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"Packaged {len(manifest['archives'])} archive(s) in {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
