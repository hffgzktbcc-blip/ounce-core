#!/usr/bin/env python3
"""Turn Berkeley TestFloat vectors into `ounce-core fuzz-replay` traces.

    gen_trace.py --gen /path/to/testfloat_gen --ops mul | ounce-core fuzz-replay -

TestFloat is a test-time tool only (BSD-3, see docs/IMPROVEMENT_IDEAS.md); nothing from it is committed.
Compared: the result register (round-to-nearest-even, default FPCR). Not compared: FPSR exception
flags (the interpreter does not model them). Cases with a NaN input or result are skipped for now,
because TestFloat's NaN rules differ from Arm's default-NaN and propagation rules.
"""
import argparse, subprocess, sys

ZERO_X = ["0"] * 31
# (testfloat name, width, base word, operand count)
BIN = {"add": 0x2800, "sub": 0x3800, "mul": 0x0800, "div": 0x1800}


def is_nan(bits, w):
    if w == 32:
        return (bits & 0x7F800000) == 0x7F800000 and (bits & 0x7FFFFF) != 0
    return (bits & 0x7FF0000000000000) == 0x7FF0000000000000 and (bits & 0xFFFFFFFFFFFFF) != 0


def word(op, w):
    base = 0x1E200000 if w == 32 else 0x1E600000
    if op in BIN:
        return base | BIN[op] | (1 << 16) | (0 << 5) | 3
    if op == "sqrt":
        return (0x1E21C000 if w == 32 else 0x1E61C000) | 3
    if op == "mulAdd":
        return (0x1F000000 if w == 32 else 0x1F400000) | (1 << 16) | (2 << 10) | 3
    raise SystemExit(f"unknown op {op}")


def line(wd, ins, out):
    v_in = ["0"] * 32
    for i, x in enumerate(ins):
        v_in[i] = f"{x:x}"
    v_out = list(v_in)
    v_out[3] = f"{out:x}"
    return " ".join(["T", f"{wd:x}", "0", *ZERO_X, *v_in, "=>", "0", "0", *ZERO_X, *v_out])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gen", required=True)
    ap.add_argument("--count", type=int, default=0, help="cases per op (0 = TestFloat level default, about 46k)")
    ap.add_argument("--level", default="1")
    ap.add_argument("--ops", default="add,sub,mul,div,sqrt,mulAdd")
    a = ap.parse_args()
    emitted = skipped = 0
    for w in (32, 64):
        for op in a.ops.split(","):
            name = f"f{w}_{op}"
            n = ["-n", str(a.count)] if a.count else []
            r = subprocess.run([a.gen, "-rnear_even", "-level", a.level, *n, name],
                               capture_output=True, text=True, check=True)
            nin = {"sqrt": 1, "mulAdd": 3}.get(op, 2)
            for ln in r.stdout.splitlines():
                f = [int(t, 16) for t in ln.split()]
                ins, out = f[:nin], f[nin]
                if any(is_nan(x, w) for x in ins) or is_nan(out, w):
                    skipped += 1
                    continue
                print(line(word(op, w), ins, out))
                emitted += 1
    print(f"emitted {emitted}, skipped {skipped} NaN cases", file=sys.stderr)


main()
