# Independent corpus: filesystem candidate

Status: source review only, 2026-10-29. No candidate binary was built or executed. This records a proposed original Switch homebrew trial, not Switch 2 compatibility evidence.

## Candidate and provenance

Use the unmodified `fs/romfs` example from `switchbrew/switch-examples`, pinned to revision `669786898205b7beb25ff1731e72982e6d0397d3`:

- [Source](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/fs/romfs/source/main.c)
- [Makefile](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/fs/romfs/Makefile)
- [Upstream README and Public Domain Mark](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/README.md)

The upstream README displays a Public Domain Mark; it does not provide CC0 legal-code text. Record that statement accurately. The linked libnx, newlib, compiler-runtime and packaging components need a separate inventory before redistributing the NRO. Start with an external fetch-and-build recipe rather than checking in a binary. This note is not a completed license audit.

The program was authored independently of Ounce-Core. Keep its source unchanged so the result tests an external program rather than another project-specific fixture.

## Expected behavior

The program initializes the console and default input layout, calls `romfsInit`, reads two embedded files using stdio, displays an EOF marker after each, then waits for Plus and returns zero after console/RomFS cleanup.

| Input | Pinned Git blob SHA | Expected observation |
| --- | --- | --- |
| `romfs/folder/file.txt` | `7c81319e963b6b1e3916357680f02e96dbd43a6f` | Complete fixed English file content, followed by EOF marker |
| `romfs/フォルダ/ファイル.txt` | `370c85befc508b572b46e188b59f15ace2e3a25d` | Complete fixed two-line content, followed by EOF marker |
| `source/main.c` | `ef29d6e3b3158d75c0fcef414324295f063e0e80` | Normal Plus-triggered exit with guest result zero |

Pass criteria also require no unhandled instruction, SVC, IPC command or memory fault. A budget stop is not a successful exit. File reads exercise the embedded RomFS, NRO access through `fsp-srv`, and a non-ASCII filename. Existing project-authored RomFS tests are useful prerequisites, not independent evidence.

## Build recipe

Prerequisites: devkitPro devkitA64, libnx and switch-tools, `DEVKITPRO` set, and the compiler/tools on `PATH`. The Makefile uses libnx `switch_rules`/`switch.specs`, links `-lnx`, and embeds its `romfs` directory.

```bash
git clone https://github.com/switchbrew/switch-examples.git
git -C switch-examples checkout --detach 669786898205b7beb25ff1731e72982e6d0397d3
make -C switch-examples/fs/romfs
sha256sum switch-examples/fs/romfs/romfs.nro
```

Record exact tool/package versions, build log, source revision, ELF/map/NRO hashes and component inventory. The project's previously recorded devkitA64 r30/libnx 4.12.0 is a possible starting toolchain, not a verified build of this candidate.

## Runner gaps and execution plan

At the reviewed Ounce-Core main revision `fb80e573fc9888da456c1b102e5dc4b829c1ece4`, `--cpu=tier3`/`--cpu=tier4` selects the Stage 20 engine; `run_file` still calls the interpreter runner. Passing that flag to an NRO invocation does not validate Tier-3 NRO execution. The roadmap's interpreter/Tier-3 requirement needs an engine-selectable NRO harness first.

Upstream `printf` writes to its on-screen console, so expected text is not automatically a host debug-log oracle. Do not modify the candidate to emit project-specific debug markers. The harness should:

1. Boot the unchanged NRO under a bounded instruction/frame budget.
2. Observe successful reads and EOF for both exact paths externally; capture the completed console frame as supporting evidence.
3. Inject neutral input, then a Plus press edge through the existing host input feed after output is complete.
4. Assert normal guest exit and result zero, retaining diagnostics and binary hashes.
5. Repeat the same binary and inputs under interpreter and Tier-3 on Linux, then request an outside developer's clean-checkout reproduction.

Do not mark the corpus milestone complete until both engine runs and the outside reproduction exist. No runtime compatibility result is claimed here.

A later input-focused option is the [pinned `hid/read-controls` example](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/hid/read-controls/source/main.c): scripted press/hold/release and analog values provide observable behavior, but require frame synchronization and console observation.
