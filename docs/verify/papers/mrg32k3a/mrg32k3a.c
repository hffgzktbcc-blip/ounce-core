// MRG32k3a with streams and substreams (L'Ecuyer, "Good Parameters and Implementations for Combined Multiple Recursive Random
// Number Generators", Operations Research 47(1), 1999; stream/substream package: L'Ecuyer, Simard, Chen and Kelton, "An
// Object-Oriented Random-Number Package with Many Long Streams and Substreams", Operations Research 50(6), 2002), written from the
// two recurrences x[n] = (a12 x[n-2] - a13n x[n-3]) mod m1 and y[n] = (a21 y[n-1] - a23n y[n-3]) mod m2, output
// (x - y) mod m1 scaled by 1/(m1+1), with jump-ahead by 3x3 matrix powers computed here (no jump matrices are typed in).
// The program replays the authors' published test program (test2RngStream.c: three streams, advance by 2^5+3, RandInt, increased
// precision, substreams, antithetic sums of 100000 values, 2^127 jumps forwards and backwards) and compares the whole of its
// output text line by line with the authors' published result file test2RngStream.res (including 23 printed generator-state lines,
// 13 printed U(0,1) values, an integer sum, three floating-point sums and the final sum). Floating-point steps use basic IEEE operations only.
// The expected text is data; no third-party code was copied. Exit 0 only if every line matches.
#include "ounce_paper.h"

static const char EXPECT[] =
    "Initial states of g1, g2, and g3:\n"
    "\n"
    "The current state of the Rngstream g1:\n"
    "   Cg = { 12345, 12345, 12345, 12345, 12345, 12345 }\n"
    "\n"
    "The current state of the Rngstream g2:\n"
    "   Cg = { 3692455944, 1366884236, 2968912127, 335948734, 4161675175, 475798818 }\n"
    "\n"
    "The current state of the Rngstream g3:\n"
    "   Cg = { 1015873554, 1310354410, 2249465273, 994084013, 2912484720, 3876682925 }\n"
    "\n"
    "State of g1 after advancing by 2^5 + 3 = 35 steps:\n"
    "The current state of the Rngstream g1:\n"
    "   Cg = { 1040577685, 3747037609, 277208355, 1712706441, 627445683, 2408037141 }\n"
    "\n"
    "RandU01 (g1) =   0.07941674\n"
    "\n"
    "State of g1 after reset and advancing 35 times by 1:\n"
    "The current state of the Rngstream g1:\n"
    "   Cg = { 1040577685, 3747037609, 277208355, 1712706441, 627445683, 2408037141 }\n"
    "\n"
    "RandU01 (g1) =   0.07941674\n"
    "\n"
    "State of g1 after reset and 35 calls to RandInt (1, 10):\n"
    "The current state of the Rngstream g1:\n"
    "   Cg = { 1040577685, 3747037609, 277208355, 1712706441, 627445683, 2408037141 }\n"
    "\n"
    "   sum of 35 integers in [1, 10] = 186\n"
    "\n"
    "RandU01 (g1) =   0.07941674\n"
    "\n"
    "State of g1 after reset, IncreasedPrecis (1) and 17 calls to RandInt (1, 10):\n"
    "The current state of the Rngstream g1:\n"
    "   Cg = { 2633078466, 1040577685, 3747037609, 3152585150, 1712706441, 627445683 }\n"
    "\n"
    "State of g1 after IncreasedPrecis (0) and 1 call to RandInt\n"
    "The current state of the Rngstream g1:\n"
    "   Cg = { 1040577685, 3747037609, 277208355, 1712706441, 627445683, 2408037141 }\n"
    "\n"
    "State of g1 after reset, IncreasedPrecis (1) and 17 calls to RandU01:\n"
    "The current state of the Rngstream g1:\n"
    "   Cg = { 2633078466, 1040577685, 3747037609, 3152585150, 1712706441, 627445683 }\n"
    "\n"
    "State of g1 after IncreasedPrecis (0) and 1 call to RandU01\n"
    "The current state of the Rngstream g1:\n"
    "   Cg = { 1040577685, 3747037609, 277208355, 1712706441, 627445683, 2408037141 }\n"
    "\n"
    "Sum of first 100 output values from stream g3:\n"
    "   sum =    53.985902\n"
    "\n"
    "\n"
    "Reset stream g3 to its initial seed.\n"
    "First 5 output values from stream g3:\n"
    "  0.72850979\n"
    "  0.96558728\n"
    "  0.99618413\n"
    "  0.11498842\n"
    "  0.97314542\n"
    "\n"
    "Reset stream g3 to the next Substream, 4 times.\n"
    "First 5 output values from stream g3, fourth Substream:\n"
    "  0.17345454\n"
    "  0.03286352\n"
    "  0.26431691\n"
    "  0.30594036\n"
    "  0.54763030\n"
    "\n"
    "Reset stream g2 to the beginning of Substream.\n"
    " Sum of 100000 values from stream g2 with double precision:     50098.2241\n"
    " Sum of 100000 antithetic output values from stream g3:     50017.2232\n"
    "\n"
    "SetPackageSeed to seed = { 1, 1, 1, 1, 1, 1 }\n"
    "\n"
    "Create an array of 4 named streams and write their full state\n"
    "The RngStream Poisson:\n"
    "   Anti = false\n"
    "   IncPrec = false\n"
    "   Ig = { 1, 1, 1, 1, 1, 1 }\n"
    "   Bg = { 1, 1, 1, 1, 1, 1 }\n"
    "   Cg = { 1, 1, 1, 1, 1, 1 }\n"
    "\n"
    "The RngStream Laplace:\n"
    "   Anti = false\n"
    "   IncPrec = false\n"
    "   Ig = { 2662865579, 741857976, 4206142246, 3352832365, 2519202871, 655500294 }\n"
    "   Bg = { 2662865579, 741857976, 4206142246, 3352832365, 2519202871, 655500294 }\n"
    "   Cg = { 2662865579, 741857976, 4206142246, 3352832365, 2519202871, 655500294 }\n"
    "\n"
    "The RngStream Galois:\n"
    "   Anti = false\n"
    "   IncPrec = false\n"
    "   Ig = { 3784663252, 802042081, 160569404, 3391851556, 2150317468, 54240022 }\n"
    "   Bg = { 3784663252, 802042081, 160569404, 3391851556, 2150317468, 54240022 }\n"
    "   Cg = { 3784663252, 802042081, 160569404, 3391851556, 2150317468, 54240022 }\n"
    "\n"
    "The RngStream Cantor:\n"
    "   Anti = false\n"
    "   IncPrec = false\n"
    "   Ig = { 3276123839, 3170955788, 1470482105, 884064067, 1017894425, 16401881 }\n"
    "   Bg = { 3276123839, 3170955788, 1470482105, 884064067, 1017894425, 16401881 }\n"
    "   Cg = { 3276123839, 3170955788, 1470482105, 884064067, 1017894425, 16401881 }\n"
    "\n"
    "Jump stream Galois by 2^127 steps backward\n"
    "The current state of the Rngstream Galois:\n"
    "   Cg = { 2662865579, 741857976, 4206142246, 3352832365, 2519202871, 655500294 }\n"
    "\n"
    "--------------------------------------\n"
    "Final Sum = 23.705324\n"
    "\n";

#ifndef MRG_A12
#define MRG_A12 1403580ULL      // first multiplier of component 1 (a13n = 810728)
#endif
#define M1 4294967087ULL
#define M2 4294944443ULL
#define A13N 810728ULL
#define A21 527612ULL
#define A23N 1370589ULL
#define NORM 2.328306549295727688e-10
#define FACT 5.9604644775390625e-8    // 2^-24, used by the increased-precision generator

typedef struct { u64 a[3][3]; } Mat;
static Mat mmul(Mat x, Mat y, u64 m) {
    Mat r;
    for (int i = 0; i < 3; i++) for (int j = 0; j < 3; j++) {
        u64 s = 0; for (int k = 0; k < 3; k++) s = (s + x.a[i][k] * y.a[k][j] % m) % m;
        r.a[i][j] = s;
    }
    return r;
}
static Mat midentity(void) { Mat r = {{{1, 0, 0}, {0, 1, 0}, {0, 0, 1}}}; return r; }
static Mat mpow(Mat x, u64 n, u64 m) { Mat r = midentity(); while (n) { if (n & 1) r = mmul(r, x, m); x = mmul(x, x, m); n >>= 1; } return r; }
static Mat mpow2(Mat x, int e, u64 m) { for (int i = 0; i < e; i++) x = mmul(x, x, m); return x; }
static u64 inv_mod(u64 a, u64 m) {   // extended Euclid
    i64 t = 0, nt = 1, r = (i64)m, nr = (i64)a;
    while (nr) { i64 q = r / nr, tmp = t - q * nt; t = nt; nt = tmp; tmp = r - q * nr; r = nr; nr = tmp; }
    if (t < 0) t += (i64)m;
    return (u64)t;
}
static Mat A1, A2, I1, I2;
static void mats_init(void) {
    Mat a = {{{0, 1, 0}, {0, 0, 1}, {M1 - A13N, MRG_A12, 0}}}; A1 = a;
    Mat b = {{{0, 1, 0}, {0, 0, 1}, {M2 - A23N, 0, A21}}}; A2 = b;
    u64 i13 = inv_mod(A13N, M1), i23 = inv_mod(A23N, M2);
    Mat c = {{{MRG_A12 * i13 % M1, 0, (M1 - i13) % M1}, {1, 0, 0}, {0, 1, 0}}}; I1 = c;
    Mat d = {{{0, A21 * i23 % M2, (M2 - i23) % M2}, {1, 0, 0}, {0, 1, 0}}}; I2 = d;
}
static void matvec(const Mat *x, u64 *v, u64 m) {
    u64 r[3];
    for (int i = 0; i < 3; i++) { u64 s = 0; for (int k = 0; k < 3; k++) s = (s + x->a[i][k] * v[k] % m) % m; r[i] = s; }
    for (int i = 0; i < 3; i++) v[i] = r[i];
}

typedef struct { const char *name; u64 Ig[6], Bg[6], Cg[6]; int anti, prec; } Stream;
static u64 nextSeed[6];
static void jump_vec(u64 *v6, int e) {      // v <- A^(2^e) v for both components
    Mat p = mpow2(A1, e, M1), q = mpow2(A2, e, M2);
    matvec(&p, v6, M1); matvec(&q, v6 + 3, M2);
}
static void create(Stream *g, const char *name) {
    g->name = name; g->anti = g->prec = 0;
    for (int i = 0; i < 6; i++) g->Ig[i] = g->Bg[i] = g->Cg[i] = nextSeed[i];
    jump_vec(nextSeed, 127);                // the next stream starts 2^127 steps later
}
static void reset_start_stream(Stream *g) { for (int i = 0; i < 6; i++) g->Cg[i] = g->Bg[i] = g->Ig[i]; }
static void reset_start_substream(Stream *g) { for (int i = 0; i < 6; i++) g->Cg[i] = g->Bg[i]; }
static void reset_next_substream(Stream *g) { jump_vec(g->Bg, 76); for (int i = 0; i < 6; i++) g->Cg[i] = g->Bg[i]; }
static void advance(Stream *g, int e, int c) {   // advance by 2^e + c steps (e < 0: backwards by 2^-e), as the published test uses it
    Mat B1 = midentity(), B2 = midentity(), C1, C2;
    if (e > 0) { B1 = mpow2(A1, e, M1); B2 = mpow2(A2, e, M2); }
    else if (e < 0) { B1 = mpow2(I1, -e, M1); B2 = mpow2(I2, -e, M2); }
    if (c >= 0) { C1 = mpow(A1, (u64)c, M1); C2 = mpow(A2, (u64)c, M2); }
    else { C1 = mpow(I1, (u64)(-c), M1); C2 = mpow(I2, (u64)(-c), M2); }
    if (e) { C1 = mmul(B1, C1, M1); C2 = mmul(B2, C2, M2); }
    matvec(&C1, g->Cg, M1); matvec(&C2, g->Cg + 3, M2);
}
static double u01(Stream *g) {
    i64 p1 = (i64)(MRG_A12 * g->Cg[1]) - (i64)(A13N * g->Cg[0]);
    p1 %= (i64)M1; if (p1 < 0) p1 += (i64)M1;
    g->Cg[0] = g->Cg[1]; g->Cg[1] = g->Cg[2]; g->Cg[2] = (u64)p1;
    i64 p2 = (i64)(A21 * g->Cg[5]) - (i64)(A23N * g->Cg[3]);
    p2 %= (i64)M2; if (p2 < 0) p2 += (i64)M2;
    g->Cg[3] = g->Cg[4]; g->Cg[4] = g->Cg[5]; g->Cg[5] = (u64)p2;
    double u = (p1 > p2) ? (double)(p1 - p2) * NORM : (double)(p1 - p2 + (i64)M1) * NORM;
    return g->anti ? 1 - u : u;
}
static double u01d(Stream *g) {
    double u = u01(g);
    if (!g->anti) { u += u01(g) * FACT; return u < 1.0 ? u : u - 1.0; }
    u += (u01(g) - 1.0) * FACT; return u < 0.0 ? u + 1.0 : u;
}
static double randu01(Stream *g) { return g->prec ? u01d(g) : u01(g); }
static int randint(Stream *g, int i, int j) { return i + (int)((j - i + 1.0) * randu01(g)); }

// ---- output text ----
static char out[8192]; static int on;
static void put(const char *s) { while (*s) out[on++] = *s++; out[on] = 0; }
static void putu(u64 v) { char t[24]; int k = 0; do { t[k++] = (char)('0' + v % 10); v /= 10; } while (v); while (k) out[on++] = t[--k]; out[on] = 0; }
static void putfix(double v, int width, int dec) {     // like printf("%*.*f"), for 0 <= v < 2^40
    u64 sc = 1; for (int i = 0; i < dec; i++) sc *= 10;
    u64 r = (u64)(v * (double)sc + 0.5), ip = r / sc, fr = r % sc;
    char t[40]; int n = 0; u64 x = ip; char d[24]; int k = 0;
    do { d[k++] = (char)('0' + x % 10); x /= 10; } while (x);
    while (k) t[n++] = d[--k];
    t[n++] = '.';
    for (int i = dec - 1; i >= 0; i--) { t[n + i] = (char)('0' + fr % 10); fr /= 10; }
    n += dec; t[n] = 0;
    for (int i = n; i < width; i++) out[on++] = ' ';
    put(t);
}
static void put6(const u64 *v) { put("{ "); for (int i = 0; i < 6; i++) { putu(v[i]); put(i < 5 ? ", " : " }"); } }
static void wstate(Stream *g) { put("The current state of the Rngstream "); put(g->name); put(":\n   Cg = "); put6(g->Cg); put("\n\n"); }
static void wfull(Stream *g) {
    put("The RngStream "); put(g->name); put(":\n   Anti = "); put(g->anti ? "true" : "false");
    put("\n   IncPrec = "); put(g->prec ? "true" : "false");
    put("\n   Ig = "); put6(g->Ig); put("\n   Bg = "); put6(g->Bg); put("\n   Cg = "); put6(g->Cg); put("\n\n");
}
static void randline(Stream *g) { put("RandU01 (g1) = "); putfix(randu01(g), 12, 8); put("\n\n"); }

static void replay(void) {
    double sum = 0.0, sum3 = 0.0; long sumi = 0;
    Stream g1, g2, g3, gar[4];
    for (int i = 0; i < 6; i++) nextSeed[i] = 12345;
    create(&g1, "g1"); create(&g2, "g2"); create(&g3, "g3");
    put("Initial states of g1, g2, and g3:\n\n");
    wstate(&g1); wstate(&g2); wstate(&g3);
    sum = randu01(&g2) + randu01(&g3);
    for (int i = 0; i < 12345; i++) randu01(&g2);
    advance(&g1, 5, 3);
    put("State of g1 after advancing by 2^5 + 3 = 35 steps:\n"); wstate(&g1); randline(&g1);
    reset_start_stream(&g1);
    for (int i = 0; i < 35; i++) advance(&g1, 0, 1);
    put("State of g1 after reset and advancing 35 times by 1:\n"); wstate(&g1); randline(&g1);
    reset_start_stream(&g1);
    for (int i = 0; i < 35; i++) sumi += randint(&g1, 1, 10);
    put("State of g1 after reset and 35 calls to RandInt (1, 10):\n"); wstate(&g1);
    put("   sum of 35 integers in [1, 10] = "); putu((u64)sumi); put("\n\n");
    sum += (double)sumi / 100.0;
    randline(&g1);
    sum3 = 0.0;
    reset_start_stream(&g1); g1.prec = 1; sumi = 0;
    for (int i = 0; i < 17; i++) sumi += randint(&g1, 1, 10);
    put("State of g1 after reset, IncreasedPrecis (1) and 17 calls to RandInt (1, 10):\n"); wstate(&g1);
    g1.prec = 0; randint(&g1, 1, 10);
    put("State of g1 after IncreasedPrecis (0) and 1 call to RandInt\n"); wstate(&g1);
    sum3 = (double)sumi / 10.0;
    reset_start_stream(&g1); g1.prec = 1;
    for (int i = 0; i < 17; i++) sum3 += randu01(&g1);
    put("State of g1 after reset, IncreasedPrecis (1) and 17 calls to RandU01:\n"); wstate(&g1);
    g1.prec = 0; randu01(&g1);
    put("State of g1 after IncreasedPrecis (0) and 1 call to RandU01\n"); wstate(&g1);
    sum += sum3 / 10.0;
    sum3 = 0.0;
    put("Sum of first 100 output values from stream g3:\n");
    for (int i = 0; i < 100; i++) sum3 += randu01(&g3);
    put("   sum = "); putfix(sum3, 12, 6); put("\n\n");
    sum += sum3 / 10.0;
    put("\nReset stream g3 to its initial seed.\n");
    reset_start_substream(&g3);
    put("First 5 output values from stream g3:\n");
    for (int i = 1; i <= 5; i++) { putfix(randu01(&g3), 12, 8); put("\n"); }
    sum += randu01(&g3);
    put("\nReset stream g3 to the next Substream, 4 times.\n");
    for (int i = 1; i <= 4; i++) reset_next_substream(&g3);
    put("First 5 output values from stream g3, fourth Substream:\n");
    for (int i = 1; i <= 5; i++) { putfix(randu01(&g3), 12, 8); put("\n"); }
    sum += randu01(&g3);
    put("\nReset stream g2 to the beginning of Substream.\n");
    reset_start_substream(&g2);
    put(" Sum of 100000 values from stream g2 with double precision:   ");
    g2.prec = 1; sum3 = 0.0;
    for (int i = 1; i <= 100000; i++) sum3 += randu01(&g2);
    putfix(sum3, 12, 4); put("\n");
    sum += sum3 / 10000.0;
    g2.prec = 0;
    g3.anti = 1;
    put(" Sum of 100000 antithetic output values from stream g3:   ");
    sum3 = 0.0;
    for (int i = 1; i <= 100000; i++) sum3 += randu01(&g3);
    putfix(sum3, 12, 4); put("\n");
    sum += sum3 / 10000.0;
    put("\nSetPackageSeed to seed = { 1, 1, 1, 1, 1, 1 }\n");
    for (int i = 0; i < 6; i++) nextSeed[i] = 1;
    put("\nCreate an array of 4 named streams and write their full state\n");
    create(&gar[0], "Poisson"); create(&gar[1], "Laplace"); create(&gar[2], "Galois"); create(&gar[3], "Cantor");
    for (int i = 0; i < 4; i++) wfull(&gar[i]);
    put("Jump stream Galois by 2^127 steps backward\n");
    advance(&gar[2], -127, 0);
    wstate(&gar[2]);
    reset_next_substream(&gar[2]);
    for (int i = 0; i < 4; i++) sum += randu01(&gar[i]);
    put("--------------------------------------\n");
    put("Final Sum = "); putfix(sum, 0, 6); put("\n\n");
}

int paper_main(void) {
    mats_init();
    replay();
    // compare line by line with the published result file
    const char *a = out, *b = EXPECT;
    int lines = 0, good = 0, ok = 1;
    for (;;) {
        const char *ea = a, *eb = b;
        while (*ea && *ea != '\n') ea++;
        while (*eb && *eb != '\n') eb++;
        int same = (ea - a) == (eb - b);
        for (long i = 0; same && i < ea - a; i++) if (a[i] != b[i]) same = 0;
        lines++; good += same;
        if (!same) {
            ok = 0; pstr("FAIL line "); pdec(lines); pstr(": got \"");
            paper_write(a, ea - a); pstr("\" expected \""); paper_write(b, eb - b); pstr("\"\n");
        }
        if (!*ea || !*eb) { if (*ea != *eb) ok = 0; break; }
        a = ea + 1; b = eb + 1;
    }
    pstr("published test2RngStream.res: "); pdec(good); pstr(" of "); pdec(lines); pstr(" lines match\n");
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
