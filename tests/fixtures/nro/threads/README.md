# threads.nro: libnx threads fixture

Homebrew-only test program built with devkitA64 and libnx, like `../hello`.

- Two workers (`threadCreate` with a libnx-allocated stack, `threadStart`) each add their ID to a shared
  total 300 times under a libnx `Mutex`. Every tenth time they yield (`svcSleepThread(0)`) while holding
  it, so the other worker blocks in `svcArbitrateLock` and gets the mutex handed over on unlock.
- When done, each signals a `CondVar` the main thread waits on (`svcWaitProcessWideKeyAtomic` /
  `svcSignalProcessWideKey`), and the main thread joins both with `threadWaitForExit`
  (`svcWaitSynchronization` on the thread handles).
- Each thread also checks its own `__thread` variable (separate thread-local regions at `TPIDRRO_EL0`).
- A third thread busy-waits on a flag without SVCs. `threadStart` runs it first, so the main thread only
  gets to set the flag if the kernel preempts the spinner (bit 5).

It prints `ounce-threads total=0x00000384` (900 = 300 x (1 + 2)) and `ounce-threads result=0x00000EEE`
when every check passes, else `0xBAD0000 | bits`.

Run: `cargo run --release -- run tests/fixtures/nro/threads/threads.nro`.
Test: `libnx_threads_fixture_synchronizes_two_workers`. Rebuild: `make -C tests/fixtures/nro/threads clean all`.
