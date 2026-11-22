#!/usr/bin/env python3
"""Require a public-docs decision alongside user-visible changes in a pull request.

Runs without dependencies or network access. Internal-only changes may use a
reasoned "Docs: none — ..." in the PR body. It does not claim to prove prose
correctness; reviewers still verify commands, claims and evidence.
"""
import os
import re
import subprocess
import sys

PUBLIC_PREFIXES = {
    "ounce-core": ("src/", "crates/", "tools/", "site/"),
    "ounce-hb": ("src/",),
    "ounce-runtime": ("src/", "crates/"),
    "deckstream": ("daemon/", "api/", "decky/", "site/"),
}
PUBLIC_ROOT_FILES = {
    "ounce-core": {"Cargo.toml"},
    "ounce-hb": {"Cargo.toml"},
    "ounce-runtime": {"Cargo.toml"},
    "deckstream": {"package.json"},
}
DOCS_PREFIXES = ("docs/",)
DOCS_FILES = {"README.md", "CHANGELOG.md", "CONTRIBUTING.md"}


def is_public_code(path, repo):
    if path.startswith("tests/") or "/tests/" in path:
        return False
    if path.endswith(("_test.rs", "_tests.rs", ".test.js", ".spec.js")):
        return False
    return path in PUBLIC_ROOT_FILES.get(repo, set()) or path.startswith(
        PUBLIC_PREFIXES.get(repo, ())
    )


def has_doc_change(paths):
    return any(
        path in DOCS_FILES
        or (path.startswith(DOCS_PREFIXES) and path.endswith(".md"))
        or (path.startswith("site/") and path.endswith(".html"))
        for path in paths
    )


def main():
    base, head = os.environ.get("DOCS_BASE_SHA"), os.environ.get("DOCS_HEAD_SHA")
    if not base or not head:
        print("Docs check needs both PR base and head commits.", file=sys.stderr)
        return 2
    repo = os.environ.get("GITHUB_REPOSITORY", "").rsplit("/", 1)[-1]
    if repo not in PUBLIC_PREFIXES:
        print(f"Unknown family repository: {repo}", file=sys.stderr)
        return 2
    try:
        changed = subprocess.check_output(
            ["git", "diff", "--name-only", base, head], text=True
        ).splitlines()
    except subprocess.CalledProcessError:
        print("Could not compare PR commits; check fetch-depth.", file=sys.stderr)
        return 2
    source = [p for p in changed if is_public_code(p, repo)]
    if not source:
        print("No user-visible source files changed: docs impact check passed.")
        return 0
    if has_doc_change(changed):
        print(f"Public docs included with {len(source)} source file(s).")
        return 0
    body = os.environ.get("PR_BODY", "")
    exemption = re.search(r"(?im)^Docs:\s*none\s*[-:–—]\s*(\S.{7,})$", body)
    if exemption:
        print("Author explained why no public docs changed: " + exemption.group(1))
        return 0
    print("Public source changed without a README, CHANGELOG, docs/*.md or site HTML update.")
    print("Please update affected public docs in THIS PR, or state a concrete reason in its body:")
    print("Docs: none — internal refactor; users see no changed commands or results")
    print("Relevant changed files: " + ", ".join(source[:12]))
    return 1


if __name__ == "__main__":
    sys.exit(main())
