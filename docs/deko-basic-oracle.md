# First-frame oracle: `deko_basic` (written before execution)

Status: **executed against passing runs on 2026-11-01.** Verified on both Interpreter and Tier-3 JIT engines
against upstream `switchbrew/switch-examples` pinned commit `669786898205b7beb25ff1731e72982e6d0397d3`.

## Frame content & hash table

- Frame 1: Initial clear buffer submitted during display bringup before the first draw completes:
  - Resolution: 1280x720 RGBA8
  - Uniform clear color: `RGBA8(32, 75, 122, 255)` (`#204b7aff`)
  - Content hash (FNV-1a 64-bit): `0x7c9a9a3fa3856325` (8978658405233550117)
- Frames 2+: Fully rendered RGB triangle with smooth interpolation over clear background:
  - Resolution: 1280x720 RGBA8
  - Non-uniform, rendered frame
  - Content hash (FNV-1a 64-bit): `0x3a00ccf6b6f28ea1` (4179565814203190945)
  - Key sample points:
    - Top apex `(640, 10)`: `(251, 2, 2, 255)` (Red dominant)
    - Center `(640, 480)`: `(85, 85, 85, 255)` (Equal 1/3 RGB mix)
    - Bottom-left `(20, 700)`: `(7, 247, 1, 255)` (Green dominant)
    - Bottom-right `(1260, 700)`: `(7, 0, 248, 255)` (Blue dominant)
    - Background outside `(100, 100)`: `(32, 75, 122, 255)` (Clear color)

## Sample

- Upstream: switchbrew/switch-examples `669786898205b7beb25ff1731e72982e6d0397d3`,
  `graphics/deko3d/deko_basic` (pinned in `tests/corpus/manifest.json`). Upstream README: Public Domain Mark 1.0.
- Build: unchanged source, default Makefile, built with devkitA64 toolchain.
- SHA-256 (canonical CI build): `4f2ea875311acc46e5cef0632b2031091c72787aca64e676d91eaadf6cee5dfa`
  (built in canonical CI container `devkitpro/devkita64:latest` image `sha256:1fc388c3a0d34bd2045a6dadcb1020e069d5f876a187fd705de14b4440c00282`, devkitA64 GCC 15.2.0, binutils 2.45.1).
- SHA-256 (local build variant): `5d108784619f6830952f7a7342ab5079c678d3d09521659204a83ac45a52b696`
  (built with local devkitA64 GCC 16.1.0 prior to CI container verification).
- Why this one: First sample using deko3d and Maxwell/Ampere 3D GPU hardware rendering (command buffers, rasterization, block-linear render target, vertex and fragment shaders via RomFS).

## What the source says the frames contain

From `source/main.c`:
- Framebuffer resolution: 1280x720 (`FB_WIDTH`, `FB_HEIGHT`).
- Format: `DkImageFormat_RGBA8_Unorm`.
- Clear colour:
  `dkCmdBufClearColorFloat(g_cmdbuf, 0, DkColorMask_RGBA, 0.125f, 0.294f, 0.478f, 1.0f);`
  - Red: `round(0.125 * 255) = 32 (0x20)`
  - Green: `round(0.294 * 255) = 75 (0x4B)`
  - Blue: `round(0.478 * 255) = 122 (0x7A)`
  - Alpha: `255 (0xFF)`
  - Hex: `RGBA8(32, 75, 122, 255)` / `#204b7aff`
- Triangle coordinates and interpolation:
  - Positions in clip space `[-1.0, 1.0]`:
    - Vertex 0: `(0.0, +1.0)` -> Top centre: `(640, 0)` in framebuffer coordinates.
      Color: `(1.0, 0.0, 0.0, 1.0)` -> Pure Red `(255, 0, 0, 255)`.
    - Vertex 1: `(-1.0, -1.0)` -> Bottom left: `(0, 720)`.
      Color: `(0.0, 1.0, 0.0, 1.0)` -> Pure Green `(0, 255, 0, 255)`.
    - Vertex 2: `(+1.0, -1.0)` -> Bottom right: `(1280, 720)`.
      Color: `(0.0, 0.0, 1.0, 1.0)` -> Pure Blue `(0, 0, 255, 255)`.
  - Triangle centre `(x=640, y=480)` (barycentric coordinates approximately 1/3, 1/3, 1/3):
    Interpolated RGB colour: approximately `(85, 85, 85, 255)`.
  - Background pixels outside the triangle (e.g. top-left `(100, 100)` or top-right `(1180, 100)`):
    Must equal the clear color `#204b7aff` `(32, 75, 122, 255)`.

Because pixel colors vary by position (smooth color interpolation across 2D/3D screen space), this sample strictly exercises and validates correct block-linear tiling / detiling and GPU rasterization.

## Pass criteria (all required)

1. The NRO is built from the pinned source with no changes, matching canonical SHA-256 `4f2ea875311acc46e5cef0632b2031091c72787aca64e676d91eaadf6cee5dfa` (or local toolchain SHA-256 `5d108784619f6830952f7a7342ab5079c678d3d09521659204a83ac45a52b696`).
2. The guest initializes RomFS, loads `romfs:/shaders/triangle_vsh.dksh` and `romfs:/shaders/color_fsh.dksh`, and initializes deko3d device, queue, and swapchain without errors.
3. At least the first submitted frame is captured from the VI queue buffer at 1280x720 RGBA8.
4. Background pixels outside the triangle match clear color `(32, 75, 122, 255)` (`#204b7aff`).
5. Triangle interior pixels exhibit smooth vertex color interpolation (Red near top, Green near bottom-left, Blue near bottom-right) and non-uniform pixel values, verifying block-linear detiling.
6. The interpreter and Tier-3 engines agree on pixel hashes.
7. Clean stop on frame limit or normal termination.

## Result (2026-11-01, CI step `deko_basic first-frame oracle` in `external-corpus.yml`)

All seven criteria passed on an x86_64 Linux container (GitHub Actions run `41322535435`).
The test is `deko_basic_first_frame_oracle` in `src/loader/corpus_runner.rs`.

1. Built unchanged from `669786898205b7beb25ff1731e72982e6d0397d3` with `devkitpro/devkita64` image `sha256:1fc388c3a0d34bd2045a6dadcb1020e069d5f876a187fd705de14b4440c00282` (devkitA64 GCC 15.2.0, binutils 2.45.1). Canonical NRO SHA-256: `4f2ea875311acc46e5cef0632b2031091c72787aca64e676d91eaadf6cee5dfa`.
2. No libnx / deko3d error; RomFS initialized and loaded shaders successfully.
3. 5 frames captured from VI `QUEUE_BUFFER`, 1280x720 RGBA8.
4. Frame 1 clear color `RGBA8(32, 75, 122, 255)`, FNV-1a hash `0x7c9a9a3fa3856325`.
5. Frames 2-5 rendered triangle with smooth interpolation, non-uniform pixels, FNV-1a hash `0x3a00ccf6b6f28ea1`.
6. Interpreter and Tier-3 captured identical frames over identical instruction counts (9,234,934 instructions on Interpreter, 9,234,940 on Tier-3).
7. Both engines stopped cleanly on the runner's queued-frame limit.
