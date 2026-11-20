# Try Ounce-Core: one useful outside test

**No PR required. No console files, firmware or keys.** An independently run mismatch—or a failed setup—is valuable evidence.

## Pick a path

| Time / equipment | Do this | Report |
|---|---|---|
| 2 minutes, browser only | Read the [real Lua wrong-answer case](LUA_FP_BUG_WALKTHROUGH.md) or [try the playground](https://hffgzktbcc-blip.github.io/ounce-core/playground/); say what was confusing | [Discussions](https://github.com/hffgzktbcc-blip/ounce-core/discussions) |
| ARM64 Mac or Linux, Rust + devkitPro/devkitA64 installed | Run the scoped Lua and floating-point comparison below | [Outside reproduction #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248) |
| Browser/editor, no ARM64 hardware | [Find a currently open beginner issue](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) and improve its documentation or examples | Submit a small PR |
| Rust / systems developer | [Try the open C library harness request #298](https://github.com/hffgzktbcc-blip/ounce-core/issues/298), or [browse currently open help-wanted tasks](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22) | Focused PR with tests |

## An ARM64 comparison

Use public, authorized test inputs only. This path needs an ARM64 Mac or Linux machine, a C compiler, Git, Python 3, Make, Rust, an internet connection for pinned open-source sources and **devkitPro/devkitA64** (see [full setup](../REPRODUCE.md)). The following checks **two named harnesses**, rather than trying every program:

```sh
git clone https://github.com/hffgzktbcc-blip/ounce-core.git
cd ounce-core
git rev-parse HEAD
cargo build --release --locked --features "nce native-jit"
python3 scripts/verify_differential.py \
  --only fp-instructions lua --nro \
  --json target/lua-fp-rerun.json
```

**Why `--nro` matters:** without `--nro` or locally provided `--nro-dir` binaries, the script can skip every guest-engine comparison even if it exits successfully. Open the JSON report and inspect the individual check results. Skipped or blocked checks are **not** a pass.

Record the commit, CPU, OS, Rust/devkitA64 versions, exact command, per-engine results and any error. Review the JSON before sharing; redact identifying paths and credentials. Use the [external report template](EXTERNAL_TEST_REPORT.md) and post in [#248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248). A fresh run on your machine is valuable even when it differs or is blocked. Do not call a run independently reproduced unless the retained evidence supports that narrow claim.

## What this is not

Ounce-Core is an experimental ARM64 execution-and-verification lab, **not** general Linux application compatibility or a retail-game runner. Coverage is partial; a local match is not universal proof. See [limits](../LIMITS.md).

**Maintainer commitment:** specific reproduction questions and focused PRs are welcome; no promised response-time SLA. Please do not post passwords, protected game assets or retail keys.
