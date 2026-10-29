# Historical provenance remediation — evidence and controlled plan

**Audit date:** 2026-10-29. **Status: investigation and narrow dry run complete / remote history not remediated.** Read-only Git metadata review, release/fixture inventory and a disposable-mirror dry run. Not a legal opinion.

## Confirmed facts

- The project's existing `docs/PROVENANCE.md` records historical firmware-derived and third-party-derived byte sequences and an external-payload loader before commit `76ce766`; it explicitly states the offending contents have **not** been re-audited and historical cleanup has **not** been verified.
- GitHub compare endpoint on 2026-10-29 confirms `c7f16f2576b69aeafb65bd872353eda74080e54a` and `4be9640381850108d0e99da216b648a98512f149` are both ancestors of current `main` (`9ec476191cf08aa371dd63393c815dc340c15261` at inspection time).
- A full non-shallow clone plus explicitly fetched GitHub PR refs produced a dated snapshot of 161 refs. Both flagged commits were reachable from 160: one local audit head, 54 remote refs, three tags, 93 PR heads and nine available PR merge refs. `refs/pull/1/head` was the sole exception. PR merge refs are ephemeral, so these are snapshot counts rather than permanent server inventory.
- Published releases `v0.3.0`–`v0.3.2` contain nine platform archives. Their downloaded SHA-256 digests matched GitHub's published digests. Exact searches of the extracted executables did not find either complete historical standalone binary object; this does not rule out shorter fragments or separately compiled source material.
- The current `main` tree contained none of the pre-cleanup blob IDs changed by `76ce766`. Historical reachability remains the open issue.
- Existing fixture register records unverified authorship for `hello`, `services`, `romfs`, `nvdrv`, `gpu`, `input`, `threads`, `audren` and `deko3d` NRO source/binary material. Exact binary component inclusion and some toolchain versions are unresolved.
- Eleven committed NRO fixtures were inventoried by path, Git object ID, size and SHA-256. The provenance/toolchain registers omit `input_multi` and `sdmc`; `deko3d` is acknowledged but lacks a toolchain and binary-identity entry. Independent source authorship and binary-component review remain unresolved for all eleven.
- Cargo declared licence metadata inventory (dated 2026-10-28) is not a source-text audit or proof of redistribution compliance.

## What has *not* been verified

- Which exact historical Git blob objects contain restricted/proprietary material, their copyright holders, and whether every flagged byte is legally restricted.
- All tags, remote branches, GitHub internal PR refs, historical releases/assets, caches, forks or downstream clones. The GitHub endpoint cannot show every reference on every server.
- Whether any current-tree binary fixture includes third-party components requiring additional notices or redistribution permissions.
- That GitHub or public caches would forget already published content after rewriting refs; they may not.

## Safe inventory procedure (Codex or maintainer, in an isolated complete clone)

1. Snapshot and verify complete refs using `git fetch --all --tags` and available GitHub PR refs; note that `git fetch --all` does **not** necessarily fetch GitHub PR refs. Use a private mirror/backup and list remote refs separately. Do not publish the backup or redistribute suspect contents.
2. Run `python3 scripts/audit_history_reachability.py > history-reachability.json`. The tool is **read-only** and reports only commit IDs and reachable refs, not flagged bytes. A shallow clone returns exit code 2, indicating incomplete history. Do not publish private ref names or personal paths without review.
3. In a restricted local environment, inspect the flagged commits and relevant ancestors by file paths, blob IDs and hashes. Create a **private** object-level inventory (Git object ID, SHA-256 of the blob, commit(s), file path, source provenance, permission evidence). Avoid echoing or uploading the contents, keys or proprietary bytes.
4. For each suspect object, classify as (a) authorised/licence-compatible, (b) unverified and held for permission review, or (c) confirmed inappropriate to distribute. Obtain expert IP advice for disputed or high-risk cases. Document supporting notices and decisions, not prohibited bytes.
5. Audit fixture source authorship and prebuilt-binary composition against their recorded toolchain inputs. If any fixture cannot be adequately validated, replace it with independently authored, reproducibly built and fully documented synthetic material **on a normal reviewed PR**, preserving existing test behaviour where legitimate; otherwise mark the fixture as withheld pending review.

## Decision before irreversible remediation

- A history rewrite could change **every descendant commit SHA**, including active PRs, tags and releases; it can break signed tags, deep links, forks, existing clones and external CI caches. No force-push, deleted tags/releases, garbage collection, or ref mutation is authorised by this PR.
- Prefer retaining a reviewed, clean working tree first. If confirmed restricted blobs remain in reachable history and the owner authorises a rewrite, use `git filter-repo` on a **disposable mirror**, with carefully specified blob or path criteria, checked against an approved private manifest.
- Perform *before/after* object reachability tests, compare legitimate source trees, run all nine mandated matrix jobs plus harness and external evidence, and inspect all active PR branch bases. Maintain a complete old-to-new ref mapping and migration directions for collaborators.
- Coordinate any GitHub remote cleanup, tag/release treatment, and host-side cached objects; note that force-pushing cannot guarantee erasure from third-party clones or public archives. Contact GitHub Support for remaining inaccessible server-side exposure as appropriate.
- Publish a dated, accurate **post-remediation** scope statement only when the rewritten refs, artifacts and retained remote reachable objects have been independently checked. Do not declare “history clean” solely because `main` was rewritten.

## Completed narrow dry run (2026-10-29)

A disposable local mirror was filtered only for the two confirmed standalone historical paths: `ns2jb_ro_payload.bin` and `switch2_showcase.nsp`. No source candidate, remote ref or GitHub object was modified.

- `git-filter-repo` parsed 347 commits; 322 commit IDs changed and 25 remained unchanged.
- The mirror mapped 159 normalized refs: 158 changed and one remained unchanged. It retained 103 explicitly fetched PR refs. This differs from the 161-ref working-clone snapshot because mirror cloning normalizes remote-tracking refs and coalesces duplicate names.
- Both paths had zero remaining path-history entries after filtering, and both historical blob IDs were absent after repacking.
- Current `main` mapped from `9ec476191cf08aa371dd63393c815dc340c15261` to `a02f08f95a065c82487177d3fe0040c6f28856d6`; its tree remained `d60caa0108a20b7b8987b66717b545973f5dd26c`.
- All three release tag trees were content-identical before and after the dry run.

The four source candidates (`src/kernel/ipc.rs`, `src/kernel/svc.rs`, `src/loader/nro.rs` and `src/main.rs`) are mixed blobs containing substantial legitimate code. They remain project-record-identified candidates with independent origin unresolved. A future dry run must use an approved private byte-range manifest and content replacement; filtering entire paths or blobs would be overbroad.

## Prevent recurrence

- Keep `docs/CLEAN_ROOM.md` restrictions in place until specifically and appropriately reviewed; no unapproved copying of other compatibility projects' code.
- Require fixture author, source revision, licence notices, toolchain, input hashes, output hashes and redistribution basis before adding binaries.
- A future CI provenance gate can fail on unregistered fixtures and known prohibited object IDs, but a keyword scanner cannot prove a repository has no copyright or provenance problems.
- Audit proposed open-source reuse *per component* with its exact licence and source identity, retaining obligations and notices.

## Codex completion criteria

- [x] Verify full local Git ref and tag inventory, including explicitly fetched available PR refs and releases (dated snapshot; not a claim about inaccessible server refs or downstream clones).
- [ ] Produce **private** flagged-blob inventory with permission classification; publish only safe summary and counts.
- [ ] Resolve fixture authorship and binary third-party provenance (or replace uncertain inputs).
- [x] Recommend scoped clean-current-tree handling separately from risky history rewrite; current-tree exact-object scan was negative and the standalone-object dry run remained local.
- [ ] Ask owner for **explicit approval of a specific rewrite plan** only after affected refs/blobs and collaborator impact are known.
- [ ] Run and record all nine CI jobs on the final approved remediation integration head.
- [ ] Update provenance register truthfully; do not claim historical removal prematurely.

No unreviewed merges, policy bypass, or commercial/retail decryption changes.
