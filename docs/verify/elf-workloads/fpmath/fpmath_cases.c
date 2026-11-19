/* elf-workload fpmath: IEEE-754 double and float arithmetic restricted to operations that are
 * correctly rounded or exact (+ - * / sqrt, floor, ceil, trunc, fmod, ldexp, frexp, comparison, casts)
 * so results do not depend on the libm. Build with -ffp-contract=off so no fused multiply-add changes
 * the rounding. Values are printed with %a-free fixed formats and as raw bit patterns.
 * Provenance: original work written for ounce-core (MIT); textbook algorithms. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <math.h>

static uint64_t bits(double d) { uint64_t u; memcpy(&u, &d, 8); return u; }
static uint32_t fbits(float f) { uint32_t u; memcpy(&u, &f, 4); return u; }

static double newton_sqrt(double x) {
    double g = x > 1 ? x / 2 : 1;
    for (int i = 0; i < 60; i++) g = 0.5 * (g + x / g);
    return g;
}

int workload_run(FILE *o) {
    int fails = 0;
    volatile double one = 1.0, three = 3.0, ten = 10.0;

    fprintf(o, "1/3 bits=%016llx 0.1+0.2 bits=%016llx\n", (unsigned long long)bits(one / three),
            (unsigned long long)bits(0.1 * one + 0.2 * one));
    int near = fabs(newton_sqrt(2.0) - sqrt(2.0 * one)) < 4.5e-16; /* Newton may oscillate by one ulp */
    fprintf(o, "sqrt(2) bits=%016llx newton within 2 ulp=%d newton bits=%016llx\n", (unsigned long long)bits(sqrt(2.0 * one)),
            near, (unsigned long long)bits(newton_sqrt(2.0)));
    fprintf(o, "sqrtf(2) bits=%08x\n", fbits(sqrtf(2.0f * (float)one)));
    if (!near) fails++;

    /* Kahan vs naive summation of 0.1 ten thousand times */
    double naive = 0, ksum = 0, comp = 0;
    for (int i = 0; i < 10000; i++) {
        naive += 0.1 * one;
        double y = 0.1 * one - comp;
        double t = ksum + y;
        comp = (t - ksum) - y;
        ksum = t;
    }
    fprintf(o, "naive=%.10f kahan=%.10f naive bits=%016llx kahan bits=%016llx\n", naive, ksum,
            (unsigned long long)bits(naive), (unsigned long long)bits(ksum));

    /* exact-operation family */
    static const double xs[] = {2.5, -2.5, 3.5, -3.5, 0.49999999999999994, 1e15 + 0.5, -0.0, 7.0, 1e300};
    for (unsigned i = 0; i < sizeof xs / sizeof *xs; i++) {
        double x = xs[i];
        fprintf(o, "x=%.17g floor=%.17g ceil=%.17g trunc=%.17g fabs=%.17g fmod(x,3)=%.17g\n", x, floor(x), ceil(x),
                trunc(x), fabs(x), fmod(x, 3.0));
    }
    int e;
    double m = frexp(1234.5, &e);
    fprintf(o, "frexp(1234.5)=%.17g*2^%d ldexp back=%.17g ldexp(1,-1074)bits=%016llx\n", m, e, ldexp(m, e),
            (unsigned long long)bits(ldexp(1.0, -1074)));
    if (ldexp(m, e) != 1234.5) fails++;

    /* limits and special values */
    double inf = one / (one - one), nan = inf - inf;
    fprintf(o, "inf=%d nan!=nan=%d isnan=%d inf>1e308=%d -inf<0=%d signbit(-0.0)=%d\n", !!isinf(inf), nan != nan, !!isnan(nan),
            inf > 1e308, -inf < 0, signbit(-0.0 * one) != 0);
    fprintf(o, "DBL_MAX*2 bits=%016llx denormal bits=%016llx\n", (unsigned long long)bits(1.7976931348623157e308 * (one + one)),
            (unsigned long long)bits(2.2250738585072014e-308 / (ten * ten)));

    /* integer <-> float conversions */
    volatile int64_t big = 9007199254740993ll; /* 2^53 + 1 is not representable */
    volatile float ff = 16777217.0f;           /* 2^24 + 1 rounds to even in float */
    fprintf(o, "(double)(2^53+1)=%.1f float(2^24+1)=%.1f (int)-7.9=%d (unsigned)3e9=%u (long long)-1e18=%lld\n", (double)big,
            (double)ff, (int)(-7.9 * one), (unsigned)(3e9 * one), (long long)(-1e18 * one));

    /* Horner polynomial, trapezoid integral of x^2 on [0,1] with 1000 steps (separate mul and add) */
    double coef[6] = {1, -2, 3, -4, 5, -6}, acc = 0, xx = 0.5 * one;
    for (int i = 5; i >= 0; i--) { double p = acc * xx; acc = p + coef[i]; }
    double area = 0, h = 1.0 / 1000.0;
    for (int i = 0; i < 1000; i++) {
        double a = i * h, b = (i + 1) * h;
        double fa = a * a, fb = b * b;
        area += (fa + fb) * 0.5 * h;
    }
    fprintf(o, "horner(0.5)=%.17g area=%.12f bits=%016llx\n", acc, area, (unsigned long long)bits(area));

    /* single-precision vector dot and running mean */
    float fv[64], dot = 0, mean = 0;
    for (int i = 0; i < 64; i++) fv[i] = (float)(i % 7) * 0.25f + 1.0f;
    for (int i = 0; i < 64; i++) { float p = fv[i] * fv[(i * 5) & 63]; dot += p; mean += (fv[i] - mean) / (float)(i + 1); }
    fprintf(o, "dot=%08x mean=%08x (%.6f %.6f)\n", fbits(dot), fbits(mean), (double)dot, (double)mean);

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
