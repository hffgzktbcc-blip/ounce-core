# Sources and legal guardrails

Status: draft, not legally reviewed. This replaces the earlier clean-room protocol (owner decision 2026-11-03).
ounce-core is **no longer a clean-room project**: it may port MIT-licensed emulator code, with credit.

## Allowed sources
Cite each one in `docs/PROVENANCE.md` when used.
- **Arm Architecture Reference Manual** (ARMv8/9 A-profile): CPU semantics and encodings.
- **Mesa NAK** (MIT) and **Nouveau/envytools**: SASS encodings and GPU classes. MIT code may be ported with its
  notice kept in `THIRD_PARTY.md`.
- **libnx, deko3d, devkitPro examples** (public homebrew source).
- **nxnative and Ryujinx** (MIT): may be read and ported to Rust. Keep their notices in `THIRD_PARTY.md` and
  record each port in `docs/PROVENANCE.md`.
- **Libraries:** wgpu/naga, SPIRV-Cross, standard crates.
- **Observation** of outside homebrew running in our runtime, and reference runs of outside homebrew (never
  retail games) in other emulators, with emulator and version recorded in `docs/PROVENANCE.md`.

## Not allowed
- GPL emulator code (yuzu, Eden, Eglantine and similar).
- Code, logs or traces from non-public projects.
- Retail games, game profiles or game-specific patches, keys, firmware, system archives, dumps, leaked material,
  Nintendo SDK code, or anything derived from them.

## Legal guardrails (every contributor, every commit)
Practical rules drawn from what got other emulators sued. Not legal advice. A PR that breaks one is rejected.
1. **Never decrypt.** No retail key handling, no NCA/NSP/XCI decryption. `RestrictedCipher` stays. ounce-core
   accepts only already-decrypted inputs: homebrew (NRO), and loose `exefs/` + `romfs/` folders the user
   dumped and decrypted on their own console.
2. **No piracy pointers:** no links to or mentions of ROM, key or dump sites in code, docs, issues, PRs,
   releases or the website.
3. **No game names or game screenshots.** Describe capability neutrally ("runs Switch software from
   user-provided decrypted dumps"), without hiding what the code can do.
4. **No retail games as test inputs**, ever. Game-specific work never happens here.
5. **No money tied to games:** no paid builds, tiers or early access that unlock game support.
6. **Nothing from non-public projects** (code, logs, traces or specs observed from retail software).
7. **Keys, games, firmware and shader caches compiled from games are never committed**, not even in tests or
   CI artifacts. `.gitignore` covers `*.keys`, `dumps/`, `*.nsp`, `*.xci` and `*.nca`, and the
   `scripts/check_legal_guardrails.sh` CI job fails if such files or known ROM/key-site links appear.

## Working rules
- Every new fixture or corpus program has a row in `docs/PROVENANCE.md` before merge: source, commit, licence,
  how it was built.
- Progress is claimed only when an outside program passes a pre-registered pixel oracle in CI (see `PLAN.md`).
- Retail decryption stays disabled (`LoadError::RestrictedCipher`).

## Honest limits
- These rules do not make an activity lawful. Interoperability and anti-circumvention law differ by country.
  Get advice from a qualified IP lawyer before wider promotion.
- The git history problem in `docs/PROVENANCE.md` is not resolved by this document.
