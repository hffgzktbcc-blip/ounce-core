# xxHash differential check: two emulator bugs found and fixed

Unmodified xxHash (BSD-2-Clause) pinned at `2ec67caeb18ec15ae60c0853174331089acddfc0` (github.com/Cyan4973/xxHash, not vendored), compiled for AArch64 with devkitA64 and run in Ounce-Core. Expected output comes from the same source compiled on the host; a host scalar build (`-DXXH_VECTOR=XXH_SCALAR`) matches the host NEON build, so the reference does not depend on vector code.

Cases: XXH32, XXH64, XXH3-64 (default and seeded), XXH3-128 and chunked streaming, across 25 input lengths from 0 to 5000 bytes (every XXH3 length class including the long-input NEON loop).

## Failures found (before this PR), Tier-3 and Tier-4, native was correct
1. `UMLAL`/`UMLAL2` (and the whole widening-multiply "three different" group) were undecoded: `[VM Error] Unimplemented instruction 0x2EBF837D` at the first input over 127 bytes.
2. `PRFM` (prefetch) was decoded as a signed 8-byte load into `Rt`. A prefetch's `Rt` field is the prefetch operation (usually 0), so every `prfm` silently overwrote `x0`; here it corrupted the input pointer and faulted at an unmapped address. Silent register corruption in any program that prefetches.

## Fix
Decode and execute `S/UMULL`, `S/UMLAL`, `S/UMLSL` (+ `2` forms, 8/16/32-bit sources; `PMULL` still undecoded). Decode all `PRFM`/`PRFUM` forms as no-ops. Unit tests added for both.

## Result (2026-11-08, macOS arm64, Ounce-Core clean main + this change)
| Engine | Output vs host reference |
|---|---|
| Tier-3 | byte-identical |
| Tier-4 JIT | byte-identical |
| Native | byte-identical (also before the fix) |

NRO sha256: `ed08740300a42663033ac36c4dda9f1445a08cb250a9f5708dc0b857cfd47d3d`. One program on one host; it does not show the NEON implementation is complete (other 'three different' ops, e.g. `PMULL`, remain undecoded).

Reproduce as in `docs/verify/lz4-harness/README.md`, with `xxhash.c`/`xxhash.h` from the pinned commit and `xxh_run_cases`.
