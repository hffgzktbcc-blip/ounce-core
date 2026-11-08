# TinyExpr differential check

TinyExpr (zlib licence, github.com/codeplea/tinyexpr, commit `ba2e32bbcc2d1e644a7e132d24c2f542821cabe0`, only `tinyexpr.c`/`tinyexpr.h`, not vendored). `tinyexpr_cases.c` evaluates about 240 expressions: precedence and associativity, unary signs, `%`, division by zero, NaN, exponent forms, every built-in function (`sin cos tan asin acos atan atan2 sinh cosh tanh exp ln log log10 pow sqrt abs ceil floor fac ncr npr`), constants `pi` and `e`, comparison operators, 33 malformed inputs (error position printed), compiled expressions with variables evaluated at six points, a 1000-step and a 200-step accumulation loop, user functions and closures, a 120-term and a 200-factor expression, and 100 nested parentheses.

Output is two streams. `expected-host.txt` is the compared one. Values from expressions that reach libm (`sin`, `exp`, `pow`, `^`, ...) are printed with `%.13g`; everything else with `%.17g`. The guest links newlib's libm and the host links the system libm, and they differ in the last bit on a few inputs. This is not an emulator question: Native, which runs the same newlib code on a real ARM64 CPU, shows the same last-digit differences. The full `%.17g` stream is written separately to `precise-host.txt` (host) and `precise-observed-ounce-core.txt` (guest). Only 6 of 242 lines differ between them (`tan(1)`, `asin(0.5)`, `acos(0.5)` and three more), all libm results; the three engines agree with each other on every line.

Build the reference: `cc -O1 -I<tinyexpr dir> -o ref host/host_main.c tinyexpr_cases.c <tinyexpr dir>/tinyexpr.c -lm; ./ref > expected-host.txt` (writes `precise-host.txt` in the current directory; two runs identical).
Build the NRO: devkitA64 with the `application` template Makefile, `SOURCES := .`, `LIBS := -lnx -lm`, with `main_nx.c`, `tinyexpr_cases.c`, `tinyexpr.c`, `tinyexpr.h` in one directory. Results are `sdmc:/results.txt` and `sdmc:/precise.txt`.

## Result (2026-11-08, macOS arm64, with the FABD fix)
| Engine | `results.txt` vs host reference | `precise.txt` vs Native | Exit |
|---|---|---|---|
| Tier-3 | byte-identical | byte-identical | 0 (`OUNCE_NRO_BUDGET=4000000000`) |
| Tier-4 JIT | byte-identical | byte-identical | 0 (same) |
| Native | byte-identical | (reference) | 0 |

Native runs on the real ARM64 CPU, so Tier-3 and Tier-4 are the real evidence. No emulator bug found by this program (the FABD gap was found by the cJSON harness, same PR).

NRO sha256: `601a8ca512c9a4b5600e42e93e00bedcacdef00aa29952ee0c2934a6d8ef90b7`.

## Not covered
Last-digit correctness of libm functions against the host (the two libms legitimately differ); `te_compile` memory failures; threads; one host (macOS arm64).
