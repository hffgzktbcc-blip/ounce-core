// Pollard's Monte Carlo (rho) factorization with Floyd's cycle finding and with Brent's cycle finding, as analysed in
// R. P. Brent, "An improved Monte Carlo factorization algorithm", BIT 20 (1980) 176-184. Section 8 / Table 8.1 count the number
// M_p of multiplications mod N needed to find the least prime factor p of N, with x0 = 0, f(x) = x^2 + 3 mod N and m = 1
// (one product term per GCD), and report the maxima of M_p / sqrt(p) over the odd primes p < 10^8: 20.3613 for the algorithm
// based on Floyd's cycle finding, attained at p = 99,398,833, and 18.9972 for Brent's algorithm (the listing of Section 7),
// attained at p = 48,569,393; and the actual means 4.122795 and 3.122533.
// Written from the two listings in the paper. Whether a factor p is found depends only on the sequence modulo p (the GCD with N
// first exceeds 1 when the product q contains a term divisible by p), so the count M_p is obtained by running the sequence modulo p.
//   Floyd based (Section 5): x := f(x); y := f(f(y)); q := q * |x - y|; one iteration costs 4 multiplications (3 for f, 1 for q).
//   Brent's algorithm: y := x0; r := 1;  repeat  x := y; r times y := f(y); then, up to r times, y := f(y), q := q * |x - y|
//   (2 multiplications per step), r := 2r, until x = y (mod p).
// Checks (fixed before the first run): the two maxima to the 4 published decimals; for every odd prime p < 300000, M_p / sqrt(p)
// does not exceed the published maxima; the mean over those primes is within 1% of the published actual means (a smaller sample
// than the paper's 10^8, so only a consistency check).  Exit 0 only if all checks pass.
#include "ounce_paper.h"

#ifndef BRENT_C
#define BRENT_C 3        // f(x) = x^2 + c
#endif

static u64 f(u64 v, u64 p) { return (v * v + BRENT_C) % p; }

static u64 mults_floyd(u64 p) {
    u64 x = 0, y = 0, j = 0;
    for (;;) {
        j++; x = f(x, p); y = f(f(y, p), p);
        if (x == y) return 4 * j;
    }
}

static u64 mults_brent(u64 p) {
    u64 y = 0, x, r = 1, M = 0;
    for (;;) {
        x = y;
        for (u64 i = 0; i < r; i++) { y = f(y, p); M++; }
        for (u64 k = 0; k < r; k++) {
            y = f(y, p); M += 2;
            if (x == y) return M;
        }
        r *= 2;
    }
}

static u64 round4(double v) { return (u64)(v * 10000.0 + 0.5); }
static void pfix4(u64 v) { pdec((i64)(v / 10000)); pstr("."); u64 f4 = v % 10000; char b[4]; for (int i = 3; i >= 0; i--) { b[i] = (char)('0' + f4 % 10); f4 /= 10; } paper_write(b, 4); }

#define PLIM 300000
static u8 comp[PLIM + 1];

int paper_main(void) {
    int ok = 1;
    // 1. the published maxima
    {
        u64 p1 = 99398833ULL, p2 = 48569393ULL;
        u64 a = round4((double)mults_floyd(p1) / paper_sqrt((double)p1));
        u64 b = round4((double)mults_brent(p2) / paper_sqrt((double)p2));
        int g1 = a == 203613, g2 = b == 189972;
        ok &= g1 & g2;
        pstr(g1 ? "PASS " : "FAIL "); pstr("Floyd-based M_p/sqrt(p) at p = 99398833: "); pfix4(a); pstr(" (paper 20.3613)\n");
        pstr(g2 ? "PASS " : "FAIL "); pstr("Brent M_p/sqrt(p) at p = 48569393: "); pfix4(b); pstr(" (paper 18.9972)\n");
    }
    // 2. scan of the odd primes below 300000
    for (u64 i = 2; i * i <= PLIM; i++) if (!comp[i]) for (u64 j = i * i; j <= PLIM; j += i) comp[j] = 1;
    double sa = 0, sb = 0, ma = 0, mb = 0; u64 n = 0;
    for (u64 p = 3; p <= PLIM; p++) {
        if (comp[p]) continue;
        double s = paper_sqrt((double)p);
        double ra = (double)mults_floyd(p) / s, rb = (double)mults_brent(p) / s;
        sa += ra; sb += rb; if (ra > ma) ma = ra; if (rb > mb) mb = rb; n++;
    }
    double meana = sa / (double)n, meanb = sb / (double)n;
    pstr("odd primes below 300000: "); pdec((i64)n); pstr("\n");
    u64 qa = round4(ma), qb = round4(mb);
    int g3 = qa <= 203613 && qb <= 189972;
    ok &= g3;
    pstr(g3 ? "PASS " : "FAIL "); pstr("maxima over the scan: Floyd-based "); pfix4(qa); pstr(", Brent "); pfix4(qb); pstr(" (not above 20.3613 and 18.9972)\n");
    u64 ea = round4(meana), eb = round4(meanb);
    // within 1% of the published actual means 4.122795 and 3.122533
    int g4 = meana > 4.122795 * 0.99 && meana < 4.122795 * 1.01 && meanb > 3.122533 * 0.99 && meanb < 3.122533 * 1.01;
    ok &= g4;
    pstr(g4 ? "PASS " : "FAIL "); pstr("means over the scan: Floyd-based "); pfix4(ea); pstr(", Brent "); pfix4(eb);
    pstr(" (paper, p < 10^8: 4.122795 and 3.122533; tolerance 1%)\n");
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
