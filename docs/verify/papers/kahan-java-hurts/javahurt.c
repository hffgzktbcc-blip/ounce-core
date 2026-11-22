// Reproduction of three numerical examples in J. D. Darcy and W. Kahan, "How Java's Floating-Point Hurts Everyone Everywhere"
// (work-in-progress note, version of 30 July 2004; invited talk, ACM 1998 Workshop on Java for High-Performance Network Computing):
//  (1) the function A(x) = (x-1)/(exp(x-1)-1) computed in binary32 by the program on page 28: its worst error near x = 1 is "about
//      +-2900 = 2^11.5 ULPs", less than half the 24 bits carried;
//  (2) the nearest point on a line, example 7 on pages 51-54: with float arithmetic throughout the first formula gives z = [1, 1, -1];
//      with intermediates in double and a final rounding to float it gives the correctly rounded z = [1/3, 2/3, -4/3];
//  (3) f(x) = (tan(sin x) - sin(tan x))/x^7 at x = 0.0200000 is 0.0333486813 and needs more than double precision.
// Written for ounce-core from the programs and data in the paper; exp, sin, tan are written here (no libm); double-double arithmetic
// (Knuth/Dekker algorithms, written here) gives the high-precision value in (3).
// Negative control: -DNEG_NOROUND uses exp(y) = 1 + y.
#include "ounce_paper.h"
#ifndef HOST
// the compiler calls these for 128-bit division; the freestanding build has no compiler runtime library
static unsigned __int128 udivmod128(unsigned __int128 n, unsigned __int128 d, unsigned __int128 *rem) {
    unsigned __int128 q = 0, r = 0;
    for (int i = 127; i >= 0; i--) { r = (r << 1) | ((n >> i) & 1); if (r >= d) { r -= d; q |= (unsigned __int128)1 << i; } }
    if (rem) *rem = r; return q;
}
unsigned __int128 __udivti3(unsigned __int128 n, unsigned __int128 d) { return udivmod128(n, d, 0); }
unsigned __int128 __umodti3(unsigned __int128 n, unsigned __int128 d) { unsigned __int128 r; udivmod128(n, d, &r); return r; }
#endif
// ---- exact decimal expansion of a double (no libm, no printf) ----
// digits of |x| (finite, 2^-100 < |x| < 2^52): value = d[0].d[1]d[2]... * 10^e10; *ru = 1 when the discarded remainder is >= half
static int dbl_digits(double x, int nd, char *d, int *e10, int *ru) {
    union { double f; u64 u; } v; v.f = x;
    int ef = (int)((v.u >> 52) & 2047); u64 m = v.u & 0xFFFFFFFFFFFFFull; int e = ef - 1075;
    if (ef) m |= 1ull << 52; else e = -1074;
    if (m == 0) { for (int i = 0; i < nd; i++) d[i] = 0; *e10 = 0; *ru = 0; return 0; }
    int k = e < 0 ? -e : 0; unsigned __int128 num = (unsigned __int128)m << (e > 0 ? e : 0);
    unsigned __int128 ip = k ? (num >> k) : num;
    unsigned __int128 rem = k ? (num & (((unsigned __int128)1 << k) - 1)) : 0;
    int n = 0, ex = 0; char tmp[48]; int nt = 0;
    if (ip) { while (ip) { tmp[nt++] = (char)(ip % 10); ip /= 10; } ex = nt - 1; for (int i = nt - 1; i >= 0 && n < nd; i--) d[n++] = tmp[i]; }
    else { ex = -1; for (;;) { rem *= 10; int dg = (int)(rem >> k); rem &= (((unsigned __int128)1 << k) - 1); if (dg) { d[n++] = (char)dg; break; } ex--; } }
    while (n < nd) { rem *= 10; d[n++] = (char)(rem >> k); rem &= (((unsigned __int128)1 << k) - 1); }
    // remainder after the digits taken (only exact if the integer part had <= nd digits, which holds in our uses)
    unsigned __int128 half = k ? ((unsigned __int128)1 << (k - 1)) : 0;
    *ru = k ? rem >= half && (rem > half || 1) : 0;   // ties round up
    *e10 = ex; return 1;
}
static void digits_round(char *d, int nd, int *e10) {
    int i = nd - 1;
    while (i >= 0) { if (d[i] == 9) { d[i] = 0; i--; } else { d[i]++; break; } }
    if (i < 0) { d[0] = 1; (*e10)++; }
}
// plain positional formatting of digits with value d0.d1d2.. * 10^e10, for -5 <= e10 <= 15
static void dig_fmt(char *out, int neg, const char *d, int nd, int e10) {
    int n = 0; if (neg) out[n++] = '-';
    if (e10 < 0) { out[n++] = '0'; out[n++] = '.'; for (int i = -1; i > e10; i--) out[n++] = '0'; for (int i = 0; i < nd; i++) out[n++] = (char)('0' + d[i]); }
    else { for (int i = 0; i < nd; i++) { if (i == e10 + 1) out[n++] = '.'; out[n++] = (char)('0' + d[i]); } for (int i = nd; i <= e10; i++) out[n++] = '0'; }
    out[n] = 0;
}
// x printed with nd significant digits, rounded (mode 0) or truncated (mode 1)
static void dbl_str(double x, int nd, int mode, char *out) {
    char d[32]; int e10, ru; int neg = x < 0; if (neg) x = -x;
    if (x == 0) { out[0] = '0'; out[1] = 0; return; }
    dbl_digits(x, nd, d, &e10, &ru);
    if (mode == 0 && ru) digits_round(d, nd, &e10);
    dig_fmt(out, neg, d, nd, e10);
}
static int streq(const char *a, const char *b) { while (*a && *a == *b) { a++; b++; } return *a == *b; }

typedef union { float f; u32 u; } FU;
static double pow2d(int e) { union { double d; u64 u; } v; v.u = (u64)(e + 1023) << 52; return v.d; }
static double expd(double x) {
    const double ln2hi = 0.693147180369123816490, ln2lo = 1.90821492927058770002e-10;
    int k = (int)(x / 0.6931471805599453 + (x < 0 ? -0.5 : 0.5));
    double r = (x - (double)k * ln2hi) - (double)k * ln2lo, t = 1.0, s = 1.0;
    for (int i = 1; i < 25; i++) { t = t * r / (double)i; s += t; }
    return s * pow2d(k);
}
static float expf_(float y) {
#ifdef NEG_NOROUND
    return 1.0f + y;   // first-order exp: drops the y*y/2 term
#else
    return (float)expd((double)y);
#endif
}
static double dabs(double x) { return x < 0 ? -x : x; }
static int bad = 0;
static void verdict(const char *what, int ok) { pstr(ok ? "  ok: " : "  FAIL: "); pstr(what); pstr("\n"); if (!ok) bad++; }

// ---- (1) A(x) in binary32 ----
static double a_true(double y) {   // A = 1 - y/2 + y^2/12 - y^4/720 + y^6/30240 - y^8/1209600 + ..., |y| < 0.01
    double y2 = y * y;
    return 1.0 - y / 2.0 + y2 / 12.0 - y2 * y2 / 720.0 + y2 * y2 * y2 / 30240.0 - y2 * y2 * y2 * y2 / 1209600.0;
}
static float a_prog(float X) {   // the paper's program: Y = X - 1; Z = EXP(Y); if Z != 1 then Z = Y/(Z-1)
    float Y = X - 1.0f, Z = expf_(Y);
    if (Z != 1.0f) Z = Y / (Z - 1.0f);
    return Z;
}
static double ulp_of(double v) {   // binary32 ulp of the binade of v (v > 0)
    union { double d; u64 u; } x; x.d = v; int e = (int)((x.u >> 52) & 2047) - 1023; return pow2d(e - 23);
}
// print v*100 rounded as an integer, as "v.vv"
static void p2(double v) { if (v < 0) { pstr("-"); v = -v; } i64 c = (i64)(v * 100.0 + 0.5); pdec(c / 100); pstr("."); pdec((c / 10) % 10); pdec(c % 10); }

// ---- (2) nearest point on a line ----
typedef float F; typedef double D;
static void crossF(const F *a, const F *b, F *r) { r[0] = a[1] * b[2] - a[2] * b[1]; r[1] = a[2] * b[0] - a[0] * b[2]; r[2] = a[0] * b[1] - a[1] * b[0]; }
static F dotF(const F *a, const F *b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
static void crossD(const D *a, const D *b, D *r) { r[0] = a[1] * b[2] - a[2] * b[1]; r[1] = a[2] * b[0] - a[0] * b[2]; r[2] = a[0] * b[1] - a[1] * b[0]; }
static D dotD(const D *a, const D *b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }

// ---- (3) double-double ----
typedef struct { double hi, lo; } DD;
static DD two_sum(double a, double b) { double s = a + b, z = s - a; DD r; r.hi = s; r.lo = (a - (s - z)) + (b - z); return r; }
static DD quick(double a, double b) { double s = a + b; DD r; r.hi = s; r.lo = b - (s - a); return r; }
static DD dd_add(DD a, DD b) { DD s = two_sum(a.hi, b.hi), t = two_sum(a.lo, b.lo); double c = s.lo + t.hi; DD u = quick(s.hi, c); c = t.lo + u.lo; return quick(u.hi, c); }
static DD dd_neg(DD a) { a.hi = -a.hi; a.lo = -a.lo; return a; }
static DD dd_sub(DD a, DD b) { return dd_add(a, dd_neg(b)); }
static DD dd_mul(DD a, DD b) { double p = a.hi * b.hi; DD r; r.hi = p; r.lo = __builtin_fma(a.hi, b.hi, -p) + (a.hi * b.lo + a.lo * b.hi); return quick(r.hi, r.lo); }
static DD dd_div(DD a, DD b) {
    double q1 = a.hi / b.hi; DD r = dd_sub(a, dd_mul(b, (DD){q1, 0}));
    double q2 = r.hi / b.hi; r = dd_sub(r, dd_mul(b, (DD){q2, 0}));
    double q3 = r.hi / b.hi; DD q = quick(q1, q2); return dd_add(q, (DD){q3, 0});
}
static DD dd_from(double x) { DD r; r.hi = x; r.lo = 0; return r; }
static DD dd_sin(DD x) { DD x2 = dd_mul(x, x), t = x, s = x; for (int k = 1; k < 16; k++) { t = dd_mul(t, x2); t = dd_div(t, dd_from((double)((2 * k) * (2 * k + 1)))); t = dd_neg(t); s = dd_add(s, t); } return s; }
static DD dd_cos(DD x) { DD x2 = dd_mul(x, x), t = dd_from(1.0), s = dd_from(1.0); for (int k = 1; k < 16; k++) { t = dd_mul(t, x2); t = dd_div(t, dd_from((double)((2 * k - 1) * (2 * k)))); t = dd_neg(t); s = dd_add(s, t); } return s; }
static DD dd_tan(DD x) { return dd_div(dd_sin(x), dd_cos(x)); }
static double f_plain(double x) {   // the same expression in plain double with Taylor sin, cos
    double x2, t, s;
    #define SIN_D(arg, out) { double a_ = (arg); x2 = a_ * a_; t = a_; s = a_; for (int k = 1; k < 16; k++) { t = -t * x2 / (double)((2 * k) * (2 * k + 1)); s += t; } out = s; }
    #define COS_D(arg, out) { double a_ = (arg); x2 = a_ * a_; t = 1.0; s = 1.0; for (int k = 1; k < 16; k++) { t = -t * x2 / (double)((2 * k - 1) * (2 * k)); s += t; } out = s; }
    double sx, cx, tsx, ssx, csx, tx, stx, ctx, sinx, cosx; (void)cx;
    SIN_D(x, sx); COS_D(x, cx);
    SIN_D(sx, ssx); COS_D(sx, csx); tsx = ssx / csx;                 // tan(sin x)
    SIN_D(x, sinx); COS_D(x, cosx); tx = sinx / cosx;                // tan x
    SIN_D(tx, stx); (void)ctx;                                        // sin(tan x)
    double x7 = x * x; x7 = x7 * x7 * x7 * x;
    return (tsx - stx) / x7;
}

int paper_main(void) {
    // (1)
    pstr("(1) A(x) = (x-1)/(exp(x-1)-1) in binary32, x within 20000 ulps of 1\n");
    double worst = 0, worst_y = 0;
    for (int side = 0; side < 2; side++) {
        FU v; v.f = 1.0f;
        for (int k = 0; k <= 20000; k++) {
            if (k > 0) { if (side == 0) v.u += 1; else v.u -= 1; }
            float X = v.f; double y = (double)X - 1.0;
            float got = a_prog(X); double tr = y == 0.0 ? 1.0 : a_true(y);
            double err = ((double)got - tr) / ulp_of(tr); if (err < 0) err = -err;
            if (err > worst) { worst = err; worst_y = y; }
        }
    }
    pstr("  worst error in ulps = "); p2(worst); pstr(" at x - 1 = "); { double yv = worst_y; if (yv < 0) { pstr("-"); yv = -yv; } pstr("0."); i64 d = (i64)(yv * 1e9 + 0.5); char b[9]; for (int i = 8; i >= 0; i--) { b[i] = (char)('0' + d % 10); d /= 10; } paper_write(b, 9); } pstr("\n");
    // paper: about +-2900 = 2^11.5 ulps; less than half the 24 bits lost, i.e. below 2^12 = 4096; tolerance written before the first run: 2500 .. 3300
    verdict("worst error is within 2500..3300 ulps (paper: about 2900 = 2^11.5)", worst >= 2500.0 && worst <= 3300.0);
    verdict("worst error is below 2^12 ulps: less than half of the 24 bits are lost", worst < 4096.0);

    // (2)
    pstr("(2) nearest point on a line, data p = [38006 23489 14517], pi = 8972, b = [23489 14517 8972], beta = 5545, y = [1 -1 1]\n");
    {
        F p[3] = {38006, 23489, 14517}, b[3] = {23489, 14517, 8972}, y[3] = {1, -1, 1}; F pi_ = 8972, be = 5545;
        F v[3], w[3], u[3], z[3]; crossF(p, b, v); F vv = dotF(v, v);
        F t1 = be - dotF(b, y), t2 = pi_ - dotF(p, y);
        for (int i = 0; i < 3; i++) w[i] = p[i] * t1 - b[i] * t2;
        crossF(v, w, u); for (int i = 0; i < 3; i++) z[i] = y[i] + u[i] / vv;
        pstr("  all float: z1 = ["); pdec((i64)z[0]); pstr(" "); pdec((i64)z[1]); pstr(" "); pdec((i64)z[2]); pstr("]\n");
        verdict("float formula 1 gives z1 = [1, 1, -1] (paper)", z[0] == 1.0f && z[1] == 1.0f && z[2] == -1.0f);
        D pd[3] = {38006, 23489, 14517}, bd[3] = {23489, 14517, 8972}, yd[3] = {1, -1, 1}; D pid = 8972, bed = 5545;
        D vd[3], wd[3], ud[3], zd[3]; crossD(pd, bd, vd); D vvd = dotD(vd, vd);
        D t1d = bed - dotD(bd, yd), t2d = pid - dotD(pd, yd);
        for (int i = 0; i < 3; i++) wd[i] = pd[i] * t1d - bd[i] * t2d;
        crossD(vd, wd, ud); for (int i = 0; i < 3; i++) zd[i] = yd[i] + ud[i] / vvd;
        float zf0 = (float)zd[0], zf1 = (float)zd[1], zf2 = (float)zd[2];
        verdict("subexpressions in double, rounded to float at the end: z = [1/3, 2/3, -4/3] correctly rounded (paper)",
                zf0 == (float)(1.0 / 3.0) && zf1 == (float)(2.0 / 3.0) && zf2 == (float)(-4.0 / 3.0));
        verdict("the float and the double-then-float results differ", z[0] != zf0);
    }

    // (3)
    pstr("(3) f(x) = (tan(sin x) - sin(tan x))/x^7 at x = 0.02\n");
    {
        DD x = dd_from(0.02);   // the double nearest 0.02, as the data of the paper "0.0200000"
        DD sx = dd_sin(x), tsx = dd_tan(sx), tx = dd_tan(x), stx = dd_sin(tx);
        DD x2 = dd_mul(x, x), x7 = dd_mul(dd_mul(dd_mul(x2, x2), x2), x);
        DD f = dd_div(dd_sub(tsx, stx), x7);
        char a[48], b[48]; dbl_str(f.hi, 9, 0, a); dbl_str(f.hi, 9, 1, b);
        double fp = f_plain(0.02);
        char c[48]; dbl_str(fp, 9, 0, c);
        pstr("  double-double: f = "); pstr(a); pstr(" (9 digits, rounded)\n  plain double:  f = "); pstr(c); pstr("\n");
        verdict("f(0.02) = 0.0333486813 to the printed 9 significant digits (paper)", streq(a, "0.0333486813") || streq(b, "0.0333486813"));
        double rel = dabs(fp - f.hi) / f.hi;
        pstr("  relative error of the plain double result = "); { int e = 0; double r = rel; if (r == 0) pstr("0"); else { while (r < 1.0) { r *= 10.0; e--; } pdec((i64)(r * 10.0 + 0.5) / 10); pstr("."); pdec((i64)(r * 10.0 + 0.5) % 10); pstr("e"); pdec(e); } } pstr("\n");
        verdict("plain double arithmetic does not deliver 9 correct digits (relative error above 1e-9; paper: more than double precision is needed)", rel > 1e-9);
    }
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
