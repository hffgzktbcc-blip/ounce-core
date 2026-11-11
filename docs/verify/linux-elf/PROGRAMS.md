# Real programs run through Ounce-Core as Linux ELFs

Result of widening the Linux ELF differential check (2026-11-10). Each program is built twice from
its pinned upstream source, with its test cases unchanged between builds: once for the host and
once as a static AArch64 Linux ELF (Zig 0.17.1, `aarch64-linux-musl`, `-O1`). The ELF is then run on
`run-elf --cpu=tier3` and `--cpu=tier4` and its stdout compared byte for byte with the reference.
The same ELF also runs under `qemu-aarch64` (qemu-user 8.2.2) as an independent AArch64 reference.
Machine-readable detail (hashes, instruction counts): [more-programs-2026-11-10.json](more-programs-2026-11-10.json).
Method and flags: [README.md](README.md).

## What was actually run

On Linux x86-64 in the development sandbox, for all 18 programs in the table: host build, qemu-user, Tier-3 and
Tier-4 outputs are byte-identical (KISS FFT is compared with qemu only; it also matched its host build).
`run-elf` has only Tier-3 and Tier-4 engines, so there is no separate interpreter run for ELFs. Host builds of
eight of the new programs (all except CoreMark and KISS FFT) also ran clean under AddressSanitizer; UBSan reported only
benign misaligned loads (FastLZ) and a signed shift (SHA-1). Native (`--cpu=native`, ARM64 hosts only), macOS and
Windows were **not** run for these programs. A new Linux-only CI step runs them on every PR; macOS and Windows stay
covered only for LZ4, xxHash, cJSON and zlib. The Linux CI step has since run: the Differential verification workflow succeeded on main at `90c7649` ([run 42897000107](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42897000107)).

| Program (pinned) | Licence | Instructions (Tier-3) | Checked against |
|---|---|---|---|
| BLAKE3 `f55849f` C, NEON path vs portable host build | CC0/Apache-2.0 | 5.9 M | host, qemu, official test vectors (empty, 1 byte) |
| crypto-algorithms `cfbde48` (SHA-1/256, MD5, MD2, AES, Blowfish, DES, RC4, Base64) | public domain | 9.5 M | host, qemu, FIPS-197 / NIST / RC4 vectors |
| FastLZ `b1342da` | MIT | 66.7 M | host, qemu |
| tinf `57ffa1f` | zlib | 12.6 M | host, qemu, CRC-32/Adler-32 check values |
| jsmn `25647e6` | MIT | 0.4 M | host, qemu |
| SHA3IUF `fc85047` | MIT | 12.3 M | host, qemu, NIST SHA3-256 vectors |
| picohttpparser `dd573d8` | MIT/Perl | 0.2 M | host, qemu |
| inih `611ab51` | BSD-3 | 0.5 M | host, qemu |
| CoreMark `1f483d5` (fixed iterations, no clock) | Apache-2.0 | 7.0 M | host, qemu, CoreMark's own reference CRCs (`0xe714 0x1fd7 0x8e3a`) |
| KISS FFT `f3fd3a5` (float, complex and real) | BSD-3 | 3.3 M | qemu only (`elf_reference`), plus round trip and Parseval checks |
| Monocypher, Lua, QuickJS, SQLite, qoi, tiny-AES, heatshrink, stb_image (existing NRO harnesses, now also as ELF) | various | 36 M to 2.8 G | host, qemu |

## Emulator bugs found

Before these fixes Monocypher, Lua and QuickJS did not match their references. Each is fixed with
interpreter and Tier-4 unit tests; the original failures were captured first.

1. **`LDPSW` did not sign-extend** (decoded as a 32-bit `LDP`). Broke Monocypher's X25519 on both tiers; found by
   bisecting the instructions used and confirmed against qemu with a 20-line inline-asm program.
2. **Scalar `FADDP Dd, Vn.2D` / `Sd, Vn.2S` was unimplemented** (opcode `0x7E70D884`); stopped Lua.
3. **Scalar `FMUL/FMLA/FMLS/FMULX` by element was unimplemented** (`0x5FC09802`); stopped QuickJS.
4. **Runner: `mremap` (syscall 216) was missing**, used by musl's `realloc` of large blocks; stopped QuickJS after fix 3.

## Not working, and limits

- **wasm3 as an ELF does not run yet.** musl's allocator asks the runner for a PROT_NONE `MAP_NORESERVE` mapping
  starting at about 1 TiB and halving; the runner refuses anything over 512 MiB, so the first module fails to
  load ("allocation failed"). qemu-user runs the same ELF. The same program passes as an NRO in the existing
  harness. Cause of the oversized first request is not yet understood (see the issue filed for it).
- Host x86-64 and AArch64 legitimately differ for wasm3 (default-NaN sign), so wasm3 must use qemu as its reference.
- The programs' own hazards, avoided in the cases: `base64_decode` reads `in[-1]` for empty input (crypto-algorithms),
  and it scans to the end of its input, so encoded buffers are NUL-terminated.
- Integer, crypto, parsing, compression and interpreter code is well covered; floating point beyond one FFT, SIMD
  beyond BLAKE3's NEON and the existing vector tests, threads, files and signals are not.
- Single-input runs: each program passes its fixed cases, not arbitrary inputs. No outside party reproduced these results.

Reproduce one: `python3 scripts/verify_linux_elf.py --harness coremark --qemu qemu-aarch64 --work /tmp/new-dir --host-cc 'python3 -m ziglang cc' --elf-cc 'python3 -m ziglang cc -target aarch64-linux-musl'`
(after `cargo build --release --features native-jit`; `pip install ziglang==0.17.1`).
