# CodeQL cleartext-logging alert review — 21 November 2026

## Decision

The ten open `rust/cleartext-logging` alerts on commit `44994c2` are false
positives. They may be dismissed with the reason `false positive` and the
alert-specific evidence below.

The shared CodeQL source is the offline account model in
[`src/kernel/account.rs`](../../src/kernel/account.rs). Its identity is the
constant runtime-local `LOCAL_USER`; it contains no Nintendo account, host
account, network identity, password, token or other credential. CodeQL follows
the account handler's generic numeric IPC return path into unrelated callers
and then treats numeric emulator/test diagnostics as if they contained the
account payload. The reviewed sinks do not print the account response buffers.

## Reviewed alerts

| Alerts | Sink | What is printed | Classification |
| --- | --- | --- | --- |
| 33, 38 | `src/cli/run.rs`, `src/cli/vm.rs` | Guest-controlled `svcBreak` reason/info words and decoded result code, formatted as bounded hexadecimal/integer values | False positive: no account payload or host identity reaches the sink |
| 34 | `src/cli/demos.rs` | Firmware-version integers and a generic IPC result from the built-in synthetic test suite | False positive: deterministic test data only |
| 35 | `src/cli/demos.rs` | Loaded NRO base address and generic IPC result from the built-in synthetic test suite | False positive: deterministic test data only |
| 36, 37 | `src/cli/vm.rs` | Built-in stage number, expected guest `X0`, stop kind and actual guest `X0` | False positive: guest/test state only |
| 39 | `src/cli/vm.rs` | Aggregate RomFS-byte and audio-buffer/sample counters | False positive: counters only |
| 40 | `src/cli/vm.rs` | Aggregate upscaler-frame/output-pixel and SaveData-byte counters | False positive: counters only |
| 41 | `src/cli/vm.rs` | Final guest `X0` and `D0` register values | False positive: guest register state, not a host/account credential |
| 42 | `src/kernel/ipc.rs` | Opt-in IPC trace number, synthetic handle, service enum and command ID | False positive: the trace omits request/response payloads and account identity data |

## Boundaries retained

- This review does **not** waive future `rust/cleartext-logging` alerts.
- Guest debug strings and filesystem paths are outside these ten alert paths and
  require their own review if CodeQL reports them.
- A future change that puts real host identity or credentials into the account
  model invalidates this classification.
- Dependabot and secret scanning had zero open alerts at review time.

The review used the CodeQL alert API's source-to-sink messages plus the exact
sink locations on `44994c2`; it does not infer that a green scan proves the
runtime sandbox is secure.
