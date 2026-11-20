# Public documentation contract — the Ounce family

**The reader should understand each project in a minute, and a merged change must not leave the live explanation behind.** This contract covers [Ounce-Core](https://github.com/hffgzktbcc-blip/ounce-core), [Ounce-HB](https://github.com/hffgzktbcc-blip/ounce-hb), [Ounce-Runtime](https://github.com/hffgzktbcc-blip/ounce-runtime) and the independently scoped [DeckStream](https://github.com/hffgzktbcc-blip/deckstream).

## Say what it does

- **Lead with the person, action and outcome.** “DeckStream retrieves game files from your own NAS on demand” is clearer than a list of infrastructure nouns. Put implementation details after the answer.
- Answer four questions first: **What problem does this solve? What can I do? How do I try it? What is still unverified?**
- Explain acronyms once; give a concrete command or before/after where possible; prefer one useful task over a giant capability list.
- Distinguish **implemented**, **tested by maintainers/CI**, **tested on named hardware**, **published in a release**, and **reproduced independently**. Do not treat them as synonyms.
- **DeckStream is a separate Steam Deck NAS/WebDAV launcher**, not an ARM64 execution engine. Keep licence/source boundaries and homebrew-only scope explicit.

## Keep live information live

| Information | Authoritative source | Public-facing behaviour |
| --- | --- | --- |
| Latest published version and assets | Each repository's [GitHub Releases](https://github.com/hffgzktbcc-blip/ounce-core/releases/latest) | Use the repo's own `/releases/latest` link and release badge, not a frozen “latest vX.Y.Z” sentence |
| What the code actually does | Code, CLI, feature tests and known limits at the actual commit | Update README / Quick Start / limits **in the same PR** as a user-visible change |
| What changed since a release | `CHANGELOG.md` under `Unreleased`, then a dated published section | Add a plain-English outcome when behavior changes; reconcile before tagging/publishing |
| Current work and blockers | Live GitHub issues and PRs, local active plan | Link open issues; never recycle closed PR numbers as “in progress” |
| Family priorities | [Ounce Family roadmap](roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md) | Update active milestones when evidence changes; the roadmap is **strategy**, not proof of shipping |
| Benchmarks, research, historical test results | Dated reports and exact commits/assets | Keep their original values and dates; link from live pages to new evidence rather than overwriting history |

## Definition of done for every change

1. Check current `main`, releases, relevant issues and existing docs **before changing** anything.
2. If a PR changes a command, flag, CLI report, supported use case, installation step or visible behaviour, update the relevant **README, setup/Quick Start, limits and `Unreleased` changelog in the same PR**. Only edit the ones actually affected; no padded status essays.
3. In the PR say **which public docs changed**. If none were needed, say `Docs: none — <concrete reason>`. The family’s lightweight CI guard checks that decision for user-facing source changes.
4. On a release, verify its tag, downloadable assets and date. Change the corresponding changelog section to **published**. Link `/releases/latest` from live entry points instead of manually copying a version number through four repositories.
5. Check public links and the deployed page when website content changes. A merged PR is not proof the Pages deploy succeeded.
6. If cross-project behaviour changes, check each affected sibling’s public explanation and compatibility claims. Update only repositories with proven changes, on separate PRs.
7. Don't imply a tester, GitHub star, CI job or bot is an **independent reproduction**. Report failures and unknowns plainly.

**Operating principle:** no agent swarm, no manually synchronised version banners, no retrospective rewrite of evidence. Keep the explanation simple and stop at the smallest verified change.
