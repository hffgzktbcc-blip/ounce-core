# M4 step 6b: native block-to-block linking (design, Track A)

Status: proposal, not implemented. Owner: Track A (`crates/ounce-recompiler/*`, `src/cpu/tier4.rs`).

## Problem

Every translated block returns to the Rust loop in `Tier4JitEngine::run_native`, which looks up the next
block (`fast_cache`) and calls it. Measured ceiling is roughly 130M blocks/s (about 390 MIPS on x86_64 for a
3-instruction loop body). Faster code inside a block cannot help until blocks stop returning between each other.

## Scheme

Generated code jumps directly to the next block when the target is already compiled.

1. **Link table in `CallCtx`.** A direct-mapped table of `(guest_pc, host_entry)` pairs, indexed by
   `(pc >> 2) & MASK`. The Rust side fills it when a block is compiled and clears the entry when it is removed.
2. **At a direct-branch exit** (target `pc` is a constant): generated code loads the entry, compares `guest_pc`,
   and on a hit makes a tail call to `host_entry` with the same `(state, ctx)`. On a miss it returns to Rust
   exactly as today, so nothing changes for cold targets.
3. **Tail calls, not calls.** Blocks use Cranelift's `tail` calling convention so a hot loop does not grow the
   stack. A small `extern "C"` trampoline per `Recompiler` is the only entry from Rust.
4. **Budget.** Generated code keeps a running instruction count in `CallCtx`. Before each link jump it checks the
   count against the budget the Rust caller passed in, and returns to Rust when it is used up. This preserves the
   `run_slice(max_instructions)` contract and keeps scheduler preemption (20,000-instruction slices) working.
5. **Invalidation.** `sync_code_writes` already drops translations after a code write. It must also clear their
   link-table entries (and flush the whole table on a bulk flush) before any further native run. A block that is
   mid-run when a store hits code still exits with `CodeWritten` as today; links never jump past that check.
6. **Indirect branches, `RET`, `SVC`, faults.** Unchanged: they return to Rust.

## Safety boundary

All `unsafe` stays in `exec.rs` (trampoline and table ownership). `backend.rs` and `tier4.rs` stay safe Rust.
The link table lives in memory owned by `Recompiler`, so an entry can never point at freed code: removal clears
the entry first.

## Risks

- Cranelift `tail` convention plus the existing `extern "C"` memory trampolines: needs a spike to confirm the
  verifier and both hosts (aarch64, x86_64) accept it. If it does not, fallback is an in-block loop for
  self-branches only, which covers tight loops but not nested ones.
- Stale-link bugs would be silent wrong-code bugs. Mitigation: extend the differential fuzzer to run with linking
  on, plus a test that patches code after links exist.

## Acceptance

- Differential fuzzer (10,000 cases) and all existing Tier-4 tests pass with linking on.
- `stage20_benchmark_scaling` reports median of 5, asserts `fallback_insns == 0` and interpreter equality.
- Steady-state MIPS reported before/after on the same host. No target is promised until measured.
