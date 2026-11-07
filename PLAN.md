# ounce-core: THE plan (single source of truth)

## Owner-directed interactive execution (2026-11-07)
The owner approved moving beyond startup checks, beginning with the unchanged
pinned deko3d examples menu and its Triangle selection. Keep the existing 50-case
first-frame corpus, expected pixels and headline unchanged.
Owner decision (2026-11-07): after being shown the (320,600) arithmetic error,
the owner directed “Solve and fix”, approving the interaction-only probe correction
from (42,139,74) to (42,170,43), derived from the already registered geometry formula.
Retain the initial failed evidence; do not change any of the 50 headline oracles.
- [x] **15i. Drive real homebrew beyond its menu.** Add queued-frame input checkpoints
      to the existing bounded test runner: neutral startup, Down, release, A,
      release. Register Triangle pixels from pinned upstream geometry and shaders
      before execution. Retain applied input transitions, NRO hash, frame hashes,
      adapter/draw/readback evidence and independent Interpreter/Tier-3 results.
      Implementation sequence: guard checkpoint parsing and application with focused
      tests; extend the runner and separate interaction registry; run unchanged
      upstream NRO; repair only reproduced failures; add bounded CI execution and
      retained report; run repository gate and canonical regression corpus.
      *Done when:* both CPU engines reach the registered Triangle pixels, agree on
      captured frames, complete host draws/readbacks (physical GPU separately
      identified), and CI retains every existing corpus pass. Unit checks alone
      never close this step; unsuccessful execution remains recorded evidence.
      *Proof:* [PR #194](https://github.com/hffgzktbcc-blip/ounce-core/pull/194): 46/50 pass; failing: account, audio-echo, playtone, save-data ([canonical report](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42372372068)). Both unchanged-NRO menu-to-Triangle interaction jobs passed: Metal on the reported Apple Paravirtual IntegratedGpu adapter and Linux host API, exact Interpreter/Tier-3 pixel and frame parity, four completed draws and 14,745,600 readback bytes per engine. Original failed reports retained; owner-approved arithmetic correction independently guarded. All hosted native and canonical corpus checks passed.

Public project: `github.com/hffgzktbcc-blip/ounce-core` (MIT, Rust). This file is the only plan.
Every agent (Claude, Antigravity, Codex) starts with `scripts/next.sh` and works on the step it shows. Changing the plan needs the owner's OK first, recorded here. Old handoff notes are history only.

## Owner-directed next phase: independent proof and practical value (2026-11-07)

**Owner approval:** On 2026-11-07 the owner requested that the deep-research and code-audit recommendations become a concrete plan in this canonical file. This is an approved *scope and order*, **not** approval to weaken verification policy, change golden oracles, rewrite Git history, publish disputed artifacts, or automatically mark any item done. The completed steps 1–14 and V1–V6 below remain historical, evidence-backed milestones.

**Product thesis:** Make ounce-core an inspectable execution-and-verification laboratory for permitted AArch64 homebrew: *Execute. Observe. Understand. Verify.* Compatibility is only claimed for the specific independently sourced workload and behavior actually covered by a registered oracle. Ounce-Verify is a trust boundary and a candidate developer-facing product, not an agent's self-grading mechanism. Commercial-game compatibility is not the objective.

**Non-negotiable operating rules**
- Start each agent session with `scripts/next.sh`. Work locally in `~/Projects`; check free space, current branch, dirty worktree and existing PRs before modifying files. One cohesive milestone branch/PR at a time; push checkpoints, run `scripts/gate.sh` locally, ship only after honest evidence and required hosted CI. No agent swarms or duplicate branches for in-flight work.
- The source of truth is this `PLAN.md`; GitHub Issues/Projects visualize work but do not supersede this plan. No separate roadmap document. Existing work should be reviewed and integrated, not silently rebuilt.
- No retail games, keys, firmware, leaked/proprietary code, dubious test assets or unsupported claims. Treat unresolved historical provenance and fixture authorship as explicit blockers for broad distribution or publication of affected materials. Legal/history remediation requires specific owner approval and specialist review; do not rewrite refs or delete evidence unilaterally.
- Keep `tests/corpus/oracles.json`, its denominator, source/provenance pins, input limits and required engines unchanged across repairs unless the owner explicitly authorizes a separately documented correction. Preserve all old passes. No observed regression oracle is to be described as an independently pre-registered expectation.
- Independent means someone outside the author/agent workflow actually executed the published recipe and posted machine, input and output evidence. CI on our own GitHub runners is valuable, but does not count as an external rerun. GitHub attestations prove a recorded build provenance, **not** bit-for-bit reproducibility or external correctness.
- Each numbered step starts unchecked and can only be ticked with a link to its acceptance evidence. If the step does not change the corpus, explicitly document that fact; do not fabricate an oracle increase. Dates below are planning horizons, not promises.

### Evidence scoreboard and scope boundaries

- **Latest certified main-branch baseline at planning:** 46/50 original-Switch outside homebrew *first-frame* oracle passes on Interpreter and Tier-3; four recorded failures: `account`, `audio-echo`, `playtone`, `save-data`. Source: existing Step 14 canonical report. This says nothing about sustained compatibility. The separate outside Switch 2 Hello CPU result and the supported deko3d host-GPU result remain narrow, separate measurements.
- **Already in flight; do not duplicate:** [PR #188](https://github.com/hffgzktbcc-blip/ounce-core/pull/188) proposes fixes for those four failures and reports 50/50 *locally*; this must not be called 50/50 certified on `main` until merged canonical CI proves it. [PR #194](https://github.com/hffgzktbcc-blip/ounce-core/pull/194) explores deterministic menu-to-triangle input and multi-frame evidence; review its unchanged-workload and host-adapter evidence before opening any parallel implementation. [Independent tester issue #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190) already invites outside reruns. Review related proposed documentation before creating another guide.
- **Success metrics**, reported separately: (a) unchanged first-frame corpus passes / 50 with required engines; (b) verified third-party reruns, not team/agent repetitions; (c) independent *byte-identical rebuilds* versus mere hash-recorded runs; (d) accepted external contributions and repeatable onboarding; (e) behavioral multi-frame/input/audio/storage scenarios, counted separately and never added to the first-frame denominator; (f) provenance and release trust exceptions outstanding.

### Work order: first prove transferability, then widen capability

- [ ] **15. Publish one independently reproducible, legally reviewed research experiment (P0; first milestone).** Select one simple outside original-Switch homebrew sample with unambiguous licence/provenance (RomFS is a candidate), using the existing pinned source and `docs/REPRODUCE_ROMFS.md` as a starting point. Produce a versioned minimal `REPRODUCE.md` or an equivalent tightly linked guide, a machine-readable manifest, exact core commit/release, registered oracle identity/class, source SHA, actual NRO SHA-256, toolchain versions and **container image digest** (avoid `latest` in the final recipe), host prerequisites, reproducible command, canonical Ounce-Verify report, raw evidence, sanitized logs, expected failure modes and verification commands. A small 10–15 minute smoke workflow is the first usability target, not a guaranteed runtime. Publish a *read-only* artifact; prohibit hidden owner-only resources and private/retail inputs. Include an explicit known limitations and provenance disclaimer; omit any unreviewed bundled fixture from the distributed artifact.
      *Acceptance gate:* a clean, unaffiliated third-party machine independently follows the published instructions, records source/input/build identifiers, runs the unchanged Interpreter and Tier-3 checks and validates the canonical evidence; the outside tester links a report from [issue #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190) and discloses independence. If the resulting binary differs, retain that as a reproducibility finding; do **not** call it a byte-identical rebuild. At least one independent successful rerun is required to tick this milestone. CI-only success may be checkpointed, not ticked.
      *Implementation guard:* First inspect whether PR #189 / existing reproduction guides already provide parts of this. No wholesale CI refactor, GPU expansion or new AI dependencies. If legal review blocks a given program, choose a permitted alternate with owner-visible rationale. **Next action:** inventory currently available source pins, Docker image digests and first-run instructions, then prepare the smallest one-program artifact.
      Internal checkpoint (2026-11-07): pinned one-program recipe and reference manifest in docs/REPRODUCE_ROMFS.md and docs/reproduce/romfs.json. Both engines matched all five unchanged RomFS frames using retained CI NRO bytes; canonical records and raw observations retained under docs/reproduce/. Local gate: 475 tests passed; runtime suite passed. Docker rebuild, clean Linux onboarding, legal review and unaffiliated rerun remain unverified; milestone stays unchecked. No corpus expectation or denominator change.
      Internal container checkpoint (2026-11-07): all published recipe blocks completed from a fresh pinned clone using Docker 29.9.3, Lima ARM64 and Rosetta amd64 execution. Rebuilt NRO matched the reference SHA-256; both engines passed five unchanged frames and canonical byte checks (docs/reproduce/romfs-container-report.json). Native clean Linux amd64 onboarding, linked-component legal review and unaffiliated rerun remain unverified; milestone stays unchecked. Corpus unchanged.
      Session checkpoint (2026-11-07): [native Linux amd64 recipe CI](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42376017690) completed every published Bash block on a fresh Ubuntu runner; rebuilt NRO matched the reference SHA-256 and both engines passed all five registered frames plus canonical byte validation. Retained docs/reproduce/romfs-linux-amd64-* evidence and ROMFS_COMPONENT_REVIEW.md with actual linked-input hashes/member selections; legal clearance is not claimed. Corpus and expectations unchanged. Next: specialist review of newlib/libsysbase, GCC runtime/startup and embedded assets; an unaffiliated tester must submit accepted evidence through #190. Internal work cannot satisfy that acceptance gate; leave this milestone unchecked.
- [ ] **16. Close provenance and release-trust gaps without destroying evidence (P0).** Inventory historical flagged commits, active refs, fixtures and released binaries with existing read-only `scripts/audit_history_reachability.py`; record what is verified, disputed and unknown. Get specialist advice/owner decisions before any history rewrite, takedown or republication. Audit GitHub required status checks, oracle manifest change controls, dependency/security gates and who can approve weakened expectations. Add release asset checksums, build-attestation verification and toolchain/environment metadata where absent; identify whether each artifact is (i) integrity-checked, (ii) rebuildable from a documented recipe or (iii) *independently byte-identically reproduced*. Redact local usernames, tokens, absolute host paths and private environment from public logs.
      *Acceptance gate:* a linked reviewed inventory dispositions every known flagged item or marks it explicitly blocked, unaffected CI and signed/release metadata checks pass, and no provenance or security uncertainty is mislabeled resolved. Any destructive remediation is a separate owner-approved action. Do not claim comprehensive historical cleanup from a local-ref-only script.
- [x] **17. Complete or accurately classify the 50-program original-Switch corpus (P0).** Review the existing four-failure work in PR #188 rather than reimplementing it. For each of `account`, `audio-echo`, `playtone`, `save-data`, retain a before/after failure dossier (NRO bytes and pinned provenance, stop or divergence, minimal subsystem fix, unchanged oracle and limits, Interpreter/Tier-3 results, all prior passes). Preserve any remaining honest failures if fixing them is blocked.
      *Acceptance gate:* the latest merged canonical CI independently verifies all 100 required records and the unchanged 50-case denominator; aim for 50/50, but never tick or claim 50/50 without it. The Native and GPU paths must be reported separately. If PR #188 has already landed with these conditions, cite its merged proof and close this milestone without duplicating code.
      *Proof:* [PR #204](https://github.com/hffgzktbcc-blip/ounce-core/pull/204): no oracle change: no corpus/oracle change on this branch: closure cites merged PR #188 plus post-merge canonical main run 42380281215. Merged [PR #188](https://github.com/hffgzktbcc-blip/ounce-core/pull/188) (no oracle/manifest changes) repaired account, audio-echo, playtone, save-data. Post-merge canonical main run [42380281215](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42380281215): all 100 required records pass with verified artifact bytes, unchanged 50-case denominator, README headline check enforced in CI (50/50). Native path reported separately: native-corpus-oracles job pass; GPU path separately: Guest SASS host execution jobs pass on macOS and Linux. Before/after dossier retained at docs/verify/corpus-completion-local.json.
- [ ] **18. Make evidence usable by outsiders: static viewer and workbench UX (P1).** Generate a public, static, read-only explorer from immutable, canonical, hash-identified evidence: corpus → program → upstream commit/licence → NRO SHA → oracle type and expectation → per-engine observations/verdict → raw logs/CI link. Never compute or approve a verdict in the browser; distinguish retained reports from live checks. Improve the native Mac launcher/results workbench through actual keyboard, accessibility, failed-load, timeout, stop/restart and crash-recovery testing. Refactor oversized `src/main.rs` only when necessary to expose tested internal boundaries, not for aesthetics.
      *Acceptance gate:* one outsider can open a published report, trace its claim to evidence, understand a preserved failure and reproduce the sample using docs; manual macOS usability/accessibility acceptance is recorded separately from build smoke CI.
- [ ] **19. Verify behavior beyond the first frame (P1; new metric, never relabel first-frame passes).** Reuse the corpus runner's frame captures and scripted input. Inspect and integrate [PR #194](https://github.com/hffgzktbcc-blip/ounce-core/pull/194) before authorizing overlap. Pre-register a permitted outside program's multi-frame and/or scripted-input expectations **before** its candidate execution; compare Interpreter and Tier-3, capture each frame/time/input transition and guest stop cause. Later extend to deterministic audio buffers or save/reload semantics only on separately registered scenarios with independently derived expectations.
      *Acceptance gate:* at least one outside unmodified homebrew scenario matches an independently established, fixed multi-frame/input oracle on both engines in hosted CI, and another person can rerun it. A cross-engine-only agreement or observed output hash remains diagnostic/regression evidence, not independent behavioral proof. Keep first-frame and behavior-score denominators distinct.
- [ ] **20. Publish a citable methodology and validate adoption (P2).** After Steps 15–16 are independently accepted, write a technically candid paper/artifact on pre-registered frame oracles and multi-engine agreement, including known weaknesses, the observed `deko-basic` regression exception, negative cases, provenance, and precise threats to validity. Add `CITATION.cff` and consider a DOI-backed archive only after reviewed redistribution rights. Record one external oracle/verification contribution or authentic independent research reuse; seek reviewer feedback, not just stars/downloads. Explore a standalone verifier interface only once a user outside the project requests or demonstrates use of the existing tool.
      *Acceptance gate:* stable citable artifact and linked reproducibility guide, independently checkable evidence and honest limitations, plus at least one substantive external research or developer use. Never call an unpublished paper peer-reviewed or portray CI as outside validation.

### Practical horizon and sequencing (nonbinding)

- **Next 7 days:** audit the existing #188/#194/#189 changes and #190 challenge; freeze a one-program reproducibility recipe with pinned image digest; prepare candidate report and safe publication boundaries. Ask a genuine outside tester to try the published instructions. Separately triage historical provenance blockers.
- **Next 30 days:** obtain the outside rerun for Step 15, close the provenance trust review items that are actually within our control, and review any merged 50/50 proof. Publish an understandable failure dossier; do not block useful independent verification merely to chase a prettier pass count.
- **Next 90 days:** expose a public read-only evidence explorer and validated Mac workflow, add one independent behavior scenario, and publish a credible citable research artifact if the provenance and reproduction gates have passed. Delay publication rather than dilute the standard.
- **Budget discipline:** prefer existing Rust/Python/Swift, GitHub Actions/Pages, local-first M3/16 GB and free tooling. Prioritize repeatable tests over speculative features and dashboards. No new hosting subscription, external model, speculative universal Switch 2 port or bulk branch creation without user-value evidence.

### Owner decisions and stop rules

The owner authorizes **planning these steps** in PLAN.md. Implementation proceeds milestone by milestone under the existing gate/PR rules. Stop for owner decision before changing any golden oracle, modifying or dropping the corpus, claiming new compatibility, publishing questionable materials, destructive Git history changes or installing costly/dependent services. **Do not tick a step based on a proposed PR, a green synthetic suite, an internal agent rerun or a release badge alone.**

---

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


## How we work (owner decision 2026-11-04; control-system repair 2026-11-07)
- **This file is the truth.** Notes, handoffs and memory are history, not plans.
- **Real frames decide progress:** a step closes on its pixel-oracle evidence in CI, never on unit tests.
- **Step IDs are unique.** Historical duplicate IDs were suffixed (`15i`, `15f`); `scripts/gate.sh` refuses to checkpoint an ID that matches more than one block. Never reuse a step number.
- **Any agent continues any step:** `scripts/next.sh` shows where it stands (fast and local by default; `--full` adds network/oracle status); `scripts/gate.sh --checkpoint` saves the step's branch state (lightweight; add `--validate` to run the full local gates); `scripts/gate.sh --ship` runs full validation, ticks with proof and opens the PR. **Agents never merge and never arm auto-merge — the owner merges after reviewing evidence and CI.**
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

## Owner-directed completion follow-up (2026-11-07)
The owner requested completing the remaining plan work after resolving the PR list.
- [x] **15f. Resolve the remaining registered corpus failures.** Diagnose account,
      save-data, audio-echo and playtone using their pinned outside binaries. Repair
      runtime service behavior and preserve every pre-registered frame expectation.
      *Done when:* all four pass their existing Interpreter/Tier-3 oracles in CI,
      all previous corpus passes remain, and repository guardrails and tests pass.
      Checkpoint (2026-11-07 05:48Z, nathanaelgovender, 1c2a1aa): GREEN — tests: cargo test: 474 passed, 0 failed; suite: pass — All 50 unchanged outside oracles pass locally with verified artifact bytes; retain four same-binary repairs
      *Proof:* [PR #188](https://github.com/hffgzktbcc-blip/ounce-core/pull/188) merged the four same-binary repairs with no oracle/manifest change; post-merge canonical main run [42380281215](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42380281215) passes all 100 required records with verified artifact bytes (50/50, unchanged denominator), native-corpus-oracles and Guest SASS host execution jobs green. Also recorded as new-phase step 17 proof.
