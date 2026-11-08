#!/usr/bin/env python3
"""Reproduce every differential check in docs/verify/differential.json.

For each harness: fetch the pinned upstream source (git commit or sha256-checked tarball),
compile the shared C cases for the host (the reference) and, with --nro, for AArch64 with
devkitA64, then run the NRO on each Ounce-Core engine and compare results.txt with the host
output byte for byte. The committed expected-host.txt is also compared with the fresh host
output; harnesses marked arm64_reference only produce a meaningful reference on an ARM64 host.

A harness may carry "host_notes": {<host_id>: {"expected_different": [check names], "reason": ..., "evidence": ...}}
(host ids: apple-silicon, linux-arm64, ...; checks: committed-vs-fresh-host, tier3, tier4, native). On that host a listed
check that differs is reported "expected-different (documented)" and does not fail; any other difference fails; a listed
check that unexpectedly matches is reported with a warning. Nothing is hidden: the differing lines are still printed.

  python3 scripts/verify_differential.py [--only NAME ...] [--engines tier3 tier4 native]
         [--nro [--build-only [--nro-out DIR]]] [--nro-dir DIR] [--ounce target/release/ounce-core] [--work DIR] [--json OUT]

Exit status is non-zero if any compared output differs or a step fails.
"""
import argparse, hashlib, json, os, platform, shutil, subprocess, sys, tarfile, urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
VERIFY = REPO / "docs" / "verify"
MAKEFILE = "/opt/devkitpro/examples/switch/templates/application/Makefile"


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def diff_lines(a, b, a_name, b_name, limit=5):
    """First `limit` differing lines between two byte strings, as 'line N: a_name=... | b_name=...'."""
    if a is None or b is None:
        return [f"{a_name if a is None else b_name}: no output produced"]
    al, bl = a.decode("utf-8", "replace").splitlines(), b.decode("utf-8", "replace").splitlines()
    out = []
    for i in range(max(len(al), len(bl))):
        x = al[i] if i < len(al) else "<missing>"
        y = bl[i] if i < len(bl) else "<missing>"
        if x != y:
            out.append(f"line {i + 1}: {a_name}={x!r} | {b_name}={y!r}")
            if len(out) == limit:
                break
    return out


def host_id():
    m = platform.machine().lower()
    if sys.platform == "darwin" and m in ("arm64", "aarch64"):
        return "apple-silicon"
    if sys.platform.startswith("linux") and m in ("arm64", "aarch64"):
        return "linux-arm64"
    return f"{sys.platform}-{m}"


def upstream_complete(up, root):
    """True if `root` holds every file the harness copies (and, for git sources, the pinned commit)."""
    if not root.is_dir() or any(not (root / src).is_file() for src in up.get("copy", {})):
        return False
    if "commit" in up:
        r = run(["git", "-C", str(root), "rev-parse", "HEAD"])
        return r.returncode == 0 and r.stdout.strip() == up["commit"]
    return True


def fetch_upstream(up, cache):
    key = up.get("commit") or up["sha256"]
    dest = cache / key
    if dest.exists():
        if upstream_complete(up, dest):
            return dest
        print(f"upstream cache {dest} is incomplete; deleting and refetching", file=sys.stderr, flush=True)
        shutil.rmtree(dest, ignore_errors=True)
    tmp = cache / (key + ".tmp")
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    if "commit" in up:
        for step in (["git", "init", "-q", str(tmp)],
                     ["git", "-C", str(tmp), "fetch", "-q", "--depth", "1", up["repo"], up["commit"]],
                     ["git", "-C", str(tmp), "checkout", "-q", "-f", "FETCH_HEAD"]):
            r = run(step)
            if r.returncode:
                raise SystemExit(f"fetch failed for {up['repo']}@{up['commit']}: {r.stderr.strip()}")
        missing = [src for src in up.get("copy", {}) if not (tmp / src).is_file()]
        if missing:
            listing = run(["git", "-C", str(tmp), "status", "--short"]).stdout[:300]
            top = sorted(p.name for p in tmp.iterdir())[:40]
            raise SystemExit(f"checkout of {up['repo']}@{up['commit']} lacks {missing}; top-level={top}; status={listing!r}")
        got = run(["git", "-C", str(tmp), "rev-parse", "HEAD"]).stdout.strip()
        if got != up["commit"]:
            raise SystemExit(f"commit mismatch: wanted {up['commit']}, got {got}")
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
        missing = [src for src in up.get("copy", {}) if not (tmp / src).is_file()]
        if missing:
            raise SystemExit(f"tarball {up['tarball']} lacks {missing}")
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


def judge(row, name, same, h, host):
    """Record one comparison. A difference fails unless harness['host_notes'][host]['expected_different'] lists `name`
    (documented, evidence-backed); a documented difference that does not occur is a warning. Returns True if it fails."""
    note = (h.get("host_notes") or {}).get(host) or {}
    documented = name in note.get("expected_different", [])
    if same and not documented:
        row["checks"][name] = "identical"
        return False
    if same:
        row["checks"][name] = "identical (UNEXPECTED: documented as different)"
        row.setdefault("warnings", []).append(
            f"{name} is documented as different on {host} but matched; update host_notes ({note.get('evidence', 'no evidence given')})")
        return False
    if documented:
        row["checks"][name] = "expected-different (documented)"
        row.setdefault("documented", {})[name] = {"reason": note.get("reason"), "evidence": note.get("evidence")}
        return False
    row["checks"][name] = "DIFFERENT"
    return True


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--only", nargs="*")
    ap.add_argument("--engines", nargs="*", default=["tier3", "tier4", "native"])
    ap.add_argument("--nro", action="store_true", help="build NROs with devkitA64 (needs DEVKITPRO)")
    ap.add_argument("--build-only", action="store_true",
                    help="with --nro: only build the NROs (no host reference, no engines) and copy them to --nro-out")
    ap.add_argument("--nro-out", help="directory receiving <harness>.nro with --build-only (default: <work>/nros)")
    ap.add_argument("--nro-dir", help="use prebuilt NROs named <harness>.nro from this directory")
    ap.add_argument("--ounce", default=str(REPO / "target/release/ounce-core"))
    ap.add_argument("--work", default=str(REPO / "target/differential"))
    ap.add_argument("--budget", type=int, default=4_000_000_000)
    ap.add_argument("--json")
    a = ap.parse_args()
    if a.build_only and not a.nro:
        ap.error("--build-only requires --nro")

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
        if a.build_only:
            nro, err = build_nro(h, d, env)
            if err:
                row["error"] = err
                print(f"{h['name']}: {err}", file=sys.stderr, flush=True)
                failed = True
                continue
            out_dir = Path(a.nro_out or work / "nros")
            out_dir.mkdir(parents=True, exist_ok=True)
            shutil.copy(nro, out_dir / nro.name)
            row["checks"]["nro-build"] = "built"
            print(f"{h['name']}: built", flush=True)
            continue
        host_out, err = build_host(h, d)
        if err:
            row["error"] = err
            failed = True
            continue
        fresh = host_out.read_bytes()
        committed = (VERIFY / h["dir"] / "expected-host.txt").read_bytes()
        row.setdefault("diffs", {})
        if h.get("arm64_reference") and not arm_host:
            row["checks"]["committed-vs-fresh-host"] = "skipped (needs ARM64 host)"
        elif h.get("reference_hosts") and host_id() not in h["reference_hosts"]:
            row["checks"]["committed-vs-fresh-host"] = f"not enforced on {host_id()} (reference_hosts={h['reference_hosts']})"
            if fresh != committed:
                row["diffs"]["committed-vs-fresh-host"] = diff_lines(committed, fresh, "committed", "fresh-host")
        else:
            same = fresh == committed
            if not same:
                row["diffs"]["committed-vs-fresh-host"] = diff_lines(committed, fresh, "committed", "fresh-host")
            failed |= judge(row, "committed-vs-fresh-host", same, h, host_id())
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
            if got is not None:
                (d / f"engine-{e}-results.txt").write_bytes(got)
            ok = got == fresh
            if not ok:
                row["diffs"][e] = diff_lines(fresh, got, "host", e)
            failed |= judge(row, e, ok, h, host_id())
        print(f"{h['name']}: " + ", ".join(f"{k}={v}" for k, v in row["checks"].items()), flush=True)
        for k, lines in row.get("diffs", {}).items():
            for ln in lines:
                print(f"    [{k}] {ln}", flush=True)
        for k, n in row.get("documented", {}).items():
            print(f"    [{k}] documented: {n['reason']} (evidence: {n['evidence']})", flush=True)
        for w in row.get("warnings", []):
            print(f"    WARNING: {w}", flush=True)
    out = {"host": platform.machine(), "host_id": host_id(), "failed": failed, "harnesses": report}
    if a.json:
        Path(a.json).write_text(json.dumps(out, indent=1) + "\n")
    documented = sum(len(r.get("documented", {})) for r in report)
    print("FAILED" if failed else "all compared outputs identical"
          + (f" except {documented} documented host difference(s)" if documented else ""))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
