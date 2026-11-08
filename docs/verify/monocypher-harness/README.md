# Monocypher differential check: one emulator bug found and fixed (USHL/SSHL)

Unmodified Monocypher 4.0.3 (BSD-2-Clause / CC0 dual licence, https://monocypher.org, not vendored) plus its optional `monocypher-ed25519.c` (SHA-512 Ed25519), compiled for AArch64 with devkitA64 and run in Ounce-Core. Source: `https://monocypher.org/download/monocypher-4.0.3.tar.gz`, sha256 `8cc9bc341a66249016db9bd70e9142d8d0aef9945973744b1ac05dbc55d8ee66` (sha512 matches the published `.sha512`). The GitHub mirror named in the task does not exist, so the tarball was used. Expected output comes from the same source compiled on the host (`cc -O1`; `-O0` gives identical output).

Cases (`mc_cases.c`), one PASS/FAIL line per primitive, results printed as hex:
- BLAKE2b: RFC 7693 "abc", 20 hash/message/key length combinations (keyed and unkeyed), streaming equals one-shot.
- Argon2: RFC 9106 Argon2id vector (m=32, t=3, p=4, with secret and associated data), Argon2d/i/id with small memory, wrong password differs.
- X25519: RFC 7748 vectors, Alice/Bob key exchange, 1000-round iterated vector, wrong-key secret differs.
- Ed25519 (RFC 8032 tests 1 and 2, SHA-512 variant) and Monocypher's native BLAKE2b EdDSA: sign/verify across lengths 0..300; tampered signature, wrong message and wrong public key are rejected.
- ChaCha20 (RFC 8439 2.4.2), Poly1305 (RFC 8439 2.5.2), XChaCha20-Poly1305 AEAD: wrong key, tampered ciphertext, tampered MAC and tampered associated data are rejected.
- Constant-time compare (`verify16/32/64`): every single-bit difference detected; `crypto_wipe`.

All published vectors also pass on the host, so the host output is itself checked against the RFCs.

## Failure found (before the fix), Tier-3 and Tier-4; native was correct
`[VM Error] Unimplemented instruction 0x6EB3479C` = `ushl v28.4s, v28.4s, v19.4s`. Vector register-shift `SSHL`/`USHL` (AdvSIMD three-same opcode 01000, and the scalar D form) was undecoded; Monocypher's vectorised BLAKE2b/Argon2 rotates use it.

## Fix
Decode and execute `SSHL`/`USHL` (vector, all lane sizes, plus scalar D): signed low-byte shift per lane, negative shifts right (arithmetic for `SSHL`), amounts of at least the lane size give 0 (or the sign fill). Unit tests in `src/cpu/simd.rs` and `src/cpu/decoder.rs`. Rounding/saturating forms (`SRSHL`, `SQSHL`...) remain undecoded.

## Result (2026-11-08, macOS arm64, Ounce-Core main + this change)
| Engine | Output vs host reference |
|---|---|
| Tier-3 | byte-identical |
| Tier-4 JIT | byte-identical |
| Native | byte-identical (also before the fix) |

Native runs the code on the real ARM64 CPU, so it only shows the harness and toolchain are right; Tier-3 and Tier-4 are the real evidence for the emulator.

NRO sha256: `b49d11be3b717d55c2e371e210158a3a28cb5ef0caed1ff69dfc4985a9e9a576`. Reference output sha256: `475443527521ef92f3a2b5d453e4253c72ea21af57e751e8e4c3676ea9b73e79`. `observed-ounce-core.txt` is the Tier-3 output (Tier-4 and native are identical to it).

Not covered: other NEON ops Monocypher does not reach, timing side channels (constant time is only checked functionally), larger Argon2 memory, other compilers or optimisation levels, and no keys other than published test vectors and synthetic fills.

## Reproduce
```sh
curl -O https://monocypher.org/download/monocypher-4.0.3.tar.gz && tar xzf monocypher-4.0.3.tar.gz
cp monocypher-4.0.3/src/monocypher.[ch] monocypher-4.0.3/src/optional/monocypher-ed25519.[ch] .   # next to the harness files
cc -O1 -o host_ref host/host_main.c mc_cases.c monocypher.c monocypher-ed25519.c && ./host_ref > expected.txt
# NRO: devkitA64 template Makefile, SOURCES := . ; then
cargo build --release --features "nce native-jit"
OUNCE_NRO_BUDGET=4000000000 target/release/ounce-core run-nro monocypher_harness.nro --present=headless --sdmc=DIR --cpu=tier3   # also tier4, native
cmp DIR/results.txt expected.txt
```
