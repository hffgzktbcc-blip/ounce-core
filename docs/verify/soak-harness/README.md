# Soak differential check (long-running mixed workload)

A project-written program (no third-party code) that runs a mixed integer, floating-point, heap and sorting workload
for 100 rounds and prints one line per round: the round number, that round's hash and a running XOR. It exists to
cover what the short harnesses do not: a run of tens of seconds that exercises block caching, the allocator and
preemption over many iterations, and a failure that only shows after a long time.

Each round runs seven kernels in a pseudo-random order: a prime sieve, a 48x48 integer matrix product, a bitwise CRC-32,
a Mandelbrot grid plus single-precision square roots, 2,000 random malloc/free calls, a `qsort` of 4,000 integers and
3,000 signed and unsigned divisions. `sdmc:/iters.txt` can raise the round count; the default is 100.

Reference: `cc -O1 -o ref host/host_main.c soak_cases.c -lm; ./ref 100 > expected-host.txt` (run twice, identical, 100 lines).
The floating-point kernel is contracted differently on x86-64 and AArch64, so the reference must come from an AArch64
host (`arm64_reference` in `differential.json`).
Run: `python3 scripts/verify_differential.py --only soak --nro --engines tier3 tier4 native` (needs devkitA64).

## Result (2026-11-10, macOS arm64)
| Engine | `sdmc:/results.txt` vs host reference | Time |
|---|---|---|
| Tier-3 | byte-identical | about 18 s |
| Tier-4 JIT | byte-identical | about 8 s |

With 200 rounds on the same machine: interpreter 52 s, Tier-3 36 s, Tier-4 16 s, output identical to the host
(this is the run that exposed `run-nro --cpu` being ignored, fixed in #305).

## Not covered
No file I/O beyond the result file, no threads, no input, no services. One host and one compiler flag set. Agreement on
this workload does not show agreement elsewhere.
