#!/usr/bin/env python3
"""Run one conformance trace through Unicorn and report the adapter counts.

An outside emulator (Unicorn, QEMU's TCG) executed against the same hardware-recorded
one-instruction traces that Ounce-Core's engines run. See conformance/arm64-v0.1.0/README.md
for the adapter contract and trace format.

  pip install unicorn
  python3 scripts/run_conformance.py --suite conformance/arm64-v0.2.0 --report NEW.json -- \
      python3 tools/conformance-adapters/unicorn_adapter.py

The word runs at 0x1800 with FPCR=FPSR=0. Compared: X0-X30 except X18, all 32 V registers,
NZCV, and FPSR.QC (bit 27). PC, SP and memory are not compared.
"""
import sys

import unicorn
from unicorn import UC_ARCH_ARM64, UC_MODE_ARM, Uc, UcError
from unicorn.arm64_const import (
    UC_ARM64_REG_CPACR_EL1, UC_ARM64_REG_FPCR, UC_ARM64_REG_FPSR, UC_ARM64_REG_NZCV,
    UC_CPU_ARM64_MAX, UC_ARM64_REG_PC, UC_ARM64_REG_V0, UC_ARM64_REG_X29, UC_ARM64_REG_X30,
)
from unicorn.arm64_const import UC_ARM64_REG_X0

CODE = 0x1800
PAGE = 0x1000
QC = 1 << 27
# X29 and X30 are not contiguous with X0-X28 in Unicorn's register enum.
X_REGS = [UC_ARM64_REG_X0 + i for i in range(29)] + [UC_ARM64_REG_X29, UC_ARM64_REG_X30]
SKIP_X = {18}  # macOS-reserved on the hardware that recorded the traces


def parse(line):
    """Split a `T WORD NZCV_IN X.. V.. => NZCV_OUT FPSR_OUT X.. V..` line into its two halves."""
    fields = line.split()
    arrow = fields.index("=>")
    return fields[1:arrow], fields[arrow + 1:]


def counts(**values):
    labels = {
        "host_executed": "host executed (not SIGILL)",
        "host_undefined": "host UNDEFINED (SIGILL)",
        "unimplemented": "emulator unimplemented",
        "errors": "emulator other errors",
        "executed": "emulator executed",
        "match": "exact match (X,V,NZCV,QC)",
        "mismatch": "MISMATCH",
        "undefined_run": "emulator runs a word the host calls UNDEFINED",
    }
    for key, label in labels.items():
        print(f"{label}: {values.get(key, 0)}")


def run(line, fpcr=0):
    """Execute the traced word in Unicorn; return (executed, state) or raise UcError."""
    head, _ = parse(line)
    word = int(head[0], 16)
    nzcv = int(head[1], 16)
    x_in = [int(v, 16) for v in head[2:33]]
    v_in = [int(v, 16) for v in head[33:65]]

    uc = Uc(UC_ARCH_ARM64, UC_MODE_ARM)
    uc.ctl_set_cpu_model(UC_CPU_ARM64_MAX)  # the default model lacks FP16, FHM, RDM, FCMA, DotProd, BF16, FRINT32/64
    uc.mem_map(CODE - 0x800, PAGE)
    uc.mem_write(CODE, word.to_bytes(4, "little"))
    uc.reg_write(UC_ARM64_REG_CPACR_EL1, 3 << 20)  # FP/SIMD not trapped
    uc.reg_write(UC_ARM64_REG_FPCR, fpcr)
    uc.reg_write(UC_ARM64_REG_FPSR, 0)
    uc.reg_write(UC_ARM64_REG_NZCV, nzcv << 28)
    for i, value in enumerate(x_in):
        uc.reg_write(X_REGS[i], value)
    for i, value in enumerate(v_in):
        uc.reg_write(UC_ARM64_REG_V0 + i, value)
    uc.emu_start(CODE, CODE + 4, count=1)
    x_out = [uc.reg_read(reg) for reg in X_REGS]
    v_out = [uc.reg_read(UC_ARM64_REG_V0 + i) for i in range(32)]
    return {
        "nzcv": (uc.reg_read(UC_ARM64_REG_NZCV) >> 28) & 0xF,
        "fpsr": uc.reg_read(UC_ARM64_REG_FPSR),
        "x": x_out,
        "v": v_out,
        "pc": uc.reg_read(UC_ARM64_REG_PC),
    }


def differences(state, expected):
    nzcv, fpsr, *rest = expected
    x_ref = [int(v, 16) for v in rest[:31]]
    v_ref = [int(v, 16) for v in rest[31:63]]
    found = []
    if state["nzcv"] != int(nzcv, 16):
        found.append(f"NZCV got {state['nzcv']:x} want {int(nzcv, 16):x}")
    if (state["fpsr"] & QC) != (int(fpsr, 16) & QC):
        found.append(f"FPSR.QC got {state['fpsr'] & QC != 0} want {int(fpsr, 16) & QC != 0}")
    for i, (got, want) in enumerate(zip(state["x"], x_ref)):
        if i not in SKIP_X and got != want:
            found.append(f"X{i} got {got:016x} want {want:016x}")
    for i, (got, want) in enumerate(zip(state["v"], v_ref)):
        if got != want:
            found.append(f"V{i} got {got:032x} want {want:032x}")
    return found


def main():
    path = sys.argv[-1]
    line = next(raw for raw in open(path) if raw.startswith("T "))
    print(f"emulator: Unicorn {unicorn.__version__}")
    try:
        state = run(line)
    except UcError as error:
        print(f"unicorn error: {error}")
        unimplemented = "INSN_INVALID" in str(error) or "Invalid instruction" in str(error)
        counts(host_executed=1, unimplemented=int(unimplemented), errors=int(not unimplemented))
        return 1
    _, expected = parse(line)
    found = differences(state, expected)
    for item in found:
        print("  diff:", item)
    counts(host_executed=1, executed=1, match=int(not found), mismatch=int(bool(found)))
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
