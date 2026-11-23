// Reproduction of the numerical results in C. J. Walczyk, L. V. Moroz, J. L. Cieslinski, "Improving the accuracy of the fast
// inverse square root algorithm" (arXiv:1802.06302 [cs.NA], 17 Feb 2018): maximum relative errors of the fast inverse square
// root code InvSqrt (magic constants 0x5F37642F and 0x5F375A86) and of the paper's InvSqrt1 and InvSqrt2, in binary32.
// Written for ounce-core from the code listings and formulas in the paper; no third-party code. The relative error is
// sqrt(x)*y - 1 with sqrt in double (paper_sqrt), over x in [1, 4) (the paper: the range is the same for every interval).
// To fit the time budget only every 16th float of [1, 4) is tested (1048576 values per algorithm, fixed grid), see paper.json.
// Negative control: -DNEG_CONST changes the constant 1.5008789f of InvSqrt2 to 1.5018789f.
#include "ounce_paper.h"

typedef union { float f; u32 u; } FU;
static float from_bits(u32 b) { FU v; v.u = b; return v.f; }
static u32 to_bits(float f) { FU v; v.f = f; return v.u; }

static float y0_of(float x, u32 R) { return from_bits(R - (to_bits(x) >> 1)); }
// returns the result after one (it = 1) or two (it = 2) iterations
static float invsqrt_std(float x, u32 R, int it) {
    float h = 0.5f * x, y = y0_of(x, R);
    y = y * (1.5f - h * y * y);
    if (it == 2) y = y * (1.5f - h * y * y);
    return y;
}
static float invsqrt1(float x, int it) {
    float s = 0.50043818f * x, y = y0_of(x, 0x5F375A86u);
    y = y * (1.5013145f - s * y * y);
    if (it == 2) y = y * (1.5000008f - 0.99912498f * s * y * y);
    return y;
}
static float invsqrt2(float x, int it) {
#ifdef NEG_CONST
    const float c1 = 1.5018789f;
#else
    const float c1 = 1.5008789f;
#endif
    float h = 0.5f * x, y = y0_of(x, 0x5F376908u);
    y = y * (c1 - h * y * y);
    if (it == 2) y = y * (1.5000006f - h * y * y);
    return y;
}

// The paper's runs used a 32-bit compiler (TDM-GCC 4.9.2), whose x87 arithmetic keeps expressions in extended precision and rounds to
// float only on assignment; the paper says 64-bit (SSE, strict float) builds give errors larger by about 1e-7. The *_x functions emulate
// the extended intermediates with double (every expression in double, rounded to float on assignment).
static float invsqrt_std_x(float x, u32 R, int it) {
    float h = (float)(0.5 * (double)x), y = y0_of(x, R);
    y = (float)((double)y * (1.5 - (double)h * y * y));
    if (it == 2) y = (float)((double)y * (1.5 - (double)h * y * y));
    return y;
}
static float invsqrt1_x(float x, int it) {
    float s = (float)((double)0.50043818f * (double)x), y = y0_of(x, 0x5F375A86u);
    y = (float)((double)y * ((double)1.5013145f - (double)s * y * y));
    if (it == 2) y = (float)((double)y * ((double)1.5000008f - (double)0.99912498f * (double)s * y * y));
    return y;
}
static float invsqrt2_x(float x, int it) {
#ifdef NEG_CONST
    const float c1 = 1.5018789f;
#else
    const float c1 = 1.5008789f;
#endif
    float h = (float)(0.5 * (double)x), y = y0_of(x, 0x5F376908u);
    y = (float)((double)y * ((double)c1 - (double)h * y * y));
    if (it == 2) y = (float)((double)y * ((double)1.5000006f - (double)h * y * y));
    return y;
}

typedef struct { double mn, mx; } Rng;
static void upd(Rng *r, double e) { if (e < r->mn) r->mn = e; if (e > r->mx) r->mx = e; }
static double dabs(double x) { return x < 0 ? -x : x; }
// print a double as m.mmm e-k with 4 significant digits
static void sci4(double v) {
    if (v < 0) { pstr("-"); v = -v; }
    int ex = 0; if (v == 0) { pstr("0"); return; }
    while (v >= 10.0) { v /= 10.0; ex++; } while (v < 1.0) { v *= 10.0; ex--; }
    i64 m = (i64)(v * 1000.0 + 0.5); if (m >= 10000) { m /= 10; ex++; }
    pdec(m / 1000); pstr("."); { i64 f = m % 1000; char b[3]; for (int i = 2; i >= 0; i--) { b[i] = (char)('0' + f % 10); f /= 10; } paper_write(b, 3); }
    pstr("e"); pdec(ex);
}
static int bad = 0;
// accept when |got - pub| <= tol * pub
static int near(double got, double pub, double tol) { double d = got - pub; if (d < 0) d = -d; return d <= tol * pub; }
static void check_val(const char *what, double got, double pub, double tol) {
    int ok = near(got, pub, tol);
    pstr(ok ? "  ok: " : "  FAIL: "); pstr(what); pstr(" = "); sci4(got); pstr(", paper "); sci4(pub); pstr(ok ? "\n" : " (outside tolerance)\n");
    if (!ok) bad++;
}

int paper_main(void) {
    Rng z0 = {1e9, -1e9};            // InvSqrt, R = 0x5F37642F, zeroth approximation (no iteration)
    Rng s1 = {1e9, -1e9}, s2 = {1e9, -1e9};      // InvSqrt, R = 0x5F375A86, 1 and 2 iterations
    Rng a1 = {1e9, -1e9}, a2 = {1e9, -1e9};      // InvSqrt1, 1 and 2 iterations
    Rng b1 = {1e9, -1e9}, b2 = {1e9, -1e9};      // InvSqrt2, 1 and 2 iterations
    Rng xs1 = {1e9, -1e9}, xs2 = {1e9, -1e9}, xa1 = {1e9, -1e9}, xa2 = {1e9, -1e9}, xb1 = {1e9, -1e9}, xb2 = {1e9, -1e9};   // extended-intermediate variants
    u32 n = 0;
    for (u32 bits = 0x3F800000u; bits < 0x40800000u; bits += 16) {
        float x = from_bits(bits); double sx = paper_sqrt((double)x);
        upd(&z0, sx * (double)y0_of(x, 0x5F37642Fu) - 1.0);
        upd(&s1, sx * (double)invsqrt_std(x, 0x5F375A86u, 1) - 1.0); upd(&s2, sx * (double)invsqrt_std(x, 0x5F375A86u, 2) - 1.0);
        upd(&a1, sx * (double)invsqrt1(x, 1) - 1.0); upd(&a2, sx * (double)invsqrt1(x, 2) - 1.0);
        upd(&b1, sx * (double)invsqrt2(x, 1) - 1.0); upd(&b2, sx * (double)invsqrt2(x, 2) - 1.0);
        upd(&xs1, sx * (double)invsqrt_std_x(x, 0x5F375A86u, 1) - 1.0); upd(&xs2, sx * (double)invsqrt_std_x(x, 0x5F375A86u, 2) - 1.0);
        upd(&xa1, sx * (double)invsqrt1_x(x, 1) - 1.0); upd(&xa2, sx * (double)invsqrt1_x(x, 2) - 1.0);
        upd(&xb1, sx * (double)invsqrt2_x(x, 1) - 1.0); upd(&xb2, sx * (double)invsqrt2_x(x, 2) - 1.0);
        n++;
    }
    pstr("floats tested in [1, 4): "); pdec(n); pstr("\n");
    pstr("InvSqrt, R = 0x5F37642F, no iteration\n");
    check_val("max |relative error| (paper (2.22): 0.03421281)", dabs(z0.mn) > dabs(z0.mx) ? dabs(z0.mn) : dabs(z0.mx), 0.03421281, 1e-5);
    pstr("--- extended-precision intermediates (emulating the paper's 32-bit x87 build) ---\n");
    pstr("InvSqrt, R = 0x5F375A86 (paper (2.26))\n");
    check_val("max |relative error| after 1 iteration (1.75118e-3)", dabs(xs1.mn) > dabs(xs1.mx) ? dabs(xs1.mn) : dabs(xs1.mx), 1.75118e-3, 5e-3);
    check_val("max |relative error| after 2 iterations (4.60e-6)", dabs(xs2.mn) > dabs(xs2.mx) ? dabs(xs2.mn) : dabs(xs2.mx), 4.60e-6, 3e-2);
    pstr("InvSqrt1, 2 iterations (paper (7.1): -6.62e-7 .. 6.35e-7)\n");
    check_val("lowest error", -xa2.mn, 6.62e-7, 3e-2); check_val("highest error", xa2.mx, 6.35e-7, 3e-2);
    pstr("InvSqrt2, 2 iterations (paper (7.5): -6.21e-7 .. 6.53e-7)\n");
    check_val("lowest error", -xb2.mn, 6.21e-7, 3e-2); check_val("highest error", xb2.mx, 6.53e-7, 3e-2);
    pstr("after 1 iteration only (paper: InvSqrt1 +-8.76e-4, InvSqrt2 +-8.79e-4)\n");
    check_val("InvSqrt1 max |error|", dabs(xa1.mn) > dabs(xa1.mx) ? dabs(xa1.mn) : dabs(xa1.mx), 8.76e-4, 5e-3);
    check_val("InvSqrt2 max |error|", dabs(xb1.mn) > dabs(xb1.mx) ? dabs(xb1.mn) : dabs(xb1.mx), 8.79e-4, 5e-3);
    { double r = (dabs(xs2.mn) > dabs(xs2.mx) ? dabs(xs2.mn) : dabs(xs2.mx)) / xb2.mx;
      pstr("InvSqrt / InvSqrt2 error ratio after 2 iterations = "); sci4(r); pstr(" (paper: about 7)\n");
      int ok = r > 6.0 && r < 8.0; pstr(ok ? "  ok: ratio between 6 and 8\n" : "  FAIL: ratio outside 6..8\n"); if (!ok) bad++; }
    pstr("--- strict binary32 arithmetic (SSE/aarch64 float, no extended intermediates) ---\n");
    pstr("(the paper: a 64-bit build has error oscillations larger by about 1e-7)\n");
    { double w_ext = xa2.mx - xa2.mn, w_str = a2.mx - a2.mn;
      pstr("InvSqrt1 2-iteration error range width: extended "); sci4(w_ext); pstr(", strict "); sci4(w_str); pstr(", difference "); sci4(w_str - w_ext); pstr("\n");
      double w_ext2 = xb2.mx - xb2.mn, w_str2 = b2.mx - b2.mn;
      pstr("InvSqrt2 2-iteration error range width: extended "); sci4(w_ext2); pstr(", strict "); sci4(w_str2); pstr(", difference "); sci4(w_str2 - w_ext2); pstr("\n");
      // the width of the range is the two-sided amplitude; a one-sided amplitude differs by half that; 'about 1e-7' accepted as 0.3e-7 .. 3e-7 for the widths
      int ok = (w_str - w_ext) > 3e-8 && (w_str - w_ext) < 3e-7 && (w_str2 - w_ext2) > 3e-8 && (w_str2 - w_ext2) < 3e-7;
      pstr(ok ? "  ok: strict float is wider by about 1e-7 as the paper says\n" : "  FAIL: difference not about 1e-7\n"); if (!ok) bad++; }
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
