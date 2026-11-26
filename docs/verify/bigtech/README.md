# Big Tech library run: first-party drivers and build commands

What is here: the small test drivers (`blake3_drv.c`, `sgp4_drv.cpp`, `sj_drv.cpp`, `webp_drv.c`) and the build commands
(`run_*.sh`, `cmp.sh`) behind [the 23 Nov 2026 library page](../bigtech-2026-11-23.md). The drivers are first-party code
written for this project; they print deterministic text that is compared with the same driver built natively on the Apple M3.

What is not here: the upstream library sources (clone the commits listed on that page), the native oracle outputs, and the raw run logs.
Without those, a rerun rebuilds the guest ELFs and oracles itself; it does not replay the recorded logs.

Set `BT` (a work directory holding the upstream checkouts), copy this directory to `$BT/h`, set `ZIG` (Zig 0.17.1) and,
for `cmp.sh`, `OCDIR` (an `ounce-core` checkout built with `--release --features native-jit`). The scripts are the ones used
for the recorded runs, with the original `/tmp` paths turned into these variables; they have not been re-run in this form.
`run_mpd.sh`, `run_s2n.sh` and `run_secp.sh` compile upstream sources only (there is no first-party driver for libmpdec, s2n-bignum or secp256k1).
