// Knuth's lagged-Fibonacci generator ran_array (D. E. Knuth, The Art of Computer Programming, Vol. 2: Seminumerical Algorithms,
// 3rd ed., Section 3.6, with the 2002 revised ran_start from exercise 3.6-15): the sequence y[n] = (y[n-100] - y[n-37]) mod 2^30.
// Written from the recurrence and from the seeding procedure (polynomial arithmetic modulo the recurrence's characteristic
// polynomial, then 10 warm-up calls); the seeding was re-implemented from the author's published reference program rng.c.
// Check: after ran_start(310952), 2009 calls of ran_array(a, 1009) leave ran_x[0] = 995235265; and 1009 calls of
// ran_array(a, 2009) from a fresh ran_start(310952) give the same value (the value printed in the author's program and in the
// book). That is one published number, checked through two call patterns. Exit 0 only if both match.
#include "ounce_paper.h"

#ifndef KNUTH_LL
#define KNUTH_LL 37      // the short lag; the long lag is 100
#endif
#define KK 100
#define LL KNUTH_LL
#define MOD ((long)1 << 30)
static long ran_x[KK];                      // the generator state: the next 100 terms y[p] .. y[p+99], ran_x[0] = y[p]
static long sub(long a, long b) { return (a - b) & (MOD - 1); }

// put n new terms in aa (n >= KK): y[p] .. y[p+n-1]; ran_x becomes y[p+n] .. y[p+n+99]
static void ran_array(long *aa, int n) {
    int j;
    for (j = 0; j < KK; j++) aa[j] = ran_x[j];
    for (; j < n; j++) aa[j] = sub(aa[j - KK], aa[j - LL]);
    int i;
    for (i = 0; i < LL; i++, j++) ran_x[i] = sub(aa[j - KK], aa[j - LL]);
    for (; i < KK; i++, j++) ran_x[i] = sub(aa[j - KK], ran_x[i - LL]);
}
static void ran_start(long seed) {
    long x[KK + KK - 1];
    long ss = (seed + 2) & (MOD - 2);
    for (int j = 0; j < KK; j++) { x[j] = ss; ss <<= 1; if (ss >= MOD) ss -= MOD - 2; }
    x[1]++;                                            // make x[1], and only x[1], odd
    ss = seed & (MOD - 1);
    int t = 69;                                        // separation parameter 70 between streams
    while (t) {
        for (int j = KK - 1; j > 0; j--) { x[j + j] = x[j]; x[j + j - 1] = 0; }       // square the polynomial
        for (int j = KK + KK - 2; j >= KK; j--) {                                     // reduce modulo the characteristic polynomial
            x[j - (KK - LL)] = sub(x[j - (KK - LL)], x[j]);
            x[j - KK] = sub(x[j - KK], x[j]);
        }
        if (ss & 1) {                                                                 // multiply by z
            for (int j = KK; j > 0; j--) x[j] = x[j - 1];
            x[0] = x[KK];
            x[LL] = sub(x[LL], x[KK]);
        }
        if (ss) ss >>= 1; else t--;
    }
    for (int j = 0; j < LL; j++) ran_x[j + KK - LL] = x[j];
    for (int j = LL; j < KK; j++) ran_x[j - LL] = x[j];
    for (int j = 0; j < 10; j++) ran_array(x, KK + KK - 1);                          // warm up
}

int paper_main(void) {
    static long a[2009];
    int ok = 1;
    ran_start(310952L);
    for (int m = 0; m < 2009; m++) ran_array(a, 1009);
    ok &= pcheck_u64("ran_start(310952), 2009 x ran_array(a,1009), ran_x[0]", (u64)ran_x[0], 995235265ULL, 8);
    ran_start(310952L);
    for (int m = 0; m < 1009; m++) ran_array(a, 2009);
    ok &= pcheck_u64("ran_start(310952), 1009 x ran_array(a,2009), ran_x[0]", (u64)ran_x[0], 995235265ULL, 8);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
