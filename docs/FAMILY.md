# Which Ounce project should I use?

**Four independent projects solve different problems.** Start with the result you want, not the technology underneath.

| I want to… | Choose | What it does today | What it does **not** prove |
| --- | --- | --- | --- |
| Check whether a **supported ARM64 program** gives the expected answer | [Ounce-Core](https://github.com/hffgzktbcc-blip/ounce-core) | Runs selected programs on multiple CPU engines, compares results with recorded hardware references and helps investigate mismatches | That arbitrary ARM64 or Linux applications work; internal CI is not an outside reproduction |
| Run **homebrew I built from source** and understand an error | [Ounce-HB](https://github.com/hffgzktbcc-blip/ounce-hb) | Supervises supported unencrypted NRO programs through an external Ounce-Core engine and reports why a run stopped | That it can launch retail software or that recognising an NSO means it can execute it |
| Experiment with **faster ARM64 execution, graphics and input** | [Ounce-Runtime](https://github.com/hffgzktbcc-blip/ounce-runtime) | Explores optimized execution, rendering and controller integrations for bounded open-source workloads | General-purpose game compatibility or independent hardware verification |
| Try launching **PC games I already own from my NAS on Steam Deck** | [DeckStream](https://github.com/hffgzktbcc-blip/deckstream) | Implements a SteamOS NAS/WebDAV launcher and phone companion; maintains local/automated test evidence | A verified clean-device Steam Deck launch or independently measured LAN performance |

## One practical first step

- **Ounce-Core:** [See how it caught Lua returning a wrong mathematical result](community/LUA_FP_BUG_WALKTHROUGH.md). No install required to read the evidence. [Try a separate-hardware reproduction](https://github.com/hffgzktbcc-blip/ounce-core/issues/248) if you have the right equipment.
- **Ounce-HB:** [Run your own unencrypted homebrew](https://github.com/hffgzktbcc-blip/ounce-hb/blob/main/docs/QUICKSTART.md) and inspect the stop reason.
- **Ounce-Runtime:** [Build or try a supported execution test](https://github.com/hffgzktbcc-blip/ounce-runtime/blob/main/docs/QUICKSTART.md) and inspect the reported result.
- **DeckStream:** [Try the local demo](https://hffgzktbcc-blip.github.io/deckstream/) or [report a real Steam Deck and NAS/WebDAV setup](https://github.com/hffgzktbcc-blip/deckstream/issues/9).

## What is published?

These links **always point to the repository's latest published, non-prerelease GitHub release**; a commit on `main` is not a tagged release.

- [Ounce-Core releases](https://github.com/hffgzktbcc-blip/ounce-core/releases/latest) — MIT
- [Ounce-HB releases](https://github.com/hffgzktbcc-blip/ounce-hb/releases/latest) — MIT
- [Ounce-Runtime releases](https://github.com/hffgzktbcc-blip/ounce-runtime/releases/latest) — GPL-3.0-or-later
- [DeckStream releases](https://github.com/hffgzktbcc-blip/deckstream/releases/latest) — MIT (older tags may predate current own-storage work)

**Important boundaries:** These projects are separate repositories with separate releases, licences and evidence. DeckStream is not an ARM64 emulator, and it does not depend on Ounce-Core's verification results. The Ounce homebrew tools do not accept retail games, proprietary firmware or console keys. Licence boundaries between the GPL runtime and MIT siblings are handled through external processes and data contracts, not copying GPL code into MIT projects.

## How does this page stay current?

The [public documentation contract](PUBLIC_DOCS_CONTRACT.md) asks every user-facing PR to update its affected README, setup/limits and changelog, or give a concrete no-docs explanation. GitHub PR checks flag missing documentation decisions. The release links above stay live automatically, while **measured results and historical release notes remain dated**. No bot or CI badge can certify an independent reproduction.
