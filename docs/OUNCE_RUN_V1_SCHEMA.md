# `ounce.run.v1` — Machine-Readable Run Report Contract

`ounce.run.v1` is a **data-only** JSON contract emitted by execution engines (`ounce-core`, `ounce-runtime`) when invoked with `--report-json=<path>` (or `--report-json <path>`). `OUNCE_REPORT_JSON` sets the same path. The flag landed on `main` on 14 Nov 2026 (`#439` in ounce-core, `#11` in ounce-runtime). It ships in ounce-core v0.9.0 and later; it was not in the ounce-core v0.8.2 or ounce-runtime v0.5.0 release archives.

Using an out-of-band report file keeps guest `svcOutputDebugString` output on `stdout` completely separate from engine diagnostics and preserves clean MIT / GPL-3.0-or-later process isolation.

On ounce-core, `run`, `run-nro` and `run-elf` write the file. For `run` / `run-nro`, `silent_fallback` is always `false`, even when a requested Tier-4 build falls back to Tier-3; `run-elf` sets it to `true` in that case. ounce-core and ounce-runtime both write `frames_presented` and `framebuffer_hash`. The two engines do not yet use the same `stop_kind` for every fault.

Every field is a flat scalar or `null`. Nested objects and arrays wait for the recursive JSON parser in `ounce-hb` (roadmap Q2). New fields are additive: consumers must ignore fields they do not know. `src/run_report.rs` tests that the emitted fields equal the table below and that the `exit_code` mapping matches the second table.

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
| `lr` | `u64 \| null` | Link register (`x30`) when the program stopped: the return address of the innermost call, so the `BL` / `BLR` that made it is at `lr - 4`. Set exactly when `pc` is, `null` otherwise. `0` when nothing has made a call yet (on `run-elf` the three small fixtures `exitcode`, `badsyscall` and `hello` report `0`). Additive field (#475) |
| `sp` | `u64 \| null` | Stack pointer when the program stopped. Set exactly when `pc` is, `null` otherwise. Additive field (#475) |
| `opcode` | `u32 \| null` | Raw instruction word for `unimplemented_insn` |
| `fault_addr` | `u64 \| null` | Virtual address for `memory_fault` |
| `svc_id` | `u32 \| null` | Supervisor call number for `unhandled_svc`. `run-elf`: the Linux syscall number (`x8`) of an unimplemented syscall, or the immediate of a non-zero `SVC #imm` |
| `sm_service` | `string \| null` | Last requested `sm` service name (or unimplemented HIPC interface name) when stopping on `unhandled_svc` (exit 14); `null` when no service was requested or for other stops (`run-elf` always writes `null`). Additive field (#475) |
| `cmif_cmd_id` | `u32 \| null` | Last dispatched CMIF command ID (or unimplemented HIPC command ID) when stopping on `unhandled_svc` (exit 14); `null` when no CMIF request ran or for other stops (`run-elf` always writes `null`). Additive field (#475) |
| `break_result` | `u32 \| null` | Horizon `Result` code read from `info1` on `svc_break` |
| `insns_executed` | `u64` | Total guest instructions executed. An instruction that faults did not complete and is not counted, but the ones before it in the same run are: a fault at the fourth instruction reports 3 (before this was fixed, `run` / `run-nro` reported only the instructions of earlier slices, often 0). With `--deterministic` it is the same on every run, host and CPU tier |
| `frames_presented` | `u64` | Frames the run loop presented (`0` when the run had no display or the guest never presented). `run` / `run-nro` have a display only with a window (`--gui`) or `--present=headless`; use the latter for comparisons. Additive field |
| `framebuffer_hash` | `string \| null` | `fnv1a64:<16 lowercase hex>`: FNV-1a 64 over the RGBA bytes of the last presented guest frame, **before** the telemetry HUD, alpha forced to `0xFF`. Width and height are not mixed in. `null` if no frame was presented, or if the host GPU drew the last frame's 3D draws. That happens only in a `--features gpu-present` build with a GPU window open, outside deterministic mode; guest memory never holds those frames. `--present=headless` and `--deterministic` / `OUNCE_DETERMINISTIC=1` draw 3D with the software rasterizer, so every frame they present has a hash, and the two give the same hash. Additive field |
| `audio_frames_mixed` | `integer` | Stereo frames (one left and one right 16-bit sample) of audio the guest mixed through `audout` or `audren`. Counted as the audio is mixed, so `--present=headless`, which opens no sound device, reports the same number as a run that plays it. `0` when the guest mixed none; `run-elf` always writes `0`. Additive field (#478) |
| `audio_hash` | `string \| null` | `fnv1a64:<16 lowercase hex>`: FNV-1a 64 over the little-endian bytes of every mixed 16-bit sample, in order. When a guest mixes through both `audren` and `audout`, the value is FNV-1a 64 over the two streams' digests (audren first, 8 little-endian bytes each). `null` when `audio_frames_mixed` is `0`. The same on every run, host and CPU tier for a fixed program with `--deterministic`. A hash says the audio is the same, not that it is correct. Additive field (#478) |
| `guest_exit_status` | `i32 \| null` | Status the guest passed to `exit` / `exit_group` (low 8 bits). `run-elf` only; `null` for homebrew, which has no exit status, and for any stop other than `clean_exit`. `exit_code` says how the engine stopped (`0` for every clean exit); this field is the program's own pass/fail. Additive field |
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
- `--deterministic` or `OUNCE_DETERMINISTIC=1` / `true`. On `run`, `run-nro` and `run-elf` the guest runs on a fixed clock: guest time advances per system call (20 µs) and per scheduler time slice of 20,000 instructions (1 ms), not with the host clock, and the wall-clock date is fixed, so `insns_executed` is the same on every run and host (#479). On homebrew `run` / `run-nro` it also makes 3D draws use the software rasterizer, even with a GPU window, so `framebuffer_hash` is never `null` for a presented frame.
