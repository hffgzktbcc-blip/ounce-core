# E2 owner decision memo — 11 November 2026

Six yes/no decisions for the project owner, plus one finding on issue #190 (E1).
Prepared from the repository at `4283b62` and read-only `gh` queries.

**No action has been taken on any item below.** No ref, release, asset, issue,
Support request or third-party message was changed or sent. This is an engineering
record, not legal advice and not a grant from any rights holder. A "yes" here
authorises only the exact action named. It does not extend to any other item.
E1 and E2 stay incomplete in `PLAN.md`.

## Read this first: the "3 open items" are already recorded as closed

`docs/RELEASE_NOTICE_REVIEW.json` currently has `"open_items": []` and
`"status": "REVIEWED"`. The three items existed as the `open_items` list from
`a147294` to `411120d`. The count fell to 2 at `42e65e9`/`b33e7f3` and to 1 at
`9718a1f`. It reached 0 at `2e5cf12`, "record owner-directed v0.6.0 distribution
decision". The reviewer field reads "Project owner — chat direction dated
2026-11-09; distribution choice delegated".

The decisive owner words, quoted in
[v0.6.0-distribution-decision.md](releases/v0.6.0-distribution-decision.md), are
"Do what ever is easier", given when asked whether to approve with qualifications
or require specialist review first. That delegates the choice. It does not itself
say "yes, no specialist is needed". Decisions 1–3 below therefore ask the owner
to confirm or reverse that record item by item.

| # | Item | Decision asked | Recommendation |
| --- | --- | --- | --- |
| 1 | objc2 / Apple SDK-derived binding terms | Keep the owner-directed REVIEWED record without specialist review? | Yes |
| 2 | Rust std / compiler-runtime notices | Accept binary-bound notice collection as sufficient? | Yes |
| 3 | Final-binary and per-platform notice coverage | Accept the three-platform proof as sufficient? | Yes |
| 4 | 72-byte third-party program in history | Authorise a history rewrite to remove it? | No, not now |
| 5 | `refs/pull/1/head` | Ask GitHub Support to hide or purge it? | Yes |
| 6 | Corrected v0.5.0 release with the DejaVu notice | Replace the v0.5.0 assets? | No |

---

## 1. objc2 and Apple SDK-derived binding terms

**Decision: keep the REVIEWED record, with no specialist legal review? Yes / No**

Evidence:
- The pinned objc2 `LICENSE.md` says its Apple-SDK-derived bindings may have
  licensing implications and that it is "unclear whether distributing derived
  works such as these crates are allowed". That qualification is preserved, not
  resolved.
- Upstream issue madsmtm/objc2#826: the maintainer confirms the declared licences
  for earlier versions. The follow-up does not confirm exhaustive copyright
  attribution.
- The Apple Xcode/SDK Agreement EA2002 (06/08/2026), section 2.4, permits
  distributing macOS applications and libraries under that agreement. The memo
  that cites it says this "does not itself settle the provenance of all
  generated bindings".
- The shipped CLI archives contain no SDK headers, frameworks or Xcode. The
  native Mac app and optional features are excluded and the README says to build
  from source (LICENSE_DISPOSITION, #310).
- v0.6.0, v0.6.1 and v0.7.0 were published on this basis.

Consequences:
- **Yes:** the record stands. Ordinary releases continue. The upstream
  qualification stays disclosed. Risk remains that a rights holder disagrees,
  and no one has been asked.
- **No:** set `status` back to `PENDING_SPECIALIST_REVIEW`. Strict packaging
  (`--require-reviewed`) then refuses further releases until a specialist reviews.
  Already published v0.6.0–v0.7.0 are unaffected unless you also withdraw them.
  A prebuilt Mac app stays blocked either way (#310 still needs specialist review).

## 2. Rust standard-library and compiler-runtime notices

**Decision: accept binary-bound toolchain notice collection as sufficient? Yes / No**

Evidence:
- Packaging requires notices captured from the actual installed toolchain that
  built the binary: MIT, Apache and the library/whole-toolchain copyright
  inventory. A generic licence label or another toolchain's record cannot
  substitute (LICENSE_DISPOSITION, "Rust notices are tied to builds").
- v0.6.0 qualification run 42569508955 passed on all three platforms. The
  archive verifier checks notice bytes and tampering fails closed.
- v0.6.0 publication proof: [v0.6.0-publication-proof.json](releases/v0.6.0-publication-proof.json).

Consequences:
- **Yes:** the open item stays closed. Each release must keep collecting its own
  notices, which the gate already enforces.
- **No:** reopen the item. Specify what more you require (for example
  specialist review of the toolchain inventory). Releases then wait.

## 3. Final-binary and per-platform notice coverage

**Decision: accept the three-platform archive verification as sufficient? Yes / No**

Evidence:
- The tag workflow's strict reviewed packaging verified actual archives on Linux
  x86-64, macOS arm64 and Windows x86-64. The Linux archive carries 22 verified
  native-notice entries. All seven v0.6.0 assets were downloaded and their
  checksums matched. The Linux executable passed 36 integration checks outside
  the checkout.
- Limits recorded in the proof: macOS and Windows executables were not run
  locally, attestation signatures were not independently verified, and hosted
  artifact ZIPs could not be downloaded. Coverage was checked against the
  binary-bound inventory, not by an outside auditor.
- I did not re-verify the v0.7.0 assets for this memo.

Consequences:
- **Yes:** the item stays closed. Remaining limits stay disclosed.
- **No:** require independent verification (for example, signature verification
  and local runs on macOS and Windows) before the next release. This costs
  effort, not a mutation of anything published.

## 4. The 72-byte third-party program in history

**Decision: authorise a history rewrite (force-push) to remove it? Yes / No**

Evidence:
- Identified as `pepsipu/ns2jb` `engine/switch2/process/test-function.bin`,
  SHA-256 `71544d50…1b42`. No permission grant was found.
- Your decision of 8 Nov 2026 (PROVENANCE_OWNER_DECISIONS item 2, repeated in
  LICENSE_DISPOSITION #308) is to remove it and not seek a grant. The request
  draft is marked "Do not send" and nothing was sent.
- It is absent from the current tree. It remains in 20 source blobs, reachable
  from 157 of 159 local refs, including main and tags v0.3.0–v0.5.0. This scope
  is a dated local snapshot.
- A disposable-mirror dry run of path-filtering changed 322 of 347 commits and
  158 of 159 refs. It covered only two other paths, so it is not a plan for this
  program. The array sits inside mixed source files, so removing it needs an
  approved byte-range manifest and an explicit old-to-new ref map
  (decision 3 in PROVENANCE_OWNER_DECISIONS). None exists.

Consequences:
- **Yes:** you must also approve a specific ref map. The rewrite changes nearly
  every commit SHA, breaks open PR bases, deep links, forks, clones, signed tags
  and release tags, and cannot erase copies elsewhere. Do it only with a prepared
  migration plan.
- **No (recommended):** the program stays in history and is treated as
  unpermitted. The project records it plainly and is exposed to a takedown claim
  from the rights holder. Current trees and published archives are scanned
  negative. You can reverse this later, or choose to send the draft request to
  the rights holder instead.

## 5. `refs/pull/1/head`

**Decision: ask GitHub Support to hide or purge it? Yes / No**

Evidence:
- Twelve firmware-labelled arrays and two standalone objects are retained only
  by this ref (23 objects reachable only from it). Current source and inspected
  release archives had no exact matches.
- PR #1 is closed and merged (head `6dd086d`). GitHub controls PR refs and may
  not support the proposed treatment. A local rewrite of 19 commits touching only
  this head was prepared and verified. It is local only.
- Outcome recorded in LICENSE_DISPOSITION #309: option (b), leave and document.
  No Support request was sent and no purge is claimed. Option (a) can be chosen
  later.

Consequences:
- **Yes (recommended):** I prepare a message naming the ref and object IDs only,
  with no bytes, and you send it or authorise it. Outcome is up to GitHub. It
  cannot remove forks, caches or clones. It is the only route to removing the
  most serious category (firmware-labelled data) from the host.
- **No:** the objects stay publicly fetchable through that ref. The project keeps
  documenting this and cannot claim they are inaccessible.

## 6. Corrected v0.5.0 release with the DejaVu notice

**Decision: replace the v0.5.0 assets with corrected archives? Yes / No**

Evidence:
- v0.5.0 (published 2026-11-06) has four assets: Linux, macOS and Windows
  archives and `SHA256SUMS`. LICENSE_DISPOSITION states that those archives
  lack the full DejaVu notice added by PR #203. I did not download them to
  re-check.
- v0.6.0, v0.6.1 and v0.7.0 include the notice. v0.7.0 is Latest.
- Replacing assets changes published checksums under an existing tag, and a
  rebuild cannot be byte-identical to the original. Existing downloads and any
  references to the old hashes would no longer match.

Consequences:
- **Yes:** replacement archives and a new `SHA256SUMS` go up under v0.5.0, so the
  record is truthful only for new downloads. Anyone holding the old files still
  has the missing notice.
- **No (recommended):** v0.5.0 stays as published. Users are pointed to v0.7.0.
  The gap stays documented. Editing the release body to say so is a separate,
  smaller action you can also approve.
- A third path, deleting the v0.5.0 release, is not asked here. It is
  destructive and the tag would remain.

---

## Issue #190: is either comment an independent rerun? (E1)

**Finding: no. Neither comment is an independent rerun. Both are one outside
tester's self-reported execution. E1 stays blocked.**

What was read: `gh issue view 190 --comments`, the linked #191, the artifact
repository, PLAN E1, and `docs/INDEPENDENT_TESTER_CHALLENGE.md`.

- Both comments are by `@cyzcz`, 2026-11-07 (08:27 and 08:59 UTC). The first
  links #191 (RomFS). The second adds simplegfx, deko-basic and cpu-jit passes
  and an audio-echo failure.
- The challenge defines "independent rerun" as "a person outside the original
  run can repeat the same input and compare the evidence", and says that until
  then a report is "self-reported, awaiting independent rerun". PLAN E1 asks for
  an independent rerun of the existing experiment. Two comments by one person
  are one run by one tester. The tester's own report carries that exact label.
- It is real outside evidence and I found no sign it is false. Both RomFS and
  deko-basic hashes (`0x3bd649bbc4f50b85`, `0x3a00ccf6b6f28ea1`) match
  `tests/corpus/oracles.json`. The artifact repository's tree has the logs and
  `inventory.json`. The tester contributed merged PR #193. I did not rerun any
  program.
- Reasons it is not enough on the project's own protocol:
  - Not the pinned recipe: Ounce-Core `938b6bc` instead of `4dba991`, an
    extracted devkitPro toolchain instead of Docker.
  - The byte-checked canonical records the recipe requires (`records.json`,
    `canonical-report.json`, `ounce_verify.py --artifact-root`) are not in the
    public RomFS bundle.
  - Independence is a self-declaration. The account is a repository contributor
    and was not authenticated otherwise. Not disqualifying, but it counts
    against calling it unaffiliated.
  - The maintainers' 10 Nov review of #191 accepted that the pixel expectations
    were pre-registered. I did not independently re-check commit dates. The
    artifact repository (created 2026-11-05, issue #190 opened 2026-11-07) is the
    tester's own, so its contents are unauthenticated self-published evidence.
- The maintainers' 10 Nov comment on #191 reaches the same classification,
  `unverified report`. I agree with it.
- One open question: the tester's `audio-echo` failure matches the committed
  `oracle-results.json` (46 pass / 4 fail) but not the README's 50/50, which
  rests on a retained canonical report from a later commit. That is a repository
  consistency question, not evidence about independence.

What would close E1: a second person, not the original tester, follows the
pinned RomFS recipe (or discloses deviations) and publishes the NRO identity,
both engine observations and byte-checked canonical records, then compares with
@cyzcz's report. A mismatch or blocked step counts too.

Verification note: this memo was assembled from repository files and `gh`
output on 11 November 2026. No cargo build or test was run; the change is
documentation only.
