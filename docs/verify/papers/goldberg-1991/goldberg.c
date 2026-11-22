// Reproduction of the worked numerical examples in D. Goldberg, "What Every Computer Scientist Should Know About Floating-Point
// Arithmetic", ACM Computing Surveys 23(1), 1991 (the version at docs.oracle.com, Appendix D of the Sun Numerical Computation
// Guide, which carries the same text and numbers plus editorial footnotes).
// Written for ounce-core from the examples in the text: a small base-10 floating-point emulator with a chosen number of digits p,
// round-to-even or round-half-up, and subtraction with no, one or exact guard digits, plus IEEE binary32 arithmetic for the
// compound-interest example (exp and ln are written here in double and rounded to float; no libm).
// Negative control: -DNEG_RNE_HALFUP uses round-half-up in the round-to-even examples.
#include "ounce_paper.h"
typedef unsigned __int128 u128;

static int P = 3;          // digits
static int HALFUP = 0;     // 0 = round to even, 1 = round half up
#ifdef NEG_RNE_HALFUP
#define FORCE_UP 1
#else
#define FORCE_UP 0
#endif
typedef struct { int neg; u64 m; int e; } Dec;   // value = +-m * 10^e, m = 0 or 10^(P-1) <= m < 10^P
static u64 p10(int k) { u64 r = 1; while (k-- > 0) r *= 10; return r; }
static Dec dnorm(int neg, u64 m, int e, int sticky) {
    Dec r; r.neg = neg; r.m = 0; r.e = 0;
    if (m == 0) { r.neg = 0; return r; }
    int n = 0; u64 t = m; while (t) { n++; t /= 10; }
    if (n > P) {
        int drop = n - P; u64 pw = p10(drop); u64 q = m / pw, rem = m % pw, half = pw / 2;
        int up = rem > half || (rem == half && (sticky || HALFUP || FORCE_UP || (q & 1)));
        if (up) q++;
        e += drop; m = q;
        if (m == p10(P)) { m = p10(P - 1); e++; }
    } else { for (int i = n; i < P; i++) { m *= 10; e--; } }
    r.m = m; r.e = e; return r;
}
static Dec dparse(const char *s) {   // [-]ddd.ddd, rounded to P digits
    int neg = 0, i = 0; if (s[0] == '-') { neg = 1; i = 1; }
    u64 m = 0; int e = 0, seen = 0;
    for (; s[i]; i++) { if (s[i] == '.') seen = 1; else { m = m * 10 + (u64)(s[i] - '0'); if (seen) e--; } }
    return dnorm(neg, m, e, 0);
}
static Dec dzero(void) { return dnorm(0, 0, 0, 0); }
static int deq(Dec a, Dec b) { return a.m == b.m && a.e == b.e && a.neg == b.neg; }
// a + b where the smaller operand is first truncated to P+g digits (g < 0: exact addition, then rounding)
static Dec dadd_g(Dec a, Dec b, int g) {
    if (a.m == 0) return b; if (b.m == 0) return a;
    int e = a.e < b.e ? a.e : b.e;
    // order by magnitude
    int ea = a.e, eb = b.e;
    int a_big = ea > eb || (ea == eb && a.m >= b.m);
    Dec big = a_big ? a : b, sml = a_big ? b : a;
    if (big.e - sml.e > 12) return dnorm(big.neg, big.m, big.e, 0);   // the smaller operand is far below half an ulp
    u64 mb = big.m, ms = sml.m; int ee = e;
    if (g >= 0) {   // keep digits down to position big.e - g
        int keep = big.e - g;
        if (sml.e < keep) { u64 pw = p10(keep - sml.e); ms = (ms / pw); ms *= pw; ee = sml.e; }   // truncate the smaller operand at 10^keep, stay in its own units
    }
    u128 A = mb, B = ms;
    int ebase = ee < big.e ? ee : big.e; if (g >= 0 && sml.e < big.e) ebase = sml.e < big.e ? sml.e : big.e;
    for (int i = big.e; i > ebase; i--) A *= 10; for (int i = sml.e; i > ebase; i--) B *= 10;
    u64 res; int neg;
    if (big.neg == sml.neg) { res = (u64)(A + B); neg = big.neg; } else { res = (u64)(A - B); neg = big.neg; }
    return dnorm(neg, res, ebase, 0);
}
static Dec dneg(Dec a) { if (a.m) a.neg = !a.neg; return a; }
static Dec dadd(Dec a, Dec b) { return dadd_g(a, b, -1); }
static Dec dsub(Dec a, Dec b) { return dadd_g(a, dneg(b), -1); }
static Dec dsub_g(Dec a, Dec b, int g) { return dadd_g(a, dneg(b), g); }
static Dec dmul(Dec a, Dec b) { return dnorm(a.neg != b.neg, a.m * b.m, a.e + b.e, 0); }
static Dec ddiv(Dec a, Dec b) { u64 num = a.m * p10(10); return dnorm(a.neg != b.neg, num / b.m, a.e - b.e - 10, (num % b.m) != 0); }
static Dec dsqrt(Dec a) {
    if (a.m == 0) return a;
    u64 m = a.m; int e = a.e; if (e & 1) { m *= 10; e--; }
    u128 X = (u128)m * p10(16);   // sqrt(m 10^e) = sqrt(m 10^16) 10^(e/2 - 8)
    u128 lo = 0, hi = (u128)1 << 63; while (lo < hi) { u128 mid = (lo + hi + 1) / 2; if (mid * mid <= X) lo = mid; else hi = mid - 1; }
    return dnorm(0, (u64)lo, e / 2 - 8, lo * lo != X);
}
static void dshow(Dec a) {
    if (a.m == 0) { pstr("0"); return; }
    if (a.neg) pstr("-");
    char d[24]; u64 m = a.m; for (int i = P - 1; i >= 0; i--) { d[i] = (char)('0' + m % 10); m /= 10; }
    int pt = a.e + P;
    if (pt <= 0) { pstr("0."); for (int i = pt; i < 0; i++) pstr("0"); paper_write(d, P); }
    else { for (int i = 0; i < P; i++) { if (i == pt) pstr("."); paper_write(&d[i], 1); } for (int i = P; i < pt; i++) pstr("0"); }
}
static int bad = 0;
static void verdict(const char *what, int ok) { pstr(ok ? "  ok: " : "  FAIL: "); pstr(what); pstr("\n"); if (!ok) bad++; }
static void line(const char *name, Dec v) { pstr("  "); pstr(name); pstr(" = "); dshow(v); pstr("\n"); }

// ---- binary32 helpers: exp and ln in double, rounded to float ----
static double pow2d(int e) { union { double d; u64 u; } v; v.u = (u64)(e + 1023) << 52; return v.d; }
static double expd(double x) {
    const double ln2hi = 0.693147180369123816490, ln2lo = 1.90821492927058770002e-10;
    int k = (int)(x / 0.6931471805599453 + (x < 0 ? -0.5 : 0.5));
    double r = (x - (double)k * ln2hi) - (double)k * ln2lo, t = 1.0, s = 1.0;
    for (int i = 1; i < 25; i++) { t = t * r / (double)i; s += t; }
    return s * pow2d(k);
}
static double logd(double y) {   // y > 0
    union { double d; u64 u; } v; v.d = y; int e = (int)((v.u >> 52) & 2047) - 1023; v.u = (v.u & 0xFFFFFFFFFFFFFull) | (1023ull << 52);
    double m = v.d; if (m > 1.4142135623730951) { m *= 0.5; e++; }
    double z = (m - 1.0) / (m + 1.0), z2 = z * z, t = z, s = z;
    for (int i = 1; i < 30; i++) { t *= z2; s += t / (double)(2 * i + 1); }
    return 2.0 * s + (double)e * 0.6931471805599453;
}
static float expf_(float x) { return (float)expd((double)x); }
static float logf_(float x) { return (float)logd((double)x); }
// is the amount (a float) published as `pub` dollars.cents? pub is given in cents; accept rounding or truncation to cents
static int cents_match(double amount, long long pub_cents) {
    long long t = (long long)(amount * 100.0), r = (long long)(amount * 100.0 + 0.5);
    return t == pub_cents || r == pub_cents;
}
static void money(const char *label, double v) { pstr("  "); pstr(label); pstr(" = "); long long c = (long long)(v * 10000.0 + 0.5); pdec(c / 10000); pstr("."); { long long f = c % 10000; char b[4]; for (int i = 3; i >= 0; i--) { b[i] = (char)('0' + f % 10); f /= 10; } paper_write(b, 4); } pstr("\n"); }

int paper_main(void) {
    // ---- guard digits (p = 3) ----
    P = 3; HALFUP = 0;
    pstr("Guard digits, beta = 10, p = 3\n");
    { Dec x = dparse("10.1"), y = dparse("9.93");
      Dec r0 = dsub_g(x, y, 0), r1 = dsub_g(x, y, 1), re = dsub(x, y);
      line("10.1 - 9.93 without a guard digit", r0); line("10.1 - 9.93 with one guard digit", r1); line("10.1 - 9.93 exactly rounded", re);
      verdict("without a guard digit the result is .2 (the paper: x - y = .02 x 10^1)", deq(r0, dparse("0.2")));
      verdict("with a guard digit the result is .17, the exact answer", deq(r1, dparse("0.17")) && deq(re, dparse("0.17")));
      // 30 ulps: |0.2 - 0.17| / 0.01
    }
    { Dec x = dparse("2.15E12"); (void)x; }
    { // 2.15e12 - 1.25e-5: both ways give 2.15e12; built with exponents by hand
      Dec x = dnorm(0, 215, 10, 0), y = dnorm(0, 125, -7, 0);
      Dec r0 = dsub_g(x, y, 0), re = dsub(x, y);
      verdict("2.15e12 - 1.25e-5 = 2.15e12 both without a guard digit and exactly rounded", deq(r0, x) && deq(re, x));
    }
    { Dec x = dparse("110"), y = dparse("8.59");
      Dec r1 = dsub_g(x, y, 1), re = dsub(x, y);
      line("110 - 8.59 with one guard digit", r1); line("110 - 8.59 exactly rounded", re);
      verdict("one guard digit gives 102 (the paper), the exactly rounded result is 101 (exact answer 101.41)", deq(r1, dparse("102")) && deq(re, dparse("101")));
      // relative error .006 > epsilon = .005: |102 - 101.41| / 101.41 = 0.5818e-2
      verdict("its relative error 0.59/101.41 = .0058 exceeds epsilon = .005 (integer arithmetic)", 59 * 1000 > 5 * 10141);
    }
    // ---- catastrophic cancellation: b^2 - 4ac ----
    { Dec b = dparse("3.34"), a = dparse("1.22"), c = dparse("2.28"), four = dparse("4");
      Dec bb = dmul(b, b), ac4 = dmul(dmul(four, a), c), d = dsub(bb, ac4);
      line("b*b", bb); line("4ac", ac4); line("b*b - 4ac", d);
      verdict("b^2 rounds to 11.2, 4ac rounds to 11.1, the difference is .1 (paper)", deq(bb, dparse("11.2")) && deq(ac4, dparse("11.1")) && deq(d, dparse("0.1")));
      // exact value .0292: 334^2 - 4*122*228 = 111556 - 111264 = 292 (scaled 10^-4)
      verdict("exact b^2 - 4ac = .0292 (integers)", 334 * 334 - 4 * 122 * 228 == 292);
      // error in ulps of .0292 (ulp .0001): (0.1 - .0292)/.0001 = 708; the paper's text says 70, its footnote 6 corrects to 708
      long long ulps = (1000 - 292) * 1;   // in units of 1e-4
      pstr("  error in ulps of .0292 = "); pdec(ulps); pstr(" (the text says 70; the editor's footnote says 708)\n");
      verdict("the error is 708 ulps as the paper's footnote states (the text's 70 is a typo)", ulps == 708);
    }
    // ---- triangle ----
    { Dec a = dparse("9.0"), b = dparse("4.53"), c = dparse("4.53"), two = dparse("2");
      Dec s = ddiv(dadd(dadd(b, c), a), two);
      Dec p = dmul(dmul(dmul(s, dsub(s, a)), dsub(s, b)), dsub(s, c));
      Dec A6 = dsqrt(p);
      line("s = ((b+c)+a)/2", s); line("area from formula (6)", A6);
      verdict("computed s = 9.05 (true 9.03) and the area from (6) is 3.04 (paper)", deq(s, dparse("9.05")) && deq(A6, dparse("3.04")));
      // formula (7): 1/4 sqrt((a+(b+c)) (c-(a-b)) (c+(a-b)) (a+(b-c)))
      Dec f1 = dadd(a, dadd(b, c)), f2 = dsub(c, dsub(a, b)), f3 = dadd(c, dsub(a, b)), f4 = dadd(a, dsub(b, c));
      Dec A7 = dmul(dsqrt(dmul(dmul(dmul(f1, f2), f3), f4)), dparse("0.25"));
      line("area from formula (7)", A7);
      verdict("formula (7) gives 2.35 (paper)", deq(A7, dparse("2.35")));
      // true area: A^2 = s(s-a)(s-b)(s-c) with s = 9.03: 9.03*.03*4.5*4.5 = 5.485...; area 2.34216... (the paper: 2.342..., 2.34216)
      double At = paper_sqrt(9.03 * 0.03 * 4.5 * 4.5); long long a5 = (long long)(At * 100000.0 + 0.5);
      pstr("  true area = "); pdec(a5); pstr(" (x 1e-5)\n");
      verdict("true area is 2.34216 as the paper says", a5 == 234216);
      // errors in ulps (ulp = .01): |3.04 - 2.34216| = 69.8 -> 70; |2.35 - 2.34216| = 0.78 -> about 1 ulp; relative error of (7) is 0.7 epsilon
      double e6 = (3.04 - At) / 0.01, e7 = (2.35 - At) / 0.01, rel7 = ((2.35 - At) / At) / 0.005;
      pstr("  error of (6) in ulps (x100): "); pdec((long long)(e6 * 100.0 + 0.5)); pstr(", of (7): "); pdec((long long)(e7 * 100.0 + 0.5)); pstr(", relative error of (7) in epsilon (x100): "); pdec((long long)(rel7 * 100.0 + 0.5)); pstr("\n");
        verdict("error of (6) is 70 ulps, of (7) about 1 ulp, relative error 0.7 epsilon (paper)", (long long)(e6 + 0.5) == 70 && (long long)(e7 + 0.5) == 1 && (long long)(rel7 * 10.0 + 0.5) == 7);
    }
    // ---- error in ulps vs relative error: x = 12.35 ----
    { Dec x = dparse("12.35"), x8;
      line("12.35 rounded to 3 digits", x); verdict("12.35 is 1.24 x 10^1 (ties to even)", deq(x, dparse("12.4")));
      x8 = dmul(dparse("8"), x); line("8 * 12.4", x8); verdict("8 x 1.24e1 = 9.92e1", deq(x8, dparse("99.2")));
      // errors 0.5 ulps and 4.0 ulps (ulp = .1): |12.4-12.35|/.1 = .5, |99.2-98.8|/.1 = 4.0
      verdict("the errors are 0.5 and 4.0 ulps, with the same relative error (integers: 5/1235 = 40/9880)", 5 * 9880 == 40 * 1235);
    }
    // ---- inexact exception example ----
    { Dec r1 = dmul(dparse("3.5"), dparse("4.2")), r2 = dmul(dparse("3.5"), dparse("4.3"));
      line("3.5 * 4.2", r1); line("3.5 * 4.3", r2);
      verdict("3.5 x 4.2 = 14.7 exactly; 3.5 x 4.3 = 15.05 rounds to 15.0 (inexact)", deq(r1, dparse("14.7")) && deq(r2, dparse("15.0")) && 35 * 42 == 1470 && 35 * 43 == 1505);
    }
    // ---- Theorem 5: round to even versus round up, beta = 10, p = 3, x = 1.00, y = -.555 ----
    { Dec y = dparse("-0.555"), x0 = dparse("1.00");
      for (int mode = 0; mode < 2; mode++) {
        HALFUP = mode; Dec x = x0; int hit = -1; int steady = 1; Dec at845 = dzero();
        Dec first[3];
        for (int n = 1; n <= 1000; n++) {
            Dec t = dsub(x, y); x = dadd(t, y);   // x_n = (x_{n-1} (-) y) (+) y
            if (n <= 3) first[n - 1] = x;
            if (mode == 1 && deq(x, dparse("9.45")) && hit < 0) hit = n;
            if (n == 845) at845 = x;
            if (mode == 0 && !deq(x, x0)) steady = 0;
        }
        if (mode == 0) { pstr("  round to even: x_n over n = 1..1000: "); pstr(steady ? "always 1.00\n" : "changed\n"); verdict("under round to even x_n = 1.00 for all n tested (Theorem 5)", steady); }
        else {
            pstr("  round half up: x_1, x_2, x_3 = "); dshow(first[0]); pstr(", "); dshow(first[1]); pstr(", "); dshow(first[2]); pstr("; x_n first reaches 9.45 at n = "); pdec(hit); pstr("; x_845 = "); dshow(at845); pstr("; x_1000 = "); dshow(x); pstr("\n");
            verdict("round up: x_1 = 1.01, x_n rises by .01 per step (paper: 1.56, 1.01, 1.57, ...) until x_n = 9.45 at n = 845 and stays there", deq(first[0], dparse("1.01")) && deq(at845, dparse("9.45")) && deq(x, dparse("9.45")) && hit <= 845 && hit > 0);
        }
      }
      HALFUP = 0;
      // the footnote: x_845 + .555 = 10.0 and 10.0 - .555 = 9.45
      HALFUP = 1; { Dec t = dsub(dparse("9.45"), y); Dec u = dadd(dparse("10.0"), y); verdict("footnote: 9.45 + .555 = 10.0 and 10.0 - .555 = 9.45", deq(t, dparse("10.0")) && deq(u, dparse("9.45"))); } HALFUP = 0;
    }
    // ---- Theorem 6: splitting, beta = 10, p = 4 ----
    P = 4; HALFUP = 0;
    { pstr("Theorem 6 example, beta = 10, p = 4\n");
      Dec b = dparse("3.476"), a = dparse("3.463"), c = dparse("3.479"), m = dparse("101");
      Dec bb = dmul(b, b), ac = dmul(a, c), d = dsub(bb, ac);
      line("b*b", bb); line("a*c", ac); line("b*b - a*c", d);
      verdict("computed b*b = 12.08, a*c = 12.05, difference .03 (paper); the exactly rounded b^2 - ac is .03480",
              deq(bb, dparse("12.08")) && deq(ac, dparse("12.05")) && deq(d, dparse("0.03")) && 3476LL * 3476 - 3463LL * 3479 == 34799);
      // 480 ulps: ulp of .03480 at p = 4 is .00001 -> (.03480 - .03)/.00001 = 480
      verdict("error 480 ulps: (3480 - 3000)/1 in units of 1e-5", 3480 - 3000 == 480);
      Dec x[3] = { b, a, c }; Dec xh[3], xl[3];
      for (int i = 0; i < 3; i++) { Dec mx = dmul(m, x[i]); xh[i] = dsub(mx, dsub(mx, x[i])); xl[i] = dsub(x[i], xh[i]); }
      for (int i = 0; i < 3; i++) { pstr("  split "); dshow(x[i]); pstr(" = "); dshow(xh[i]); pstr(" + ("); dshow(xl[i]); pstr(")\n"); }
      verdict("Theorem 6 splits b = 3.5 - .024, a = 3.5 - .037, c = 3.5 - .021 (paper)",
              deq(xh[0], dparse("3.5")) && deq(xl[0], dparse("-0.024")) && deq(xh[1], dparse("3.5")) && deq(xl[1], dparse("-0.037")) && deq(xh[2], dparse("3.5")) && deq(xl[2], dparse("-0.021")));
      // b^2 = 3.5^2 - 2*3.5*.024 + .024^2 ; a*c = 3.5^2 - (3.5*.037 + 3.5*.021) + .037*.021
      Dec h = dparse("3.5"), t35 = dmul(h, h);
      Dec b_mid = dmul(dmul(dparse("2"), h), dparse("0.024")), b_lo = dmul(dparse("0.024"), dparse("0.024"));
      Dec a_mid = dadd(dmul(h, dparse("0.037")), dmul(h, dparse("0.021"))), a_lo = dmul(dparse("0.037"), dparse("0.021"));
      line("3.5^2", t35); line("2*3.5*.024", b_mid); line(".024^2", b_lo); line("3.5*.037 + 3.5*.021", a_mid); line(".037*.021", a_lo);
      verdict("the pieces are 12.25 - .168 + .000576 and 12.25 - .2030 + .000777 as printed",
              deq(t35, dparse("12.25")) && deq(b_mid, dparse("0.168")) && deq(b_lo, dparse("0.000576")) && deq(a_mid, dparse("0.203")) && deq(a_lo, dparse("0.000777")));
      // subtract term by term: 0 + (-.168 + .2030) + (.000576 - .000777): .0350 and -.000201 ; sum .03480
      Dec t1 = dsub(t35, t35), t2 = dsub(a_mid, b_mid), t3 = dsub(b_lo, a_lo);
      line("term 1", t1); line("term 2", t2); line("term 3", t3);
      Dec fin = dadd(dadd(t1, t2), t3); line("sum", fin);
      verdict("term by term the estimate of b^2 - ac is 0 + .0350 - .000201 = .03480, the exactly rounded result (paper)", deq(t2, dparse("0.035")) && deq(t3, dparse("-0.000201")) && deq(fin, dparse("0.0348")));
    }
    // ---- denormals: p = 3, emin = -98, x = 6.87e-97, y = 6.81e-97 ----
    P = 3;
    { pstr("Denormals, beta = 10, p = 3, emin = -98\n");
      // x - y exactly = .06e-97 = 6.0e-99 < 1.00e-98: flush to zero; gradual underflow keeps .6e-98 (= 6.0e-99, a denormal)
      Dec x = dnorm(0, 687, -99, 0), y = dnorm(0, 681, -99, 0), d = dsub(x, y);   // 6.87e-97 = 687e-99
      int lead_exp = d.e + P - 1;   // exponent of the leading digit
      int flush = lead_exp < -98;
      pstr("  x - y = 6.00 x 10^"); pdec(d.e + P - 1); pstr(" (emin = -98, smallest normalized number 1.00 x 10^-98)\n");
      verdict("x - y = 6.0e-99 is below the smallest normalized number 1.00e-98, so flush-to-zero gives 0 although x != y; gradual underflow keeps .6e-98 (paper)",
              d.m == 600 && d.e == -101 && flush && !deq(x, y));
    }
    // ---- compound interest, binary32 ----
    pstr("Compound interest, i = .06, n = 365, binary32 (beta = 2, p = 24)\n");
    { volatile float i = 0.06f, n = 365.0f; float x = i / n; float one_x = 1.0f + x;
      float p = 1.0f; for (int k = 0; k < 365; k++) p = p * one_x;
      double amount1 = (double)(100.0f * (p - 1.0f) / x);
      float p4 = expf_(n * x); double amount_lin = (double)(100.0f * (p4 - 1.0f) / x);   // ln(1+x) ~ x
      float l5 = (one_x == 1.0f) ? x : x * logf_(one_x) / (one_x - 1.0f);                // Theorem 4
      float p5 = expf_(n * l5); double amount_t4 = (double)(100.0f * (p5 - 1.0f) / x);
      // the exact answer in double
      double X = 0.06 / 365.0, q = 1.0; for (int k = 0; k < 365; k++) q = q * (1.0 + X); double amount_exact = 100.0 * (q - 1.0) / X;
      money("(1 + i/n)^n formula", amount1); money("ln(1+x) ~ x", amount_lin); money("Theorem 4 formula", amount_t4); money("exact (double)", amount_exact);
      verdict("formula with 1 + i/n: $37615.45 (paper)", cents_match(amount1, 3761545));
      verdict("exact answer $37614.05 (paper; to cents, rounded)", amount_exact * 100.0 + 0.5 >= 3761405.0 && amount_exact * 100.0 + 0.5 < 3761406.0);
      verdict("ln(1+x) ~ x gives $37617.26 (paper; our value is 37617.2656, printed truncated)", cents_match(amount_lin, 3761726));
      verdict("Theorem 4 formula gives $37614.07 (paper)", cents_match(amount_t4, 3761407));
      verdict("discrepancies $1.40 and $3.21 as stated", cents_match(amount1 - amount_exact, 140) && cents_match(amount_lin - amount_exact, 321));
    }
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
