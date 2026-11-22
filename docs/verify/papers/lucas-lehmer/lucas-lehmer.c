// Lucas-Lehmer test for Mersenne numbers M_p = 2^p - 1 (Lucas 1878; Lehmer, "An extended theory of Lucas' functions",
// Annals of Mathematics 31, 1930). Written from the criterion: for an odd prime p, M_p is prime iff s_(p-2) = 0 mod M_p, where
// s_0 = 4 and s_(i+1) = s_i^2 - 2. The program applies it to every prime p from 3 to 1300 using its own multi-word arithmetic
// (64-bit limbs, fold of the double-width square modulo 2^p - 1) and compares the exponents that pass with the published list
// of Mersenne prime exponents (OEIS A000043, terms up to 1300). Exit 0 only if the lists are identical.
#include "ounce_paper.h"

#define PMAX 1300
#define NLIMB 21                    // 21*64 = 1344 bits >= PMAX

#ifndef LL_START
#define LL_START 4
#endif
#ifndef LL_SUB
#define LL_SUB 2
#endif

// Mersenne prime exponents <= 1300 from OEIS A000043 (terms 1..15).
static const u32 PUB[15] = {2, 3, 5, 7, 13, 17, 19, 31, 61, 89, 107, 127, 521, 607, 1279};

static int is_prime_small(u32 n) {
    if (n < 2) return 0;
    for (u32 d = 2; d * d <= n; d++) if (n % d == 0) return 0;
    return 1;
}

typedef unsigned __int128 u128;

// s = (s*s - sub) mod (2^p - 1), s has n limbs, s < 2^p - 1 on entry and exit.
static void sqr_sub_mod(u64 *s, int n, u32 p, u64 sub) {
    u64 t[2 * NLIMB + 2];
    for (int i = 0; i < 2 * n + 2; i++) t[i] = 0;
    // schoolbook square: off-diagonal products once, doubled, plus the diagonal
    for (int i = 0; i < n; i++) {
        u64 carry = 0;
        for (int j = i + 1; j < n; j++) {
            u128 v = (u128)s[i] * s[j] + t[i + j] + carry;
            t[i + j] = (u64)v; carry = (u64)(v >> 64);
        }
        t[i + n] = carry;
    }
    u64 c = 0;
    for (int i = 0; i < 2 * n; i++) { u64 nc = t[i] >> 63; t[i] = (t[i] << 1) | c; c = nc; }
    t[2 * n] = c;
    u64 carry = 0;
    for (int i = 0; i < n; i++) {
        u128 v = (u128)s[i] * s[i] + t[2 * i] + carry;
        t[2 * i] = (u64)v;
        u128 w = (v >> 64) + t[2 * i + 1];
        t[2 * i + 1] = (u64)w; carry = (u64)(w >> 64);
    }
    t[2 * n] += carry;
    // t = hi * 2^p + lo  ->  lo + hi (mod 2^p - 1)
    u32 wl = p / 64, bl = p % 64;
    u64 lo[NLIMB + 1], hi[NLIMB + 2];
    for (int i = 0; i <= n; i++) lo[i] = 0;
    for (int i = 0; i < n + 2; i++) hi[i] = 0;
    for (u32 i = 0; i < wl; i++) lo[i] = t[i];
    if (bl) lo[wl] = t[wl] & (((u64)1 << bl) - 1);
    for (int i = 0; i < n + 2 && (int)(wl + i) < 2 * n + 1; i++) {
        u64 a = t[wl + i];
        u64 b = (wl + i + 1 < (u32)(2 * n + 1)) ? t[wl + i + 1] : 0;
        hi[i] = bl ? (a >> bl) | (b << (64 - bl)) : a;
    }
    // sum = lo + hi, then fold overflow above bit p twice
    u64 r[NLIMB + 2];
    carry = 0;
    for (int i = 0; i < n + 2; i++) {
        u64 a = i <= n ? lo[i] : 0;
        u128 v = (u128)a + hi[i] + carry;
        r[i] = (u64)v; carry = (u64)(v >> 64);
    }
    for (int pass = 0; pass < 2; pass++) {
        // extract bits >= p from r
        u64 over[NLIMB + 2];
        for (int i = 0; i < n + 2; i++) {
            u64 a = (wl + i < (u32)(n + 2)) ? r[wl + i] : 0;
            u64 b = (wl + i + 1 < (u32)(n + 2)) ? r[wl + i + 1] : 0;
            over[i] = bl ? (a >> bl) | (b << (64 - bl)) : a;
        }
        // clear bits >= p in r
        for (int i = (int)wl + (bl ? 1 : 0); i < n + 2; i++) r[i] = 0;
        if (bl) r[wl] &= ((u64)1 << bl) - 1;
        carry = 0;
        for (int i = 0; i < n + 2; i++) {
            u128 v = (u128)r[i] + over[i] + carry;
            r[i] = (u64)v; carry = (u64)(v >> 64);
        }
    }
    // r may equal 2^p - 1 exactly: reduce to 0
    int allones = 1;
    for (u32 i = 0; i < wl; i++) if (r[i] != ~(u64)0) allones = 0;
    if (bl) { if (r[wl] != (((u64)1 << bl) - 1)) allones = 0; }
    if (allones) for (int i = 0; i < n + 2; i++) r[i] = 0;
    // subtract `sub` modulo 2^p - 1: if r >= sub, r -= sub; else r = r + (2^p - 1) - sub
    int ge = 1;
    for (int i = n + 1; i >= 1; i--) if (r[i]) { ge = 1; break; }
    int big = 0; for (int i = 1; i < n + 2; i++) if (r[i]) big = 1;
    if (!big && r[0] < sub) ge = 0;
    if (ge) {
        u64 br = sub;
        for (int i = 0; i < n + 2; i++) { u64 a = r[i]; r[i] = a - br; br = a < br; }
    } else {
        // r + 2^p - 1 - sub : add all-ones mask (2^p - 1) then subtract sub
        u64 m[NLIMB + 2];
        for (int i = 0; i < n + 2; i++) m[i] = 0;
        for (u32 i = 0; i < wl; i++) m[i] = ~(u64)0;
        if (bl) m[wl] = ((u64)1 << bl) - 1;
        carry = 0;
        for (int i = 0; i < n + 2; i++) { u128 v = (u128)r[i] + m[i] + carry; r[i] = (u64)v; carry = (u64)(v >> 64); }
        u64 br = sub;
        for (int i = 0; i < n + 2; i++) { u64 a = r[i]; r[i] = a - br; br = a < br; }
    }
    for (int i = 0; i < n; i++) s[i] = r[i];
}

static int lucas_lehmer(u32 p) {
    int n = (int)((p + 63) / 64);
    u64 s[NLIMB];
    for (int i = 0; i < n; i++) s[i] = 0;
    s[0] = LL_START;
    for (u32 i = 0; i < p - 2; i++) sqr_sub_mod(s, n, p, LL_SUB);
    for (int i = 0; i < n; i++) if (s[i]) return 0;
    return 1;
}

int paper_main(void) {
    int found = 0, tested = 0, ok = 1;
    u32 got[32];
    pstr("Mersenne exponents found by Lucas-Lehmer for odd primes p <= 1300:");
    // p = 2 is the base case the test excludes: M_2 = 3 is prime.
    got[found++] = 2; pstr(" 2");
    for (u32 p = 3; p <= PMAX; p++) {
        if (!is_prime_small(p)) continue;
        tested++;
        if (lucas_lehmer(p)) { if (found < 32) got[found] = p; found++; pstr(" "); pdec(p); }
    }
    pstr("\n");
    pstr("odd primes tested: "); pdec(tested); pstr("\n");
    pstr("exponents found: "); pdec(found); pstr(", published (OEIS A000043, <= 1300): 15\n");
    int match = found == 15;
    if (match) for (int i = 0; i < 15; i++) if (got[i] != PUB[i]) match = 0;
    pstr(match ? "PASS exponent list identical to A000043\n" : "FAIL exponent list differs from A000043\n");
    ok &= match;
    return ok ? 0 : 1;
}
