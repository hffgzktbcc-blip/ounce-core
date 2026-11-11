# Repeating the CPU throughput measurements

These drivers use unchanged registered programs and verify their existing
oracles on **every** run. They never regenerate expected frames or stdout.
They require already-built local inputs, do not download retail content, and
preserve raw observations in a new output directory. They fail on an oracle,
engine, binary identity, or guest instruction-count mismatch. An interrupted
report has `complete: false`.

Build release executables on the baseline and candidate revisions using the
same toolchain and flags, copying the CLI and test executable outside `target`
before switching revisions. `cargo test --release --no-run --message-format=json`
prints the paths: select the `ounce-core` executable with `profile.test == true`
for the corpus driver, and the ordinary CLI for the ELF driver. Do not mix debug
and release builds. Run one cargo build at a time.

```sh
python3 scripts/benchmark_corpus.py \
  --baseline /tmp/before-tests --executable /tmp/after-tests \
  --builds /path/to/corpus/builds --output /tmp/corpus-benchmark \
  --only printing-hello-world cpu-jit --repeats 9 --warmups 1
```

The corpus driver runs the existing first-frame test separately for each member
of every A/B pair, with fresh CPU engines and the same registered frame count.
It reports guest instructions per second and queued frames per second from the
runner's elapsed time, which includes load, kernel/SVC work, frame capture and
oracle setup. This is end-to-end headless throughput, **not** game frame rate.
The combined-engine child-process CPU time includes test startup, both engines,
and oracle validation; it is deliberately not attributed to either engine.
`--seconds` is a per-engine cooperative deadline, with an additional process
watchdog. Failed/timeout samples are retained and abort the measurement.

For the pinned library harnesses in `docs/verify/differential.json`, first build
and verify an AArch64 Linux ELF with an available static cross compiler:

```sh
python3 scripts/verify_linux_elf.py --harness coremark \
  --elf-cc 'zig cc -target aarch64-linux-musl' \
  --ounce /tmp/after-cli --engines interpreter tier3 --work /tmp/coremark-verify
python3 scripts/benchmark_linux_elf.py \
  --verification /tmp/coremark-verify/report.json \
  --baseline /tmp/before-cli --candidate /tmp/after-cli \
  --output /tmp/coremark-benchmark --repeats 9 --warmups 1
```

The ELF driver checks the verified ELF and host-reference hashes before running,
requires successful exit and the requested engine identity, compares stdout
byte for byte, and requires equal guest instruction counts. Each process runs
with `--deterministic`. Wall and child user+system CPU time both include CLI
startup and ELF loading. CPU time excludes descheduling, helping distinguish
code improvements from contention on a shared host; it is not a substitute for
wall time. The CoreMark port uses a fixed 25 iterations and zero guest timer
ticks for deterministic CRC output: these are guest instructions per second,
**not** an official CoreMark score. Its expected ten-second timing warning is
part of the unchanged host reference.

Both drivers alternate baseline and candidate (ABAB), retain every measured
sample, and report paired speedup as `100 * (before_seconds / after_seconds - 1)`.
Warmups are retained but excluded from summaries. Medians and min/max are
observations, not confidence intervals or a universal performance guarantee.
Compare paired differences with their spread before claiming a wall-time win.
The Python CPU-time metrics require a Unix host (`resource.getrusage`).

For correctness beyond the timed cases, use the existing full corpus runner:

```sh
python3 scripts/run_corpus_oracles.py --executable /tmp/after-tests \
  --builds /path/to/corpus/builds --evidence /tmp/all-oracles --shards 1
```

Profiling on macOS can use the same test executable and registered input:

```sh
OUNCE_CORPUS_ROOT=/path/to/corpus/builds OUNCE_ORACLE_ID=simplegfx \
  /tmp/before-tests loader::fixture_tests::corpus_runner::corpus_first_frame_oracle \
  --exact --ignored --nocapture > /tmp/profile-run.log 2>&1 &
sample $! 5 -file /tmp/profile.txt
```

A sample is diagnostic evidence only. A five-second sample may cover only one
engine or a portion of startup; it does not establish whole-program speed.
