# PMULL / PMULL2 host-vs-guest check (issue #224)

`pmull.c` runs inline-asm `pmull` / `pmull2` in both forms (8-bit lanes to 8H, and 64-bit to 1Q) over 144 edge-value operand combinations (0, 1, 3, 0x80, 0xFF, 0x8000000000000001, all-ones, ...) plus 200 xorshift-random operand sets, with the destination preloaded with junk. It prints each result as a 128-bit hex string.

- Reference: compiled and run on the Mac (Apple Silicon is real ARM64): `clang -O1 -march=armv8-a+crypto pmull.c -o pmull-host && ./pmull-host > expected-host.txt`.
- Guest: NRO built with devkitA64 using the `templates/application` Makefile with `SOURCES := .` and `-march=armv8-a+crypto` added to `CFLAGS` (`Makefile` here). Writes `sdmc:/results.txt`.
- Run: `target/release/ounce-core run-nro pmull.nro --present=headless --sdmc=DIR --cpu=tier3` (also `tier4`, and `native` with `--features "nce native-jit"`), then `cmp DIR/results.txt expected-host.txt`.

## Result (2026-11-08, macOS arm64)
| Engine | results.txt vs host (1720 lines) |
|---|---|
| Tier-3 | byte-identical |
| Tier-4 | byte-identical |
| Native | byte-identical |

`observed-ounce-core.txt` is the Tier-3 output. NRO sha256 `2de3bbf7e7e50dba1c39e5e2065e1df16ca090821e4d9c116934d403bd89abe3`.

Not covered: PMULL with the reserved sizes 1 and 2 (decoded as undefined, not executed by this test), and the other "three different" ops. Native runs the instruction on hardware, so it validates the test and the reference rather than the emulator's PMULL.
