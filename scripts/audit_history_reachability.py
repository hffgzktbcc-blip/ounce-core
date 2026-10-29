#!/usr/bin/env python3
"""Read-only Git ref reachability inventory for flagged historical commits.

Requires a complete local git clone. Does not inspect or export flagged blob bytes,
change refs, run git gc, or make any legal finding.
"""
import json
import subprocess
import sys

FLAGGED_COMMITS = {
    "c7f16f2": "c7f16f2576b69aeafb65bd872353eda74080e54a",
    "4be9640": "4be9640381850108d0e99da216b648a98512f149",
}


def git(*args, check=True):
    p = subprocess.run(
        ["git", *args], capture_output=True, text=True, check=False,
        timeout=120,
    )
    if check and p.returncode:
        raise RuntimeError(f"git {args[0]} failed: {p.stderr.strip()}")
    return p


def main():
    if git("rev-parse", "--is-inside-work-tree").stdout.strip() != "true":
        raise RuntimeError("Run from within a local git checkout")
    shallow = git("rev-parse", "--is-shallow-repository").stdout.strip() == "true"
    refs_raw = git("for-each-ref", "--format=%(refname)", "refs/heads",
                   "refs/remotes", "refs/tags", "refs/pull").stdout
    refs = sorted(set(line for line in refs_raw.splitlines() if line))
    data = {
        "schema": "ounce-history-reachability/v1",
        "shallow_clone": shallow,
        "ref_count": len(refs),
        "ref_scope": ["refs/heads", "refs/remotes", "refs/tags", "refs/pull"],
        "local_ref_snapshot_only": True,
        "flagged": [],
        "warnings": [
            "This does not enumerate every server-side ref, fork, release asset, archive or clone.",
            "Reachability is not a judgement about legality or specific blob contents.",
            "History removal would require separately authorised, coordinated ref rewriting.",
        ],
    }
    for label, commit in FLAGGED_COMMITS.items():
        exists = git("cat-file", "-e", f"{commit}^{{commit}}", check=False).returncode == 0
        containing = []
        if exists:
            for ref in refs:
                result = git("merge-base", "--is-ancestor", commit, ref, check=False)
                if result.returncode == 0:
                    containing.append(ref)
                elif result.returncode != 1:
                    raise RuntimeError(f"Could not test reachability for {ref}")
        data["flagged"].append({
            "label": label, "commit": commit,
            "object_available": exists, "reachable_from_local_refs": containing,
        })
    print(json.dumps(data, indent=2, sort_keys=True))
    return 0 if not shallow else 2


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (RuntimeError, subprocess.TimeoutExpired) as exc:
        print(f"History audit unavailable: {exc}", file=sys.stderr)
        sys.exit(2)
