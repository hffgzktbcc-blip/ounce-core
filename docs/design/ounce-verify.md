# Ounce-Verify: independent execution evidence contract (design proposal)

Status: proposal only, 2026-10-30. No runtime code or test behavior is changed by this document.

## Purpose
Ounce-Verify is a **deterministic evidence and regression layer** for Ounce-Core. It records what was actually executed, what was observed, and whether an explicitly defined oracle passed. It must not turn "host exit 0", successful CI, successful build, or process startup into a compatibility claim. The longer-term goal remains a verified Switch 2 homebrew vertical slice; original-Switch libnx samples are foundation tests, *not* Switch 2 compatibility evidence.

## Coordination and existing sources of truth
- PR #89 is merged. Main contains an opt-in external-corpus build and interpreter startup probe. The current manifest pins four original-Switch candidates; the preparation inventory records source and NRO hashes and observed toolchain details. The probe records host status and guest-stop diagnostics, but it is not a structured compatibility verdict, has no scripted input or frame oracle, and the manifest still labels candidates as untested.
- PR #90 remains open and proposes an explicit CPU-engine NRO runner with bounded guest observations.
- PR #91 is merged and adds bounded Binder parcel and object-lifetime validation.
- PR #93 is merged and adds synthetic fixture execution and Tier-3 CPU regressions.
- PR #94 is merged and corrects host replay behavior. PR #95 remains open and covers VI/Binder producer-window and graphics-startup work.
- This is a **documentation-only, independent planning lane**. Do not modify those branches, the external-sample build script, kernel, GPU, CPU, or workflow owners without coordination through PR comments / AGENTS.md.
- Follow docs/CLEAN_ROOM.md and docs/PROVENANCE.md. No firmware, retail keys, copyrighted game assets, upstream implementation code or built upstream binaries in the repository.

## v0 evidence record (proposed JSON object)
Records must be stable, bounded, machine-readable and self-describing. Unknown/not-collected is distinct from empty/zero/success.

```json
{
  "schema_version": "ounce-verify/v0",
  "case_id": "independent/hello",
  "target": "original-switch-libnx",
  "source": {
    "origin": "external-public-homebrew",
    "repository": "source locator recorded by corpus manifest",
    "commit": "pinned source revision",
    "source_tree_sha256": "sha256-hex-or-null",
    "binary_sha256": "sha256-hex-or-null",
    "toolchain": "observed identity; not a reproducibility guarantee"
  },
  "execution": {
    "core_commit": "40-char-git-sha",
    "engine": "interpreter",
    "runner": "external-corpus-probe",
    "input_script_sha256": null,
    "limits": {"max_guest_instructions": 1000000, "host_timeout_seconds": 30},
    "host_status": "exited-zero",
    "guest_stop": "exit-process",
    "guest_pc": "0x0000000000000000",
    "guest_x0": "0x0000000000000000",
    "guest_instructions": 100,
    "diagnostics_truncated": false
  },
  "oracles": [
    {"name": "guest-stop", "status": "pass", "expected": "exit-process", "observed": "exit-process"}
  ],
  "verdict": "not-assessed"
}
```

All values in the example are illustrative, not observations or the current emitted artifact schema. Main's external-corpus workflow currently emits an inventory and a startup-probe record with fields such as `runtime_commit`, `source_head`, `requested_engine`, `actual_engine`, `probe_stop`, `host_exit_code`, `guest_stop`, optional `break_result`, `log_sha256`, and `compatibility: "not-assessed"`. The inventory records per-source-file hashes and per-NRO hashes; it does not currently emit this example's `source_tree_sha256` field. The validator design should map these existing records explicitly, preserving their original fields, before defining any normalized record. Null = not observed, never success. Preserve runtime commit and test-merge commit separately where relevant.

## Strict verdict vocabulary
- `pass`: at least one **explicit, appropriate oracle** passed, and every required oracle for that named case passed.
- `fail`: one or more required oracles demonstrably failed (not an expected unsupported-feature observation).
- `blocked`: required execution could not be completed (missing toolchain, timeout, unsupported operation, inability to build).
- `not-assessed`: evidence collected but no sufficient compatibility oracle exists.
- An expected failure can *pass a regression oracle* while the program's compatibility verdict remains `not-assessed` or `blocked`.
- Never infer guest success from a zero host status; `svcBreak` and host exit 0 is **not** a guest pass.
- A guest's normal `svcExitProcess` is evidence of startup/exit, **not** rendering correctness.
- Rendering pass requires a known-good raw-frame oracle (dimensions, pixel format, stride/layout, expected pixels or golden hash with tolerance policy) and actual guest-originated frame bytes. No hard-coded/synthetic success frame, present-call counter, or host-only mock counts as a guest frame.
- A mismatch between interpreter and Tier-3 is a divergence, **not** proof that either is correct. Interpreter is a diagnostic reference, not an independent external oracle.

## Case isolation and reproducibility
1. Run one unchanged candidate per evidence record, with explicit engine/feature flags/input script.
2. Record pinned source revision, actual toolchain/container identity, hashes of inputs and outputs, core head SHA, OS/architecture, and command line. Preserve original-Switch versus Switch 2 target classification.
3. Separate host build outcome, host-run outcome, guest stop, visual output and oracle verdict.
4. Bound CPU steps, logs, IPC capture, artifact sizes and outer wall-clock. Mark truncation or timeout explicitly.
5. Maintain a versioned manifest with case ID, provenance category, target, expected oracles and required observation fields. Reject duplicate case IDs and malformed or absent required fields.
6. Prefer structured deterministic output; stable order and redaction of local paths/tokens. Never include private host environment variables.
7. In CI, test the **validator** against fabricated positive/negative records, but only report a real workload verdict from actually executed artifacts.

## v0 minimal implementation (new files only; coordinate before touching workflows)
- `tools/ounce_verify.py`: validate JSON schema/manifest invariants and compute an oracle-based case verdict; read existing probe artifacts as inputs, do not run the program or modify it.
- `tests/test_ounce_verify.py`: stdlib-only deterministic tests.
- `tests/verify/fixtures/`: tiny synthetic JSON fixtures (not upstream NRO files).
- `docs/verify/README.md`: CLI examples and provenance limits.
- No frontend dependency in v0: output can later be shown in the UI without putting verdict rules in the UI.

## Required validator tests before integration
1. Host status zero + guest `svcBreak` => never pass.
2. Normal guest exit + missing frame oracle => graphics status not-assessed.
3. Expected unsupported IPC response + regression oracle => regression pass, application compatibility not-assessed.
4. Missing source/binary hash and moving toolchain => incomplete reproducibility; never described as bit-for-bit reproducible.
5. Interpreter/Tier-3 divergence => explicit comparison mismatch; no automatic guilty engine.
6. Empty/missing required field, duplicate case ID, unknown verdict, unsupported schema version, and record truncation => explicit validation failure or incomplete evidence.
7. Synthetic golden frame match/mismatch test, with unambiguous pixel format, dimensions, row layout and declared tolerance.
8. Deterministic JSON report across repeated runs for identical inputs; no timestamps or machine-specific paths embedded in canonical comparison.
9. Both successful and failing cases preserve separate host execution and guest execution states.
10. Switch 2 claims require a distinct Switch 2 target case and its own independently observed oracle; original-Switch results cannot satisfy this gate.

## Out of scope for v0
- New interpreter features, Tier-4 implementation, VI/Binder queue or GPU fixes.
- Automated compatibility percentages, game support lists, unexplained "working" labels, marketing claims.
- Frontend UI implementation; expose a report contract for a future viewer.

## Recommended follow-up, after owner acknowledgement
Use the merged PR #89 workflow and artifacts as the input-format reference; coordinate the remaining runner boundary with PR #90 while it is open. Then add the stdlib-only validator on a fresh branch, test it with synthetic records, and propose an opt-in workflow that consumes (never overstates) the corpus artifacts. Merge only after all nine required CI jobs are green. Never bypass review or protection.
