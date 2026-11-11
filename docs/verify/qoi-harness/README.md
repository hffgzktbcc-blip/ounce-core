# QOI differential check

QOI ("Quite OK Image", MIT, github.com/phoboslab/qoi, commit `ffb2d2cb74a1de60819b21b939f7209aa53e91c1`, only `qoi.h`, not vendored; the header-only library is compiled with `QOI_IMPLEMENTATION` and `QOI_NO_STDIO`). Licence confirmed MIT (copyright Dominic Szablewski) before use.

`qoi_cases.c` encodes 360 synthetic images (10 sizes from 1x1 to 320x240, 6 generated patterns that exercise runs, index hits, small diffs, luma diffs and full RGB/RGBA ops, 3 and 4 channels, both colourspaces), prints length, FNV-1a hash, head and tail bytes of each encoding, then decodes it back as native, 3-channel and 4-channel output and checks round trip and channel conversion. It also checks the encoder's rejection of 5 invalid descriptions and a NULL input, and runs the decoder on 22 malformed streams (empty, short, truncated, bad magic/channels/colourspace, zero and huge dimensions, scrambled opcodes, bad padding, height larger than the data). Inputs come from a fixed LCG; nothing varies between runs. 144 output lines.

Build the reference: `cc -O1 -I<qoi dir> -o ref host/host_main.c qoi_cases.c; ./ref > expected-host.txt` (two runs identical; an `-fsanitize=address,undefined` build gives the same bytes and no report).
Build the NRO: devkitA64 with the `application` template Makefile, `SOURCES := .`, with `main_nx.c`, `qoi_cases.c`, `qoi.h` in one directory (`scripts/verify_differential.py --nro --build-only` does this). Output is `sdmc:/results.txt`.

## Result (2026-11-10, Linux x86-64 host, devkitA64 container, `scripts/verify_differential.py --only qoi`)
| Engine | `results.txt` vs host reference | 
|---|---|
| Tier-3 | byte-identical |
| Tier-4 JIT | byte-identical |
| Native | not run (needs an ARM64 host; CI's ARM64 runners cover it) |

No emulator bug found by this program. `observed-ounce-core.txt` is the Tier-4 output (Tier-3 is identical).

NRO sha256: `45b409120323df36bbdc57502630bd4c032a12634b67e0146d4a74490655b903`.

## Not covered
Only the in-memory API (`qoi_encode`/`qoi_decode`); no file I/O (`qoi_read`/`qoi_write`); no out-of-memory paths; images up to 320x240 plus one 8x64 malformed case; integer-only code, so no floating-point or vector instructions are exercised; the reference host here is x86-64 (QOI output is architecture-independent, and an ARM64 host should reproduce the same file).
