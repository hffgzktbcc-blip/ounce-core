#!/usr/bin/env python3
"""Hand-assemble the WebAssembly test modules and write wasm_modules.h (C byte arrays).
No wasm toolchain involved: every byte is produced here. Run: python3 gen_wasm.py > wasm_modules.h"""
import sys

def u(n):
    o = bytearray()
    while True:
        b = n & 0x7f; n >>= 7
        if n: o.append(b | 0x80)
        else: o.append(b); return bytes(o)

def vec(items): return u(len(items)) + b"".join(items)
def name(s): return u(len(s)) + s.encode()
def section(i, body): return bytes([i]) + u(len(body)) + body
T = {"i": 0x7f, "l": 0x7e, "f": 0x7d, "d": 0x7c}

class Mod:
    def __init__(s):
        s.types = []; s.funcs = []; s.exports = []; s.extra = {}
    def tidx(s, sig):
        p, r = sig.split(">")
        t = b"\x60" + vec([bytes([T[c]]) for c in p]) + vec([bytes([T[c]]) for c in r])
        if t not in s.types: s.types.append(t)
        return s.types.index(t)
    def fn(s, nm, sig, body, locs=(), export=True):
        s.funcs.append((nm, s.tidx(sig), bytes(body), locs, export)); return len(s.funcs) - 1
    def build(s):
        o = b"\x00asm\x01\x00\x00\x00"
        o += section(1, vec(s.types))
        o += section(3, vec([u(f[1]) for f in s.funcs]))
        for k in (4, 5, 6):
            if k in s.extra: o += section(k, s.extra[k])
        ex = [name(f[0]) + b"\x00" + u(i) for i, f in enumerate(s.funcs) if f[4]]
        ex += s.extra.get("exp", [])
        o += section(7, vec(ex))
        if 9 in s.extra: o += section(9, s.extra[9])
        code = []
        for f in s.funcs:
            loc = vec([u(n) + bytes([T[t]]) for n, t in f[3]])
            b = loc + f[2] + b"\x0b"
            code.append(u(len(b)) + b)
        o += section(10, vec(code))
        if 11 in s.extra: o += section(11, s.extra[11])
        return o

L0, L1, L2 = b"\x20\x00", b"\x20\x01", b"\x20\x02"
auto = []

# ---- module "ops": every numeric opcode as an exported function --------------------------
ops = Mod()
def un(nm, sig, op):
    ops.fn(nm, sig, L0 + bytes([op])); auto.append(nm)
def bi(nm, sig, op):
    ops.fn(nm, sig, L0 + L1 + bytes([op])); auto.append(nm)
names_i = "add sub mul div_s div_u rem_s rem_u and or xor shl shr_s shr_u rotl rotr".split()
cmp_i = "eq ne lt_s lt_u gt_s gt_u le_s le_u ge_s ge_u".split()
for pre, t, base_bin, base_cmp, base_un in (("i32", "i", 0x6a, 0x46, 0x67), ("i64", "l", 0x7c, 0x51, 0x79)):
    for k, n in enumerate(names_i): bi(f"{pre}.{n}", f"{t}{t}>{t}", base_bin + k)
    for k, n in enumerate(cmp_i): bi(f"{pre}.{n}", f"{t}{t}>i", base_cmp + k)
    for k, n in enumerate("clz ctz popcnt".split()): un(f"{pre}.{n}", f"{t}>{t}", base_un + k)
    un(f"{pre}.eqz", f"{t}>i", 0x45 if t == "i" else 0x50)
for pre, t, bb, bc, bu in (("f32", "f", 0x92, 0x5b, 0x8b), ("f64", "d", 0xa0, 0x61, 0x99)):
    for k, n in enumerate("add sub mul div min max copysign".split()): bi(f"{pre}.{n}", f"{t}{t}>{t}", bb + k)
    for k, n in enumerate("eq ne lt gt le ge".split()): bi(f"{pre}.{n}", f"{t}{t}>i", bc + k)
    for k, n in enumerate("abs neg ceil floor trunc nearest sqrt".split()): un(f"{pre}.{n}", f"{t}>{t}", bu + k)
for nm, sig, op in [("i32.wrap_i64", "l>i", 0xa7), ("i32.trunc_f32_s", "f>i", 0xa8), ("i32.trunc_f32_u", "f>i", 0xa9),
    ("i32.trunc_f64_s", "d>i", 0xaa), ("i32.trunc_f64_u", "d>i", 0xab), ("i64.extend_i32_s", "i>l", 0xac),
    ("i64.extend_i32_u", "i>l", 0xad), ("i64.trunc_f32_s", "f>l", 0xae), ("i64.trunc_f32_u", "f>l", 0xaf),
    ("i64.trunc_f64_s", "d>l", 0xb0), ("i64.trunc_f64_u", "d>l", 0xb1), ("f32.convert_i32_s", "i>f", 0xb2),
    ("f32.convert_i32_u", "i>f", 0xb3), ("f32.convert_i64_s", "l>f", 0xb4), ("f32.convert_i64_u", "l>f", 0xb5),
    ("f32.demote_f64", "d>f", 0xb6), ("f64.convert_i32_s", "i>d", 0xb7), ("f64.convert_i32_u", "i>d", 0xb8),
    ("f64.convert_i64_s", "l>d", 0xb9), ("f64.convert_i64_u", "l>d", 0xba), ("f64.promote_f32", "f>d", 0xbb),
    ("i32.reinterpret_f32", "f>i", 0xbc), ("i64.reinterpret_f64", "d>l", 0xbd), ("f32.reinterpret_i32", "i>f", 0xbe),
    ("f64.reinterpret_i64", "l>d", 0xbf), ("i32.extend8_s", "i>i", 0xc0), ("i32.extend16_s", "i>i", 0xc1),
    ("i64.extend8_s", "l>l", 0xc2), ("i64.extend16_s", "l>l", 0xc3), ("i64.extend32_s", "l>l", 0xc4)]:
    un(nm, sig, op)
# select / unreachable
ops.fn("x.select", "iii>i", L0 + L1 + L2 + b"\x1b")
ops.fn("x.unreachable", ">i", b"\x00\x41\x01")
# memory: 1 page, max 3, data segment at 16
ops.extra[5] = vec([b"\x01\x01\x03"])
data = bytes([0x77, 0x61, 0x73, 0x6d, 0x33, 0x00, 0xff, 0x80, 0x7f, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07])
ops.extra[11] = vec([b"\x00\x41\x10\x0b" + u(len(data)) + data])
def ma(al, off): return bytes([al]) + u(off)
mem_sigs = [("i32.load", "i>i", 0x28, 2), ("i64.load", "i>l", 0x29, 3), ("f32.load", "i>f", 0x2a, 2), ("f64.load", "i>d", 0x2b, 3),
    ("i32.load8_s", "i>i", 0x2c, 0), ("i32.load8_u", "i>i", 0x2d, 0), ("i32.load16_s", "i>i", 0x2e, 1), ("i32.load16_u", "i>i", 0x2f, 1),
    ("i64.load8_s", "i>l", 0x30, 0), ("i64.load8_u", "i>l", 0x31, 0), ("i64.load16_s", "i>l", 0x32, 1), ("i64.load16_u", "i>l", 0x33, 1),
    ("i64.load32_s", "i>l", 0x34, 2), ("i64.load32_u", "i>l", 0x35, 2)]
for nm, sig, op, al in mem_sigs:
    ops.fn("mem." + nm, sig, L0 + bytes([op]) + ma(al, 0))
    ops.fn("mem." + nm + "_off", sig, L0 + bytes([op]) + ma(al, 65530))
st_sigs = [("i32.store", "ii>", 0x36, 2), ("i64.store", "il>", 0x37, 3), ("f32.store", "if>", 0x38, 2), ("f64.store", "id>", 0x39, 3),
    ("i32.store8", "ii>", 0x3a, 0), ("i32.store16", "ii>", 0x3b, 1), ("i64.store8", "il>", 0x3c, 0), ("i64.store16", "il>", 0x3d, 1),
    ("i64.store32", "il>", 0x3e, 2)]
for nm, sig, op, al in st_sigs:
    ops.fn("mem." + nm, sig, L0 + L1 + bytes([op]) + ma(al, 0))
ops.fn("mem.size", ">i", b"\x3f\x00")
ops.fn("mem.grow", "i>i", L0 + b"\x40\x00")
# sum of bytes [0,len)
ops.fn("mem.sum", "i>i", bytes.fromhex("0240 03 40 20 01 20 00 4e 0d 01 20 02 20 01 2d 00 00 6a 21 02 20 01 41 01 6a 21 01 0c 00 0b 0b 20 02".replace(" ", "")),
       locs=[(2, "i")])

# ---- module "prog": small programs -------------------------------------------------------
pr = Mod()
fib = len(pr.funcs)
pr.fn("fib", "i>i", bytes.fromhex("2000410248047f20000520004101 6b 1000 20004102 6b 1000 6a 0b".replace(" ", "")))
pr.fn("fac", "l>l", bytes.fromhex("42 01 21 01 02 40 03 40 20 00 50 0d 01 20 01 20 00 7e 21 01 20 00 42 01 7d 21 00 0c 00 0b 0b 20 01".replace(" ", "")), locs=[(1, "l")])
pr.fn("collatz", "l>i", bytes.fromhex(
    "0240 03 40 2000 4201 51 0d01 2000 4201 83 50 04 40 2000 4201 88 2100 05 2000 4203 7e 4201 7c 2100 0b 2001 4101 6a 2101 0c00 0b0b 2001".replace(" ", "")),
    locs=[(1, "i")])
pr.fn("brtable", "i>i", bytes.fromhex("0240 0240 0240 0240 2000 0e03 00010203 0b 410a 0f 0b 4114 0f 0b 411e 0f 0b 4128".replace(" ", "")))
pr.fn("sieve", "i>i", bytes.fromhex(
    "4100 2101 0240 0340 2001 2000 4e 0d01 2001 4100 3a0000 2001 4101 6a 2101 0c00 0b0b "
    "4102 2101 0240 0340 2001 2000 4e 0d01 2001 2d0000 45 0440 2003 4101 6a 2103 2001 2001 6a 2102 "
    "0240 0340 2002 2000 4e 0d01 2002 4101 3a0000 2002 2001 6a 2102 0c00 0b0b 0b "
    "2001 4101 6a 2101 0c00 0b0b 2003".replace(" ", "")), locs=[(3, "i")])
pr.fn("gcd", "ii>i", bytes.fromhex("2001 45 04 7f 2000 05 2001 2000 2001 70 1002 0b".replace(" ", "")))  # gcd(a,b)=b==0?a:gcd(b,a%b) (func idx patched below)
pr.fn("counter", ">i", bytes.fromhex("2300 4101 6a 2400 2300"))
# call_indirect table: add, sub, mul, and a (i32)->i32 function that mismatches the call type
add_f = pr.fn("add", "ii>i", L0 + L1 + b"\x6a", export=False)
sub_f = pr.fn("sub", "ii>i", L0 + L1 + b"\x6b", export=False)
mul_f = pr.fn("mul", "ii>i", L0 + L1 + b"\x6c", export=False)
neg_f = pr.fn("neg", "i>i", b"\x41\x00" + L0 + b"\x6b", export=False)
ti = pr.tidx("ii>i")
pr.fn("dispatch", "iii>i", L1 + L2 + L0 + b"\x11" + u(ti) + b"\x00")
pr.extra[4] = vec([b"\x70\x01\x04\x04"])
pr.extra[6] = vec([b"\x7f\x01\x41\xe4\x00\x0b"])  # mutable i32 global = 100
pr.extra[9] = vec([b"\x00\x41\x00\x0b" + vec([u(add_f), u(sub_f), u(mul_f), u(neg_f)])])
pr.extra[5] = vec([b"\x00\x01"])
# patch gcd's recursive call to its own index
gi = [i for i, f in enumerate(pr.funcs) if f[0] == "gcd"][0]
f = pr.funcs[gi]; body = f[2].replace(b"\x10\x02", b"\x10" + u(gi)); pr.funcs[gi] = (f[0], f[1], body, f[3], f[4])
fi = [i for i, f in enumerate(pr.funcs) if f[0] == "fib"][0]
f = pr.funcs[fi]; pr.funcs[fi] = (f[0], f[1], f[2].replace(b"\x10\x00", b"\x10" + u(fi)), f[3], f[4])

def carr(nm, b):
    s = f"static const unsigned char {nm}[] = {{\n"
    for i in range(0, len(b), 16): s += " " + ",".join(f"0x{x:02x}" for x in b[i:i + 16]) + ",\n"
    return s + "};\n"
print("/* generated by gen_wasm.py; do not edit */")
print(carr("WASM_OPS", ops.build()))
print(carr("WASM_PROG", pr.build()))
print("static const char* const AUTO_FUNCS[] = {" + ",".join(f'"{a}"' for a in auto) + "};")
print(f"#define N_AUTO {len(auto)}")
