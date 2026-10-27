# M4 design: Tier-4 native recompiler

Status: **approved 2026-10-26** (recommended options: Cranelift, memory plan A then B, `native-jit` + `--cpu=`, workspace). Steps 1 and 2 done (section 12).
Date: 2026-10-26. Base: `main` @ 4ecacae (v0.3.1, 25 stages `0xEEE`, `#![forbid(unsafe_code)]` in `src/main.rs`).

## 1. Goal and scope

Add a fourth CPU tier that translates hot guest AArch64 blocks into host machine code, for higher guest MIPS on homebrew and synthetic research workloads.

Hard constraints (from AGENTS.md "Unsafe Boundary"):
- All `unsafe` lives in a separate crate, `crates/ounce-recompiler`. `ounce-core` keeps `#![forbid(unsafe_code)]` and the `grep -rn "unsafe" src` gate keeps returning 0.
- The safe Tier-3 engine stays the default and the fallback. Tier-4 is opt-in, behind a Cargo feature, and every failure path ends in Tier-3, never in a crash.
- Default build, unit tests and the 25-stage harness are unchanged when the feature is off.

Out of scope:
- Guest PAC/BTI enforcement. The decoder treats `PACIBSP`, `BTI` and friends as hint NOPs today; Tier-4 must match that, not change it.
- Translating NVN/GPU work. Separate milestone.
- Multi-core parallel guest execution. The scheduler runs threads one at a time; Tier-4 keeps that model.

## 2. Current state (read from code)

- `src/cpu/interpreter.rs`: `Cpu::step` decodes through `JitBlockCache` (a decode cache, not native code) and executes one `Instruction` (68 variants in `instructions.rs`; anything else is `Instruction::Unimplemented { raw }`).
- `src/cpu/tier2_jit.rs`: `Tier2Block::compile` decodes a basic block (up to 128 instructions, ends at a branch, `SVC` or unimplemented op). `execute` runs the decoded `Instruction`s in a tight loop. Exits are `BlockExit::{Branch, BranchCondTaken, BranchFallthrough, Svc, Return, BudgetExceeded, Fault}`.
- `src/cpu/tier3_jit.rs`: `Tier3JitEngine` counts block entries (`HOT_THRESHOLD = 4`), fuses up to 4 blocks along static targets into a `SuperBlock`, and runs them back to back. Despite the doc comment ("Native Host JIT"), **it emits no host code**: it is fused interpretation. Tier-4 would be the first real native tier.
- `src/mmu/memory.rs`: `Memory` is a `HashMap<u64, Page>` keyed by `va >> 12`, 4 KiB pages with a `Permission`. Every guest load/store is a hash lookup. `Memory` is not `Clone`.
- `Pstate` holds N/Z/C/V as four `bool`s. Registers: `x[31]`, `sp`, `pc`, `v[32]` (u128), `tpidr_el0`, `tpidrro_el0`.
- No code-cache invalidation on guest writes to executable pages today (`JitBlockCache::invalidate_all` exists but nothing calls it on writes). Fine for current workloads, not fine once compiled code is long-lived.

Honest expectation: the hash-map memory model, not instruction dispatch, is likely the largest cost once code is native. Section 5 treats memory access as the central design decision.

## 3. Crate layout and the `unsafe` boundary

```
Cargo.toml                 # becomes a workspace; ounce-core stays the root package
crates/ounce-recompiler/   # all unsafe; no dependency on ounce-core
  src/lib.rs               # safe public API
  src/ir.rs                # small, recompiler-owned block IR (safe)
  src/backend/*.rs         # IR -> host code (Cranelift, see section 4)
  src/exec.rs              # the only module with unsafe: W^X buffers, icache flush, entry trampoline
```

- `ounce-core` lowers its decoded `Instruction`s into `ounce_recompiler::ir` (safe code, lives in `src/cpu/tier4.rs`). This keeps the decoder in one place and avoids a dependency cycle. The recompiler never sees `ounce-core` types.
- Feature `native-jit` on `ounce-core` (off by default) pulls in the crate as an optional dependency, the same pattern as `gpu-present`.
- Safe API sketch:
  ```rust
  pub struct GuestState { pub x: [u64; 31], pub sp: u64, pub pc: u64, pub nzcv: u32, pub v: [u128; 32], pub tpidr_el0: u64, pub tpidrro_el0: u64 }
  pub trait MemoryBus { fn read(&mut self, va: u64, size: u8) -> Result<u64, Fault>; fn write(&mut self, va: u64, size: u8, val: u64) -> Result<(), Fault>; /* 128-bit variants */ }
  pub struct Recompiler { /* code arena, block map */ }
  impl Recompiler {
      pub fn compile(&mut self, block: &ir::Block) -> Result<BlockId, CompileError>;
      pub fn run(&mut self, id: BlockId, state: &mut GuestState, mem: &mut dyn MemoryBus, budget: u32) -> Exit;
      pub fn invalidate_range(&mut self, va: u64, len: u64);
  }
  ```
  `Exit` mirrors `BlockExit`, plus `Unsupported { pc }` so `ounce-core` can hand the rest of the block to Tier-3.
- Inside the crate: `#![deny(unsafe_op_in_unsafe_fn)]`, every `unsafe` block carries a `// SAFETY:` comment, and `unsafe` is confined to `exec.rs`. A CI grep enforces that.

## 4. Backend: Cranelift vs a direct emitter

| | Cranelift (`cranelift-codegen` + `cranelift-frontend`) | Direct emitter (e.g. `dynasmrt`) |
| --- | --- | --- |
| Host targets | x86_64 and aarch64 from one IR. Covers Linux/Windows CI (x86_64) and Apple Silicon. | One hand-written backend per host. AArch64->AArch64 is close to 1:1; x86_64 is a full second backend. |
| Register allocation | Built in (regalloc2). | By hand. Simple pinning of hot guest regs is possible but is real work. |
| Compile latency | Higher: typically tens to hundreds of µs per block. Needs a hotness threshold well above Tier-3's 4. | Very low. |
| Code quality | Good general optimiser; no AArch64 flag-model shortcuts. | Can map guest NZCV straight onto host NZCV on aarch64, which is the main win of same-ISA translation. |
| `unsafe` surface | Small: Cranelift itself is safe Rust; we only need the executable buffer and the call. | Larger: the emitter writes raw bytes; correctness of every encoding is on us. |
| Dependencies | Several crates, pure Rust, actively maintained by the Bytecode Alliance. | One small crate. |
| Fit with fuzzing | Bugs are mostly in our lowering, which the differential fuzzer covers. | Bugs can also be in encodings, also covered by the fuzzer but more of them. |

**Recommendation: Cranelift.** One backend runs on every host the project tests (Linux/Windows x86_64 CI, macOS Apple Silicon), the `unsafe` surface stays small, and register allocation comes for free. A direct AArch64 emitter remains a possible later fast path for Apple Silicon only, once the fuzzer exists and numbers justify it.

I'd use `cranelift-codegen` + `cranelift-frontend` with our own `exec.rs` buffer management rather than `cranelift-jit`, so the W^X handling in section 6 is ours, explicit, and testable. Pin exact versions; Cranelift's API changes between minor releases.

## 5. Memory access (the main design decision)

Options:
- **A. Callback per access.** Generated code calls `MemoryBus::read/write` through a trampoline. Simple and precise (faults return to Rust). Still pays the `HashMap` lookup per access.
- **B. Software TLB.** A direct-mapped table (e.g. 4096 entries) of `guest page -> host pointer + perms`, inlined into generated code; a miss calls the bus. Needs page storage with stable host addresses and a TLB flush on `map_range`, unmap and permission changes.
- **C. Fastmem.** Reserve host address space and mirror guest pages into it; faults via signal handlers. Fastest, but brings signal handling, per-OS virtual memory tricks and a lot more `unsafe`.

**Plan: A first, B second, C not planned.** A gives a correct, fuzzable baseline. B is where the speed is and fits the existing page model with a contained change in `Memory` (stable page buffers plus a mapping generation counter), still safe on the `ounce-core` side. C is out of proportion for a research runtime.

Precise faults: guest registers live in `GuestState` in memory. Before every call that can fault (memory, SVC, unsupported), generated code writes back dirty guest registers and the current `pc`. A fault then leaves state exactly as Tier-3 would at the faulting instruction.

Self-modifying code: the bus marks pages that hold compiled code; a write to one calls `invalidate_range` and ends the block. Same rule added for Tier-2/3 caches so all tiers agree.

## 6. W^X and `MAP_JIT`

| Host | Allocate | Write | Make executable | Icache |
| --- | --- | --- | --- | --- |
| macOS Apple Silicon | `mmap(..., MAP_PRIVATE \| MAP_ANON \| MAP_JIT)` | `pthread_jit_write_protect_np(0)` on the writing thread | `pthread_jit_write_protect_np(1)` | `sys_icache_invalidate` |
| macOS x86_64 | `mmap` RW | write | `mprotect` RX | not needed |
| Linux aarch64 | `mmap` RW | write | `mprotect` RX | `__clear_cache` / `IC IVAU` + `DSB ISH` + `ISB` |
| Linux / Windows x86_64 | `mmap` / `VirtualAlloc` RW | write | `mprotect` / `VirtualProtect` RX | `FlushInstructionCache` on Windows (cheap, required by contract) |

Rules:
- Never RWX. A page is writable or executable, not both, for the current thread.
- On Apple Silicon the write toggle is per thread, so all compilation happens on the thread that runs guest code (true today: the runtime is single-threaded per guest core).
- Hardened runtime: a signed and notarized macOS build needs the `com.apple.security.cs.allow-jit` entitlement. Unsigned local and CI builds work without it. The release workflow must add it before Tier-4 ships in a macOS binary.
- Rust's `aarch64-apple-darwin` target is not `arm64e`, so generated host code needs no pointer signing.

## 7. Integration with the tiers

- `src/cpu/tier4.rs` (safe, feature-gated): owns a `Recompiler`, a per-PC hotness counter (initial threshold 64, tunable), and a "do not compile" set for blocks that hit `Unsupported` or `CompileError`.
- Dispatch: Tier-4 compiled block if present, else Tier-3, else interpreter. Any `Exit::Unsupported { pc }` continues in Tier-3 from that `pc`.
- Selection: `--cpu=tier3|tier4` (default `tier3`) and `OUNCE_CPU`, mirroring `--present=`. A build without the feature prints that Tier-4 is unavailable and runs Tier-3.
- Initial instruction coverage: integer ALU, moves, logical, shifts, compare/branch, loads/stores and pairs, `SVC` exit. FP/SIMD comes later; until then those ops exit to Tier-3.
- State conversion: `GuestState` <-> `CpuRegisters` at block entry/exit. `Pstate` bools pack to NZCV bits. This is a copy per block entry, removed later by block chaining.

## 8. Differential fuzzing against Tier-3

Oracle: Tier-3. Because Tier-3 itself is not proven, every case also runs on the interpreter (`Cpu::step`), so a mismatch says which side is wrong.

Per case:
1. Generate a block: 1 to 64 instructions drawn from encodings the decoder maps to supported `Instruction`s (filtering out `Unimplemented`), ending in a terminator. Random registers, flags, `V0`-`V31`, and 1 to 4 mapped pages of random data, some read-only, plus addresses aimed near page edges and unmapped pages so faults get exercised.
2. Run the same snapshot through the interpreter, Tier-3 and Tier-4 with the same instruction budget.
3. Compare: `X0`-`X30`, `SP`, `PC`, NZCV, `V0`-`V31`, `TPIDR_EL0`, the exit reason (including fault PC and SVC number), executed instruction count, and every byte written to memory. Writes are logged by a recording `MemoryBus` wrapper and compared as `(va, size, value)` sequences plus final page contents.
4. On mismatch: shrink to the shortest failing instruction sequence and print it as a ready-to-paste regression test.

Plumbing: `Memory` needs `#[derive(Clone)]` for snapshots (safe, additive). The generator uses a seeded PRNG so every failure reproduces from its seed.

Where it runs:
- `cargo test --release --features native-jit`: a fixed-seed run (target 10,000 cases, tuned to stay under about a minute) on Linux x86_64 and macOS aarch64 runners, so both Cranelift backends are covered on every PR.
- Optional nightly job: `cargo fuzz` (libFuzzer) over the same generator for open-ended runs; findings are committed as fixed-seed regression cases.
- Seed corpus: the guest programs from the 25 harness stages.

## 9. Implementation plan (separate small PRs, after approval)

1. Workspace + `crates/ounce-recompiler` skeleton + `exec.rs` (W^X buffer, icache flush, trampoline) + a test that runs a "return 42" function on every CI host. `native-jit` feature wired, no dispatch change.
2. IR, Cranelift lowering for integer ops with callback memory (option A), `tier4.rs` dispatch, `--cpu=` flag.
3. Differential fuzzer (section 8) in CI. Tier-4 cannot be selected by default until this is green on both host architectures.
4. Self-modifying code invalidation across all tiers.
5. Software TLB (option B).
6. Block chaining and FP/SIMD coverage, each gated by the fuzzer.

Each PR keeps the gates: default build unchanged, all unit tests, 25 stages `0xEEE`, 0 `unsafe` in `src/`.

## 10. Risks

- Compile latency on short-lived code: mitigated by the hotness threshold and Tier-3 fallback.
- Subtle flag semantics (carry on `SUBS`, 32-bit forms): exactly what the fuzzer targets.
- Apple Silicon W^X toggling bugs show up as crashes, not wrong results: covered by step 1's per-host execution test.
- Cranelift API churn: pinned versions, upgrades in their own PR.
- Expectations: no MIPS target is promised. Stage 20's measured MIPS is the benchmark, reported before and after each step.

## 11. Decisions needed

1. Approve Cranelift as the backend (vs a direct emitter).
2. Approve memory plan A then B, with fastmem (C) not planned.
3. Approve the `native-jit` feature name and `--cpu=tier3|tier4` flag.
4. Approve converting the root `Cargo.toml` into a workspace.

## 12. Implementation notes

Step 1 (workspace, crate skeleton, executable memory):
- `crates/ounce-recompiler/src/exec.rs` holds all `unsafe`; `lib.rs` has `#![deny(unsafe_code)]`, so the compiler rejects `unsafe` anywhere else in the crate.
- `ExecBuffer::seal` maps, copies once and seals. Buffers are never reopened for writing; step 2 adds an arena on the same rules.
- Apple Silicon: macOS requires `MAP_JIT` regions to be mapped with `PROT_READ | PROT_WRITE | PROT_EXEC`; W^X is enforced per thread by `pthread_jit_write_protect_np`, which is toggled only around the copy. This is the one place the mapping itself is RWX.
- Linux aarch64 uses inline `DC CVAU` / `IC IVAU` / `DSB ISH` / `ISB` (line sizes from `CTR_EL0`) rather than linking `__clear_cache`.
- Cranelift is not a dependency yet; it arrives with step 2. Step 1's self-test emits a hand-written `return 42` per host (aarch64, x86_64).
- Verified: x86_64 Linux locally (tests + `jit-selftest`); `cargo clippy` clean for `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`. Execution on macOS (Apple Silicon) and Windows is covered by the `native-jit` CI job.

Step 2 (IR, Cranelift backend, callback memory, Tier-4 dispatch):
- `crates/ounce-recompiler/src/ir.rs`: SSA block IR, 64-bit values with `W32` ops that zero-extend. Ops: constants, guest register get/set (`X0`-`X30`, `SP`, packed NZCV), add/sub/mul/logic/shifts/rotate/div, sign/zero extend, NZCV for add/sub/logic, condition codes, select, load/store with a fault site, and an instruction-boundary check. Terminator: one exit or a two-way exit. Division never traps (`/0 = 0`, `MIN / -1 = MIN`), shift amounts are taken modulo the width.
- `backend.rs` (safe): Cranelift `=0.137.0` (`codegen`, `frontend`, `native`), `opt_level=speed`, no `cranelift-jit`. Generated function: `extern "C" fn(*mut GuestState, *mut CallCtx) -> u64`; exit word `code | insns << 32`, exit `pc` in `GuestState.pc`. Guest registers are loaded/stored at every get/set, so state in memory is current at each memory call and faults are precise. Code needing relocations is rejected.
- `exec.rs` gained the `extern "C"` memory trampolines and the block call. A panic in a `MemoryBus` is caught, the block exits as a fault, and the panic resumes after the call (never unwinds through generated frames).
- Self-modifying code: `MemoryBus::write` reports `CodeWritten` when a store bumped `Memory::code_write_gen`; the block stops at the end of that instruction, then `Tier4JitEngine::sync_code_writes` drops translations via `code_written_since` (same rule as Tier-2/3, section 5). `invalidate_range` from the API sketch is `Recompiler::remove(BlockId)`; the guest-range bookkeeping lives in `ounce-core`.
- `src/cpu/tier4.rs` (feature `native-jit`): lowering mirrors `Cpu::step` (the interpreter is the reference, including its `XZR`/`SP` choices). Translated: `ADD/SUB` (imm, shifted, extended, flags), `MOVZ/MOVN/MOVK`, logical imm/reg, bitfield, `EXTR`, `CSEL` family, `CCMP/CCMN`, `MADD/MSUB`, `UDIV/SDIV`, variable shifts, `ADR/ADRP`, `NOP`/hints, `LDR/STR` imm/reg/pre/post (all sizes, signed), `LDP/STP`, `B/BL/B.cond/CBZ/CBNZ/TBZ/TBNZ/BR/BLR/RET`, `SVC`. Everything else (system registers, FP/SIMD, exclusives) ends the block and runs on Tier-3. Blocks are at most 64 instructions; hotness threshold 64 entries. Consecutive translated blocks run back to back without converting state. PC maps use a multiply hash: SipHash per block lookup cost more than the block itself.
- Selection: `cpu::CpuTier` / `cpu::CpuEngine` (`--cpu=tier3|tier4`, `OUNCE_CPU`, default Tier-3). Today only Stage 20 runs on a block engine; `run_vm` still steps the interpreter.
- Tests: crate tests run IR through native code (flags, division, precise faults, code-write stop, panic carry). `tier4.rs`: interpreter vs Tier-3 vs Tier-4 on fixed programs (loops, calls, pairs, writeback, ALU mix, fallback), precise fault, SMC, and 2,000 seeded random straight-line blocks against the interpreter. Tier-3 is not in the random comparison while its Tier-2 fast path ignores shifted/extended operands (issue #25).
- Measured on x86_64 Linux (cloud container, noisy): Stage 20 loop scaled to 308k instructions, Tier-4 ~115-120 MIPS vs Tier-3 ~60-75 MIPS. Stage 20 itself (15k instructions) is slower on Tier-4 (~15 MIPS) because Cranelift compile time dominates such a short run.
