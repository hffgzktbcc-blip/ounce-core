/* elf-workload intmath: number theory on 64-bit and 128-bit integers (__int128 exercises the
 * compiler runtime's multiply and divide helpers): sieve (pi(100000) = 9592), gcd/lcm, extended
 * Euclid, modular exponentiation, deterministic Miller-Rabin for 64-bit inputs, integer square
 * root, Collatz lengths, perfect numbers, Euler phi.
 * Provenance: original work written for ounce-core (MIT); textbook algorithms. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

typedef unsigned __int128 u128;

static uint64_t gcd(uint64_t a, uint64_t b) { while (b) { uint64_t t = a % b; a = b; b = t; } return a; }
static int64_t egcd(int64_t a, int64_t b, int64_t *x, int64_t *y) {
    if (!b) { *x = 1; *y = 0; return a; }
    int64_t x1, y1, g = egcd(b, a % b, &x1, &y1);
    *x = y1;
    *y = x1 - (a / b) * y1;
    return g;
}
static uint64_t mulmod(uint64_t a, uint64_t b, uint64_t m) { return (uint64_t)((u128)a * b % m); }
static uint64_t powmod(uint64_t b, uint64_t e, uint64_t m) {
    uint64_t r = 1 % m;
    b %= m;
    while (e) { if (e & 1) r = mulmod(r, b, m); b = mulmod(b, b, m); e >>= 1; }
    return r;
}
static int is_prime(uint64_t n) {
    if (n < 2) return 0;
    static const uint64_t small[] = {2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37};
    for (unsigned i = 0; i < 12; i++) { if (n % small[i] == 0) return n == small[i]; }
    uint64_t d = n - 1;
    int s = 0;
    while (!(d & 1)) { d >>= 1; s++; }
    for (unsigned i = 0; i < 12; i++) { /* these 12 bases are deterministic for all n < 3.3e24 */
        uint64_t x = powmod(small[i], d, n);
        if (x == 1 || x == n - 1) continue;
        int comp = 1;
        for (int r = 1; r < s; r++) { x = mulmod(x, x, n); if (x == n - 1) { comp = 0; break; } }
        if (comp) return 0;
    }
    return 1;
}
static uint64_t isqrt(uint64_t n) {
    uint64_t lo = 0, hi = 0xFFFFFFFFull;
    while (lo < hi) {
        uint64_t mid = lo + (hi - lo + 1) / 2;
        if (mid * mid <= n) lo = mid; else hi = mid - 1;
    }
    return lo;
}
static uint32_t phi(uint32_t n) {
    uint32_t r = n;
    for (uint32_t p = 2; (uint64_t)p * p <= n; p++) {
        if (n % p == 0) { while (n % p == 0) n /= p; r -= r / p; }
    }
    if (n > 1) r -= r / n;
    return r;
}

int workload_run(FILE *o) {
    int fails = 0;
    static uint8_t comp[100001];
    unsigned pi = 0;
    for (unsigned i = 2; i <= 100000; i++) {
        if (!comp[i]) { pi++; for (unsigned long j = (unsigned long)i * i; j <= 100000; j += i) comp[j] = 1; }
    }
    fprintf(o, "pi(100000)=%u\n", pi);
    if (pi != 9592) fails++;

    fprintf(o, "gcd(1071,462)=%llu lcm(21,6)=%llu gcd(2^40,6^20)=%llu\n",
            (unsigned long long)gcd(1071, 462), (unsigned long long)(21 / gcd(21, 6) * 6),
            (unsigned long long)gcd(1ull << 40, 3656158440062976ull));
    int64_t x, y;
    int64_t g = egcd(240, 46, &x, &y);
    fprintf(o, "egcd(240,46)=%lld x=%lld y=%lld check=%lld\n", (long long)g, (long long)x, (long long)y,
            (long long)(240 * x + 46 * y));
    if (g != 2 || 240 * x + 46 * y != 2) fails++;

    fprintf(o, "2^100 mod 1e9+7=%llu 3^(p-1) mod p=%llu\n", (unsigned long long)powmod(2, 100, 1000000007ull),
            (unsigned long long)powmod(3, 1000000006ull, 1000000007ull));
    if (powmod(3, 1000000006ull, 1000000007ull) != 1) fails++;

    /* Miller-Rabin against the sieve for every n < 100001, then known large values */
    int mism = 0;
    for (unsigned i = 0; i <= 100000; i++) if (is_prime(i) != (i >= 2 && !comp[i])) mism++;
    fprintf(o, "miller-rabin mismatches below 100001: %d\n", mism);
    if (mism) fails++;
    static const struct { uint64_t n; int prime; } big[] = {
        {2305843009213693951ull, 1},      /* 2^61 - 1, Mersenne prime */
        {18446744073709551557ull, 1},     /* largest prime below 2^64 */
        {18446744073709551615ull, 0},     /* 2^64 - 1 */
        {1000000007ull * 998244353ull, 0},
        {3825123056546413051ull, 0},      /* strong pseudoprime to bases 2..23 */
        {4294967297ull, 0},               /* 2^32 + 1 = 641 * 6700417 */
    };
    for (unsigned i = 0; i < sizeof big / sizeof *big; i++) {
        int p = is_prime(big[i].n);
        fprintf(o, "is_prime(%llu)=%d\n", (unsigned long long)big[i].n, p);
        if (p != big[i].prime) fails++;
    }

    /* 128-bit arithmetic through compiler-runtime helpers */
    u128 f = 1;
    for (unsigned i = 2; i <= 34; i++) f *= i;
    char dec[48];
    int len = 0;
    u128 t = f;
    while (t) { dec[len++] = (char)('0' + (int)(t % 10)); t /= 10; }
    for (int i = 0; i < len / 2; i++) { char c = dec[i]; dec[i] = dec[len - 1 - i]; dec[len - 1 - i] = c; }
    dec[len] = 0;
    fprintf(o, "34! = %s\n", dec);
    if (strcmp(dec, "295232799039604140847618609643520000000")) fails++;
    __int128 neg = -(__int128)1 << 100;
    fprintf(o, "-(2^100)/12345 = %lld rem %lld\n", (long long)(neg / 12345), (long long)(neg % 12345));

    fprintf(o, "isqrt: %llu %llu %llu %llu\n", (unsigned long long)isqrt(99), (unsigned long long)isqrt(1000000),
            (unsigned long long)isqrt(0xFFFFFFFFFFFFFFFFull), (unsigned long long)isqrt(4611686014132420609ull));
    if (isqrt(0xFFFFFFFFFFFFFFFFull) != 4294967295ull) fails++;

    unsigned best = 0, besti = 0;
    for (unsigned i = 1; i < 30000; i++) {
        uint64_t n = i;
        unsigned steps = 0;
        while (n != 1) { n = n & 1 ? 3 * n + 1 : n / 2; steps++; }
        if (steps > best) { best = steps; besti = i; }
    }
    fprintf(o, "collatz longest below 30000: n=%u steps=%u\n", besti, best);

    fprintf(o, "perfect numbers below 10000:");
    for (unsigned n = 2; n < 10000; n++) {
        unsigned s = 1;
        for (unsigned d = 2; d * d <= n; d++) if (n % d == 0) { s += d; if (d != n / d) s += n / d; }
        if (s == n) fprintf(o, " %u", n);
    }
    fprintf(o, "\nphi: %u %u %u %u sum(phi 1..1000)=", phi(1), phi(36), phi(97), phi(1000000));
    unsigned sp = 0;
    for (unsigned i = 1; i <= 1000; i++) sp += phi(i);
    fprintf(o, "%u\n", sp);
    if (sp != 304192) fails++;

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
