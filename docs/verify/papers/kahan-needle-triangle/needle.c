// Reproduction of Table 1 in W. Kahan, "Miscalculating Area and Angles of a Needle-like Triangle" (lecture notes, version of
// 4 September 2014): area and angle C of twelve triangles, computed with Heron's formula and with Kahan's formulas, in the
// paper's own arithmetic (10 significant decimals, the HP-15C), which this file emulates with integers.
// Written for ounce-core from the formulas in sections 1-3 and 5 of the paper; no third-party code.
// Negative control: -DNEG_HERON_PARENS evaluates Kahan's area formula with the parentheses removed (a+b+c)(c-a+b)...
#include "ounce_paper.h"
typedef unsigned __int128 u128;
typedef __int128 i128;
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

#define P 10
typedef struct { int neg; u64 m; int e; int err; } Dec;   // value (neg ? -1 : 1) * m * 10^e; m = 0 or 10^9 <= m < 10^10; err: "Error"
static u64 p10(int k) { u64 r = 1; while (k-- > 0) r *= 10; return r; }
static Dec dnorm(int neg, u128 m, int e, int sticky) {   // round to P digits, ties away from zero (as a calculator does)
    Dec r; r.neg = neg; r.m = 0; r.e = 0; r.err = 0;
    if (m == 0) { r.neg = 0; return r; }
    int n = 0; u128 t = m; while (t) { n++; t /= 10; }
    if (n > P) {
        int drop = n - P; u128 pw = 1; for (int i = 0; i < drop; i++) pw *= 10;
        u128 q = m / pw, rem = m % pw, half = pw / 2;
        if (rem >= half) q++;
        (void)sticky;
        e += drop; m = q;
        if (m == (u128)p10(P)) { m = p10(P - 1); e++; }
    } else { for (int i = n; i < P; i++) { m *= 10; e--; } }
    r.m = (u64)m; r.e = e; return r;
}
static Dec dzero(void) { return dnorm(0, 0, 0, 0); }
static Dec derr(void) { Dec r = dzero(); r.err = 1; return r; }
static Dec dadd(Dec a, Dec b) {
    if (a.err || b.err) return derr();
    if (a.m == 0) return b; if (b.m == 0) return a;
    int e = a.e < b.e ? a.e : b.e; u128 ma = a.m, mb = b.m;
    for (int i = a.e; i > e; i--) ma *= 10; for (int i = b.e; i > e; i--) mb *= 10;
    if (a.neg == b.neg) return dnorm(a.neg, ma + mb, e, 0);
    if (ma >= mb) return dnorm(a.neg, ma - mb, e, 0);
    return dnorm(b.neg, mb - ma, e, 0);
}
static Dec dneg(Dec a) { if (a.m && !a.err) a.neg = !a.neg; return a; }
static Dec dsub(Dec a, Dec b) { return dadd(a, dneg(b)); }
static Dec dmul(Dec a, Dec b) { if (a.err || b.err) return derr(); return dnorm(a.neg != b.neg, (u128)a.m * b.m, a.e + b.e, 0); }
static Dec ddiv(Dec a, Dec b) {
    if (a.err || b.err || b.m == 0) return derr();
    u128 num = (u128)a.m * p10(16); u128 q = num / b.m;
    return dnorm(a.neg != b.neg, q, a.e - b.e - 16, 0);
}
static Dec dsqrt(Dec a) {
    if (a.err || (a.m && a.neg)) return derr();
    if (a.m == 0) return a;
    u128 m = a.m; int e = a.e; if (e & 1) { m *= 10; e--; }
    u128 X = m * p10(16); int j = 8;   // sqrt(m 10^e) = sqrt(m 10^16) 10^(e/2 - 8)
    u128 lo = 0, hi = (u128)1 << 63; while (lo < hi) { u128 mid = (lo + hi + 1) / 2; if (mid * mid <= X) lo = mid; else hi = mid - 1; }
    return dnorm(0, lo, e / 2 - j, 0);
}
static Dec dparse(const char *s) {   // [-]ddd.ddd[E[-]dd]
    int neg = 0, i = 0; if (s[0] == '-') { neg = 1; i = 1; }
    u128 m = 0; int e = 0, seen = 0;
    for (; s[i] && s[i] != 'E'; i++) { if (s[i] == '.') seen = 1; else { m = m * 10 + (u128)(s[i] - '0'); if (seen) e--; } }
    if (s[i] == 'E') { i++; int en = 0; if (s[i] == '-') { en = 1; i++; } int x = 0; for (; s[i]; i++) x = x * 10 + (s[i] - '0'); e += en ? -x : x; }
    return dnorm(neg, m, e, 0);
}
static int dless0(Dec a) { return !a.err && a.m && a.neg; }
static int dge(Dec a, Dec b) { Dec d = dsub(a, b); return !d.err && !(d.m && d.neg); }
static double pow10d(int e) { double r = 1.0; if (e >= 0) while (e-- > 0) r *= 10.0; else while (e++ < 0) r /= 10.0; return r; }
static double d2f(Dec a) { double v = (double)a.m * pow10d(a.e); return a.neg ? -v : v; }
static i128 dround_at(Dec a, int e) {   // a / 10^e rounded half away from zero, as an integer
    if (a.m == 0) return 0;
    i128 r; int sh = a.e - e;
    if (sh >= 0) { u128 v = a.m; for (int i = 0; i < sh; i++) v *= 10; r = (i128)v; }
    else { u128 pw = 1; for (int i = 0; i < -sh; i++) pw *= 10; u128 q = a.m / pw, rem = a.m % pw; if (rem * 2 >= pw) q++; r = (i128)q; }
    return a.neg ? -r : r;
}
// published value: text, with its last digit's decimal exponent
typedef struct { int err; i128 N; int e; double v; } Pub;
static Pub pub_parse(const char *s) {
    Pub p; p.err = 0; p.N = 0; p.e = 0; p.v = 0;
    if (s[0] == 'E' && s[1] == 'r') { p.err = 1; return p; }
    int neg = 0, i = 0; if (s[0] == '-') { neg = 1; i = 1; }
    int e = 0, seen = 0; i128 m = 0;
    for (; s[i] && s[i] != 'E'; i++) { if (s[i] == '.') seen = 1; else { m = m * 10 + (s[i] - '0'); if (seen) e--; } }
    if (s[i] == 'E') { i++; int en = 0; if (s[i] == '-') { en = 1; i++; } int x = 0; for (; s[i]; i++) x = x * 10 + (s[i] - '0'); e += en ? -x : x; }
    p.N = neg ? -m : m; p.e = e; p.v = (double)(i64)m * pow10d(e); if (neg) p.v = -p.v; return p;
}
static int bad = 0;
static void pdec128(i128 v) { pdec((i64)v); }
static void show(Dec a) {
    if (a.err) { pstr("Error"); return; }
    if (a.m == 0) { pstr("0"); return; }
    if (a.neg) pstr("-");
    char d[12]; u64 m = a.m; for (int i = P - 1; i >= 0; i--) { d[i] = (char)('0' + m % 10); m /= 10; }
    int pt = a.e + P;
    if (pt > 0 && pt <= 12) { for (int i = 0; i < P; i++) { if (i == pt) pstr("."); paper_write(&d[i], 1); } for (int i = P; i < pt; i++) pstr("0"); }
    else { paper_write(d, 1); pstr("."); paper_write(d + 1, P - 1); pstr("E"); pdec(pt - 1); }
}

// ---- atan in double (no libm) ----
static double atan_d(double x) {   // x >= 0
    if (x > 1.0) return 1.5707963267948966 - atan_d(1.0 / x);
    for (int i = 0; i < 3; i++) x = x / (1.0 + paper_sqrt(1.0 + x * x));
    double x2 = x * x, t = x, s = x;
    for (int k = 1; k < 14; k++) { t = -t * x2; s += t / (double)(2 * k + 1); }
    return 8.0 * s;
}

// ---- the paper's formulas in decimal arithmetic ----
static void sort3(Dec *a, Dec *b, Dec *c) {
    Dec t; if (!dge(*a, *b)) { t = *a; *a = *b; *b = t; } if (!dge(*b, *c)) { t = *b; *b = *c; *c = t; } if (!dge(*a, *b)) { t = *a; *a = *b; *b = t; }
}
static Dec heron(Dec a, Dec b, Dec c) {
    Dec two = dparse("2"); Dec s = ddiv(dadd(dadd(a, b), c), two);
    Dec prod = dmul(dmul(dmul(s, dsub(s, a)), dsub(s, b)), dsub(s, c));
    return dsqrt(prod);
}
static Dec area(Dec a, Dec b, Dec c) {
    sort3(&a, &b, &c);
    Dec f2 = dsub(c, dsub(a, b));
    if (dless0(f2)) return derr();
#ifdef NEG_HERON_PARENS
    Dec f1 = dsub(dadd(dadd(a, b), c), dparse("0")), f3 = dadd(c, dsub(a, b)), f4 = dsub(dadd(a, b), c);
    f2 = dadd(dsub(c, a), b); f3 = dsub(dadd(c, a), b);
    Dec r = dsqrt(dmul(dmul(dmul(f1, f2), f3), f4)); return dmul(r, dparse("0.25"));
#else
    Dec f1 = dadd(a, dadd(b, c)), f3 = dadd(c, dsub(a, b)), f4 = dadd(a, dsub(b, c));
    Dec r = dsqrt(dmul(dmul(dmul(f1, f2), f3), f4));
    return dmul(r, dparse("0.25"));
#endif
}
// angle C in degrees: returns 1 and sets *deg, or 0 for "Error"
static int angle(Dec a, Dec b, Dec c, double *deg) {
    if (!dge(a, b)) { Dec t = a; a = b; b = t; }
    Dec mu;
    if (dge(b, c) && dge(c, dzero())) mu = dsub(c, dsub(a, b));
    else if (dge(c, b) && dge(b, dzero()) && !(c.m == b.m && c.e == b.e && c.neg == b.neg)) mu = dsub(b, dsub(a, c));
    else return 0;
    Dec num = dmul(dadd(dsub(a, b), c), mu);
    Dec den = dmul(dadd(a, dadd(b, c)), dadd(dsub(a, c), b));
    if (num.err || den.err) return 0;
    if (den.m == 0) { if (num.m == 0) return 0; if (num.neg) return 0; *deg = 180.0; return 1; }
    Dec q = ddiv(num, den);
    Dec r = dsqrt(q);
    if (r.err) return 0;
    *deg = atan_d(d2f(r)) * 114.59155902616465;
    return 1;
}

static const char *rows[12][3] = {
    {"10", "10", "10"}, {"-3", "4", "2"}, {"100000", "99999.99979", "0.00029"}, {"100000", "100000", "1.00005"},
    {"99999.99996", "99999.99994", "0.00003"}, {"99999.99996", "0.00003", "99999.99994"}, {"10000", "5000.000001", "15000"},
    {"99999.99999", "99999.99999", "200000"}, {"5278.64055", "94721.35941", "99999.99996"}, {"100002", "100002", "200004"},
    {"31622.77662", "0.000023", "31622.77661"}, {"31622.77662", "0.0155555", "31622.77661"} };
// published columns: Heron's area, accurate area, accurate angle C (degrees)
static const char *pubH[12] = {"43.30127019", "2.905", "17.6", "50010.0", "Error", "Error", "0", "0", "Error", "0", "0.447", "246.18"};
static const char *pubA[12] = {"43.30127020", "Error", "9.999999990", "50002.50003", "1.118033988", "1.118033988", "612.3724358", "Error", "0", "0", "0.327490458", "245.9540000"};
static const char *pubC[12] = {"60", "Error", "1.145915591E-7", "5.72986443E-4", "1.281172578E-8", "48.18968510", "179.9985965", "Error", "180", "180", "64.22853824", "89.96315276"};

int paper_main(void) {
    pstr("Table 1 of the paper, 10 significant decimals\n");
    pstr("row  Heron's area (computed | published)   accurate area   accurate angle C in degrees\n");
    int hexact = 0, aexact = 0, aclose = 0, cclose = 0, hrows_ok = 0;
    for (int r = 0; r < 12; r++) {
        Dec a = dparse(rows[r][0]), b = dparse(rows[r][1]), c = dparse(rows[r][2]);
        Dec h = heron(a, b, c), ar = area(a, b, c);
        double deg = 0; int cok = angle(a, b, c, &deg);
        Pub ph = pub_parse(pubH[r]), pa = pub_parse(pubA[r]), pc = pub_parse(pubC[r]);
        // Heron: exact match at the printed digits (or Error / 0)
        int hmatch = ph.err ? h.err : (!h.err && dround_at(h, ph.e) == ph.N);
        // accurate area: exact match at the printed digits counts as exact; the pass criterion is relative 2e-9
        int amatch = pa.err ? ar.err : (!ar.err && dround_at(ar, pa.e) == pa.N);
        int aok = amatch;
        if (!amatch && !pa.err && !ar.err) { double v = d2f(ar), d = v - pa.v; if (d < 0) d = -d; double ap = pa.v < 0 ? -pa.v : pa.v; aok = pa.v == 0 ? v == 0 : d <= 2e-9 * ap; }
        int cmatch;
        if (pc.err) cmatch = !cok; else { double d = deg - pc.v; if (d < 0) d = -d; cmatch = cok && (pc.v == 0 ? deg == 0 : d <= 2e-9 * pc.v); }
        hexact += hmatch; aexact += amatch; aclose += aok; cclose += cmatch;
        pdec(r + 1); pstr("  "); show(h); pstr(" | "); pstr(pubH[r]); pstr(hmatch ? " ok" : " DIFFERS"); pstr("   "); show(ar); pstr(" | "); pstr(pubA[r]); pstr(aok ? (amatch ? " ok" : " ok(close)") : " DIFFERS");
        pstr("   "); if (cok) { char s[40]; // print degrees with 9 significant digits via scaled integer
            double v = deg; int ex = 0; if (v != 0) { while (v >= 10.0) { v /= 10.0; ex++; } while (v < 1.0) { v *= 10.0; ex--; } }
            i64 mant = (i64)(v * 1e8 + 0.5); pdec(mant / 100000000); pstr("."); { i64 f = mant % 100000000; char b[9]; for (int i = 7; i >= 0; i--) { b[i] = (char)('0' + f % 10); f /= 10; } paper_write(b, 8); } pstr("E"); pdec(ex); (void)s; }
        else pstr("Error");
        pstr(" | "); pstr(pubC[r]); pstr(cmatch ? " ok\n" : " DIFFERS\n");
        if (!hmatch || !aok || !cmatch) bad++;
        hrows_ok += hmatch;
    }
    pstr("Heron's area column: "); pdec(hexact); pstr(" of 12 rows equal the printed digits\n");
    pstr("accurate area column: "); pdec(aexact); pstr(" of 12 rows equal the printed digits, "); pdec(aclose); pstr(" within relative 2e-9\n");
    pstr("accurate angle column: "); pdec(cclose); pstr(" of 12 rows within relative 2e-9 (or Error where the paper says Error)\n");
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
