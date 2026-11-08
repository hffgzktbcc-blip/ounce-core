# wasm3 (WebAssembly interpreter) through Ounce-Core

[wasm3](https://github.com/wasm3/wasm3) (MIT) at commit `28ecb9a`, core sources only (`source/m3_*.c`, no WASI or libc
bindings, no `m3_deterministic.c`), unmodified. It is not vendored here: clone it to `src/`, copy the sources to `nxsrc/`
as in `Makefile` (needs `wasm3.h`, `wasm3_defs.h`, `m3_*.h` too, and `-lm`).

`wasm3_cases.c` loads a 61-byte hand-assembled module exporting a recursive `fib(i32) -> i32` and calls it for 0..22,
printing results. The guest runs a whole interpreter (parser, compiler, tail-call dispatch) on top of the emulator.

Found emulator gaps: scalar `UQSUB` (and the other saturating add/subtract forms) and `SMOV`. See ../saturating-add-sub/.
Result: Tier-3, Tier-4 and native byte-match the Apple Silicon host (`fib 22 17711`). Not covered: more wasm modules,
WASI, SIMD ops in the module.
