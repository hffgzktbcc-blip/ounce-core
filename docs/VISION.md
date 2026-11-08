# Vision

**Run ARM64 programs anywhere, and prove the answer matches real hardware.**

Ounce-Core is a safe-Rust AArch64 execution lab. It started as a Switch homebrew runtime. Switch and Switch 2 are no longer the goal. What the project turned out to be good at is something more general: running real programs on an ARM64 CPU model and checking, byte for byte, that the result matches a real ARM64 machine.

## What the evidence says (8 November 2026)

In one working week, running unmodified open-source programs and diffing them against an Apple Silicon host found, and fixed, silent and loud bugs that a year of Switch-specific testing had not:

- a prefetch instruction decoded as a load, silently overwriting registers;
- two floating-point instructions with the wrong sign, so `sin`, `cos` and `pow` returned wrong numbers;
- missing vector instructions (widening multiply and add, byte reverse, register shifts, table lookup, polynomial multiply, narrowing and saturating ops), plus CRC32, scalar add and duplicate forms, and a fixed 50-million-instruction cap.

Programs checked so far on three engines (an interpreter, a JIT and native host execution): LZ4, xxHash, SQLite, Lua, and, in review, zlib, cJSON, TinyExpr and Monocypher. Method and results: [docs/verify/](verify/README.md). A 300-input loader fuzz run produced no panics. This is the asset: a repeatable way to find ARM64 emulation bugs quickly.

## What it can become

1. **Verify.** A conformance and differential-testing toolkit for ARM64 execution. Point it at any program, or at generated instruction sequences, and it reports the first place a model and real hardware disagree. Useful for anyone building or trusting an emulator, a JIT, a binary translator or a hypervisor.
2. **Run.** User-mode ARM64 on any host (x86 Linux, Windows, macOS, a browser) in memory-safe Rust: deterministic, sandboxed, embeddable. The step that makes this real is loading ordinary static Linux ARM64 programs with a small system-call layer, so no console SDK is needed to run or test anything.
3. **Debug.** Record and replay a run, trace it, and compare two engines to find the first diverging instruction. Failures become small shareable reproductions.
4. **Learn.** A way to see an ARM64 program execute: registers, flags, vector lanes, one instruction at a time, explained. Eventually in the browser.

## Honest limits

- It is not a fast emulator and does not aim to compete with QEMU or Apple's Rosetta on speed or coverage. Its claim is verified correctness on what it supports, and clarity.
- Coverage today is partial: many vector, floating-point and system instructions are undecoded or untested. The cumulative saturation flag (QC) is not modelled. Native execution runs on the host CPU, so on an ARM64 host it tests the harness, not the emulator.
- No claim of Switch or Switch 2 compatibility is made. The earlier work stays in the repository as a regression corpus of pinned, permitted programs; it is a test set, not the product.

## Roadmap (each step is one small, checkable result)

| # | Result | How we will know |
|---|---|---|
| 1 | Run static Linux ARM64 programs (ELF loader plus about 40 Linux system calls, deterministic mode) | the verified programs run as plain `aarch64-linux` binaries on Linux, macOS and Windows, with no console SDK |
| 2 | Instruction-level differential fuzzer in CI on a real ARM64 runner | thousands of generated sequences per instruction family match hardware; every new decoder change is fuzzed |
| 3 | First-divergence report between engines and against a reference | a failing run points at one instruction with registers before and after |
| 4 | Record and replay | a recorded failure replays identically on another machine |
| 5 | Browser playground (WebAssembly build) | a visitor steps through a program with no install |
| 6 | Publish the conformance suite for other emulators | at least one outside project runs it and reports results |

Steps 1 and 2 come first because they remove the dependency on a console toolchain and turn the one-off bug hunt into a standing guarantee.

## Where Switch fits

The first-frame oracle corpus (50 pinned programs) and the Switch-specific services, GPU and audio code remain, are tested in CI, and are not removed. They are a regression set and a source of real programs, maintained as long as they cost little. New work is not driven by them.
