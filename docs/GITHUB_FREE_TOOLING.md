# Free GitHub infrastructure

This document records the intended configuration. Workflow files alone do not enable GitHub repository settings.

## Already configured by proposed PR

- Dependabot weekly Cargo and GitHub Actions version-update PRs; no auto-merge. Security alerts and security updates require GitHub security settings.
- Dependency review on dependency-changing PRs; requires an enabled dependency graph. Fails on newly introduced **high** or **critical** known vulnerabilities.
- Static GitHub Pages site from `site/`, deploying on relevant `main` updates once Pages is configured with **GitHub Actions** as source.
- Release process: multi-platform artifacts with checksums, per-platform tests, and GitHub artifact attestations on new version tags (not retroactive to old releases).

## Repository administrator settings

1. **Pages:** Settings → Pages → Build and deployment → Source: **GitHub Actions**. Then dispatch `Documentation website` if it has not run.
2. **Discussions:** Settings → General → Features → check **Discussions**. Suggested categories: General, Ideas, Q&A, Research results.
3. **Projects:** Create a new GitHub Project and link the repository. Suggested fields: Status (Backlog / Ready / In Progress / Review / Done), Area (CPU / GPU / Verify / Frontend), Priority, Owner. Populate from existing issues/PRs; avoid duplicate tracking tickets.
4. **Security:** Settings → Advanced Security (or Code security and analysis) → enable Dependency graph, Dependabot alerts and Dependabot security updates as available. Keep CodeQL and RustSec workflows enabled.
5. **Branch protection:** Preserve required CI and independent review. Do not enable auto-merge or bypass checks.

## Release policy

- The repository already published `v0.3.2`; do not retroactively claim its artifacts have attestations/checksums from this change.
- Do not publish a new tag until the tested release candidate is reviewed and provenance issues evaluated.
- On future new version tags, inspect the full release CI result and uploaded archives, SHA-256 files, and attestations.
- Check signatures using the GitHub CLI, for example `gh attestation verify <archive> --repo hffgzktbcc-blip/ounce-core` after downloading the actual released archive.
- A provenance attestation identifies a workflow and subject digest, not software correctness or historical licensing clearance.
