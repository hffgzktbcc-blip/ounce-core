// RANLUX family: the subtract-with-carry generator of Marsaglia and Zaman ("A New Class of Random Number Generators", Annals of
// Applied Probability 1(3), 1991) with Luscher's luxury-level decimation ("A portable high-quality random number generator for
// lattice field theory simulations", Computer Physics Communications 79, 1994), written from the recurrence
//   Y = X[i-s] - X[i-r] - c,  X[i] = Y mod 2^w,  c = (Y < 0),
// and from the decimation "use r values of every block of p, discard the rest". Four generators are run with the parameters the
// C++ standard fixes (ranlux24_base: w=24 s=10 r=24; ranlux48_base: w=48 s=5 r=12; ranlux24: ranlux24_base with p=223 r=23;
// ranlux48: ranlux48_base with p=389 r=11), default-seeded as the standard specifies (19780503 through a 40014 mod 2147483563
// generator). The check is the standard's required value of the 10000th output of each. Four published numbers.
// The expected numbers are data copied as numbers; no third-party code was copied. Exit 0 only if all four match.
#include "ounce_paper.h"

#ifndef RANLUX_S24
#define RANLUX_S24 10        // short lag of ranlux24_base (the long lag is 24)
#endif
typedef struct { u64 x[24]; int r, s, w, p; u64 carry; } Swc;

static void swc_seed(Swc *g, int w, int s, int r, u64 value) {
    g->w = w; g->s = s; g->r = r; g->p = 0;
    u64 e = value == 0 ? 19780503ULL : value % 2147483563ULL;       // the LCG that seeds the state
    int n = (w + 31) / 32;
    u64 mask = w == 64 ? ~0ULL : ((u64)1 << w) - 1;
    for (int k = 0; k < r; k++) {
        u64 v = 0;
        for (int j = 0; j < n; j++) { e = e * 40014ULL % 2147483563ULL; v += e << (32 * j); }
        g->x[k] = v & mask;                                         // x[0] is the oldest value X[-r]
    }
    g->carry = g->x[r - 1] == 0 ? 1 : 0;
}
static u64 swc_next(Swc *g) {
    u64 mask = ((u64)1 << g->w) - 1;
    u64 xs = g->x[(g->p + g->r - g->s) % g->r], xr = g->x[g->p];    // X[i-s], X[i-r]
    u64 y = xs - xr - g->carry;
    g->carry = (xs < xr + g->carry) ? 1 : 0;                        // Y < 0
    y &= mask;
    g->x[g->p] = y;
    g->p = (g->p + 1) % g->r;
    return y;
}
typedef struct { Swc b; int p, r, n; } Disc;
static u64 disc_next(Disc *d) {
    if (d->n >= d->r) { for (int i = 0; i < d->p - d->r; i++) swc_next(&d->b); d->n = 0; }
    d->n++;
    return swc_next(&d->b);
}

int paper_main(void) {
    int ok = 1;
    u64 v = 0;
    Swc a; swc_seed(&a, 24, RANLUX_S24, 24, 0);
    for (int i = 0; i < 10000; i++) v = swc_next(&a);
    ok &= pcheck_u64("ranlux24_base, 10000th output", v, 7937952ULL, 6);
    Swc b; swc_seed(&b, 48, 5, 12, 0);
    for (int i = 0; i < 10000; i++) v = swc_next(&b);
    ok &= pcheck_u64("ranlux48_base, 10000th output", v, 61839128582725ULL, 12);
    Disc c; swc_seed(&c.b, 24, RANLUX_S24, 24, 0); c.p = 223; c.r = 23; c.n = 0;
    for (int i = 0; i < 10000; i++) v = disc_next(&c);
    ok &= pcheck_u64("ranlux24 (p=223, r=23), 10000th output", v, 9901578ULL, 6);
    Disc d; swc_seed(&d.b, 48, 5, 12, 0); d.p = 389; d.r = 11; d.n = 0;
    for (int i = 0; i < 10000; i++) v = disc_next(&d);
    ok &= pcheck_u64("ranlux48 (p=389, r=11), 10000th output", v, 249142670248501ULL, 12);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
