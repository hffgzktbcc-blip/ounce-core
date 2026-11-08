# zlib differential check (host reference vs Ounce-Core engines)

A real, widely used program, not homebrew: unmodified zlib 1.3.1 (zlib licence) compiled for AArch64 with devkitA64 (`-O2`) and run inside Ounce-Core. The expected output comes from the same C source compiled with the host `cc`, so the oracle is independent of Ounce-Core.

- zlib pinned at tag `v1.3.1`, commit `51b7f2abdade71cd9bb0e7a373ef2610ec6f9daf` (github.com/madler/zlib). Not vendored here; fetch it to reproduce. No `Z_SOLO` or `NO_GZIP`: the full library minus `gz*.c` and `infback.c`.
- Harness (this repo): `cases.c` (shared cases), `main_nx.c` (NRO entry, writes `sdmc:/results.txt`), `host/host_main.c` (host reference).
- Cases: six inputs (empty, 1 byte, 5000 B text, 20000 B zeros, 30000 B low-entropy random, 70000 B mixed) at levels 0/1/6/9, all five strategies (default, filtered, huffman, rle, fixed) at levels 1 and 6, raw and gzip wrappers, window 9 / memLevel 1, `compress2`/`uncompress`/`compressBound`; each prints compressed size, FNV-1a of the compressed bytes and a round-trip check. `crc32`/`adler32` of every input, `crc32_combine`/`adler32_combine`, byte-wise vs bulk crc, CRC table entries. Error paths: truncated stream, four corrupted bytes, too-small output buffer (inflate, deflate, `compress`), bad header, bad adler trailer, wrong dictionary, missing dictionary, plus dictionary and sync-flush chunked streams.
- The gzip header OS byte differs by host (macOS writes 19, the guest 3), so the harness pins it to 3 before hashing. That is a harness normalisation, not an emulator difference.

## Finding: two missing instruction families (fixed in this PR)
The first run failed on Tier-3 and Tier-4 with `[VM Error] Unimplemented instruction`:
1. `0x1AC04021` = `CRC32B w1, w1, w0`. zlib 1.3.1 uses the ARMv8 CRC32 instructions whenever the compiler defines `__ARM_FEATURE_CRC32`, which devkitA64 does. Added `CRC32{B,H,W,X}` and `CRC32C{B,H,W,X}` (`Instruction::Crc32`).
2. `0x4E030000` = `TBL v0.16b, {v0.16b}, v3.16b`, emitted by gcc's auto-vectorised `slide_hash`. Added `TBL`/`TBX` with 1 to 4 table registers (`Instruction::SimdTbl`, `simd::table_lookup`).
Unit tests: `crc32_instructions` and `tbl_tbx_instructions` in `src/cpu/interpreter.rs`, `table_lookup_tbl_tbx` in `src/cpu/simd.rs`. Both instructions go through the interpreter fallback in Tier-3 and Tier-4 (no JIT-native lowering). The CRC32C forms and the 64-bit CRC32X form are covered by the unit tests only; zlib uses CRC32 (not CRC32C) and the harness did not exercise every size.

## Result (2026-11-08, macOS arm64, `--cpu=...`, `OUNCE_NRO_BUDGET=4000000000`, build `--features "nce native-jit"`)
| Engine | Output vs host reference |
|---|---|
| Tier-3 | byte-identical (after the two fixes above) |
| Tier-4 JIT | byte-identical (after the two fixes above) |
| Native | byte-identical |

Native executes on real ARM64 hardware, so it is a check of the harness and toolchain rather than of the emulator; Tier-3 and Tier-4 are the real evidence.

Built NRO sha256: `6d431aaf657ec7b41de6924304ff36f1638aca2134abad636360e0f447eebc26`
Reference output (`expected-host.txt`, 122 lines) sha256: `c0099818ea2bcc6b61d60601b7b1432b6be1128694e2c5f222209cc1898be41e`. The host reference was run twice with identical output, and a second build with `-O0 -fno-vectorize` agrees.

## Not covered
One program, one host, synthetic inputs. `gz*` file API and `infback` are excluded. CRC32C and some CRC32 operand sizes are unit-tested but not exercised by zlib. No concurrency, no inputs over 70 KB, no `Z_SOLO` build, no ARM NEON `adler32` path (not present in 1.3.1 upstream).

## Reproduce
```sh
git clone --depth 1 --branch v1.3.1 https://github.com/madler/zlib
# copy zlib/*.h and adler32 compress crc32 deflate inffast inflate inftrees trees uncompr zutil .c next to cases.c main_nx.c
cc -O1 -o host_ref host/host_main.c cases.c adler32.c compress.c crc32.c deflate.c inffast.c inflate.c inftrees.c trees.c uncompr.c zutil.c
./host_ref > expected.txt
# NRO: devkitA64 + libnx template Makefile (SOURCES := .), then:
cargo build --release --features "nce native-jit"
OUNCE_NRO_BUDGET=4000000000 target/release/ounce-core run-nro zlib_harness.nro --present=headless --sdmc=DIR --cpu=tier3   # also tier4, native
cmp DIR/results.txt expected.txt
```
