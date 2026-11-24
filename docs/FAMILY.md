# Which Ounce project should I use?

**Two repositories are active; three earlier siblings are archived.** Start with the result you want, not the technology underneath. GitHub archive status was checked on 2026-11-24.

| I want to… | Choose | What it does today | What it does **not** prove |
| --- | --- | --- | --- |
| Check whether a **supported ARM64 program** gives the expected answer | [Ounce-Core](https://github.com/hffgzktbcc-blip/ounce-core) (active) | Runs selected programs on multiple CPU engines, compares results with recorded hardware references and helps investigate mismatches | That arbitrary ARM64 or Linux applications work; internal CI is not an outside reproduction |
| Try launching **PC games I already own from my NAS on Steam Deck** | [DeckStream](https://github.com/hffgzktbcc-blip/deckstream) (active, separate project) | Implements a SteamOS NAS/WebDAV launcher and phone companion; maintains local/automated test evidence | A verified clean-device Steam Deck launch or independently measured LAN performance |

DeckStream is not an ARM64 emulator and does not depend on Ounce-Core's verification results.

## Archived projects

[ounce-hb](https://github.com/hffgzktbcc-blip/ounce-hb), [ounce-runtime](https://github.com/hffgzktbcc-blip/ounce-runtime) and [Ounce-Spark](https://github.com/hffgzktbcc-blip/Ounce-Spark) are archived on GitHub: read-only, no new releases or fixes. Their code and releases stay available as a record. This repository's schema documents still mention `ounce-hb` and `ounce-runtime` where they describe who wrote or consumed a data format.

## Where to start

- **Ounce-Core:** [See how it caught Lua returning a wrong mathematical result](community/LUA_FP_BUG_WALKTHROUGH.md). No install required to read the evidence. [Try a separate-hardware reproduction](https://github.com/hffgzktbcc-blip/ounce-core/issues/248) if you have the right equipment.
- **DeckStream:** [Try the local demo](https://hffgzktbcc-blip.github.io/deckstream/) or [report a real Steam Deck and NAS/WebDAV setup](https://github.com/hffgzktbcc-blip/deckstream/issues/9).

Release links: [Ounce-Core](https://github.com/hffgzktbcc-blip/ounce-core/releases/latest) (MIT), [DeckStream](https://github.com/hffgzktbcc-blip/deckstream/releases/latest) (MIT; older tags may predate current own-storage work). The [public documentation contract](PUBLIC_DOCS_CONTRACT.md) says how each project keeps its explanation current; **measured results and historical release notes remain dated**.
