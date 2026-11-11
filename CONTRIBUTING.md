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

Preserve independent expectations, source provenance and honest failure reports. Do not submit retail firmware, keys or protected assets. See [Code of conduct](CODE_OF_CONDUCT.md) and [Security](SECURITY.md).

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
scripts/gate.sh --check runtime             # quick + default release tests/harness
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
Docs/metadata/Python-only changes avoid Rust rebuilds. Runtime/build/oracle changes
run the supported platform/feature matrices and canonical corpus. Full release
qualification remains separate. See the workflow files for exact triggers.
README prose edits stay cheap; changing certified headline/badge values requires
fresh canonical corpus validation. Embedded fonts count as runtime inputs.

Run focused tests while editing. Avoid repeating successful expensive checks on the
same inputs. In a PR state what changed for the user, how to reproduce/check it,
what passed and what remains untested. A missing tool is an environment limitation,
not a passing check; it does not prevent saving or submitting useful work.

Mark a PLAN task complete manually with its acceptance evidence. A green build,
PR creation or agent assertion does not by itself complete the user's scenario.
Outside dependencies block their dependent action, not all other work.

## Automated repository bots

The repository uses automated GitHub Actions workflows powered by standard runners and GitHub Models to streamline contributions:

- **Issue triage** (`.github/workflows/issue-triage.yml`): Runs on newly opened issues. It verifies required reproduction fields (`program`, `version`, `command`, `output`), asks politely for any missing information, suggests existing repository labels, searches for potential duplicate open issues, and summarizes crash or problem reports with their likely code area.
- **CI explainer** (`.github/workflows/ci-explainer.yml`): Runs when a pull request CI workflow run completes with failure. It posts or updates a single comment identifying the failing job and step, extracts relevant trailing log lines, explains the root cause using GitHub Models, and provides the local `scripts/gate.sh` command to reproduce the failure.
- **Shiftbot** (`.github/workflows/shiftbot.yml`): Provides status summaries of open pull request review states and checks.
- **Community onboarding** (`.github/workflows/community.yml`): Welcomes first-time contributors and monitors beginner issue availability.

## Sources and integrity

Permissively licensed code may be ported with notices in THIRD_PARTY.md and source
references in docs/PROVENANCE.md; this project is not clean-room. Use public specs,
permitted open-source workloads, and synthetic focused tests as appropriate. Record
source, licence, version/hash and build recipe for new redistributed fixtures.

Do not commit keys, retail software, firmware, dumps, leaked/private material or
incompatible licensed code. Retail decryption remains disabled. See
[the source policy](docs/CLEAN_ROOM.md) and [provenance](docs/PROVENANCE.md) for the
specific outstanding distribution questions. They do not prohibit unrelated tooling.

Preserve fixed expectations, input identities and failures when repairing behavior.
Keep `src/` safe Rust; unsafe operations remain confined to existing isolated crate
boundaries. Describe actual evidence, supported inputs and known limits plainly.

## Community guides

New to the project? Read [Start here](docs/community/START_HERE.md) for a five-step path. The
[glossary](docs/community/GLOSSARY.md) explains the terms, and the
[external test report template](docs/community/EXTERNAL_TEST_REPORT.md) is for reporting a
rerun, whether it matched, mismatched or could not run.
