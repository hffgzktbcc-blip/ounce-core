# Original ELF workloads (`elf-*` harnesses)

Project-written C programs that grow the Linux ARM64 ELF matrix (`scripts/run_linux_arm64_matrix.py`, CI job
`linux-arm64-elf-matrix`) toward the 50+ workload target of issue #474. They are first-party: no vendored
binaries, no third-party source, no downloads. CI builds each one from the `.c` file in this directory with the
same Zig `aarch64-linux-musl` toolchain as the other harnesses (`-static -no-pie`, so the result is a static musl
`ET_EXEC`), and builds the same source for the host as the reference.

Every workload is single-threaded, reads no input, touches no host file, clock or environment, prints
deterministic text to stdout and returns 0 only if its own self-checks pass (`fails=0` is the last line, except
`control`, whose atexit handlers print after it). "File-bounded" workloads use in-memory `FILE` streams
(`fmemopen`, `open_memstream`) because `run-elf` has no `open`/`read` support.

## Provenance

All entries: written for ounce-core by its contributors, MIT licensed (the repository licence), clean-room from
public algorithm descriptions or standard C library behaviour. Where a workload checks published values, they
are listed. The `provenance` field of each `differential.json` entry points here.

| Harness | Source | What it covers | Published values checked |
|---|---|---|---|
| `elf-strops` | `strops/strops_cases.c` | string.h, ctype, strtol family, strtok_r, snprintf truncation, qsort/bsearch on strings | none (self-consistency) |
| `elf-sorts` | `sorts/sorts_cases.c` | insertion, merge, heap, quick sort vs libc qsort, bsearch, record sort with tie-break | none (all sorts must agree) |
| `elf-hashes` | `hashes/hashes_cases.c` | CRC-32 (bitwise and table), Adler-32, FNV-1a 32/64, djb2, sdbm, Jenkins one-at-a-time, SHA-256 | CRC-32 `CBF43926`, Adler-32 `11E60398`, FNV-1a("a"), SHA-256 of "", "abc", and the FIPS two-block message |
| `elf-bigint` | `bigint/bigint_cases.c` | base-10^9 big integers: factorials to 300!, Fibonacci to F(500), 2^1000 | 25!, F(100), digit sums of 100! (648) and 2^1000 (1366); the rest cross-checked in Python |
| `elf-printfmt` | `printfmt/printfmt_cases.c` | printf/snprintf integer, string, char and exact float conversions with flags | expected strings written from the C standard |
| `elf-intmath` | `intmath/intmath_cases.c` | sieve, gcd, extended Euclid, modpow, 64-bit Miller-Rabin, `__int128` arithmetic, isqrt, Collatz, perfect numbers, phi | pi(100000)=9592, 34!, sum of phi(1..1000)=304192, known primes and pseudoprimes |
| `elf-memstream` | `memstream/memstream_cases.c` | `fmemopen`, `open_memstream`, fgets, fscanf, fread/fwrite, fseek, ungetc | none (self-consistency) |
| `elf-heapmix` | `heapmix/heapmix_cases.c` | malloc/calloc/realloc/free churn, vector, list, hash table, aligned and mmap-sized blocks | none (content checks) |
| `elf-fpmath` | `fpmath/fpmath_cases.c` | exact or correctly rounded double/float operations, special values, conversions (built with `-ffp-contract=off`) | none (host vs ELF bit patterns) |
| `elf-wordfreq` | `wordfreq/wordfreq_cases.c` | tokenising original prose, open-addressing hash table, sort, ROT13 | none |
| `elf-codecs` | `codecs/codecs_cases.c` | Base64, RLE, LZSS, LEB128 varints, zig-zag | RFC 4648 test vectors |
| `elf-bytecode` | `bytecode/bytecode_cases.c` | stack-machine interpreter running fib, gcd, recursive factorial, prime count | 15! and pi(2000)=303 printed beside the VM result |
| `elf-puzzles` | `puzzles/puzzles_cases.c` | N-queens, sudoku, Game of Life glider, knapsack, LCS, Levenshtein, coin change, Pascal | queens 92/352, kitten/sitting=3, coin change 73682, C(20,10)=184756 |
| `elf-rng` | `rng/rng_cases.c` | MT19937, xorshift32, splitmix64, PCG32, unbiased bounded draws, Fisher-Yates | MT19937 seed 5489 outputs 1 and 10000, xorshift32 seed 2463534242, splitmix64 seed 0, PCG32 demo seed 42/54 |
| `elf-calendar` | `calendar/calendar_cases.c` | civil-date arithmetic, ISO weeks, `timegm`/`gmtime_r`/`strftime` in UTC, month grid | 1900..2200 day-by-day agreement with libc, 688 Friday-13ths per 400 years |
| `elf-control` | `control/control_cases.c` | setjmp/longjmp, atexit order, varargs, Ackermann, struct by value, bit-fields, switch tables, 48 KiB stack frame | ack(3,5)=253 |
| `elf-trees` | `trees/trees_cases.c` | AVL insert/delete, binary heap, Dijkstra, union-find, trie | none (ordering and bound checks) |
| `elf-graphs` | `graphs/graphs_cases.c` | Tarjan SCC, Kahn topological sort, Bellman-Ford (and negative-cycle check), Floyd-Warshall, Kruskal MST vs Prim | none (cross-checked against Prim/FW) |
| `elf-dp` | `dp/dp_cases.c` | matrix chain multiplication, edit distance (2D vs 1D), LIS O(n log n) vs O(n^2), subset sum, Catalan numbers | CLRS chain = 15125, kitten/sitting = 3, C(10)=16796, C(15)=9694845, C(19)=1767263190 |
| `elf-geometry` | `geometry/geometry_cases.c` | integer 2D cross products, monotone-chain convex hull, shoelace area, segment intersection, point-in-polygon | none (geometric invariants) |
| `elf-compress` | `compress/compress_cases.c` | canonical Huffman coding, BWT + Move-To-Front round-trip, Elias gamma coding | none (Kraft sum and round-trip checks) |
| `elf-automata` | `automata/automata_cases.c` | KMP, Boyer-Moore-Horspool, Aho-Corasick multi-pattern search, DFA lexer | none (cross-checked against brute-force scan) |
| `elf-linalg` | `linalg/linalg_cases.c` | modular matrix multiply/power over GF(p), Gauss-Jordan inversion over GF(p), Bareiss integer determinant vs Laplace | Fib(50) mod 10^9+7 = 586268941, Vandermonde(1..5) = 288 |

## What was run, and what it means

The initial 17 harnesses (`elf-strops` through `elf-trees`) were built with `python3 -m ziglang cc -target aarch64-linux-musl` (Zig 0.17.1) and the host
`cc` (gcc) at `O1`, `O2` and `O3_M1`, and run through `scripts/run_linux_arm64_matrix.py --only ...` on an
x86-64 Linux host: host stdout equals the `run-elf` stdout byte for byte on `--cpu=tier3` and `--cpu=tier4`,
and `diverge` reports interpreter, Tier-3 and Tier-4 identical. The 6 newer harnesses (`elf-graphs` through `elf-linalg`, bringing the Linux ARM64 ELF matrix to 51 harnesses) were compiled and run locally on Darwin ARM64 with `clang -Wall -Wextra -Werror` (`-O1` and `-O2 -fsanitize=undefined,address`) with `fails=0`, and await the Linux ARM64 CI matrix run on `ubuntu-24.04-arm`. The host reference is glibc on Linux (and Apple libc on Darwin), so a case where
glibc and musl legitimately differ would show up as a difference; two such cases (musl's `strtoll` end pointer
after overflow, glibc's `open_memstream` seek-to-end size) were removed from the workloads rather than hidden.
This is internal differential evidence for a bounded envelope. It is not an independent reproduction, did not
use `qemu-user` (not installed in the authoring sandbox), and has not yet run on the native `ubuntu-24.04-arm`
CI job.

Reproduce one: `python3 scripts/run_linux_arm64_matrix.py --work /tmp/new-dir --only elf-sorts` after
`cargo build --release --locked --features "nce native-jit"` and `pip install ziglang==0.17.1`.

Adding a workload: create `<name>/<name>_cases.c` defining `int workload_run(FILE *out)` (shared `host_main.c`
calls it), add an entry with `"linux_elf_only": true` and a `provenance` string to `docs/verify/differential.json`
(`scripts/test_run_linux_arm64_matrix.py` enforces both), add a row here, run the matrix on it, and keep only
output that is identical on host and `run-elf`.
