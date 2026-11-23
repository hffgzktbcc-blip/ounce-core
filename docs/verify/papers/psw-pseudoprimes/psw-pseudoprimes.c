// Pseudoprime census of C. Pomerance, J. L. Selfridge and S. S. Wagstaff Jr., "The pseudoprimes to 25*10^9",
// Mathematics of Computation 35 (1980) 1003-1026. For every odd composite n below 10^7 the program decides, with its own code,
// whether n is a pseudoprime to base 2 (2^(n-1) = 1 mod n), an Euler pseudoprime to base 2 (2^((n-1)/2) = (2/n) mod n, Jacobi
// symbol), a strong pseudoprime to base 2 (2^d = 1 or 2^(d 2^r) = -1 mod n for some 0 <= r < s, n-1 = d 2^s, d odd) and whether
// n is a Carmichael number (squarefree, composite, and p-1 divides n-1 for every prime p dividing n; tested on the base-2
// pseudoprimes, which contain all Carmichael numbers). It compares the counts below 10^3 ... 10^7 with the five columns of the
// paper's Table I (rows for 10^3 to 10^7), the least element in each region of the paper's Figure 1, and the worked example
// n = 2284453 = 1069 * 2137 in the text. Written from the definitions in the paper; no third-party code.
// Exit 0 only if everything matches.
#include "ounce_paper.h"

#ifndef PSW_BASE
#define PSW_BASE 2
#endif
#define LIMIT 10000000UL

static u8 comp[LIMIT / 2 + 1];      // comp[i] != 0 means 2i+1 is composite (i >= 1)

static u64 mulmod(u64 a, u64 b, u64 n) { return a * b % n; }      // n < 2^32 here

// b^e mod n for small base b < n, left to right
static u64 powbase(u64 e, u64 n, u64 b) {
    u64 x = 1;
    for (int i = 63 - __builtin_clzll(e | 1); i >= 0; i--) {
        x = mulmod(x, x, n);
        if ((e >> i) & 1) { x = x * b; x %= n; }
    }
    return x % n;
}

static int jacobi(u64 a, u64 n) {          // n odd positive
    int r = 1; a %= n;
    while (a) {
        while ((a & 1) == 0) { a >>= 1; u64 m = n & 7; if (m == 3 || m == 5) r = -r; }
        u64 t = a; a = n; n = t;
        if ((a & 3) == 3 && (n & 3) == 3) r = -r;
        a %= n;
    }
    return n == 1 ? r : 0;
}

static int carmichael(u64 n) {              // n odd composite
    u64 m = n;
    for (u64 p = 3; p * p <= m; p += 2) {
        if (m % p == 0) {
            m /= p;
            if (m % p == 0) return 0;               // not squarefree
            if ((n - 1) % (p - 1) != 0) return 0;
        }
    }
    if (m != n) { if (m > 1 && (n - 1) % (m - 1) != 0) return 0; return 1; }
    return 0;
}

// Table I of the paper: rows x = 10^3 .. 10^7; columns P2, E2, E2-S2, S2, C.
static const u32 TABLE[5][5] = {
    {3, 1, 1, 0, 1},
    {22, 12, 7, 5, 7},
    {78, 36, 20, 16, 16},
    {245, 114, 68, 46, 43},
    {750, 375, 213, 162, 105},
};

int paper_main(void) {
    int ok = 1;
    for (u64 i = 1; i <= LIMIT / 2; i++) {            // sieve odd numbers 2i+1
        if (comp[i]) continue;
        u64 p = 2 * i + 1;
        for (u64 j = (p * p - 1) / 2; j <= LIMIT / 2; j += p) comp[j] = 1;
    }
    u64 P = 0, E = 0, S = 0, C = 0;
    u64 snap[5][4]; int row = 0; u64 nextthr = 1000;
    u64 least[6] = {0, 0, 0, 0, 0, 0};            // regions A..F of Figure 1
    for (u64 n = 3; n < LIMIT; n += 2) {
        if (n > nextthr) {                         // all n < nextthr counted
            snap[row][0] = P; snap[row][1] = E; snap[row][2] = S; snap[row][3] = C; row++; nextthr *= 10;
        }
        if (!comp[(n - 1) / 2]) continue;
        u64 d = n - 1; int s = 0;
        while ((d & 1) == 0) { d >>= 1; s++; }
        u64 x = powbase(d, n, PSW_BASE);
        int strong = x == 1 || x == n - 1;
        u64 y = x;                                 // will hold base^((n-1)/2)
        for (int r = 1; r < s; r++) { x = mulmod(x, x, n); if (x == n - 1) strong = 1; }
        y = x;
        x = mulmod(x, x, n);                       // base^(n-1)
        int psp = x == 1;
        if (!psp) continue;
        // here s >= 1: y = base^((n-1)/2)
        int j = jacobi(PSW_BASE, n);
        int epsp = (j == 1 && y == 1) || (j == -1 && y == n - 1);
        int carm = carmichael(n);
        P++; if (epsp) E++; if (strong) S++; if (carm) C++;
        int reg = -1;
        if (strong) reg = carm ? 5 : 4;
        else if (epsp) reg = carm ? 3 : 2;
        else reg = carm ? 1 : 0;
        if (!least[reg]) least[reg] = n;
    }
    snap[row][0] = P; snap[row][1] = E; snap[row][2] = S; snap[row][3] = C; row++;   // x = 10^7
    pstr("Table I rows (base "); pdec(PSW_BASE); pstr("): P2, E2, E2-S2, S2, C   [paper in brackets]\n");
    int good = 0;
    for (int r = 0; r < 5; r++) {
        u64 got[5] = {snap[r][0], snap[r][1], snap[r][1] - snap[r][2], snap[r][2], snap[r][3]};
        int m = 1;
        pstr("10^"); pdec(r + 3); pstr(":");
        for (int c = 0; c < 5; c++) {
            pstr(" "); pdec((i64)got[c]); pstr(" ["); pdec(TABLE[r][c]); pstr("]");
            if (got[c] != TABLE[r][c]) m = 0; else good++;
        }
        pstr(m ? "  ok\n" : "  MISMATCH\n");
    }
    pstr("Table I entries matching: "); pdec(good); pstr(" of 25\n");
    ok &= good == 25;
    // Figure 1: least element of each region
    static const u64 FIG[6] = {341, 2821, 1905, 561, 2047, 15841};
    static const char *RN[6] = {"psp(2), not Euler, not Carmichael", "psp(2), not Euler, Carmichael",
        "Euler psp, not strong, not Carmichael", "Euler psp, not strong, Carmichael", "strong psp, not Carmichael", "strong psp, Carmichael"};
    int fg = 0;
    for (int i = 0; i < 6; i++) {
        pstr("Figure 1 least element, "); pstr(RN[i]); pstr(": "); pdec((i64)least[i]); pstr(" [paper "); pdec((i64)FIG[i]); pstr("]\n");
        if (least[i] == FIG[i]) fg++;
    }
    pstr("Figure 1 regions matching: "); pdec(fg); pstr(" of 6\n");
    ok &= fg == 6;
    // worked example: n = 2284453 = 1069 * 2137, e = (n-1)/2 = 1142226
    {
        u64 n = 2284453, e = 1142226;
        int a = n == 1069ULL * 2137ULL;
        int b = powbase(e, n, 2) == n - 1 && powbase(e, n, 7) == n - 1;   // 2^e = 7^e = -1
        int c = powbase(e, n, 14) == 1 && powbase(e / 2, n, 14) == 4275;  // 14^e = 1, 14^(e/2) = 4275
        pstr("text example n = 2284453: product "); pstr(a ? "ok" : "BAD"); pstr(", 2^e = 7^e = -1 "); pstr(b ? "ok" : "BAD");
        pstr(", 14^e = 1 and 14^(e/2) = 4275 "); pstr(c ? "ok\n" : "BAD\n");
        ok &= a && b && c;
    }
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
