# NRO fixture toolchain/version register

All checked-in NRO fixtures were added on 2026-10-27 and built from the same devkitPro `switch-dev`
cohort on the maintainer's machine.

Toolchain cohort recorded for that date:

- `devkitA64` package: **r30-1**
  - depends on `devkita64-gcc >= 16.1.0-1`
  - depends on `devkita64-newlib >= 4.6.0.20260123-4`
- `libnx` package: **4.12.0-1**

Per-fixture binary identity:

| Fixture | Introduced in commit | NRO build ID (header 0x40..0x53) | SHA-256 |
|---|---|---|---|
| `hello/hello.nro` | `f1b33183b6a9a76db35811d21c639f4b83c4da27` | `142f31d514f7c7b03e9352c61156150382b2b275` | `b1d6a5723be27dd1e2f3dd304458f2e5574f6c7c8982c31f493aee4dc82a2b03` |
| `services/services.nro` | `8bd8f93cb7564d5a327d6bb06386520faf5f41de` | `f310ef2bfd8e25a9215117da3c126f0847d569ca` | `6ff6970a5eb75360892400b71312a279417a255bdeeec2241e9c77f89e3c172e` |
| `romfs/romfs.nro` | `146f7403517dd795ff68bb64fd0411b7dd571455` | `6429735a8ed747f04ca8f8d07c1a4fb2ee524d63` | `0ddd814f2d1c81899a3171b2eacd0d2684644bb0688a4f1cc1edc7dc8ac929b9` |
| `nvdrv/nvdrv.nro` | `95c863687d317da03c2fe5ac4afafa5bb820b663` | `21fb0836948217d6e6a24229b4eba9ea677018ee` | `356f59517e2f610a42d153564fba85ca52f07b6cfa956ff0f6d2895fcbf63dc8` |
| `gpu/gpu.nro` | `14dd13682d9eefbfb6ec68281cc422d00afa6307` | `cb87c379591661d4ee440aca57874479429def72` | `6583a56cf3afb5d88b167379330e6d8dcce27d4e6c55d67c9ac008cbc7079c74` |
| `input/input.nro` | `e8c53cd49730f2264805f6d64878285410f841ef` | `47058d19847dccf5e20b552030e355fdfc42f89b` | `a67aeb592aa4d03db8d52b0b6780073b73a3f797ddd9e4aaa3f8af1549453204` |
| `threads/threads.nro` | `e8027e4ec9a6d8d45a3c120fe9c12f28184dd371` | `8db32f75fae8b5dedba72ccf368838180ce42a0d` | `496c6db53ec97bacfde9f25e7dba6e8e7deeae9c974d072e8c9adb0abebad78c` |
| `audren/audren.nro` | `10177e93d452b6412ac3d77d387fbb1590ac9ed8` | `ee9a60cc76f1c112b449102d8a10c71e8452bc83` | `3fd52f32643de7e3869d954a7f5e6b66f8b243e7e2ec465f729483ed9ef0916f` |

Version source:

- devkitPro package metadata repository `devkitPro/pacman-packages`
  - `devkita64/devkitA64/PKGBUILD` at `f103fe88e37180ecd0b7a9173b52b7580a54f71a`
  - `libnx/PKGBUILD` at `b79ff0a9942ebdc84a02d46839a283ab92d3c5c8`
