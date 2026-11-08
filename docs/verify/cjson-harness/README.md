# cJSON differential check

cJSON 1.7.19 (MIT, github.com/DaveGamble/cJSON tag `v1.7.19`, commit `c859b25da02955fef659d658b8f324b5cde87be3`, only `cJSON.c`/`cJSON.h`, not vendored). `cjson_cases.c` parses valid documents (nested, `\u` escapes including surrogate pairs, raw UTF-8, big integers, doubles up to 1e308 and 5e-324, `-0`, 1e309), prints each formatted and unformatted, round-trips it, runs 36 malformed inputs (error offset printed), nests arrays to the 1000 limit (999 and 1000 parse, 1001 and 2000 fail at offset 1000), builds and edits documents through the API (add, replace, insert, detach, delete, duplicate, compare, references, raw), duplicate keys, NaN and infinity set through the API (printed as `null`), `Minify`, preallocated and buffered printing, and a 2000-number array. The host build of the same source is the oracle.

Build the reference: `cc -O1 -I<cJSON dir> -o ref host/host_main.c cjson_cases.c <cJSON dir>/cJSON.c -lm; ./ref > expected-host.txt` (run twice, identical, 263 lines).
Build the NRO: devkitA64 with the `application` template Makefile, `SOURCES := .`, `LIBS := -lnx -lm`, with `main_nx.c`, `cjson_cases.c`, `cJSON.c`, `cJSON.h` in one directory.

## Result (2026-11-08, macOS arm64, with the FABD fix)
| Engine | `sdmc:/results.txt` vs host reference | Exit |
|---|---|---|
| Tier-3 | byte-identical | 0 (`OUNCE_NRO_BUDGET=4000000000`) |
| Tier-4 JIT | byte-identical | 0 (same) |
| Native | byte-identical | 0 |

Native runs the guest code on the real ARM64 CPU, so Tier-3 and Tier-4 are the real evidence here.

Before the fix Tier-3 and Tier-4 stopped with `[VM Error] Unimplemented instruction 0x7EFCD7DE`: scalar `FABD d30, d30, d28`, which the compiler emits for the tolerance test in `cJSON_Compare`. It is now decoded as `Instruction::FpAbd` (single and double) and covered by `test_decode_fabd_scalar` and `fabd_scalar_absolute_difference`. Native was identical before and after.

NRO sha256: `1d174e27d3260c6c75cb9fa3acfba68a2ef4ba14ee7e939b5961d0582a64cf1d`.

## Not covered
Only `cJSON.c` (not `cJSON_Utils`: JSON Pointer, Patch, sorting); no file I/O; no custom allocator hooks; no out-of-memory paths; no threads; one host (macOS arm64) and one compiler flag set (`-O1` host, devkitA64 `-O2` guest).
