# sdmc.nro: libnx SD card fixture

Homebrew-only test program built with devkitA64 and libnx, like `../romfs`. It mounts `sdmc:/` with
`fsdevMountSdmc()` and uses newlib stdio/POSIX calls, which libnx's fsdev turns into `fsp-srv`
`IFileSystem` / `IFile` / `IDirectory` requests:

| bit | step |
|-----|------|
| 0-2 | `smInitialize`, `fsInitialize`, `fsdevMountSdmc` |
| 3 | `mkdir("sdmc:/ounce")` |
| 4 | `fopen(.., "w")` + `fwrite` 11 bytes |
| 5 | `stat`: regular file, size 11 |
| 6 | `fopen(.., "a")` append, read back `"hello sdmc\nmore"` |
| 7 | `rename` a.txt -> b.txt; old name is `ENOENT` |
| 8 | `opendir`/`readdir`: exactly `b.txt` as `DT_REG` |
| 9 | `fopen("sdmc:/../escape.txt", "w")` fails |
| 10 | `remove`, `rmdir`, directory gone |
| 11 | reads `sdmc:/host.txt` (`from host\n`) that the test put in the host folder |

Prints `ounce-sdmc result=0x00000EEE` through `svcOutputDebugString` when every step works.

Build: `make -C tests/fixtures/nro/sdmc clean all`. The integration test
`libnx_sdmc_fixture_performs_host_file_operations` boots the compiled fixture with a host folder,
checks the guest success report, preserves `host.txt`, and checks that the guest removed its files.

Built on 2026-10-28 using devkitA64 r30 (GCC 16.1.0), libnx 4.12.0 and switch-tools 1.13.1.
Binary SHA-256: `42d46955e74b1aa01cd6181f442b54a19c7138721de15b19999e93e938004f48`.
This project-authored fixture checks integration; it is not an independently authored homebrew sample.
Run by hand: `cargo run --release -- run tests/fixtures/nro/sdmc/sdmc.nro --sdmc=<dir>`.

The same requests are covered without devkitPro by the libnx-shaped CMIF tests in `src/kernel/fsp.rs`.
