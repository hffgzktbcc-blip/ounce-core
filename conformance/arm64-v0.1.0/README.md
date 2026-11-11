# ARM64 conformance seed 0.1.0

This MIT-licensed seed contains one unchanged Apple Silicon hardware trace. It
exercises FMUL V0.2D,V1.2D,V2.2D with zero and infinity. It found a real host-sign
dependency in Ounce Core. It is a reusable regression input, not broad instruction
coverage, an independent result or a correctness certificate.

The manifest pins the exact case and hardware capture. Preserve version 0.1.0's
bytes; additions or reference changes require a new version. The containing
hardware job passed; its containing workflow was later superseded and cancelled.
Project evidence lives in `docs/verify/insn-fuzzer/default-nan-mul2d/`.

## Run with Ounce Core

From the repository root, after building the runtime:

```sh
python3 scripts/run_conformance.py --report /tmp/NEW-conformance-report.json -- \
  target/release/ounce-core fuzz-replay --engine interp
```

The runner appends an absolute trace path to this argument vector, without a
shell. Reports must be new files. It verifies the pinned trace before execution
and requires one executed and matched instruction, with no unsupported cases,
errors, mismatches or host-undefined cases. Exit 0 alone is insufficient: the
runtime's exploratory fuzzer permits unsupported instructions without failing.
The runner exits 0 for all cases passing, 1 for an observed nonpass, 2 for invalid
suite/input or invocation. Retain the JSON report with exact emulator revision,
build options and host details when reporting a result.

## Adapter contract for another emulator

An adapter accepts the trace path as its final argument. Run the actual emulator
on each input and compare the observed output against the trace; do not echo the
reference as execution. Emit these decimal-count lines on stdout, exactly once:

```text
host executed (not SIGILL): 1
host UNDEFINED (SIGILL): 0
emulator unimplemented: 0
emulator other errors: 0
emulator executed: 1
exact match (X,V,NZCV,QC): 1
MISMATCH: 0
emulator runs a word the host calls UNDEFINED: 0
```

Counts must describe actual observations; unsupported instructions are nonpasses.
Additional diagnostic lines are allowed. Exit nonzero for mismatches/errors.
The runner checks this adapter-reported result; it cannot prove that an adapter
honestly executed an emulator. Publish the adapter source and emulator identity.

Each trace line is whitespace-separated hexadecimal, without `0x` prefixes:

```text
T WORD NZCV_IN X0_IN ... X30_IN V0_IN ... V31_IN => NZCV_OUT FPSR_OUT X0_OUT ... X30_OUT V0_OUT ... V31_OUT
```

X fields are 64-bit integers, V fields are raw 128-bit integers, and NZCV is a
four-bit value. Initialize FPCR/FPSR to zero. Execute the single word, then compare
X0–X30 except X18, every V register, NZCV and FPSR.QC (bit 27). Ignore other FPSR
flags; hardware recorded IOC for this case, which Ounce Core does not model.
PC, SP, memory, timing and full FPCR behavior are outside this seed's comparison.
The original harness places the word at 0x1800 followed by an SVC stop sentinel;
that wrapper is not a second conformance instruction.

Outside-project execution is still outstanding. No contact with another project
or independent adoption is implied by publishing this seed.
