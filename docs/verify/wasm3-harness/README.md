# wasm3 (WebAssembly interpreter) through Ounce-Core

[wasm3](https://github.com/wasm3/wasm3) (MIT), core sources only (`source/m3_*.c`; no WASI, libc bindings or
`m3_deterministic.c`), unmodified, pinned at commit `28ecb9af6d2040e474a70f7cb7f43666740141fb` (verified with
`git fetch --depth 1`). It is not vendored: `scripts/verify_differential.py` fetches it. The guest runs a whole
interpreter (parser, validator, compiler, tail-call threaded dispatch) on top of the emulator, and that interpreter
in turn executes the WebAssembly below, so the emulator is exercised by two layers of integer, float and memory code.

## What is tested
- `gen_wasm.py` hand-assembles the modules (no wasm toolchain) and writes `wasm_modules.h` (committed, generated).
- `wasm3_cases.c` loads them through the public API (`m3_ParseModule`, `m3_LoadModule`, `m3_FindFunction`, `m3_Call`,
  `m3_GetResults`) and prints every argument and result as raw hex bits, or the trap text.
- Module `ops`: every i32 and i64 arithmetic, bitwise, shift, rotate, compare, clz/ctz/popcnt/eqz op over 10 and 12
  edge values per operand (all pairs); every f32 and f64 add/sub/mul/div/min/max/copysign/compare/abs/neg/ceil/floor/
  trunc/nearest/sqrt over 17 edge values (zeros of both signs, +-1, 0.5, 2.5, infinities, quiet and signalling NaN,
  2^31, 2^32, 2^63, denormal, max); all 29 conversion, reinterpret and sign-extension ops (including traps on NaN and
  out-of-range truncation); select and `unreachable`.
- Memory: all 14 load and 9 store forms, static offsets, addresses at and beyond the 64 KiB edge, a data segment,
  `memory.size`, `memory.grow` (up to the declared maximum, then failure), a byte-sum loop.
- Programs: recursive `fib`, i64 loop `fac` (with wrap-around), `collatz` (i64 mixed ops), `br_table` (in range, default,
  negative), a memory sieve of Eratosthenes up to 65536, recursive `gcd`, a mutable global, `call_indirect` through a
  table (valid slots, type mismatch, out-of-bounds slot).
- Also the original 61-byte `fib` module, kept as its own section.
- 14,766 output lines; 282 are traps (50 integer overflow, 88 divide by zero, 16 invalid conversion, 120 out-of-bounds
  memory access, 1 unreachable, 5 undefined element, 2 indirect call type mismatch). The host reference was
  hand-checked against the WebAssembly spec on a sample (div_s overflow, trunc of NaN/2^31, nearest ties-to-even, fac(21)
  wrap, primes below 65536 = 6542, Collatz 27 = 111 steps).

## Result (2026-11-08, macOS arm64, Ounce-Core built from `origin/main` `852d9ab` with `--features "nce native-jit"`, `OUNCE_NRO_BUDGET=4000000000`)
| Engine | Command flag | `results.txt` vs host reference |
|---|---|---|
| Tier-3 | `--cpu=tier3` | byte-identical |
| Tier-4 JIT | `--cpu=tier4` | byte-identical |
| Native | `--cpu=native` | byte-identical |

Reproduce: `python3 scripts/verify_differential.py --nro --only wasm3` prints
`wasm3: committed-vs-fresh-host=identical, tier3=identical, tier4=identical, native=identical`.
NRO sha256 `3165080ed761a0cb8d2656bbae61effb5e2c93793bc8bb5c853d13896ea1270f`; `expected-host.txt` sha256
`33536c8f3564a8fcac47f5dc30b440cf3c2690136dc59575dfa7f379611e678a`. The observed output of each engine is identical
to `expected-host.txt`, so no separate copy is committed.

Observed (first lines of each section, identical on host and all engines):
```
fib 00000016 -> 0000452f
i32.add 00000000 00000000 -> 00000000
i32.div_s 80000000 ffffffff -> trap: [trap] integer overflow
f32.nearest 40200000 -> 40000000
mem.i32.load 0000ffff -> trap: [trap] out of bounds memory access
sieve 00004e20 -> 000008d6
dispatch 00000003 00000007 00000003 -> trap: [trap] indirect call type mismatch
```

## Honest limits
- The oracle is the same C source compiled for the Apple Silicon host (real ARM64), so it shows the emulator agrees with
  hardware on this instruction mix, not that wasm3 is correct (output was also checked by hand against the spec on a sample).
- Native runs the AArch64 code on the CPU, so its identical result mostly validates the harness and reference, not the emulator's
  instruction semantics. Tier-3 and Tier-4 are the real tests.
- Not covered: WASI/libc bindings, SIMD proposal, threads/atomics, multi-memory, host imports, floating-point NaN payload
  propagation beyond what the table prints, other wasm3 build options (`d_m3HasTracer`, fast-math flags), non-ARM64 hosts as the reference.
- Wall time is dominated by the emulated interpreter; the run fits in the default 4e9 instruction budget.
