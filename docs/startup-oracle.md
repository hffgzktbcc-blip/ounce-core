# First-frame oracle: `startup` (`templates/application`)

Status: **oracle defined for switch-examples `templates/application`.**
Sample: upstream `switchbrew/switch-examples` pinned commit `669786898205b7beb25ff1731e72982e6d0397d3`, `templates/application` (pinned in `tests/corpus/manifest.json`).

## Frame content & behavior

From `source/main.c`:
- Calls `consoleInit(NULL)` which configures a standard libnx text console in R5G6B5 format at 1280x720.
- Prints:
  ```text
  Hello World!
  ```
- Submits console frames via `consoleUpdate(NULL)` in an event loop while checking `appletMainLoop()`.
- Captured frame resolution: 1280x720 RGBA8 (expanded from R5G6B5Unorm).
- Both Interpreter and Tier-3 JIT engines must execute and capture identical rendered text frames.

## Pass criteria (all required)

1. Unchanged upstream source build from `669786898205b7beb25ff1731e72982e6d0397d3`.
2. Libnx text console initializes successfully (`consoleInit(NULL)`).
3. At least 5 frames captured from the submitted VI buffer at 1280x720 RGBA8.
4. Non-black text pixels rendered across the frame representing "Hello World!".
5. Bit-exact agreement between Interpreter and Tier-3 JIT engines.
6. Clean termination on queued frame limit.
