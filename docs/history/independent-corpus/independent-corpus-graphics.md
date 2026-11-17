# Independent graphics corpus candidate

Reviewed 2026-10-29 against Ounce-Core commit
`fb80e573fc9888da456c1b102e5dc4b829c1ece4`. This is a source-inspection plan;
the external example has not been built or executed in this investigation.
No compatibility result is claimed.

## Selected candidate and source pin

Use the unmodified `graphics/simplegfx` example from
[`switchbrew/switch-examples`](https://github.com/switchbrew/switch-examples/tree/669786898205b7beb25ff1731e72982e6d0397d3/graphics/simplegfx)
at commit `669786898205b7beb25ff1731e72982e6d0397d3`.

- [Guest source](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/graphics/simplegfx/source/main.c),
  blob `412fc76003bd590c01071d70961a3ff60e78fb69`.
- [Build recipe](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/graphics/simplegfx/Makefile).
- [Repository declaration](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/README.md)
  describes examples for devkitA64/libnx and displays a Public Domain Mark.

This example was authored outside Ounce-Core and does not depend on Ounce-Core
result constants. It exercises CPU-written framebuffer presentation rather than
shader execution. Passing it would establish only the tested original-Switch
framebuffer path, not GPU shader or Switch 2 compatibility.

## Build and fixture-use record

The repository's declared Public Domain Mark is the source-use evidence found;
no root LICENSE file was present in the inspected listing. A Public Domain Mark
is a status declaration, not a substitute for recording the linked binary's
components. Review libnx, newlib, compiler runtime and any packaging assets
separately before redistributing a compiled fixture. Keep this as an external
build initially; do not vendor upstream source or NRO bytes merely to select it.

Use an installed devkitA64/libnx toolchain with `DEVKITPRO` set and its tools on
PATH. The example links `-lnx` and includes `$(DEVKITPRO)/libnx/switch_rules`.
No deko3d, UAM, shader asset, firmware, key or image asset is needed in the default
configuration. Do not define `DISPLAY_IMAGE`.

From a separate external checkout:

```sh
git clone https://github.com/switchbrew/switch-examples.git
git -C switch-examples checkout --detach 669786898205b7beb25ff1731e72982e6d0397d3
make -C switch-examples/graphics/simplegfx NO_ICON=1 NO_NACP=1
sha256sum switch-examples/graphics/simplegfx/simplegfx.nro
```

`NO_ICON=1 NO_NACP=1` uses upstream build switches to omit package metadata;
it does not modify guest source. Record these switches alongside the compiler,
devkitA64, libnx, newlib and switch-tools package versions, full build log, NRO
SHA-256 and source-tree cleanliness. The versions already used by Ounce-Core's
project fixtures are a starting point, not a verified successful build pin for
this external sample.

## Deterministic checkpoints

The source creates a 1280 x 720 RGBA8888 double-buffered framebuffer and requests
its linear CPU view. With neutral controller input, every visible pixel in each
completed frame is the same 32-bit word. The counter increments before drawing:

| Completed frame | Counter | Guest pixel word | Raw RGBA bytes |
| --- | --- | --- | --- |
| 1 | 1 | `0x04040404` | `04 04 04 04` |
| 2 | 2 | `0x08080808` | `08 08 08 08` |
| 60 | 60 | `0xF0F0F0F0` | `F0 F0 F0 F0` |
| 61 | 0 | `0x00000000` | `00 00 00 00` |

For frame number `n >= 1`, the counter is `n % 61`, so the expected word is
`0x01010101 * 4 * (n % 61)`. Capture after the guest's `framebufferEnd` has
completed its submission, not merely when a framebuffer allocation or a VSync
wait occurs. Validate all 1280 x 720 visible pixels using the real row stride;
exclude allocation padding. A first-frame check is the initial bounded trial;
the 60/61 wraparound check is useful only after the path works.

The original sample runs until Plus or an applet exit. Preserve that source:
after observing the chosen complete frame, inject a Plus press through the host
input feed and require a clean guest return. A budget stop, arbitrary fixed
instruction count or black host window is not success. Record instruction budget,
last PC, stop reason, frame count and the first unimplemented service or opcode
on failure. Do not replace upstream behavior with an Ounce-Core success marker.

## Current runtime blockers and capture constraints

These are source-inspection findings, not observed failures of this sample:

1. `src/kernel/ipc.rs` currently models VI with a small `GetDisplayService` /
   `OpenDisplay` / `CreateStrayLayer` path. The inspected service enum has no
   Binder relay service and the file has no buffer-queue transaction decoder.
   The sample uses libnx `nwindow` / `framebuffer`, so a real VI/Binder producer
   bridge is likely required before its first frame can be submitted. Record
   the actual first failure before choosing an implementation change.
2. `DisplayServer` reads the fixed synthetic framebuffer at `0x90000000`.
   Independently allocated libnx framebuffers must be resolved from submitted
   buffer metadata; mapping an unrelated synthetic framebuffer is not evidence.
3. `DisplayServer::composite` forces alpha to `0xFF` and draws a telemetry HUD.
   Capture raw guest-visible pixels before those operations for the table above.
   A host screenshot has different alpha and overlay pixels and needs a separate,
   explicit comparison policy. Do not generate a golden image from Ounce-Core
   output and use that same image as the sole correctness oracle.
4. `run_file` uses `run_vm_internal`, whose instruction loop calls `cpu.step`.
   Current `--cpu=tier3|tier4` help explicitly limits selection to Stage 20.
   Therefore adding `--cpu=tier3` to an NRO command does not demonstrate a
   Tier-3 NRO run. A tested guest runner with real tier selection is needed for
   the roadmap's interpreter/Tier-3 requirement.
5. The CLI does not expose a bounded frame capture for `run` in the inspected
   code. Use a test runner that observes the real submitted frame, handles SVCs
   and schedules guest threads; avoid counting unrelated synchronization calls
   as completed graphics frames.

After those paths exist, run the identical NRO under the interpreter and Tier-3
on Linux. Preserve raw frame hashes, expected-pixel checks, logs and exit results.
Ask an outside developer to reproduce the pinned build/run from a clean checkout
before marking the independent-corpus milestone complete.

## Later GPU sample

The same upstream pin contains
[`graphics/deko3d/deko_basic`](https://github.com/switchbrew/switch-examples/tree/669786898205b7beb25ff1731e72982e6d0397d3/graphics/deko3d/deko_basic),
which draws a static triangle over a fixed clear color. It is a later candidate:
it adds shaders/RomFS, deko3d, swapchain acquisition/presentation and framebuffer
compression/layout requirements. Ounce-Core's existing offscreen project-authored
deko3d fixture does not establish that this upstream presentation path works.
