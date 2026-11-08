# Provenance: what is resolved and what needs the owner

Status 2026-11-08. Summarises `PROVENANCE_REMEDIATION_REPORT.md`, `HISTORY_REMEDIATION.md`
and `RELEASE_NOTICE_REVIEW.json`. Not legal advice. No history was rewritten and no
release was published by this document.

## Resolved (evidence in the linked docs)
- Ref/tag inventory and reachability snapshot; current tree and 15 release archives scanned negative for known byte sequences.
- All 11 committed NROs rebuild byte-for-byte; provenance register covers every fixture with a validator gate.
- DejaVu notice added to release packaging; 169 dependency notices collected and hash-bound.
- Credit file added (`CREDITS.md`).

## Needs an owner or specialist decision (agents must not do these)
| # | Decision | Options | Effect if skipped |
|---|---|---|---|
| 1 | PR #1 head holds 12 firmware-labelled arrays + 2 objects | (a) ask GitHub Support to purge/hide `refs/pull/1/head`; (b) leave and document | Runtime publication stays gated; history stays as-is |
| 2 | 72-byte third-party homebrew program: permission UNVERIFIED | ask upstream author for a grant; or approve a targeted content replacement; or accept risk in writing | Same gate |
| 3 | Any force-push / tag or release change | explicit approval of a specific old-to-new ref map | None done |
| 4 | objc2/Apple SDK terms, Rust runtime notices, final-binary coverage | specialist review, then fill `RELEASE_NOTICE_REVIEW.json` reviewer/evidence | Release packaging refuses to publish |
| 5 | Branch protection requiring CODEOWNERS review | approve settings change | Review stays advisory |
| 6 | Corrected-release publication | approve after 1-4 | Published v0.5.0 assets lack the font notice |

Unaffected: ordinary development and the standalone Python diagnosis toolkit (PLAN.md E2).
