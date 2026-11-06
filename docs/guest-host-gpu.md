# Guest shaders on a host graphics device

The experimental backend executes guest Maxwell/Ampere shader programs through
wgpu. It receives the same guest channel draw records as the CPU renderer, reads
SASS, vertex streams, indices and constant buffers from guest GPU virtual memory,
translates the shaders to WGSL, submits an offscreen render pass, and reads the
pixels back into the original pitch-linear or block-linear guest render target.
VI presentation subsequently reads those guest bytes. No CPU vertex or fragment
shader execution supplies the host draw's pixels.

Build and opt in:

```sh
cargo build --release --locked --features gpu-present
OUNCE_GPU_BACKEND=host target/release/ounce-core run /path/to/homebrew.nro
```

`OUNCE_GPU_BACKEND=cpu` selects the existing CPU renderer (also the default).
Host mode rejects unsupported draws and records a diagnostic; it does not silently
substitute CPU shading. Clears currently use the CPU path. This is a hybrid
implementation of a bounded command/shader subset, not NVN or a native Tegra driver.

The initial scope is one RGBA8 color attachment, supported vertex/fragment SASS,
bounded uniform bindings (including indexed reads), indexed or nonindexed triangles (including supported
strip/fan/quad normalization), viewport/scissor, face culling and color write masks.
Texture sampling, depth/stencil, blending, instancing, layered targets, geometry,
tessellation, multisampling and other unimplemented GPU state are not qualified.
New outside-workload failures should drive extensions to this scope.

Target dimensions/storage, vertex staging and index work are bounded. Submission
waits time out after 30 seconds. Validation, mapping or guest writeback failures do
not increment completed host draw counters. The runtime checks that readback bytes
reached guest memory before granting completion. Shader compilation/driver calls
still require a surrounding process timeout for unattended jobs.

`[guest-gpu]` diagnostics identify the actual adapter, API backend and adapter type.
`host_draws` includes software adapters such as llvmpipe; `hardware_draws` includes
only integrated/discrete adapters. Set `OUNCE_GPU_REQUIRE_HARDWARE=1` to reject
software or unclassified adapters. The old `gpu/vulkan.rs` fixture opens no device
and cannot increment a hardware presentation counter.

## Independent qualification

Two explicit, ignored tests exercise this backend:

```sh
cargo test --release --locked --features gpu-present \
  host_sass_draw_writes_original_guest_surface -- --ignored --nocapture
OUNCE_GPU_BACKEND=host OUNCE_DEKO_BASIC_NRO=/path/to/deko_basic.nro \
  cargo test --release --locked --features gpu-present \
  guest_gpu_deko_existing_oracle -- --ignored --nocapture
```

The first test is unit evidence: Ampere SASS writes both surface layouts and a
rejected depth draw preserves bytes and counters. It requires an adapter and never
silently skips. The second executes the pinned, unchanged outside deko3d program
under Interpreter and Tier-3, requires host draws/readbacks, and compares every
frame against the existing `tests/corpus/oracles.json` values. No frame expectation,
workload registration or verifier policy is changed for this implementation.

Hosted qualification runs Linux with software Vulkan and macOS with physical GPU
required. Logs retain adapter identity, NRO hash, per-engine draw/readback counts,
and exact frame comparisons. A completed draw alone is not an oracle pass. PLAN.md
step 14 stays open until its physical-GPU and unchanged-oracle gates pass.
