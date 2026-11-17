# Instruction budgets

Issue #479. CI catches performance regressions by counting guest instructions, not by timing. Wall time on shared
runners varies too much to fail a build on. The number of guest instructions a fixed program executes does not vary.

## What is checked

[`tests/insn-budgets.json`](../tests/insn-budgets.json) lists the workloads: the 11 committed NRO fixtures under
`tests/fixtures/nro/`, plus three Linux ELF fixtures from `tests/linux-elf/bin/`. Each one stops by itself.

[`scripts/insn_budgets.py`](../scripts/insn_budgets.py) runs every workload with `--deterministic --report-json` on
the interpreter and on Tier-3. It reads `insns_executed` from each `ounce.run.v1` report. The check fails when a
workload:

- **goes over its budget.** The budget is the recorded baseline plus 2% (at least one instruction).
- **stops in a different way than recorded**, for example `memory_fault` instead of `clean_exit`. A count from a run
  that stopped early cannot be compared.
- **counts differently on the two tiers.** Every tier must execute, and count, the same guest instructions.
- **has a fixture whose SHA-256 changed.** A rebuilt fixture runs different code, so it needs a new baseline.
- **does not stop within 300 s.**

Wall time is printed in the table for information only. A count below the baseline passes, with a notice.

The Runtime job in `ci.yml` runs the check on Linux, macOS and Windows. Its table goes to the job summary. A
negative control then lowers one budget below its count and requires the check to fail. `scripts/test_insn_budgets.py`
covers every failure path with a fake binary. It also confirms that the committed file still matches the committed
fixtures.

## Why the counts are stable

`--deterministic` runs the guest on a fixed clock on `run`, `run-nro` and `run-elf`:

- guest time advances 20 us per system call;
- a sleeping guest skips ahead instead of waiting;
- the date is fixed.

Before #479, `run` and `run-nro` kept the host clock. `input.nro` then gave 7,602 or 8,207 instructions depending
on host timing. With the fixed clock it gives 7,602 on every run and on both tiers.

## Using it

```sh
cargo build --release
python3 scripts/insn_budgets.py                  # check all workloads
python3 scripts/insn_budgets.py --only nro/threads
python3 scripts/insn_budgets.py --update         # record the current counts as baselines
```

Only run `--update` when a count change is intended, and say why in the pull request. The diff of
`tests/insn-budgets.json` shows the change for each workload. `--update` refuses to record when tiers disagree or a
run fails.

## What it catches, and what it does not

These counts move when the emulator changes what the same guest program does. Examples:

- a scheduler change that leaves a thread spinning longer;
- a service result that sends libnx down a slower path or into a retry;
- an SVC that now takes extra round trips.

As a check, the preemption interval was raised from 20,000 to 100,000 instructions, and only locally. `nro/threads`
went from 62,912 to 143,388 instructions (+127.9%), and the check failed with that message. The same change had no
effect on fixtures that do not wait on other threads.

The counts do not measure host speed. A slower interpreter or JIT that executes the same guest instructions passes.
Throughput measurements, which need a quiet machine, are in [verify/perf-hot-paths/](verify/perf-hot-paths/README.md).
The workloads are small programs, not games, so a regression that only shows up in a large program can pass.
