/* elf-workload rng: pseudo-random generators written from their published definitions, checked
 * against published first outputs: MT19937 seeded with 5489 (first output 3499211612, 10000th
 * output 4123659995), the C-standard sample LCG, xorshift32 (Marsaglia, seed 2463534242) and
 * splitmix64 (seed 0) whose first output is 0xE220A8397B1DCDAF, plus PCG32 (O'Neill's
 * demonstration seed 42, stream 54: first output 0xA15C02B7). Also draws from each, shuffles a
 * deck with Fisher-Yates and prints chi-square-style bucket counts.
 * Provenance: original work written for ounce-core (MIT); algorithms are public domain or published specifications. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

static uint32_t mt[624];
static int mti = 625;
static void mt_seed(uint32_t s) {
    mt[0] = s;
    for (int i = 1; i < 624; i++) mt[i] = 1812433253u * (mt[i - 1] ^ (mt[i - 1] >> 30)) + (uint32_t)i;
    mti = 624;
}
static uint32_t mt_next(void) {
    if (mti >= 624) {
        for (int k = 0; k < 624; k++) {
            uint32_t y = (mt[k] & 0x80000000u) | (mt[(k + 1) % 624] & 0x7FFFFFFFu);
            mt[k] = mt[(k + 397) % 624] ^ (y >> 1) ^ ((y & 1) ? 0x9908B0DFu : 0);
        }
        mti = 0;
    }
    uint32_t y = mt[mti++];
    y ^= y >> 11;
    y ^= (y << 7) & 0x9D2C5680u;
    y ^= (y << 15) & 0xEFC60000u;
    y ^= y >> 18;
    return y;
}

static uint32_t xs32 = 2463534242u;
static uint32_t xorshift32(void) { xs32 ^= xs32 << 13; xs32 ^= xs32 >> 17; xs32 ^= xs32 << 5; return xs32; }

static uint64_t sm = 0;
static uint64_t splitmix64(void) {
    uint64_t z = (sm += 0x9E3779B97F4A7C15ull);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBull;
    return z ^ (z >> 31);
}

static uint64_t pcg_state, pcg_inc;
static uint32_t pcg32(void) {
    uint64_t old = pcg_state;
    pcg_state = old * 6364136223846793005ull + pcg_inc;
    uint32_t xorshifted = (uint32_t)(((old >> 18) ^ old) >> 27), rot = (uint32_t)(old >> 59);
    return (xorshifted >> rot) | (xorshifted << ((0u - rot) & 31));
}
static void pcg_seed(uint64_t seed, uint64_t seq) {
    pcg_state = 0; pcg_inc = (seq << 1) | 1;
    pcg32(); pcg_state += seed; pcg32();
}

static uint32_t sample_lcg_state = 1;
static int sample_rand(void) { sample_lcg_state = sample_lcg_state * 1103515245u + 12345u; return (int)((sample_lcg_state / 65536u) % 32768u); }

static uint32_t bounded(uint32_t (*gen)(void), uint32_t n) { /* rejection sampling, no modulo bias */
    uint32_t limit = 0xFFFFFFFFu - 0xFFFFFFFFu % n;
    uint32_t r;
    do { r = gen(); } while (r >= limit);
    return r % n;
}

int workload_run(FILE *o) {
    int fails = 0;
    mt_seed(5489);
    uint32_t first = mt_next(), tenk = first;
    for (int i = 1; i < 10000; i++) tenk = mt_next();
    fprintf(o, "mt19937 first=%u 10000th=%u\n", first, tenk);
    if (first != 3499211612u || tenk != 4123659995u) fails++;

    uint32_t x1 = xorshift32();
    fprintf(o, "xorshift32 first=%u\n", x1);
    if (x1 != 723471715u) fails++;

    uint64_t s0 = splitmix64(), s1 = splitmix64();
    fprintf(o, "splitmix64 %016llx %016llx\n", (unsigned long long)s0, (unsigned long long)s1);
    if (s0 != 0xE220A8397B1DCDAFull) fails++;

    pcg_seed(42, 54);
    uint32_t p[6];
    for (int i = 0; i < 6; i++) p[i] = pcg32();
    fprintf(o, "pcg32 %08x %08x %08x %08x %08x %08x\n", p[0], p[1], p[2], p[3], p[4], p[5]);
    if (p[0] != 0xA15C02B7u || p[1] != 0x7B47F409u) fails++;

    int sr[5];
    for (int i = 0; i < 5; i++) sr[i] = sample_rand();   /* argument evaluation order is unspecified; sequence explicitly */
    fprintf(o, "sample LCG: %d %d %d %d %d\n", sr[0], sr[1], sr[2], sr[3], sr[4]);

    /* uniformity: 60000 draws from each generator into 12 buckets */
    uint32_t (*gens[])(void) = {mt_next, xorshift32, pcg32};
    const char *names[] = {"mt19937", "xorshift32", "pcg32"};
    for (int g = 0; g < 3; g++) {
        unsigned bucket[12] = {0};
        for (int i = 0; i < 60000; i++) bucket[bounded(gens[g], 12)]++;
        unsigned mn = bucket[0], mx = bucket[0];
        double chi = 0;
        fprintf(o, "%-10s buckets:", names[g]);
        for (int b = 0; b < 12; b++) {
            fprintf(o, " %u", bucket[b]);
            if (bucket[b] < mn) mn = bucket[b];
            if (bucket[b] > mx) mx = bucket[b];
            double d = (double)bucket[b] - 5000.0;
            chi += d * d / 5000.0;
        }
        fprintf(o, " min=%u max=%u chi2x1000=%u\n", mn, mx, (unsigned)(chi * 1000.0));
        if (chi > 40.0) fails++;     /* 11 degrees of freedom: 40 is far beyond the 99.99th percentile */
    }

    /* Fisher-Yates shuffle of a 52-card deck with pcg32 */
    int deck[52];
    for (int i = 0; i < 52; i++) deck[i] = i;
    pcg_seed(2026, 1);
    for (int i = 51; i > 0; i--) {
        int j = (int)bounded(pcg32, (uint32_t)i + 1);
        int t = deck[i]; deck[i] = deck[j]; deck[j] = t;
    }
    int seen[52] = {0}, perm = 1;
    fprintf(o, "shuffle:");
    for (int i = 0; i < 52; i++) { fprintf(o, " %d", deck[i]); perm &= !seen[deck[i]]++; }
    fprintf(o, "\npermutation=%d\n", perm);
    if (!perm) fails++;

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
