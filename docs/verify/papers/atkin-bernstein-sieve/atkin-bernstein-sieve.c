// Sieve of Atkin: A. O. L. Atkin and D. J. Bernstein, "Prime sieves using binary quadratic forms", Mathematics of Computation 73
// (2004) 1023-1030. Written from the paper's characterization (Section 1): a squarefree integer p > 3 is prime iff the number of
// positive solutions (x, y) of 4x^2 + y^2 = p (p = 1 mod 4), of 3x^2 + y^2 = p (p = 7 mod 12) or of 3x^2 - y^2 = p with x > y
// (p = 11 mod 12) is odd. The program flips one bit per solution of each form up to N = 10^7, removes the multiples of the squares of
// the surviving numbers up to sqrt(N), adds the primes 2 and 3, and counts the primes not exceeding 10^k for k = 1..7. The counts
// are compared with the published values of pi(10^k) (OEIS A006880). Exit 0 only if all seven counts match.
#include "ounce_paper.h"

#define N 10000000UL
#ifndef AB_SQFREE
#define AB_SQFREE 1     // remove non-squarefree numbers
#endif

static u64 bits[N / 64 + 1];
#define FLIP(n) (bits[(n) >> 6] ^= (u64)1 << ((n) & 63))
#define GET(n) ((bits[(n) >> 6] >> ((n) & 63)) & 1)

// pi(10^k), k = 1..7, from OEIS A006880
static const u64 PI[7] = {4, 25, 168, 1229, 9592, 78498, 664579};

int paper_main(void) {
    // 4x^2 + y^2 = n, n mod 12 in {1, 5}
    for (u64 x = 1; 4 * x * x < N; x++) {
        u64 a = 4 * x * x;
        for (u64 y = 1; a + y * y <= N; y++) {
            u64 n = a + y * y, r = n % 12;
            if (r == 1 || r == 5) FLIP(n);
        }
    }
    // 3x^2 + y^2 = n, n mod 12 = 7
    for (u64 x = 1; 3 * x * x < N; x++) {
        u64 a = 3 * x * x;
        for (u64 y = 1; a + y * y <= N; y++) {
            u64 n = a + y * y;
            if (n % 12 == 7) FLIP(n);
        }
    }
    // 3x^2 - y^2 = n with x > y > 0, n mod 12 = 11
    for (u64 x = 2; 2 * x * x + 2 * x - 1 <= N; x++) {
        u64 a = 3 * x * x;
        for (u64 y = x - 1; y >= 1; y--) {
            u64 n = a - y * y;
            if (n > N) break;
            if (n % 12 == 11) FLIP(n);
        }
    }
    if (AB_SQFREE) {
        for (u64 n = 5; n * n <= N; n++) {
            if (!GET(n)) continue;
            u64 s = n * n;
            for (u64 k = s; k <= N; k += s) if (GET(k)) FLIP(k);
        }
    }
    // count the primes up to 10^k: 2, 3 and every n > 4 left in the sieve
    int ok = 1, matched = 0;
    u64 count = 2, limit = 10, k = 1;
    for (u64 n = 5; n <= N; n++) {
        while (n > limit) {                         // 10^k reached: all n <= limit are counted
            u64 c = count;
            int good = c == PI[k - 1];
            matched += good; ok &= good;
            pstr(good ? "PASS " : "FAIL "); pstr("pi(10^"); pdec((i64)k); pstr(") = "); pdec((i64)c); pstr(" (published "); pdec((i64)PI[k - 1]); pstr(")\n");
            limit *= 10; k++;
        }
        if (GET(n)) count++;
    }
    {
        int good = count == PI[k - 1];
        matched += good; ok &= good;
        pstr(good ? "PASS " : "FAIL "); pstr("pi(10^"); pdec((i64)k); pstr(") = "); pdec((i64)count); pstr(" (published "); pdec((i64)PI[k - 1]); pstr(")\n");
    }
    pstr("counts matching: "); pdec(matched); pstr(" of 7\n");
    pstr(ok && matched == 7 ? "PASS\n" : "FAIL\n");
    return ok && matched == 7 ? 0 : 1;
}
