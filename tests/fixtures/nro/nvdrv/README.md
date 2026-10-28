# nvdrv.nro: libnx nvdrv fixture

Homebrew-only test program built with devkitA64 and libnx, like `../hello`.

1. `smInitialize()`.
2. `nvInitialize()`: `GetServiceHandle("nvdrv")`, transfer memory (`svcCreateTransferMemory`),
   `Initialize` with the process and transfer-memory handles, and a cloned session.
3. `nvMapInit()`: `Open("/dev/nvmap")` (path in an A buffer).
4. `nvMapCreate()` on a 64 KiB heap block: `NVMAP_IOC_CREATE`, `NVMAP_IOC_ALLOC`, `NVMAP_IOC_GET_ID`
   (argument block in an X buffer, written back through a C buffer).
5. Checks the handle, ID and size, then `nvMapClose` (`NVMAP_IOC_FREE`), `nvMapExit`, `nvExit`.

It prints `ounce-nvdrv result=0x00000EEE` through `svcOutputDebugString` when every step works, else
`0xBAD0000 | bits` (bits 0-4 = steps above) after an `rc=` line with the first failing result code.

Run: `cargo run --release -- run tests/fixtures/nro/nvdrv/nvdrv.nro`.
Test: `libnx_nvdrv_fixture_creates_an_nvmap_object` in `src/loader/fixture_tests.rs`.
Rebuild: `make -C tests/fixtures/nro/nvdrv clean all` (byte-reproducible).

Exact devkitA64/libnx versions and this binary's build ID + SHA-256 are recorded in
`../TOOLCHAIN_VERSIONS.md`.
