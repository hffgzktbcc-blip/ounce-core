# Diagnose and replay one instruction difference

`fuzz-replay` can retain the first **value mismatch among compared cases** in a
hardware-reference instruction trace. Each trace case is an independent test
from fresh state, not one step in an arbitrary program's execution. Earlier
unimplemented cases, execution errors and host-UNDEFINED observations are still
reported separately; this dossier mode does not locate their first occurrence.

```sh
cargo build --release --locked --features native-jit
ounce-core fuzz-replay --engine tier3 --first-difference NEW-DIRECTORY reference.trace
```

A mismatch exits 1 and creates `diagnosis.json` plus `case.trace`. The directory
must be new and its parent must exist. Matching traces create no dossier. The
record includes the input, reference and actual X/V registers and flags, the
instruction word, trace line number, harness PC, all register differences,
SHA-256 of the replay case and runtime, and a replay command. Values are hex
strings so 64/128-bit registers are not rounded by JSON readers. State comes
from the same execution that produced the mismatch, not an extra rerun.

The wrapper executes the test word followed by SVC #0. Its PC is a harness
address, not an original application's PC. x18 is excluded because macOS reserves
it. Only FPSR.QC is compared; other FPSR flags remain observations. FPCR is zero.
Tier-4's native instruction count includes the wrapper: a nonzero count alone
must not be described as proof that the test word ran natively.

## Re-execute a dossier

```sh
ounce-core replay-instruction NEW-DIRECTORY
ounce-core replay-instruction NEW-DIRECTORY --engine tier4
```

Replay checks the retained bytes against `case_sha256`, checks the schema and
input/reference snapshots against the trace, validates recorded actual state,
and executes the fixed bounded harness. It never executes the stored command
array. Missing, corrupt, oversized or inconsistent inputs and unavailable engines
exit 2. Existing evidence is never modified.

The JSON result has **two verdicts**:

| Replay result | Meaning | Exit |
|---|---|---|
| `reproduced`, reference `mismatch` | The recorded failure happened again | 0 |
| `changed`, reference `match` | Current execution differs from the old failure and agrees with the supplied reference | 1 |
| `changed`, reference `mismatch` | Behavior changed but still differs from the reference | 1 |
| `changed`, reference `not-compared` | The valid retained instruction now faults or is unimplemented | 1 |

Exit 0 means the recorded observation was reproduced, **not** that the emulator
is correct. Hash checks establish correspondence with declared bytes, not the
reference's provenance or independent reproduction. The original runtime hash
and current runtime hash are both retained so replay across builds is explicit.

## Real bounded case: zero times infinity

[The retained case](verify/insn-fuzzer/default-nan-mul2d/acceptance-before.json)
executes `FMUL V0.2D,V1.2D,V2.2D` (`0x6e62dc20`) with zero and positive infinity
in both lanes. An actual Apple Silicon run produced positive default NaNs:
`0x7ff80000000000007ff8000000000000`. The x86-64 Linux implementation produced
negative default NaNs: `0xfff8000000000000fff8000000000000`.

The [hardware trace](verify/insn-fuzzer/default-nan-mul2d/hardware.trace) came from
job [128970464303](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42558815987/job/128970464303).
The generator fixes inputs but captures actual outputs; it never substitutes the
expected NaN. The case matched interpreter/Tier-3/Tier-4 on that ARM64 host.
The [Linux dossier](verify/insn-fuzzer/default-nan-mul2d/before/tier3/diagnosis.json)
retains the real mismatching execution. In the pre-fix Linux run each selected engine reproduced
that failure. Tier-4 ran one native instruction in the wrapper; the decoded vector-FP instruction itself
runs through the shared SIMD interpreter fallback, so this is not a native-JIT vector-FP correctness claim.

```sh
ounce-core replay-instruction docs/verify/insn-fuzzer/default-nan-mul2d/before/tier3
```

The [same Linux dossier was replayed on Windows](verify/insn-fuzzer/default-nan-mul2d/cross-host-before.json), reproducing the mismatch with validated input bytes. macOS correctly reported changed behavior and a matching reference. Those replay steps passed; their containing jobs later failed on an unrelated restored LZ4 evidence directory, which is repaired by placing evidence outside the Rust cache. This is internal cross-host evidence, not outside-user reproduction.
Synthetic MOV/reference contradictions in the CLI tests exercise dossier handling;
they are explicitly separate from this hardware-observed case.

## Repair acceptance

The [local repaired build](verify/insn-fuzzer/default-nan-mul2d/after/acceptance.json)
returns the positive default NaN in the decoded shared SIMD multiply path. The
unchanged trace now matches all three selected engines. Replaying the old dossier
returns `changed` plus reference `match`, preserving the historical failure rather
than replacing it with passing data. The same pre-existing LZ4 ELF still matches
its original 406-byte host output on Tier-3 and Tier-4.

Regression tests cover the actual hardware instruction and shared-helper single/
double precision sign and operand-order cases, input NaN payload preservation,
and FMULX's distinct result. Full local suite: 565 unit tests and 11 CLI tests
passed, with 10 existing opt-in tests ignored. Three-host repair qualification is
pending. The repair does not certify legacy/scalar floating-point paths, other
invalid arithmetic, FPCR modes or FPSR exception flags.
