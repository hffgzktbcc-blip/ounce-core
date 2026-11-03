# ounce-core: THE plan (single source of truth)

Public project: `github.com/hffgzktbcc-blip/ounce-core` (MIT, Rust). This file is the only plan.
Every agent (Claude, Antigravity, Codex) reads it first, does **only the next unchecked step**, ticks it with
proof, and ends. Changing the plan needs the owner's OK first, recorded here. Old handoff notes are history only.

## Goal
Run open-source Switch homebrew on macOS Apple Silicon (and Linux/Windows), with every claim backed by
a real program's real frame.
**Headline metric:** "N outside homebrew programs render their first frame pixel-exact." That's 4 today
(simplegfx, deko_basic, romfs, startup).

## Rule 0: no bullshit (overrides everything, for every agent)
1. **No synthetic tests as progress.** Unit tests may guard code, but they never count as a milestone or a
   "Verified" claim. Progress = a real outside program (one we didn't write) reaching a frame that matches a
   pre-registered pixel oracle, in CI.
2. **Proof or it didn't happen.** Never write "running", "works" or "verified" without a pixel-oracle pass or a
   screenshot of real frames. Instruction counts, "N tests pass" and "the window opened" are not proof.
3. **One plan, one step per session.** Read this file, do the next unchecked step, tick it with one line of proof
   (commit, CI run, oracle), then end. No side quests, no new plan files, no mid-step hand-offs between agents or
   models.
4. **Ask the owner only for what needs their hands** (accounts, passwords, approvals). Pick sensible defaults
   for everything else. One question at a time.
5. **Never undo or delete other agents' or the owner's work** without asking. Build on what exists.
6. **Check the environment before heavy work:** `df -h ~`. Work only in `~/Projects` (`~/Documents` is
   iCloud-synced and stalls builds and git). Run long jobs in the background.
7. **Keep retail out.** No retail games, game profiles or game-specific patches, keys, firmware or dumps ever
   enter this repo. MIT engine code (nxnative, Ryujinx) may be ported in, with credit (see the rules below).

## Sources: what we may use (this is where the speed comes from)
Allowed, and cited in `docs/PROVENANCE.md` whenever used:
- **Arm Architecture Reference Manual** (ARMv8/9 A-profile): CPU semantics and encodings.
- **Mesa NAK** (MIT; Mesa's Rust shader compiler for NVIDIA Turing/Ampere) and **Nouveau/envytools**: SASS
  encodings and GPU classes. MIT code may be ported directly, keeping its notice in `THIRD_PARTY.md`.
- **libnx, deko3d, devkitPro examples** (public homebrew source): what programs ask of the system and GPU.
- **Permissive emulator and engine code:** nxnative and Ryujinx (MIT) may be read and ported.
- **Libraries:** wgpu/naga, SPIRV-Cross, standard crates.
- **Our own observation** of outside homebrew running in our runtime.

**Reference runs are allowed:** run outside homebrew (never retail games) in any emulator and compare its frames
and logs with ours; record emulator and version in `PROVENANCE.md`.

**Port MIT code that works (owner decision 2026-11-03):** ounce-core may read and port **nxnative** (MIT, seedds)
and **Ryujinx** (MIT) to Rust instead of re-deriving everything. Keep their notices in `THIRD_PARTY.md` and note
each port in `PROVENANCE.md`. Because of this, ounce-core is **no longer labelled clean-room**. Step 1 makes the
README and `docs/CLEAN_ROOM.md` say so honestly.

Still not allowed: code from the owner's non-public projects; GPL emulator code (yuzu, Eden, Eglantine); retail games, game profiles or game-specific patches,
keys, firmware, dumps, leaked material, Nintendo SDK code.

## Legal guardrails (public repo, every agent, every commit)
Practical rules drawn from what got other emulators sued (yuzu, 2024). Not legal advice, but they apply to every
change. A PR that breaks one is rejected.
1. **Never decrypt.** No retail key handling, no NCA/NSP/XCI decryption. `RestrictedCipher` stays. ounce-core
   accepts only **already-decrypted** inputs: homebrew (NRO), and loose `exefs/` + `romfs/` folders the user
   dumped and decrypted on their own console.
2. **No piracy pointers:** no links to or mentions of ROM, key or dump sites, in code, docs, issues, PRs,
   releases or the website.
3. **No game names or game screenshots.** Describe capability neutrally: "runs Switch software from
   user-provided decrypted dumps". Don't hide what the code can do either. Neutral and honest, not secret.
4. **No retail games as test inputs**, ever. Compatibility with commercial software may grow as
   a side effect of public specs, homebrew and observation. It's never debugged against retail games here.
   Game-specific work never happens here.
5. **No money tied to games:** no paid builds, tiers or early access that unlock game support.
6. **Nothing from the owner's non-public projects** (code, logs, traces, specs observed from retail software)
   enters this repo. That code was debugged against retail software, so it stays out even though it's the owner's.
7. **Keys, games, firmware and shader caches compiled from games are never committed**, not even in tests or CI
   artifacts. `.gitignore` covers `*.keys`, `dumps/` and `*.nsp`/`*.xci`/`*.nca`.

## Steps (one step = one session)
- [x] **1. Honest README.** Replace every "Verified" that rests on unit tests with "tested". Add the headline
      metric with links to the 4 oracle runs. Remove Switch 2 retail claims from this repo.
      Retire `docs/CLEAN_ROOM.md`'s from-scratch rules in favour of "Sources" above (MIT ports allowed, no GPL,
      no retail).
      *Done when:* the README states only what a real program proved, the docs match practice, and CI is green.
      *Proof:* [PR #131](https://github.com/hffgzktbcc-blip/ounce-core/pull/131), CI green: https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41693005314
- [x] **1b. Legal guardrails into the repo.** Add the guardrails above to `docs/CLEAN_ROOM.md` and
      `CONTRIBUTING.md`. Add the `.gitignore` entries. Add a CI check that fails if the repo gains key files or
      retail container files, or links to known ROM/key sites. Audit README, docs and issues for game names and
      piracy pointers, and remove them.
      *Done when:* the CI check is green on main and the audit finds nothing.
      *Proof:* [PR #131](https://github.com/hffgzktbcc-blip/ounce-core/pull/131) adds `scripts/check_legal_guardrails.sh` ("Legal guardrails" job, green: https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41693005314); audit found no game names, piracy links or issue templates.
- [ ] **2. Homebrew corpus, 4 → 20.** Add outside programs: devkitPro `switch-examples` (graphics, input, audio,
      fs), more deko3d samples, and a few popular open-source homebrew apps. Build them reproducibly (devkitA64
      docker), record source, commit and licence in `PROVENANCE.md`, and pre-register a first-frame oracle for
      each.
      *Done when:* 20 oracles exist in CI, each pass or fail recorded honestly. Failures are the to-do list.
- [ ] **3. Fix by corpus.** Take the failing programs one by one, cheapest first. Each fix lands with that
      program's oracle turning green. No speculative features.
      *Done when:* metric ≥ 15/20.
- [ ] **4. Native execution on Apple Silicon (NCE).** Rewrite a homebrew NRO's `svc` and
      `TPIDR_EL0`-touching instructions into trampolines, and run the rest of its ARM64 code directly in
      `MAP_JIT` memory. Port what already works (nxnative's NSO patcher, MIT) rather than designing
      from scratch.
      *Done when:* every green oracle still passes under `--cpu=native`, with a measured speedup.
- [ ] **5. SASS accuracy from NAK.** Audit `src/gpu/sass` decoding against NAK's documented Turing/Ampere
      encodings (port NAK code where it fits, notice kept). Translate to WGSL via naga where that removes hand-written code.
      *Done when:* corpus programs with shaders pass, and no oracle regresses.
- [ ] **6. Merge #115** (texture mips/LOD/bias/clamp) once a corpus program needs it and its oracle passes.
- [ ] **7. Corpus 20 → 50**, then repeat step 3. The metric keeps climbing in public.
- [ ] **8. Switch 2 homebrew.** As soon as public, open-source tools can build Switch 2 ("Ounce") homebrew, add
      those programs to the corpus with pre-registered oracles, exactly like step 2. Prepare in the meantime by
      porting nxnative engine code (NVN on Metal, native execution) without game-specific parts.
      *Done when:* the first outside Switch 2 homebrew program passes its oracle in CI.
      Commercial Switch 2 games never enter this repo.

- [ ] **9. Make the unique parts visible.** Only after step 1 has made the README honest:
      - a README section (or blog post) on the pre-registered pixel-oracle method: expected frames fixed before
        the run, CI fails on a single pixel, interpreter and JIT must agree bit for bit;
      - a short note on the safe-Rust design, and where `unsafe` is now allowed (porting, native execution);
      - badges or a small table: oracle pass count, CI status on all three OSes, the safe-Rust status.
      *Done when:* each claim links to its CI run or oracle file, and nothing claims more than the evidence shows.

## How we work (what got us unstuck)
- **Single source of truth:** this file. Agents' own notes and handoffs are history, not plans.
- **Real frames only:** pixel oracles in CI decide progress.
- **One step per session:** short sessions save usage and stop drift.
- **Environment first:** disk space, `~/Projects`, background jobs.
- **Agents test themselves.** The owner is never the test dummy. Use scripted input and automated frame capture.
