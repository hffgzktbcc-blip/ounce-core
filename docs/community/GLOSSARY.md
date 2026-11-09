# Plain-English glossary

Short definitions of the words this project uses when it talks about evidence. For what each
result does and does not show, read [LIMITS.md](../LIMITS.md). The four maturity labels are
defined in [VISION.md](../VISION.md#maturity-labels). The method is in
[docs/verify](../verify/README.md).

1. **ARM64 / AArch64.** The 64-bit instruction set used by Apple Silicon Macs, many phones and
   servers. "AArch64" is Arm's own name for the execution state; "ARM64" is the common name for
   the same thing. This project runs selected ARM64 programs.
2. **Interpreter.** A program that reads ARM64 instructions one at a time and does what each one
   says, in ordinary Rust. It is simple and slow, which makes it a good reference point for the
   faster engines. Tier-3 is a faster engine that fuses hot decoded blocks into traces, still in safe Rust and without emitting native code.
3. **JIT (just-in-time compiler).** Translates ARM64 instructions into machine code for the host
   while the program runs. In this project the optional Tier-4 recompiler does this. A JIT can be fast
   and wrong in ways an interpreter is not, so its output is compared against the others.
4. **Hardware reference.** The output of the same test program built for and run on a real
   ARM64 machine. It is the "answer" the engines are compared with. It is not an emulator run.
   The committed references were recorded on Apple Silicon; see
   [LIMITS.md](../LIMITS.md) for what that does not cover.
5. **Native reference.** A host build of the test cases compiled with the host C compiler and run
   directly on the host CPU. Together with the committed `expected-host.txt`, it gives the
   bytes the engines must match. On an ARM64 host the "native" engine runs the guest on the host CPU,
   so it checks the harness and toolchain, not the emulator.
6. **Differential testing.** Run the same program in two or more ways and compare the outputs
   byte for byte. Any difference is a bug in one of them, or a documented difference between
   machines.
7. **Oracle.** The thing that decides whether a result is right. Here that is a hardware
   reference, a published test vector, or, for the first-frame corpus, a registered image hash.
   An oracle is only as strong as how independently it was defined.
8. **Reproducibility.** Someone else can follow the written steps and get the same result. The
   steps live in [REPRODUCE.md](../REPRODUCE.md). Reproducing on your own machine is not the same
   as the project's own CI saying it passed.
9. **Bounded-verified.** A result matches a reference on the listed inputs, with retained
   evidence. It says nothing about inputs that were not tested, and it is not a claim that a
   program or the emulator is correct everywhere.
10. **Independently reproduced.** Someone outside the project reran the result and got the same
    answer. Nothing carries this label yet. Filling in a report template does not earn it; only a
    real outside rerun with evidence does. See the
    [independent tester challenge](../INDEPENDENT_TESTER_CHALLENGE.md).
