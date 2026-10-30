# Host GPU replay correctness

The optional `gpu-present` path consumes `Gpu3DDrawOp` records, rather than
implementing a guest GPU driver or NVN. Guest method decoding and CPU rendering
remain separate. This work does not establish original Switch or Switch 2
application compatibility.

## Ordered batches and persistent state

`Wgpu3DRenderer::render_ops` uploads a batch's vertices, indices and uniform slots
once. Each clear begins a new render pass at its command position, with attachment
stores and loads preserving earlier output until explicitly cleared. Vertex/index
offsets continue across passes. Viewport, texture, matrix and depth-enable state
survive pass and submission boundaries. A texture unbind restores the default
white texture.

Texture-cache eviction occurs between batches, preserving the active binding;
it cannot invalidate an earlier draw's binding during the batch pre-pass. The
64-entry threshold triggers pruning on the next batch, rather than imposing a
hard per-batch bound. A batch using many distinct textures can exceed it.

## Regression evidence

`test_replay_order_and_inherited_bindings` uses an offscreen adapter and RGBA8
readback to check clears between draws and after a draw, inherited texture and
half-width viewport across submissions, more than 64 texture bindings in a batch,
and the final binding after cache pruning. It fails against the previous replay
implementation. Run with:

```sh
cargo test --release --features gpu-present test_replay_order_and_inherited_bindings -- --nocapture
```

Like the existing offscreen tests, it explicitly logs and skips if no adapter is
available. An `ok` result alone on such a machine is not GPU execution evidence.
The test prints adapter details when it executes.

## Remaining limits

The replay record supports triangle lists/strips, RGB vertices, a single RGBA8
texture with a fixed sampler, an MVP matrix, viewport, full color/depth clears,
and a depth-enable toggle. It does not carry general guest shaders, blending,
stencil, scissor, guest sampler state, multiple render targets, or complete depth
state. Host output is an offscreen presentation result, not GPU writes back into
guest memory. These fixes do not add those missing behaviors. Callers submit each
encoded batch before reusing its upload buffers; multiple unsubmitted replay
calls sharing those buffers are not supported.

Independent `deko_basic` startup progress reported by the coordinator is not a
frame oracle. Its initialization submission includes skipped methods on 2D class
`0x902D` (`0x2A4`, `0x290`, `0x884`). This replay change neither decodes those
methods nor turns startup/shutdown completion into a validated frame pass.
