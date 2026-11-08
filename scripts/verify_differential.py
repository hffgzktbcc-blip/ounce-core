#!/usr/bin/env python3
"""Reproduce every differential check in docs/verify/differential.json.

For each harness: fetch the pinned upstream source (git commit or sha256-checked tarball),
compile the shared C cases for the host (the reference) and, with --nro, for AArch64 with
devkitA64, then run the NRO on each Ounce-Core engine and compare results.txt with the host
output byte for byte. The committed expected-host.txt is also compared with the fresh host
output; harnesses marked arm64_reference only produce a meaningful reference on an ARM64 host.

  python3 scripts/verify_differential.py [--only NAME ...] [--engines tier3 tier4 native]
         [--nro] [--nro-dir DIR] [--ounce target/release/ounce-core] [--work DIR] [--json OUT]

Exit status is non-zero if any compared output differs or a step fails.
"""
import argparse, hashlib, json, os, platform, shutil, subprocess, sys, tarfile, urllib.request, zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
VERIFY = REPO / "docs" / "verify"
MAKEFILE = "/opt/devkitpro/examples/switch/templates/application/Makefile"


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def fetch_upstream(up, cache):
    key = up.get("commit") or up["sha256"]  # cache dir name
    dest = cache / key[:16]
    if dest.exists():
        return dest
    tmp = cache / (key[:16] + ".tmp")
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    if "commit" in up:
        for step in (["git", "init", "-q", str(tmp)],
                     ["git", "-C", str(tmp), "fetch", "-q", "--depth", "1", up["repo"], up["commit"]],
                     ["git", "-C", str(tmp), "checkout", "-q", "FETCH_HEAD"]):
            r = run(step)
            if r.returncode:
                raise SystemExit(f"fetch failed for {up['repo']}@{up['commit']}: {r.stderr.strip()}")
        got = run(["git", "-C", str(tmp), "rev-parse", "HEAD"]).stdout.strip()
        if got != up["commit"]:
            raise SystemExit(f"commit mismatch: wanted {up['commit']}, got {got}")
    elif "zip" in up:
        data = urllib.request.urlopen(up["zip"], timeout=300).read()
        digest = hashlib.sha256(data).hexdigest()
        if digest != up["sha256"]:
            raise SystemExit(f"zip sha256 mismatch for {up['zip']}: {digest}")
        archive = tmp / "src.zip"
        archive.write_bytes(data)
        with zipfile.ZipFile(archive) as z:
            for name in z.namelist():
                parts = Path(name).parts[up.get("strip", 0):]
                if name.endswith("/") or not parts or name.startswith("/") or ".." in parts:
                    continue
                target = tmp / Path(*parts)
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(z.read(name))
        archive.unlink()
    else:
        data = urllib.request.urlopen(up["tarball"], timeout=120).read()
        digest = hashlib.sha256(data).hexdigest()
        if digest != up["sha256"]:
            raise SystemExit(f"tarball sha256 mismatch for {up['tarball']}: {digest}")
        archive = tmp / "src.tgz"
        archive.write_bytes(data)
        with tarfile.open(archive) as t:
            for m in t.getmembers():
                parts = Path(m.name).parts[up.get("strip", 0):]
                if not parts or m.name.startswith("/") or ".." in parts:
                    continue
                m.name = str(Path(*parts))
                t.extract(m, tmp, **({"filter": "data"} if hasattr(tarfile, "data_filter") else {}))
        archive.unlink()
    tmp.rename(dest)
    return dest


def stage(h, work, cache):
    d = work / h["name"]
    shutil.rmtree(d, ignore_errors=True)
    d.mkdir(parents=True)
    src = VERIFY / h["dir"]
    for f in h["files"] + [h["nx_main"]]:
        shutil.copy(src / f, d / Path(f).name)
    (d / "host").mkdir()
    shutil.copy(src / h["host_main"], d / "host" / "host_main.c")
    up = h.get("upstream")
    if up:
        root = fetch_upstream(up, cache)
        for s, t in up["copy"].items():
            shutil.copy(root / s, d / t)
    return d


def build_host(h, d):
    cfiles = ["host/host_main.c"] + sorted(p.name for p in d.glob("*.c")
                                      if p.name != Path(h["nx_main"]).name)
    libs = h.get("libs", "") + (" " + h.get("libs_linux", "") if platform.system() == "Linux" else "")
    defs = ["-D" + x for x in h.get("defines", [])]
    cflags = h.get("cflags", "").split()
    r = run(["cc", "-O1", "-I.", *cflags, *defs, "-o", "host_ref", *cfiles, *libs.split()], cwd=d)
    if r.returncode:
        return None, "host build failed: " + r.stderr.strip()[-400:]
    r = run(["./host_ref"], cwd=d)
    if r.returncode:
        return None, f"host run failed ({r.returncode})"
    out = d / "host-output.txt"
    out.write_text(r.stdout)
    return out, None


def build_nro(h, d, env):
    shutil.copy(MAKEFILE, d / "Makefile")
    mk = (d / "Makefile").read_text()
    mk = mk.replace("TARGET\t\t:=\t$(notdir $(CURDIR))", f"TARGET := {h['name']}")
    mk = mk.replace("SOURCES\t\t:=\tsource", "SOURCES := .")
    if h.get("libs"):
        mk = mk.replace("LIBS\t:= -lnx", "LIBS := -lnx " + h["libs"])
    if h.get("cflags"):
        mk = mk.replace("-D__SWITCH__", "-D__SWITCH__ " + h["cflags"], 1)
    (d / "Makefile").write_text(mk)
    if h.get("defines"):
        old = "CFLAGS\t+=\t$(INCLUDE) -D__SWITCH__"
        assert old in mk, "template CFLAGS line changed"
        mk = mk.replace(old, old + " " + " ".join("-D" + x for x in h["defines"]))
        (d / "Makefile").write_text(mk)
    r = run(["make", "-j4"], cwd=d, env=env)
    nro = d / f"{h['name']}.nro"
    if r.returncode or not nro.exists():
        return None, "NRO build failed: " + (r.stdout + r.stderr).strip()[-400:]
    return nro, None


def run_engine(ounce, nro, engine, d, budget, result_file="results.txt"):
    sd = d / f"sd_{engine}"
    shutil.rmtree(sd, ignore_errors=True)
    sd.mkdir()
    env = dict(os.environ, OUNCE_NRO_BUDGET=str(budget), OUNCE_NRO_TIMEOUT_SECS="300")
    r = run([str(ounce), "run-nro", str(nro), "--present=headless", f"--sdmc={sd}", f"--cpu={engine}"], env=env)
    res = sd / result_file
    return (res.read_bytes() if res.exists() else None), r


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--only", nargs="*")
    ap.add_argument("--engines", nargs="*", default=["tier3", "tier4", "native"])
    ap.add_argument("--nro", action="store_true", help="build NROs with devkitA64 (needs DEVKITPRO)")
    ap.add_argument("--nro-dir", help="use prebuilt NROs named <harness>.nro from this directory")
    ap.add_argument("--ounce", default=str(REPO / "target/release/ounce-core"))
    ap.add_argument("--work", default=str(REPO / "target/differential"))
    ap.add_argument("--budget", type=int, default=4_000_000_000)
    ap.add_argument("--json")
    a = ap.parse_args()

    manifest = json.loads((VERIFY / "differential.json").read_text())
    work, cache = Path(a.work).resolve(), Path(a.work).resolve() / "_upstream"
    cache.mkdir(parents=True, exist_ok=True)
    arm_host = platform.machine().lower() in ("arm64", "aarch64")
    env = dict(os.environ, DEVKITPRO=os.environ.get("DEVKITPRO", "/opt/devkitpro"))
    env.setdefault("DEVKITA64", env["DEVKITPRO"] + "/devkitA64")
    env["PATH"] = env["DEVKITPRO"] + "/devkitA64/bin:" + env["PATH"]
    report, failed = [], False
    for h in manifest["harnesses"]:
        if a.only and h["name"] not in a.only:
            continue
        row = {"name": h["name"], "checks": {}}
        report.append(row)
        d = stage(h, work, cache)
        host_out, err = build_host(h, d)
        if err:
            row["error"] = err
            failed = True
            continue
        fresh = host_out.read_bytes()
        committed = (VERIFY / h["dir"] / "expected-host.txt").read_bytes()
        if h.get("arm64_reference") and not arm_host:
            row["checks"]["committed-vs-fresh-host"] = "skipped (needs ARM64 host)"
        else:
            same = fresh == committed
            row["checks"]["committed-vs-fresh-host"] = "identical" if same else "DIFFERENT"
            failed |= not same
        nro = None
        if a.nro_dir:
            nro = Path(a.nro_dir) / f"{h['name']}.nro"
            nro = nro if nro.exists() else None
        elif a.nro:
            nro, err = build_nro(h, d, env)
            if err:
                row["error"] = err
                failed = True
                continue
        if not nro:
            row["checks"]["engines"] = "skipped (no NRO)"
            continue
        row["nro_sha256"] = hashlib.sha256(nro.read_bytes()).hexdigest()
        for e in a.engines:
            got, r = run_engine(a.ounce, nro, e, d, a.budget, h.get("result_file", "results.txt"))
            ok = got == fresh
            row["checks"][e] = "identical" if ok else "DIFFERENT"
            failed |= not ok
        print(f"{h['name']}: " + ", ".join(f"{k}={v}" for k, v in row["checks"].items()), flush=True)
    out = {"host": platform.machine(), "failed": failed, "harnesses": report}
    if a.json:
        Path(a.json).write_text(json.dumps(out, indent=1) + "\n")
    print("FAILED" if failed else "all compared outputs identical")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
