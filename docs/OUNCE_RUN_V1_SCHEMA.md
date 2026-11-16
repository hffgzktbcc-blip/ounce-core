# `ounce.run.v1` — Machine-Readable Run Report Contract

`ounce.run.v1` is a **data-only** JSON contract emitted by execution engines (`ounce-core`, `ounce-runtime`) when invoked with `--report-json=<path>` (or `--report-json <path>`).

Using an out-of-band report file keeps guest `svcOutputDebugString` output on `stdout` completely separate from engine diagnostics and preserves clean MIT / GPL-3.0-or-later process isolation.

---

## Schema Fields

| Field | Type | Description |
| :--- | :--- | :--- |
| `schema` | `string` | Always `"ounce.run.v1"` |
| `engine` | `string` | `"ounce-core"` or `"ounce-runtime"` |
| `engine_version` | `string` | Semver from `CARGO_PKG_VERSION` |
| `tier` | `string` | `"interpreter"` \| `"tier3"` \| `"tier4"` \| `"nce"` |
| `silent_fallback` | `bool` | `true` if a requested tier (`tier4`/`nce`) fell back to a lower tier |
| `stop_kind` | `string` | Canonical stop reason (see table below) |
| `exit_code` | `integer` | Stable `ounce-hb` exit code (`0`, `10`–`17`) |
| `pc` | `u64 \| null` | Faulting or stopping program counter |
| `opcode` | `u32 \| null` | Raw instruction word for `unimplemented_insn` |
| `fault_addr` | `u64 \| null` | Virtual address for `memory_fault` |
| `svc_id` | `u32 \| null` | Supervisor call number for `unhandled_svc` |
| `break_result` | `u32 \| null` | Horizon `Result` code read from `info1` on `svc_break` |
| `insns_executed` | `u64` | Total guest instructions executed |
| `message` | `string \| null` | Optional human-readable error/halt detail |

---

## Canonical `stop_kind` Mapping

| `stop_kind` | `exit_code` | Meaning |
| :--- | :--- | :--- |
| `clean_exit` | `0` | Guest exited via `svcExitProcess` |
| `window_closed` | `0` | Host GUI window closed cleanly |
| `frame_limit` | `0` | Requested `--frames` / benchmark limit reached |
| `unimplemented_insn` | `10` | Undecoded / unimplemented AArch64 opcode |
| `memory_fault` | `11` | Unmapped read/write, permission violation, or alignment fault |
| `cpu_error` | `12` | Breakpoint (`BRK`), BTI fault, or JIT CPU fault |
| `svc_break` | `13` | Guest called `svcBreak` |
| `unhandled_svc` | `14` | Unhandled Horizon SVC |
| `cpu_halted` | `15` | CPU halted |
| `budget_exhausted` | `16` | Instruction budget exhausted |
| `loader_error` | `17` | Failed to load `.nro` / container |

---

## Companion Determinism Flags

When orchestrating cross-engine parity or escalation, callers pass:
- `--present=headless`
- `--audio=null` (suppresses host audio sink initialization)
- `--deterministic` (fixes non-deterministic clocks/entropy where supported)
