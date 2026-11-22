// Jump consistent hash from Lamping and Veach, "A Fast, Minimal Memory, Consistent Hash Algorithm" (Google, 2014; arXiv:1406.2294),
// written from the paper's Figure 1 and its explanation (a 64-bit linear congruential generator seeded with the key, jumps
// at bucket b to floor((b+1)/r)). Checks the properties the paper states, on generated keys (fixed seeds):
//  1. monotonicity: going from n to n+1 buckets a key either stays or moves to the new bucket n (20000 keys, n = 1..200: 0 violations)
//  2. balance of the rebalancing: growing from 10 to 12 buckets moves 1/6 of the keys (the paper's example), within 0.01 absolute
//     on 200000 keys; growing from 1000 to 1001 buckets moves about 0.1% (the paper's example), 400 to 600 of 500000 keys
//  3. cost: the mean number of loop iterations is one plus the number of jumps below n, i.e. the harmonic number H_n = 1 + sum 1/i
//     (i = 2..n), within 2%, and below ln(n) + 1 as the paper states, for n = 2, 3, 10, 100, 1000, 100000 (200000 keys each)
// Tolerances fixed before the first run. Exit 0 only if everything holds. The 7.64e-9 standard error of the paper's Figure 2
// and its timing figures are not reproduced.
#include "ounce_paper.h"

#ifndef NEG
#define NEG 0   // NEG=1: the random value is derived from key>>32 instead of key>>33
#endif

static long its;   // loop iterations of the last call
static int jump_hash(u64 key, int num_buckets) {                      // Figure 1
    i64 b = -1, j = 0; its = 0;
    while (j < num_buckets) {
        b = j; its++;
        key = key * 2862933555777941757ULL + 1;
        j = (i64)((double)(b + 1) * ((double)(1LL << 31) / (double)((key >> (NEG ? 32 : 33)) + 1)));
    }
    return (int)b;
}
static u64 mix(u64 x) { x += 0x9e3779b97f4a7c15ULL; x = (x ^ (x >> 30)) * 0xbf58476d1ce4e5b9ULL; x = (x ^ (x >> 27)) * 0x94d049bb133111ebULL; return x ^ (x >> 31); }
static double LN2;
static double my_ln(double x) {                                       // ln x = k ln2 + 2 atanh((m-1)/(m+1)), m in [1,2)
    int k = 0; while (x >= 2.0) { x /= 2.0; k++; }
    double z = (x - 1.0) / (x + 1.0), z2 = z * z, t = z, s = 0.0;
    for (int i = 0; i < 40; i++) { s += t / (double)(2 * i + 1); t *= z2; }
    return (double)k * LN2 + 2.0 * s;
}
static void pfix(double v, int dec) { u64 s = 1; for (int i = 0; i < dec; i++) s *= 10; u64 r = (u64)(v * (double)s + 0.5); pdec((i64)(r / s)); pstr("."); char d[8]; for (int i = dec - 1; i >= 0; i--) { d[i] = (char)('0' + r % 10); r /= 10; } paper_write(d, dec); }

int paper_main(void) {
    int ok = 1;
    { double s = 0, p = 1; for (int j = 1; j < 60; j++) { p /= 2; s += p / (double)j; } LN2 = s; }
    // 1. monotonicity
    long viol = 0, checks = 0;
    for (int i = 0; i < 20000; i++) {
        u64 key = mix((u64)i); int prev = 0;
        for (int n = 1; n <= 200; n++) { int c = jump_hash(key, n); if (n > 1 && !(c == prev || c == n - 1)) viol++; prev = c; checks++; }
    }
    pstr("monotonicity: "); pdec(viol); pstr(" violations in "); pdec(checks); pstr(" (key, n) steps\n"); ok &= viol == 0;
    // 2. fraction moved
    long moved = 0, all_new = 1; for (int i = 0; i < 200000; i++) { u64 key = mix(1000000ULL + (u64)i); int a = jump_hash(key, 10), b = jump_hash(key, 12); if (a != b) moved++; }
    pstr("10 -> 12 buckets: moved "); pdec(moved); pstr(" of 200000 = "); pfix((double)moved / 200000.0, 4); pstr(" (paper: 1/6 = 0.1667)\n");
    double fr = (double)moved / 200000.0; int r2 = fr >= 1.0 / 6.0 - 0.01 && fr <= 1.0 / 6.0 + 0.01; ok &= r2;
    long m2 = 0; for (int i = 0; i < 500000; i++) { u64 key = mix(5000000ULL + (u64)i); int a = jump_hash(key, 1000), b = jump_hash(key, 1001); if (a != b) { m2++; if (b != 1000) all_new = 0; } }
    pstr("1000 -> 1001 buckets: moved "); pdec(m2); pstr(" of 500000 (expected about 500; range 400 to 600), all to the new bucket: "); pstr(all_new ? "yes\n" : "no\n");
    ok &= m2 >= 400 && m2 <= 600 && all_new;
    // 3. iterations
    static const int NS[6] = {2, 3, 10, 100, 1000, 100000}; int r3 = 0;
    for (int t = 0; t < 6; t++) {
        int n = NS[t]; u64 tot = 0; for (int i = 0; i < 200000; i++) { jump_hash(mix(9000000ULL + (u64)i), n); tot += (u64)its; }
        double mean = (double)tot / 200000.0, h = 1.0; for (int i = 2; i <= n; i++) h += 1.0 / (double)i;
        double lnb = my_ln((double)n) + 1.0; double d = mean > h ? mean - h : h - mean;
        int good = d <= 0.02 * h && mean < lnb; r3 += good;
        pstr("n="); pdec(n); pstr(": mean iterations "); pfix(mean, 4); pstr(", H_n "); pfix(h, 4); pstr(", ln(n)+1 "); pfix(lnb, 4); pstr(good ? " ok\n" : " OUT OF TOLERANCE\n");
    }
    pstr("iteration counts agreeing with the paper's analysis: "); pdec(r3); pstr(" of 6\n"); ok &= r3 == 6;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
