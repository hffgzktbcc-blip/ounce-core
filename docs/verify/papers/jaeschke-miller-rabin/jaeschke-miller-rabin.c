// Smallest strong pseudoprimes to the first k prime bases, from G. Jaeschke, "On strong pseudoprimes to several bases",
// Mathematics of Computation 61 (1993) 915-926 (the numbers psi_k of that paper, listed as OEIS A014233: the smallest odd
// composite n for which the strong-pseudoprime (Miller-Rabin) test to the bases of the first k primes does not reveal compositeness).
//   psi_1 = 2047, psi_2 = 1373653, psi_3 = 25326001, psi_4 = 3215031751, psi_5 = 2152302898747, psi_6 = 3474749660383,
//   psi_7 = psi_8 = 341550071728321, psi_9 = psi_10 = psi_11 = 3825123056546413051.
// Written from the definition of a strong pseudoprime (n - 1 = d 2^s, d odd; base a passes if a^d = 1 or a^(d 2^r) = -1 mod n for some
// 0 <= r < s). Own Montgomery multiplication modulo odd n < 2^63 for the large numbers, plain modular arithmetic for the search.
// Checks:
//   1. exhaustive search over all odd n below 1373653: the least odd composite strong pseudoprime to base 2 is 2047, and the least
//      one to bases 2 and 3 is 1373653;
//   2. for each k = 1..11, psi_k is a strong pseudoprime to each of the first k primes, and (where the published sequence
//      increases, i.e. psi_(k+1) > psi_k) psi_k is detected as composite by the next prime base, so it is not a pseudoprime for k+1;
//      for psi_7 the next base 19 does not detect it (psi_8 = psi_7) but base 23 does.
// Exit 0 only if everything matches.
#include "ounce_paper.h"

typedef unsigned __int128 u128;

#ifndef MR_ACCEPT_MINUS_ONE
#define MR_ACCEPT_MINUS_ONE 1      // the "-1" branch of the strong test
#endif

static const u32 PRIMES[12] = {2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37};
static const u64 PSI[11] = {2047ULL, 1373653ULL, 25326001ULL, 3215031751ULL, 2152302898747ULL, 3474749660383ULL,
    341550071728321ULL, 341550071728321ULL, 3825123056546413051ULL, 3825123056546413051ULL, 3825123056546413051ULL};

// ---- Montgomery arithmetic, n odd, n < 2^63 ----
typedef struct { u64 n, ninv_neg, one, r2; } mont_t;
static u64 mred(const mont_t *m, u64 hi, u64 lo) {
    u64 q = lo * m->ninv_neg;
    u128 p = (u128)q * m->n;
    u64 r = hi + (u64)(p >> 64) + (lo != 0);
    return r >= m->n ? r - m->n : r;
}
static u64 mmul(const mont_t *m, u64 a, u64 b) { u128 t = (u128)a * b; return mred(m, (u64)(t >> 64), (u64)t); }
static void minit(mont_t *m, u64 n) {
    m->n = n;
    u64 inv = n;                                   // Newton: inv = n^-1 mod 2^64
    for (int i = 0; i < 6; i++) inv *= 2 - n * inv;
    m->ninv_neg = (u64)0 - inv;
    m->one = ((u64)0 - n) % n;                     // 2^64 mod n
    u64 r2 = m->one;
    for (int i = 0; i < 64; i++) { r2 <<= 1; if (r2 >= n) r2 -= n; }   // times 2^64 -> R^2 mod n
    m->r2 = r2;
}

// strong test of odd n > 2 to base a (a < n); returns 1 if "probably prime"
static int strong_mont(u64 n, u64 a) {
    mont_t m; minit(&m, n);
    u64 d = n - 1; int s = 0;
    while ((d & 1) == 0) { d >>= 1; s++; }
    u64 am = mmul(&m, a % n, m.r2);
    if (a % n == 0) return 1;
    u64 x = m.one;
    for (int i = 63 - __builtin_clzll(d); i >= 0; i--) {
        x = mmul(&m, x, x);
        if ((d >> i) & 1) x = mmul(&m, x, am);
    }
    u64 minus1 = n - m.one;
    if (x == m.one) return 1;
    if (MR_ACCEPT_MINUS_ONE && x == minus1) return 1;
    for (int r = 1; r < s; r++) {
        x = mmul(&m, x, x);
        if (MR_ACCEPT_MINUS_ONE && x == minus1) return 1;
    }
    return 0;
}

// ---- plain arithmetic for n < 2^32 ----
static int strong_plain(u64 n, u64 a) {
    u64 d = n - 1; int s = 0;
    while ((d & 1) == 0) { d >>= 1; s++; }
    u64 x = 1, b = a % n;
    for (u64 e = d; e; e >>= 1) { if (e & 1) x = x * b % n; b = b * b % n; }
    if (x == 1) return 1;
    if (MR_ACCEPT_MINUS_ONE && x == n - 1) return 1;
    for (int r = 1; r < s; r++) { x = x * x % n; if (MR_ACCEPT_MINUS_ONE && x == n - 1) return 1; }
    return 0;
}

#define SLIM 1373654UL
static u8 comp[SLIM / 2 + 1];

int paper_main(void) {
    int ok = 1;
    // 1. exhaustive search below psi_2
    for (u64 i = 1; i <= SLIM / 2; i++) {
        if (comp[i]) continue;
        u64 p = 2 * i + 1;
        for (u64 j = (p * p - 1) / 2; j <= SLIM / 2; j += p) comp[j] = 1;
    }
    u64 least1 = 0, least2 = 0;
    for (u64 n = 3; n <= 1373653ULL; n += 2) {
        if (!comp[(n - 1) / 2]) continue;
        if (!strong_plain(n, 2)) continue;
        if (!least1) least1 = n;
        if (strong_plain(n, 3)) { least2 = n; break; }
    }
    int g1 = least1 == 2047, g2 = least2 == 1373653;
    ok &= g1 & g2;
    pstr(g1 ? "PASS " : "FAIL "); pstr("least odd composite strong pseudoprime to base 2 (exhaustive search): "); pdec((i64)least1); pstr(" (psi_1 = 2047)\n");
    pstr(g2 ? "PASS " : "FAIL "); pstr("least odd composite strong pseudoprime to bases 2 and 3 (exhaustive search): "); pdec((i64)least2); pstr(" (psi_2 = 1373653)\n");
    // 2. the published numbers
    for (int k = 1; k <= 11; k++) {
        u64 n = PSI[k - 1];
        int passes = 1;
        for (int i = 0; i < k; i++) if (!strong_mont(n, PRIMES[i])) passes = 0;
        // cross-check Montgomery against plain arithmetic where n < 2^32
        if (n < 4294967296ULL) for (int i = 0; i < k; i++) if (strong_plain(n, PRIMES[i]) != strong_mont(n, PRIMES[i])) passes = 0;
        int detected = 1; u32 witness = 0;
        if (k < 11) {
            // the first prime base among the next ones that rejects n
            detected = 0;
            for (int i = k; i < 12; i++) if (!strong_mont(n, PRIMES[i])) { detected = 1; witness = PRIMES[i]; break; }
        } else {
            for (int i = 11; i < 12; i++) if (!strong_mont(n, PRIMES[i])) { witness = PRIMES[i]; }
            detected = witness != 0;
        }
        // first rejecting base must be the (k+1)-th prime unless psi_(k+1) = psi_k
        int expect_next = (k < 11 && PSI[k] != n);
        int good = passes && detected;
        if (expect_next) good = good && witness == PRIMES[k];
        ok &= good;
        pstr(good ? "PASS " : "FAIL "); pstr("psi_"); pdec(k); pstr(" = "); pdec((i64)n);
        pstr(": strong pseudoprime to the first "); pdec(k); pstr(" primes "); pstr(passes ? "yes" : "NO");
        pstr(", first rejecting base "); pdec(witness); pstr("\n");
    }
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
