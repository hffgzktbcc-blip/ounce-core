# SVC coverage

What `src/kernel/svc.rs` does with every SVC that libnx declares (`nx/source/kernel/svc.s`, libnx master
b156b47, 2026-10-19: 128 SVCs). Semantics and result codes come from libnx `svc.h`/`result.h` and
Atmosphère's kernel (`kern_k_address_arbiter.cpp`, `kern_svc_thread.cpp`). Tests live in
`src/kernel/svc_tests.rs` unless noted.

Status: **done** = behaves per the docs for homebrew; **partial** = answers in the right shape but a
part is simplified (named in the note); **missing** = returns `SvcResult::Unhandled` (the run stops and
says so, nothing is faked).

## SVCs libnx itself calls (43, plus the ones homebrew calls directly)

| SVC | Name | libnx caller | Status | Notes / test |
|---|---|---|---|---|
| 0x01 | SetHeapSize | init.c | done | 2 MiB steps (InvalidSize), heap region bound (OutOfMemory); grow keeps contents, shrink unmaps. `set_heap_size_steps_keep_contents_and_shrink` |
| 0x02 | SetMemoryPermission | dynamic.c (RELRO) | partial | Accepted; permissions not modelled |
| 0x03 | SetMemoryAttribute | dynamic.c, map.c | partial | Accepted; attributes not modelled |
| 0x04 / 0x05 | MapMemory / UnmapMemory | thread.c (stack mirror), virtmem.c | done | Pages move and back. `map_memory_and_physical_memory` |
| 0x06 | QueryMemory | virtmem.c, thread.c, tmem.c, argv.c | partial | Whole mapped/free runs; state is only Normal or Free, perms RW or none |
| 0x07 | ExitProcess | applet.c, fatal.c | done | `identity_debug_and_process_svcs` |
| 0x08 / 0x09 | CreateThread / StartThread | thread.c | done | Priority/core checks; scheduler tests |
| 0x0A | ExitThread | thread.c | done | Last thread ends the process |
| 0x0B | SleepThread | newlib.c, many | done | 0 / -1 / -2 yields, timed sleep; scheduler tests |
| 0x0C-0x0F | Get/SetThreadPriority, Get/SetThreadCoreMask | newlib.c (core mask) | done | Scheduler tests |
| 0x10 | GetCurrentProcessorNumber | newlib.c | done | |
| 0x11 / 0x12 | SignalEvent / ClearEvent | event.c | done | InvalidHandle on unknown handles. `events_signal_clear_reset_and_bad_handles` |
| 0x13 / 0x14 | Map/UnmapSharedMemory | shmem.c | done | `shared_and_transfer_memory` |
| 0x15 | CreateTransferMemory | tmem.c | partial | Handle only; source pages keep their permissions |
| 0x16 | CloseHandle | many | partial | Always succeeds, even for unknown handles |
| 0x17 | ResetSignal | event.c, wait.c | done | |
| 0x18 | WaitSynchronization | wait.c | done | Threads, events, service events; honours CancelSynchronization. Without known handles it is still the demos' VSync |
| 0x19 | CancelSynchronization | wait.h | **done (new)** | Wakes a waiter with Cancelled, else arms the next wait. `cancel_synchronization_wakes_or_arms` |
| 0x1A-0x1D | ArbitrateLock/Unlock, WaitProcessWideKeyAtomic, SignalProcessWideKey | mutex.c, condvar.c | done | Scheduler tests |
| 0x1E | GetSystemTick | (armGetSystemTick reads CNTPCT) | done | |
| 0x1F | ConnectToNamedPort | sm.c | done | |
| 0x21 | SendSyncRequest | service.h, tipc.h | done | HIPC/CMIF dispatch in ipc.rs (its own tests) |
| 0x22 | SendSyncRequestWithUserBuffer | svc.h | done | Requires a page-aligned, nonzero, page-multiple RW mapped range; uses it as the HIPC request/response area and leaves TLS separate. Kernel IPC page locking/state transitions are not modeled. `sync_request_with_user_buffer_uses_the_supplied_hipc_area` |
| 0x24 / 0x25 | GetProcessId / GetThreadId | | done | |
| 0x26 | Break | diag.c, fatal.c | done | |
| 0x27 | OutputDebugString | console_debug.c | done | |
| 0x28 | ReturnFromException | | partial | Exceptions are not delivered, so this stops like Break |
| 0x29 | GetInfo | init.c, virtmem.c, newlib.c, random.c, detect.h | **done (fixed)** | Types 0-8, 11-18, 21-23, 28. Types 16/17 were mislabelled as 20/21; 21/22/23/28 are new. `get_info_answers_every_type_libnx_asks_for` |
| 0x2A / 0x2B | FlushEntire/FlushDataCache | | done | Memory is coherent |
| 0x2C / 0x2D | Map/UnmapPhysicalMemory | | **done (fixed)** | Unmap used to be a no-op |
| 0x32 | SetThreadActivity | thread.c (threadPause/Resume) | **done (new)** | Busy for the caller, InvalidState for no-op transitions. `thread_activity_pauses_and_context_reads_registers` |
| 0x33 | GetThreadContext3 | thread.c (threadDumpContext) | **done (new)** | Paused threads only; FPCR/FPSR read 0 |
| 0x34 | WaitForAddress | levent.c | **done (fixed)** | Was "return success": now all 4 arbitration types, InvalidState / TimedOut / real blocking. `wait_for_address_checks_value_alignment_and_type` |
| 0x35 | SignalToAddress | levent.c | **done (fixed)** | Now wakes waiters (priority order) and updates per type; mismatch → InvalidState. `signal_to_address_*` |
| 0x45 | CreateEvent | event.c | done | |
| 0x4B / 0x4C | CreateCodeMemory / ControlCodeMemory | jit.c (JitType_CodeMemory) | missing | Needs two views of the same pages; the MMU has no aliasing. Not hinted, so libnx uses the 0x73/0x77/0x78 path |
| 0x50 | CreateSharedMemory | shmem.c | **done (new)** | |
| 0x51 / 0x52 | Map/UnmapTransferMemory | tmem.c | **done (new)** | Maps a fresh view (single process) |
| 0x73 | SetProcessMemoryPermission | jit.c | partial | Accepted; permissions not modelled |
| 0x77 / 0x78 | Map/UnmapProcessCodeMemory | jit.c | **done (fixed)** | Used to map zeroed pages at dst; now moves src pages there and back. `process_code_memory_moves_jit_pages` |

## Everything else in svc.s

| SVC | Status |
|---|---|
| 0x41 AcceptSession, 0x43 ReplyAndReceive | partial: scripted for the Stage 10 ro:1 harness only |
| 0x74 / 0x75 MapProcessMemory / UnmapProcessMemory, 0x76 QueryProcessMemory | partial: legacy RO/FS path (maps fresh pages / fixed answer) |
| 0x20, 0x23 (light/asynchronous user-buffer IPC), 0x2E, 0x2F, 0x30, 0x31, 0x36, 0x37, 0x39, 0x3A, 0x3C, 0x3D, 0x40, 0x42, 0x44, 0x46-0x4F, 0x53-0x72, 0x79-0x7F, 0x90, 0x91 | missing (Unhandled). Sysmodule, debug, device and process-management calls; libnx never calls them on its own |

## Known gaps worth doing next

- Code memory (0x4B/0x4C) needs page aliasing in `mmu`.
- CloseHandle should return InvalidHandle for unknown handles (needs one handle table across ipc/sync/scheduler).
- QueryMemory reports every mapped page as Normal/RW (no Code/Static/Stack states or real permissions).
