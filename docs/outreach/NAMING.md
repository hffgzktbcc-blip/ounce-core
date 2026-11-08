# Naming and discoverability

Research date: 2026-11-08. This is engineering research, not legal advice. A trademark search is the owner's to do in an official database (USPTO TESS/Search, EUIPO eSearch, WIPO Global Brand Database, UKIPO). Nothing here replaces that.

Method: the WebSearch tool in standard mode (a general search index, not Google itself, so coverage is partial), plus plain fetches of public registry pages and APIs with the default client identity. No user-agent spoofing, no bot-check bypass. Absence of a hit means "not found by this tool", not "does not exist".

## Findings

### 1. "ounce core" (plain phrase)
Results are all unit-of-weight and unrelated "core" pages (bowling ball cores, drill core bits, ticker pages, spam "troy ounce" pages). No product, company or project uses "ounce core" as a name. Source: search `"ounce core"`; example hits https://en.wikipedia.org/wiki/Ounce, https://en.wikipedia.org/wiki/Core_(manufacturing).
Implication: the user's "unrelated results" is a semantic-noise problem (a common word plus a common word), not a name collision. Nobody else owns the term in this index.

### 2. "ounce-core" / GitHub
- GitHub repository search `ounce-core in:name` returns exactly 1 repo: ours (hffgzktbcc-blip/ounce-core, 206 stars at fetch time). Source: https://api.github.com/search/repositories?q=ounce-core+in:name
- General web search finds no other repo. Near-miss only: `pounce-lang/core` (npm `@pounce-lang/core`, a concatenative language). Different name, no confusion beyond fuzzy matching. Source: https://kandi.openweaver.com/typescript/pounce-lang/core

### 3. "ounce core emulator" / "Ounce-Core ARM64"
- The top hit for `"ounce core" emulator ARM64` is a third-party critical article, "ounce core switch 2 emulator" on heldgames.com: https://heldgames.com/guides/ounce-core-switch-2-emulator . It is about this project. As fetched, it describes it as a Switch 2-related research harness, says it is not a console emulator, questions the value of self-written tests, mentions an earlier provenance cleanup and a pending history rewrite, notes commits attributed to Claude, flags an unrelated repo on the same account, and repeats our README's malware-impersonation warning. Treat it as a claim by a third party; verify before responding to any of it.
- This is the real discovery problem: for emulator-flavoured queries the most prominent result is not our README. Some of its points are stale relative to the current README (the project is no longer pitched as a Switch runtime; see docs/VISION.md).
- `"Ounce-Core" ARM64 verification` returns nothing about us (unrelated ARM64 verification papers). We have essentially zero indexed pages using that phrasing yet. Source: search results (seL4, LFI paper arXiv 2508.15898, etc.).

### 4. Adjacent names
- Ounce Labs: a source-code security analysis vendor, Waltham MA, founded 2002, acquired by IBM on 28 July 2009. A different product ("Ounce 5"), and the company no longer exists as an independent brand. Source: https://en.wikipedia.org/wiki/Ounce_Labs . Same broad field (code analysis) but a 15-year-old dormant mark; any trademark status is for the owner to check.
- Oboromi (https://www.mintlify.com/0xNikilite/oboromi/core/cpu-emulation): unrelated Switch 2 emulator; surfaces in the same searches only through topic overlap.

### 5. Registries
| Registry | Name | Result | Source |
|---|---|---|---|
| crates.io | ounce-core | 404 (no such crate; page is a JS app, so API checked) | https://crates.io/api/v1/crates/ounce-core |
| docs.rs | ounce-core | 404 | https://docs.rs/ounce-core |
| npm | ounce-core | 404 | https://registry.npmjs.org/ounce-core |
| PyPI | ounce-core | 404 | https://pypi.org/pypi/ounce-core/json |

The name is free on all four registries. Squatting risk is real: the README itself warns about malware labelled "ounce-core". Consider reserving (owner's decision, not done here).

## Collision risk table

| Surface | Collision | Risk | Note |
|---|---|---|---|
| Web search, plain phrase | None, but generic words | Low (noise) | Disambiguate with context words |
| GitHub | None | Low | Only repo of that name |
| crates.io / docs.rs / npm / PyPI | None, name free | Low | Squat/impersonation risk if unclaimed |
| Companies | Ounce Labs (IBM, 2009) | Low-medium | Dormant brand, adjacent field; owner to check marks |
| Search reputation | Critical third-party article ranks first for emulator queries | Medium | A content problem, not a name problem; a rename would not remove it and would break continuity |
| Trademark | Unknown | Unknown | Owner to search official databases (classes 9 and 42) |

## Candidate alternative names (same checks)

Checked names were invented for this exercise; "free" means 404 on the registry APIs and no web hit.

| Candidate | crates.io | npm | PyPI | Web search | Note |
|---|---|---|---|---|---|
| a64lens | 404 | not checked | 404 | no hits | Descriptive (AArch64 plus lens), awkward to say |
| armproof | 404 | 404 | not checked | no hits | Very clear, but "proof" overclaims for experimental software and "arm" invites Arm Ltd trademark attention |
| tracewright | 404 | 404 | not checked | no hits (nearest: TraceLens, trace-ui) | Distinct, pronounceable, suits the divergence-tracing pillar |

Sources: https://crates.io/api/v1/crates/a64lens , /armproof , /tracewright ; https://registry.npmjs.org/armproof , /tracewright ; https://pypi.org/pypi/a64lens/json . I did not check domains, GitHub orgs, or trademarks for these.

## Recommendation

Keep the name. Fix discoverability with disambiguating wording and content. Reasons:
1. No real collision exists; the noise comes from two common words, which context terms solve.
2. The most visible negative result is about the project's history and claims, and follows the project, not the name. A rename would reset 200+ stars of continuity, break links and badges, and look like evasion.
3. The strongest remedy is a current, honest, indexable page that owns "Ounce-Core ARM64 verification" plus a short FAQ that corrects stale impressions factually (what it is and is not, current provenance state, what the tests do and do not prove).
4. Reserve the crate/npm/PyPI names only if the owner wants to defend against impersonation.
5. Revisit a rename only if the owner's trademark search finds a live conflicting mark in software classes. If so, `tracewright` is the best of the three candidates (pending the same checks on domains and marks).

## Low-cost SEO plan (keep the name)

Always write "Ounce-Core" (hyphenated, capitalised) and pair it with a qualifier: "ARM64", "AArch64", "verification", "differential testing", "safe Rust".

1. Page titles (site/*.html `<title>`, 50-60 chars):
   - Home: `Ounce-Core: ARM64 execution and verification in safe Rust`
   - Proven: `Ounce-Core ARM64 verification: what is proven, with hashes`
   - Docs/quickstart: `Ounce-Core quickstart: run and diff an ARM64 program`
   - Add a program: `Add a program to Ounce-Core's ARM64 verification corpus`
2. Meta description (about 155 chars): `Open-source ARM64 (AArch64) execution lab in safe Rust. Compares interpreter, JIT and native engines against real ARM64 hardware and reports where they diverge.`
3. H1/first paragraph on each page: lead with the phrase "Ounce-Core ARM64 verification".
4. FAQ block on the site home: "Is Ounce-Core a console emulator?" (No; state plainly), "What do the 50/50 oracles prove and not prove?", "How is provenance handled?" Link to docs/PROVENANCE.md and docs/CLEAN_ROOM.md. Factual, no rebuttal tone.
5. JSON-LD for the site home (`<script type="application/ld+json">`):
```json
{
  "@context": "https://schema.org",
  "@type": "SoftwareSourceCode",
  "name": "Ounce-Core",
  "alternateName": ["ounce-core", "Ounce-Core ARM64 verification"],
  "description": "Open-source ARM64 (AArch64) execution, debugging and verification platform in safe Rust.",
  "codeRepository": "https://github.com/hffgzktbcc-blip/ounce-core",
  "programmingLanguage": "Rust",
  "license": "https://opensource.org/licenses/MIT",
  "applicationCategory": "DeveloperApplication",
  "runtimePlatform": "AArch64 / ARM64"
}
```
6. Add `robots.txt` and `sitemap.xml` to the site; submit the sitemap in Google Search Console and Bing Webmaster Tools (owner action).
7. One technical write-up page, "Finding emulation bugs by diffing against Apple Silicon", containing the real bug examples already in the README. Cross-link from README and site.
8. Add a `CITATION.cff` so the name appears in citation indexes.
9. Official-identity line in README: "The only official source is github.com/hffgzktbcc-blip/ounce-core" (the malware warning already exists; keep it near the top).
10. Release assets: consistent name `ounce-core-<version>-<target>` with checksums, to make impostors easier to spot.

### Proposed GitHub repo settings (owner applies; not changed here)

Description (under 350 chars):
> Ounce-Core: open-source ARM64 (AArch64) execution and verification lab in safe Rust. Runs programs on an interpreter, JIT and native engines and diffs them against real ARM64 hardware to show where they diverge.

Website: the project's site URL.

Topics (all lowercase, max 20):
`arm64`, `aarch64`, `rust`, `emulator`, `interpreter`, `jit`, `differential-testing`, `verification`, `debugging`, `cpu-emulation`, `instruction-set`, `apple-silicon`, `safe-rust`, `reproducible-builds`, `testing`

Avoid topics that attract the wrong crowd (`switch`, `nintendo`, `switch-emulator`).

### Awesome-list submissions to prepare (do NOT submit)
Each needs the owner's go-ahead and a check of that list's contribution rules (many require a minimum age, a clear description, and an alphabetical entry).
- awesome-rust (https://github.com/rust-unofficial/awesome-rust), category Emulators or Development tools > Testing. Entry: `[hffgzktbcc-blip/ounce-core](https://github.com/hffgzktbcc-blip/ounce-core) - ARM64 execution and differential-verification lab in safe Rust`.
- awesome-emulators-style lists (CPU or ARM sections), only where a non-console, verification-focused emulator fits.
- awesome-fuzzing / awesome-testing-style lists, under differential testing, only if the entry matches the list's scope.
- Prepare a one-line entry, a 3-line pitch, and a link to the "what is proven" section for each. Submit manually, one at a time, after the site FAQ and title changes are live.

## What I did not verify
- Domains, GitHub organisation names, and trademarks for any name (including Ounce-Core).
- Whether the crates.io/npm/PyPI 404s will remain so; recheck before relying on them.
- The accuracy of the third-party article's claims; they are summarised as reported.
- Search coverage beyond the single tool used; run the same queries in a logged-out browser for a ground-truth view.
