# First-frame oracle: `romfs` (written before execution)

Status: **oracle defined for switch-examples `fs/romfs`.**
Sample: upstream `switchbrew/switch-examples` pinned commit `669786898205b7beb25ff1731e72982e6d0397d3`, `fs/romfs` (already pinned in `tests/corpus/manifest.json`).

## Frame content & behavior

From `source/main.c`:
- Calls `consoleInit(NULL)` which sets up a standard libnx text console in R5G6B5 format at 1280x720.
- Initializes romfs via `romfsInit()`.
- Prints:
  ```text
  romfs Init Successful!
  ```
  Followed by contents of `romfs:/folder/file.txt`:
  ```text
  Hello World!
  >>EOF<<
  ```
  Followed by contents of `romfs:/フォルダ/ファイル.txt`:
  ```text
  This is a file in a folder with non-ASCII characters.
  >>EOF<<
  ```
- Submits console frames via `consoleUpdate(NULL)` in an event loop.
- Captured frame resolution: 1280x720 RGBA8 (expanded from R5G6B5Unorm).
- Both Interpreter and Tier-3 JIT engines must execute and capture identical rendered text frames.

## Pass criteria (all required)

1. Unchanged upstream source build.
2. Libnx RomFS initializes successfully (`romfsInit()` returns 0).
3. At least 5 frames captured from the submitted VI buffer at 1280x720 RGBA8.
4. Non-black text pixels rendered across the frame representing the console text output.
5. Bit-exact agreement between Interpreter and Tier-3 JIT engines.
6. Clean termination on queued frame limit.
