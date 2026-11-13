# Independent corpus: startup and CPU candidate

Source inspection dated 2026-10-29. This is a proposed trial, not a passing
compatibility result. No external source or compiled binary is included here.

## Pinned candidates

Repository: `switchbrew/switch-examples`, commit
`669786898205b7beb25ff1731e72982e6d0397d3`.

- Startup baseline: [`templates/application/source/main.c`](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/templates/application/source/main.c).
- CPU and generated-code trial: [`jit/source/main.c`](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/jit/source/main.c).
- Build definition: [`jit/Makefile`](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/jit/Makefile).

These examples were authored outside Ounce-Core. Their upstream authorship was
not individually audited; selecting several directories from one repository does
not itself establish three distinct authors.

## Source and binary terms

The pinned [root README](https://github.com/switchbrew/switch-examples/blob/669786898205b7beb25ff1731e72982e6d0397d3/README.md)
marks the examples public domain using Public Domain Mark 1.0. No contradictory
file-level notice was observed in the reviewed candidate source. This records
upstream's rights assertion, not a completed legal audit.

The Makefile links libnx and toolchain runtime components. Source terms alone
do not determine the redistribution terms of the resulting NRO. Initially fetch
and build outside the repository; record linked-component versions and notices
before considering binary inclusion. Do not treat this document as permission to
copy external source into the implementation.

## Reproducible build proposal

Prerequisites: GNU make, devkitA64, libnx and Switch tools including `elf2nro`
and `nacptool`, with `DEVKITPRO` pointing at the installation.

```bash
git clone https://github.com/switchbrew/switch-examples.git
git -C switch-examples checkout --detach 669786898205b7beb25ff1731e72982e6d0397d3
make -C switch-examples/templates/application NO_ICON=1
make -C switch-examples/jit NO_ICON=1
```

Expected outputs are `templates/application/application.nro` and `jit/jit.nro`.
Record the exact package versions, compiler version, complete build command,
build log and SHA-256 of each NRO and ELF. The source revision alone does not
pin the compiler, libnx or runtime libraries. These commands have not been run
for this trial; no build success or output hash is claimed.

## Expected observations

The unmodified startup template initializes the console and controller, prints
`Hello World!`, waits in `appletMainLoop`, and returns zero after a Plus press.
It is a startup baseline, not a broad CPU correctness test.

The unmodified JIT example requests a 1 MiB generated-code region, writes the
AArch64 sequence `mov x0, #7; ret`, makes it executable and calls it. A successful
run must show:

- `jitCreate() returned: 0x0`;
- `jitTransitionToWritable() returned: 0x0`;
- `jitTransitionToExecutable() returned: 0x0`;
- `Test code returned: 0x7`;
- `jitClose() returned: 0x0`.

It then waits for Plus and returns zero. Pointer values and the selected JIT type
must be recorded, not compared to invented fixed addresses. Console text is
rendered to the guest framebuffer and is not necessarily available on host
stdout. A test adapter needs framebuffer observation or an ELF/disassembly-based
observation of the generated function's actual return value. A host process exit
of zero, a blank frame, or an instruction-budget stop is not a passing result.

## Source-level blockers in the inspected runtime

Inspection baseline: Ounce-Core
`fb80e573fc9888da456c1b102e5dc4b829c1ece4`.

### Loader does not advertise a usable JIT environment

The inspected [libnx JIT implementation](https://github.com/switchbrew/libnx/blob/b156b4774fb2df7b93402169509266da7547fc4c/nx/source/kernel/jit.c)
requires either syscall hints for `0x4B`/`0x4C`, or hints for
`0x73`/`0x77`/`0x78` plus a valid own-process handle. Without these it returns
`LibnxError_JitUnavailable` before executing generated code.

Ounce-Core's [HomebrewConfig](https://github.com/hffgzktbcc-blip/ounce-core/blob/fb80e573fc9888da456c1b102e5dc4b829c1ece4/src/loader/homebrew.rs)
defines syscall-hint and process-handle entry constants but exposes neither as
configuration fields nor emits those entries. `setup_homebrew_nro` supplies
thread handle, heap and argv. The external JIT example is therefore an expected
failure candidate at this baseline, pending an actual run; it is not evidence
that generated-code compatibility already works.

If adding loader capabilities, advertise only verified, usable SVC paths. Do not
hint unsupported CodeMemory calls to force the example past this check. Validate
the process-handle semantics and memory transitions as well as the returned
function value.

### NRO CLI execution is interpreter-only

At the inspected baseline, [`run_file` and `run_vm_internal`](https://github.com/hffgzktbcc-blip/ounce-core/blob/fb80e573fc9888da456c1b102e5dc4b829c1ece4/src/main.rs)
execute external NROs using `cpu.step`. The globally accepted `--cpu=tier3`
argument does not select Tier-3 for that path. Passing the flag alone cannot
establish the roadmap's Tier-3 corpus result.

The trial adapter must explicitly select interpreter or Tier-3, preserve kernel
dispatch and scheduling, inject input deterministically, impose an instruction
budget, and report the actual stop reason and guest result. Keep initial input
idle until the sample reaches its loop, then inject Plus; holding Plus from
startup can skip observable frames or make a test depend on startup timing.

## Trial order and acceptance

1. Build the unchanged startup template and JIT sample with recorded inputs.
2. Run the startup template under the interpreter; capture its message and a
   clean guest exit after deterministic input.
3. Record the unchanged JIT sample's initial behavior, including any failure
   result; preserve this evidence before changing runtime capabilities.
4. Add and regression-test truthful loader capabilities if required.
5. Run the same unchanged binaries under interpreter and Tier-3 with explicit
   engine selection and the observations above.
6. Have an outside developer reproduce from a clean checkout before marking the
   independent-corpus milestone complete.

Original Switch homebrew results exercise shared foundations. They do not
establish Switch 2 ABI, graphics, service or commercial-software compatibility.
