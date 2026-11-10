# QuickJS differential check

QuickJS (MIT, github.com/bellard/quickjs, VERSION `2026-06-04`, commit `e5ac135f8db58f8d6b7b5862b8263ce6ba9c5ab6`, `quickjs.c`, `cutils.c`, `dtoa.c`, `libregexp.c`, `libunicode.c` and headers; not vendored; the `qjs` shell, `quickjs-libc` and the `std`/`os` modules are not used). Licence confirmed (MIT, Fabrice Bellard / Charlie Gordon) before use.

`qjs_cases.c` evaluates 46 fixed scripts with `JS_Eval` in a fresh context each and prints the string form of the result or the exception: number formatting and arithmetic (`toString` radixes, `toFixed`, `toPrecision`, 5e-324, 2^53), libm-backed `Math` functions printed to 10 digits, BigInt (2^200, 60!, division, asUintN), Unicode case mapping, normalisation and surrogate pairs, regular expressions (named groups, look-behind, backreferences, `\p{Lu}`, unicode sets, a catastrophic-backtracking pattern on 22 characters), JSON stringify/parse and a parse error, sorting 2000 elements, Map/Set/Object key order with 500 to 1000 entries, classes with private fields, generators, async ordering, Proxy/Reflect, typed arrays and DataView, UTC date functions, a 200000-iteration loop, recursion to depth 400, `eval`/`Function`, runtime errors, a syntax error and a thrown RangeError. Output is 60 lines, deterministic.

Guest build notes (all in the harness, upstream untouched):
- The upstream `.c` files are staged as `.inc` and compiled through five one-line wrappers, because the template Makefile and the host build both compile every `*.c` in the directory and `CONFIG_VERSION` needs a string literal. Only `qjs_core.c` has guest-specific code.
- devkitA64's newlib has no `tm_gmtoff`, so on the guest `qjs_core.c` enables the field and routes `localtime_r` to `gmtime_r` (local time is UTC; there is no time zone database). The scripts use only UTC date functions, so nothing compared depends on this.
- `#include <malloc.h>` supplies `malloc_usable_size`, which upstream's generic fallback calls.
- The recursion depth is kept at 400 with a 512 KiB JS stack limit. A first draft used depth 5000 and "stack overflow" fired at a host-compiler-dependent depth, so it is not a CPU-comparable result.

Build the reference: `cc -O1 -I. -o ref host/host_main.c qjs_cases.c qjs_core.c qjs_cutils.c qjs_dtoa.c qjs_libregexp.c qjs_libunicode.c -lm -lpthread -ldl` in a directory with the upstream files staged as above; `./ref > expected-host.txt` (two runs identical; identical at `-O2`, under ASan, and from an AArch64 `-O2` build run in QEMU; UBSan reports a signed left shift in `quickjs.c`, upstream's, without effect on the output).
Build the NRO: `scripts/verify_differential.py --nro --build-only` (devkitA64, `LIBS := -lnx -lm`). Output is `sdmc:/results.txt`.

## Result (2026-11-10, Linux x86-64 host, devkitA64 container, `scripts/verify_differential.py --only quickjs`)
| Engine | `results.txt` vs host reference | Wall time on this host |
|---|---|---|
| Tier-3 | byte-identical | about 95 s |
| Tier-4 JIT | byte-identical | about 93 s |
| Native | not run (needs an ARM64 host; CI's ARM64 runners cover it) | |

The issue expected this stretch target to find missing instructions or system calls; it did not. This build completes all 46 scripts with no `[VM Error]`. `observed-ounce-core.txt` is the Tier-4 output (Tier-3 is identical). NRO sha256: `d7a90b7e3d3b6268a5e91a9871d789352c652f0944c89624f24154b844ca5848`.

## Not covered
Threads, `Atomics.wait`, the module loader, `std`/`os`, the `qjs` binary, bytecode (`qjsc`) output, time zones, `Date.now()`/`Math.random()` (non-deterministic), memory-limit exhaustion, test262, Intl; Tier-3 and Tier-4 take about the same time here, so these cases say nothing about JIT speed-up. One host (Linux x86-64).
