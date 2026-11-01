
## Result (2026-11-01, Claude, local run; CI step `simplegfx first-frame oracle` in `external-corpus.yml`)

All six criteria passed on an x86_64 Linux container. The test is `simplegfx_first_frame_oracle` in `src/loader/corpus_runner.rs`.

1. Built unchanged from `669786898205...` with `devkitpro/devkita64` image `sha256:1fc388c3a0d3...` (devkitA64 gcc 15.2.0, `libnx.a` `12a59cdf...`). Two clean builds gave the same NRO SHA-256: `b59fc85f0079bcd93007d97150e2af8302ac0804d1120cee43cf32fd37b69bf1`.
2. No libnx fatal error.
3. 61 frames were captured, one right after each VI `QUEUE_BUFFER`. Every frame is 1280x720, and every pixel equals `4 * (i mod 61)` in R, G and B. Frames 1, 2 and 60 match the table hashes.
4. Frame 61 is all zero and matches its hash.
5. The interpreter and Tier-3 captured identical frames over the same 865,587,709 instructions.
6. Both engines stopped on the runner's queued-frame limit.

The "What a pass does NOT prove" section above still applies. In particular this run says nothing about tiling, GPU, timing or other binaries.
