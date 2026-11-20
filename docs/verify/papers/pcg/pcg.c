// PCG (O'Neill, "PCG: A Family of Simple Fast Space-Efficient Statistically Good Algorithms for Random Number Generation",
// Harvey Mudd College technical report HMC-CS-2014-0905, 2014), written from the paper: a linear congruential generator with a
// permuted output. Two members are run: PCG-XSH-RR 64/32 (pcg32) and PCG-XSL-RR 128/64 (pcg64), each seeded with (42, 54) as in the
// authors' reference code, with the sequence selector as the increment, bounded numbers by rejection, and LCG jump-ahead.
// Checks the authors' published test output for each generator, 5 rounds of: 6 raw values, the same 6 values again after
// advancing by -6, 65 coin tosses, 33 dice rolls and a 52-card shuffle (810 values per generator, 1620 in all), compared as text.
// The expected output is data copied as text; none of the authors' code was copied. Exit 0 only if every line matches.
#include "ounce_paper.h"

static const char *const WANT32[5][5] = {
    {"0xa15c02b7 0x7b47f409 0xba1d3330 0x83d2f293 0xbfa4784b 0xcbed606e", "0xa15c02b7 0x7b47f409 0xba1d3330 0x83d2f293 0xbfa4784b 0xcbed606e", "HHTTTHTHHHTHTTTHHHHHTTTHHHTHTHTHTTHTTTHHHHHHTTTTHHTTTTTHTTTTTTTHT", "3 4 1 1 2 2 3 2 4 3 2 4 3 3 5 2 3 1 3 1 5 1 4 1 5 6 4 6 6 2 6 3 3", "Qd Ks 6d 3s 3d 4c 3h Td Kc 5c Jh Kd Jd As 4s 4h Ad Th Ac Jc 7s Qs 2s 7h Kh 2d 6c Ah 4d Qh 9h 6s 5s 2c 9c Ts 8d 9s 3c 8c Js 5d 2h 6h 7d 8s 9d 5h 8h Qc 7c Tc"},
    {"0x74ab93ad 0x1c1da000 0x494ff896 0x34462f2f 0xd308a3e5 0x0fa83bab", "0x74ab93ad 0x1c1da000 0x494ff896 0x34462f2f 0xd308a3e5 0x0fa83bab", "HHHHHHHHHHTHHHTHTHTHTHTTTTHHTTTHHTHHTHTTHHTTTHHHHHHTHTTHTHTTTTTTT", "5 1 1 3 3 2 4 5 3 2 2 6 4 3 2 4 2 4 3 2 3 6 3 2 3 4 2 4 1 1 5 4 4", "7d 2s 7h Td 8s 3c 3d Js 2d Tc 4h Qs 5c 9c Th 2c Jc Qd 9d Qc 7s 3s 5s 6h 4d Jh 4c Ac 4s 5h 5d Kc 8h 8d Jd 9s Ad 6s 6c Kd 2h 3h Kh Ts Qh 9h 6d As 7c Ks Ah 8c"},
    {"0x39af5f9f 0x04196b18 0xc3c3eb28 0xc076c60c 0xc693e135 0xf8f63932", "0x39af5f9f 0x04196b18 0xc3c3eb28 0xc076c60c 0xc693e135 0xf8f63932", "HTTHHTTTTTHTTHHHTHTTHHTTHTHHTHTHTTTTHHTTTHHTHHTTHTTHHHTHHHTHTTTHT", "5 1 5 3 2 2 4 5 3 3 1 3 4 6 3 2 3 4 2 2 3 1 5 2 4 6 6 4 2 4 3 3 6", "Kd Jh Kc Qh 4d Qc 4h 9d 3c Kh Qs 8h 5c Jd 7d 8d 3h 7c 8s 3s 2h Ks 9c 9h 2c 8c Ad 7s 4s 2s 5h 6s 4c Ah 7h 5s Ac 3d 5d Qd As Tc 6h 9s 2d 6c 6d Td Jc Ts Th Js"},
    {"0x55ce6851 0x97a7726d 0x17e10815 0x58007d43 0x962fb148 0xb9bb55bd", "0x55ce6851 0x97a7726d 0x17e10815 0x58007d43 0x962fb148 0xb9bb55bd", "HHTHHTTTTHTHHHHHTTHHHTTTHHTHTHTHTHHTTHTHHHHHHTHHTHHTHHTTTTHHTHHTT", "6 6 3 2 3 4 2 6 4 2 6 3 2 3 5 5 3 4 4 6 6 2 6 5 4 4 6 1 6 1 3 6 5", "Qd 8h 5d 8s 8d Ts 7h Th Qs Js 7s Kc 6h 5s 4d Ac Jd 7d 7c Td 2c 6s 5h 6d 3s Kd 9s Jh Kh As Ah 9h 3c Qh 9c 2d Tc 9d 2s 3d Ks 4h Qc Ad Jc 8c 2h 3h 4s 4c 5c 6c"},
    {"0xfcef7cd6 0x1b488b5a 0xd0daf7ea 0x1d9a70f7 0x241a37cf 0x9a3857b7", "0xfcef7cd6 0x1b488b5a 0xd0daf7ea 0x1d9a70f7 0x241a37cf 0x9a3857b7", "HHHHTHHTTHTTHHHTTTHHTHTHTTTTHTTHTHTTTHHHTHTHTTHTTHTHHTHTHHHTHTHTT", "5 4 1 2 6 1 3 1 5 6 3 6 2 1 4 4 5 2 1 5 6 5 6 4 4 4 5 2 6 4 3 5 6", "4d 9s Qc 9h As Qs 7s 4c Kd 6h 6s 2c 8c 5d 7h 5h Jc 3s 7c Jh Js Ks Tc Jd Kc Th 3h Ts Qh Ad Td 3c Ah 2d 3d 5c Ac 8s 5s 9c 2h 6c 6d Kh Qd 8d 7d 2s 8h 4h 9d 4s"},
};
static const char *const WANT64[5][5] = {
    {"0x86b1da1d72062b68 0x1304aa46c9853d39 0xa3670e9e0dd50358 0xf9090e529a7dae00 0xc85b9fd837996f2c 0x606121f8e3919196", "0x86b1da1d72062b68 0x1304aa46c9853d39 0xa3670e9e0dd50358 0xf9090e529a7dae00 0xc85b9fd837996f2c 0x606121f8e3919196", "TTTHHHTTTHHHTTTTHHTTHHTHTHTTHHTHTTTTHHTTTHTHHTHTTTTHHTTTHHHTTTHTT", "6 4 1 5 1 5 5 3 6 3 4 6 2 3 6 5 5 5 1 5 3 6 2 6 1 4 4 3 5 2 6 3 2", "3d 7d 3h Qd 9d 8c Ts Ad 9s 6c Jh Ac 5s 4c 2c 7s Kh Kd 7h Qh 6d Qc 8d Qs 6s Js 4d Kc 9h 3c 2h Td 5d 5h 9c 4s 5c 7c 3s 4h As Th 6h Jc 2s Jd Tc Ah 2d Ks 8h 8s"},
    {"0x1773ba241e7a792a 0xe41aed7117b0bc10 0x36bac8d9432af525 0xe0c78e2f3c850a38 0xe3ad939c1c7ce70d 0xa302fdced8c79e93", "0x1773ba241e7a792a 0xe41aed7117b0bc10 0x36bac8d9432af525 0xe0c78e2f3c850a38 0xe3ad939c1c7ce70d 0xa302fdced8c79e93", "TTTTHTHTHHTHTHTTTTTHHTTHHHHTHTHHHHHHHTHHHTHHTHTTTHHHHTTHHTTTHTHTH", "6 1 1 5 4 1 5 6 3 2 4 2 2 4 6 2 1 5 2 6 2 3 1 5 1 1 5 4 4 2 3 6 3", "As 2h 4d 7d Ad Qc 9s 7h Kh Jc 7c 3d 8c Th 9c Qd 9h Td 6d 8d Qs 5c 6s 8s Ac Kd 2d 3h Qh Tc Jh Ah 3s 4h 9d 8h Jd 4s 2s Ts 5s Kc 4c 5d 3c 6h 2c 6c 7s Js 5h Ks"},
    {"0xc96006593aed3b62 0xf04d5afa3f197bf1 0xce6f729cc913a50f 0x98b5fc4fbb1e4aea 0x802dce1b410fc8c3 0xe3bac0a14f6e5033", "0xc96006593aed3b62 0xf04d5afa3f197bf1 0xce6f729cc913a50f 0x98b5fc4fbb1e4aea 0x802dce1b410fc8c3 0xe3bac0a14f6e5033", "HTTHTHTTTTTHTTTHHTHTHHTHHHHHHHHHTTTHTHTHTHHTTTTTTHHHHTHTTTTHHHHHH", "5 6 4 3 3 1 4 5 2 3 2 1 1 3 2 3 4 5 4 6 4 3 6 2 2 6 3 2 2 4 5 2 5", "5c 5d 9d 4s Qs Kh 2c 3h Ac 2s 7s 4c 6s 8h 9c 6d 2h 4d 3c 5h 6h Ad 7c Js Jd 6c 2d 3d 4h Kd 9s Th Kc 7h 8s Tc Qc Qd Jh Ks 8d Ts Ah Jc 5s As Qh 8c 3s Td 7d 9h"},
    {"0x68da679de81de48a 0x7ee3c031fa0aa440 0x6eb1663983530403 0xfec4d7a9a7aec823 0xbce221c255ee9467 0x460a42a962b8a2f9", "0x68da679de81de48a 0x7ee3c031fa0aa440 0x6eb1663983530403 0xfec4d7a9a7aec823 0xbce221c255ee9467 0x460a42a962b8a2f9", "HHHTTTTHHHHHTTTTTTTHHHTHHHHTTHTTTHTTTTHTHHHHTHHTTTHHHTHHTTHHHTHTH", "3 5 6 3 6 4 5 6 5 6 1 1 6 6 5 5 5 1 6 4 6 4 5 1 1 4 4 4 3 5 6 1 6", "7c Kh 2d Qc Jh Js Kc Ks Kd 3d 8d 4s Jc 8c 9d 5c 9c Qh As Qd 3s Ac 3h 3c Ad 9h 6h Th Jd 5s 6s 7h 7d 7s 2c 2h 2s 6d 8h 4d Ts Tc 4h 5h 4c Ah 9s Td 8s 5d 6c Qs"},
    {"0x9e0d084cff42fe2f 0x63cd8347aae338ea 0x112aae00540d3fa1 0x53968bc829afd6ec 0x1b9900eb6c5b6d90 0xe89ed17ea33cb420", "0x9e0d084cff42fe2f 0x63cd8347aae338ea 0x112aae00540d3fa1 0x53968bc829afd6ec 0x1b9900eb6c5b6d90 0xe89ed17ea33cb420", "HTTTTTHTHTHHHTHTTTHTHHTHHTHTTTHHTTHHHTTTTHTTHHTHHTHHHTTHHTHTHHHHH", "6 6 5 1 1 4 5 5 3 1 2 6 5 2 4 6 4 2 6 4 4 3 2 5 3 3 6 5 3 4 5 1 2", "Jd Qh 8s 9h Kh 3c Ts Th Kc Kd 4s Ah 5h 4d Jc 7d 9c Ac 8c Ks 6s 2d Td Qc 2s 8h Tc 6c 3d 3h 4h 6h 7s Qs As 5d 3s 5c 6d 4c Js 5s 8d 9d 2c 9s 7h Qd Jh Ad 2h 7c"},
};

#ifndef PCG32_XS
#define PCG32_XS 18       // xorshift amount of the XSH-RR output function (the paper's pcg32 uses 18 and 27)
#endif
#ifndef PCG64_ROT
#define PCG64_ROT 122     // the top 6 bits of the 128-bit state choose the rotation
#endif
typedef unsigned __int128 u128;
#define M32 6364136223846793005ULL
static const u128 M128 = ((u128)2549297995355413924ULL << 64) | 4865540595714422341ULL;

// Brown's LCG jump: state after `delta` steps of s -> mult * s + plus (mod 2^N); delta may be a negative step count mod 2^N.
static u64 adv32(u64 s, u64 delta, u64 mult, u64 plus) {
    u64 am = 1, ap = 0;
    while (delta) {
        if (delta & 1) { am *= mult; ap = ap * mult + plus; }
        plus = (mult + 1) * plus; mult *= mult; delta >>= 1;
    }
    return am * s + ap;
}
static u128 adv128(u128 s, u128 delta, u128 mult, u128 plus) {
    u128 am = 1, ap = 0;
    while (delta) {
        if (delta & 1) { am *= mult; ap = ap * mult + plus; }
        plus = (mult + 1) * plus; mult *= mult; delta >>= 1;
    }
    return am * s + ap;
}
typedef struct { u64 state, inc; } P32;
typedef struct { u128 state, inc; } P64;
static u32 p32_next(P32 *g) {
    u64 old = g->state;
    g->state = old * M32 + g->inc;
    u32 xs = (u32)(((old >> PCG32_XS) ^ old) >> 27);
    u32 rot = (u32)(old >> 59);
    return (xs >> rot) | (xs << ((0U - rot) & 31));
}
static void p32_seed(P32 *g, u64 initstate, u64 initseq) {
    g->state = 0; g->inc = (initseq << 1) | 1;
    p32_next(g); g->state += initstate; p32_next(g);
}
static u32 p32_bounded(P32 *g, u32 bound) {
    u32 threshold = (0U - bound) % bound;
    for (;;) { u32 r = p32_next(g); if (r >= threshold) return r % bound; }
}
static u64 p64_next(P64 *g) {
    g->state = g->state * M128 + g->inc;
    u64 v = (u64)(g->state >> 64) ^ (u64)g->state;
    unsigned rot = (unsigned)(g->state >> PCG64_ROT) & 63U;
    return (v >> rot) | (v << ((0U - rot) & 63U));
}
static void p64_seed(P64 *g, u128 initstate, u128 initseq) {
    g->state = 0; g->inc = (initseq << 1) | 1;
    // the seeding steps are plain LCG steps without output
    g->state = g->state * M128 + g->inc; g->state += initstate; g->state = g->state * M128 + g->inc;
}
static u64 p64_bounded(P64 *g, u64 bound) {
    u64 threshold = (0ULL - bound) % bound;
    for (;;) { u64 r = p64_next(g); if (r >= threshold) return r % bound; }
}

// ---- tiny text builder ----
typedef struct { char b[400]; int n; } Buf;
static void bs(Buf *o, const char *s) { while (*s) o->b[o->n++] = *s++; o->b[o->n] = 0; }
static void bc(Buf *o, char c) { o->b[o->n++] = c; o->b[o->n] = 0; }
static void bhex(Buf *o, u64 v, int digits) {
    bs(o, "0x");
    for (int i = digits - 1; i >= 0; i--) bc(o, "0123456789abcdef"[(v >> (4 * i)) & 15]);
}
static void bint(Buf *o, int v) { if (v >= 10) bc(o, (char)('0' + v / 10)); bc(o, (char)('0' + v % 10)); }
static int same(const char *a, const char *b) { while (*a && *a == *b) { a++; b++; } return *a == *b; }
static void reset(Buf *o) { o->n = 0; o->b[0] = 0; }

// bounded(i): bounded random number below i from the generator under test; raw(): next raw value as hex text
static P32 g32; static P64 g64;
static int use64;
static u64 bounded(u64 n) { return use64 ? p64_bounded(&g64, n) : p32_bounded(&g32, (u32)n); }
static void raw(Buf *o) { if (use64) { bhex(o, p64_next(&g64), 16); } else { bhex(o, p32_next(&g32), 8); } }

static int check_line(const char *gen, int round, const char *what, const Buf *got, const char *want) {
    int ok = same(got->b, want);
    if (!ok) { pstr("FAIL "); pstr(gen); pstr(" round "); pdec(round); pstr(" "); pstr(what); pstr(": "); pstr(got->b); pstr(" expected "); pstr(want); pstr("\n"); }
    return ok;
}
static int run(const char *gen, int is64, const char *const (*want)[5]) {
    use64 = is64;
    if (is64) p64_seed(&g64, 42, 54); else p32_seed(&g32, 42, 54);
    int lines = 0, bad = 0;
    for (int r = 0; r < 5; r++) {
        Buf o;
        reset(&o); for (int i = 0; i < 6; i++) { if (i) bc(&o, ' '); raw(&o); }
        lines++; bad += !check_line(gen, r + 1, "values", &o, want[r][0]);
        // jump back 6 steps with the LCG jump-ahead and generate the same 6 values again
        if (is64) g64.state = adv128(g64.state, (u128)0 - 6, M128, g64.inc);
        else g32.state = adv32(g32.state, (u64)0 - 6, M32, g32.inc);
        reset(&o); for (int i = 0; i < 6; i++) { if (i) bc(&o, ' '); raw(&o); }
        lines++; bad += !check_line(gen, r + 1, "again after advance(-6)", &o, want[r][1]);
        reset(&o); for (int i = 0; i < 65; i++) bc(&o, bounded(2) ? 'H' : 'T');
        lines++; bad += !check_line(gen, r + 1, "coins", &o, want[r][2]);
        reset(&o); for (int i = 0; i < 33; i++) { if (i) bc(&o, ' '); bint(&o, (int)bounded(6) + 1); }
        lines++; bad += !check_line(gen, r + 1, "rolls", &o, want[r][3]);
        int cards[52];
        for (int i = 0; i < 52; i++) cards[i] = i;
        for (int i = 52; i > 1; i--) { int c = (int)bounded((u64)i); int t = cards[c]; cards[c] = cards[i - 1]; cards[i - 1] = t; }
        reset(&o);
        for (int i = 0; i < 52; i++) { if (i) bc(&o, ' '); bc(&o, "A23456789TJQK"[cards[i] / 4]); bc(&o, "hcds"[cards[i] % 4]); }
        lines++; bad += !check_line(gen, r + 1, "cards", &o, want[r][4]);
    }
    pstr(gen); pstr(": "); pdec(lines - bad); pstr(" of "); pdec(lines); pstr(" published lines match\n");
    return bad == 0;
}
int paper_main(void) {
    int ok = 1;
    ok &= run("pcg32 (XSH-RR 64/32)", 0, WANT32);
    ok &= run("pcg64 (XSL-RR 128/64)", 1, WANT64);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
