# romfs.nro: libnx filesystem + RomFS fixture

Homebrew-only test program built with devkitA64 and libnx, like `../hello`. It mounts storage the way
NRO homebrew does with libnx's default startup, but step by step:

1. `smInitialize()`.
2. `fsInitialize()`: `GetServiceHandle("fsp-srv")`, `ConvertCurrentObjectToDomain`, `SetCurrentProcess`.
3. `fsdevMountSdmc()`: `OpenSdCardFileSystem` (a domain object).
4. `romfsInit()`: opens the `.nro` named by `argv[0]` (`sdmc:/romfs.nro`) with `IFileSystem::OpenFile`
   (path in an X buffer), reads the NRO header and asset section with `IFile::Read` (B buffer), then
   mounts the embedded RomFS.
5. Reads `romfs:/greeting.txt` (`hello from romfs\n`) with POSIX `open`/`read`.

It prints `ounce-romfs result=0x00000EEE` through `svcOutputDebugString` when every step works, else
`0xBAD0000 | bits` (bits 0-4 = steps above) after an `rc=` line with the first failing result code.

`ounce-core run` puts the NRO on the virtual SD card and passes `sdmc:/<file name>` as `argv[0]`, as
homebrew loaders do: `cargo run --release -- run tests/fixtures/nro/romfs/romfs.nro`.
Test: `libnx_romfs_fixture_reads_its_romfs` in `src/loader/fixture_tests.rs`.

Rebuild: `make -C tests/fixtures/nro/romfs clean all` (byte-reproducible). The RomFS is appended by
`add_romfs.py` because `elf2nro --romfsdir` silently ignores the option on macOS (its option parsing
stops at the first positional argument).
