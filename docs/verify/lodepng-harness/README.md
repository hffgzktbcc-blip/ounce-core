# LodePNG differential check

LodePNG (zlib licence, github.com/lvandeve/lodepng, version 20261102, commit `7b2893e35891c0c85bee5595caafaf3c2044b409`, only `lodepng.cpp` and `lodepng.h`, not vendored). Licence confirmed (zlib, copyright Lode Vandevenne) before use. Following upstream's note that the library compiles as C90, `lodepng.cpp` is copied as `lodepng.c` and built with `-DLODEPNG_NO_COMPILE_CPP -DLODEPNG_NO_COMPILE_DISK`, so no C++ runtime is involved.

`lp_cases.c` encodes generated images and decodes them back, comparing with the source: 15 colour-type/bit-depth combinations (grey 1/2/4/8/16, RGB 8/16, palette 1/2/4/8, grey+alpha 8/16, RGBA 8/16) x 5 patterns with different filter strategies and Adam7 interlacing; 7 deflate settings (stored, fixed Huffman, dynamic, LZ77 off, window sizes 2 to 32768); all eight filter strategies on a 160x90 image; automatic format reduction; the standalone zlib compress/decompress with CRC-32; and the colour converter. It also decodes truncated PNGs and 40 single-byte corruptions (error code and text printed) and invalid decode requests. For each valid PNG it prints the encoded size and hash, so the encoder is compared as well as the decoder. 178 lines, `fails=0`.

Note for anyone extending it: LodePNG's raw buffers for bit depths below 8 are tightly packed, with no padding at the end of each row. My first draft padded rows and its round-trip check failed on the host itself; the cases now use the library's layout.

Build the reference: `cc -O1 -I<dir> -DLODEPNG_NO_COMPILE_CPP -DLODEPNG_NO_COMPILE_DISK -o ref host/host_main.c lp_cases.c <dir>/lodepng.c` with `lodepng.cpp` copied to `lodepng.c`; `./ref > expected-host.txt`. Identical across two runs, at `-O2`, under ASan/UBSan, and from an AArch64 `-O2` build run in QEMU.
Build the NRO: devkitA64 with the `application` template Makefile, `SOURCES := .`, with `main_nx.c`, `lp_cases.c`, `lodepng.c`, `lodepng.h` in one directory (`scripts/verify_differential.py --nro --build-only` does this). Output is `sdmc:/results.txt`.

## Result (2026-11-10, Linux x86-64 host, devkitA64 container, `scripts/verify_differential.py --only lodepng`)
| Engine | `results.txt` vs host reference |
|---|---|
| Tier-3 | byte-identical |
| Tier-4 JIT | byte-identical |
| Native | not run (needs an ARM64 host; CI's ARM64 runners cover it) |

No emulator bug found by this program. `observed-ounce-core.txt` is the Tier-4 output (Tier-3 is identical). NRO sha256: `4db8632325d91e01f01899310a421424797aabee90f85d22992b8a94bbc331c5`.

## Not covered
The C++ API, `lodepng_util`, disk I/O, ancillary chunks (text, iCCP, gAMA, tIME), 16-bit palettes, images over 160x90, real-world files, out-of-memory paths; integer-only code (no FP or vector instructions); one host (Linux x86-64).
