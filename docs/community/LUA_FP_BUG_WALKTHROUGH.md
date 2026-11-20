# One real bug: Lua's maths was wrong even when Ounce-Core's engines agreed

**Start here if you want to understand what Ounce-Core actually does.** No installation is needed to follow this recorded example. The experiment happened on **8 November 2026**; this page explains that dated result, not a new test run.

## The problem

Lua 5.4.7 ran a small script through Ounce-Core. The emulator engines completed the test, but some answers were wrong. For example, the historical run reported:

```text
math.sin(1) = 1.1585    # wrong result from the recorded guest run
```

The expected mathematical value, in radians, is about **0.84147**. The Lua test also exposed errors in `math.cos` and `pow`. **Agreement between the engines did not make the answer correct.**

## What Ounce-Core checked

1. The maintainers pinned Lua 5.4.7 and compiled the same test for an Apple Silicon host and for an ARM64 homebrew NRO.
2. They ran the NRO through the recorded Tier-3, Tier-4 and native paths, then compared the output with the separately built host reference.
3. The mismatch led to a smaller, **5,964-line floating-point instruction test**. Before the fix, **533 lines disagreed**, all for `FNMADD` and `FNMSUB`.
4. The interpreter applied the wrong sign to the product in these fused operations. The fix corrected that calculation and added tests for all four fused multiply-add/subtract forms.
5. In the **8 November macOS ARM64 recorded rerun**, the floating-point test and Lua output matched the host byte-for-byte on the three tested paths.

The important distinction is that **the first engine-to-engine agreement did not establish correctness**. A separately built, native-hardware reference exposed the mistake.

## Inspect the actual evidence (browser only)

- [PR #222 — the bug, correction and review](https://github.com/hffgzktbcc-blip/ounce-core/pull/222) documents what failed and what changed.
- [Floating-point harness report](../verify/fp-instructions/README.md) records the 533 mismatches among 5,964 lines and the bounded successful rerun.
- [Lua 5.4.7 harness report](../verify/lua-harness/README.md) shows the three checked engines and the original mismatching maths results.
- [Test sources and recorded expectations](../verify/fp-instructions/) let you inspect the reference rather than trusting a headline.

## Rerun the *current* code (optional)

This is for someone with an **ARM64 Mac or Linux machine**, Rust, a C compiler, Python 3, Git, Make, and **devkitPro/devkitA64 with libnx** installed. [Install/setup and reporting details](../REPRODUCE.md) explain the NRO prerequisites. You do **not** need retail firmware, games, keys or a physical console.

```sh
git clone https://github.com/hffgzktbcc-blip/ounce-core.git
cd ounce-core
git rev-parse HEAD  # record the exact code under test

cargo build --release --locked --features "nce native-jit"
python3 scripts/verify_differential.py \
  --only fp-instructions lua \
  --nro \
  --work target/lua-fp-rerun \
  --json target/lua-fp-rerun.json
```

The `--nro` flag matters: **without it (or `--nro-dir` with suitable local test binaries), the script can skip the guest-engine comparisons**. A zero exit code from a host-only check is **not** an independent ARM64 rerun. The script also downloads pinned open-source test dependencies, so network access is needed. Choose a new `--work` directory if rerunning.

Read the per-harness `tier3`, `tier4`, `native` and `committed-vs-fresh-host` entries in the JSON report. Record any `DIFFERENT`, `skipped`, `expected-different`, error or unsupported case, not just the final exit code.

**A fresh run on today's checkout is not automatically a recreation of the original 8 November environment.** Note the exact commit, host, compilers, devkitA64 version and any changed inputs. To share your observation, use [independent reproduction issue #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248), including failures.

## What this example does *not* prove

This was a bounded test of a particular Lua version and floating-point cases on one recorded ARM64 host. It does **not** prove that all ARM64 instructions, all Lua programs or arbitrary Linux applications work. The project has **not established a successful outside reproduction** of its published results; a merged PR and the maintainers' own rerun are internal evidence.

For more test results, see the [current v0.10.0 release](../releases/v0.10.0.md) and [limits](../LIMITS.md). For a **static Linux ARM64 ELF** test that doesn't need devkitPro, use the [community verification guide](../COMMUNITY_VERIFICATION_GUIDE.md).
