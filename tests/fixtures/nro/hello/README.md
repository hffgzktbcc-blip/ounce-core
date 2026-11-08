# hello.nro: libnx homebrew loader fixture

A tiny homebrew program built with devkitA64 and libnx. It is homebrew only: no firmware, keys or
retail code. `main.c` is the whole source; the binary links libnx (ISC license) and newlib.

It uses no services (`__appInit`/`__appExit` are empty) and returns `0xEEE` from `main` when all
of these hold, else `0xBAD0000 | bits`:

| bit | check |
|-----|-------|
| 0 | the `DT_INIT_ARRAY` constructor ran |
| 1 | pointers in `.data` were relocated (`DT_RELR`) |
| 2 | `.bss` is zero |
| 3 | `argv` came from the loader's `Argv` ConfigEntry (`argc == 2`, `argv[1] == "fixture"`) |
| 4 | `malloc` works on the loader's `OverrideHeap` |
| 5 | a `__thread` variable has its `.tdata` value |
| 6 | `memset(p, 0, 4096)` zeroes the block (newlib's `DC ZVA` path) |

libnx passes 0 back to the loader on exit whatever `main` returns, so the result is also written with
`svcOutputDebugString` as `ounce-fixture result=0x00000EEE`.

Tests: `src/loader/fixture_tests.rs`. The built `hello.nro` is checked in so tests need no toolchain.

## Rebuild

Needs devkitPro's `switch-dev` package (macOS: install the devkitPro pacman `.pkg` from
github.com/devkitPro/pacman/releases, then `sudo dkp-pacman -S switch-dev`).

```bash
make -C tests/fixtures/nro/hello clean all
```

Exact devkitA64/libnx versions and this binary's build ID + SHA-256 are recorded in
`../TOOLCHAIN_VERSIONS.md`.
