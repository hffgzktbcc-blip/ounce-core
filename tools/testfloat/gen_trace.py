#!/usr/bin/env python3
"""Turn Berkeley TestFloat vectors into `ounce-core fuzz-replay` traces.

    gen_trace.py --gen /path/to/testfloat_gen --ops add,mul,to_int --count 2000 | ounce-core fuzz-replay -

TestFloat is a test-time tool only (BSD-3, see docs/IMPROVEMENT_IDEAS.md); nothing from it is committed.

What is compared: the result register or NZCV and the FPSR exception flags (IOC DZC OFC UFC IXC), for f32 and f64,
in all four FPCR rounding modes (default FPCR otherwise, so no FZ and no DN). Tininess is detected before rounding
(`-tininessbefore`), which is what AArch64 does.

Operations (`--ops`, comma separated; default all):
  add sub mul div sqrt mulAdd   FADD FSUB FMUL FDIV FSQRT FMADD
  cvt                           FCVT between single and double
  from_int                      SCVTF/UCVTF from W and X registers
  to_int                        FCVTNS/PS/MS/ZS/AS and the unsigned forms, to W and X
  round                         FRINTN/P/M/Z/A and FRINTX (FRINTX/FRINTI follow FPCR.RMode)
  cmp                           FCMP and FCMPE (NaNs included: NZCV 0011 and IOC rules)

Expected results that depend on NaN or integer-overflow conventions are computed here from the Arm rules rather than
taken from TestFloat, so the SoftFloat specialisation it was built with does not matter: an invalid operation on
non-NaN inputs gives the positive default NaN; float-to-integer saturates and a NaN gives 0. Arithmetic and
conversion cases with a NaN *input* are skipped (TestFloat's propagation rules are not Arm's); compare cases keep
them, because the result is just "unordered".
"""
import argparse, subprocess, sys

ZERO_X = [0] * 31
RM = {"-rnear_even": 0, "-rmax": 1 << 22, "-rmin": 2 << 22, "-rminMag": 3 << 22}  # FPCR.RMode RN RP RM RZ
# TestFloat exception flags -> FPSR cumulative bits
TF_FLAGS = [(0x10, 0x01), (0x08, 0x02), (0x04, 0x04), (0x02, 0x08), (0x01, 0x10)]  # invalid, infinite, overflow, underflow, inexact


def fpsr(tf):
    return sum(b for t, b in TF_FLAGS if tf & t)


def is_nan(bits, w):
    if w == 32:
        return (bits & 0x7F800000) == 0x7F800000 and (bits & 0x7FFFFF) != 0
    return (bits & 0x7FF0000000000000) == 0x7FF0000000000000 and (bits & 0xFFFFFFFFFFFFF) != 0


def is_neg(bits, w):
    return bool(bits >> (w - 1))


def default_nan(w):
    return 0x7FC00000 if w == 32 else 0x7FF8000000000000


def line(word, fpcr, vin=(), xin=(), vout=None, xout=None, nzcv_in=0, nzcv_out=0, flags=0):
    v_in, x_in = [0] * 32, list(ZERO_X)
    for i, x in enumerate(vin):
        v_in[i] = x
    for i, x in enumerate(xin):
        x_in[i] = x
    v_out, x_out = list(v_in), list(x_in)
    if vout is not None:
        v_out[3] = vout
    if xout is not None:
        x_out[3] = xout
    h = lambda xs: " ".join(f"{x:x}" for x in xs)
    return f"TF {fpcr:x} {word:x} {nzcv_in:x} {h(x_in)} {h(v_in)} => {nzcv_out:x} {fpsr(flags):x} {h(x_out)} {h(v_out)}"


def gen(a, name, rnd=None, exact=None):
    cmd = [a.gen, "-tininessbefore", "-level", a.level]
    if rnd:
        cmd.append(rnd)
    if exact is not None:
        cmd.append("-exact" if exact else "-notexact")
    if a.count:
        cmd += ["-n", str(a.count)]
    r = subprocess.run(cmd + [name], capture_output=True, text=True, check=True)
    return [[int(t, 16) for t in ln.split()] for ln in r.stdout.splitlines() if ln.strip()]


def emit_arith(a, out, stats):
    BIN = {"add": 0x2800, "sub": 0x3800, "mul": 0x0800, "div": 0x1800}
    for w in (32, 64):
        base = 0x1E200000 if w == 32 else 0x1E600000
        for op in ("add", "sub", "mul", "div", "sqrt", "mulAdd"):
            if op not in a.ops:
                continue
            if op in BIN:
                word, nin = base | BIN[op] | (1 << 16) | 3, 2
            elif op == "sqrt":
                word, nin = (0x1E21C000 if w == 32 else 0x1E61C000) | 3, 1
            else:
                word, nin = (0x1F000000 if w == 32 else 0x1F400000) | (1 << 16) | (2 << 10) | 3, 3
            for rnd, fpcr in RM.items():
                for f in gen(a, f"f{w}_{op}", rnd):
                    ins, res, fl = f[:nin], f[nin], f[nin + 1]
                    if any(is_nan(x, w) for x in ins):
                        stats["skipped"] += 1
                        continue
                    if is_nan(res, w):
                        res = default_nan(w)  # invalid operation: Arm gives the positive default NaN
                    out.append(line(word, fpcr, vin=ins, vout=res, flags=fl))


def emit_cvt(a, out, stats):
    for src, dst in ((32, 64), (64, 32)):
        word = (0x1E22C000 if src == 32 else 0x1E624000) | 3
        for rnd, fpcr in (RM.items() if src == 64 else [("-rnear_even", 0)]):  # widening is exact in every mode
            for f in gen(a, f"f{src}_to_f{dst}", rnd):
                x, res, fl = f
                if is_nan(x, src):
                    stats["skipped"] += 1
                    continue
                out.append(line(word, fpcr, vin=[x], vout=res, flags=fl))


def emit_from_int(a, out, stats):
    for fp in (32, 64):
        for isz in (32, 64):
            for signed in (True, False):
                name = f"{'i' if signed else 'ui'}{isz}_to_f{fp}"
                word = (0x1E220000 if signed else 0x1E230000) | (0 if fp == 32 else 0x400000) | (0x80000000 if isz == 64 else 0) | 3
                for rnd, fpcr in RM.items():
                    for x, res, fl in gen(a, name, rnd):
                        out.append(line(word, fpcr, xin=[x], vout=res, flags=fl))


# (rmode field, opcode) of the FCVT*S/U forms and the TestFloat rounding option each uses; FPCR stays default
TO_INT = {"N": (0, 0, "-rnear_even"), "P": (1, 0, "-rmax"), "M": (2, 0, "-rmin"), "Z": (3, 0, "-rminMag"), "A": (0, 4, "-rnear_maxMag")}


def saturate(x, fp, signed, isz):
    """Arm FPToFixed result for an invalid conversion: NaN gives 0, otherwise the end of the range."""
    if is_nan(x, fp):
        return 0
    neg = is_neg(x, fp)
    if signed:
        return (1 << (isz - 1)) if neg else (1 << (isz - 1)) - 1
    return 0 if neg else (1 << isz) - 1


def emit_to_int(a, out, stats):
    for fp in (32, 64):
        for isz in (32, 64):
            for signed in (True, False):
                name = f"f{fp}_to_{'i' if signed else 'ui'}{isz}"
                for k, (rmode, opc, rnd) in TO_INT.items():
                    word = 0x1E200000 | (rmode << 19) | ((opc + (0 if signed else 1)) << 16) | (0 if fp == 32 else 0x400000) \
                        | (0x80000000 if isz == 64 else 0) | 3
                    for x, res, fl in gen(a, name, rnd, exact=True):
                        if fl & 0x10:
                            res = saturate(x, fp, signed, isz)
                        out.append(line(word, 0, vin=[x], xout=res, flags=fl))


def emit_round(a, out, stats):
    # FRINTN/P/M/Z/A: fixed mode, not exact; FRINTX: FPCR.RMode, exact
    fixed = {"N": (0x1E244000, "-rnear_even"), "P": (0x1E24C000, "-rmax"), "M": (0x1E254000, "-rmin"),
             "Z": (0x1E25C000, "-rminMag"), "A": (0x1E264000, "-rnear_maxMag")}
    for w in (32, 64):
        name = f"f{w}_roundToInt"
        for k, (word, rnd) in fixed.items():
            for x, res, fl in gen(a, name, rnd, exact=False):
                if is_nan(x, w):
                    stats["skipped"] += 1
                    continue
                out.append(line(word | (0 if w == 32 else 0x400000) | 3, 0, vin=[x], vout=res, flags=fl))
        for rnd, fpcr in RM.items():
            for x, res, fl in gen(a, name, rnd, exact=True):
                if is_nan(x, w):
                    stats["skipped"] += 1
                    continue
                out.append(line(0x1E274000 | (0 if w == 32 else 0x400000) | 3, fpcr, vin=[x], vout=res, flags=fl))


def emit_cmp(a, out, stats):
    # FCMP s0, s1 / FCMPE s0, s1. NZCV: equal 0110, less 1000, greater 0010, unordered 0011.
    for w in (32, 64):
        for signalling, eq, lt in ((False, f"f{w}_eq", f"f{w}_lt_quiet"), (True, f"f{w}_eq_signaling", f"f{w}_lt")):
            word = (0x1E202000 if w == 32 else 0x1E602000) | (1 << 16) | (0x10 if signalling else 0)
            e_rows, l_rows = gen(a, eq), gen(a, lt)
            # same inputs in both streams (same generator, same options), so the rows line up
            for (x, y, e, efl), (x2, y2, l, lfl) in zip(e_rows, l_rows):
                assert (x, y) == (x2, y2), "eq and lt streams diverged"
                if is_nan(x, w) or is_nan(y, w):
                    nzcv = 0x3
                elif e:
                    nzcv = 0x6
                elif l:
                    nzcv = 0x8
                else:
                    nzcv = 0x2
                out.append(line(word, 0, vin=[x, y], nzcv_out=nzcv, flags=efl))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gen", required=True)
    ap.add_argument("--count", type=int, default=0, help="cases per op and rounding mode (0 = TestFloat level default, about 46k)")
    ap.add_argument("--level", default="1")
    ap.add_argument("--ops", default="add,sub,mul,div,sqrt,mulAdd,cvt,from_int,to_int,round,cmp")
    a = ap.parse_args()
    a.ops = set(a.ops.split(","))
    unknown = a.ops - {"add", "sub", "mul", "div", "sqrt", "mulAdd", "cvt", "from_int", "to_int", "round", "cmp"}
    if unknown:
        raise SystemExit(f"unknown op {sorted(unknown)}")
    out, stats = [], {"skipped": 0}
    emit_arith(a, out, stats)
    for op, fn in (("cvt", emit_cvt), ("from_int", emit_from_int), ("to_int", emit_to_int), ("round", emit_round), ("cmp", emit_cmp)):
        if op in a.ops:
            fn(a, out, stats)
    sys.stdout.write("\n".join(out) + ("\n" if out else ""))
    print(f"emitted {len(out)}, skipped {stats['skipped']} NaN-input cases", file=sys.stderr)


main()
