// Division by invariant integers using multiplication: T. Granlund and P. L. Montgomery, PLDI 1994 (SIGPLAN Notices 29(6), 61-72).
// Written from the paper's Figure 6.2 (CHOOSE_MULTIPLIER), Figure 4.2 (unsigned n/d for constant d, N = 32) and Section 9 (exact
// division and divisibility by multiplying with the inverse mod 2^N). Checks:
//   1. the constants the paper states in its examples: n/10 -> (m, shpost, l) = ((2^34+1)/5, 3, 4); n/7 -> m = (2^35+3)/7 > 2^32;
//      n/14 -> shpre = 1 and ((2^34+5)/7, shpost 2); d = 641 -> final shift zero; CHOOSE_MULTIPLIER(3, 31) -> shpost 0 and
//      m = (2^32+2)/3; divisibility by 100: dinv = (19*2^32+1)/25 and qmax = (2^31-48)/25;
//   2. the generated code (multiply-high, shifts, adds as in Figure 4.2) returns exactly floor(n/d), compared with the CPU's own
//      division, for every divisor d = 1..65536 and for 62 more divisors (2^k-1, 2^k, 2^k+1 for k = 17..31 and some others), each with
//      about 100 dividends (0, 1, 2^32-1, neighbours of multiples of d near the ends and a fixed pseudo-random set);
//   3. the divisibility test of Section 9 (signed, d = 100: q0 = MULL(dinv, i) is a multiple of 4 within [-qmax, qmax]) agrees with
//      i mod 100 = 0 for every 1021st 32-bit value and for all multiples of 100 within 20000 of -2^31, 0 and 2^31.
// Exit 0 only if everything agrees.
#include "ounce_paper.h"

#ifndef GM_ADD
#define GM_ADD 1          // include the 2^(N+l-prec) term in m_high (Figure 6.2)
#endif

#define NB 32

static int ceil_log2(u64 d) { return d <= 1 ? 0 : 64 - __builtin_clzll(d - 1); }

// Figure 6.2 for d <= 2^31, prec <= 32 (all intermediates fit in 64 bits)
static void choose_multiplier(u64 d, int prec, u64 *m, int *shpost, int *l) {
    int ll = ceil_log2(d), sh = ll;
    u64 mlow = ((u64)1 << (NB + ll)) / d;
    u64 mhigh = (((u64)1 << (NB + ll)) + (GM_ADD ? ((u64)1 << (NB + ll - prec)) : 0)) / d;
    while (mlow / 2 < mhigh / 2 && sh > 0) { mlow /= 2; mhigh /= 2; sh--; }
    *m = mhigh; *shpost = sh; *l = ll;
}

typedef struct { u64 m; int shpre, shpost, l; int pow2; int big; } divplan;

// Figure 4.2
static divplan plan(u64 d) {
    divplan p; u64 m; int shpost, l;
    choose_multiplier(d, NB, &m, &shpost, &l);
    p.shpre = 0;
    if (m >= ((u64)1 << NB) && (d & 1) == 0) {
        int e = __builtin_ctzll(d);
        p.shpre = e;
        int ld;
        choose_multiplier(d >> e, NB - e, &m, &shpost, &ld);
    }
    p.m = m; p.shpost = shpost; p.l = l;
    p.pow2 = d == ((u64)1 << l);
    p.big = m >= ((u64)1 << NB);
    return p;
}

static u32 divide(const divplan *p, u32 n) {
    if (p->pow2) return n >> p->l;
    if (p->big) {
        u32 t1 = (u32)(((p->m - ((u64)1 << NB)) * n) >> NB);
        return (t1 + ((n - t1) >> 1)) >> (p->shpost - 1);
    }
    return (u32)(((p->m * (u64)(n >> p->shpre)) >> NB) >> p->shpost);
}

static u64 rng = 0x9E3779B97F4A7C15ULL;
static u32 rnd(void) { rng = rng * 6364136223846793005ULL + 1442695040888963407ULL; return (u32)(rng >> 32); }

static u64 tested_div = 0, bad_div = 0, nvals = 0;

static void check_divisor(u32 d) {
    divplan p = plan(d);
    tested_div++;
    u32 qmax = 0xFFFFFFFFu / d;
    u32 vals[100]; int nv = 0;
    vals[nv++] = 0; vals[nv++] = 1; vals[nv++] = 0xFFFFFFFFu; vals[nv++] = 0xFFFFFFFEu; vals[nv++] = 0x80000000u; vals[nv++] = 0x7FFFFFFFu;
    for (u32 k = 1; k <= 4 && k <= qmax; k++) { u64 x = (u64)k * d; vals[nv++] = (u32)(x - 1); vals[nv++] = (u32)x; vals[nv++] = (u32)(x + 1); }
    for (u32 k = 0; k < 4 && k < qmax; k++) {
        u64 x = (u64)(qmax - k) * d;
        vals[nv++] = (u32)(x - 1); vals[nv++] = (u32)x; if (x + 1 <= 0xFFFFFFFFu) vals[nv++] = (u32)(x + 1);
    }
    if (d < 0xFFFFFFFFu) { vals[nv++] = d - 1; vals[nv++] = d; vals[nv++] = d + 1; }
    while (nv < 100) vals[nv++] = rnd();
    for (int i = 0; i < nv; i++) { nvals++; if (divide(&p, vals[i]) != vals[i] / d) bad_div++; }
}

int paper_main(void) {
    int ok = 1;
    // 1. constants stated in the paper
    {
        u64 m; int sp, l; int g = 1;
        choose_multiplier(10, 32, &m, &sp, &l);
        int g1 = m == (((u64)1 << 34) + 1) / 5 && sp == 3 && l == 4;
        pstr(g1 ? "PASS " : "FAIL "); pstr("d = 10: m = "); pdec((i64)m); pstr(" = (2^34+1)/5, shpost "); pdec(sp); pstr(", l "); pdec(l); pstr("\n");
        choose_multiplier(7, 32, &m, &sp, &l);
        int g2 = m == (((u64)1 << 35) + 3) / 7 && m > ((u64)1 << 32);
        pstr(g2 ? "PASS " : "FAIL "); pstr("d = 7: m = "); pdec((i64)m); pstr(" = (2^35+3)/7, above 2^32\n");
        divplan p14 = plan(14);
        int g3 = p14.shpre == 1 && p14.m == (((u64)1 << 34) + 5) / 7 && p14.shpost == 2;
        pstr(g3 ? "PASS " : "FAIL "); pstr("d = 14: shpre "); pdec(p14.shpre); pstr(", m = "); pdec((i64)p14.m); pstr(" = (2^34+5)/7, shpost "); pdec(p14.shpost); pstr("\n");
        choose_multiplier(641, 32, &m, &sp, &l);
        int g4 = sp == 0 && m < ((u64)1 << 32);
        pstr(g4 ? "PASS " : "FAIL "); pstr("d = 641: final shift "); pdec(sp); pstr(" (paper: zero)\n");
        // CHOOSE_MULTIPLIER(3, 31) as used for signed division: shpost 0 and m = (2^32+2)/3
        {
            int ll = ceil_log2(3), sh = ll;
            u64 mlow = ((u64)1 << (NB + ll)) / 3, mhigh = (((u64)1 << (NB + ll)) + ((u64)1 << (NB + ll - 31))) / 3;
            while (mlow / 2 < mhigh / 2 && sh > 0) { mlow /= 2; mhigh /= 2; sh--; }
            int g5 = sh == 0 && mhigh == (((u64)1 << 32) + 2) / 3;
            pstr(g5 ? "PASS " : "FAIL "); pstr("signed d = 3 (prec 31): shpost "); pdec(sh); pstr(", m = "); pdec((i64)mhigh); pstr(" = (2^32+2)/3\n");
            g = g1 && g2 && g3 && g4 && g5;
        }
        // divisibility by 100: dinv = inverse of 25 mod 2^32 equals (19*2^32+1)/25
        u32 dinv = 25; for (int i = 0; i < 5; i++) dinv *= 2 - 25 * dinv;
        u64 want = (19 * ((u64)1 << 32) + 1) / 25;
        int g6;
        g6 = dinv == want && (u32)(dinv * 25u) == 1u && (((u64)1 << 31) - 48) % 25 == 0;
        pstr(g6 ? "PASS " : "FAIL "); pstr("d = 100: dinv = "); pdec((i64)dinv); pstr(" = (19*2^32+1)/25, qmax = "); pdec((i64)((((u64)1 << 31) - 48) / 25)); pstr("\n");
        ok &= g && g6;
    }
    // 2. generated division code against the CPU's division
    for (u32 d = 1; d <= 65536; d++) check_divisor(d);
    for (int k = 17; k <= 31; k++) {
        u32 b = (u32)1 << k;
        check_divisor(b - 1); check_divisor(b); check_divisor(b + 1 > 0x80000000u ? 0x7FFFFFFFu : b + 1);
    }
    static const u32 EXTRA[17] = {100, 641, 1000, 1000000, 1000000007u, 3000000000u / 2, 274177, 6700417, 999999999, 123456789, 2147483647u, 1431655765u, 1431655766u, 715827883u, 4000u, 99991u, 1073741825u};
    for (int i = 0; i < 17; i++) check_divisor(EXTRA[i]);
    pstr("divisors tested: "); pdec((i64)tested_div); pstr(", quotients compared: "); pdec((i64)nvals); pstr(", wrong: "); pdec((i64)bad_div); pstr("\n");
    ok &= bad_div == 0;
    // 3. divisibility by 100 via the inverse
    {
        u32 dinv = (u32)((19 * ((u64)1 << 32) + 1) / 25);
        i64 qmax = (((i64)1 << 31) - 48) / 25;
        u64 tested = 0, wrong = 0;
        for (i64 i = -((i64)1 << 31); i < ((i64)1 << 31); i += 1021) {
            i64 q0 = (i64)(int)((u32)i * dinv);
            int div = (q0 & 3) == 0 && q0 >= -qmax && q0 <= qmax;
            tested++; if (div != (i % 100 == 0)) wrong++;
        }
        i64 centers[3] = {-((i64)1 << 31), 0, ((i64)1 << 31) - 1};
        for (int c = 0; c < 3; c++)
            for (i64 i = centers[c] - 20000; i <= centers[c] + 20000; i++) {
                if (i < -((i64)1 << 31) || i >= ((i64)1 << 31)) continue;
                i64 q0 = (i64)(int)((u32)i * dinv);
                int div = (q0 & 3) == 0 && q0 >= -qmax && q0 <= qmax;
                tested++; if (div != (i % 100 == 0)) wrong++;
            }
        pstr("divisibility by 100 tested on "); pdec((i64)tested); pstr(" values, wrong: "); pdec((i64)wrong); pstr("\n");
        ok &= wrong == 0;
    }
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
