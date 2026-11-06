# ounce-core: THE plan (single source of truth)

Public project: `github.com/hffgzktbcc-blip/ounce-core` (MIT, Rust). This file is the only plan.
Every agent (Claude, Antigravity, Codex) starts with `scripts/next.sh` and works on the step it shows. Changing the plan needs the owner's OK first, recorded here. Old handoff notes are history only.

## Owner-directed Ounce-Verify evolution (2026-11-06)
The owner approved the proposed roadmap and requested execution in this session.
Implement sequentially alongside corpus expansion; preserve the pre-registered frame
oracles and the existing headline metric. The verifier decides results independently
of any solver. A repair never changes an oracle, drops a failing case, or weakens a gate.

- [x] **V1. Evidence integrity and complete coverage.** Verify actual NRO bytes against
      evidence hashes, reject missing execution observations as passes, and derive coverage
      from the registered corpus and exact required engines. Preserve v0 read compatibility.
      *Done when:* corruption and incomplete engine coverage cannot pass the integrity gate;
      existing corpus evidence still validates in CI with byte verification enabled.
      *Proof:* [PR #156](https://github.com/hffgzktbcc-blip/ounce-core/pull/156), [canonical corpus CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42161576657): all 40 interpreter and Tier-3 records passed pixel oracles with verified artifact bytes (20/20 programs).
- [x] **V2. Unified bounded execution.** Orchestrate existing build/run/verify tools with
      explicit limits, versioned manifests, retained raw observations and replayable commands.
      *Done when:* the outside corpus runs through one entry point with traceable evidence.
      *Proof:* [PR #157](https://github.com/hffgzktbcc-blip/ounce-core/pull/157), [canonical corpus CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42175855697): one bounded command produced a run manifest, 20 raw oracle observations and 40 byte-verified passing records (20/20 programs).
- [x] **V3. Diagnosis.** Compare runs, group failures, isolate first divergence and emit replay
      bundles with likely subsystem ownership; keep hypotheses separate from observations.
      *Done when:* a real outside-program failure has a reproducible, actionable diagnosis.
      *Proof:* [PR #158](https://github.com/hffgzktbcc-blip/ounce-core/pull/158) compares [passing corpus CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42175855697) with the [historical SDL2 failure](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41872194259); the bundle identifies missing frame 1, and a same-NRO replay reproduced both engines exiting at the same guest PC before the first frame.
- [x] **V4. Solve.** Propose repairs in isolated branches, run focused checks and the regression
      corpus, and retain before/after evidence. No self-approved oracle corrections.
      *Done when:* a failing outside program passes its unchanged oracle without regressions.
      *Proof:* [PR #159](https://github.com/hffgzktbcc-blip/ounce-core/pull/159) qualifies the isolated SDL2 repair from [17/20 reference CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41872194259) to [18/20 candidate CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42179288488): the identical NRO passed unchanged pixel oracles on interpreter and Tier-3, preserving all 17 prior passes.
- [x] **V5. Self-heal.** Recover known infrastructure failures with bounded retries and an audit
      trail; use rollback and explicit stop conditions for narrowly defined repairs.
      *Done when:* a real recovery succeeds unattended and unsuccessful attempts stop cleanly.
      *Proof:* [PR #165](https://github.com/hffgzktbcc-blip/ounce-core/pull/165) and [registry recovery CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42186494970/job/127787986260) show an actual refused Docker connection recovering on retry; focused tests confirm permanent errors and exhausted timeouts stop.
- [x] **V6. Scale.** Add sharding, content-addressed build caches, resumable jobs, bisection and
      scheduled regression/performance comparisons, using the growing 50-program corpus.
      *Done when:* CI demonstrates retained coverage and actionable regression evidence at scale.
      *Proof:* [Warm-cache CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42193209243/attempts/2) reused verified builds; [resume](https://github.com/hffgzktbcc-blip/ounce-core/pull/168) retained completed oracle work; [bisection](https://github.com/hffgzktbcc-blip/ounce-core/pull/169) isolated a real SDL2 first-frame transition; [comparison CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42196871177) passed with all 100 engine records across 50 homebrews, zero regressions, and a passing native corpus job. [Nightly workflow](https://github.com/hffgzktbcc-blip/ounce-core/pull/170) merged.

## Owner-directed deterministic execution follow-up (2026-11-06)
The owner requested continued evolution using real implementation and execution,
without model dependencies. This follow-up hardens the existing corpus runner:
malformed per-case JSON must preserve raw evidence, fail the invocation, and allow
remaining cases in every shard to finish and enter the aggregate report.
Checkpoint: implemented and checked with real host subprocess fault injection;
original code fails the new checks, updated code passes. These are runner recovery
checks, not guest compatibility evidence. No expected frame or headline changes.
Next: full repository gate and outside-corpus CI before shipping. This environment
has a partial connector snapshot and no Cargo; gate checkpoint/ship is unavailable.

## Owner-directed usability task (2026-11-05)
The owner requested the next practical step after the audit: a native Mac launcher.
Package the existing runtime, expose homebrew opening, session stop, SD folder and logs,
and verify actual guest frames through the ordinary GUI path. This does not change the
20/20 compatibility metric or complete the corpus expansion step.

2026-11-06 follow-up: distinguish launcher startup from first guest frame receipt and name the
NRO window after its file. Local GUI proof on Apple M3: the pinned application sample displayed
its Hello World frame through Metal, the launcher reported frame receipt, and Escape closed the session.

Launcher onboarding follow-up: tested the redesigned welcome screen on Apple M3. The file picker
rejected a mislabeled file, the same launcher opened the pinned Hello World sample and displayed its
guest frame, and Stop Session returned to the welcome actions. The built-in demo also opened.

Launcher boundary follow-up: the bundled runtime now validates NROs through a dedicated entry
point before executing guest bytes. On Apple M3 it rejected a mislabeled file before execution
and rendered the pinned Hello World sample through the Mac picker.

## Goal
Run open-source Switch homebrew on macOS Apple Silicon (and Linux/Windows), with every claim backed by
a real program's real frame.
**Headline metric:** "N/50 outside homebrew programs pass their registered first-frame pixel oracle": the README's oracle
badge, derived from the Ounce-Verify canonical report in CI. It is the only metric.

## Rule 0: no bullshit (overrides everything, for every agent)
1. **No synthetic tests as progress.** Unit tests may guard code, but they never count as a milestone or a
   "Verified" claim. Progress = a real outside program (one we didn't write) reaching a frame that matches a
   pre-registered pixel oracle, in CI.
2. **Proof or it didn't happen.** Never write "running", "works" or "verified" without a pixel-oracle pass or a
   screenshot of real frames. Instruction counts, "N tests pass" and "the window opened" are not proof.
3. **One plan, one step at a time.** Work only on the step `scripts/next.sh` shows; it closes with one line of proof
   (commit, CI run, oracle) written by `scripts/gate.sh --ship`. No side quests, no new plan files. Stopping mid-step
   is fine after `gate.sh --checkpoint`; the next agent continues from it.
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
- [x] **2. Homebrew corpus, 4 → 20.** Add outside programs: devkitPro `switch-examples` (graphics, input, audio,
      fs), more deko3d samples, and a few popular open-source homebrew apps. Build them reproducibly (devkitA64
      docker), record source, commit and licence in `PROVENANCE.md`, and pre-register a first-frame oracle for
      each.
      *Done when:* 20 oracles exist in CI, each pass or fail recorded honestly. Failures are the to-do list.
      *Proof:* [PR #134](https://github.com/hffgzktbcc-blip/ounce-core/pull/134): 20 oracles in `tests/corpus/oracles.json` (16 new ones pre-registered in `2fe006e` before any run), `first-frame-oracles` job green: https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41791374072 (on main with #115; canonical report: `ounce-verify-canonical-report` artifact, 40 `ounce-verify/v0` records, 15/20 pass on both engines). Owner-approved addition: Ounce-Verify evidence and README count from the report. Failures for step 3: sixaxis, deko-console, nx-hbmenu (unimplemented instructions), sdl2-simple, hybrid-app (exit before the first frame).
- [x] **3. Fix by corpus.** Take the failing programs one by one, cheapest first. Each fix lands with that
      program's oracle turning green. No speculative features.
      *Done when:* metric ≥ 15/20.
      *Proof:* [PR #145](https://github.com/hffgzktbcc-blip/ounce-core/pull/145): **17/20** from the canonical report of
      https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41872194259 (sixaxis: FP/SIMD forms; deko-console: DMA
      copy, inline-to-memory, 2D-array TEXS, fill rectangle, shadow RAM, +Y-down winding). deko-basic's observed frame-1
      hash was corrected with owner approval (it recorded the winding bug; see `docs/deko-basic-oracle.md`).
      Still failing, for step 7's repeat: nx-hbmenu (needs `bsd:u`/`nifm:u`, `NextLoadPath`, and a `pl:u` shared font,
      which requires bundling an open-licensed font), sdl2-simple and hybrid-app (Mesa over nvdrv exits before the
      first frame).
- [x] **4. Native execution on Apple Silicon (NCE).** Rewrite a homebrew NRO's `svc` and
      `TPIDR_EL0`-touching instructions into trampolines, and run the rest of its ARM64 code directly in
      `MAP_JIT` memory. Port what already works (nxnative's NSO patcher, MIT) rather than designing
      from scratch.
      *Done when:* every green oracle still passes under `--cpu=native`, with a measured speedup.
      *Proof:* [PR #137](https://github.com/hffgzktbcc-blip/ounce-core/pull/137) (native execution) and
      [PR #141](https://github.com/hffgzktbcc-blip/ounce-core/pull/141) (corpus oracles natively). CI on Apple M1:
      all 15 recorded passes pass under `--cpu=native` with frames equal to Tier-3
      (https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41795276178/job/126459320723), as do the four
      first-frame oracles (https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41795276178/job/126459469189).
      Wall time Tier-3 → native: simplegfx 41.6 s → 0.52 s (81×), the console/input/service programs 14–34×,
      deko_basic 1.4×. Code pages are sealed W^X with `mprotect`, not `MAP_JIT` (`MAP_JIT` cannot be `MAP_FIXED`,
      and guest code must sit at its guest address).
- [x] **5. SASS accuracy from NAK.** Audit `src/gpu/sass` decoding against NAK's documented Turing/Ampere
      encodings (port NAK code where it fits, notice kept). Translate to WGSL via naga where that removes hand-written code.
      *Done when:* corpus programs with shaders pass, and no oracle regresses.
      *Proof:* [PR #135](https://github.com/hffgzktbcc-blip/ounce-core/pull/135) (merged `3e660c6`), audited against
      Mesa NAK sm50/sm70 (mesa `f4a706c5a218`), envytools and Ryujinx, credited in `THIRD_PARTY.md`/`PROVENANCE.md`.
      Fixed Ampere `IPA` frequency codes, added Maxwell `RRO`, `XMAD.CSFU`, `PLOP3` negation, `MUFU` forms.
      All 4 oracles pass (deko_basic is the only shader program) in https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41703660643;
      CI green: https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41703660653.
      Partly met: step 2's shader programs (#134) had not landed, so only deko_basic's shaders are proven;
      re-check them when they land. Open: NAK vs CuAsm on Maxwell bound `TEX` opcode, `FSET` cbuf negate bit.
- [x] **6. Merge #115** (texture mips/LOD/bias/clamp). DONE 2026-11-03 by owner decision, ahead of a corpus program needing it: merged for release v0.4.0 (cb24c76, CI green).
- [x] **7. Corpus 20 → 50**, then repeat step 3. The metric keeps climbing in public.
      *Proof:* [PR #160](https://github.com/hffgzktbcc-blip/ounce-core/pull/160): 45/50 pass; failing: account, audio-echo, controller-colors, playtone, save-data ([canonical report](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42180887173)). Thirty new checks were pre-registered in 53b8a1d. All 100 engine records were byte-verified, all original 20 passed, and the Native job recorded the same 45/50. The five misses are recorded in tests/corpus/oracle-results.json.
- [x] **8. Switch 2 homebrew.** As soon as public, open-source tools can build Switch 2 ("Ounce") homebrew, add
      those programs to the corpus with pre-registered oracles, exactly like step 2. Prepare in the meantime by
      porting nxnative engine code (NVN on Metal, native execution) without game-specific parts.
      *Done when:* the first outside Switch 2 homebrew program passes its oracle in CI.
      Commercial Switch 2 games never enter this repo.
      *Proof:* [Switch 2 oracle CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42201830705) built the pinned outside Hello CPU package and matched all four pre-registered first-frame pixels on Interpreter and Tier-3, with byte-identical software-rendered frames.

- [x] **9. Make the unique parts visible.** Only after step 1 has made the README honest:
      - a README section (or blog post) on the pre-registered pixel-oracle method: expected frames fixed before
        the run, CI fails on a single pixel, interpreter and JIT must agree bit for bit;
      - a short note on the safe-Rust design, and where `unsafe` is now allowed (porting, native execution);
      - badges or a small table: oracle pass count, CI status on all three OSes, the safe-Rust status.
      *Done when:* each claim links to its CI run or oracle file, and nothing claims more than the evidence shows.
      *Proof:* [PR #139](https://github.com/hffgzktbcc-blip/ounce-core/pull/139) (merged `9903f22`): README "How the proof works" and "Safe Rust by construction", evidence table and badges, each linked; the oracle badge is now checked against the canonical report like the headline. CI green: https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41795150292; oracles 15/20 with README check: https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41795150245.


## Owner-directed oracle hardening (2026-11-06)
The owner requested improving the oracle. Keep existing golden expectations and
compatibility counts unchanged while strengthening registration and observations.
- [x] **10. Reject vacuous and unreachable frame checks.** Require non-empty checks,
      reachable positive frame indices, positive dimensions, a valid whole-frame hash
      or bounded RGB probes, and explicit failures for missing captured frames. Retain
      observed probe coordinates and RGBA values in each raw workload report.
      *Done when:* all existing registrations validate, malformed definitions fail,
      the repository gate passes, and real outside frames still match unchanged oracles.
      suite passes. Local pinned-startup build blocked by Docker VFS disk exhaustion;
      the downloaded image was removed to recover space. No new frame pass claimed.
      Checkpoint (2026-11-06 13:41Z, codex, ea95b42): GREEN — tests: cargo test: 470 passed, 0 failed; suite: pass — Reject vacuous or unreachable oracle checks; retain observed probe pixels
      *Proof:* combined canonical corpus CI https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42231436126: 46/50 programs; all previous passes retained.

## Owner-directed runtime capability follow-up (2026-11-06)
The owner requested actual runtime improvements instead of additional models.
- [x] **11. Stop fabricating controller color metadata.** Keyboard-backed and generic
      controllers must report color-read unavailability unless their host input feed
      supplies device metadata. Publish updated metadata without a style change and
      clear stale colors on disconnect, style change or metadata removal.
      *Done when:* the unchanged outside controller-colors NRO passes its existing
      first-frame oracle on interpreter and Tier-3, the repository gate passes,
      and the canonical corpus shows no regressions in CI.
      Local proof: docs/verify/controller-colors-before-after.json retains the same NRO
      failing before and passing unchanged frame 1 on Interpreter and Tier-3 after.
      Repository gate: 469 tests passed, zero failed; runtime suite passed.
      Checkpoint (2026-11-06 13:48Z, codex, ea95b42): GREEN — tests: cargo test: 469 passed, 0 failed; suite: pass — Replace fabricated controller colors with host-metadata availability; real colors NRO now matches unchanged oracle on both engines
      *Proof:* combined canonical corpus CI https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42231436126 records controller-colors passing unchanged on both engines (46/50 total).


## How we work (owner decision 2026-11-04)
- **This file is the truth.** Notes, handoffs and memory are history, not plans.
- **Real frames decide progress:** a step closes on its pixel-oracle evidence in CI, never on unit tests.
- **Any agent continues any step:** `scripts/next.sh` shows where it stands; `scripts/gate.sh --checkpoint` saves (red or green) on the step's branch; `scripts/gate.sh --ship` ticks it with proof and GitHub auto-merges when CI is green.
- **A step ticked with proof is closed**, unless one of its oracles fails again.
- **Ask the owner only** about oracle corrections, legal doubt or anything destructive.

Integration proof (2026-11-05): PR #154 combines #149 with the correctness audit. The canonical report
from https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42033845756 records 40 passing records
(20/20 on Interpreter and Tier-3) for the combined code at `2d88c99` (tested through the PR merge ref).
The README metric was derived using scripts/corpus_verify.py. Step 7 (expand to 50) remains next.


## Owner-directed v0.5.0 release (2026-11-06)
The owner requested improvements to the oracle, Ounce-Verify and Switch 2 research,
followed by a release. Integrate #172–#176, preserve hardware/rendering boundaries,
and qualify the combined commit before tagging.
- [x] **12. Qualify and release v0.5.0.** Ship validated frame registrations, bounded
      malformed-evidence recovery, honest HID color availability and a structured
      HBLauncher SDK-path first-frame report. Publish all three tested platform
      archives together with checksums after full CI and canonical corpus evidence.
      Checkpoint (2026-11-06 14:29Z, codex, eb95697): GREEN — tests: cargo test: 471 passed, 0 failed; suite: pass — Record published v0.5.0 assets, checksums and final qualification links
      Next: Release complete; next work requires a new owner-directed scope

Release proof (2026-11-06): [v0.5.0](https://github.com/hffgzktbcc-blip/ounce-core/releases/tag/v0.5.0)
published from `eb95697845e44f35675cb0a4bfb0467ed50b264a` after all 21 candidate checks passed.
[Release workflow](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42233497706)
built and tested Linux x86-64, macOS arm64 and Windows x86-64 archives and published
all three with SHA256SUMS. Canonical corpus: 46/50 on both engines, all prior passes
retained; native corpus and the separately scoped HBLauncher SDK-path oracle passed.

## Owner-directed verifier independence and app usability (2026-11-06)
The owner reaffirmed all six phases and requested a front end ordinary users can
understand. Existing V1–V6 proofs remain the baseline, not a claim of universal
correctness or autonomous repair. No model-provider dependencies are introduced.
- [x] **13. Preserve the trust boundary and expose saved results in the app.** Reject
      workload target/provenance relabeling during repair qualification; protect cache,
      comparison and regression policy inputs; require a clean committed checkout for
      bounded execution; declare headline target and required engines in the manifest.
      Independently hash Switch 2 SDK package bytes when validating its reports.
      Add a native, accessible saved-results viewer that never computes verifier verdicts
      or grants repair success. Clearly distinguish recorded results from fresh checks.
      *Done when:* adversarial policy tests pass, the 50-case corpus keeps its coverage,
      and the Mac bundle compiles with report decoding tests. Interactive Mac GUI
      acceptance remains separately recorded; compilation is not a guest-frame claim.
      Checkpoint (2026-11-06 15:15Z, codex, 39570fc): GREEN — tests: cargo test: 471 passed, 0 failed; suite: pass — Record workbench trust-boundary, Mac preview and unchanged corpus qualification
      Next: Interactive Mac usability review; future runtime fixes remain corpus-driven

Phase acceptance stays tied to the existing V1–V6 evidence: artifact/coverage checks;
bounded execution; replayable diagnoses; independent same-NRO repair qualification;
bounded transient recovery; cached/sharded/resumable runs and bisection. New failures
in the 50-program corpus drive subsequent fixes. Oracle corrections require an explicit
owner decision and are never a solver option or a control in the results viewer.


Workbench proof (2026-11-06): [PR #179](https://github.com/hffgzktbcc-blip/ounce-core/pull/179)
merged after all 18 hosted checks passed. The [canonical and native corpus](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42238034141)
retains 46/50 passes with all workload expectations unchanged. The [Mac job](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42238034085)
passed decoder, bundle, signature and runtime smoke checks and retained macos-app-preview.
The [SDK byte-validation run](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42238034210/attempts/2)
passed after one retry of a timed-out package-mirror download. No checks or source were
changed for that retry. Interactive Mac usability/accessibility acceptance is not claimed.

## Owner-directed guest GPU execution (2026-11-06)
- [x] **14. Execute supported guest draws through a real host graphics API.** Wire the
      Maxwell/Ampere SASS-to-WGSL translator into offscreen host draw submission and
      read back into the original guest render target. Keep adapter identity, software
      adapters, unsupported state and completed readbacks visible; never count replay
      or a bookkeeping facade as hardware execution. Keep the CPU path available.
      *Done when:* the unchanged outside deko3d workload passes its existing oracle
      with recorded host submissions/readbacks on both CPU engines and a physical
      GPU. Preserve expected frames and report any remaining failures honestly.
      *Proof:* [PR #182](https://github.com/hffgzktbcc-blip/ounce-core/pull/182): 46/50 pass; failing: account, audio-echo, playtone, save-data ([canonical report](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42274752750)). Guest SASS executes through Metal on the reported IntegratedGpu Apple Paravirtual device and software Vulkan: both engines match every unchanged deko3d frame, with five completed draws and 18,432,000 readback bytes each. Metal proof: https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42274752750/job/128091109327 . Linux proof: https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42274752750/job/128091109496 . All 23 implementation checks passed; 100 canonical binary records independently byte-verified. Broader Ampere app and native Tegra coverage remain unestablished.
