# Menu-to-Triangle interaction oracle

Registered before guest execution on 2026-11-07. This is separate from the
50-program first-frame headline and never changes its existing expectations.

Source: switchbrew/switch-examples at
`669786898205b7beb25ff1731e72982e6d0397d3`, already pinned and credited by
the external corpus. Read `graphics/deko3d/deko_examples/source/main.cpp`,
`Example02_Triangle.cpp`, `basic_vsh.glsl` and `color_fsh.glsl` at that revision.
No upstream source or guest bytes are patched.

After two neutral menu queues, press libnx Down (bit 15), release after the
next queue, then press A (bit 0) after queue 4. A leaves the menu without
queuing another console frame. Release after queue 5, the first Triangle
queue. Check queues 6–8, exercising both framebuffer slots. Bound host time
and guest instructions; failure to reach any transition or checked frame fails.

Triangle's vertices are (0,+1), (-1,-1), (+1,-1), with red, green and blue
attributes respectively. Its shaders pass position and color unchanged.
Viewport is 1280×720, RGBA8 UNORM, with black clear. At pixel centers (x+.5,y+.5),
using the deko3d positive-Y-up viewport convention:
`r=1-(y+.5)/720`, `b=((y+.5)/720+(x+.5-640)/640)/2`, `g=1-r-b`.
Round each channel times 255 to nearest. Thus (640,480) is (85,85,85),
and (320,600) is (42,170,43). (100,100) and (1180,100) lie outside the
triangle and must remain black. These values come from upstream geometry,
not captured emulator output. Never adjust them to fit a run.

Both Interpreter and Tier-3 must match the probes and agree on every queued
frame hash. Host-GPU qualification additionally requires completed draws and
readbacks, retains adapter diagnostics, and distinguishes software adapters
from physical hardware. A successful CPU path alone cannot prove host GPU
execution. Retain failures as well as passes.

## Initial execution (2026-11-07, not a passing oracle)

The pre-registration commit is `8f62d2f`. The existing CI-built unchanged
NRO came from `corpus-nros` artifact `13533340959` in run `42274752750`.
Downloaded ZIP SHA-256 matches GitHub's artifact digest:
`26f56fda050e637fa3eced3551da9d878e3e632607d374bdc97affcaf53c162c`.
The reports retain the individual NRO SHA-256.

Both engines applied all five inputs at exactly the registered queue boundaries
and captured identical hashes for queues 1–8. The CPU path and host API path
also matched. The host path used llvmpipe (LLVM 19.1.7), OpenGL, CPU adapter,
`physical=false`: four completed draws and 14,745,600 readback bytes per engine,
with no GPU diagnostics. This is software-adapter API execution, not physical
GPU proof.

The oracle nevertheless **failed**: the (320,600) probe was registered with
an arithmetic error. Exact substitution into the formula above yields
`r=239/1440`, `g=15359/23040`, `b=3857/23040`, which rounds to **(42,170,43)**.
The original registered value is (42,139,74). Both reports retain that failure:
[CPU evidence](verify/menu-triangle-before.json) and
[host API evidence](verify/menu-triangle-host-before.json).
The owner directed “Solve and fix” after the correction was presented on
2026-11-07, authorizing the arithmetic correction to (42,170,43). The original
failed reports remain unchanged. `scripts/test_interaction_oracle.py` independently
recomputes every registered probe using exact rational barycentric coordinates;
it failed against the original value before the correction. Fresh guest execution
against the corrected registration is required before claiming an oracle pass.

The local repository gate passed: 475 tests, zero failures, and the runtime
suite passed. The unchanged first-frame corpus rerun retained **46/50** passes;
only the existing account, audio-echo, playtone and save-data cases failed.
This local regression observation does not replace hosted CI qualification.

## Corrected-registration replay (2026-11-07)

After the recorded owner decision, fresh runs of the **same NRO bytes** passed
the corrected probes on Interpreter and Tier-3 with identical complete frame
hash sequences. Both the CPU path and llvmpipe host path passed. The host path
again completed four draws and 14,745,600 readback bytes per CPU engine, with
zero GPU diagnostics. Reports: [CPU replay](verify/menu-triangle-after.json),
[host replay](verify/menu-triangle-host-after.json). The original failed reports
remain available above; the headline corpus remains unchanged. Physical GPU
qualification is separately exercised by the Mac interaction CI job.
