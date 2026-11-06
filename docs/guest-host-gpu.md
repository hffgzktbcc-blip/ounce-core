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

The initial scope is one RGBA8 color attachment, supported vertex/fragment SASS with all four position components written,
bounded uniform bindings (including indexed reads), indexed or nonindexed triangles (including supported
strip/fan/quad normalization), viewport/scissor, face culling and color write masks.
Texture sampling, depth/stencil, blending, instancing, layered targets, geometry,
tessellation, multisampling and other unimplemented GPU state are not qualified.
Unaligned static constant-buffer loads also reject before submission. New
outside-workload failures should drive extensions to this scope.

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
step 14 records acceptance against the adapter identity and unchanged-oracle evidence below.

Local execution evidence (2026-11-06): unchanged switchbrew/switch-examples commit
`669786898205b7beb25ff1731e72982e6d0397d3`, default deko_basic Makefile, public
devkitA64 GCC 16.1.0, libnx 4.12.0, deko3d 0.5.0 and uam 1.1.0. Observed NRO
SHA-256: `6808f4fe94f2349835c59ec7f70dd297f4566b33ff5cc107860dc2400c92d9fc`
(build identity, not a reproducible-build claim). Vulkan adapter: llvmpipe,
LLVM 19.1.7, type CPU. Both Interpreter and Tier-3 stopped on five queued frames,
completed five host draws and 18,432,000 readback bytes, and matched the existing
`0x3a00ccf6b6f28ea1` frame hash on every frame. Hardware draw count was zero.
The separate physical-adapter-required test correctly rejected this software device.


Hosted qualification (2026-11-06): implementation `e1f89ac`, tested through PR merge
commit `4688d97d0351393551d980e993381acd471d889e`, passed all 23 checks in
[PR #182](https://github.com/hffgzktbcc-blip/ounce-core/pull/182).
The [Metal job](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42274752750/job/128091109327)
reported **Apple Paravirtual device**, backend Metal, type IntegratedGpu. With the
hardware requirement enabled, both Interpreter and Tier-3 completed five host draws,
five hardware-class draws and 18,432,000 readback bytes. Every frame matched the
unchanged `0x3a00ccf6b6f28ea1` oracle. This is execution through CI's virtualized Metal
adapter, not bare-metal Tegra validation. Hardware classification comes from wgpu's
reported device type, not independent hardware attestation.

The [Linux job](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42274752750/job/128091109496)
matched the same frames on Vulkan llvmpipe (LLVM 21.1.8), correctly recording zero
hardware draws. Both jobs also passed Ampere SASS guest-surface unit tests and
unsupported-state rejection. The outside workload exercises Maxwell SASS; broader
independent Ampere application coverage remains unestablished.

The [canonical and native corpus](https://github.com/hffgzktbcc-blip/ounce-core/actions/runs/42274752750)
retained 46/50 passes with all prior passes preserved. GitHub artifact ZIP digests were
checked on download, and the original corpus binary bytes were independently
revalidated against all 100 canonical engine records. The README headline was checked
using the exact required engine set. No frame oracle, source registration or verifier
policy changed for this work.
