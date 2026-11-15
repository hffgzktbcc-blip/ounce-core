# tiny-AES-c differential check

tiny-AES-c (Unlicense / public domain, github.com/kokke/tiny-AES-c, commit `23856752fbd139da0b8ca6e471a13d5bcc99a08d`, only `aes.c`/`aes.h`, not vendored). Licence confirmed (`unlicense.txt`) before use. The library is compiled as shipped: AES-128 (`aes.h` hard-codes `AES128`; 192/256 would need an edit and are not tested) with ECB, CBC and CTR enabled.

`aes_cases.c` checks the NIST SP 800-38A AES-128 known-answer vectors for ECB (per block), CBC (one 64-byte call) and CTR (whole, and in 16/32-byte chunks), each with its decryption/round trip. Then it runs 24 generated key/IV rounds of CBC (1 to 20 blocks, then 64 to 256 blocks; all-zero and all-0xff key/IV included, so the counter carries through every byte), CTR for every length 0 to 70 (with all-0xff IVs on every third length), and one 4096-byte CTR run. Each line prints a hash of the ciphertext and the round-trip result. 103 lines, all self-checks pass on the host (`fails=0`).

Note found while writing the cases: the library keeps no partial-block position in `AES_CTR_xcrypt_buffer`, so CTR called in non-multiple-of-16 pieces gives different output from one call. That is upstream behaviour (host and guest agree), so the chunked case uses block multiples.

Build the reference: `cc -O1 -I<dir> -o ref host/host_main.c aes_cases.c <dir>/aes.c; ./ref > expected-host.txt` (two runs identical; an ASan/UBSan build gives the same bytes).
Build the NRO: devkitA64 with the `application` template Makefile, `SOURCES := .`, with `main_nx.c`, `aes_cases.c`, `aes.c`, `aes.h` in one directory (`scripts/verify_differential.py --nro --build-only` does this). Output is `sdmc:/results.txt`.

## Result (2026-11-10, Linux x86-64 host, devkitA64 container, `scripts/verify_differential.py --only tiny-aes`)
| Engine | `results.txt` vs host reference |
|---|---|
| Tier-3 | byte-identical |
| Tier-4 JIT | byte-identical |
| Native | not run (needs an ARM64 host; CI's ARM64 runners cover it) |

No emulator bug found by this program. `observed-ounce-core.txt` is the Tier-4 output (Tier-3 is identical). NRO sha256: `856c0735bc7f7c52a507915eba62fa0ba8a94a7dc0a9d199d0d2024ea49ee9dc`.

## Not covered
AES-192/256; timing or side-channel behaviour; hardware AES instructions (this library is table based, so no crypto-extension instructions are exercised); invalid lengths (the library does not check them); one host (Linux x86-64).
