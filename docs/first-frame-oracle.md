# First-frame oracle: `simplegfx` (written before execution)

Status: **oracle for the next run, not yet executed against it.** Written on
2026-11-01 from the upstream source.

Provenance: this is **not** a prediction made before any ounce-core run of the
sample. PR #103 already recorded a simplegfx frame hash of `0xd95ddf22e46ba325`
on 2026-10-30, which is the frame-1 value below. Frame 1 is therefore a
**previously observed result**, now explained by a calculation from the source.
Frames 2, 60 and 61, the wrap check and the raw-alpha values are **prospective**:
no evidence reviewed so far confirms them. The hashes were recomputed
independently from the formula on 2026-11-01 (8/8 match). That checks the
arithmetic, not the source semantics or any run.

## Sample

- Upstream: switchbrew/switch-examples `669786898205b7beb25ff1731e72982e6d0397d3`,
  `graphics/simplegfx/source/main.c` (93 lines). Already pinned as `simplegfx` in
  `tests/corpus/manifest.json`. Upstream README: Public Domain Mark 1.0. Linked
  libnx/newlib components need their own inventory (see `THIRD_PARTY.md`).
- Build: unchanged source, default Makefile, **no** `DISPLAY_IMAGE` define (that
  variant needs a user-supplied image and is out of scope).
- Why this one: the guest writes pixels from the CPU (`framebufferCreate`,
  `framebufferMakeLinear`, `framebufferBegin`/`End`). No GPU, shaders or
  rasteriser are involved, so a failure points at VI/Binder, nvmap or frame
  extraction.

## What the source says the frames contain

```c
if (cnt != 60) cnt++; else cnt = 0;          // before drawing
framebuf[pos] = 0x01010101 * cnt * 4;        // every pixel, 1280x720
```

`cnt` starts at 0 and steps before each frame is drawn. For the frame queued by
the *i*-th `framebufferEnd` (i = 1, 2, ...):

- `cnt = i mod 61`, so every byte of every pixel (R, G, B **and A**) is `4 * cnt`.
  The largest value is `0xF0`; nothing overflows.
- Frame 1 = `04 04 04 04` everywhere, frame 60 = `F0 F0 F0 F0`, frame 61 = all
  zero, frame 62 = frame 1.

## Expected values (1280x720, FNV-1a 64 as in `Frame::content_hash`)

| frame i | byte | hash, alpha forced to FF (`frame_from_surface`) | hash, raw bytes |
|---|---|---|---|
| 1 (previously observed in #103) | 0x04 | `0xd95ddf22e46ba325` | `0xa81539f09f682325` |
| 2 | 0x08 | `0x1dd22fb233d7a325` | `0x39fccc50d4972325` |
| 60 | 0xF0 | `0xa50b597b02e4a325` | `0x13b359fc16672325` |
| 61 | 0x00 | `0x52bda05e66a4a325` | `0xd395e456e4e72325` |

`frame_from_surface` forces alpha to `0xFF`, so PR #103's `RawFrameEvidence`
hash should match the middle column. The raw alpha (`4 * cnt`) can only be
checked on the guest bytes before that conversion.

## Pass criteria (all required)

1. The NRO is built from the pinned source with no changes. Its SHA-256 is
   recorded, and a second clean build reproduces it.
2. The guest gets through `nwindowGetDefault`/`framebufferCreate` without a libnx
   fatal error. Today the startup sample stops at `BadGfxInit` (`0x2559`); see
   `docs/CODEX_HANDOFF.md`.
3. At least frames 1 and 2 are captured from the **submitted** VI buffer
   (`submitted_vi_surface` -> `tiling_surface` -> `frame_from_surface`), at
   1280x720 RGBA8, and match the table. All 921,600 pixels have the stated value.
4. If the frame limit is at least 61, frame 61 is all zero (the wrap shows the
   counter is advancing and frames are not stale).
5. The interpreter and Tier-3 produce identical results for every captured frame.
6. The guest stops because of the runner's frame limit, or exits normally after
   an injected Plus press. Budget exhaustion, a fault or an unhandled SVC is a
   failure, even if a frame was captured first.

## What a pass does NOT prove

- **Tiling.** libnx swizzles the linear shadow buffer into the block-linear VI
  buffer itself. A uniform image looks the same under any swizzle, so this oracle
  cannot catch a wrong GOB or sector layout in `gpu::tiling`. The detiler stays
  checked only by its own round-trip tests (see the sector-layout caveat in
  `AGENTS.md`). A later sample needs pixels that vary by position.
- Presentation timing, vsync, scaling or anything about host-window output.
- Any GPU path.
- Compatibility beyond this one binary.

## Failure reporting

If any criterion fails, record which one, the guest stop reason, the PC, the last
VI/Binder transaction (`OUNCE_TRACE_IPC=1`) and the first differing pixel. Do not
report a partial capture as a pass.

## Result (2026-11-01, Claude, local run; CI step `simplegfx first-frame oracle` in `external-corpus.yml`)

All six criteria passed on an x86_64 Linux container. The test is `simplegfx_first_frame_oracle` in `src/loader/corpus_runner.rs`.

1. Built unchanged from `669786898205...` with `devkitpro/devkita64` image `sha256:1fc388c3a0d3...` (devkitA64 gcc 15.2.0, `libnx.a` `12a59cdf...`). Two clean builds gave the same NRO SHA-256: `b59fc85f0079bcd93007d97150e2af8302ac0804d1120cee43cf32fd37b69bf1`.
2. No libnx fatal error.
3. 61 frames were captured, one right after each VI `QUEUE_BUFFER`. Every frame is 1280x720, and every pixel equals `4 * (i mod 61)` in R, G and B. Frames 1, 2 and 60 match the table hashes.
4. Frame 61 is all zero and matches its hash.
5. The interpreter and Tier-3 captured identical frames over the same 865,587,709 instructions.
6. Both engines stopped on the runner's queued-frame limit.

The "What a pass does NOT prove" section above still applies. In particular this run says nothing about tiling, GPU, timing or other binaries.
