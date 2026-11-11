# Provenance remediation progress

Updated 8 November 2026. This public summary records the controls merged in PR #203 and integrated with the developer toolkit in PR #205. Current notice decisions are in [LICENSE_DISPOSITION.md](LICENSE_DISPOSITION.md). Detailed historical object manifests and suspect contents remain private. Remote historical cleanup and release publication require separate owner decisions.

## Object-level findings

The complete available snapshot at main `9e813b984930f6280b1a23ac2d3a8ed9d9f5d2e7` contains 773 commits and 352 fetched refs, including available PR refs. Twelve distinct nonzero firmware-labelled arrays and two standalone objects are retained only by `refs/pull/1/head`. (Object count, reconciled 2026-11-08 against the private inventory: 23 objects are reachable only from that ref, namely 21 historical source-file versions (20 of them contain the 12 arrays; one `src/kernel/ipc.rs` version has no flagged array) plus the 2 standalone objects. The 72-byte program is a separate item and appears in history beyond that ref.) A separate 72-byte third-party homebrew test program remains in wider history, with permission review open. Commit reachability alone does not establish the origin or rights status of particular bytes.

A hash-validated local test rewrite changed nineteen commits and only the retained PR #1 head. It preserved main, all five tags, every unrelated file entry and all non-target source bytes. A separate agent verified the result. This is local preparation. GitHub controls its PR references and may not support the proposed treatment. No public history has been rewritten.

The current source and fifteen release archives were scanned for exact candidate byte sequences, with no matches. All sixteen downloaded release assets matched their GitHub-recorded SHA-256 digests. These are exact-match and integrity results. They do not establish exhaustive source provenance, transformed-code absence or legal clearance.

The 72-byte program's origin is corroborated by the original upstream pinned checksum register. The pinned source header, root licence paths and complete repository trees contained no redistribution grant in the reviewed scope. Permission remains UNVERIFIED, without a denial or infringement finding. This does not authorise a broad history rewrite.

## Fixture evidence and controls

All eleven committed NROs rebuilt byte-for-byte from their tracked inputs with the recorded native toolchain. `tests/fixtures/rebuild-evidence.json` retains commands, successful exit codes, source/tool/archive hashes and linker-selected members. Package-signature verification is unclaimed. Source authorship and component redistribution review remain open.

`tests/fixtures/provenance.json` covers every fixture, including input_multi, sdmc and deko3d. The fixture validator checks binary and build-input identity, rejects unregistered NROs and requires review evidence for changes beyond the explicit legacy baseline. A passing identity gate does not establish legal permission. The baseline and validator are sensitive review inputs.

## Distribution corrections

All three published v0.5.0 executables contain the bundled DejaVu font, while their archives omit its notice. The merged packaging change includes the project licence, third-party register and complete retained font licence. A further 169 dependency notice records are now collected with pinned source identities. Release packaging verifies their input hashes and refuses publication while the notice review is pending. The follow-up adds the inherited Objective-C MIT grant/attribution and requires binary-bound notices from the actual Rust toolchain. The upstream SDK qualification remains distinct from those notice fixes. Source package metadata now agrees with the project's recorded MIT-porting practice.

Published artifacts, checksums and attestations must remain distinguishable from any separately prepared repackaging. Existing release evidence is retained. Publication of corrected assets is a separate approval and qualification step.

## Validation and remaining decisions

The prior local release check passed 474 unit tests and three CLI tests, with nine ignored. The runtime harness passed 36 checks across 25 stages. Fixture identity, legal guardrails and unsafe-boundary checks passed. Four isolated negative validator checks rejected missing registration, unreviewed metadata mutation, altered binary bytes and altered source inputs.

The unchanged runtime and corpus inputs match the captured main source. The retained [canonical run 42380281215](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42380281215) is existing evidence for that runtime. Final #203 head `411120d` passed its 29 hosted checks, including canonical corpus artifact revalidation. Integration candidate `6520fc0` for [PR #205](https://github.com/hffgzktbcc-blip/ounce-core/pull/205) passed all 33 hosted checks and merged as `9b3961f`. These completed qualifications must not be described as still pending; they do not grant third-party rights.

Merged CODEOWNERS coverage requests human review for fixtures, oracles, provenance and workflows. Requiring that review needs a separately approved protection update. Existing required checks must be preserved. The Support request concerning PR #1, any release publication and any protection mutation remain subject to owner review. Historical homebrew permission and component-specific redistribution questions remain open. No compatibility metric or registered oracle was weakened.
