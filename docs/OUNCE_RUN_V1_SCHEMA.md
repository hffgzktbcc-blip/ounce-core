# `ounce.run.v1` — Machine-Readable Run Report Contract

`ounce.run.v1` is a **data-only** JSON contract emitted by execution engines (`ounce-core`, `ounce-runtime`) when invoked with `--report-json=<path>` (or `--report-json <path>`). `OUNCE_REPORT_JSON` sets the same path. The flag landed on `main` on 14 Nov 2026 (`#439` in ounce-core, `#11` in ounce-runtime). It ships in ounce-core v0.9.0 and later; it was not in the ounce-core v0.8.2 or ounce-runtime v0.5.0 release archives.

Using an out-of-band report file keeps guest `svcOutputDebugString` output on `stdout` completely separate from engine diagnostics and preserves clean MIT / GPL-3.0-or-later process isolation.

On ounce-core today, `run` and `run-nro` write the file. `run-elf` does not. `silent_fallback` is written and is always `false`, including when a requested Tier-4 build falls back to Tier-3. `frames_presented` and `framebuffer_hash` are not ounce-core fields. ounce-runtime's report adds those two fields. The two engines do not yet use the same `stop_kind` for every fault.

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
| `frames_presented` | `u64` | Frames the run loop presented (`0` when the run had no display or the guest never presented). `run` / `run-nro` have a display only with a window (`--gui`) or `--present=headless`; use the latter for comparisons. Additive field |
| `framebuffer_hash` | `string \| null` | `fnv1a64:<16 lowercase hex>`: FNV-1a 64 over the RGBA bytes of the last presented guest frame, **before** the telemetry HUD, alpha forced to `0xFF`. Width and height are not mixed in. `null` if no frame was presented or the frame was drawn on the host GPU. Additive field |
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
- `--present=headless` (no window and no host audio device)
- `--audio=null` or `OUNCE_AUDIO=null` (no host audio device, including when a window is open)
- `--deterministic` or `OUNCE_DETERMINISTIC=1` / `true`. On `run-elf` this uses a fixed guest clock. On homebrew `run` / `run-nro` the flag is accepted and the kernel clock is unchanged unless the caller sets that clock itself (the corpus runner does).
