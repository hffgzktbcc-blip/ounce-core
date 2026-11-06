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
in the candidate, exact corpus coverage, and byte-identical registered oracle manifests. Evidence tooling must be unchanged.
It compares the required expectations in the reports and checks that the target and all
previously passing cases execute the same NRO bytes under the same limits and inputs, and
pass in the candidate. The output
retains both reports, both manifests, the diagnosis and replay instructions, and the gate
decision. A rejected candidate exits with status 1 and still records its evidence.

Use reports from the independent corpus workflow. Qualification compares the evidence
and the committed oracle manifests. The current corpus workflow supplies byte verification
and executes the pixel checks.
