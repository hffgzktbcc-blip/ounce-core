# Multi-Optimization Matrix (`O1` / `O2` / `O3_M1`) & High-Star Gauntlet

Differential verification report covering multi-optimization support in
`scripts/verify_linux_elf.py`, all 24 existing real-world Linux AArch64 ELF
harnesses under higher optimization levels (`-O2` and `-O3 -mcpu=apple_m1`),
and three newly onboarded high-star C projects (`yyjson`, `stb-truetype`,
`zstd`).

- **Base revision**: `d52f6d0de98330ae47d61e18e9c6f0921806c7ff`
- **Host compiler (`--host-cc cc`)**: `Apple clang version 21.0.0 (clang-2100.4.1.1)`
- **ELF compiler (`--elf-cc`)**: `clang version 22.1.8` (`uv run --no-project --with ziglang==0.17.1 python3 -m ziglang cc -target aarch64-linux-musl`)
- **Result summary**: `59` matrix rows executed across `27` harnesses (`0` failures, `0` emulator bugs exposed).

## Tooling Changes

### `scripts/verify_linux_elf.py` & `scripts/test_verify_linux_elf.py`
- Added `--opt` (`O1` default for backwards-compatible bare invocations; choices: `O1`, `O2`, `O3`, `O3_M1`) and `--opt-matrix` (comma-separated list, e.g. `O1,O2,O3_M1`).
- Implemented `opt_flags(opt)` with strict cross-platform separation:
  - `O1` -> host `['-O1']`, elf `['-O1']`
  - `O2` -> host `['-O2']`, elf `['-O2']`
  - `O3` -> host `['-O3']`, elf `['-O3']`
  - `O3_M1` -> host `['-O3']`, elf `['-O3', '-mcpu=apple_m1']` (so x86_64 CI host compilers never receive `-mcpu=apple_m1`).
- Added `normalize_elf_build_flags(build_flags, elf_opt)` so existing `pmull` cflags (`-march=armv8-a+crypto`) map to `-mcpu=generic+crypto` under `zig cc` (or defer to `-mcpu=apple_m1` under `O3_M1`) while keeping `host-build` flags untouched.
- Added `--diverge` to invoke `ounce-core diverge <work>/<harness>.elf --budget <budget> --json <work>/diverge.json` and record verdict/engine statuses in `report['checks']['diverge']`.
- Multi-tier `--opt-matrix` runs organize builds into `<work>/<opt>/` and emit an aggregated `<work>/report.json` returning non-zero if any optimization tier or engine fails.
- Added unit test coverage (`OptMatrixTests`, `DivergeVerdictTests`) in `scripts/test_verify_linux_elf.py`.

### `scripts/verify_differential.py`
- Created parent directories before writing nested `upstream.copy` destination paths (`(d / t).parent.mkdir(parents=True, exist_ok=True)`) so multi-directory upstream trees (such as `zstd`) extract cleanly without flattening filenames.

## Newly Onboarded High-Star C Harnesses

Registered in `docs/verify/differential.json` with `linux_elf_only: true` and pinned `fetch_upstream` commits:

| Harness | Upstream Repo | Pinned Commit | License | Driver & Stress Surface | ASan + UBSan Host Check |
| --- | --- | --- | --- | --- | --- |
| `yyjson` | `https://github.com/ibireme/yyjson` | `ebb8d7e5aaa51bea4edefb39ee73a235066b0023` | MIT | `docs/verify/yyjson-harness/yyjson_cases.c`: parsing, Dragonbox float serialization, mutable docs, JSON Pointer, RFC 6902 Patch, RFC 7386 Merge Patch, error paths (SWAR, `CLZ`/`CTZ`, bitfields). | `exit=0`, `2196` bytes |
| `stb-truetype` | `https://github.com/nothings/stb` | `4e9b368bfc071e97206ea5c6adba68dd0cbf8897` | MIT / Unlicense | `docs/verify/stb-truetype-harness/stbtt_cases.c` (`-fwrapv -ffp-contract=off`): in-memory synthetic TrueType SFNT font (multi-contour, quadratic Beziers, consecutive off-curve points, compound scaled glyph), outline extraction, subpixel rasterization, atlas packing, SDF generation. | `exit=0`, `1642` bytes |
| `zstd` | `https://github.com/facebook/zstd` | `9d728bb08622963fa95fbb0c788ac13162829916` | BSD-3-Clause | `docs/verify/zstd-harness/zstd_lib.c` + `zstd_cases.c` (`ZSTD_LEGACY_SUPPORT=0`, `ZSTD_DISABLE_ASM=1`, single-threaded): one-shot compression across levels, CCtx parameters, streaming flush/end, raw & CDict/DDict dictionaries, frame inspection, corrupt frame rejection. | `exit=0`, `2534` bytes |

## Multi-Optimization Gauntlet Results (`59/59` Passed, `0` Failed)

Every check below produced byte-for-byte identical stdout against the native AArch64 macOS reference binary (`host.exe`), and matching `diverge` results where run.

> Notes:
> - **Fast + crypto + FFT harnesses** (`diverge`): tested with `--engine tier3 --engine tier4 --diverge`.
> - **Heavy harnesses** (`lua`, `monocypher`, `quickjs`, `sqlite`, `stb-image`, `wasm3`, `zlib`): tested with `--engine tier3 --engine tier4 --budget 4000000000`.
> - **New high-star harnesses** (`yyjson`, `stb-truetype`, `zstd`): tested across `O1,O2,O3_M1` with `--engine interpreter --engine tier3 --engine tier4 --diverge`.
> - **`kissfft`**: preserves `"elf_reference": "qemu"` in `docs/verify/differential.json` for Linux CI; on this macOS `arm64` host (without `qemu-aarch64`), verified directly against `host.exe` across `O1,O2,O3_M1`.

| Harness | Opt | ELF SHA-256 (12) | Output Bytes | Tier-3 Insns | Tier-4 Insns | Syscalls | Checks |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| `blake3` | `O2` | `2a16768b3288` | 3673 | 5946993 | 5946993 | 8 | `tier3`, `tier4`, `diverge` = `identical` |
| `blake3` | `O3_M1` | `3e164e858e13` | 3673 | 5947718 | 5947718 | 8 | `tier3`, `tier4`, `diverge` = `identical` |
| `cjson` | `O2` | `3064716c3277` | 7025 | 42829871 | 42829871 | 49 | `tier3`, `tier4`, `diverge` = `identical` |
| `cjson` | `O3_M1` | `47130201eae1` | 7025 | 45196449 | 45196449 | 49 | `tier3`, `tier4`, `diverge` = `identical` |
| `coremark` | `O2` | `13306ac88177` | 420 | 7046307 | 7046307 | 8 | `tier3`, `tier4`, `diverge` = `identical` |
| `coremark` | `O3_M1` | `852474ebded9` | 420 | 7045324 | 7045324 | 8 | `tier3`, `tier4`, `diverge` = `identical` |
| `crypto-algorithms` | `O2` | `32656ff9d794` | 6927 | 9543746 | 9543746 | 14 | `tier3`, `tier4`, `diverge` = `identical` |
| `crypto-algorithms` | `O3_M1` | `47fcfacf30de` | 6927 | 9586018 | 9586018 | 14 | `tier3`, `tier4`, `diverge` = `identical` |
| `fastlz` | `O2` | `fb3e65de9dc7` | 5400 | 66668367 | 66668367 | 10 | `tier3`, `tier4`, `diverge` = `identical` |
| `fastlz` | `O3_M1` | `5a38810b4466` | 5400 | 67415340 | 67415340 | 10 | `tier3`, `tier4`, `diverge` = `identical` |
| `heatshrink` | `O2` | `1affa9642ac4` | 26407 | 251243836 | 251243836 | 384 | `tier3`, `tier4`, `diverge` = `identical` |
| `heatshrink` | `O3_M1` | `324479c75539` | 26407 | 251470354 | 251470354 | 384 | `tier3`, `tier4`, `diverge` = `identical` |
| `inih` | `O2` | `6ad10536919c` | 3475 | 508866 | 508866 | 8 | `tier3`, `tier4`, `diverge` = `identical` |
| `inih` | `O3_M1` | `88f1662c5b81` | 3475 | 509412 | 509412 | 8 | `tier3`, `tier4`, `diverge` = `identical` |
| `jsmn` | `O2` | `3f8591f792a2` | 2033 | 422556 | 422556 | 6 | `tier3`, `tier4`, `diverge` = `identical` |
| `jsmn` | `O3_M1` | `480000eef007` | 2033 | 423010 | 423010 | 6 | `tier3`, `tier4`, `diverge` = `identical` |
| `kissfft` | `O1` | `a5a5555c6b1c` | 2695 | 3320205 | 3320205 | 31 | `tier3`, `tier4`, `diverge` = `identical` |
| `kissfft` | `O2` | `a5a5555c6b1c` | 2695 | 3320205 | 3320205 | 31 | `tier3`, `tier4`, `diverge` = `identical` |
| `kissfft` | `O3_M1` | `bb111ee2ade5` | 2695 | 3592495 | 3592495 | 31 | `tier3`, `tier4`, `diverge` = `identical` |
| `lua` | `O2` | `b45ed02e00e4` | 1122 | 50160670 | 50155783 | 34 | `tier3`, `tier4` = `identical` |
| `lua` | `O3_M1` | `6ad7d346c651` | 1122 | 49872764 | 49874519 | 34 | `tier3`, `tier4` = `identical` |
| `lz4` | `O2` | `8118b055d654` | 406 | 3862807 | 3862807 | 5 | `tier3`, `tier4`, `diverge` = `identical` |
| `lz4` | `O3_M1` | `782ffd55884f` | 406 | 3873269 | 3873269 | 5 | `tier3`, `tier4`, `diverge` = `identical` |
| `monocypher` | `O2` | `2621830e6b68` | 2512 | 661433816 | 661433816 | 9 | `tier3`, `tier4` = `identical` |
| `monocypher` | `O3_M1` | `94449ac42deb` | 2512 | 676877056 | 676877056 | 9 | `tier3`, `tier4` = `identical` |
| `picohttpparser` | `O2` | `399b906b9a0b` | 1898 | 200740 | 200740 | 6 | `tier3`, `tier4`, `diverge` = `identical` |
| `picohttpparser` | `O3_M1` | `3e6f712e1c95` | 1898 | 199695 | 199695 | 6 | `tier3`, `tier4`, `diverge` = `identical` |
| `pmull` | `O1` | `5855d764b421` | 83592 | 3840000 | 3840000 | 85 | `tier3`, `tier4`, `diverge` = `identical` |
| `pmull` | `O2` | `a14022046e6d` | 83592 | 3840000 | 3840000 | 85 | `tier3`, `tier4`, `diverge` = `identical` |
| `pmull` | `O3_M1` | `8385d8e61336` | 83592 | 3843916 | 3843916 | 85 | `tier3`, `tier4`, `diverge` = `identical` |
| `qoi` | `O2` | `1add69217f88` | 28460 | 332453318 | 332453318 | 174 | `tier3`, `tier4`, `diverge` = `identical` |
| `qoi` | `O3_M1` | `08d8363f99d8` | 28460 | 332215852 | 332215852 | 174 | `tier3`, `tier4`, `diverge` = `identical` |
| `quickjs` | `O2` | `6bfeae8d2ee9` | 2189 | 2765640233 | 2765640233 | 86 | `tier3`, `tier4` = `identical` |
| `quickjs` | `O3_M1` | `d684d1922934` | 2189 | 2735792321 | 2735792321 | 86 | `tier3`, `tier4` = `identical` |
| `sha3` | `O2` | `04c95483ca01` | 13420 | 12253541 | 12253541 | 18 | `tier3`, `tier4`, `diverge` = `identical` |
| `sha3` | `O3_M1` | `56265c3faff2` | 13420 | 11986842 | 11986842 | 18 | `tier3`, `tier4`, `diverge` = `identical` |
| `sqlite` | `O2` | `5f92e6b701d9` | 913 | 36121501 | 36121501 | 43 | `tier3`, `tier4` = `identical` |
| `sqlite` | `O3_M1` | `c0875981562d` | 913 | 36461106 | 36461106 | 43 | `tier3`, `tier4` = `identical` |
| `stb-image` | `O2` | `8e1fc4c93c1f` | 51938 | 262445384 | 262445384 | 248 | `tier3`, `tier4` = `identical` |
| `stb-image` | `O3_M1` | `38dcd61c2d91` | 51938 | 264076501 | 264076501 | 248 | `tier3`, `tier4` = `identical` |
| `stb-truetype` | `O1` | `eec5ce9c1932` | 1642 | 3759767 | 3759767 | 24 | `interpreter`, `tier3`, `tier4`, `diverge` = `identical` |
| `stb-truetype` | `O2` | `908df2fe8205` | 1642 | 3759767 | 3759767 | 24 | `interpreter`, `tier3`, `tier4`, `diverge` = `identical` |
| `stb-truetype` | `O3_M1` | `40060e0e84e3` | 1642 | 3890995 | 3890995 | 24 | `interpreter`, `tier3`, `tier4`, `diverge` = `identical` |
| `tinf` | `O2` | `ab9c66763e73` | 3383 | 12610224 | 12610224 | 8 | `tier3`, `tier4`, `diverge` = `identical` |
| `tinf` | `O3_M1` | `f6f3ba45cd4f` | 3383 | 12595638 | 12595638 | 8 | `tier3`, `tier4`, `diverge` = `identical` |
| `tiny-aes` | `O2` | `95a54aa3b8ca` | 5048 | 6381509 | 6381509 | 9 | `tier3`, `tier4`, `diverge` = `identical` |
| `tiny-aes` | `O3_M1` | `446a29b9d7cd` | 5048 | 6315670 | 6315670 | 9 | `tier3`, `tier4`, `diverge` = `identical` |
| `wasm3` | `O2` | `e168384305a5` | 727018 | 224870180 | 224870070 | 1277 | `tier3`, `tier4` = `identical` |
| `wasm3` | `O3_M1` | `151b1036adb2` | 727018 | 224874502 | 224874392 | 1277 | `tier3`, `tier4` = `identical` |
| `xxhash` | `O2` | `c1410a7746c2` | 3444 | 247701 | 247701 | 11 | `tier3`, `tier4`, `diverge` = `identical` |
| `xxhash` | `O3_M1` | `f5a647b75ed8` | 3444 | 247828 | 247828 | 11 | `tier3`, `tier4`, `diverge` = `identical` |
| `yyjson` | `O1` | `4279baefea5f` | 2196 | 634874 | 634874 | 31 | `interpreter`, `tier3`, `tier4`, `diverge` = `identical` |
| `yyjson` | `O2` | `fc1892b7040e` | 2196 | 634874 | 634874 | 31 | `interpreter`, `tier3`, `tier4`, `diverge` = `identical` |
| `yyjson` | `O3_M1` | `3f6a77ddc070` | 2196 | 637846 | 637846 | 31 | `interpreter`, `tier3`, `tier4`, `diverge` = `identical` |
| `zlib` | `O2` | `5cd573b0b94d` | 5190 | 313690664 | 313690664 | 786 | `tier3`, `tier4` = `identical` |
| `zlib` | `O3_M1` | `25c3519cef28` | 5190 | 312524737 | 312524737 | 786 | `tier3`, `tier4` = `identical` |
| `zstd` | `O1` | `001c7b7557ce` | 2534 | 42131511 | 42131511 | 135 | `interpreter`, `tier3`, `tier4`, `diverge` = `identical` |
| `zstd` | `O2` | `e5c56d8c48df` | 2534 | 42131511 | 42131511 | 135 | `interpreter`, `tier3`, `tier4`, `diverge` = `identical` |
| `zstd` | `O3_M1` | `fd8875dd946d` | 2534 | 42274258 | 42274258 | 135 | `interpreter`, `tier3`, `tier4`, `diverge` = `identical` |

## Reproducing Locally

```sh
cargo build --release --locked --features native-jit

python3 scripts/verify_linux_elf.py \
  --harness yyjson \
  --opt-matrix O1,O2,O3_M1 \
  --host-cc cc \
  --elf-cc "uv run --no-project --with ziglang==0.17.1 python3 -m ziglang cc -target aarch64-linux-musl" \
  --ounce target/release/ounce-core \
  --engine interpreter --engine tier3 --engine tier4 \
  --diverge \
  --budget 4000000000 \
  --work /tmp/verify-yyjson
```
