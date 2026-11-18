# `ounce.trace.v1`: Bounded Step Trace and First-Divergence Summary

`ounce.trace.v1` is a **data-only** JSON contract for finding the first instruction where two engines (or two runs) stop agreeing. It is implemented independently by `ounce-core` (MIT) and `ounce-runtime` (GPL-3.0-or-later); the two share this document and no code. It complements [`ounce.run.v1`](OUNCE_RUN_V1_SCHEMA.md), which records how a run ended, not what it did on the way.

There are two document kinds, both `"schema": "ounce.trace.v1"`:

1. `kind: "trace"`: written by an engine. The first N retired guest instructions: pc, opcode and the registers each one changed.
2. `kind: "divergence"`: written by a comparer (`ounce-core trace-diff`, later `ounce-hb diff-trace`). The first step at which an *expected* trace and an *actual* trace differ, plus a small window of the steps before it.

Status: ounce-core writes traces and compares them. ounce-runtime and `ounce-hb diff-trace` do not implement it yet (roadmap Q2 2027, issue #477). Nothing here has been checked against another implementation.

---

## Producing a trace (engine side)

- `OUNCE_TRACE_SUMMARY=<path>` makes `ounce-core run` / `run-nro` write one `kind: "trace"` document to `<path>` when the run ends. An engine must write it on every stop, including faults.
- `OUNCE_TRACE_STEPS=<n>` sets the bound `step_limit` (default 1024, at most 65536; larger values are clamped, invalid values fall back to the default).
- A **step** is one guest instruction that retired. An instruction that faulted before retiring is not a step. An `svc` instruction is a step; its recorded registers are those after the instruction itself and before the host handles the call, so what the host's service handler writes into registers does not appear in that step. The next step shows the guest's view after the call. (Handler effects therefore surface as register differences at the first instruction that reads them, which is the point at which engines disagree observably.)
- Step index `i` counts from `0` in execution order from the start of the run (the first guest instruction at the entry point).
- `regs` lists only registers whose value after the step **differs** from before it. A write of the same value is not listed.
- The recorder stops storing at `step_limit` but keeps counting. `truncated` is `true` iff more steps retired than were stored (or recording could not continue); a trace that ends exactly at the limit is not truncated.
- ounce-core runs one instruction at a time while recording, on every engine. Multi-instruction blocks are therefore not what is traced.
- ounce-core does not record memory writes (`mem`) yet. The field is optional so another engine may.

## Number formats

All 64-bit and wider values are strings, because JSON numbers lose precision above 2^53 in common parsers: lowercase hex with a `0x` prefix. Writers use fixed width (16 digits for `pc`, `x*`, `sp`, `addr`; 8 for `opcode`, `nzcv`, `fpsr`, `fpcr`; 32 for `v*`; `2*size` for memory `value`). Readers must parse the number and compare numerically, not textually. Counts, `i`, `step` and `size` are plain JSON integers.

## Register names and comparison order

`x0`..`x30`, `sp`, `nzcv`, `fpsr`, `fpcr`, `v0`..`v31` (the 128-bit vector registers). This is also the order in which a comparer looks for the first differing register. `nzcv` is the PSTATE flags word as a 32-bit value (N,Z,C,V in bits 31..28). `pc` is not a register entry: it is the step's `pc`, the address of the instruction before it executes.

---

## Trace document

| Field | Type | Description |
| :--- | :--- | :--- |
| `schema` | `string` | Always `"ounce.trace.v1"` |
| `kind` | `string` | Always `"trace"` |
| `engine` | `string` | `"ounce-core"` or `"ounce-runtime"` |
| `engine_version` | `string` | Semver of the writing engine |
| `step_limit` | `u64` | The bound used: `1..=65536` |
| `steps_executed` | `u64` | Steps that retired in the run, including those past the bound |
| `steps_recorded` | `u64` | Length of `steps`; at most `step_limit`; at most `steps_executed` |
| `truncated` | `bool` | More steps retired than were stored, or recording stopped early |
| `steps` | `array` | One object per recorded step, in order |

## Step

| Field | Type | Description |
| :--- | :--- | :--- |
| `i` | `u64` | Zero-based step index; equals the position in `steps` |
| `pc` | `hex u64` | Address of the instruction before it ran |
| `opcode` | `hex u32` | The instruction word |
| `regs` | `object` | Register name to new value (hex) for each register that changed |
| `mem` | `array` (optional) | Stores performed by the step, in order. Absent means "not recorded", which is different from `[]` ("recorded: none") |

## Memory write

| Field | Type | Description |
| :--- | :--- | :--- |
| `addr` | `hex u64` | Guest virtual address |
| `size` | `u8` | Bytes stored, `1..=16` |
| `value` | `hex` | The stored bytes as a little-endian integer, `2*size` digits |

---

## Comparing (`kind: "divergence"`)

`ounce-core trace-diff <expected.json> <actual.json> [--window N] [--json out.json]` prints one line and, with `--json`, writes the summary. Exit status: `0` no divergence in the compared steps, `1` divergence, `2` bad input (unreadable, not JSON, wrong schema or kind, inconsistent counts, unknown register, bad hex, `steps[i].i` not equal to `i`).

Steps are compared in index order over the shorter trace. At each step the first difference found wins, checked in this order:

1. `pc`
2. `opcode`
3. `register`: the first register in the canonical order whose entry differs; a register that one side changed and the other did not has a `null` value on the other side
4. `memory`: only when **both** steps have `mem`; the first index whose write differs (`name` is `mem[k]`; the missing side is `null`)

If the compared prefix is identical but the traces have different lengths:

- the shorter one is **not** truncated (its run ended): `field` is `length`, `step` is the shorter length, `expected` / `actual` are the two `steps_recorded` counts as decimal strings;
- the shorter one is truncated (it hit its bound): not a divergence. `match` stays `true` and `truncated` is `true`, because the result covers only `compared_steps`.

`truncated` in the summary is `true` if either input was truncated. A `match: true` with `truncated: true` means "no difference in the first `compared_steps` steps", not "the runs agree".

The `window` holds the up-to-`--window` steps immediately before the divergent step (default 4, at most 16; fewer near the start), taken from the expected trace; by construction both sides agree on them.

## Divergence summary

| Field | Type | Description |
| :--- | :--- | :--- |
| `schema` | `string` | Always `"ounce.trace.v1"` |
| `kind` | `string` | Always `"divergence"` |
| `engine` | `string` | The comparer: `"ounce-core"` or `"ounce-runtime"` |
| `engine_version` | `string` | Semver of the comparer |
| `expected_engine` | `string` | `engine` of the expected trace |
| `actual_engine` | `string` | `engine` of the actual trace |
| `match` | `bool` | `true` if no divergence was found in the compared steps |
| `compared_steps` | `u64` | Steps compared, counting the divergent step |
| `truncated` | `bool` | Either input was truncated (see above) |
| `step` | `u64 \| null` | Index of the first divergent step; `null` on match |
| `field` | `string \| null` | `pc`, `opcode`, `register`, `memory` or `length`; `null` on match |
| `name` | `string \| null` | Register name or `mem[k]` for those fields; otherwise `null` |
| `expected` | `string \| null` | Expected value (hex; decimal count for `length`; `mem` writes as `addr/size=value`); `null` if absent or on match |
| `actual` | `string \| null` | Actual value, same format |
| `pc_expected` | `hex u64 \| null` | `pc` of the expected step at `step`, if it exists |
| `pc_actual` | `hex u64 \| null` | `pc` of the actual step at `step`, if it exists |
| `opcode_expected` | `hex u32 \| null` | Opcode of the expected step at `step`, if it exists |
| `opcode_actual` | `hex u32 \| null` | Opcode of the actual step at `step`, if it exists |
| `window` | `array` | Up to 16 steps before `step`; empty on match |

## Window entry

| Field | Type | Description |
| :--- | :--- | :--- |
| `i` | `u64` | Step index |
| `pc` | `hex u64` | Address of the instruction |
| `opcode` | `hex u32` | The instruction word |

---

## Bounds

A trace holds at most 65536 steps. A divergence summary holds at most 16 window entries plus fixed fields. Neither grows with run length.

## What this does not show

- It compares what the two engines recorded, not what real hardware does. For hardware replay use [`ounce-core diverge`](DIVERGE.md).
- Only the first 1024 instructions are recorded by default. A match says nothing about later steps.
- ounce-core records registers only: no memory, no system registers other than `nzcv`/`fpsr`/`fpcr`, no host-side effects of SVC handlers.
- Instruction sampling is not used here; this is a contiguous prefix of the run, unlike the sampled `OUNCE_TRACE` hardware-check stream.
- Both traces must come from the same program, arguments and starting state. Differences in clocks, ASLR-like layout or host-provided values show up as divergences.
