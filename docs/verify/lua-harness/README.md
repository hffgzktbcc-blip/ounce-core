# Lua differential check

Lua 5.4.7 (MIT, github.com/lua/lua tag `v5.4.7`, commit `1ab3208a1fceb12fca8f24ba57d6e13c5bff15e3`, not vendored). The interpreter core and the base, string, table, math, utf8 and coroutine libraries are compiled for AArch64 with devkitA64 (`-lm`); `io`, `os` and dynamic loading are left out. The harness runs a 33-line Lua script (recursion, 64-bit integer and float edge cases, `string.format`, patterns, `string.pack`, `table.sort`, coroutines, `pcall`/`error`, metatables, closures, `goto`, a 100,000-iteration loop, GC) and records its output; the host build of the same source is the oracle.

## Result (2026-11-08, macOS arm64, with the FNMADD/FNMSUB fix)
| Engine | Output vs host reference | Exit |
|---|---|---|
| Tier-3 | byte-identical | 0 (`OUNCE_NRO_BUDGET=4000000000`) |
| Tier-4 JIT | byte-identical | 0 (same) |
| Native | byte-identical | 0 |

Before the fix all three engines agreed with each other but differed from the host on `2^53`, `math.sin` and `math.cos`, which is how the FNMADD/FNMSUB bug was found. NRO sha256: `99fe7de3690c82a3d5d739b74ee9ace7712695722003a90a595912f431f80453`. One program, one host.
