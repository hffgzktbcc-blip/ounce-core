# Try Ounce-Core: one useful outside test

**No PR required. No console files, firmware or keys.** An independently run mismatch—or a failed setup—is valuable evidence.

## Pick a path

| Time / equipment | Do this | Report |
|---|---|---|
| 2 minutes, browser only | [Try the playground](https://hffgzktbcc-blip.github.io/ounce-core/playground/) and explain where the instructions confused you | [Discussions](https://github.com/hffgzktbcc-blip/ounce-core/discussions) |
| ARM64 Mac or Linux, Rust installed | Run the independent comparison below | [Outside reproduction #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248) |
| Browser/editor, no ARM64 hardware | [Check four paper references #301](https://github.com/hffgzktbcc-blip/ounce-core/issues/301) | Submit a small PR |
| Rust / systems developer | [Inspect failure behaviour #385](https://github.com/hffgzktbcc-blip/ounce-core/issues/385) or [add C library test #298](https://github.com/hffgzktbcc-blip/ounce-core/issues/298) | Focused PR with tests |

## An ARM64 comparison

Use public, authorized test inputs only. On an ARM64 Mac or Linux machine:

```sh
git clone https://github.com/hffgzktbcc-blip/ounce-core.git
cd ounce-core
cargo build --release
python3 scripts/verify_differential.py --json my-results.json
```

Record the commit (`git rev-parse HEAD`), CPU, OS, Rust version, exact command, output and whether it matched or failed. Review the JSON before sharing; redact identifying paths and credentials. Use the [external report template](EXTERNAL_TEST_REPORT.md) and post in [#248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248). Do not call a run verified unless the retained evidence supports that narrow claim.

## What this is not

Ounce-Core is an experimental ARM64 execution-and-verification lab, **not** general Linux application compatibility or a retail-game runner. Coverage is partial; a local match is not universal proof. See [limits](../LIMITS.md).

**Maintainer commitment:** specific reproduction questions and focused PRs are welcome; no promised response-time SLA. Please do not post passwords, protected game assets or retail keys.
