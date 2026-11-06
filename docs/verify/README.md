# Ounce-Verify v0

`tools/ounce_verify.py` validates already-collected JSON evidence. It never executes a guest,
downloads inputs, or turns a successful host process into a compatibility result.

```sh
python3 tools/ounce_verify.py evidence.json
python3 tools/ounce_verify.py evidence.json --output canonical-report.json
python3 -m unittest tests/test_ounce_verify.py
```

The input is one `ounce-verify/v0` record or a non-empty array. Output is canonical JSON with
records sorted by case ID. Verdicts are computed only from required `application` and `graphics`
oracles. Regression and engine-comparison oracles remain useful evidence but cannot elevate an
application verdict. Truncated evidence, duplicate IDs, malformed hashes, unknown schema values,
and a declared verdict that disagrees with the computed verdict are rejected with exit status 2.

This validator does not establish provenance or legal status. `content-addressed` means only that
both source-tree and binary SHA-256 fields were supplied and syntactically valid; it is not a
reproducible-build claim. Original-Switch evidence remains classified separately from Switch 2
homebrew evidence. Rendering requires an explicit raw guest-frame oracle; a present count, normal
guest exit, synthetic host frame, or agreement between Ounce-Core engines is insufficient.


## Evidence integrity gate

CI invokes the validator with `--artifact-root` set to the run’s builds directory.
The corpus adapter emits additive `artifacts` entries with `role`, relative `path`, and
`sha256`. With this option the validator streams each referenced file through SHA-256,
rejects paths escaping the root (including symlinks), and requires a matching binary
artifact for every passing record. Output records get `artifact_integrity: verified`
only after the bytes match `source.binary_sha256`; blocked records without binaries
remain `incomplete`. Missing or corrupt files fail validation (exit 2).

Without `--artifact-root`, v0 evidence remains readable and no artifact-integrity claim
is emitted. Hash verification establishes correspondence with the declared digest,
not independent provenance, reproducible builds, or frame correctness. The solver must
never rewrite expected frames to make a repair pass.

The headline uses `tests/corpus/oracles.json` as its denominator. Every registered
program needs exactly the interpreter and Tier-3 records to count as passing. Missing
programs remain in the denominator; duplicate IDs, unregistered programs/engines and
engine identity mismatches are rejected. Native execution remains separately assessed
until explicitly included in the required corpus engine policy.

## Bounded corpus execution

From the repository root on a Linux Docker host with Rust, Python 3 and the runtime build
dependencies installed, use the same entry point as the canonical CI job:

```sh
python3 scripts/verify_corpus_run.py --output /path/to/new-run --check-readme
```

The command builds the pinned outside programs, compiles the oracle runner, runs the
registered first-frame checks and validates the report against the built NRO bytes.
Use `--build-seconds`, `--compile-seconds` and `--engine-seconds` to adjust the limits.
CI also applies a job-wide deadline.

The output's `evidence/run-manifest.json` records the checkout revision, hashes of the
registered input manifests, and each stage's working directory (shared across stages),
argument vector, time limit, result and log paths. It is written incrementally so earlier
stages remain inspectable after a failure. The evidence directory also retains the raw
per-program observations, build inventories and canonical report when produced. Replay
a stage using its recorded argument vector and working directory with its input paths
available. Use `--image` to select the recorded toolchain image if available locally
or from the registry. Start a full rerun in a fresh output directory.

## Corpus diagnosis

Compare two retained canonical reports with the same command used for either a regression
or a historical failure:

```sh
python3 scripts/diagnose_corpus_run.py good/canonical-report.json failing/canonical-report.json \
    --output diagnosis.json
```

The bundle records input report hashes, groups non-passing cases by likely subsystem and
reports the first differing retained frame by index. When the frames match, it compares
terminal observations. Missing coverage and different NRO hashes or oracle expectations
are reported separately. Likely subsystem and follow-up checks are hypotheses. The first
retained difference does not locate the first differing guest instruction.

To replay a side of a case, check out the recorded core revision, locate the NRO at the path
registered for that program in the checkout's `tests/corpus/oracles.json`, and check its
SHA-256 against the bundle. Set `OUNCE_CORPUS_ROOT` to the build root and
`OUNCE_ORACLE_RESULT` to a fresh output file. In that checkout, run the recorded `argv`
with the recorded `env` entries. The normal corpus test runs both engines and checks
the same registered expectations. Keep the original canonical reports and per-program logs
alongside the diagnosis for audit or follow-up.

The SDL2 sample from the [historical corpus run](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/41872194259)
exited before frame 1 on both engines. Replaying the historical core revision reproduced
the early exit at guest PC `0x000000007ffe0004` with the same NRO SHA-256 as the
[passing reference run](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42175855697).
The bundle identified startup IPC and display initialization as the next paths to trace.

## Qualifying a corpus repair

Start a candidate branch from the failing revision. Use the diagnosis bundle to replay the
failing program before and after the patch, then run the full corpus on the candidate using
the bounded command above. Keep both canonical reports. Fetch the two revisions named in
those reports so the qualification command can read their registered oracles from Git. Run
the qualifier from a trusted checkout separate from the candidate.

```sh
python3 scripts/qualify_corpus_repair.py failing/canonical-report.json candidate/canonical-report.json \
    --program sdl2-simple --output repair-evidence
```

The gate requires an observed target failure in the reference, a clean pass on both engines
in the candidate, exact corpus coverage, and byte-identical registered oracle manifests.
Evidence tooling and build manifests must be unchanged. The gate compares the required
expectations, pinned source identity, execution limits and input scripts. The target must
execute identical NRO bytes. Previously passing cases must pass after rebuilding from the
same source, which allows timestamp differences in packaged RomFS artifacts. The output
retains both reports, both manifests, the diagnosis and replay instructions, and the gate
decision. A rejected candidate exits with status 1 and still records its evidence.

Use reports from the independent corpus workflow. Qualification compares the evidence
and the committed oracle manifests. The current corpus workflow supplies byte verification
and executes the pixel checks.

The corpus runner retries a Docker image pull up to twice after a timeout or a recognized
registry connection failure. It waits two seconds before the first retry and four before
the second. Each attempt keeps its own command, deadline, exit status, and logs in the
run evidence. Permanent pull errors, build errors, and oracle failures stop without retry.

Use `--shards 2` to run two disjoint groups of oracles concurrently on one host. The report
keeps the registered corpus order and the same coverage and regression checks. Each guest
still has its own log, observation file, and timeout. Canonical CI uses two shards. Compare
wall times only between runs with equivalent hosts and shard settings.

Pass `--build-cache PATH` to reuse builds on the same machine. The key includes the
resolved Docker image ID, build recipe, pinned manifests, and registered NRO paths.
A hit checks all NRO hashes against their build inventories before restoring them.
The runner still executes every oracle and verifies the artifacts in the report.
A bad entry is recorded as rejected and the runner builds from source. Cache entries
contain the registered NROs, build inventories, and toolchain package list.

Canonical CI persists this cache across runs and pins the devkitA64 image to a registry
digest. Updating that digest or the build inputs changes the CI cache key. The run
manifest reports whether the runner accepted, rejected, or missed the restored snapshot.

For a resumable local oracle run, pass `--resume` to `scripts/run_corpus_oracles.py` from
the start. After an interruption, repeat the same command with the same evidence
directory. A completed case is reusable when its executable, NRO, oracle manifest,
host and run settings match and its raw result and log still match their saved hashes.
Cases without a complete checkpoint run again. The final report still covers the full
corpus and applies the current regression baseline. Use a dedicated evidence directory
for each resumable run. This mode runs on Linux and macOS.

The main `verify_corpus_run.py` command also accepts `--resume`. Enable it on the first
invocation and repeat the command with the same output path after interrupting the
oracle stage. It verifies the saved build inventory and toolchain record before using
the existing builds, recompiles the oracle runner if needed, and retains stage logs
for each attempt. A run interrupted before its build snapshot is ready needs a new output
directory.

To locate a transition along a first-parent history, use
`python3 scripts/bisect_corpus_oracle.py --start GOOD --end BAD --program ID --builds BUILDS --output REPORT`.
Use the same built corpus inputs throughout. The command checks both endpoints and
runs the selected oracle on Interpreter and Tier-3 in an isolated Git worktree.
It retains per-commit logs, raw observations, and the remaining interval in
`bisection.json`. If an oracle changes or a trial cannot produce a comparable result,
the command reports the unresolved interval. Use `--before fail` to locate a repair.
Bisection assumes one transition within the chosen range.

The nightly corpus run compares its evidence with the last successful scheduled run.
A manual workflow run can name a baseline run ID. The comparison retains coverage,
verdict changes, and the diagnosis for replay. Missing current records and new oracle failures fail the comparison step. New cases
without older observations are reported as baseline gaps. Instruction counts are compared for the same binaries and
matching oracle observations. Wall-time changes are advisory and require matching host
and shard settings. The first successful scheduled run establishes the baseline.

## Switch 2 SDK-path observations

`python3 tools/ounce_verify.py observations.json --hbx-oracle tests/corpus/switch2/hello-cpu-first-frame.json`
validates the separate `ounce-verify/hbx-first-frame-v1` report. It verifies the
registered oracle file digest and source identity, requires both engines and all
registered samples, recomputes pixel and engine verdicts, and rejects inconsistent
claims. Without `--artifact-root`, hashes identify recorded inputs only. With
`--artifact-root PACKAGE`, the verifier independently hashes code.bin, data.bin and
report.json and rejects changed or missing bytes and paths escaping the package root. HBLauncher host UI callback rendering is explicitly distinguished
from Switch 2 GPU-path validation and does not enter the original-Switch headline.

## Trusted repair qualification

Run `qualify_corpus_repair.py` from a trusted checkout or detached worktree sharing
the reference/candidate Git objects. Do not run a solver-modified copy of the verifier
to approve that solver's patch. The qualifier rejects changed targets, provenance,
expectations, execution limits and missing engine/case coverage, and protects the
build cache, comparison tooling and recorded regression policy from candidate edits.
Changing an oracle requires an explicit owner decision outside repair qualification.

Bounded corpus execution now refuses a dirty checkout before creating outputs, so a
recorded commit cannot silently identify different source or verification code.
The original-Switch headline declares `target` and `required_engines` in the oracle
manifest; separately labeled Switch 2 results cannot inflate that metric.

## Native results viewer

In the Mac app, choose **View Check Results…** and open a saved canonical corpus JSON
report. The viewer explains pass, failure, blocked and not-assessed outcomes and shows
case IDs, target, engine, revision and oracle results. It is read-only: imported
results are labeled as recorded, not independently verified in the current session.
Opening a report runs no app, changes no oracle and approves no repair.
