# Codex handoff — independent compatibility trials

Prepared 2026-10-29. Start from PR [#89](https://github.com/hffgzktbcc-blip/ounce-core/pull/89), branch `codex/independent-corpus-preparation`, and fetch its current head before editing. The baseline runtime inspected was `fb80e573fc9888da456c1b102e5dc4b829c1ece4`. Read root `AGENTS.md` and inspect current main/active branches; preserve peer work and file ownership.

## What is done

- Four unchanged external-source candidates pinned in `tests/corpus/manifest.json`: startup template, CPU/JIT, RomFS, simple graphics. All are from switchbrew/switch-examples `669786898205b7beb25ff1731e72982e6d0397d3`.
- Python source-preparation tool and seven offline tests, with opt-in upstream builds and source/binary SHA-256 inventories outside this repository.
- Actual pinned-source fetch succeeded and inventoried 210 tracked files. Local temporary inventory is disposable and not committed; rerun the tool to recreate it.
- Category investigations and a runner design are linked from `docs/independent-corpus.md`.
- No Rust runtime code or CI workflow changed. No external sample was built or executed. No compatibility milestone is complete.

## Verification and limitations

Run `python3 -m unittest discover -s scripts -p 'test_*.py' -v`; seven tests passed here, including a fake local build path (not a real devkitA64 build). `git diff --check` passed. Requires Python 3.9+ and Git. A real source-only preparation also passed.

Rust/Cargo and devkitPro were unavailable in this working environment. Do not turn those omissions into a passing Rust or guest result. Check the current PR Actions results; all nine platform/feature jobs must pass before any merge, with no admin bypass. This handoff does not assert the PR's eventual CI status.

## Continuing Codex execution and protocol correction (2026-10-29)

The preparation-only status above describes the original change. Follow-up work on PR #89 built all four unchanged pinned samples with devkitPro and executed the startup NRO on Linux through a bounded interpreter CLI probe. Run [40841651117](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/40841651117) records `guest_stop=svc_break`, `break_result=0x00002559` (libnx BadGfxInit), and no compatibility verdict. The host process returning zero is not a guest pass. The startup NRO SHA-256 is `48eb23c81eafeeb1ea368ff75e6dc7323eb33b15ae3cf46b7fc9bb31c4408156`.

Trace evidence reaches ApplicationDisplayService discovery 100–103, OpenDisplay 1010 and OpenLayer 2020 before cleanup. The runtime correction returns session handles for discovery and explicitly rejects unsupported native-window operations instead of claiming success with unwritten parcel data. The new HIPC regression covers discovery and unsupported commands. Opt-in IPC tracing is capped at 256 requests; break diagnostics report PC and a readable four-byte result payload. The probe exposes its bounded log in Actions and records the observed guest stop separately from host completion.

Code head `563cf5e8a7da3e6f3b9fe4730f1ea3173efb0a10` passed all nine jobs in [CI 40841651067](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/40841651067); the default Linux job reports 365 tests and 27/27 harness checks across 25 stages. Later commits need their own CI. The external workflow records image/package identities and output hashes, but uses a moving devkitPro image and does not guarantee reproducible package inputs. Artifacts retain logs/inventories for seven days and exclude external source and NRO binaries.

### Acknowledged swarm lanes

See PR #89 comments 6491694410, 6491728010 and 6491732532. The continuing Codex session owns the current PR #89 implementation, diagnostics, workflow and this handoff; the preparation/swarm session independently reviews without editing that branch. Under the owner's explicit swarm request, two subordinate follow-ups use separate branches and draft PRs from the validated code head:

- `codex/vi-binder-bootstrap`: new `src/kernel/vi.rs` and minimal IPC/module hooks for bounded parcel and Binder object-lifetime bootstrap. A producer queue and rendered output must not be claimed before implemented and verified.
- `codex/bounded-nro-runner`: new `src/loader/corpus_runner.rs` mounted from `fixture_tests.rs`, explicit interpreter/Tier-3 test adapter and structured bounded results; no kernel, CLI or workflow overlap.

Read latest comments before editing or integrating either branch. Neither follow-up is complete merely because it is claimed. Keep all PRs draft/unmerged until independently reviewed and all nine checks pass.

The next display implementation must produce a real validated native-window parcel and model its Binder object/queue, with bounded descriptor parsing and meaningful tests. The runner must preserve real SVC/scheduler semantics, distinguish missing input and cooperative deadline limits, and retain an outer host timeout for blocking SVCs. External unchanged-program assertions, raw-frame capture, Tier-3 evidence and outside reproduction remain outstanding.

## Original first execution task (now built/probed; compatibility still blocked)

1. On an environment with Rust and devkitPro, prepare and build the unchanged startup baseline using the manifest/tool. Record compiler, libnx/newlib/switch-tools package versions and notices. Keep external source and NRO output outside Ounce-Core.
2. Run the unchanged startup NRO under the existing interpreter and preserve the first actual failure: PC, instruction, SVC/IPC command, guest stop reason and relevant service trace. Do not patch the sample to bypass it.
3. Source inspection suggests libnx console/window initialization will encounter the missing VI/Binder producer queue. Confirm this from execution before implementing. `printf` renders to guest pixels; host debug strings are not the oracle.
4. Coordinate ownership in AGENTS.md before changing IPC/kernel files. Implement only the first missing behavior and add a focused regression from public interfaces. Keep the entire release suite and 25-stage harness green.

## Next implementation steps

- Implement the required VI/Binder/window submission bridge, resolving real submitted guest buffers. The fixed synthetic display address is insufficient. Reuse the existing surface conversion where appropriate; capture raw pixels before HUD, host scaling or alpha replacement.
- Add a bounded, explicitly engine-selectable NRO test adapter using the existing fixture setup seam. Current global `--cpu=tier3` selects Stage 20 only: it does not prove Tier-3 NRO execution. Preserve SVC dispatch, scheduler/preemption, deterministic input and complete stop reasons. Read `docs/independent-corpus-runner.md` before designing it.
- CPU/JIT sample likely reports unavailable because HomebrewConfig does not emit SVC capability hints or own-process handle. Verify before fixing; advertise only implemented fallback SVCs (0x73/0x77/0x78), never unsupported CodeMemory calls.
- Once startup works, exercise the exact same binaries under interpreter and Tier-3 on Linux: generated return 7, both RomFS files including Unicode path, and simplegfx raw first-frame pixels. Inject Plus only after the relevant observation; require expected guest completion rather than host exit zero or budget exhaustion.
- Preserve first-failure evidence and obtain an outside developer's clean-checkout reproduction before completing the roadmap milestone.

## Boundaries

These trials exercise original Switch homebrew foundations, not Switch 2 compatibility. Keep retail decryption disabled. Do not import firmware, keys, proprietary/game material, upstream implementation code or generated NRO binaries into the repository. Upstream README's Public Domain Mark declaration and linked-component review caveats are recorded in the category documents; preparation hashes are not a full binary provenance audit.
