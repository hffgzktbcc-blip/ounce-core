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

## First execution task

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
