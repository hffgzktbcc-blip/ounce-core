# Ounce-Core Independent Tester Challenge

**Goal:** Establish whether a developer *outside the Ounce-Core team* can reproduce a real first-frame result from unchanged, legally distributable original-Switch homebrew on their own machine.

This is an invitation to **test, report, and challenge the evidence**, not a claim about commercial games, Switch 2 compatibility, or universal homebrew support. The project's internal 50-program corpus number is **not** an independent user count.

## Choose a track

1. **Reproduce a registered corpus oracle (recommended).** Follow [the existing RomFS reproduction guide](REPRODUCE_ROMFS.md) using the pinned upstream source and documented build container. Do not modify the homebrew source, expected frame, oracle, or runtime. Record **both** interpreter and Tier-3 outcomes.
2. **Report an independent homebrew experiment.** Run a separately maintained, permission-cleared original-Switch homebrew sample and publish its exact upstream revision, license, build recipe, input hash, and observable outcome. If there is **no pre-registered oracle**, report *observation only*, not a compatibility pass.
3. **Report a failure.** A reproducible failure, mismatch, or crash is valuable. Never adjust an expected frame to make a failing result pass.

## Safe start: registered RomFS example

Prerequisites for the pinned recipe: Linux amd64, Rust stable, Git, Python 3, Docker, and the build dependencies in [REPRODUCE_ROMFS.md](REPRODUCE_ROMFS.md). Other hosts must report deviations.

Follow all sections of [the pinned RomFS recipe](REPRODUCE_ROMFS.md), in order,
in one Bash shell. It builds only the selected outside sample, invokes the exact
registered corpus oracle, captures both engine observations, and validates the
two canonical records against actual binary bytes. The machine-readable
[reference manifest](reproduce/romfs.json) records the pinned core, image digest,
source revision and existing CI input hash.

The recipe currently targets Linux amd64; other hosts must disclose deviations.
It is an internally prepared recipe, not a claim that an unaffiliated person has
completed it. Legal review of linked binary components remains pending; share
sanitized text evidence rather than redistributing binaries. Do not treat a zero
host process exit or engine agreement alone as a verified guest frame.

## Submit evidence

Open a new issue in [Ounce-Core issues](https://github.com/hffgzktbcc-blip/ounce-core/issues/new) titled `Independent test: <OS/architecture> — <sample> — <pass/fail/observation>`, linking this challenge. Use this template:

```text
Tester: GitHub handle (self-reported; no personal information needed)
Independent of maintainers/agents? yes / no / disclose relationship
Track: registered oracle / new outside sample / failure reproduction
Host: OS/version, CPU architecture, GPU (optional), Rust version
Ounce-Core commit SHA:
Upstream homebrew project, license, exact revision:
Build environment and container image ID (if applicable):
Artifact SHA-256 (and how obtained):
Oracle name and oracle revision, if pre-registered:
Exact reproduction commands:
Interpreter: observed stop reason, frames/queued frames, oracle verdict:
Tier-3: observed stop reason, frames/queued frames, oracle verdict:
Evidence: full redacted logs, CI links, hashes, frame/probe evidence
Modifications (if any; changed inputs are not unchanged-corpus passes):
Observed result: pass / fail / inconclusive / observation only
Known limitations, failure details, and steps to reproduce:
```

Attach text logs and non-sensitive outputs or provide stable public links. **Do not upload** retail games, firmware, keys, personal accounts, proprietary SDK assets, or copyrighted binaries without redistribution permission. Redact usernames, tokens, host paths, and private data. Do not download untrusted executables from strangers to validate a report.

## Verification and recognition

Maintainers may label a submission only after checking all of the following:

- **Identified:** public reporter and reproducible record; relationship to the team is disclosed.
- **Outside input:** independently maintained, appropriately licensed source at an exact upstream revision, unchanged for a registered-corpus pass.
- **Reproducible:** full commands, host details, Ounce-Core SHA, artifact hash and original unedited output.
- **Oracle-bound:** pre-registered expected frame (before execution), frame/probe evidence, and separately recorded interpreter and Tier-3 results.
- **Independent rerun:** a person outside the original run can repeat the same input and compare the evidence. Until then, mark `self-reported, awaiting independent rerun`.

Outcomes should be recorded explicitly as `verified pass`, `verified failure`, `unverified report`, or `observation only`. **Do not roll them into the internal corpus total.** No cash prizes or promises of prizes; public credit is opt-in and must link to the evidence.

Questions and corrections are welcome as issues, including negative results. The goal is useful independent science, not inflated metrics.
