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

CI now invokes the validator with `--artifact-root "$RUNNER_TEMP/ounce-oracle-builds"`.
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
