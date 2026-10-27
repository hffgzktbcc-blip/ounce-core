# M5 step 1: deko3d "clear + one triangle" (design note)

Status: proposal from Track B (Claude). Needs a Track A (Antigravity) ack on the ownership split below
before any code lands. Nothing here touches `src/gpu/present/*`, `tier4.rs`, the recompiler or CI.

## Goal

A homebrew-only libnx + deko3d program clears a render target and draws one triangle, and ounce-core
produces the right pixels. Step 1 checks pixels in guest memory (no window). Step 2, later and jointly
with Track A, shows the same frame in the `run --gui` window through a swapchain.

## Fixture (`tests/fixtures/nro/deko3d/`, Track B)

- Built with devkitA64, libnx and deko3d, all already installed on the owner's Mac (`libdeko3d.a`,
  `uam` for shaders). Byte-reproducible like the other fixtures.
- `dkDeviceCreate`, one CPU-visible memory block for code/data and one GPU memory block for the image,
  a command buffer, a queue (graphics).
- Render target: 256 x 256 RGBA8, **pitch linear** (`DkImageFlags_PitchLinear | UsageRender`), so the
  CPU can read pixels back with no detiling in step 1.
- Commands: bind the render target, viewport/scissor, `dkCmdBufClearColorFloat` (dark blue), bind a
  vertex shader and a fragment shader compiled by `uam` from GLSL at build time (the DKSH binaries are
  checked in next to the source), one vertex buffer with three positions + colors, `dkCmdBufDraw`
  (triangle, 3 vertices), submit, `dkQueueWaitIdle`.
- Checks: a corner pixel equals the clear color; the centroid pixel equals the triangle's color (flat
  color first, so the step-1 check is exact; interpolated color is a follow-up). Reports
  `ounce-deko3d result=0x00000EEE` through `svcOutputDebugString` like the other fixtures.

## What the kernel/GPU path has to do

Found the same way as earlier fixtures: run it, record every IPC command, ioctl and GPU method it hits,
implement those, nothing speculative.

1. **nvdrv** (Track B, `src/kernel/nvdrv.rs`): deko3d opens the same devices as libnx's NvGpu (done in
   #39) plus whatever extra ioctls it issues (likely `ALLOC_OBJ_CTX` for 3D `0xB197`, compute `0xB1C0`,
   DMA `0xB0B5`, inline-to-memory `0xA140`; `SET_ERROR_NOTIFIER`; possibly `MAP_BUFFER_EX` with
   kinds/flags not seen yet). Unhandled ioctls already log and return `NotImplemented`, so gaps show up.
2. **GPFIFO -> Maxwell methods** (new `src/gpu/maxwell/`, proposed Track B): today
   `NvgpuChannel::process_pushbuffer` interprets ounce-core's own method numbers (`METHOD_SET_CLEAR_COLOR`
   etc.), which deko3d does not emit. Step 1 adds a separate decoder for the real Maxwell B (`0xB197`)
   class, selected per subchannel by `SET_OBJECT`, with the state step 1 needs: render target 0 (address,
   width, height, format, pitch), viewport/scissor, clear color + `CLEAR_SURFACE`, shader program region
   and per-stage start offsets, vertex stream 0 (address, stride) and attribute formats, constant buffer
   binding, `DRAW_BEGIN`/`VERTEX_BEGIN_GL`/`DRAW_END`. Method numbers and field layouts come from the
   public Nouveau/Mesa `nvc0`/`gm204` class headers (`cl9097.h`/`clb197.h` style) and deko3d's own source.
   Existing `NvgpuChannel` behaviour and the harness stages stay as they are; the new decoder only runs
   for channels that bind class `0xB197`.
3. **Shaders**: deko3d hands the GPU Maxwell SASS. Step 1 runs a small SASS interpreter for exactly what
   `uam` emits for a passthrough vertex shader and a flat-color fragment shader (attribute load/store,
   `MOV`/`MOV32I`, `FFMA`/`FMUL` if the transform needs them, `EXIT`), per vertex and per pixel, in
   software. Unknown opcodes stop the draw and log, like unknown CPU instructions. The existing
   `src/gpu/shader.rs` (`SassOp`, Ampere-oriented) is reused where the encodings agree; if they don't,
   the Maxwell decoder lives beside it rather than changing it.
4. **Raster**: one triangle, edge functions, top-left fill rule, writes to the pitch-linear RGBA8 target in
   guest memory. Block-linear targets (the default, and what a swapchain uses) are step 2.
5. **Sync**: already in place from #39: submits complete immediately and advance the channel syncpoint,
   so `dkQueueWaitIdle`'s fence wait passes.

## Step 2 (later, joint): to the screen

- `vi` + binder (`IApplicationDisplayService`, `IHOSBinderDriver` transactions for
  `IGraphicBufferProducer`: connect, set buffers, dequeue, queue) in the kernel (Track B), so
  `dkSwapchainCreate` on libnx's `NWindow` works.
- On `queueBuffer`, the kernel publishes a frame descriptor: guest address, width, height, format,
  layout (block linear with its block height), stride. Track A's present path detiles and shows it in
  the `run --gui` window. That descriptor is the whole interface between the lanes.

## Proposed ownership (needs Track A ack)

| Area | Owner |
|------|-------|
| `tests/fixtures/nro/deko3d/`, `src/loader/fixture_tests.rs` | Track B |
| `src/kernel/nvdrv.rs`, `src/kernel/ipc.rs` (vi/binder in step 2) | Track B |
| new `src/gpu/maxwell/` (B197 method state, SASS subset, rasterizer) | Track B |
| `src/gpu/nvgpu.rs` | shared: Track B adds only the per-subchannel hook that routes `0xB197` to `src/gpu/maxwell/`; existing behaviour unchanged |
| `src/gpu/present/*`, frame detiling and window presentation (step 2) | Track A |
| frame descriptor type (step 2) | defined by Track B in `src/kernel/`, consumed by Track A |

## Open questions for Track A

1. OK with Track B owning a new `src/gpu/maxwell/` and a one-hook change in `src/gpu/nvgpu.rs`?
2. For step 2, does the present path prefer to receive detiled RGBA frames (Track B detiles) or the raw
   block-linear surface plus its layout (Track A detiles, possibly on the GPU)?
3. Any planned Tier-4 or present work that would also touch `nvgpu.rs` soon?
