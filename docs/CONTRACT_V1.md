# v1.0 contract: what callers can depend on (DRAFT)

> **DRAFT. Nothing in this file is frozen.** Issue #481 asks for the v1.0 freeze of `ounce.run.v1`,
> `ounce.repro.v1`, `ounce.trace.v1`, the exit codes and the DX flags. The freeze is an **owner decision**. This file
> and `tests/contract_v1.rs` are the material for that decision: the initial snapshot at commit
> `9cac0d4`, updated for the v0.10.0 release, plus compatibility tests that would enforce it. Until the owner records a decision (see
> [Owner questions](#owner-questions)), the schema docs and the code stay free to change.

> **Archived sibling context:** `ounce-hb` and `ounce-runtime` are archived. References below describe the roles those
> repositories had when this draft was written; they are not current owners, consumers, or active implementations.

Every statement below was checked against the code or by running the binary; the last column of each table names the
test that pins it. Where the issue text and the code disagree, this file says what the code does and flags it.

## The additive-only rule (proposed)

Once a contract item is frozen:

1. **Allowed:** adding a new key to a JSON document; adding a new `stop_kind`; adding a new flag or command; adding a
   new value for an enumerated string that consumers are told to treat as open (`tier`, `stop_kind`).
2. **Not allowed:** removing or renaming a key; changing a key's JSON type (including making a non-null key nullable
   or the reverse); changing the meaning of an existing value (an `exit_code` for a `stop_kind`, the unit of a
   number, the encoding of a hash); changing the process exit status for an existing situation; removing a flag or
   changing what an accepted value means.
3. **Consumers** must ignore keys they do not know (already stated in
   [OUNCE_RUN_V1_SCHEMA.md](OUNCE_RUN_V1_SCHEMA.md)).
4. **How an addition is made on purpose:** change the code, the schema doc and the baseline file in
   `tests/contract/` in one commit. The tests fail on an addition that skips the baseline (the same way they fail on
   a removal), so no key slips in unnoticed.
5. A breaking change needs a new schema name (`ounce.run.v2`), not an edit.

## Findings that differ from the issue text

| Issue says | What the code does | Consequence |
|---|---|---|
| Exit codes `0`, `2-5`, `10-17` | ounce-core's own process statuses are `0`, `1`, `2` (plus `70`-`73` on `run-elf`, and the guest's status). It never exits `3`, `4`, `5` or `10`-`17`. `10`-`17` are the value of the `exit_code` **field** in the report. The historical family roadmap assigned `2..5` infrastructure exits to archived `ounce-hb`. | `2-5` are not implemented in this repo and the repository once assigned them is archived, so they are not verifiable or testable here. See [Exit codes](#exit-codes). |
| Flag `--timeout` | Not an option of `run` / `run-nro` / `run-elf` (`run` refuses it with status 2). Only `ounce-core ui --timeout SECS` exists. The historical roadmap put it on archived `ounce-hb run`. | Pinned as "not a run flag here". |
| Flag `--json` | Not an option of `run` / `run-nro` / `run-elf` (refused with status 2). `--json <path>` exists on `diverge` and `trace-diff`; `census --json` is a boolean switch. The run report flag is `--report-json=<path>` (or `OUNCE_REPORT_JSON`). | Owner decides whether `--report-json` joins the frozen flags. |
| `ounce.repro.v1` | Ounce-Core now keeps ten metadata-only manifests under `tests/repro/` and checks them in `tests/repro_corpus.rs`; the historical plan assigned the full repro-bundle/consumer contract to now-archived `ounce-hb`. | The local corpus tests do not freeze the historical cross-project schema. |

## `ounce.run.v1`

Schema doc: [OUNCE_RUN_V1_SCHEMA.md](OUNCE_RUN_V1_SCHEMA.md). Written by `run`, `run-nro` and `run-elf` with
`--report-json=<path>`. Pinned by `run_v1_keys_and_json_types_are_pinned` against
[`tests/contract/run-v1-keys.json`](../tests/contract/run-v1-keys.json): the key set is exact, each value has one of
the listed types, and each listed type occurs in at least one of 18 representative runs (11 `run-nro`, 7 `run-elf`).

| Key | JSON type | First released |
|---|---|---|
| `schema`, `engine`, `engine_version`, `tier`, `stop_kind`, `message` | string (`message`: string or null) | v0.9.0 |
| `silent_fallback` | bool | v0.9.0 |
| `exit_code`, `insns_executed` | number | v0.9.0 |
| `pc`, `opcode`, `fault_addr`, `svc_id`, `break_result` | number or null | v0.9.0 |
| `frames_presented` | number | v0.10.0 (#460) |
| `framebuffer_hash` | string `fnv1a64:<16 hex>` or null | v0.10.0 (#460, #486) |
| `lr`, `sp` | number or null (set exactly when `pc` is) | v0.10.0 (#475, #513) |
| `fault_symbol` | string or null (resolved ELF `.symtab` symbol at `pc` on faults) | **unreleased** (#475) |
| `backtrace` | string or null (bounded `X29`/`LR` frame chain on faults/`svc_break`) | **unreleased** (#475) |
| `sm_service` | string or null (`unhandled_svc` / exit 14 after `sm:` / HIPC activity) | v0.10.0 (#475, #513) |
| `cmif_cmd_id` | number or null (`unhandled_svc` / exit 14 after CMIF dispatch) | v0.10.0 (#475, #513) |
| `audio_frames_mixed` | number | v0.10.0 (#478) |
| `audio_hash` | string `fnv1a64:<16 hex>` or null | v0.10.0 (#478) |
| `guest_exit_status` | number or null (`run-elf` clean exit only) | v0.10.0 (#473) |

v0.9.0 (`d3380a0`) had 14 keys; nine additional fields shipped in v0.10.0 (`ae13734`) and two (`fault_symbol`, `backtrace`) were added afterwards on `main`. Shipping those fields does **not** freeze this proposed v1.0 contract. Changes to published fields should be deliberate, documented and tested; see the owner decisions below. `tier` is `interpreter`, `tier3`, `tier4` or `nce`
(`CpuTier::report_label`); only `interpreter` and `tier3` are exercised by the tests, because `tier4` and `nce` need
cargo features. Details pinned beyond the key set (`run_v1_stop_details_follow_the_stop`): a fault's `fault_addr`
or `opcode`, `break_result` read from `info1` when `info2` is 4 (else null), `guest_exit_status` 42 with `exit_code` 0
for a guest that calls `exit(42)`, `svc_id` for an unknown syscall, the `fnv1a64:` hash shape.

Known looseness the owner may want fixed before freezing: for `run` / `run-nro` a load failure writes **no report**
at all (`run_nro_load_failure_writes_no_report` pins today's behaviour), so `loader_error` exists only on `run-elf`.

## Exit codes

### The report's `exit_code` field (`0`, `10`-`17`)

Mapping from `stop_kind` (`run_report::exit_code_for`). Pinned by `run_v1_exit_code_follows_stop_kind`, which
provokes each reachable `stop_kind` with a real program and checks the pair against the baseline.

| `exit_code` | `stop_kind` | Provoked by (test program) |
|---|---|---|
| 0 | `clean_exit` | NRO `mov w0,#0; svc #7`; `exitcode.elf` |
| 0 | `frame_limit` | NRO that presents forever with `--max-frames 2` |
| 0 | `window_closed` | **not reachable in tests** (needs `--gui` and a window) |
| 10 | `unimplemented_insn` | `udf #0` (NRO); `rmif x0, #0, #1`, a valid encoding not implemented yet (ELF) |
| 11 | `memory_fault` | load from unmapped `0x10` (NRO and ELF) |
| 12 | `cpu_error` | `brk #0`; `udf #0` in an ELF (the guest dies of SIGILL, process status 132) |
| 13 | `svc_break` | `svc #0x26` |
| 14 | `unhandled_svc` | `svc #0xff` (NRO); `badsyscall.elf` (Linux syscall 278) |
| 15 | `cpu_halted` | **not reachable**: nothing in the tree constructs `StepResult::Halt`, so the code is dead today |
| 16 | `budget_exhausted` | `b .` with `OUNCE_NRO_BUDGET=5000` (NRO); `--max-insns=1000` (ELF) |
| 17 | `loader_error` | `run-elf` on a file that is not an ELF (no report from `run-nro`, see above) |

Unlisted or unclassified faults map to `12`.

### Process exit status of the `ounce-core` binary

Pinned by `run_and_run_nro_process_exit_statuses`, the `*_usage_*` tests and `trace_diff_exit_statuses_are_0_1_2`.

| Command | Status | Meaning |
|---|---|---|
| `run`, `run-nro` | 0 | `clean_exit`, `frame_limit` (or `window_closed`) |
| | 1 | any other stop, a loader failure, a missing input file |
| | 2 | usage error: unknown option, bad `--max-frames`, no path, unknown command, guest args for a non-NRO |
| `run-elf` | guest status & 0xff | the program's own `exit`/`exit_group` status (42 for `exitcode.elf`) |
| | 70 / 71 / 72 / 73 | unimplemented syscall / instruction or memory fault / cannot load ELF / instruction budget |
| | 2 | usage error |
| `trace-diff` | 0 / 1 / 2 | no divergence / divergence / bad input |
| `diverge` | 0 / 1 / 2 | no disagreement / disagreement / could not run (only the status-2 paths are tested: the rest need a C compiler and the host CPU) |

Not tested: `run-elf` guest status that collides with 70-73 is indistinguishable from an engine stop (a property of
the design; the report's `stop_kind` tells them apart); `census`, `fuzz-replay`, `replay-instruction` statuses.

### `2`-`5`

Not produced by ounce-core as distinct codes (`2` is its usage-error status; `3`-`5` do not occur). If these are the
infrastructure exits historically assigned to `ounce-hb`, they cannot now be frozen and tested in that archived repository.

## `ounce.trace.v1`

Schema doc: [OUNCE_TRACE_V1_SCHEMA.md](OUNCE_TRACE_V1_SCHEMA.md). Status: **released in ounce-core v0.10.0** (#477), but not frozen across projects; no second implementation or consumer has checked it. Pinned by
`trace_v1_keys_and_json_types_are_pinned` against
[`tests/contract/trace-v1-keys.json`](../tests/contract/trace-v1-keys.json): exact keys and types for the trace
document, a step, the divergence summary (match, opcode divergence and register divergence documents) and a window
entry. `trace_v1_numbers_are_hex_strings_and_counts_are_integers` pins the number formats (`pc` 16 hex digits,
`opcode` 8, counts as integers). The optional `mem` step field is not recorded by ounce-core and is **not** pinned.
Recording uses `OUNCE_TRACE_SUMMARY` / `OUNCE_TRACE_STEPS`, and `trace-diff <expected> <actual> [--window N]
[--json out]`; the env variable names are not covered by any test beyond `OUNCE_TRACE_SUMMARY` being used to produce
the traces.

## CLI flags

| Flag | Where it exists | What is pinned (test) |
|---|---|---|
| `--max-frames N` / `--max-frames=N` | `run`, `run-nro` (global option) | both spellings stop after exactly N presented frames with status 0 and `stop_kind` `frame_limit`; `0`, `-1`, `abc` and empty are status 2 with "expects a positive integer" (`max_frames_*`) |
| `--sdmc=<dir>` | `run`, `run-nro` | the guest sees `host.txt` in the folder; the folder is unchanged without `--sdmc-writeback`; bare `--sdmc` is status 2 (`sdmc_directory_*`). Observed by hand, not pinned: a nonexistent directory is accepted silently (status 0) |
| `--timeout` | `ui` / `lab` only (`--timeout SECS`) | missing or non-numeric value is status 2 (`ui_timeout_*`); **not** accepted by `run`, `run-nro`, `run-elf` (`timeout_and_json_are_not_run_flags_on_ounce_core`); after `--` it is a guest argument |
| `--json <path>` | `diverge`, `trace-diff` (`census --json` is a switch, untested) | `diverge --json` without a path is status 2; `trace-diff --json out` writes the summary (`trace_v1_*`); **not** accepted by `run`, `run-nro`, `run-elf` |

`--report-json=<path>` (also `--report-json <path>` and `OUNCE_REPORT_JSON`) is used by every test above; its three
spellings are not individually pinned. `--present`, `--cpu`, `--deterministic`, `--press`, `--max-insns` are outside
the flags the issue names and are not frozen by this draft.

## What the tests do and do not show

- 16 integration tests in `tests/contract_v1.rs`, about two seconds, no new dependencies, no host window, no sound
  device, no C compiler. Programs are 2-6 instruction NROs/ELFs assembled in the test plus the in-repo fixtures
  `audren.nro`, `sdmc.nro`, `exitcode.elf`, `badsyscall.elf`. Hand-assembled encodings: `mov w0,#0` `0x52800000`,
  `svc #7` `0xD40000E1`, `svc #0x26` `0xD40004C1`, `svc #0xff` `0xD4001FE1`, `svc #0x18` `0xD4000301`, `brk #0`
  `0xD4200000`, `udf #0` `0x00000000`.
- They detect regressions in **this** binary. They do not show that another active consumer parses these documents, and
  they say nothing about guest correctness. The named historical consumers `ounce-hb` and `ounce-runtime` are archived;
  `deckstream` is an independent project.
- Red check, done once while writing them and reverted: renaming `lr`, changing `memory_fault` to 21, changing the
  `run-elf` fault status 71 to 75, turning `pc` into a string and adding an unlisted key each fail a named test.
- Not reachable or not tested: `window_closed`, `cpu_halted`, `tier4` / `nce` reports, `--gui`, historical `ounce-hb` statuses
  `2-5`, `ounce.repro.v1`, the `diverge` hardware-replay statuses.

## Owner questions

1. **Scope of "exit codes `2-5`".** Were these the infrastructure exits assigned to archived `ounce-hb`? If so, no
   active repository currently owns their freeze; ounce-core's own statuses (`0`, `1`, `2`, `70`-`73`) are not in the issue's list. Should they be
   frozen too, or marked implementation detail?
2. **`--timeout` and `--json` were assigned to archived `ounce-hb`.** Should the frozen set for ounce-core be
   `--max-frames`, `--sdmc` and `--report-json` instead, leaving those historical flags outside this contract?
3. **Which of the nine fields first shipped in v0.10.0 should join the frozen v1.0 contract?** (`frames_presented`, `framebuffer_hash`, `lr`, `sp`,
   `sm_service`, `cmif_cmd_id`, `audio_frames_mixed`, `audio_hash`, `guest_exit_status`.) They are published but not yet frozen as a cross-project compatibility contract. Specific doubts: `framebuffer_hash`
   excludes width and height and is `null` for a `gpu-present` window outside deterministic mode; `lr` / `sp` mean
   "at the stop"; `guest_exit_status` is `run-elf` only.
4. **`run` / `run-nro` load failures write no report.** Add a `loader_error` report (additive, status stays 1) before
   freezing, or freeze today's behaviour?
5. **`cpu_halted` / `15` is dead code.** Freeze a value nothing can produce, or drop it from the table?
6. **`ounce.repro.v1`.** No definition exists in this repository. With its former owner `ounce-hb` archived, should a
   copy of its contract doc and key baseline be added here, or is it frozen only there?
7. **`ounce.trace.v1`** has one active implementation and no active consumer. Freeze before an active second
   implementation has been checked against it? The formerly named candidates (`ounce-runtime`, `ounce-hb diff-trace`) are archived.
8. **Status 70-73 on `run-elf`** share the number space with guest statuses. Keep as is and freeze, or move engine
   stops to a distinct channel (the report already has one)?
9. **Open enumerations.** Confirm `tier` and `stop_kind` are open sets (additions allowed), as the additive-only rule
   above assumes.
10. **Versioning.** Does the freeze coincide with a release tag (v1.0.0), and is the `schema` string the only version
    marker, or should `engine_version` ranges be stated?

## Proposed answers (not yet accepted)

These are the maintainers' working picks for the ten questions above, written so the owner can accept, change or
reject them in one pass. **Nothing here is frozen.** The freeze happens only when the owner records a decision.

| # | Proposed answer |
|---|---|
| 1 | Freeze `0`, `1` and `2` with the meanings the per-command table above gives (for `run` / `run-nro`: success, failure, usage error). `run-elf` is different: it exits with the program's own status, so `2` there is either a usage error or the program's own status (the number alone cannot tell them apart), and `diverge` uses `2` for "could not run". Statuses `70`-`73` are an implementation detail and may change. The historical proposal put `2`-`5` in now-archived `ounce-hb`, not here. |
| 2 | Freeze `--max-frames`, `--sdmc` and `--report-json` for ounce-core. The historical proposal assigned `--timeout` and `--json` to now-archived `ounce-hb`; they need a new active owner before a freeze. |
| 3 | Freeze `frames_presented`, `audio_frames_mixed`, `audio_hash` and `guest_exit_status` now. Wait on `framebuffer_hash` (it ignores width and height and is `null` only when no frame was presented, or when a `gpu-present` build draws the frame on the host GPU with a window open outside deterministic mode), `lr` and `sp` (they only mean "at the stop") until real use settles them. |
| 4 | Add a `loader_error` report for `run` / `run-nro` load failures (additive; the process status stays `1`), then freeze. |
| 5 | Drop `cpu_halted` / `15` from the table: nothing can produce it. |
| 6 | The historical proposal assigned `ounce.repro.v1` to now-archived `ounce-hb`; it needs a new active owner before a freeze. |
| 7 | Do not freeze `ounce.trace.v1` until an active second implementation has been checked against it; the named candidates, `ounce-runtime` and `ounce-hb diff-trace`, are archived. |
| 8 | Keep statuses `70`-`73` as they are, unfrozen (see question 1). Engine stops are already reported in the report's `stop_kind`. |
| 9 | Yes: `tier` and `stop_kind` are open sets, so additions are allowed. |
| 10 | The freeze coincides with the `v1.0.0` tag. The `schema` string is the version marker. |

**Historical sibling check (2026-11-19).** Before it was archived, `ounce-runtime` wrote `lr`, `sp` and `guest_exit_status` (its PR #25) and
`audio_frames_mixed` and `audio_hash` (its PR #26). Running both tools on the 11 shared programs, every program agrees on
the five agreed keys (`schema`, `stop_kind`, `exit_code`, `frames_presented`, `framebuffer_hash`) and no advisory key is missing from either tool. `insns_executed` still differs: ounce-core counts 15
more on 10 programs and 1 fewer on `threads`. Until that is explained it stays advisory, and nothing here is frozen.
