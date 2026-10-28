# services.nro: libnx service handshake fixture

Homebrew-only test program built with devkitA64 and libnx, like `../hello`. It skips libnx's default
`__appInit` and opens services itself:

1. `smInitialize()`: `svcConnectToNamedPort("sm:")`, then `RegisterClient` with the process ID.
2. `hidInitialize()`: `GetServiceHandle("hid")`, `CreateAppletResource` (PID, returns a session as a
   move handle), `GetSharedMemoryHandle` (copy handle), then libnx `virtmem` picks a free address with
   `svcQueryMemory` and maps it with `svcMapSharedMemory`.
3. Reads the mapped HID shared memory, then `hidExit()` and `smExit()`.

It prints `ounce-services result=0x00000EEE` through `svcOutputDebugString` when every step works,
else `0xBAD0000 | bits` (bit 0 sm, bit 1 hid, bit 2 shared memory) after an `rc=` line with the first
failing result code.

Run it: `cargo run --release -- run tests/fixtures/nro/services/services.nro`.
Test: `libnx_services_fixture_opens_sm_and_hid` in `src/loader/fixture_tests.rs`.
Rebuild: `make -C tests/fixtures/nro/services clean all` (byte-reproducible; see `../hello/README.md`).

Exact devkitA64/libnx versions and this binary's build ID + SHA-256 are recorded in
`../TOOLCHAIN_VERSIONS.md`.
