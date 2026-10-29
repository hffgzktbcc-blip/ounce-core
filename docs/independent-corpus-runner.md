# Independent homebrew corpus: runner design

Review baseline: `fb80e573fc9888da456c1b102e5dc4b829c1ece4`, 2026-10-29.
This is an implementation plan, not a compatibility result. No external NRO was built or run
for this review, and no Switch 2 compatibility follows from the proposed original Switch tests.

## Current execution paths

`src/main.rs::run_file` loads an NRO through `BootLoader::load_any_package`, calls
`setup_homebrew_nro`, and enters `run_vm_internal`. That loop always calls `Cpu::step`.
The existing `--cpu=tier3|tier4` option selects the engine for harness Stage 20 only. Passing
that option to `run example.nro` does **not** establish Tier-3 or Tier-4 NRO execution.

The best existing test seam is `src/loader/fixture_tests.rs::boot_with_kernel_hid_sdmc`.
It uses the same VM/homebrew setup and the real kernel, permits a fixed HID state and a
host SD-card folder, captures SVC 0x27 debug strings, and checks termination. It is also
interpreter-only. Its current assertions reject `PresentFrame`, so it cannot directly run
an interactive display sample unchanged.

## Smallest useful runner

Add an opt-in external-corpus runner, initially inside the existing crate test modules so
it can reuse private setup functions. Read NRO paths from an explicit local corpus directory;
do not silently download, rebuild or modify samples during an execution test. Keep external
programs unmodified, record their source revision and NRO SHA-256, and use a fresh VM for
each sample/engine pair. Missing NROs must produce an explicit not-run result, never a pass.

Use a local runner engine enum with `Interpreter` and `Tier3(Tier3JitEngine)`; do not infer
the engine from the current global Stage-20 option. Record both requested and actual engine.
Use the same kernel SVC dispatch for both paths. In the Tier-3 path, consume every returned
`BlockExit`: dispatch `Svc`, continue at normal branch/return boundaries, end a slice on
`BudgetExceeded`, and report `Fault` with PC and instruction bytes where readable. A return
from a guest function is not a process exit. Do not convert a fault into interpreter success.

Bound instruction count, host elapsed time, captured frames and trace/log storage. A host
deadline prevents wall-clock sleeps or service stalls from defeating an instruction budget.
`Tier3JitEngine::run_slice` currently executes a whole cached super-block and can exceed its
requested instruction limit. Treat that limit as a slice target unless the engine is changed
to enforce a hard boundary; record the actual count and do not claim instruction-exact limits.
Preempt after crossing a scheduler quantum, rather than testing `executed % quantum == 0`,
because a block can cross the quantum without landing exactly on it. SVC dispatch can change
the current guest thread and its registers; resume from the post-dispatch CPU state.

Do not instantiate physical gamepad, host window or audio-device backends in this runner.
Apply an explicit input sequence at recorded guest checkpoints, for example after a captured
frame. An immediate held Plus button can make a sample exit before exercising its main loop.
Wall-clock HID polling is unsuitable for repeatable test input. Scheduler and time-service
clocks still use host time, so claim deterministic assertions only for observables that do
not depend on timing until a controlled guest clock exists.

## Display and console blocker

The checked-in display model is not yet a libnx window-system implementation. In
`src/kernel/ipc.rs`, `Vi` implements only command 2 and `ApplicationDisplayService` implements
only `OpenDisplay` and a synthetic `CreateStrayLayer` response. Other commands return a
generic success-shaped response. There is no Binder relay / parcel / producer buffer-queue
path. This is a likely blocker for independent examples that call `consoleInit`,
`nwindowGetDefault` or use swapchain presentation; the first real run must identify the actual
failing operation. Generic success responses are not evidence that those services worked.

`printf` in the usual libnx console writes pixels, not SVC 0x27 debug text. Searching captured
debug strings for its text is therefore an invalid success oracle for an unmodified example.
Do not redirect or patch external sample output and still label that run unmodified.

There is already a useful consumer:
`gpu::present::surface::frame_from_surface(mmu, address, surface)`. It reads pitch or block-linear
RGBA/BGRA surfaces and converts them into a packed frame. It needs a real descriptor producer
from the VI/Binder queue; addresses guessed from the heap are not a substitute.

`OUNCE_DUMP_FRAMES` in `HeadlessBackend` dumps frames passed to the existing host presenter.
The regular `DisplayServer::composite` reads a configured fixed framebuffer and adds telemetry.
Neither proves that a libnx console/swapchain image was captured. Corpus assertions should
use raw guest frames before telemetry, record dimensions/format/layout, and compare fixed
pixels or an approved reference hash. Keep reference-system captures distinct from
runtime-generated baselines; two runtime engines agreeing is useful but not hardware validation.

## Result record

Use a machine-readable record for every attempted sample/engine pair with these fields:

| Field | Required content |
| --- | --- |
| Identity | Sample ID, upstream URL, exact revision and relative source path |
| Build | Toolchain/package versions, build command, NRO SHA-256, build log reference |
| Runtime | Ounce-Core commit, host OS/architecture, requested and actual engine |
| Controls | Instruction/slice limits, host timeout, input/checkpoint sequence |
| Observations | Actual instructions, final PC/X0, SVC trace, debug text, IPC/ioctl diagnostics, frame count |
| Assertion | Expected observable, its source, actual observable, explicit pass/fail/not-run |
| Termination | Stop reason and details, independent of the assertion verdict |
| Evidence | Raw frame/log artifact names and hashes; external reproduction status |

Stop reasons should distinguish `exit_process`, `svc_break`, `unhandled_svc`, `cpu_halt`,
`memory_or_instruction_fault`, `instruction_budget`, `host_timeout`, `frame_limit`,
`checkpoint_complete`, `load_error`, and `missing_input`. A process exit is not automatically
a pass. An interactive sample stopped after an asserted checkpoint is not a graceful exit.
Collect unsupported IPC/ioctl diagnostics even if the service returns a success-shaped reply.

## Verification sequence

1. Test runner accounting, fault handling, SVC dispatch and preemption with existing synthetic
   programs; then run existing project-authored NRO fixtures under both engines. This checks
   runner plumbing and does not count toward the independent corpus.
2. Build the pinned external samples locally with recorded toolchain inputs. Run the unchanged
   NROs under the interpreter first and record the first unsupported behavior without weakening
   the expected result.
3. Implement only the missing behavior demonstrated by that run, with regression tests. A
   VI/Binder implementation requires its own bounded parcel/descriptor parsing and queue tests.
4. Run the same NRO bytes under Tier-3 and compare expected observables and diagnostics.
5. Ask an outside developer to reproduce the documented build and execution from a clean
   checkout before marking the roadmap milestone complete.

Rust changes require the repository verification gates and all nine configured CI jobs before
merging. This design document makes no claim that those new runner checks have run.
