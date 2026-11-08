# LZ4 differential check (host reference vs Ounce-Core engines)

A real, widely used program, not homebrew: unmodified LZ4 (BSD-2-Clause, `lib/lz4.c`)
compiled for AArch64 with devkitA64 and run inside Ounce-Core. The expected output is
produced by the same C source compiled natively on the host, so the oracle is independent
of Ounce-Core.

- LZ4 pinned at `d04dab3152ffdb8f8e8708e4d135f7293c0f6136` (github.com/lz4/lz4). Not vendored here; fetch it to reproduce.
- Harness (MIT, this repo): `lz4_cases.c` (shared cases), `main_nx.c` (NRO entry), `host/host_main.c` (host reference).
- Cases: six round-trips (empty, 1 byte, text, zeros, low-entropy, mixed up to 65000 bytes) with compressed-size and FNV-1a hash of the compressed bytes; compress into an 8-byte buffer (expect 0); decompress into a 100-byte buffer (expect negative); four truncations and four bit-flips of a compressed stream (expect no crash, bounded return codes).

## Result (2026-11-08, macOS arm64, Ounce-Core `efeaaf7`)
| Engine | Command flag | Output vs host reference |
|---|---|---|
| Tier-3 | `--cpu=tier3` | byte-identical |
| Tier-4 JIT | `--cpu=tier4` | byte-identical |
| Native | `--cpu=native` | byte-identical |

Built NRO sha256: `500d55a5ba54aec3fd7b36f62d114b2c93da9d3626f77b446c5e945ef234be46`
Reference output sha256: `e0c701319fea9379f0687dd1a53778425653baac45cab085dddee868283ec9fd`
No discrepancy found. This is one program on one host; it does not show LZ4 is correct on every input or that other instruction mixes behave.

## Reproduce
```sh
git clone https://github.com/lz4/lz4 && git -C lz4 checkout d04dab3152ffdb8f8e8708e4d135f7293c0f6136
cp lz4/lib/lz4.c lz4/lib/lz4.h .   # next to the harness files
cc -O1 -o host_ref host/host_main.c lz4_cases.c lz4.c && ./host_ref > expected.txt
# NRO: devkitA64 + libnx template Makefile with SOURCES := . ; then:
cargo build --release --features "nce native-jit"
target/release/ounce-core run lz4_harness.nro --present=headless --sdmc=DIR --cpu=tier3   # also tier4, native
diff DIR/lz4-results.txt expected.txt
```
Note: `run-nro` with `--cpu=tier4|native` printed help in this build; use `run`.
