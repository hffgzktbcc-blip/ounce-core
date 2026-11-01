# Handoff history: multi-agent lanes (archived 2026-11-01)

Moved verbatim from `AGENTS.md` section 3. These describe the retired Track A / Track B split and file reservations. They are history, not current rules. Current working model is in `AGENTS.md`.

* **Active Agent Threads & File Ownership**:
  - **Claude (Track B: Services)**: Homebrew service plumbing (`sm:`, `hid`, `fsp-srv` RomFS, `nvdrv`). Reserved files: `src/kernel/ipc.rs`, `src/kernel/sm.rs`, `src/kernel/hid.rs`, `src/kernel/nvdrv.rs`, `src/fs/romfs.rs`, test fixtures.
  - **Antigravity (Track A: Tier-4 Recompiler Enhancements)**: Tier-4 direct block chaining, Software TLB (Option B), and differential fuzzer in CI. Branch `feat/tier4-chaining-tlb-fuzzer`. Reserved files: `crates/ounce-recompiler/*`, `src/cpu/tier4.rs`, `.github/workflows/*`.
  - **Shared Boundary**: Antigravity avoids Claude's reserved paths (`src/kernel/*`, `src/fs/romfs.rs`, test fixtures). Claude avoids `crates/ounce-recompiler/*`, `src/cpu/tier4.rs`, `.github/workflows/*`. Neither touches `src/main.rs` without coordination.
* **Claim: Track B, homebrew service plumbing (Claude)** (2026-10-27, split agreed with the user)
  - Reserved: `src/kernel/ipc.rs`, `src/kernel/sm.rs`, `src/kernel/hid.rs`, `src/kernel/nvdrv.rs`, `src/fs/romfs.rs`, service test fixtures (`tests/fixtures/nro/*`, `src/loader/fixture_tests.rs`). Track A (Antigravity) owns `crates/ounce-recompiler/*`, `src/cpu/tier4.rs`, `.github/workflows/*`; Claude does not touch those. `src/main.rs` changes are flagged to the user first.
  - Files outside that list changed by the Track B PRs opened before the split, for reviewers: #34 `src/kernel/hipc.rs` (new), `src/kernel/svc.rs` (QueryMemory regions, UnmapSharedMemory), `src/kernel/mod.rs`, `src/mmu/memory.rs` (additive `region_at`, `unmap_range`), `src/main.rs` (homebrew main thread handle constant only). #35 adds `src/cpu/{decoder,instructions,interpreter,simd}.rs` (new AdvSIMD ops; not Tier-4), `src/main.rs` (stage 17 uses fsp-srv cmd 200; `setup_homebrew_nro` puts the NRO on the virtual SD card).
* **Autonomous Parallel Execution Agreement (Owner Approved, 2026-10-27)**:
  - PR #34, #35, #36 (Track B) and PR #37 (Track A) are all merged to `main` at `c6d37ef`.
  - Both agents operate autonomously in their dedicated lanes without requiring conversational round-trips for permission.
  - **Merge Protocol**: Each agent opens draft/ready PRs for their lane. Once all 9 cross-platform CI matrix jobs are 100% green and no files collide outside reserved lists, the author agent is authorized to squash-merge to `main` without admin bypass.
  - **Track B Lane (Claude)**:
    - Next Goal: Homebrew threads (`svcCreateThread`, thread-local storage, and libnx mutex arbitration via Horizon synchronization primitives) followed by audio mixing (`audren:u` 64-voice mixer).
    - Reserved files: `src/kernel/ipc.rs`, `src/kernel/sm.rs`, `src/kernel/hid.rs`, `src/kernel/nvdrv.rs`, `src/kernel/scheduler.rs`, `src/kernel/sync.rs`, `src/fs/romfs.rs`, `tests/fixtures/nro/*`, `src/loader/fixture_tests.rs`.
  - **Track A Lane (Antigravity)**:
    - Next Goal: Benchmarking and optimizing Tier-4 MIPS throughput on long workloads, Cranelift register optimization, and wiring `run --gui` interactive window presentation with live swapchain pacing.
    - Reserved files: `crates/ounce-recompiler/*`, `src/cpu/tier4.rs`, `src/gpu/present/*`, `.github/workflows/*`.
  - **Shared Files**: Avoid touching `src/main.rs` unless strictly necessary for minimal CLI flags, with changes kept completely disjoint.
