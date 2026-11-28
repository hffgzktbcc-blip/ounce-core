# Contributing to Ounce-Core

**Understand why programs fail. Prove when they work.**

Ounce-Core is an experimental ARM64 execution and verification lab written in Rust. We compare selected open-source programs across execution engines and real-hardware references. Nintendo Switch knowledge is not required for beginner contributions.

## Find a first task

New to the project? Visit our [contributor landing page](https://hffgzktbcc-blip.github.io/ounce-core/contribute.html) or [GitHub's live Contribute page](https://github.com/hffgzktbcc-blip/ounce-core/contribute). First-time PRs receive one automated welcome; human maintainers still review all changes.

- [Good first issues](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) — scoped documentation and usability improvements; some require only a browser.
- [Help wanted](https://github.com/hffgzktbcc-blip/ounce-core/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22) — additional contributor opportunities.
- [Independent ARM64 reproduction #248](https://github.com/hffgzktbcc-blip/ounce-core/issues/248) — an outside test with evidence is particularly valuable; failed reproductions are welcome.
- [Cross-host FP discrepancy #257](https://github.com/hffgzktbcc-blip/ounce-core/issues/257) — specialist task, not beginner work.

## How to contribute without write access

1. Pick an issue; check whether someone has already opened a PR for it.
2. For documentation-only issues, use GitHub's browser editor in your fork. Rust, ARM64 hardware and a console SDK are not needed.
3. Submit a focused PR to main with the issue number and what you verified; disclose checks you could not run.
4. A maintainer reviews the change. A merged PR does not automatically establish verified or independently reproduced behaviour.

Start with [Vision](docs/VISION.md), [Limits](docs/LIMITS.md) and [Reproduction guide](docs/REPRODUCE.md). The current supported envelope is narrower than arbitrary ARM64/Linux applications. Ask in [Discussions](https://github.com/hffgzktbcc-blip/ounce-core/discussions) if stuck.

Preserve independent expectations, source provenance and honest failure reports. Do not submit retail firmware, keys or protected assets. When editing documentation, use [the plain-language and freshness checklist](docs/README.md#keep-live-documentation-clear-and-current). See [Code of conduct](CODE_OF_CONDUCT.md) and [Security](SECURITY.md).

---

## Advanced local development and runtime contributions
## Make a useful change

An issue is welcome for a new substantial direction, but is not required for an
already-authorized fix, documentation improvement or small contribution. Inspect
existing PRs to avoid duplicating work. Use your available checkout on a topic branch;
there is no required workstation path or assigned agent/file owner.

Reproduce the problem, make the smallest useful change, run focused checks, and
submit the result with a command or artifact another developer can use. Code and
unit tests are valid implementation progress; user-facing claims need a check of
the actual behavior. Pixel oracles apply to graphics claims, not every kind of work.
An external rerun is required to call a result independent, not to submit a fix.

## Optional helpers

```sh
scripts/next.sh                            # local read-only task/status summary
# Edit files, then stage only the paths you own:
git add <paths>
scripts/gate.sh --checkpoint "what/why"    # local signed-off commit; no tests/push
scripts/gate.sh --checkpoint "what/why" --push  # save and push explicitly
scripts/gate.sh                            # relevant local checks by changed paths
scripts/gate.sh --check quick               # shell/Python/policy checks; no Rust
scripts/gate.sh --check runtime             # quick + clippy + default release tests/harness
scripts/gate.sh --check full                # runtime + optional feature tests
scripts/gate.sh --submit                    # push and open/update a PR; no merge
```

Ordinary Git and GitHub commands are equally supported. `--ship` is a compatibility
alias for `--submit`; it does not tick PLAN, certify evidence or enable auto-merge.
A checkpoint commits only staged files and records validation as not run. `--next`,
`--tried`, `--decision`, `--step` and `--proof` are optional commit notes. Network
failure leaves the local checkpoint intact. Sign off commits (`git commit -s`).

## Validation and integration

CI always emits the existing required `Test (ubuntu-latest)` and
`Test (windows-latest)` statuses. They summarize the applicable legal, tooling,
runtime and feature checks; failed or cancelled applicable checks fail the summary.
Docs/metadata/Python-only changes avoid Rust rebuilds on pull requests and pushes.
Runtime/build/oracle changes run single-OS smoke slices on pull requests and expand on
pushes to `main`. Every Monday at 03:00 UTC (`schedule`) and on manual `workflow_dispatch`,
`ci.yml` runs every job regardless of changed files on Linux x86-64, Linux ARM64, macOS
ARM64 and Windows, including all 102 paper checks (`paper-checks`, on `ubuntu-24.04-arm`), with
pass/fail counts in the job summary and a 30-day `paper-checks-logs` artifact.
`differential.yml` runs all 51 Linux ARM64 programs on its own Monday schedule and on
`workflow_dispatch`. Full release
qualification remains separate. See the workflow files for exact triggers.
Rust changes must build without warnings: the `lint` job runs
`cargo clippy --locked --workspace --all-targets -- -D warnings` (also with
`--features native-jit,nce` and `--features gpu-present`) on a pinned toolchain, and the
`msrv` job builds default features with Rust 1.89. Fix a lint rather than silencing it;
when a lint is wrong for a site, use a scoped `#[allow(clippy::...)]` with a one-line reason.
README prose edits stay cheap; changing certified headline/badge values requires
fresh canonical corpus validation. Embedded fonts count as runtime inputs.

Run focused tests while editing. Avoid repeating successful expensive checks on the
same inputs. In a PR state what changed for the user, how to reproduce/check it,
what passed and what remains untested. A missing tool is an environment limitation,
not a passing check; it does not prevent saving or submitting useful work.

Mark a PLAN task complete manually with its acceptance evidence. A green build,
PR creation or agent assertion does not by itself complete the user's scenario.
Outside dependencies block their dependent action, not all other work.

## Versioning and releases

The project owner tags releases. Until `v1.0.0`, versions are `0.MINOR.PATCH`:

- **Minor (`0.13.0`)**: anything users see or rely on changes. That covers new features or flags, changed CLI
  output, `ounce.run.v1`, `ounce.trace.v1` or `ounce.repro.v1` schema changes, exit codes, the Rust library API,
  what the release downloads contain, and emulation changes that alter a guest program's results.
- **Patch (`0.12.1`)**: fixes only, with no new flags, schema fields or contract keys. That covers correctness
  fixes, docs and CI.
- Breaking changes are allowed before 1.0, in a minor release only; a patch release never contains one.
- A change is **breaking** when a script, saved report or comparison that worked on the previous release can fail or read
  differently without being edited: a removed or renamed flag, schema field, stop kind or exit code, a changed meaning or
  default for one of them, or a changed default engine. Adding a field, flag or stop kind is not breaking.
- Each breaking change is its own entry in [CHANGELOG.md](CHANGELOG.md) that starts with `Changed (breaking):` and ends
  with a `Migration:` sentence saying what to change. A pull request that makes a breaking change is not merged without both.
- The release pull request renames `## Unreleased` to `## X.Y.Z — YYYY-MM-DD (released)`, opens a new empty
  `## Unreleased` above it, and sets `version` in `Cargo.toml` to `X.Y.Z`; the annotated tag is `vX.Y.Z` on that commit.

Cadence: at most one minor release a week. A patch release ships only when a correctness or security fix can't
wait. Two releases never ship within 24 hours unless the second fixes a correctness bug in the first.

A release ships only when:

1. CI is green on the full platform matrix, including Linux x86-64, Linux ARM64, Windows and macOS (pull requests run only part of it,
   so run `ci.yml` and `differential.yml` by hand (`workflow_dispatch`) on the release branch: together they run the full
   platform matrix, all 51 Linux ARM64 programs and all 102 paper checks);
2. `python3 scripts/refresh_notice_review.py --check` passes, and the notice review has no open items;
3. the release notes (`docs/releases/vX.Y.Z.md`) use absolute links, because GitHub publishes the file as the
   release page;
4. every headline number in the release notes links to its evidence and says what it was compared against.

Only the newest patch of the newest minor is supported ([SECURITY.md](SECURITY.md)). `v1.0.0` follows the
contract freeze in [#481](https://github.com/hffgzktbcc-blip/ounce-core/issues/481); from then on, versions follow
[Semantic Versioning](https://semver.org/) strictly. The crate stays `publish = false` until the owner decides on
crates.io. Release tags are annotated `vX.Y.Z` tags; no new component tags (such as the historical
`diagnosis-v0.2.0`) are created.

## Sources and integrity

Permissively licensed code may be ported with notices in THIRD_PARTY.md and source
references in docs/PROVENANCE.md; this project is not clean-room. Use public specs,
permitted open-source workloads, and synthetic focused tests as appropriate. Record
source, licence, version/hash and build recipe for new redistributed fixtures.

Do not commit keys, retail software, firmware, dumps, leaked/private material or
incompatible licensed code. Key handling and encrypted-container decryption are out of scope. See
[the source policy](docs/CLEAN_ROOM.md) and [provenance](docs/PROVENANCE.md) for the
specific outstanding distribution questions. They do not prohibit unrelated tooling.

Preserve fixed expectations, input identities and failures when repairing behavior.
Keep `src/` safe Rust; unsafe operations remain confined to existing isolated crate
boundaries. Describe actual evidence, supported inputs and known limits plainly.

## AI-assisted contributions

Contributions prepared with AI assistants (LLMs, coding assistants, or agents) are welcome under the following conditions:

- **Disclose AI use**: Note the tool or model used and what it did in your pull request description.
- **Human responsibility**: The submitter is accountable for the change. You must understand the proposed code, verify that it works, and be able to explain and defend it during review. Do not submit unreviewed machine output.
- **Test results over claims**: Run focused checks locally and report the exact commands and output. Never fabricate compatibility or performance claims; state untested boundaries and known limits plainly.
- **Integrity and provenance**: Follow [AGENTS.md](AGENTS.md) and repository safety rules. Do not commit keys, retail content, firmware, dumps, or incompatible licensed code.
- **Safe Rust**: Keep `src/` safe Rust; `unsafe` operations remain confined to existing isolated crate boundaries.

## Community guides

New to the project? Read [Start here](docs/community/START_HERE.md) for a five-step path. The
[glossary](docs/community/GLOSSARY.md) explains the terms, and the
[external test report template](docs/community/EXTERNAL_TEST_REPORT.md) is for reporting a
rerun, whether it matched, mismatched or could not run.

## Git Blame Hygiene

To skip tree-wide formatting commits (listed in [`.git-blame-ignore-revs`](.git-blame-ignore-revs)) when running `git blame`, configure Git once in your clone:

```bash
git config blame.ignoreRevsFile .git-blame-ignore-revs
```
