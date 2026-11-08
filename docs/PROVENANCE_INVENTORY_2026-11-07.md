# Provenance and release-trust inventory — dated snapshot 2026-11-07

PLAN step 16 working inventory. Read-only checks only; **no ref mutation, no
legal finding, no destructive remediation**. Extends, does not replace,
[HISTORY_REMEDIATION.md](HISTORY_REMEDIATION.md) (2026-10-29) and
[PROVENANCE.md](PROVENANCE.md). Raw reachability output:
[history-reachability-2026-11-07.json](history-reachability-2026-11-07.json).

## Dispositions

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1 | Flagged commits `c7f16f2`, `4be9640` reachability | **Confirmed still present** (blocked on owner-approved rewrite plan) | Full clone (336 refs: 1 head, 134 remote, 5 tags, 196 PR refs). Both commits' objects available and each reachable from **335/336 refs** (2026-10-29 snapshot: 160/161). Ref growth comes from new PR heads. Reachability ≠ legality judgement. |
| 2 | Flagged standalone paths in current `main` tree | **Verified absent** | `git ls-tree -r origin/main` at `19adb50`: zero matches for `ns2jb_ro_payload.bin`, `switch2_showcase.nsp`, `*.nsp`, `*.xci`, `*.keys`. Consistent with the 2026-10-29 finding; historical reachability remains the open issue. |
| 3 | Required status checks on `main` | **Blocked — not auditable with CI/agent token** | Branch-protection API returns 403 for integration tokens. Indirect evidence that protection exists: PR #200 showed `mergeStateStatus: BLOCKED` until required checks passed. Owner/admin must confirm the required-check list and who may change it. |
| 4 | Oracle manifest change control | **Partial — CI-trigger only, no dedicated reviewer gate** | No `CODEOWNERS` file exists, so `tests/corpus/oracles.json` / `manifest.json` changes get no per-path required reviewer. Changes do trigger `ci.yml` and `external-corpus.yml`; the corpus cache key is invalidated by oracle/manifest edits. Weakening an expectation is detectable in review but not mechanically gated. |
| 5 | Dependency/security gates | **Verified present** | `dependabot.yml` active (PR #186 open), `dependency-review.yml` runs on PRs, `rust-security.yml` runs `cargo audit` on Cargo.lock plus CodeQL. |
| 6 | Release asset integrity | **Mostly verified; one gap** | **v0.5.0**: published `SHA256SUMS` matches GitHub's recorded asset digests exactly (re-checked today); `release.yml` also writes per-asset `.sha256` and runs `actions/attest@v4`; release run 42233497706 succeeded. **v0.4.0**: no published checksum file — only GitHub API digests. **v0.3.0–v0.3.2**: nine archives' digests matched on 2026-10-29. |
| 7 | Release rebuildability / independent reproduction | **Unknown/none** | No release artifact is (iii) independently byte-identically reproduced. Rebuild recipe from tags is not published per release; the RomFS one-program recipe (step 15) is the only documented byte-matched rebuild, and it is CI/internal only. |
| 8 | Fixture authorship and binary composition | **Open (carried from register)** | `tests/fixtures/nro/*` authorship unverified; `input_multi`, `sdmc` omitted and `deko3d` toolchain entry incomplete per HISTORY_REMEDIATION. No change today. |
| 9 | Public-log redaction | **Verified clean (scoped)** | Scan of `docs/reproduce/*` and `docs/REPRODUCE_ROMFS.md` found no local usernames, `/Users/...` or `/home/...` host paths, or token-shaped strings. Scope limited to retained text evidence, not CI log archives. |

## What remains blocked on owner/specialist decisions

- Any history rewrite, tag/release treatment or GitHub Support request for
  server-side objects (needs the approved private byte-range manifest first;
  see HISTORY_REMEDIATION "Decision before irreversible remediation").
- Confirming branch-protection required checks and oracle-change approvers
  (item 3) — needs an admin token or owner screenshot.
- The private flagged-blob inventory with permission classification
  (HISTORY_REMEDIATION checklist) — restricted-environment task, not done here.
- Closing the v0.4.0 checksum gap is optional; new releases already publish
  `SHA256SUMS` and attestations.

## Non-claims

This inventory does not enumerate forks, clones, caches or server-side refs
beyond fetched `refs/pull/*`; does not re-audit flagged blob contents; does not
prove redistribution compliance for any artifact; and authorizes no remediation
action by itself.
