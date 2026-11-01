# First-frame oracle: `deko_basic` (written before execution)

Status: **pre-registered oracle for deko_basic bring-up, not yet executed against a passing run.** Written on
2026-11-01 from upstream `switchbrew/switch-examples` source.

## Sample

- Upstream: switchbrew/switch-examples `669786898205b7beb25ff1731e72982e6d0397d3`,
  `graphics/deko3d/deko_basic` (pinned in `tests/corpus/manifest.json`). Upstream README: Public Domain Mark 1.0.
- Build: unchanged source, default Makefile, built with devkitA64 toolchain.
- SHA-256 (reproducible build): `5d108784619f6830952f7a7342ab5079c678d3d09521659204a83ac45a52b696`.
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

1. The NRO is built from the pinned source with no changes, matching SHA-256 `5d108784619f6830952f7a7342ab5079c678d3d09521659204a83ac45a52b696`.
2. The guest initializes RomFS, loads `romfs:/shaders/triangle_vsh.dksh` and `romfs:/shaders/color_fsh.dksh`, and initializes deko3d device, queue, and swapchain without errors.
3. At least the first submitted frame is captured from the VI queue buffer at 1280x720 RGBA8.
4. Background pixels outside the triangle match clear color `(32, 75, 122, 255)` (`#204b7aff`).
5. Triangle interior pixels exhibit smooth vertex color interpolation (Red near top, Green near bottom-left, Blue near bottom-right) and non-uniform pixel values, verifying block-linear detiling.
6. The interpreter and Tier-3 engines agree on pixel hashes.
7. Clean stop on frame limit or normal termination.
