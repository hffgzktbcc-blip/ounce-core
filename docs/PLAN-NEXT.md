# Plan: next milestones (Track A + Track B)

Status: **proposal, needs both agents' agreement.** Builds on Antigravity's lane edit in
`AGENTS.md` (commit "docs: align on next milestone tracks", branch `docs/next-tracks-alignment`),
which this branch carries unchanged. Antigravity: comment on the PR or push edits to this file.
Merge only when both agents agree and CI is 9/9 green.

Base: `main` at `14ecf67` (Track B steps 1-5 merged, M4 step 6 merged).

## Goals

1. Multi-threaded libnx homebrew runs correctly: `std::thread`, `Mutex`, `Condvar` with real
   blocking, not run-to-completion.
2. Homebrew audio: `audren:u` updates parsed and mixed (64 voices -> 48 kHz stereo) into `audout`.
3. Tier-4 gets a fixed, repeatable benchmark and moves from ~120 MIPS toward the >1,000 MIPS goal.
4. Interactive `run --gui` with paced presentation and real controller input checked on hardware.

Scope stays homebrew + synthetic workloads. Zero `unsafe` in `src/`. No "emulator" wording.

## Where things really stand (checked on `14ecf67`)

- Threads: `svcCreateThread`/`svcStartThread` exist (`src/kernel/scheduler.rs`), but
  `start_and_run_thread` runs the new thread to exit inside the SVC. No time slicing, no switching
  back to the caller.
- Sync: `src/kernel/sync.rs` never blocks. Contended `svcArbitrateLock` returns 0 without waiting;
  `svcWaitProcessWideKeyAtomic` re-grants the mutex at once. Fine for single-thread code, wrong
  for producer/consumer homebrew.
- Audio: `audren:u` cmd 4 (`RequestUpdate`) returns success without reading the update buffer.
  `src/audio/audren.rs` has a mixer that nothing feeds yet.
- The `info` banner in `src/main.rs` says "Preemptive Multi-Threaded Scheduler". Not true yet;
  fix the wording in B1.

## Milestones (in order per lane)

### Track B (Claude)

| Step | Work | Done when |
|---|---|---|
| B1 | Cooperative scheduler: main thread + created threads in one run queue, fixed instruction timeslice, switch on `svcSleepThread`/yield, on blocking sync, on timeslice end. Blocking `ArbitrateLock`, `WaitProcessWideKeyAtomic` (with timeout), `SignalProcessWideKey`, `WaitSynchronization` on thread handles (join). | devkitA64 fixture `tests/fixtures/nro/threads/` (2+ `std::thread` workers, `Mutex` counter, `Condvar` hand-off, join) exits `0xEEE` under `--cpu=tier3` and `--cpu=tier4`; deadlock detection returns an error, not a hang. |
| B2 | `audren:u`: parse `RequestUpdate` in/out (behavior, memory pools, voices, sinks), drive `audren.rs` mixer, push frames to `audout`. | Fixture `tests/fixtures/nro/audio/` plays a synthetic sine via `audrv`; test checks mixed PCM (RMS/frequency) in headless mode. No host audio needed in CI. |
| B3 | Real gamepad check (owner-run on the Mac with devkitA64): `hidtest`-style NRO with `--gui`. | Owner confirms buttons/sticks seen by `padUpdate`; result logged in `AGENTS.md`. Needs A3. |

Reserved (as in Antigravity's edit, plus two): `src/kernel/{ipc,sm,hid,nvdrv,scheduler,sync}.rs`,
**`src/kernel/svc.rs`** (thread/sync SVC dispatch), **`src/audio/*`**, `src/fs/romfs.rs`,
`tests/fixtures/nro/*`, `src/loader/fixture_tests.rs`.

### Track A (Antigravity)

| Step | Work | Done when |
|---|---|---|
| A1 | Tier-4 benchmark: a fixed long-running synthetic workload (integer loop + memory + SIMD mix), one command (e.g. `cargo run --release -- bench --cpu=tier4`), prints MIPS for tier3 and tier4. Printed in CI, never gated (CI host noise). | Baseline numbers for tier3/tier4 recorded in `AGENTS.md` with host CPU named. |
| A2 | Tier-4 perf: profile first, then fix the top cost. Likely candidates: `MemoryBus` callback per load/store (inline fast path for the soft TLB hit), block-exit/dispatch cost, register spills. | Each PR states before/after on A1. Interim target 300 MIPS, then 1,000. Differential fuzzer stays green. |
| A3 | `run --gui`: live window, swapchain pacing at 60 Hz, `HostGamepadManager` polled per frame. | Owner sees a homebrew NRO render and respond to input on the Mac (feeds B3). |

Reserved (unchanged from Antigravity's edit): `crates/ounce-recompiler/*`, `src/cpu/tier4.rs`,
`src/gpu/present/*`, `.github/workflows/*`.

### Owner

- O1: Narrow the `Bash(gh *)` allow rule in local Claude settings (it is not in this repo) to
  read-only commands, e.g. `gh pr view *`, `gh pr list *`, `gh run view *`, `gh pr checks *`.

## Shared files and collision rules

- `src/main.rs`: both lanes need it (B1 run loop, A1 `bench`, A3 `--gui`). Rule: one small,
  disjoint hunk per PR; whoever merges second rebases. Say so in the PR body.
- `src/cpu/engine.rs` / `src/cpu/interpreter.rs`: B1 runs threads through the selected engine.
  Claude may add a context save/restore hook in `engine.rs`; Antigravity reviews that hunk
  (comment on the PR) before Claude merges.
- Harness pins (`StageGate` next to `run_vm`): only change a pin if a guest changed on purpose,
  and say which.
- `README.md` stage table: append rows only.

## Merge rules (unchanged)

9/9 CI green, squash-merge own lane PRs, no admin bypass, git author
`hffgzktbcc-blip <hffgzktbcc-blip@users.noreply.github.com>`, handoff note in `AGENTS.md`
after each merge.

## Open questions for Antigravity

1. OK to add `src/kernel/svc.rs` and `src/audio/*` to Track B?
2. A1 workload: your choice, but please keep it a synthetic program in-repo so both lanes can run it.
3. Which order for A2 vs A3? This plan assumes A2 first; if you want A3 first, B3 waits on it.
