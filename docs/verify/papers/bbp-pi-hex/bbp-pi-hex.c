// BBP digit-extraction algorithm for pi (Bailey, Borwein, Plouffe 1997; stated step by step in Bailey, "The BBP Algorithm
// for Pi", 2006). Hex digits of pi after the first d hex digits come from {16^d pi} = {4{16^d S1} - 2{16^d S4} - {16^d S5}
// - {16^d S6}} with S_j = sum_k 1/(16^k (8k+j)); {16^d S_j} is summed with 16^(d-k) mod (8k+j) done exactly in integers and
// the fractions in IEEE double. Written from that description, no third-party code.
// Checks (tolerances fixed before the first run):
//   1. d = 1,000,000: the four values {16^d S_j} and {16^d pi} printed in the paper agree to 1e-9 (absolute), and the first
//      8 hex digits are 6C65E52C (the paper: "6C65E52CB459350050E4BB1 ... beginning at position 1,000,001"; its Table 1 row
//      for position 10^6 reads 26C65E52CB4593, the same digits shifted by one).
//   2. For ten positions d = 0 .. 19000, the first 8 hex digits equal the hexadecimal expansion of pi in OEIS A062964.
// Exit 0 only if everything matches.
#include "ounce_paper.h"

#ifndef BBP_COEF4
#define BBP_COEF4 4          // the coefficient of S1 in the BBP formula
#endif

static u64 pow16mod(u64 e, u64 m) {      // 16^e mod m, m < 2^24 so products fit in 64 bits
    u64 r = 1 % m, b = 16 % m;
    while (e) { if (e & 1) r = r * b % m; b = b * b % m; e >>= 1; }
    return r;
}

// {16^d * S_j}
static double series(u64 d, u64 j) {
    double s = 0.0;
    for (u64 k = 0; k <= d; k++) {
        u64 m = 8 * k + j;
        s += (double)pow16mod(d - k, m) / (double)m;
        if (s >= 1.0) s -= 1.0;
    }
    double t = 1.0 / 16.0;               // tail k > d: 16^(d-k)/(8k+j)
    for (u64 k = d + 1; k < d + 20; k++) {
        s += t / (double)(8 * k + j);
        t /= 16.0;
    }
    while (s >= 1.0) s -= 1.0;
    return s;
}

static double frac(double x) { while (x < 0.0) x += 1.0; while (x >= 1.0) x -= 1.0; return x; }

typedef struct { double s1, s4, s5, s6, x; } bbp_t;
static bbp_t bbp(u64 d) {
    bbp_t r;
    r.s1 = series(d, 1); r.s4 = series(d, 4); r.s5 = series(d, 5); r.s6 = series(d, 6);
    r.x = frac(BBP_COEF4 * r.s1 - 2.0 * r.s4 - r.s5 - r.s6);
    return r;
}

static void hexdigits(double x, int n, char *out) {
    for (int i = 0; i < n; i++) { x *= 16.0; int dg = (int)x; x -= dg; out[i] = "0123456789ABCDEF"[dg]; }
    out[n] = 0;
}

static u64 scaled10(double x) { return (u64)(x * 1e10); }   // truncated to 10 decimals
static void pfrac10(u64 v) { pstr("0."); char b[10]; for (int i = 9; i >= 0; i--) { b[i] = (char)('0' + v % 10); v /= 10; } paper_write(b, 10); }
static u64 absdiff(u64 a, u64 b) { return a > b ? a - b : b - a; }

// Hex digits of pi from OEIS A062964: eight digits following the first d hex digits.
static const struct { u32 d; const char *hex; } OEIS[10] = {
    {0, "243F6A88"}, {1, "43F6A888"}, {2, "3F6A8885"}, {3, "F6A8885A"}, {10, "A308D313"},
    {100, "29B7C97C"}, {1000, "49F1C09B"}, {5000, "CAD18115"}, {10000, "8AC8FCFB"}, {19000, "E1FFBA3D"},
};

int paper_main(void) {
    int ok = 1;
    // 1. the paper's worked example at d = 1,000,000
    {
        bbp_t r = bbp(1000000);
        static const u64 W[5] = {1810395338ULL, 7760655498ULL, 3624585640ULL, 3861386739ULL, 4234297975ULL};
        const double v[5] = {r.s1, r.s4, r.s5, r.s6, r.x};
        static const char *nm[5] = {"{16^d S1}", "{16^d S4}", "{16^d S5}", "{16^d S6}", "{16^d pi}"};
        for (int i = 0; i < 5; i++) {
            u64 g = scaled10(v[i]);
            int good = absdiff(g, W[i]) <= 10;      // 1e-9 (10 units of 1e-10)
            ok &= good;
            pstr(good ? "PASS " : "FAIL "); pstr("d=1000000 "); pstr(nm[i]); pstr(" = "); pfrac10(g);
            pstr(" (paper "); pfrac10(W[i]); pstr(", tolerance 1e-9)\n");
        }
        char h[9]; hexdigits(r.x, 8, h);
        int good = 1; const char *want = "6C65E52C";
        for (int i = 0; i < 8; i++) if (h[i] != want[i]) good = 0;
        ok &= good;
        pstr(good ? "PASS " : "FAIL "); pstr("hex digits at position 1,000,001: "); pstr(h); pstr(" (paper 6C65E52C)\n");
    }
    // 2. ten positions against the hexadecimal expansion of pi
    int n = 0;
    for (int i = 0; i < 10; i++) {
        bbp_t r = bbp(OEIS[i].d);
        char h[9]; hexdigits(r.x, 8, h);
        int good = 1; for (int k = 0; k < 8; k++) if (h[k] != OEIS[i].hex[k]) good = 0;
        n += good;
        if (!good) { pstr("FAIL d="); pdec(OEIS[i].d); pstr(": "); pstr(h); pstr(" expected "); pstr(OEIS[i].hex); pstr("\n"); }
    }
    pstr("positions matching OEIS A062964 (8 hex digits each): "); pdec(n); pstr(" of 10\n");
    ok &= n == 10;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
