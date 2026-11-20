# Understand Ounce-Core in five browser-only steps

You can explore the project and propose a documentation fix without installing anything. A hardware reproduction requires additional tools and a suitable machine.

1. **See what Ounce-Core does.** It runs selected ARM64 programs on multiple CPU engines,
   compares their results with recorded references and helps developers investigate differences.
   Read [the latest release](../releases/v0.10.0.md) and [vision](../VISION.md). It does not
   run arbitrary ARM64 or Linux applications.
2. **Inspect one real bug.** Read [why Lua calculated the wrong answer](LUA_FP_BUG_WALKTHROUGH.md):
   it shows the initial mismatch, the fix and the recorded hardware-reference check. Or choose
   a harness from [docs/verify](../verify/README.md) for more detail. Each recorded result applies
   only to its stated inputs; a roadmap item is a proposal, not a passed test.
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
