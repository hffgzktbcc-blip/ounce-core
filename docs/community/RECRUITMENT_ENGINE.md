# Ounce-Core passive recruitment engine

**Status:** GitHub-native parts are proposed through a reviewable PR. Listings on external sites are **not submitted** merely by adding this file.

## Where newcomers enter

- [Public contribution landing page](https://hffgzktbcc-blip.github.io/ounce-core/contribute.html) (available after website deployment).
- [GitHub's own contribution page](https://github.com/hffgzktbcc-blip/ounce-core/contribute) and [live beginner issue queue](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22).
- [Contributing guide](../../CONTRIBUTING.md), [first-time reader guide](START_HERE.md), [research paper](../../site/research.html), [external reproducibility task #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248).

These are designed to link to **open issues**, not a fixed list of IDs that inevitably closes.

## Automation (starts after this PR is merged)

[.github/workflows/community.yml](../../.github/workflows/community.yml) has two narrowly scoped jobs:

1. **First-time PR welcome:** On a PR authored by a contributor GitHub identifies as a first-timer, post at most one helpful, clearly automated onboarding comment. Does not check out PR content, run untrusted code, grant permissions, auto-approve, auto-merge, or message a stranger outside this repository. A bot is skipped.
2. **Starter inventory watch:** Mondays at 06:23 UTC (08:23 South African time) and via manual workflow dispatch, count *open* issues carrying `good first issue`. When fewer than three exist, open/reopen a **single** maintainer reminder; close it again when there are at least three. It never fabricates filler issues, relabels advanced work, creates jobs, sends unsolicited DMs or automatically admits contributors.

To turn these off, disable the workflow in GitHub Actions or remove this workflow through a PR. Workflow permissions are limited to reading repository content and writing issue/PR comments.

## External directories: one-time owner login needed

A third-party listing may increase discovery but requires the external provider's authentication and/or acceptance. **No listing is claimed as completed** until a public listing URL exists.

| Service | Submission | Status |
| --- | --- | --- |
| Good First Issue (`goodfirstissue.org`) | [Submit your GitHub owner/repo](https://www.goodfirstissue.org/submit-repo); GitHub sign-in is required. It can then crawl recent public repos. | Not submitted through this workflow |
| OpenSrc | [Suggest repository](https://opensrc.studio/submit) by pasting `hffgzktbcc-blip/ounce-core`; the service checks and may file a review issue. | Not submitted through this workflow |
| CodeTriage | [Add a repo](https://www.codetriage.com/repos/new); requires the owner's interactive login. | Not submitted through this workflow |
| Up For Grabs | [List a project via external PR](https://github.com/up-for-grabs/up-for-grabs.net/blob/master/docs/list-a-project.md); external maintainers review listings. | No external PR opened |

GitHub itself surfaces appropriately labelled issues on its **Contribute** page; that discovery does not require third-party registration. External providers may apply independent activity, star, review or eligibility criteria.

## What the engine does not do

- It does not guarantee signups, interest, independent reproduction, or review time.
- It does not auto-send email, cold DMs, social posts, GitHub mentions or invite collaborators.
- It does not convert every user-submitted program request into a `good first issue`.
- It does not claim an internal CI pass is independent scientific verification.

## Lightweight health check

Use [GitHub's live beginner issue filter](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22), [external reproduction #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248), [pull requests](https://github.com/hffgzktbcc-blip/ounce-core/pulls), and the Community onboarding workflow's weekly summary. Track meaningful first-time PRs, useful outside test reports and accepted improvements—not the number of strangers messaged.

**A human maintainer is still needed** to review incoming PRs, respond to questions, and keep the issue pool genuinely useful. The bot handles low-friction welcome and replenishment reminders, not mentorship.
