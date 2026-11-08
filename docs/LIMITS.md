# What this does not prove

Each result in this project is evidence for a narrow statement. This page says, per claim, what the
evidence does **not** show. Numbers come from [docs/verify](verify/README.md) and [the vision](VISION.md); if a number here disagrees with the
per-harness README, the README wins.

| Claim | What it does NOT prove |
|---|---|
| Native execution output is byte-identical to the host reference | Native runs the guest on the host CPU. On an ARM64 host it validates the harness, toolchain and loader only, **not the emulator**. Tier-3 and Tier-4 are the evidence for the emulator. On an x86 host native is unavailable. |
| Tier-3 and Tier-4 are byte-identical to the host reference on a harness | Agreement on the listed inputs only. It says nothing about inputs not in the harness, and nothing about instructions the harness never executes. Many vector, floating-point and system instructions are undecoded or untested. |
| QC-flag harnesses match the host (`qc-flag/saturating-shifts`: 5700 lines, 5684 with qc=1) | Most cases saturate (5684 of 5700), so the "QC clear" path and boundary cases just below saturation are barely exercised. A model that always set QC would pass almost all of them. FPCR rounding mode, flush-to-zero, default-NaN, trap enables and the cumulative exception flags (IOC, DZC, OFC, UFC, IXC, IDC) are not modelled, as the [qc-flag README](verify/qc-flag/README.md) states. |
| Everything was compared against real ARM64 hardware | Committed references were recorded on Apple Silicon macOS arm64. The differential workflow also runs on Linux ARM64; [documented host differences](verify/README.md#hardware-differences) are retained rather than treated as agreement. This does not cover every ARM64 core or compiler. No outside machine has reproduced it yet ([REPRODUCE.md](REPRODUCE.md)). |
| Harness inputs are random plus edge values | Inputs are a finite fixed set (for example 300 or 400 operand sets per form). They are not exhaustive and not a proof of instruction-level correctness. The [single-instruction fuzzer](verify/insn-fuzzer/README.md) exists, with seeded biased inputs; it does not cover all instructions, FPCR modes or multi-instruction interactions. |
| Scalar FMAX/FMIN/FMAXNM/FMINNM NaN handling matches ([fmaxnm-nan](verify/fmaxnm-nan/README.md)) | Covers 15 single and 14 double edge values through the scalar forms, compared as output only. FMAX/FMIN already matched before the FMAXNM/FMINNM fix, but the scalar signalling-NaN ordering of FMAX/FMIN is not checked beyond those pairs, and the vector forms are a separate harness. Treat scalar SNaN ordering as unchecked outside the listed edge values. |
| LZ4, xxHash, SQLite, Lua, FP instruction test run correctly | The programs produced the same output as the host on the test cases in the harness. This is not a statement that the program or the emulator is correct everywhere. Additional merged harnesses are listed in [the manifest](verify/differential.json); each retains its own scope and evidence. |
| 50/50 outside programs pass the first-frame oracle | Each program matches a **registered pixel oracle** (whole-frame hash or named pixel probes, see [corpus-oracles.md](corpus-oracles.md)) on the interpreter and Tier-3. That shows the registered frame is reproduced, not that the program runs correctly, and not that any console or Switch compatibility holds. Native and Tier-4 are not part of this count. |
| Failure diagnosis and replay | One bounded workflow and one historical case (the SDL2 early exit). Diagnosis output is a hypothesis, not a root cause. |
| Hashes and evidence are retained | A matching hash shows the bytes match the declared digest, not independent provenance or a reproducible build. |
| Loader fuzz run found no panics | 300 inputs. A small run, not a security audit. |
| Speed | None claimed. The interpreter runs at roughly 20 million instructions per second. |

Not established at all: running arbitrary ARM64 or Linux applications, and automatic root-cause finding.

## Maturity labels and where each claim sits

| Label | Meaning |
|---|---|
| research-only | explored, not dependable |
| bounded-verified | matches a reference on the listed inputs, with retained evidence |
| independently reproduced | someone outside the project reran it and got the same result |
| publicly supported | documented, versioned, and maintained for users |

| Claim | Label today |
|---|---|
| Tier-3 / Tier-4 / native match the host on LZ4, xxHash, SQLite, Lua | bounded-verified |
| Differential harnesses for floating-point, vector, QC and saturating instructions (hardware reference) | bounded-verified on the listed inputs |
| Additional library harnesses | see individual harness results; merging alone establishes no correctness claim |
| Instruction-level checking against hardware in general | research-only |
| 50/50 first-frame oracle corpus | bounded-verified for the registered pixel oracles only |
| Diagnosis and replay of a historical failure | bounded-verified (one workflow, one case) |
| Anything reproduced by an outside party | none yet; this is what [REPRODUCE.md](REPRODUCE.md) and the tracking issue ask for |
| Versioned diagnosis toolkit | released 0.2.0 for the [documented CLI scope](DIAGNOSIS_CLI.md); no general runtime support claim |

No outside reproduction is recorded here. The standalone diagnosis toolkit release does not establish public support for arbitrary runtime inputs.
