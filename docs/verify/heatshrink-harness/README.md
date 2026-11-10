# heatshrink differential check

heatshrink (ISC licence, github.com/atomicobject/heatshrink, commit `7d419e1fa4830d0b919b9b6a91fe2fb786cf3280`, encoder, decoder and their headers only; the `heatshrink.c` command-line tool is not used; not vendored). Licence confirmed (ISC, copyright Scott Vokes) before use. Built with the shipped defaults: dynamic allocation and the search index on.

`hs_cases.c` compresses generated inputs (zeros, noise, repetitive text, structured, mixed) at 10 lengths from 0 to 20000 bytes under 7 window/lookahead settings (8/4, 8/3, 10/5, 11/4, 4/3, 15/8, 12/6), feeding the encoder in different chunk sizes. It prints compressed length and hash, decompresses in one pass and again with 1-byte input and 1-byte output pieces, and checks both reproduce the input. It then decodes 40 random noise streams and 7 truncations of a valid stream (output length and hash printed; the decoder has no error for these, so what it emits is the observation), and checks parameter validation (`alloc` with a window below 4 / above 15, lookahead out of range, a zero input buffer) and API misuse (NULL, sink after finish). 371 lines, `fails=0` on the host.

Build the reference: `cc -O1 -I<dir> -o ref host/host_main.c hs_cases.c <dir>/heatshrink_encoder.c <dir>/heatshrink_decoder.c; ./ref > expected-host.txt` (two runs identical; an ASan/UBSan build gives the same bytes).
Build the NRO: devkitA64 with the `application` template Makefile, `SOURCES := .`, with `main_nx.c`, `hs_cases.c` and the six upstream files in one directory (`scripts/verify_differential.py --nro --build-only` does this). Output is `sdmc:/results.txt`.

## Result (2026-11-10, Linux x86-64 host, devkitA64 container, `scripts/verify_differential.py --only heatshrink`)
| Engine | `results.txt` vs host reference |
|---|---|
| Tier-3 | byte-identical |
| Tier-4 JIT | byte-identical |
| Native | not run (needs an ARM64 host; CI's ARM64 runners cover it) |

No emulator bug found by this program. `observed-ounce-core.txt` is the Tier-4 output (Tier-3 is identical). NRO sha256: `e8292b4107d8ff43f8f5927eb0d58e616391c4a576fe87300294cab0c2806c55`.

## Not covered
The static-allocation configuration (`HEATSHRINK_DYNAMIC_ALLOC 0`) and `HEATSHRINK_USE_INDEX 0`; allocation failure; inputs larger than 20000 bytes; integer-only code (no FP/vector instructions); one host (Linux x86-64).
