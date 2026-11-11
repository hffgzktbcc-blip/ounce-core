# Outside reproduction and contribution: November 2026

Ounce-Core welcomes reproducible reports, including failed runs, from people outside the maintainer workflow. This note documents the first public outside test report and a related merged code fix; it is **not** a claim that every report has passed second-person verification.

## Work reported by @cyzcz

In [community challenge #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190), [@cyzcz](https://github.com/cyzcz) documented executions on Debian 12 / Linux x86_64, with a pinned Ounce-Core commit, toolchain information, original sample revisions, binary hashes, and [public evidence artifacts](https://github.com/cyzcz/protocol-verification-artifacts).

| Experiment | Reported observation | Evidence and acceptance boundary |
| --- | --- | --- |
| `romfs` | Pre-registered first-frame output matched on Interpreter and Tier-3. | [Independent report #191](https://github.com/hffgzktbcc-blip/ounce-core/issues/191); reviewed, still **self-reported pending a second-person rerun**. |
| `simplegfx` | Interpreter and Tier-3 matched across 61 frames. | [Follow-up report in #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190); self-reported, second-person rerun pending. |
| `deko-basic` | Corpus oracle passed; a separate standalone oracle rejected a different clean build hash before execution. | [Report #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190) and [bug #192](https://github.com/hffgzktbcc-blip/ounce-core/issues/192). The build-path-sensitive digest problem was fixed through **[@cyzcz's merged PR #193](https://github.com/hffgzktbcc-blip/ounce-core/pull/193)**; #192 is correctly closed. |
| `cpu-jit` | Interpreter and Tier-3 matched the registered frame and pixel probes. | [Follow-up report in #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190); self-reported, second-person rerun pending. |
| `audio-echo` | Reproduced an expected failure on the pinned older commit: `audin:u` command 3 was missing and the observed frame hash differed from the oracle. | [Follow-up report #190](https://github.com/hffgzktbcc-blip/ounce-core/issues/190). The relevant audio paths were subsequently reported repaired in [completed issue #188](https://github.com/hffgzktbcc-blip/ounce-core/issues/188); **do not imply that the older reproduction proves a failure on current main**. |

Thank you for reporting both matching and non-matching outcomes, and for supplying a concrete fix. Attribution above describes publicly linked work; a voluntary contributor spotlight or named recognition beyond this factual record should be confirmed with the contributor first.

## The challenge remains open

These are **opportunities for further work**, not requests to reopen already-resolved bugs:

- [#190 — independent first-frame challenge](https://github.com/hffgzktbcc-blip/ounce-core/issues/190): new testers can reproduce the registered oracles, including a negative result, and share host, exact commit, hashes and logs.
- [#191 — independent RomFS report](https://github.com/hffgzktbcc-blip/ounce-core/issues/191): a **different** tester can rerun the pinned recipe, produce the required canonical byte-validated evidence and compare it with the first report. The published recipe pins a different revision/build method; document deviations rather than treating them as identical.
- [#248 — hardware differential reproduction](https://github.com/hffgzktbcc-blip/ounce-core/issues/248): reproduce on a separate ARM64 host; this is a distinct challenge from the x86_64 homebrew oracle runs.
- [Good first issues](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) and [help wanted](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22): small code, docs, test and evidence tasks.

Start with [the tester challenge guide](../INDEPENDENT_TESTER_CHALLENGE.md) or [CONTRIBUTING.md](../../CONTRIBUTING.md). Please report failures faithfully and do not change preregistered expected outputs to manufacture passes. No retail assets, firmware, keys or redistribution of unlicensed binaries.

The internal 50/50 first-frame corpus is separate from the number of people who have independently reproduced a result. Keep the **second-person rerun** requirement explicit.
