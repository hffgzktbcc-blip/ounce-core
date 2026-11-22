// Reproduction of numbers in W. Kahan, "How Futile are Mindless Assessments of Roundoff in Floating-Point Computation?"
// (work-in-progress note, version of 11 January 2006), section 8 "An Old Hand Accuses Division", page 29-30:
//   f(x) = atan(log x)/acosh(x)^2 for x > 1 (and -atan(log x)/acos(x)^2 for x < 1), f(1) = 1/2;
//  (1) the nine terms of its Taylor series at x = 1 "computed by MAPLE": 1/2 - z/6 - z^2/20 + 124 z^3/945 - 8221 z^4/113400 - ...;
//  (2) the correct values acos(1 - eps/2) = 1.49011611938476564e-8 and acosh(1 + eps) = 2.10734242554470155e-8, eps = 2^-52.
// Written for ounce-core from the definition of f in the paper: the series are derived here from first principles with exact rational
// arithmetic (log(1+z) and atan as power series, composed; acosh(1+z)^2 from the differential equation (z^2+2z)y'' + (1+z)y' = 2),
// then divided; the two correct values come from the series acos(1-d) = sqrt(2d)(1 + d/12 + ...) and acosh(1+z) = sqrt(2z)(1 - z/12 + ...),
// evaluated with integers and a big-number type written here. No third-party code.
// Negative control: -DNEG_ODE uses the recurrence a(m+1) = -m^2 a(m)/((m+1)(2m+3)) (a wrong constant) for acosh(1+z)^2.
#include "ounce_paper.h"
typedef unsigned __int128 u128;
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

// ---- exact rationals: sign, numerator, denominator (u128) ----
typedef struct { int neg; u128 n, d; } Fr;
static u128 gcd128(u128 a, u128 b) { while (b) { u128 t = a % b; a = b; b = t; } return a; }
static Fr fr_make(int neg, u128 n, u128 d) { Fr r; if (n == 0) { r.neg = 0; r.n = 0; r.d = 1; return r; } u128 g = gcd128(n, d); r.neg = neg; r.n = n / g; r.d = d / g; return r; }
static Fr fr_int(long long v) { return fr_make(v < 0, (u128)(v < 0 ? -v : v), 1); }
static Fr fr_add(Fr a, Fr b) {
    u128 x = a.n * b.d, y = b.n * a.d, d = a.d * b.d;
    if (a.neg == b.neg) return fr_make(a.neg, x + y, d);
    if (x >= y) return fr_make(a.neg, x - y, d);
    return fr_make(b.neg, y - x, d);
}
static Fr fr_neg(Fr a) { if (a.n) a.neg = !a.neg; return a; }
static Fr fr_mul(Fr a, Fr b) { return fr_make(a.neg != b.neg, a.n * b.n, a.d * b.d); }
static Fr fr_div(Fr a, Fr b) { return fr_make(a.neg != b.neg, a.n * b.d, a.d * b.n); }
static int fr_eq(Fr a, int neg, u128 n, u128 d) { Fr b = fr_make(neg, n, d); return a.neg == b.neg && a.n == b.n && a.d == b.d; }
static void pu128(u128 v) { char b[44]; int n = 0; if (!v) { pstr("0"); return; } while (v) { b[n++] = (char)('0' + (int)(v % 10)); v /= 10; } while (n) paper_write(&b[--n], 1); }
static void pfr(Fr a) { if (a.neg) pstr("-"); pu128(a.n); if (a.d != 1) { pstr("/"); pu128(a.d); } }

#define K 10
static void smul(const Fr *a, const Fr *b, Fr *r) { for (int i = 0; i <= K; i++) r[i] = fr_int(0); for (int i = 0; i <= K; i++) for (int j = 0; i + j <= K; j++) r[i + j] = fr_add(r[i + j], fr_mul(a[i], b[j])); }

// ---- big numbers for the decimal expansions ----
#define L 12
typedef struct { u32 w[L]; } Big;
static void b_set(Big *a, u64 v) { for (int i = 0; i < L; i++) a->w[i] = 0; a->w[0] = (u32)v; a->w[1] = (u32)(v >> 32); }
static int b_zero(const Big *a) { for (int i = 0; i < L; i++) if (a->w[i]) return 0; return 1; }
static void b_mul(Big *a, u32 m) { u64 c = 0; for (int i = 0; i < L; i++) { u64 t = (u64)a->w[i] * m + c; a->w[i] = (u32)t; c = t >> 32; } }
static void b_shl(Big *a, int s) { while (s > 0) { int t = s > 31 ? 31 : s; u32 c = 0; for (int i = 0; i < L; i++) { u32 n = (a->w[i] << t) | c; c = a->w[i] >> (32 - t); a->w[i] = n; } s -= t; } }
static int b_cmp(const Big *a, const Big *b) { for (int i = L - 1; i >= 0; i--) if (a->w[i] != b->w[i]) return a->w[i] < b->w[i] ? -1 : 1; return 0; }
static void b_sub(Big *a, const Big *b) { i64 br = 0; for (int i = 0; i < L; i++) { i64 t = (i64)a->w[i] - b->w[i] - br; br = t < 0; a->w[i] = (u32)(t + (br ? (1LL << 32) : 0)); } }
// digits of num/den (num, den > 0): value = d[0].d[1]... * 10^e10; *ru = remainder after nd digits >= half
static void ratio(Big num, Big den, int nd, char *d, int *e10, int *ru) {
    int e = 0;
    while (b_cmp(&num, &den) < 0) { b_mul(&num, 10); e--; }
    for (;;) { Big t = den; b_mul(&t, 10); if (b_cmp(&num, &t) < 0) break; den = t; e++; }
    for (int i = 0; i < nd; i++) { int q = 0; while (b_cmp(&num, &den) >= 0) { b_sub(&num, &den); q++; } d[i] = (char)q; b_mul(&num, 10); }
    Big five = den; b_mul(&five, 5); *ru = b_cmp(&num, &five) >= 0; *e10 = e;
}
// d0.d1d2... (nd digits) times 10^e10 as text d0.d1d2..e-8 style: "1.49011611938476564" with exponent separate
static int digits_match(const char *pub, const char *d, int nd, int ru) {   // pub like "1.49011611938476564": digits only compared; truncated or rounded
    char p[40]; int np = 0; for (int i = 0; pub[i]; i++) if (pub[i] != '.') p[np++] = (char)(pub[i] - '0');
    if (np > nd) return 0;
    int trunc_ok = 1; for (int i = 0; i < np; i++) if (p[i] != d[i]) trunc_ok = 0;
    if (trunc_ok) return 1;
    char r[40]; for (int i = 0; i < np; i++) r[i] = d[i];
    if (!(d[np] >= 5)) return 0;
    int i = np - 1; while (i >= 0) { if (r[i] == 9) { r[i] = 0; i--; } else { r[i]++; break; } }
    if (i < 0) return 0;
    for (int j = 0; j < np; j++) if (p[j] != r[j]) return 0;
    return 1; (void)ru;
}
static int bad = 0;
static void verdict(const char *what, int ok) { pstr(ok ? "  ok: " : "  FAIL: "); pstr(what); pstr("\n"); if (!ok) bad++; }

int paper_main(void) {
    // ---- (1) Taylor series of f(1+z) ----
    pstr("(1) Taylor series of f(1+z) = atan(log(1+z))/acosh(1+z)^2 derived with exact rationals\n");
    Fr lg[K + 1]; lg[0] = fr_int(0); for (int k = 1; k <= K; k++) lg[k] = fr_make((k % 2) == 0, 1, (u128)k);
    Fr w2[K + 1], pw[K + 1], tmp[K + 1], N[K + 1];
    smul(lg, lg, w2); for (int i = 0; i <= K; i++) { pw[i] = lg[i]; N[i] = fr_int(0); }
    for (int j = 0; j < 6; j++) {   // atan(w) = sum (-1)^j w^(2j+1)/(2j+1)
        Fr c = fr_make(j % 2, 1, (u128)(2 * j + 1));
        for (int i = 0; i <= K; i++) N[i] = fr_add(N[i], fr_mul(c, pw[i]));
        smul(pw, w2, tmp); for (int i = 0; i <= K; i++) pw[i] = tmp[i];
    }
    Fr a[K + 2]; for (int i = 0; i < K + 2; i++) a[i] = fr_int(0); a[1] = fr_int(2);
    for (int m = 1; m < K; m++) {
#ifdef NEG_ODE
        a[m + 1] = fr_neg(fr_div(fr_mul(fr_int((long long)m * m), a[m]), fr_int((long long)(m + 1) * (2 * m + 3))));
#else
        a[m + 1] = fr_neg(fr_div(fr_mul(fr_int((long long)m * m), a[m]), fr_int((long long)(m + 1) * (2 * m + 1))));   // from (z^2+2z)y'' + (1+z)y' = 2
#endif
    }
    Fr f[9];
    for (int k = 0; k < 9; k++) { Fr s = N[k + 1]; for (int i = 0; i < k; i++) s = fr_add(s, fr_neg(fr_mul(f[i], a[k - i + 1]))); f[k] = fr_div(s, a[1]); }
    // published coefficients (sign, numerator, denominator) read from the paper's page 29
    static const struct { int neg; unsigned long long n, d; } pub[9] = {
        {0, 1, 2}, {1, 1, 6}, {1, 1, 20}, {0, 124, 945}, {1, 8221, 113400}, {1, 46969, 1247400},
        {0, 948249251ull, 10216206000ull}, {1, 208838923ull, 3831077250ull}, {1, 14025530287ull, 521026506000ull} };
    int okc = 0;
    for (int k = 0; k < 9; k++) {
        int ok = fr_eq(f[k], pub[k].neg, pub[k].n, pub[k].d);
        pstr("  z^"); pdec(k); pstr(": "); pfr(f[k]); pstr(ok ? "  matches\n" : "  DIFFERS from the paper\n"); okc += ok;
    }
    verdict("all nine Taylor coefficients equal the paper's", okc == 9);
    verdict("f(1) = 1/2 (paper)", fr_eq(f[0], 0, 1, 2));

    // ---- (2) correct values of acos(1 - eps/2) and acosh(1 + eps) ----
    pstr("(2) correct values (eps = 2^-52)\n");
    {   // acos(1-d), d = 2^-53: 2^-26 (1 + d/12 + 3 d^2/160 ...) ; the d^2 term is below 1e-33 relative and is dropped
        Big num, den; b_set(&num, 12); b_shl(&num, 53); { Big one; b_set(&one, 1); /* num = 12*2^53 + 1 */ u32 c = 1; for (int i = 0; i < L && c; i++) { u64 t = (u64)num.w[i] + c; num.w[i] = (u32)t; c = (u32)(t >> 32); } (void)one; }
        b_set(&den, 12); b_shl(&den, 53 + 26);
        char d[40]; int e10, ru; ratio(num, den, 20, d, &e10, &ru);
        pstr("  acos(1-eps/2) = "); for (int i = 0; i < 20; i++) { if (i == 1) pstr("."); char c = (char)('0' + d[i]); paper_write(&c, 1); } pstr("e"); pdec(e10); pstr("\n");
        verdict("acos(1 - eps/2) = 1.49011611938476564e-8 (paper, correct value)", e10 == -8 && digits_match("1.49011611938476564", d, 20, ru));
    }
    {   // acosh(1+z), z = 2^-52: sqrt(2z) (1 - z/12 + ...) = 2^-26 sqrt(2) (1 - z/12); sqrt(2) = isqrt(2e38)/1e19 to 20 digits
        u128 lo = 0, hi = 15000000000000000000ull; u128 X = (u128)2 * 10000000000000000000ull * 10000000000000000000ull; // 2e38
        while (lo < hi) { u128 mid = (lo + hi + 1) / 2; if (mid * mid <= X) lo = mid; else hi = mid - 1; }
        Big num, den, t; b_set(&num, 0); { u64 s = (u64)lo; b_set(&num, s); }
        // num = S * (12*2^52 - 1)
        b_set(&t, 12); b_shl(&t, 52); { i64 br = 1; for (int i = 0; i < L && br; i++) { i64 v = (i64)t.w[i] - br; br = v < 0; t.w[i] = (u32)(v + (br ? (1LL << 32) : 0)); } }
        { Big acc; b_set(&acc, 0); Big cur = num; for (int i = 0; i < L; i++) for (int bit = 0; bit < 32; bit++) { if ((t.w[i] >> bit) & 1) { u64 c = 0; for (int j = 0; j < L; j++) { u64 s2 = (u64)acc.w[j] + cur.w[j] + c; acc.w[j] = (u32)s2; c = s2 >> 32; } } b_shl(&cur, 1); } num = acc; }
        // den = 10^19 * 12 * 2^52 * 2^26
        b_set(&den, 12); for (int i = 0; i < 19; i++) b_mul(&den, 10); b_shl(&den, 52 + 26);
        char d[40]; int e10, ru; ratio(num, den, 20, d, &e10, &ru);
        pstr("  acosh(1+eps) = "); for (int i = 0; i < 20; i++) { if (i == 1) pstr("."); char c = (char)('0' + d[i]); paper_write(&c, 1); } pstr("e"); pdec(e10); pstr("\n");
        verdict("acosh(1 + eps) = 2.10734242554470155e-8 (paper, correct value)", e10 == -8 && digits_match("2.10734242554470155", d, 20, ru));
    }
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
