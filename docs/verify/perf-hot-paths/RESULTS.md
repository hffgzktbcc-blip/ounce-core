# MMU scalar access measurements — 2026-11-11

On this shared Apple M3/macOS 26.6.2 machine, the pinned CoreMark harness used
less **child user+system CPU time** after the MMU change in every measured pair:

| Engine | Baseline M guest insns / CPU second (median) | Candidate | Paired CPU throughput gain, median [min, max] |
| --- | ---: | ---: | ---: |
| Interpreter | 11.074 | 12.036 | +9.02% [+3.46%, +13.59%] |
| Tier-3 | 16.510 | 17.981 | +8.00% [+4.02%, +17.49%] |

There were 15 measured A/B pairs per engine, after one discarded warmup pair,
using fresh processes, identical ELF bytes and deterministic syscalls. Every
measured run executed **7,046,297 guest instructions**, made 8 syscalls, exited
successfully, identified the requested engine and matched the unchanged host
reference byte for byte. All 30 measured CPU-time differences were positive.
These are CPU efficiency measurements on this host, not official CoreMark scores.

**Wall-time improvement is inconclusive.** The paired wall-throughput changes
ranged from -36.60% to +82.67% for the interpreter (median +16.55%), and -11.47%
to +45.58% for Tier-3 (median +7.08%). Other agents were building and the user
was using this machine. Descheduling and heterogeneous-core placement are not
controlled. Do not present those medians as a reliable end-user wall-time win.

The registered NRO cases were also run for 9 measured A/B pairs after one
warmup pair, with their original frame oracles checked on every execution:

| Case | Guest insns per engine | Combined-engine CPU throughput change, median [min, max] |
| --- | ---: | ---: |
| printing-hello-world | 31,863,353 | +4.62% [-18.41%, +16.14%] |
| cpu-jit | 16,513,824 | +4.60% [-1.32%, +15.44%] |

Both NRO cases have inconclusive performance results: their paired spread
crosses zero. Their exact per-engine wall rates, CPU times, frame hashes and
all paired differences are retained in `corpus-benchmark.json`. Combined child
CPU time includes both engines and test/oracle work; it cannot establish a
per-engine NRO improvement. CoreMark's two engines were measured separately.

## What changed and why

The interpreter sample (`profile-before.txt`, five seconds on registered
simplegfx) exposed instruction dispatch, predecoded instruction-cache lookup,
scalar MMU reads and writes, and the harness's per-instruction deadline clock.
Among collapsed leaf counts were `Cpu::step` 888, `fetch_or_compile` 382,
`read_bytes` 212 and `write_bytes` 53; timer calls also dominated. These are
sampling counts, not whole-run time percentages.

The Tier-3 sample (`profile-tier3-before.txt`, three seconds of the unchanged
simplegfx NRO through the normal CLI) likewise exposed fallback `Cpu::step`
547, hash construction 494, `Tier3JitEngine::run_slice` 360, SipHash writing 262,
`read_bytes` 204 and `write_bytes` 73. Waiting UI/audio/helper threads are
present in the raw profile and excluded from this description. The profiling
runs were stopped after capture; they are not correctness or completion runs.

The optimization marks byte reads/writes inline so the existing scalar accessors
can specialize their fixed-size buffers. Writes now borrow the page mutably once
instead of hashing the same page number for permission checking and again for
copying/invalidation. The arena field is borrowed separately under `nce`.
Permissions, page splitting, overflow behavior, code-write generations, and the
arena-vs-page storage choice are preserved. No unsafe Rust or hashing algorithm
change was introduced. No timers or validation work were removed to improve scores.

## Identity and reproduction

See [README.md](README.md) for executable commands. Rust was
`rustc 1.98.0 (88d9e12ae 2026-08-18)`, using the default release profile,
`CARGO_BUILD_JOBS=2`, no optional features, the same worktree and target directory.
The ELF compiler was `uv run --with ziglang python -m ziglang cc -target
aarch64-linux-musl` (Zig 0.17.1), with the existing verifier's `-O1 -static
-no-pie` flags and unchanged manifest defines (`ITERATIONS=25`).

- CLI baseline source: `d06eb0127cf93deecb89f895ea8c0598db1e5fc3`.
- CLI candidate source: `4f8b795` (the only Rust change from that baseline is
  `src/mmu/memory.rs`). Both CLIs have the new explicit interpreter selection.
- NRO benchmark baseline source: `c47fa1468e36316b61a5aff2d54029d98799f90d`.
  Its candidate has only the same MMU patch applied; these executables were
  preserved before the interpreter-selection infrastructure rebuild. That
  infrastructure is unused by the existing NRO oracle execution paths.
- Exact executable, ELF, reference, NRO and manifest hashes are in the retained
  JSON reports. The reports' `revision`/`dirty` fields identify the **driver
  checkout**, not a self-attested source revision inside a copied executable;
  use the mappings above when rebuilding.
- `coremark-verification.json` records source hashes, pinned upstream commit,
  compiler commands and both engines' successful reference comparisons.
  `coremark-reference.txt` retains the reference. No ELF/NRO or upstream sources
  were added to the repository.

The benchmark summaries retain every measured sample. Raw per-execution logs
and observations were also retained locally under `/tmp/ounce-perf-paired-cpu`,
`/tmp/ounce-perf-coremark-paired`, and `/tmp/ounce-perf-coremark-final` during this
run; those temporary directories are not required to read the committed results.
