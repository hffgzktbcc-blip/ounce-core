#!/usr/bin/env python3
"""Render an evidence-gated roadmap from docs/roadmap/milestones.json.

Only observational PR status may be refreshed from GitHub. A merged PR does not
promote a research stage or certify an oracle. The certified corpus count remains
owned by scripts/corpus_verify.py.
"""
import argparse
import json
import os
from pathlib import Path
import re
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "docs/roadmap/milestones.json"
README = ROOT / "README.md"
START = "## The road ahead\n"
END = "## Sources and porting policy\n"
HEADLINE = re.compile(r"<!-- corpus-headline -->(\d+/\d+)<!-- /corpus-headline -->")
STAGES = {"implemented-subset", "measured", "v6-merged", "experimental", "local-preview"}
PR_STATES = {"open", "closed", "merged", "unknown"}


def load_registry():
    data = json.loads(REGISTRY.read_text(encoding="utf-8"))
    if data.get("schema_version") != 1 or not isinstance(data.get("tracks"), list):
        raise ValueError("unsupported roadmap schema")
    ids = set()
    for row in data["tracks"]:
        if not all(isinstance(row.get(k), str) and row[k] for k in
                   ("id", "name", "stage", "evidence", "evidence_url", "next")):
            raise ValueError("missing milestone fields")
        if row["id"] in ids or row["stage"] not in STAGES:
            raise ValueError("duplicate milestone or unsupported stage")
        ids.add(row["id"])
        for field in ("name", "evidence", "evidence_url", "next"):
            if any(x in row[field] for x in ("|", "\n", "\r")):
                raise ValueError("unsafe markdown table content")
        if "pr" in row:
            if not isinstance(row["pr"], int) or row["pr"] <= 0:
                raise ValueError("invalid PR number")
            if row.get("pr_status", "unknown") not in PR_STATES:
                raise ValueError("invalid observational PR status")
    if sum(bool(r.get("corpus_headline")) for r in data["tracks"]) != 1:
        raise ValueError("exactly one corpus headline is required")
    return data


def render(data, corpus):
    out = [
        START.rstrip("\n"), "",
        "*" + data["principle"] + "*", "",
        "<!-- BEGIN GENERATED ROADMAP: scripts/render_roadmap.py -->",
        "This section is rendered from [the milestone registry](docs/roadmap/milestones.json). "
        "Research stages require reviewed evidence; PR status is observational only. "
        "A green badge or merged PR is not proof of compatibility.",
        "",
        "| Live check | Current GitHub status | Evidence boundary |",
        "| :--- | :--- | :--- |",
        "| CI | [![CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/ci.yml?query=branch%3Amain) | Configured test jobs only |",
        "| External corpus | [![Corpus](https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/external-corpus.yml/badge.svg?branch=main)](https://github.com/hffgzktbcc-blip/ounce-core/actions/workflows/external-corpus.yml?query=branch%3Amain) | Oracle pipeline health, not a new pass by itself |",
        "",
        "| Research track | Reviewed stage | Supporting evidence | Next proof gate |",
        "| :--- | :--- | :--- | :--- |",
    ]
    for row in data["tracks"]:
        evidence = "[" + row["evidence"] + "](" + row["evidence_url"] + ")"
        if row.get("corpus_headline"):
            evidence += " — **<!-- corpus-headline -->" + corpus + "<!-- /corpus-headline --> passes** (CI-certified)"
        if row.get("pr"):
            evidence += " (PR #" + str(row["pr"]) + ": " + row.get("pr_status", "unknown") + ", observational)"
        out.append("| " + row["name"] + " | " + row["stage"] + " | " + evidence + " | " + row["next"] + " |")
    out.extend([
        "",
        "**Live navigation:** [open PRs](https://github.com/hffgzktbcc-blip/ounce-core/pulls?q=is%3Apr+is%3Aopen)"
        " · [open issues](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+is%3Aopen)"
        " · [Actions](https://github.com/hffgzktbcc-blip/ounce-core/actions)"
        " · [verification method](docs/verify/README.md) · [PLAN.md](PLAN.md).",
        "",
        "No general homebrew, commercial-game or Switch 2 compatibility claim is implied. "
        "The corpus count is independently guarded by [the canonical headline checker](scripts/corpus_verify.py).",
        "<!-- END GENERATED ROADMAP -->",
        "",
        "---",
        "",
    ])
    return "\n".join(out) + "\n"



ARCH_REGISTRY = ROOT / "docs/architecture/layers.json"
ARCH_START = "### The architecture at a glance\n"
ARCH_END = "<details>\n"


def render_architecture():
    data = json.loads(ARCH_REGISTRY.read_text(encoding="utf-8"))
    if data.get("schema_version") != 1 or not isinstance(data.get("layers"), list):
        raise ValueError("invalid architecture registry")
    lines = [
        "### The architecture at a glance", "",
        "<!-- BEGIN GENERATED ARCHITECTURE: scripts/render_roadmap.py -->",
        "Implemented code paths are not universal compatibility proof. Source links identify implementation, not certification.",
        "", "| Layer | Current implementation | Important boundary | Source |",
        "| :--- | :--- | :--- | :--- |",
    ]
    seen = set()
    for row in data["layers"]:
        for key in ("id", "name", "capability", "boundary"):
            if not isinstance(row.get(key), str) or not row[key] or any(
                sep in row[key] for sep in ("|", "\n", "\r")
            ):
                raise ValueError("invalid architecture field " + key)
        if row["id"] in seen:
            raise ValueError("duplicate architecture layer")
        seen.add(row["id"])
        sources = row.get("sources")
        if not isinstance(sources, list) or not sources:
            raise ValueError("architecture layer must have sources")
        for path in sources:
            if (not isinstance(path, str) or not path.startswith(("src/", "crates/", "scripts/", "tools/"))
                    or ".." in path or not (ROOT / path).is_file()):
                raise ValueError("missing or unsafe architecture source: " + str(path))
        links = ", ".join("[" + path.rsplit("/", 1)[-1] + "](" + path + ")" for path in sources[:2])
        lines.append("| **" + row["name"] + "** | " + row["capability"] +
                     " | " + row["boundary"] + " | " + links + " |")
    lines.extend(["", "<!-- END GENERATED ARCHITECTURE -->", ""])
    return "\n".join(lines) + "\n"


def refresh_architecture(readme):
    if readme.count(ARCH_START) != 1:
        raise ValueError("architecture heading not unique")
    before, rest = readme.split(ARCH_START, 1)
    if ARCH_END not in rest:
        raise ValueError("architecture section end missing")
    _, after = rest.split(ARCH_END, 1)
    return before + render_architecture() + ARCH_END + after


def sync_prs(data):
    changed = False
    for row in data["tracks"]:
        if "pr" not in row:
            continue
        url = f"https://api.github.com/repos/hffgzktbcc-blip/ounce-core/pulls/{row['pr']}"
        headers = {"Accept": "application/vnd.github+json", "User-Agent": "ounce-roadmap-bot"}
        if os.getenv("GITHUB_TOKEN"):
            headers["Authorization"] = "Bearer " + os.environ["GITHUB_TOKEN"]
        with urlopen(Request(url, headers=headers), timeout=12) as response:
            item = json.load(response)
        state = "merged" if item.get("merged_at") else item["state"]
        if state not in PR_STATES:
            raise ValueError("unexpected GitHub PR state")
        if row.get("pr_status") != state:
            row["pr_status"] = state
            changed = True
    if changed:
        REGISTRY.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return changed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if README differs from generated roadmap")
    parser.add_argument("--sync-prs", action="store_true", help="fetch PR state only, never advance reviewed stage")
    args = parser.parse_args()
    if args.check and args.sync_prs:
        parser.error("--check and --sync-prs are mutually exclusive")
    data = load_registry()
    if args.sync_prs:
        sync_prs(data)
    original = README.read_text(encoding="utf-8")
    if original.count(START) != 1 or original.count(END) != 1:
        raise ValueError("README roadmap boundaries must be unique")
    match = HEADLINE.search(original[:original.index(START)])
    if not match:
        raise ValueError("CI-managed corpus headline missing")
    left, tail = original.split(START, 1)
    _, right = tail.split(END, 1)
    expected = refresh_architecture(left + render(data, match.group(1)) + END + right)
    if args.check:
        if original != expected:
            raise SystemExit("README roadmap stale: run python3 scripts/render_roadmap.py")
    else:
        README.write_text(expected, encoding="utf-8")
    print("roadmap generated and checked" if args.check else "roadmap generated")


if __name__ == "__main__":
    main()
