# Drafts for owner action (nothing here has been sent or applied)

Prepared 2026-11-08. Not legal advice. Send/apply only after the owner decides.

## A. objc2 / Apple SDK question: brief for the notice reviewer

Verified with `cargo tree --locked`:
- `objc2`-family crates enter the **default** build only on **macOS**, via `cpal 0.18.2`
  (audio): `objc2-foundation`, `objc2-core-audio`, `objc2-audio-toolbox`, `coreaudio-rs`.
  `cargo tree -i objc2` is empty for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc`.
  So the open question affects only the macOS arm64 release binary.
- Retained upstream text (`THIRD_PARTY_NOTICES.txt`, from madsmtm/objc2 `LICENSE.md`):
  `objc2`, `block2`, `objc2-foundation`, `objc2-encode` are MIT; other crates Zlib OR Apache-2.0 OR MIT.
  The crates are "usually automatically derived from Apple SDKs"; upstream states it is
  "unclear whether distributing derived works such as these crates are allowed" under the Xcode licence.
  This is upstream's own uncertainty, not a finding by this project.

Questions for the reviewer: (1) is shipping a macOS binary that statically links these
generated bindings acceptable; (2) is the retained notice text sufficient; (3) if not, is the
fallback to drop macOS audio or ship macOS as build-from-source acceptable.
Then fill `reviewer` and `decision_evidence` in `RELEASE_NOTICE_REVIEW.json`
(it is hash-bound: regenerate with the existing packaging tool, do not hand-edit hashes).

## B. Draft GitHub Support request: pull request #1 head

> Subject: Request to remove cached objects reachable only from refs/pull/1/head (hffgzktbcc-blip/ounce-core)
>
> Repository: hffgzktbcc-blip/ounce-core. The read-only `refs/pull/1/head` ref retains
> historical objects that the owner has determined should not be redistributed (firmware-labelled
> byte arrays and two standalone files; object IDs available privately on request).
> The commits are not reachable from any branch or tag. Please advise whether you can remove or
> disable the PR #1 head ref and run garbage collection on the repository, and whether any
> cached views or forks of those objects can be purged. We have not rewritten any branch or tag.
> Contact: <owner email>.

Owner must: confirm the wording, supply object IDs privately, and submit it. Do not
attach the suspect bytes.

## C. 72-byte third-party program: permission request template

> Hello <upstream author>, ounce-core (MIT) historically committed a 72-byte test program
> derived from your <project/path>. We found no explicit licence in the pinned source. May we
> keep it (with attribution) under <licence>, or would you prefer we remove it?

Owner must identify the upstream author/contact; origin evidence is in
`PROVENANCE_REMEDIATION_REPORT.md`.
