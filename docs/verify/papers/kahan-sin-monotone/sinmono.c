// Reproduction of the monotonicity example in W. Kahan, "Mathematics Written in Sand" (Proc. Joint Statistical Meeting of the
// American Statistical Association, 1983; version of 22 Nov. 1983, retypeset 2001): F(x) := x - SIN(x), where SIN(x) is sin(x)
// correctly rounded to p significant decimals, is monotone non-decreasing like x - sin(x) "everywhere except at a scattered
// handful of exceptions": for p = 6 the sole exception is at x = 0.100167, for p = 5 at x = 0.010000, for p = 4 nowhere.
// Written for ounce-core from the statement and the table in the paper: every p-digit decimal x in [0.001, 1) is enumerated, sin(x)
// is computed in double with a Taylor series (no libm) and rounded to p decimals, F is formed exactly in integers.
// Negative control: -DNEG_TRUNC truncates SIN(x) instead of rounding it.
#include "ounce_paper.h"
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

static double pw10[24];
static double sin_d(double x) {   // |x| < 1.6
    double x2 = x * x, t = x, s = x;
    for (int k = 1; k < 12; k++) { t = -t * x2 / (double)((2 * k) * (2 * k + 1)); s += t; }
    return s;
}
// round (or truncate) s > 0 to p significant decimals: returns the integer N (10^(p-1) <= N < 10^p) and the exponent: s ~ N * 10^-(p+m)
static i64 round_sig(double s, int p, int *m, int *unsafe) {
    int d = 0;   // s in [10^-d-1.. ) : find m such that 10^(p-1) <= s * 10^(p+m) < 10^p
    int mm = 0;
    for (;;) { double v = s * pw10[p + mm]; if (v >= pw10[p - 1] && v < pw10[p]) break; mm++; if (mm > 8) break; }
    (void)d;
    double v = s * pw10[p + mm];
    i64 N = (i64)v; double fr = v - (double)N;
    double dd = fr - 0.5; if (dd < 0) dd = -dd;
    if (dd < 1e-8) (*unsafe)++;
    if (fr > 0.5) N++;
#ifdef NEG_TRUNC
    N = (i64)v;
#endif
    if (N >= (i64)pw10[p]) { N /= 10; mm--; }
    *m = mm; return N;
}
static i64 ipow10(int k) { i64 r = 1; while (k-- > 0) r *= 10; return r; }
static void print_scaled(i64 v, int decimals) {   // v * 10^-decimals
    if (v < 0) { pstr("-"); v = -v; }
    i64 ip = v / ipow10(decimals), fp = v % ipow10(decimals);
    pdec(ip); pstr("."); char b[24]; for (int i = decimals - 1; i >= 0; i--) { b[i] = (char)('0' + fp % 10); fp /= 10; } paper_write(b, decimals);
}
static int bad = 0;
static void verdict(const char *what, int ok) { pstr(ok ? "  ok: " : "  FAIL: "); pstr(what); pstr("\n"); if (!ok) bad++; }

// scan one precision; records exceptions (F decreases between consecutive p-digit decimals); returns their count
typedef struct { i64 n0, n1; int m; i64 f0, f1; } Exc;
static int scan(int p, Exc *ex, int maxex, int *unsafe) {
    int nex = 0; i64 prevF = -1; i64 prevN = 0; int prevm = 0; int first = 1;
    for (int m = 2; m >= 0; m--) {         // decades [0.001,0.01), [0.01,0.1), [0.1,1)
        i64 lo = ipow10(p - 1), hi = ipow10(p);
        for (i64 N = lo; N < hi; N++) {
            double x = (double)N / pw10[p + m];
            double s = sin_d(x);
            int ms; i64 S = round_sig(s, p, &ms, unsafe);
            i64 Xu = N * ipow10(3 - m), Su = S * ipow10(3 - ms);   // units of 10^-(p+3)
            i64 F = Xu - Su;
            if (!first && F < prevF && nex < maxex) { ex[nex].n0 = prevN; ex[nex].n1 = N; ex[nex].m = m; ex[nex].f0 = prevF; ex[nex].f1 = F; nex++; }
            else if (!first && F < prevF) nex++;
            prevF = F; prevN = N; prevm = m; first = 0;
        }
    }
    (void)prevm; return nex;
}

int paper_main(void) {
    pw10[0] = 1.0; for (int i = 1; i < 24; i++) pw10[i] = pw10[i - 1] * 10.0;
    // the published table rows
    pstr("table rows of the paper: sin(x) to the printed digits\n");
    { static const struct { double x; const char *sin; } t[4] = { {0.100167, "0.09999958095"}, {0.100168, "0.1000005759"}, {0.010000, "0.009999833334"}, {0.010001, "0.01000083328"} };
      for (int i = 0; i < 4; i++) {
          double s = sin_d(t[i].x); int nd = 0, st = 0; for (int j = 0; t[i].sin[j]; j++) { char c = t[i].sin[j]; if (c >= '0' && c <= '9') { if (c != '0') st = 1; if (st) nd++; } }
          char a[48], b[48]; dbl_str(s, nd, 0, a); dbl_str(s, nd, 1, b);
          int ok = streq(a, t[i].sin) || streq(b, t[i].sin);
          pstr("  sin("); { char xs[24]; dbl_str(t[i].x, 6, 0, xs); pstr(xs); } pstr(") = "); pstr(a); pstr(" ; paper "); pstr(t[i].sin); pstr(ok ? " ok\n" : " DIFFERS\n"); if (!ok) bad++;
      } }
    Exc ex[8]; int unsafe = 0;
    int nex6, nex5, nex4;
    pstr("p = 6\n"); nex6 = scan(6, ex, 8, &unsafe);
    pstr("  exceptions found for x in [0.001, 1): "); pdec(nex6); pstr("\n");
    for (int i = 0; i < nex6 && i < 8; i++) { pstr("    x = "); print_scaled(ex[i].n0 * ipow10(2 - ex[i].m), 8); pstr(" -> "); print_scaled(ex[i].n1 * ipow10(2 - ex[i].m), 8); pstr(": F = "); print_scaled(ex[i].f0, 9); pstr(" -> "); print_scaled(ex[i].f1, 9); pstr("\n"); }
    int ok6 = nex6 == 1 && ex[0].n0 == 100167 && ex[0].m == 0 && ex[0].f0 == 167400 && ex[0].f1 == 167000;
    verdict("p = 6: the sole exception is at x = 0.100167 (F = 0.0001674 then 0.0001670, as in the paper's table)", ok6);
    Exc ex5[8]; pstr("p = 5\n"); nex5 = scan(5, ex5, 8, &unsafe);
    pstr("  exceptions found for x in [0.001, 1): "); pdec(nex5); pstr("\n");
    for (int i = 0; i < nex5 && i < 8; i++) { pstr("    x = "); print_scaled(ex5[i].n0 * ipow10(2 - ex5[i].m), 7); pstr(" -> "); print_scaled(ex5[i].n1 * ipow10(2 - ex5[i].m), 7); pstr(": F = "); print_scaled(ex5[i].f0, 8); pstr(" -> "); print_scaled(ex5[i].f1, 8); pstr("\n"); }
    int ok5 = nex5 == 1 && ex5[0].n0 == 10000 && ex5[0].m == 1 && ex5[0].f0 == 20 && ex5[0].f1 == 0;
    verdict("p = 5: the sole exception is at x = 0.010000 (F = 0.0000002 then 0, as in the paper's table)", ok5);
    Exc ex4[8]; pstr("p = 4\n"); nex4 = scan(4, ex4, 8, &unsafe);
    pstr("  exceptions found for x in [0.001, 1): "); pdec(nex4); pstr("\n");
    verdict("p = 4: no exception (paper: nowhere)", nex4 == 0);
    pstr("values of sin(x) within 1e-8 of a rounding tie (would make the rounding ambiguous): "); pdec(unsafe); pstr("\n");
    verdict("no ambiguous roundings", unsafe == 0);
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
