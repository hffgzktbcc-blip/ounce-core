# gpu.nro: libnx GPU channel fixture

Homebrew-only test program built with devkitA64 and libnx, like `../hello`. It brings the GPU up the
way libnx graphics code does and submits one command buffer:

1. `smInitialize`, `nvInitialize`, `nvMapInit`.
2. `nvFenceInit` (`/dev/nvhost-ctrl`), `nvGpuInit` (`/dev/nvhost-ctrl-gpu`: characteristics, TPC masks,
   zcull info).
3. `nvAddressSpaceCreate` (`/dev/nvhost-as-gpu`), `nvGpuChannelCreate` (`/dev/nvhost-gpu`: nvmap fd,
   GPFIFO allocation, object contexts, zcull, error notifier, `QueryEvent`).
4. An nvmap command buffer mapped into the GPU address space (`MAP_BUFFER_EX`) holding one method
   header with two arguments, appended and kicked off (`SUBMIT_GPFIFO`).
5. `nvFenceWait` on the returned fence (`SYNCPT_WAIT` / `EVENT_WAIT`), then teardown.

It prints `ounce-gpu result=0x00000EEE` through `svcOutputDebugString` when every step works, else
`0xBAD0000 | bits` (bit n = step n of the 12 in `main.c`) after an `rc=` line.

Run: `cargo run --release -- run tests/fixtures/nro/gpu/gpu.nro`.
Test: `libnx_gpu_fixture_submits_a_command_buffer` (also checks the GPU model processed the command
and that no ioctl went unhandled). Rebuild: `make -C tests/fixtures/nro/gpu clean all` (byte-reproducible).
