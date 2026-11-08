# SQLite differential check: four more NEON/scalar gaps found and fixed

SQLite 3.50.4 amalgamation (public domain, sqlite.org, not vendored; `sqlite-amalgamation-3500400.zip`) compiled for AArch64 with devkitA64 and run in Ounce-Core with an in-memory database. Compile flags: `-DSQLITE_THREADSAFE=0 -DSQLITE_OMIT_LOAD_EXTENSION -DSQLITE_TEMP_STORE=3 -DSQLITE_OMIT_WAL -DSQLITE_DQS=0`. `stubs.c` supplies `fchown`/`geteuid`, which the Switch libc lacks and an in-memory database never calls. Expected output is the same source compiled on the host.

Cases: schema and index, 3000 prepared-statement inserts in a transaction, aggregates, GROUP BY/ORDER BY, LIKE, a self-join, a window function, a recursive CTE, string and printf functions, 64-bit integer overflow, float edge cases, JSON, a UNIQUE violation, rollback, `PRAGMA integrity_check` and a syntax error.

## Failures found and fixed (Tier-3 and Tier-4; native hit the same gaps and fell back to its interpreter)
1. Vector `REV16`/`REV32`/`REV64` undecoded (`0x4E201BFF`).
2. Widening add/subtract/absolute-difference family (`S/UADDL`, `S/UADDW`, `S/USUBL`, `S/USUBW`, `S/UABDL`, `S/UABAL`) undecoded (`0x2EBE13BC`).
3. Scalar `ADD`/`SUB`/`CMEQ`... on D registers undecoded (`0x5EFE87FF`).
4. Scalar `DUP`/`MOV Sd, Vn.S[i]` undecoded (`0x5E1C07EE`).
5. Not a decoder bug: NRO runs were capped at 50 million instructions, which SQLite exceeds. `OUNCE_NRO_BUDGET=<n>` now raises the cap (default unchanged).

## Result (2026-11-08, macOS arm64)
| Engine | Output vs host reference | Exit |
|---|---|---|
| Tier-3 | byte-identical | 0 (with `OUNCE_NRO_BUDGET=4000000000`) |
| Tier-4 JIT | byte-identical | 0 (same) |
| Native | byte-identical | 0 |

NRO sha256: `a2698cb89af7cd95e2b82686a3cf0e3cf4dcf7a0fb8bda81be0239cdcb104643`. One program on one host. `PMULL`, saturating-doubling and narrowing three-different ops remain undecoded.
