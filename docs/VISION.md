# Vision

**Understand why programs fail. Prove when they work.**

Ounce-Core is an open-source ARM64 execution, debugging and verification platform, written in safe Rust. It runs selected software on several CPU engines, shows what happened, reproduces failures, and checks results against real ARM64 hardware, keeping enough evidence that anyone can reproduce the answer.

Verification is the product. Ounce-Core is not another emulator promising to run everything. It started as a Switch homebrew runtime; Switch and Switch 2 are no longer the goal, and that work stays as a regression corpus.

## Three pillars

1. **Ounce-Verify: prove correctness.** Run a program on the interpreter, the JIT and native execution, compare against a reference build on real hardware, detect discrepancies, and preserve hashes and reports. *Most mature pillar.*
2. **Ounce-Debug: understand failures.** Reproduce an execution problem, compare historical results, and find where behaviour first diverges. *Demonstrated for one bounded workflow; broader tooling planned.*
3. **Ounce-Learn: make computing understandable.** A practical place to study ARM64 execution, memory, CPU architecture and software verification, eventually one instruction at a time in the browser. *Browser teaching prototype exists; real WebAssembly execution is planned.*

## Who it is for

- **Library maintainers.** "Does my C library behave identically on ARM64 and x86?" Build it, run it, diff it, with no ARM hardware needed to reproduce a report. This is the sharpest first use.
- **Emulator, JIT and binary-translator authors.** Use the differential method and, later, a published conformance suite to find where a model and real hardware disagree.
- **Educators and students.** Study how ARM64 executes with real programs.
- **Researchers.** Reproducible execution evidence with clear limits.

## What the evidence says (8 November 2026)

In one working week, running unmodified open-source programs and diffing them against an Apple Silicon host found, and fixed:
- a prefetch instruction decoded as a load, silently overwriting registers;
- two fused floating-point instructions with the wrong sign, so `sin`, `cos` and `pow` returned wrong numbers;
- a long list of missing vector instructions (widening multiply and add, byte reverse, register shifts, table lookup, polynomial multiply, narrowing and saturating ops), plus CRC32, scalar add and duplicate forms, and a fixed 50-million-instruction cap.

Programs merged and checked on three engines (interpreter, JIT, native host execution): LZ4, xxHash, SQLite, Lua, and a 5,964-line floating-point instruction test. Additional harnesses, including zlib, cJSON, TinyExpr, Monocypher and wasm3, are now in the [differential manifest](verify/differential.json); consult each harness for its measured inputs and limits. A 300-input loader fuzz run produced no panics. Method and results: [docs/verify/](verify/README.md).

## Maturity labels

Every claim carries one of four labels:

| Label | Meaning |
|---|---|
| **research-only** | explored, not dependable |
| **bounded-verified** | matches a reference on the listed inputs, with retained evidence |
| **independently reproduced** | someone outside the project reran it and got the same result |
| **publicly supported** | documented, versioned, and maintained for users |

| Capability | Maturity today |
|---|---|
| Execute selected ARM64 programs on three engines | bounded-verified on listed harness inputs; see [manifest](verify/differential.json) |
| Compare engines and compare against a host reference | bounded-verified |
| Record results, hashes and evidence | bounded-verified |
| Diagnose differences between retained reports | bounded-verified (one workflow) |
| Bounded replay of a historical failure | bounded-verified (SDL2 case); [instruction dossier replay](INSTRUCTION_DIAGNOSIS.md) now also exists |
| First differing compared instruction case with register snapshots | bounded-verified on one real vector-FP discrepancy; not arbitrary-program tracing |
| Instruction-level checking against hardware | bounded-verified on recorded single-instruction inputs; [fuzzer scope and results](verify/insn-fuzzer/README.md) |
| Run static Linux ARM64 programs | research-only runner; six freestanding fixtures and one bounded LZ4/musl case, [run-elf evidence](verify/linux-elf/README.md) |
| Run arbitrary ARM64 or Linux applications | not established |
| Automatically find any bug's root cause | not established |
| Independent reproduction by an outside developer | none yet ([issue #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190)) |

## Known limits and what we do about them

- **Coverage is partial.** Many vector, floating-point and system instructions are undecoded or untested. A [decoder census](verify/CENSUS_2026-11-08.md) and [single-instruction fuzzer](verify/insn-fuzzer/README.md) now exist. The recorded census is a revision-specific snapshot, not current coverage. Next: retain CI fuzz results and close observed gaps.
- **QC is modelled for implemented saturating instructions.** [Hardware comparisons](verify/qc-flag/README.md) cover bounded inputs; FPCR behavior and cumulative FP exception flags remain unmodelled. [Cross-host differences](verify/README.md#hardware-differences) remain explicit.
- **It is not fast.** The interpreter runs at roughly 20 million instructions per second, which is fine for tests. *Plan:* measure first. The bar is that the whole verified-program suite runs in CI in a few minutes. Speed is not what we compete on; beyond that bar it does not drive the roadmap.
- **Native execution runs on the host CPU.** On an ARM64 host it checks the harness and the toolchain, not the emulator, so Tier-3 and Tier-4 are the real evidence.

## Roadmap (each step is one small, checkable result)

| # | Result | How we will know |
|---|---|---|
| 0 | Prove one outside developer can use it | a library maintainer outside the project runs a harness and reports a result ([issue #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190)) |
| 1 | Run static Linux ARM64 programs (ELF loader plus about 40 Linux system calls, deterministic mode) | the verified programs run as plain `aarch64-linux` binaries on Linux, macOS and Windows with no console SDK |
| 2 | Instruction-level differential fuzzer in CI on a real ARM64 runner | thousands of generated sequences per instruction family match hardware; every decoder change is fuzzed |
| 3 | First-divergence report between engines and against a reference | a failing run points at one instruction with registers before and after |
| 4 | Record and replay | a recorded failure replays identically on another machine |
| 5 | Browser playground (WebAssembly build) | a visitor steps through a program with no install |
| 6 | Publish the conformance suite for other emulators | at least one outside project runs it and reports results |

Current progress and remaining acceptance:

- **Step 1 started:** `run-elf` executes six tiny freestanding fixtures and the unchanged [LZ4 harness linked with musl](verify/linux-elf/README.md#lz4-without-a-console-sdk). A bounded virtual clock is available with `--deterministic`; the LZ4 case and CLI scenarios [passed on all three host OSes](verify/linux-elf/three-host-2026-11-08.json). The broader verified-program suite as Linux ELF and wider syscall coverage remain incomplete.
- **Step 2 started:** single-instruction host fuzzing and interpreter/Tier-3/Tier-4 comparisons exist. The [Apple Silicon CI job](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42521905867/job/128857024677) passed: 60,000 engine-comparison words and a 20,000-word host trace replayed through each tier, with zero unexpected differences. Generated multi-instruction sequences and per-family acceptance remain future work.
- **Steps 3–4 partial:** the released [diagnosis CLI](DIAGNOSIS_CLI.md) inspects retained reports and one historical replay. [Instruction diagnosis](INSTRUCTION_DIAGNOSIS.md) now retains one hardware-observed discrepancy with register snapshots and checked replay. The same retained instruction failure has also reproduced on Windows with content checks. Arbitrary-program first divergence and general execution recording remain incomplete.
- **Step 5 prototype:** `site/lab.html` is a scripted teaching/workbench prototype, not a WebAssembly build of the Rust runtime.
- **Step 6 pending:** no outside-project conformance-suite result is recorded.

Step 0 remains an external acceptance goal, not a block on steps 1–2. The real test of this vision is an ordinary developer solving a problem they care about, not another internal count. Steps 1 and 2 follow because they remove the console-toolchain dependency and turn the one-off bug hunt into a standing guarantee.

## Where Switch fits

The first-frame oracle corpus (50 pinned programs) and the Switch-specific services, GPU and audio code remain, are tested in CI, and are maintained as long as they cost little. They are a regression set and a source of real programs, not the product. No Switch or Switch 2 compatibility is claimed.

The real [WebAssembly playground](PLAYGROUND.md) and versioned [conformance seed](../conformance/arm64-v0.1.0/README.md) now have local execution evidence. Hosted/public browser acceptance is recorded separately; the conformance seed does not yet have an outside-project result.
