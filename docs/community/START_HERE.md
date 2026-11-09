# Start here: five steps for a first-time reader

You only need a browser for steps 1 to 5.

1. **What it is.** Ounce-Core is an open-source ARM64 execution and verification lab in safe
   Rust: it runs selected programs on several CPU engines and checks the results against real
   hardware. Read [the vision](../VISION.md). It is not a general emulator and does not run
   arbitrary ARM64 or Linux applications.
2. **Inspect one bounded result.** Open [docs/verify](../verify/README.md) and pick a harness
   such as LZ4. Each one says which inputs were compared and what matched. A result that exists
   is evidence for those inputs only. A capability listed under "Roadmap" is a proposal, not a
   result.
3. **See the limits.** [LIMITS.md](../LIMITS.md) says, for each claim, what it does not prove,
   and gives the four maturity labels. Unfamiliar word? Try the [glossary](GLOSSARY.md).
4. **Choose a first task.** Browse the open
   [good first issues](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aopen+label%3A%22good+first+issue%22).
   Most are documentation and need only a browser. [CONTRIBUTING.md](../../CONTRIBUTING.md)
   explains how to send a change.
5. **Report feedback.** Ask questions in
   [Discussions](https://github.com/hffgzktbcc-blip/ounce-core/discussions). If you ran the
   checks yourself, say what happened with the
   [test report template](EXTERNAL_TEST_REPORT.md), including a mismatch or a failed run. The
   [independent tester challenge](../INDEPENDENT_TESTER_CHALLENGE.md) explains what counts as an
   outside reproduction.
