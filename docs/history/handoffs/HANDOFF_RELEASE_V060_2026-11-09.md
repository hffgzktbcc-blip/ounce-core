# v0.6.0 release handoff — 2026-11-09

The requested normal release is published and verified as Latest:
https://github.com/hffgzktbcc-blip/ounce-core/releases/tag/v0.6.0

Tag source: `429f44bd8c21edc0d79b4359730e9f86d9eb8ccc`.
Workflow: https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42574581442
All three platform jobs, provenance-attestation steps and publication succeeded.
Release contains default CLI binaries for Linux x86-64, macOS arm64 and Windows
x86-64, individual checksums and SHA256SUMS. All seven assets were downloaded;
actual bytes match API digests and checksum files. Archive binary/review/notice
contents match final hosted reports. Downloaded Linux CLI passed all 36 integration
checks outside the checkout. No independent verification of attestation signatures
or local execution of macOS/Windows binaries is claimed.

Proof: docs/releases/v0.6.0-publication-proof.json. Final downloaded files and logs:
/workspace/evidence/v060-published-downloads, v060-published-smoke,
v060-final-workflow.json and v060-final-platform-reports.json.
Git bundle backup: /workspace/ounce-core-handoff.bundle (refreshed before stopping).

The real owner reply after disclosure and the release-vs-specialist question was
"Do what ever is easier". The selected path and exact delegation are recorded in
docs/releases/v0.6.0-distribution-decision.md. REVIEWED denotes this owner-directed
distribution decision, not specialist legal review, exhaustive ownership clearance
or an additional Apple/third-party grant. All SDK/contributor qualifications stay
in the record. Do not fabricate additional approvals or keep asking generic release
permission; publication within this scope was authorized and is complete.

PR273 prepared 0.6.0 and closed genuine Linux native-library notice gaps;
PR276 retained qualification evidence; PR278 recorded the direction and finalized
notes. Strict packaging now requires actual installed shared-library copyright
and common-license texts for Linux ELF, including LGPL-2.1, bound to the runtime
hash. Libraries themselves are not bundled. Missing/tampered/escaping notices fail.

Normal release scope excludes the native Mac GUI app and optional GPU/native/JIT
feature graphs. App review, final bundle notices and live GUI acceptance remain
separate. Historical 72-byte program permission/source-provenance questions and
outside-project conformance adoption remain unresolved; no history was rewritten.
Earlier handoffs saying the normal release is pending are superseded by this one.
For wider vision work, use PLAN.md; V7's public Wasm route already works and V8's
one-case seed is published internally qualified on all three hosts. Outside adoption
cannot be manufactured. Do not repeat expensive passing tests without changed
inputs or an unresolved concern.

Environment: source /workspace/toolchains/env.sh; Rust 1.99.1, Zig 0.17.1. Network,
Git/gh and localhost sockets need approved sandbox escalation. Release asset
downloads work; Actions artifact ZIP storage earlier denied downloads. GitHub
connector decoded completed job logs supplied qualification reports. This installed
gh lacks newer pr-checks --json/baseRefOid fields and pr-edit hits deprecated classic
project GraphQL; structured REST PATCH works. Preserve tagged history and source.
