// Reproduction of the published output of U. Adams, "Ryu: fast float-to-string conversion", Proc. 39th ACM SIGPLAN Conference
// on Programming Language Design and Implementation (PLDI 2018), pp. 270-282: shortest decimal representation of a double that reads back as the
// same double, closest to the exact value, ties to even.
// Written for ounce-core from the algorithm in the paper: Ryu's decode, the e2 >= 0 and e2 < 0 cases with the 5^q reciprocal and 5^i
// tables (125-bit entries, computed here at run time with a small big-number type instead of being stored), the trailing-zero tracking
// and the digit-removal loops. No Ryu source code was used. The test vectors are the 132 expected strings in the authors' test
// file d2s_test.cc; the inputs written there as decimal literals were converted to IEEE bit patterns with Python's correctly rounded float().
// Negative control: -DNEG_NOEVEN removes the round-to-even correction of the last removed digit.
#include "ounce_paper.h"
typedef unsigned __int128 u128;
#ifndef HOST
// the compiler calls these for 128-bit division; the freestanding build has no compiler runtime library
static u128 udivmod128(u128 n, u128 d, u128 *rem) {
    u128 q = 0, r = 0;
    for (int i = 127; i >= 0; i--) { r = (r << 1) | ((n >> i) & 1); if (r >= d) { r -= d; q |= (u128)1 << i; } }
    if (rem) *rem = r; return q;
}
u128 __udivti3(u128 n, u128 d) { return udivmod128(n, d, 0); }
u128 __umodti3(u128 n, u128 d) { u128 r; udivmod128(n, d, &r); return r; }
#endif

#define NL 28
typedef struct { u32 w[NL]; } Big;
static void b_set(Big *a, u32 v) { for (int i = 0; i < NL; i++) a->w[i] = 0; a->w[0] = v; }
static void b_mul5(Big *a) { u64 c = 0; for (int i = 0; i < NL; i++) { u64 t = (u64)a->w[i] * 5 + c; a->w[i] = (u32)t; c = t >> 32; } }
static void b_shl1(Big *a) { u32 c = 0; for (int i = 0; i < NL; i++) { u32 n = (a->w[i] << 1) | c; c = a->w[i] >> 31; a->w[i] = n; } }
static int b_cmp(const Big *a, const Big *b) { for (int i = NL - 1; i >= 0; i--) if (a->w[i] != b->w[i]) return a->w[i] < b->w[i] ? -1 : 1; return 0; }
static void b_sub(Big *a, const Big *b) { i64 br = 0; for (int i = 0; i < NL; i++) { i64 t = (i64)a->w[i] - b->w[i] - br; br = t < 0; a->w[i] = (u32)(t + (br ? (1LL << 32) : 0)); } }
static void b_setbit(Big *a, int k) { b_set(a, 0); a->w[k >> 5] = 1u << (k & 31); }
static int b_bitlen(const Big *a) { for (int i = NL - 1; i >= 0; i--) if (a->w[i]) return i * 32 + 32 - __builtin_clz(a->w[i]); return 0; }
static u128 b_low128(const Big *a) { return (u128)a->w[0] | ((u128)a->w[1] << 32) | ((u128)a->w[2] << 64) | ((u128)a->w[3] << 96); }
static void b_shr(Big *a, int s) {   // shift right by s bits
    int ws = s >> 5, bs = s & 31;
    for (int i = 0; i < NL; i++) { u64 lo = (i + ws < NL) ? a->w[i + ws] : 0, hi = (i + ws + 1 < NL) ? a->w[i + ws + 1] : 0; a->w[i] = bs ? (u32)((lo >> bs) | (hi << (32 - bs))) : (u32)lo; }
}
static void b_shl(Big *a, int s) { while (s-- > 0) b_shl1(a); }

#define BITCOUNT 125
#define FBITS_INV 59
#define FBITS 61
static u32 pow5bits(u32 e) { return (u32)(((u64)e * 1217359) >> 19) + 1; }
static u32 log10Pow2(u32 e) { return (u32)(((u64)e * 78913) >> 18); }
static u32 log10Pow5(u32 e) { return (u32)(((u64)e * 732923) >> 20); }
// DOUBLE_POW5_SPLIT[i]: 5^i aligned to exactly 125 bits (floor)
static u128 pow5_split_b(u32 i, int nbits) {
    Big a; b_set(&a, 1); for (u32 k = 0; k < i; k++) b_mul5(&a);
    int bl = b_bitlen(&a);
    if (bl > nbits) b_shr(&a, bl - nbits); else b_shl(&a, nbits - bl);
    return b_low128(&a);
}
// DOUBLE_POW5_INV_SPLIT[q] = floor(2^(pow5bits(q) - 1 + 125) / 5^q) + 1
static u128 pow5_inv_split_b(u32 q, int nbits) {
    if (q == 0) return ((u128)1 << nbits) + 1;
    Big d; b_set(&d, 1); for (u32 k = 0; k < q; k++) b_mul5(&d);
    int t = (int)pow5bits(q) - 1;
    Big r; b_setbit(&r, t); u128 quo = 0;
    for (int s = 0; s < nbits; s++) { b_shl1(&r); quo <<= 1; if (b_cmp(&r, &d) >= 0) { b_sub(&r, &d); quo |= 1; } }
    return quo + 1;
}
static u128 pow5_split(u32 i) { return pow5_split_b(i, BITCOUNT); }
static u128 pow5_inv_split(u32 q) { return pow5_inv_split_b(q, BITCOUNT); }
// (m * mul) >> j for a 125-bit mul and m < 2^55, j >= 64
static u64 mul_shift(u64 m, u128 mul, int j) {
    u64 lo = (u64)mul, hi = (u64)(mul >> 64);
    u128 b0 = (u128)m * lo, b2 = (u128)m * hi;
    u128 s = (b0 >> 64) + b2;
    return (u64)(s >> (j - 64));
}
static u32 pow5_factor(u64 v) { u32 c = 0; for (;;) { u64 q = v / 5; if (q * 5 != v) return c; v = q; c++; } }
static int mult5(u64 v, u32 p) { return pow5_factor(v) >= p; }
static int mult2(u64 v, u32 p) { return (v & (((u64)1 << p) - 1)) == 0; }

static int ryu_d2s(u64 bits, char *out) {
    int sign = (int)(bits >> 63); u64 mant = bits & 0xFFFFFFFFFFFFFull; u32 ex = (u32)((bits >> 52) & 2047);
    int n = 0;
    if (ex == 2047) { const char *s = mant ? "NaN" : (sign ? "-Infinity" : "Infinity"); while ((out[n] = s[n])) n++; return n; }
    if (ex == 0 && mant == 0) { if (sign) out[n++] = '-'; out[n++] = '0'; out[n++] = 'E'; out[n++] = '0'; out[n] = 0; return n; }
    int e2; u64 m2;
    if (ex == 0) { e2 = 1 - 1023 - 52 - 2; m2 = mant; } else { e2 = (int)ex - 1023 - 52 - 2; m2 = ((u64)1 << 52) | mant; }
    int even = (m2 & 1) == 0, acceptBounds = even;
    u64 mv = 4 * m2; u32 mmShift = (mant != 0 || ex <= 1) ? 1 : 0;
    u64 vr, vp, vm; int e10; int vmTZ = 0, vrTZ = 0;
    if (e2 >= 0) {
        u32 q = log10Pow2((u32)e2) - ((u32)e2 > 3);
        e10 = (int)q;
        int k = BITCOUNT + (int)pow5bits(q) - 1; int i = -e2 + (int)q + k;
        u128 mul = pow5_inv_split(q);
        vr = mul_shift(4 * m2, mul, i); vp = mul_shift(4 * m2 + 2, mul, i); vm = mul_shift(4 * m2 - 1 - mmShift, mul, i);
        if (q <= 21) {
            if (mv % 5 == 0) vrTZ = mult5(mv, q);
            else if (acceptBounds) vmTZ = mult5(mv - 1 - mmShift, q);
            else vp -= mult5(mv + 2, q);
        }
    } else {
        u32 q = log10Pow5((u32)(-e2)) - ((u32)(-e2) > 1);
        e10 = (int)q + e2;
        int i = -e2 - (int)q; int k = (int)pow5bits((u32)i) - BITCOUNT; int j = (int)q - k;
        u128 mul = pow5_split((u32)i);
        vr = mul_shift(4 * m2, mul, j); vp = mul_shift(4 * m2 + 2, mul, j); vm = mul_shift(4 * m2 - 1 - mmShift, mul, j);
        if (q <= 1) {
            vrTZ = 1;
            if (acceptBounds) vmTZ = mmShift == 1; else --vp;
        } else if (q < 63) {
            vrTZ = mult2(mv, q);
        }
    }
    int removed = 0; int lastRemoved = 0; u64 output;
    if (vmTZ || vrTZ) {
        while (vp / 10 > vm / 10) { vmTZ &= vm % 10 == 0; vrTZ &= lastRemoved == 0; lastRemoved = (int)(vr % 10); vr /= 10; vp /= 10; vm /= 10; ++removed; }
        if (vmTZ) { while (vm % 10 == 0) { vrTZ &= lastRemoved == 0; lastRemoved = (int)(vr % 10); vr /= 10; vp /= 10; vm /= 10; ++removed; } }
#ifndef NEG_NOEVEN
        if (vrTZ && lastRemoved == 5 && vr % 2 == 0) lastRemoved = 4;
#endif
        output = vr + (((vr == vm && (!acceptBounds || !vmTZ)) || lastRemoved >= 5) ? 1 : 0);
    } else {
        int roundUp = 0;
        while (vp / 10 > vm / 10) { roundUp = vr % 10 >= 5; vr /= 10; vp /= 10; vm /= 10; ++removed; }
        output = vr + ((vr == vm || roundUp) ? 1 : 0);
    }
    int exp10 = e10 + removed;
    char dg[24]; int nd = 0; u64 t = output; while (t) { dg[nd++] = (char)('0' + t % 10); t /= 10; }
    if (sign) out[n++] = '-';
    out[n++] = dg[nd - 1];
    if (nd > 1) { out[n++] = '.'; for (int i = nd - 2; i >= 0; i--) out[n++] = dg[i]; }
    out[n++] = 'E';
    int e = exp10 + nd - 1; if (e < 0) { out[n++] = '-'; e = -e; }
    char eb[8]; int ne = 0; if (e == 0) eb[ne++] = '0'; while (e) { eb[ne++] = (char)('0' + e % 10); e /= 10; }
    while (ne) out[n++] = eb[--ne];
    out[n] = 0; return n;
}

// ---- binary32 version (the paper's Ryu for float: 64-bit tables of 59 and 61 bits, plus the extra last-removed-digit step) ----
static u32 mul_shift32(u32 m, u128 mul, int j) { return (u32)(((u128)m * mul) >> j); }
static int ryu_f2s(u32 bits, char *out) {
    int sign = (int)(bits >> 31); u32 mant = bits & 0x7FFFFFu; u32 ex = (bits >> 23) & 255;
    int n = 0;
    if (ex == 255) { const char *s = mant ? "NaN" : (sign ? "-Infinity" : "Infinity"); while ((out[n] = s[n])) n++; return n; }
    if (ex == 0 && mant == 0) { if (sign) out[n++] = '-'; out[n++] = '0'; out[n++] = 'E'; out[n++] = '0'; out[n] = 0; return n; }
    int e2; u32 m2;
    if (ex == 0) { e2 = 1 - 127 - 23 - 2; m2 = mant; } else { e2 = (int)ex - 127 - 23 - 2; m2 = (1u << 23) | mant; }
    int even = (m2 & 1) == 0, acceptBounds = even;
    u32 mv = 4 * m2, mp = 4 * m2 + 2; u32 mmShift = (mant != 0 || ex <= 1) ? 1 : 0; u32 mm = 4 * m2 - 1 - mmShift;
    u32 vr, vp, vm; int e10; int vmTZ = 0, vrTZ = 0; u32 lastRemoved = 0;
    if (e2 >= 0) {
        u32 q = log10Pow2((u32)e2); e10 = (int)q;
        int k = FBITS_INV + (int)pow5bits(q) - 1; int i = -e2 + (int)q + k;
        u128 mul = pow5_inv_split_b(q, FBITS_INV);
        vr = mul_shift32(mv, mul, i); vp = mul_shift32(mp, mul, i); vm = mul_shift32(mm, mul, i);
        if (q != 0 && (vp - 1) / 10 <= vm / 10) {
            int l = FBITS_INV + (int)pow5bits(q - 1) - 1;
            lastRemoved = (u32)(mul_shift32(mv, pow5_inv_split_b(q - 1, FBITS_INV), -e2 + (int)q - 1 + l) % 10);
        }
        if (q <= 9) {
            if (mv % 5 == 0) vrTZ = mult5(mv, q);
            else if (acceptBounds) vmTZ = mult5(mm, q);
            else vp -= mult5(mp, q);
        }
    } else {
        u32 q = log10Pow5((u32)(-e2)); e10 = (int)q + e2;
        int i = -e2 - (int)q; int k = (int)pow5bits((u32)i) - FBITS; int j = (int)q - k;
        u128 mul = pow5_split_b((u32)i, FBITS);
        vr = mul_shift32(mv, mul, j); vp = mul_shift32(mp, mul, j); vm = mul_shift32(mm, mul, j);
        if (q != 0 && (vp - 1) / 10 <= vm / 10) {
            j = (int)q - 1 - ((int)pow5bits((u32)i + 1) - FBITS);
            lastRemoved = (u32)(mul_shift32(mv, pow5_split_b((u32)i + 1, FBITS), j) % 10);
        }
        if (q <= 1) {
            vrTZ = 1;
            if (acceptBounds) vmTZ = mmShift == 1; else --vp;
        } else if (q < 31) {
            vrTZ = mult2(mv, q - 1);
        }
    }
    int removed = 0; u32 output;
    if (vmTZ || vrTZ) {
        while (vp / 10 > vm / 10) { vmTZ &= vm % 10 == 0; vrTZ &= lastRemoved == 0; lastRemoved = vr % 10; vr /= 10; vp /= 10; vm /= 10; ++removed; }
        if (vmTZ) { while (vm % 10 == 0) { vrTZ &= lastRemoved == 0; lastRemoved = vr % 10; vr /= 10; vp /= 10; vm /= 10; ++removed; } }
#ifndef NEG_NOEVEN
        if (vrTZ && lastRemoved == 5 && vr % 2 == 0) lastRemoved = 4;
#endif
        output = vr + (((vr == vm && (!acceptBounds || !vmTZ)) || lastRemoved >= 5) ? 1 : 0);
    } else {
        while (vp / 10 > vm / 10) { lastRemoved = vr % 10; vr /= 10; vp /= 10; vm /= 10; ++removed; }
        output = vr + ((vr == vm || lastRemoved >= 5) ? 1 : 0);
    }
    int exp10 = e10 + removed;
    char dg[24]; int nd = 0; u32 t = output; while (t) { dg[nd++] = (char)('0' + t % 10); t /= 10; }
    if (sign) out[n++] = '-';
    out[n++] = dg[nd - 1];
    if (nd > 1) { out[n++] = '.'; for (int i = nd - 2; i >= 0; i--) out[n++] = dg[i]; }
    out[n++] = 'E';
    int e = exp10 + nd - 1; if (e < 0) { out[n++] = '-'; e = -e; }
    char eb[8]; int ne = 0; if (e == 0) eb[ne++] = '0'; while (e) { eb[ne++] = (char)('0' + e % 10); e /= 10; }
    while (ne) out[n++] = eb[--ne];
    out[n] = 0; return n;
}
static int streq(const char *a, const char *b) { while (*a && *a == *b) { a++; b++; } return *a == *b; }

static const struct { const char *s; u64 bits; } vec[] = {
    {"0E0", 0x0000000000000000ull},
    {"-0E0", 0x8000000000000000ull},
    {"1E0", 0x3ff0000000000000ull},
    {"-1E0", 0xbff0000000000000ull},
    {"NaN", 0x7ff8000000000000ull},
    {"Infinity", 0x7ff0000000000000ull},
    {"-Infinity", 0xfff0000000000000ull},
    {"2.2250738585072014E-308", 0x0010000000000000ull},
    {"1.7976931348623157E308", 0x7fefffffffffffffull},
    {"5E-324", 0x0000000000000001ull},
    {"2.9802322387695312E-8", 0x3e60000000000000ull},
    {"-2.109808898695963E16", 0xc352bd2668e077c4ull},
    {"4.940656E-318", 0x00000000000f4240ull},
    {"1.18575755E-316", 0x00000000016e3600ull},
    {"2.989102097996E-312", 0x0000008cdcdea440ull},
    {"9.0608011534336E15", 0x434018601510c000ull},
    {"4.708356024711512E18", 0x43d055dc36f24000ull},
    {"9.409340012568248E18", 0x43e052961c6f8000ull},
    {"1.2345678E0", 0x3ff3c0ca2a5b1d5dull},
    {"5.764607523034235E39", 0x4830f0cf064dd592ull},
    {"1.152921504606847E40", 0x4840f0cf064dd592ull},
    {"2.305843009213694E40", 0x4850f0cf064dd592ull},
    {"1E0", 0x3ff0000000000000ull},
    {"1.2E0", 0x3ff3333333333333ull},
    {"1.23E0", 0x3ff3ae147ae147aeull},
    {"1.234E0", 0x3ff3be76c8b43958ull},
    {"1.2345E0", 0x3ff3c083126e978dull},
    {"1.23456E0", 0x3ff3c0c1fc8f3238ull},
    {"1.234567E0", 0x3ff3c0c9539b8887ull},
    {"1.2345678E0", 0x3ff3c0ca2a5b1d5dull},
    {"1.23456789E0", 0x3ff3c0ca4283de1bull},
    {"1.234567895E0", 0x3ff3c0ca43db770aull},
    {"1.2345678901E0", 0x3ff3c0ca428abd53ull},
    {"1.23456789012E0", 0x3ff3c0ca428c1d2bull},
    {"1.234567890123E0", 0x3ff3c0ca428c51f2ull},
    {"1.2345678901234E0", 0x3ff3c0ca428c58fcull},
    {"1.23456789012345E0", 0x3ff3c0ca428c59ddull},
    {"1.234567890123456E0", 0x3ff3c0ca428c59f8ull},
    {"1.2345678901234567E0", 0x3ff3c0ca428c59fbull},
    {"4.294967294E0", 0x40112e0be8047a7dull},
    {"4.294967295E0", 0x40112e0be815a889ull},
    {"4.294967296E0", 0x40112e0be826d695ull},
    {"4.294967297E0", 0x40112e0be83804a1ull},
    {"4.294967298E0", 0x40112e0be84932adull},
    {"1.7800590868057611E-307", 0x0040000000000000ull},
    {"2.8480945388892175E-306", 0x007fffffffffffffull},
    {"2.446494580089078E-296", 0x0290000000000000ull},
    {"4.8929891601781557E-296", 0x029fffffffffffffull},
    {"1.8014398509481984E16", 0x4350000000000000ull},
    {"3.6028797018963964E16", 0x435fffffffffffffull},
    {"2.900835519859558E-216", 0x1330000000000000ull},
    {"5.801671039719115E-216", 0x133fffffffffffffull},
    {"3.196104012172126E-27", 0x3a6fa7161a4d6e0cull},
    {"9.007199254740991E15", 0x433fffffffffffffull},
    {"9.007199254740992E15", 0x4340000000000000ull},
    {"1E0", 0x3ff0000000000000ull},
    {"1.2E1", 0x4028000000000000ull},
    {"1.23E2", 0x405ec00000000000ull},
    {"1.234E3", 0x4093480000000000ull},
    {"1.2345E4", 0x40c81c8000000000ull},
    {"1.23456E5", 0x40fe240000000000ull},
    {"1.234567E6", 0x4132d68700000000ull},
    {"1.2345678E7", 0x41678c29c0000000ull},
    {"1.23456789E8", 0x419d6f3454000000ull},
    {"1.23456789E9", 0x41d26580b4800000ull},
    {"1.234567895E9", 0x41d26580b5c00000ull},
    {"1.2345678901E10", 0x4206fee0e1a80000ull},
    {"1.23456789012E11", 0x423cbe991a140000ull},
    {"1.234567890123E12", 0x4271f71fb04cb000ull},
    {"1.2345678901234E13", 0x42a674e79c5fe400ull},
    {"1.23456789012345E14", 0x42dc12218377de40ull},
    {"1.234567890123456E15", 0x43118b54f22aeb00ull},
    {"1E0", 0x3ff0000000000000ull},
    {"1E1", 0x4024000000000000ull},
    {"1E2", 0x4059000000000000ull},
    {"1E3", 0x408f400000000000ull},
    {"1E4", 0x40c3880000000000ull},
    {"1E5", 0x40f86a0000000000ull},
    {"1E6", 0x412e848000000000ull},
    {"1E7", 0x416312d000000000ull},
    {"1E8", 0x4197d78400000000ull},
    {"1E9", 0x41cdcd6500000000ull},
    {"1E10", 0x4202a05f20000000ull},
    {"1E11", 0x42374876e8000000ull},
    {"1E12", 0x426d1a94a2000000ull},
    {"1E13", 0x42a2309ce5400000ull},
    {"1E14", 0x42d6bcc41e900000ull},
    {"1E15", 0x430c6bf526340000ull},
    {"1.000000000000001E15", 0x430c6bf526340008ull},
    {"1.00000000000001E15", 0x430c6bf526340050ull},
    {"1.0000000000001E15", 0x430c6bf526340320ull},
    {"1.000000000001E15", 0x430c6bf526341f40ull},
    {"1.00000000001E15", 0x430c6bf526353880ull},
    {"1.0000000001E15", 0x430c6bf526403500ull},
    {"1.000000001E15", 0x430c6bf526ae1200ull},
    {"1.00000001E15", 0x430c6bf52af8b400ull},
    {"1.0000001E15", 0x430c6bf555e30800ull},
    {"1.000001E15", 0x430c6bf7030a5000ull},
    {"1.00001E15", 0x430c6c07c6932000ull},
    {"1.0001E15", 0x430c6caf69eb4000ull},
    {"1.001E15", 0x430c733bcb5c8000ull},
    {"1.01E15", 0x430cb4b799c90000ull},
    {"1.1E15", 0x430f438daa060000ull},
    {"8E0", 0x4020000000000000ull},
    {"6.4E1", 0x4050000000000000ull},
    {"5.12E2", 0x4080000000000000ull},
    {"8.192E3", 0x40c0000000000000ull},
    {"6.5536E4", 0x40f0000000000000ull},
    {"5.24288E5", 0x4120000000000000ull},
    {"8.388608E6", 0x4160000000000000ull},
    {"6.7108864E7", 0x4190000000000000ull},
    {"5.36870912E8", 0x41c0000000000000ull},
    {"8.589934592E9", 0x4200000000000000ull},
    {"6.8719476736E10", 0x4230000000000000ull},
    {"5.49755813888E11", 0x4260000000000000ull},
    {"8.796093022208E12", 0x42a0000000000000ull},
    {"7.0368744177664E13", 0x42d0000000000000ull},
    {"5.62949953421312E14", 0x4300000000000000ull},
    {"9.007199254740992E15", 0x4340000000000000ull},
    {"8E3", 0x40bf400000000000ull},
    {"6.4E4", 0x40ef400000000000ull},
    {"5.12E5", 0x411f400000000000ull},
    {"8.192E6", 0x415f400000000000ull},
    {"6.5536E7", 0x418f400000000000ull},
    {"5.24288E8", 0x41bf400000000000ull},
    {"8.388608E9", 0x41ff400000000000ull},
    {"6.7108864E10", 0x422f400000000000ull},
    {"5.36870912E11", 0x425f400000000000ull},
    {"8.589934592E12", 0x429f400000000000ull},
    {"6.8719476736E13", 0x42cf400000000000ull},
    {"5.49755813888E14", 0x42ff400000000000ull},
    {"8.796093022208E15", 0x433f400000000000ull},
};

static const struct { const char *s; u32 bits; } fvec[] = {
    {"0E0", 0x00000000u},
    {"-0E0", 0x80000000u},
    {"1E0", 0x3f800000u},
    {"-1E0", 0xbf800000u},
    {"NaN", 0x7fc00000u},
    {"Infinity", 0x7f800000u},
    {"-Infinity", 0xff800000u},
    {"1.1754944E-38", 0x00800000u},
    {"3.4028235E38", 0x7f7fffffu},
    {"1E-45", 0x00000001u},
    {"3.355445E7", 0x4c000004u},
    {"9E9", 0x50061c46u},
    {"3.436672E10", 0x510006a8u},
    {"3.0540412E5", 0x48951f84u},
    {"8.0990312E3", 0x45fd1840u},
    {"2.4414062E-4", 0x39800000u},
    {"2.4414062E-3", 0x3b200000u},
    {"4.3945312E-3", 0x3b900000u},
    {"6.3476562E-3", 0x3bd00000u},
    {"4.7223665E21", 0x63800000u},
    {"8.388608E6", 0x4b000000u},
    {"1.6777216E7", 0x4b800000u},
    {"3.3554436E7", 0x4c000001u},
    {"6.7131496E7", 0x4c800b0du},
    {"1.9310392E-38", 0x00d24584u},
    {"-2.47E-43", 0x800000b0u},
    {"1.993244E-38", 0x00d90b88u},
    {"4.1039004E3", 0x45803f34u},
    {"5.3399997E9", 0x4f9f24f7u},
    {"6.0898E-39", 0x00424fe2u},
    {"1.0310042E-3", 0x3a8722c3u},
    {"2.882326E17", 0x5c800041u},
    {"7.038531E-26", 0x15ae43fdu},
    {"9.223404E17", 0x5d4cccfbu},
    {"6.710887E7", 0x4c800001u},
    {"1E-44", 0x00000007u},
    {"2.816025E14", 0x57800ed8u},
    {"9.223372E18", 0x5f000000u},
    {"1.5846086E29", 0x700000f0u},
    {"1.1811161E19", 0x5f23e9acu},
    {"5.368709E18", 0x5e9502f9u},
    {"4.6143166E18", 0x5e8012b1u},
    {"7.812537E-3", 0x3c000028u},
    {"1E-45", 0x00000001u},
    {"1.18697725E20", 0x60cde861u},
    {"1.00014165E-36", 0x03aa2a50u},
    {"2E2", 0x43480000u},
    {"3.3554432E7", 0x4c000000u},
    {"6.7108864E17", 0x5d1502f9u},
    {"1.3421773E18", 0x5d9502f9u},
    {"2.6843546E18", 0x5e1502f9u},
    {"1E0", 0x3f800000u},
    {"1.2E0", 0x3f99999au},
    {"1.23E0", 0x3f9d70a4u},
    {"1.234E0", 0x3f9df3b6u},
    {"1.2345E0", 0x3f9e0419u},
    {"1.23456E0", 0x3f9e0610u},
    {"1.234567E0", 0x3f9e064bu},
    {"1.2345678E0", 0x3f9e0651u},
    {"1.23456735E-36", 0x03d20cfeu},
};

#define NV ((int)(sizeof vec / sizeof vec[0]))
#define NFV ((int)(sizeof fvec / sizeof fvec[0]))
int paper_main(void) {
    int ok = 0, okf = 0;
    for (int i = 0; i < NV; i++) {
        char buf[48]; ryu_d2s(vec[i].bits, buf);
        int m = streq(buf, vec[i].s);
        if (m) ok++;
        else { pstr("FAIL bits "); phex(vec[i].bits, 16); pstr(": got "); pstr(buf); pstr(", published "); pstr(vec[i].s); pstr("\n"); }
    }
    pstr("published d2s test vectors (double): "); pdec(ok); pstr(" of "); pdec(NV); pstr(" reproduced\n");
    for (int i = 0; i < NFV; i++) {
        char buf[48]; ryu_f2s(fvec[i].bits, buf);
        int m = streq(buf, fvec[i].s);
        if (m) okf++;
        else { pstr("FAIL bits "); phex(fvec[i].bits, 8); pstr(": got "); pstr(buf); pstr(", published "); pstr(fvec[i].s); pstr("\n"); }
    }
    pstr("published f2s test vectors (float): "); pdec(okf); pstr(" of "); pdec(NFV); pstr(" reproduced\n");
    int good = ok == NV && okf == NFV;
    pstr(good ? "PASS\n" : "FAIL\n");
    return good ? 0 : 1;
}
