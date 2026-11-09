# Contributing to ounce-core

Ounce-core is evolving into a reproducible execution-debugging and regression toolkit.
The current runtime and corpus adapters remain experimental and platform-specific.
Start with [AGENTS.md](AGENTS.md) and the ready outcomes in [PLAN.md](PLAN.md).

## Your first contribution (about an hour)

New here? Do one of these. Each ends in a small pull request.

1. **Run a program and compare** ([guide](docs/ADD_A_PROGRAM.md)). Pick an open-source program, build it as a Switch program, and compare its output with a host build. A difference is a bug report, and often a fix. This is the most valuable contribution.
2. **Fix a missing instruction.** If a run prints `[VM Error] Unimplemented instruction 0x...`, decode and implement it in `src/cpu/`, with a unit test. Examples: `PMULL`, the saturating-doubling ops, the narrowing three-different ops.
3. **Improve the docs.** If something here confused you, fix it.

Setup: install Rust, clone the repository, run `cargo build --release` and `scripts/gate.sh`. If the gate passes you are ready. Ask questions in [Discussions](https://github.com/hffgzktbcc-blip/ounce-core/discussions); no issue is needed before a small fix. Please read the [code of conduct](CODE_OF_CONDUCT.md); report vulnerabilities as described in [SECURITY.md](SECURITY.md).

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
