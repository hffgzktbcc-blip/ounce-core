// Reproduction of T. Ogita, S. M. Rump, S. Oishi, "Accurate Sum and Dot Product", SIAM J. Sci. Comput. 26(6), 2005.
// Written for ounce-core from the algorithms in the paper (TwoSum 3.1, Split 3.2, TwoProduct 3.3, TwoProductFMA 3.5,
// VecSum 4.3, Sum2 4.4, SumK 4.8, Dot2 5.3, GenDot 6.1). Exact reference values come from a long-integer accumulator written
// here (no floating point in the reference). Random numbers come from Philox4x32-10 (see ../philox), so every run is the same.
// Build (aarch64, freestanding): clang --target=aarch64-linux-gnu -O1 -ffreestanding -fno-builtin -ffp-contract=off ...
// Build (host, for the bit-for-bit comparison):  clang -O1 -ffp-contract=off -DHOST accurate_sum.c -lm
// Prints what it checked and exits 0 only if every check passed.
typedef unsigned int u32;
typedef unsigned long long u64;
typedef long long i64;
typedef unsigned __int128 u128;

#ifdef HOST
#include <stdio.h>
#include <stdlib.h>
static void put(const char *s, long n) { fwrite(s, 1, (size_t)n, stdout); }
static void quit(int code) { fflush(stdout); exit(code); }
#else
static void put(const char *s, long n) {
    register long x0 __asm__("x0") = 1;
    register const char *x1 __asm__("x1") = s;
    register long x2 __asm__("x2") = n;
    register long x8 __asm__("x8") = 64;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x8) : "memory");
}
void *memcpy(void *d, const void *s, unsigned long n) {   // the compiler emits calls to this for struct copies
    unsigned char *dd = d; const unsigned char *ss = s;
    while (n--) *dd++ = *ss++;
    return d;
}
static void quit(int code) {
    register long x0 __asm__("x0") = code;
    register long x8 __asm__("x8") = 93;
    __asm__ volatile("svc #0" : : "r"(x0), "r"(x8));
    for (;;) {}
}
#endif

static void str(const char *s) { long n = 0; while (s[n]) n++; put(s, n); }
static void num(i64 v) {
    char b[24]; int n = 0; u64 u = v < 0 ? (u64)(-v) : (u64)v;
    if (v < 0) put("-", 1);
    do { b[n++] = (char)('0' + u % 10); u /= 10; } while (u);
    while (n) put(&b[--n], 1);
}
static void fixed1(double d) { i64 t = (i64)(d * 10.0 + 0.5); num(t / 10); put(".", 1); num(t % 10); }

// ---- Philox4x32-10 (validated against the published vectors by ../philox) ----
static u32 rk0, rk1, rc0, rc1, rbuf[4]; static int rleft;
static void philox(u32 c0, u32 c1, u32 c2, u32 c3, u32 k0, u32 k1, u32 out[4]) {
    for (int r = 0; r < 10; r++) {
        u64 p0 = (u64)0xD2511F53u * c0, p1 = (u64)0xCD9E8D57u * c2;
        u32 n0 = (u32)(p1 >> 32) ^ c1 ^ k0, n1 = (u32)p1, n2 = (u32)(p0 >> 32) ^ c3 ^ k1, n3 = (u32)p0;
        c0 = n0; c1 = n1; c2 = n2; c3 = n3; k0 += 0x9E3779B9u; k1 += 0xBB67AE85u;
    }
    out[0] = c0; out[1] = c1; out[2] = c2; out[3] = c3;
}
static u32 rnd32(void) {
    if (!rleft) { philox(rc0, rc1, 0, 0, rk0, rk1, rbuf); if (!++rc0) rc1++; rleft = 4; }
    return rbuf[--rleft];
}
static double rnd01(void) { u32 a = rnd32() >> 5, b = rnd32() >> 6; return ((double)a * 67108864.0 + (double)b) / 9007199254740992.0; }
static u32 rndbelow(u32 n) { return (u32)(((u64)rnd32() * n) >> 32); }

// ---- exact accumulator: a signed integer of 140 limbs, in units of 2^-2148 ----
#define LIMBS 140
#define OFFSET 2148
typedef struct { u32 w[LIMBS]; } Acc;
static void acc_zero(Acc *a) { for (int i = 0; i < LIMBS; i++) a->w[i] = 0; }
static int acc_is_zero(const Acc *a) { for (int i = 0; i < LIMBS; i++) if (a->w[i]) return 0; return 1; }
static void acc_add_m(Acc *a, int neg, u128 m, int pos) {
    if (!m) return;
    int limb = pos >> 5, sh = pos & 31;
    u128 lo = m << sh;
    u32 t[5] = {(u32)lo, (u32)(lo >> 32), (u32)(lo >> 64), (u32)(lo >> 96), sh ? (u32)(m >> (128 - sh)) : 0};
    if (!neg) {
        u64 carry = 0;
        for (int k = 0; limb + k < LIMBS; k++) {
            u64 s = (u64)a->w[limb + k] + (k < 5 ? t[k] : 0) + carry;
            a->w[limb + k] = (u32)s; carry = s >> 32;
            if (k >= 4 && !carry) break;
        }
    } else {
        u64 borrow = 0;
        for (int k = 0; limb + k < LIMBS; k++) {
            u64 sub = (u64)(k < 5 ? t[k] : 0) + borrow, w = a->w[limb + k];
            a->w[limb + k] = (u32)(w - sub); borrow = w < sub;
            if (k >= 4 && !borrow) break;
        }
    }
}
static void decomp(double x, int *sign, u64 *mant, int *e) {
    union { double d; u64 u; } v; v.d = x;
    *sign = (int)(v.u >> 63); int ef = (int)((v.u >> 52) & 2047); u64 fr = v.u & 0xFFFFFFFFFFFFFull;
    if (ef == 0) { *mant = fr; *e = -1074; } else { *mant = fr | (1ull << 52); *e = ef - 1075; }
}
static void acc_add(Acc *a, double x, int negate) {
    int s, e; u64 m; decomp(x, &s, &m, &e); acc_add_m(a, s ^ negate, m, e + OFFSET);
}
static void acc_add_prod(Acc *a, double x, double y, int negate) {
    int sx, sy, ex, ey; u64 mx, my; decomp(x, &sx, &mx, &ex); decomp(y, &sy, &my, &ey);
    acc_add_m(a, sx ^ sy ^ negate, (u128)mx * my, ex + ey + OFFSET);
}
static void acc_add_acc(Acc *a, const Acc *b, int negate) {
    u64 c = negate ? 1 : 0;
    for (int i = 0; i < LIMBS; i++) { u64 s = (u64)a->w[i] + (negate ? ~b->w[i] : b->w[i]) + c; a->w[i] = (u32)s; c = s >> 32; }
}
static int bit_at(const Acc *a, int i) { return (int)((a->w[i >> 5] >> (i & 31)) & 1); }
static double pow2(int e) { union { double d; u64 u; } v; v.u = (u64)(e + 1023) << 52; return v.d; }
static double acc_to_double(const Acc *a) {   // nearest double, ties to even
    Acc t = *a; int neg = (int)(t.w[LIMBS - 1] >> 31);
    if (neg) { u64 c = 1; for (int i = 0; i < LIMBS; i++) { u64 s = (u64)(~t.w[i]) + c; t.w[i] = (u32)s; c = s >> 32; } }
    int top = -1;
    for (int i = LIMBS - 1; i >= 0; i--) if (t.w[i]) { top = i * 32 + 31 - __builtin_clz(t.w[i]); break; }
    if (top < 0) return 0.0;
    int lsb = top > 52 ? top - 52 : 0; u64 mant = 0;
    for (int i = top; i >= lsb; i--) mant = (mant << 1) | (u64)bit_at(&t, i);
    if (lsb > 0) {
        int round = bit_at(&t, lsb - 1), sticky = 0;
        for (int i = 0; i < lsb - 1 && !sticky; i++) sticky = bit_at(&t, i);
        if (round && (sticky || (mant & 1))) mant++;
    }
    int e2 = lsb - OFFSET, h = e2 / 2; double v = (double)mant * pow2(h) * pow2(e2 - h);
    return neg ? -v : v;
}
static double dabs(double x) { return x < 0 ? -x : x; }
static double log2d(double v) {   // v > 0, normal; series, accurate to ~1e-7: only used to print digits and pick exponents
    union { double d; u64 u; } x; x.d = v; int e = (int)((x.u >> 52) & 2047) - 1023; x.u = (x.u & 0xFFFFFFFFFFFFFull) | (1023ull << 52);
    double m = x.d, z = (m - 1.0) / (m + 1.0), z2 = z * z;
    double ln = 2.0 * z * (1.0 + z2 / 3.0 + z2 * z2 / 5.0 + z2 * z2 * z2 / 7.0 + z2 * z2 * z2 * z2 / 9.0 + z2 * z2 * z2 * z2 * z2 / 11.0);
    return (double)e + ln * 1.4426950408889634;
}
static void share(double v) {   // print "none" or "about 2^k" for a non-negative ratio
    if (v == 0.0) { str("none (the error never exceeded eps|s|)"); return; }
    union { double d; u64 u; } x; x.d = v; str("about 2^"); num((int)((x.u >> 52) & 2047) - 1023);
}

// ---- the paper's algorithms (plain double arithmetic; built with -ffp-contract=off so nothing is fused) ----
static void two_sum(double a, double b, double *x, double *y) { double s = a + b, z = s - a; *x = s; *y = (a - (s - z)) + (b - z); }
static void split(double a, double *x, double *y) { double c = 134217729.0 * a; *x = c - (c - a); *y = a - *x; }
static void two_product(double a, double b, double *x, double *y) {
    double p = a * b, a1, a2, b1, b2; split(a, &a1, &a2); split(b, &b1, &b2);
    *x = p; *y = a2 * b2 - (((p - a1 * b1) - a2 * b1) - a1 * b2);
}
static void two_product_fma(double a, double b, double *x, double *y) { double p = a * b; *x = p; *y = __builtin_fma(a, b, -p); }
static void vec_sum(double *p, int n) { for (int i = 1; i < n; i++) two_sum(p[i], p[i - 1], &p[i], &p[i - 1]); }
static double finish_sum(const double *p, int n) { double c = p[0]; for (int i = 1; i < n - 1; i++) c += p[i]; return c + p[n - 1]; }
static double sum_k(double *work, int n, int k) { for (int j = 1; j < k; j++) vec_sum(work, n); return finish_sum(work, n); }
static double dsum(const double *p, int n) { double s = p[0]; for (int i = 1; i < n; i++) s += p[i]; return s; }
static double dot2(const double *x, const double *y, int n) {
    double p, s, h, r, q; two_product(x[0], y[0], &p, &s);
    for (int i = 1; i < n; i++) { two_product(x[i], y[i], &h, &r); two_sum(p, h, &p, &q); s = s + (q + r); }
    return p + s;
}

#define EPS 1.1102230246251565e-16   /* 2^-53 */
static double gamma_k(double k) { return k * EPS / (1.0 - k * EPS); }
static int failures;
static void verdict(const char *what, int ok) { str(what); str(ok ? ": ok\n" : ": FAIL\n"); if (!ok) failures++; }

// ---- Theorem 3.4: the error-free transformations are exact ----
static void check_transformations(void) {
    int ok_sum = 1, ok_prod = 1, ok_fma = 1, ok_same = 1, ok_bound = 1; const int N = 3000;
    for (int i = 0; i < N; i++) {
        double a = (2.0 * rnd01() - 1.0) * pow2((int)rndbelow(121) - 60), b = (2.0 * rnd01() - 1.0) * pow2((int)rndbelow(121) - 60);
        double x, y; Acc t;
        two_sum(a, b, &x, &y); acc_zero(&t); acc_add(&t, a, 0); acc_add(&t, b, 0); acc_add(&t, x, 1); acc_add(&t, y, 1);
        ok_sum &= acc_is_zero(&t) && x == a + b && dabs(y) <= EPS * dabs(x);
        double xs, ys; two_product(a, b, &xs, &ys); acc_zero(&t); acc_add_prod(&t, a, b, 0); acc_add(&t, xs, 1); acc_add(&t, ys, 1);
        ok_prod &= acc_is_zero(&t) && xs == a * b && dabs(ys) <= EPS * dabs(xs);
        double xf, yf; two_product_fma(a, b, &xf, &yf); acc_zero(&t); acc_add_prod(&t, a, b, 0); acc_add(&t, xf, 1); acc_add(&t, yf, 1);
        ok_fma &= acc_is_zero(&t) && xf == a * b && dabs(yf) <= EPS * dabs(xf);
        ok_same &= xs == xf && ys == yf;
        ok_bound &= 1;
    }
    str("Theorem 3.4, "); num(N); str(" random pairs\n");
    str("  "); verdict("TwoSum: a+b = x+y exactly, x = fl(a+b), |y| <= eps|x|", ok_sum);
    str("  "); verdict("TwoProduct (Split): a*b = x+y exactly, x = fl(a*b), |y| <= eps|x|", ok_prod);
    str("  "); verdict("TwoProductFMA: a*b = x+y exactly, x = fl(a*b), |y| <= eps|x|", ok_fma);
    str("  "); verdict("TwoProduct (Split) and TwoProductFMA give the same x and y", ok_same);
    (void)ok_bound;
}

// ---- Algorithm 6.1 (GenDot) and the sums built from it ----
#define MAXN 500
static double gx[MAXN], gy[MAXN], gp[2 * MAXN], gw[2 * MAXN];
static int roundi(double v) { return (int)(v + 0.5); }
static void gendot(int n, double c, Acc *dot) {
    int n2 = roundi(n / 2.0); double b = log2d(c); int e[MAXN];
    acc_zero(dot);
    for (int i = 0; i < n2; i++) e[i] = roundi(rnd01() * b / 2.0);
    e[0] = roundi(b / 2.0) + 1; e[n2 - 1] = 0;
    for (int i = 0; i < n2; i++) { gx[i] = (2.0 * rnd01() - 1.0) * pow2(e[i]); gy[i] = (2.0 * rnd01() - 1.0) * pow2(e[i]); acc_add_prod(dot, gx[i], gy[i], 0); }
    int cnt = n - n2;
    for (int i = n2; i < n; i++) {
        int k = i - n2, ee = roundi(b / 2.0 - (double)k * (b / 2.0) / (double)(cnt - 1));
        gx[i] = (2.0 * rnd01() - 1.0) * pow2(ee);
        gy[i] = ((2.0 * rnd01() - 1.0) * pow2(ee) - acc_to_double(dot)) / gx[i];
        acc_add_prod(dot, gx[i], gy[i], 0);
    }
    for (int i = n - 1; i > 0; i--) { int j = (int)rndbelow((u32)i + 1); double t = gx[i]; gx[i] = gx[j]; gx[j] = t; t = gy[i]; gy[i] = gy[j]; gy[j] = t; }
}
static double rel_digits(const Acc *s, double res) {   // correct decimal digits, 0..16, against the exact sum
    Acc d = *s; acc_add(&d, res, 1); double err = dabs(acc_to_double(&d)), sv = dabs(acc_to_double(s));
    if (err == 0.0) return 16.0;
    double rel = err / sv; if (rel >= 1.0) return 0.0;
    double dg = -log2d(rel) * 0.30102999566398120; return dg > 16.0 ? 16.0 : dg;
}
static void sort(double *v, int n) { for (int i = 1; i < n; i++) { double t = v[i]; int j = i - 1; while (j >= 0 && v[j] > t) { v[j + 1] = v[j]; j--; } v[j + 1] = t; } }
static double median(double *v, int n) { sort(v, n); return n & 1 ? v[n / 2] : 0.5 * (v[n / 2 - 1] + v[n / 2]); }
static double mad(double *v, int n, double med) { double d[256]; for (int i = 0; i < n; i++) d[i] = dabs(v[i] - med); return median(d, n); }

#define CASES 60
static const struct { double cond; const char *label; double pub_dsum, pub_sum2, pub_sum3; } rows[] = {
    {1e7, "1e7 ", 8.9, 16.0, 16.0}, {1e14, "1e14", 2.0, 16.0, 16.0}, {1e21, "1e21", 0.0, 10.3, 16.0}, {1e28, "1e28", 0.0, 3.8, 16.0}};

static void check_bounds_and_table(void) {
    int ok_s2 = 1, ok_s3 = 1, ok_d2 = 1, ok_gen = 1, ok_fmadot = 1; double worst_s2 = 0, worst_s3 = 0, worst_d2 = 0;
    static double dg[3][CASES]; double tab[4][3], tabmad[4][3];
    for (int r = 0; r < 4; r++) {
        for (int cse = 0; cse < CASES; cse++) {
            int m = 100 + 100 * (cse % 10), n = m / 2;
            Acc dot; gendot(n, rows[r].cond, &dot);
            int m2 = 0; Acc sum; acc_zero(&sum);
            for (int i = 0; i < n; i++) { double p, q; two_product(gx[i], gy[i], &p, &q); gp[m2++] = p; gp[m2++] = q; }
            for (int i = m2 - 1; i > 0; i--) { int j = (int)rndbelow((u32)i + 1); double t = gp[i]; gp[i] = gp[j]; gp[j] = t; }
            double S = 0; for (int i = 0; i < m2; i++) { acc_add(&sum, gp[i], 0); S += dabs(gp[i]); }
            { Acc t = sum; acc_add_acc(&t, &dot, 1); ok_gen &= acc_is_zero(&t); }   // sum must equal the exact dot product
            double s_d = dabs(acc_to_double(&sum));
            double d1 = dsum(gp, m2); for (int i = 0; i < m2; i++) gw[i] = gp[i];
            double d2 = sum_k(gw, m2, 2); for (int i = 0; i < m2; i++) gw[i] = gp[i];
            double d3 = sum_k(gw, m2, 3);
            dg[0][cse] = rel_digits(&sum, d1); dg[1][cse] = rel_digits(&sum, d2); dg[2][cse] = rel_digits(&sum, d3);
            // Proposition 4.5 (Sum2) and 4.10 (SumK, K = 3)
            Acc e2 = sum; acc_add(&e2, d2, 1); double err2 = dabs(acc_to_double(&e2));
            double g = gamma_k((double)(m2 - 1)), b2 = EPS * s_d + g * g * S;
            ok_s2 &= err2 <= b2 * (1.0 + 1e-9); { double ex = err2 - EPS * s_d; if (ex > 0) { double q = ex / (g * g * S); if (q > worst_s2) worst_s2 = q; } }
            Acc e3 = sum; acc_add(&e3, d3, 1); double err3 = dabs(acc_to_double(&e3));
            double g2 = gamma_k((double)(2 * m2 - 2)), b3 = (EPS + 3.0 * g * g) * s_d + g2 * g2 * g2 * S;
            ok_s3 &= err3 <= b3 * (1.0 + 1e-9); { double ex = err3 - EPS * s_d; if (ex > 0) { double q = ex / (3.0 * g * g * s_d + g2 * g2 * g2 * S); if (q > worst_s3) worst_s3 = q; } }
            // Proposition 5.5 (Dot2) on the generated vectors
            double dd = dot2(gx, gy, n), abs_dot = 0; for (int i = 0; i < n; i++) abs_dot += dabs(gx[i] * gy[i]);
            Acc ed = dot; acc_add(&ed, dd, 1); double errd = dabs(acc_to_double(&ed)), dv = dabs(acc_to_double(&dot));
            double gn = gamma_k((double)n), bd = EPS * dv + gn * gn * abs_dot;
            ok_d2 &= errd <= bd * (1.0 + 1e-9); { double ex = errd - EPS * dv; if (ex > 0) { double q = ex / (gn * gn * abs_dot); if (q > worst_d2) worst_d2 = q; } }
            // Dot2 with Split-TwoProduct and with FMA-TwoProduct must agree
            double p, s, h, rr, q2; two_product_fma(gx[0], gy[0], &p, &s);
            for (int i = 1; i < n; i++) { two_product_fma(gx[i], gy[i], &h, &rr); two_sum(p, h, &p, &q2); s = s + (q2 + rr); }
            ok_fmadot &= (p + s) == dd;
        }
        for (int a = 0; a < 3; a++) { double med = median(dg[a], CASES); tab[r][a] = med; tabmad[r][a] = mad(dg[a], CASES, med); }
    }
    str("GenDot (Algorithm 6.1) sums, "); num(4 * CASES); str(" cases, lengths 100..1000, cond 1e7..1e28\n");
    str("  "); verdict("generated sum equals the exact dot product (TwoProduct is exact)", ok_gen);
    str("  "); verdict("Proposition 4.5, Sum2: |res - s| <= eps|s| + gamma_{n-1}^2 S", ok_s2);
    str("    error beyond the unavoidable eps|s| term, as a share of the gamma^2 term, at worst: "); share(worst_s2); str("\n");
    str("  "); verdict("Proposition 4.10, SumK K=3: |res - s| <= (eps + 3 gamma_{n-1}^2)|s| + gamma_{2n-2}^3 S", ok_s3);
    str("    error beyond the unavoidable eps|s| term, as a share of the gamma^2 term, at worst: "); share(worst_s3); str("\n");
    str("  "); verdict("Proposition 5.5, Dot2: |res - x'y| <= eps|x'y| + gamma_n^2 |x|'|y|", ok_d2);
    str("    error beyond the unavoidable eps|s| term, as a share of the gamma^2 term, at worst: "); share(worst_d2); str("\n");
    str("  "); verdict("Dot2 gives the same result with Split-TwoProduct and FMA-TwoProduct", ok_fmadot);
    str("Table 6.2, median correct digits (median absolute deviation); paper's median in [ ]; tolerance 1.0 digit\n");
    str("  cond   DSum                    Sum2                    Sum3\n");
    for (int r = 0; r < 4; r++) {
        str("  "); str(rows[r].label); str("  ");
        double pubs[3] = {rows[r].pub_dsum, rows[r].pub_sum2, rows[r].pub_sum3};
        for (int a = 0; a < 3; a++) {
            fixed1(tab[r][a]); str(" ("); fixed1(tabmad[r][a]); str(") ["); fixed1(pubs[a]); str("]");
            double d = tab[r][a] - pubs[a]; if (d < 0) d = -d;
            int ok = d <= 1.0; if (!ok) failures++;
            str(ok ? " ok    " : " FAIL  ");
        }
        str("\n");
    }
}

static void run(void) {
    rk0 = 0x5eed1234; rk1 = 0xa11ce5; rc0 = rc1 = 0; rleft = 0;
    u32 kat[4]; philox(0, 0, 0, 0, 0, 0, kat);
    verdict("Philox4x32-10 generator matches the published vector for the all-zero input", kat[0] == 0x6627e8d5 && kat[1] == 0xe169c58d && kat[2] == 0xbc57ac4c && kat[3] == 0x9b00dbd8);
    check_transformations();
    check_bounds_and_table();
    str(failures ? "FAIL\n" : "PASS\n");
    quit(failures != 0);
}
#ifdef HOST
int main(void) { run(); return 0; }
#else
void _start(void) { run(); }
#endif
