# sdmc.nro: libnx SD card fixture (source only, not built yet)

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

Build on a machine with devkitPro: `make -C tests/fixtures/nro/sdmc clean all`, commit `sdmc.nro`, then
add a test in `src/loader/fixture_tests.rs` that writes `host.txt` into a temp folder and boots it with
`boot_with_kernel_sdmc(.., Some(&dir))`, expecting `0xEEE` and an empty folder apart from `host.txt`.
Run by hand: `cargo run --release -- run tests/fixtures/nro/sdmc/sdmc.nro --sdmc=<dir>`.

The same requests are covered without devkitPro by the libnx-shaped CMIF tests in `src/kernel/fsp.rs`.
