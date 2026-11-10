# stb_image differential check

stb_image and stb_image_write (public domain / MIT, either, github.com/nothings/stb, commit `4e9b368bfc071e97206ea5c6adba68dd0cbf8897`, only `stb_image.h` and `stb_image_write.h`, not vendored). Licence confirmed (dual MIT / public domain, `LICENSE`) before use. Built with `STBI_NO_STDIO`, `STBI_NO_HDR`, `STBI_NO_LINEAR`; SIMD stays on, so the guest runs stb_image's NEON JPEG paths.

`stbi_cases.c` generates every input itself, no files: 78 hand-built PNGs (colour types 0/2/3/4/6, bit depths 1/2/4/8/16, all five filter types plus a per-row cycle, tRNS, stored multi-block deflate), 6 BMPs (24/32-bit, top-down and bottom-up, row padding), and 5 sizes x 4 channel counts of PNGs and JPEGs (3 qualities) written by stb_image_write. Each image is probed with `stbi_info` and decoded with every requested channel count 0 to 4; the line shows size, an FNV-1a hash of the pixels and the first bytes. Also: truncated PNGs and JPEGs, bit-flips, wrong signature/bit depth/colour type/dimensions, a bad BMP, an empty buffer and random junk (failure reason printed). 217 lines.

Two findings about the reference, handled in the harness:
- **JPEG encoder output is compiler dependent.** stb_image_write's JPEG writer shifts a signed int past `INT_MAX` (UBSan flags it) and does floating-point maths that AArch64 compilers fuse into multiply-adds by default. Without `-fwrapv -ffp-contract=off` the *encoded* q=100 JPEGs differ between x86-64 and AArch64 builds (and so do their decodes) even in QEMU. The manifest entry sets those two flags for host and guest, and the cases print a hash of every encoded file, so a difference in the input is visible, not mistaken for a decoder difference. I first read this as an emulator bug (the six lines differed on Tier-3 and Tier-4); QEMU AArch64 at `-O2` gave the same six differences, which pointed at the inputs.
- **Truncated or corrupt JPEGs leave part of the output uninitialised** (the host differs from itself between runs), so for those cases only size and success are printed.

Build the reference: `cc -O1 -fwrapv -ffp-contract=off -I<stb dir> -o ref host/host_main.c stbi_cases.c; ./ref > expected-host.txt` (three runs identical; the same bytes with `-O2`, with `-DSTBI_NO_SIMD`, under ASan, and from an AArch64 `-O2` build run in QEMU, both NEON and scalar).
Build the NRO: devkitA64 with the `application` template Makefile, `SOURCES := .`, with `main_nx.c`, `stbi_cases.c`, `stb_image.h`, `stb_image_write.h` in one directory and the two cflags above (`scripts/verify_differential.py --nro --build-only` does this). Output is `sdmc:/results.txt`.

## Result (2026-11-10, Linux x86-64 host, devkitA64 container, `scripts/verify_differential.py --only stb-image`)
| Engine | `results.txt` vs host reference |
|---|---|
| Tier-3 | byte-identical |
| Tier-4 JIT | byte-identical |
| Native | not run (needs an ARM64 host; CI's ARM64 runners cover it) |

No emulator bug found by this program. `observed-ounce-core.txt` is the Tier-4 output (Tier-3 is identical). NRO sha256: `b9144ba1e0bbf657bf2a89109d1ba6e0b2f1bad28a3a2b26643af574850fecf9`.

## Not covered
GIF, PSD, TGA, HDR, PIC, PNM and interlaced PNG (the PNG writer here does not interlace); progressive and arithmetic-coded JPEGs (stb_image_write only writes baseline); real-world files; `stbi_load_16` and the float API; out-of-memory paths; one host (Linux x86-64).
