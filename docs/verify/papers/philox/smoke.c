// Statistical smoke tests for Philox4x32, written for ounce-core from textbook definitions (NIST SP 800-22 frequency and runs
// tests, Pearson chi-square, lag-1 serial correlation, strict avalanche). This is a SMOKE test: it catches gross defects (it
// fails a 2-round Philox) and says nothing about the paper's BigCrush claim. Thresholds were fixed before the first run:
// |z| < 4 for every statistic, and |z| < 5 for the worst of the avalanche pairs. The input is deterministic, so a pass or a
// fail is a fixed fact about this generator, not a chance event.
// Freestanding aarch64 Linux (or -DHOST). -DROUNDS=n changes the round count (default 10) for the negative control.
// Exit 0 only if every test passes.
typedef unsigned int u32;
typedef unsigned long long u64;
#ifndef ROUNDS
#define ROUNDS 10
#endif

#ifdef HOST
#include <stdio.h>
#include <stdlib.h>
static void put(const char *s, long n) { fwrite(s, 1, (size_t)n, stdout); }
static void quit(int code) { fflush(stdout); exit(code); }
#else
static void put(const char *s, long n) {
    register long x0 __asm__("x0") = 1;
    register const char *x1 __asm__("x1") = s;
    register long x2 __asm__("x2") = n;
    register long x8 __asm__("x8") = 64;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x8) : "memory");
}
void *memset(void *d, int c, unsigned long n) { unsigned char *p = d; while (n--) *p++ = (unsigned char)c; return d; }
static void quit(int code) {
    register long x0 __asm__("x0") = code;
    register long x8 __asm__("x8") = 93;
    __asm__ volatile("svc #0" : : "r"(x0), "r"(x8));
    for (;;) {}
}
#endif
static void str(const char *s) { long n = 0; while (s[n]) n++; put(s, n); }
static void num(long long v) {
    char b[24]; int n = 0; unsigned long long u = v < 0 ? (unsigned long long)(-v) : (unsigned long long)v;
    if (v < 0) put("-", 1);
    do { b[n++] = (char)('0' + u % 10); u /= 10; } while (u);
    while (n) put(&b[--n], 1);
}
static void fix2(double d) {   // signed, 2 decimals
    if (d < 0) { put("-", 1); d = -d; } else put("+", 1);
    long long t = (long long)(d * 100.0 + 0.5); num(t / 100); put(".", 1); if (t % 100 < 10) put("0", 1); num(t % 100);
}

static void philox(u32 c0, u32 c1, u32 c2, u32 c3, u32 k0, u32 k1, u32 out[4]) {
    for (int r = 0; r < ROUNDS; r++) {
        u64 p0 = (u64)0xD2511F53u * c0, p1 = (u64)0xCD9E8D57u * c2;
        u32 n0 = (u32)(p1 >> 32) ^ c1 ^ k0, n1 = (u32)p1, n2 = (u32)(p0 >> 32) ^ c3 ^ k1, n3 = (u32)p0;
        c0 = n0; c1 = n1; c2 = n2; c3 = n3; k0 += 0x9E3779B9u; k1 += 0xBB67AE85u;
    }
    out[0] = c0; out[1] = c1; out[2] = c2; out[3] = c3;
}

typedef struct { u32 k0, k1, c0, c1, c2, c3, buf[4]; int left; } Stream;
static void stream_init(Stream *s, u32 k0, u32 k1, u32 c2, u32 c3) { s->k0 = k0; s->k1 = k1; s->c0 = 0; s->c1 = 0; s->c2 = c2; s->c3 = c3; s->left = 0; }
static u32 stream_next(Stream *s) {   // sequential counters: the case a counter-based generator must get right
    if (!s->left) {
        philox(s->c0, s->c1, s->c2, s->c3, s->k0, s->k1, s->buf);
        if (!++s->c0 && !++s->c1 && !++s->c2) s->c3++;
        s->left = 4;
    }
    return s->buf[4 - s->left--];
}

static int failures;
#ifdef HOST
static double sqrtd(double x) { return __builtin_sqrt(x); }
#else
static double sqrtd(double x) { double r; __asm__("fsqrt %d0, %d1" : "=w"(r) : "w"(x)); return r; }   // correctly rounded, like the host's
#endif
static void report(const char *name, double z, double limit) {
    int ok = (z < 0 ? -z : z) < limit; if (!ok) failures++;
    str("  "); str(name); str(": z = "); fix2(z); str(ok ? "  ok\n" : "  FAIL\n");
}
static double chi_z(const unsigned *hist, int bins, double expected) {   // (chi2 - k) / sqrt(2k), k = bins - 1
    double chi = 0; for (int i = 0; i < bins; i++) { double d = (double)hist[i] - expected; chi += d * d / expected; }
    return (chi - (bins - 1)) / sqrtd(2.0 * (bins - 1));
}

#define N (1u << 20)
static unsigned hist8[256], hist16[65536];
static void stream_tests(const char *label, Stream *s) {
    for (int i = 0; i < 256; i++) hist8[i] = 0;
    for (int i = 0; i < 65536; i++) hist16[i] = 0;
    u64 ones = 0, trans = 0; double su = 0, suu = 0, sux = 0, prev = 0, first = 0; u32 prev_w = 0;
    for (u32 i = 0; i < N; i++) {
        u32 w = stream_next(s); double u = (double)w / 4294967296.0;
        ones += (u64)__builtin_popcount(w);
        trans += (u64)__builtin_popcount((w ^ (w >> 1)) & 0x7FFFFFFFu);
        if (i) trans += (u64)((prev_w >> 31) ^ (w & 1));
        hist8[w & 255]++; hist8[(w >> 8) & 255]++; hist8[(w >> 16) & 255]++; hist8[w >> 24]++;
        hist16[w & 65535]++; hist16[w >> 16]++;
        su += u; suu += u * u; if (i) sux += prev * u; else first = u;
        prev = u; prev_w = w;
    }
    double bits = 32.0 * (double)N, pi = (double)ones / bits;
    str(label); str("\n");
    report("frequency of ones (NIST 2.1)", ((double)ones - bits / 2.0) / sqrtd(bits / 4.0), 4.0);
    double runs = 1.0 + (double)trans, e = 2.0 * bits * pi * (1.0 - pi);
    report("runs of equal bits (NIST 2.3)", (runs - e) / (2.0 * sqrtd(2.0 * bits) * pi * (1.0 - pi)), 4.0);
    report("chi-square of bytes, 256 bins", chi_z(hist8, 256, 4.0 * (double)N / 256.0), 4.0);
    report("chi-square of 16-bit halves, 65536 bins", chi_z(hist16, 65536, 2.0 * (double)N / 65536.0), 4.0);
    double n = (double)N, num_ = n * (sux + prev * first) - su * su, den = n * suu - su * su;
    report("lag-1 serial correlation", (num_ / den) * sqrtd(n), 4.0);
}

static u64 sm_state;
static u64 splitmix(void) { u64 z = (sm_state += 0x9E3779B97F4A7C15ull); z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull; z = (z ^ (z >> 27)) * 0x94D049BB133111EBull; return z ^ (z >> 31); }
#define SAMPLES 1024
static unsigned flips[192][128];
static void avalanche(void) {
    for (int i = 0; i < 192; i++) for (int j = 0; j < 128; j++) flips[i][j] = 0;
    sm_state = 12345;
    for (int s = 0; s < SAMPLES; s++) {
        u64 a = splitmix(), b = splitmix(), c = splitmix(); u32 in[6] = {(u32)a, (u32)(a >> 32), (u32)b, (u32)(b >> 32), (u32)c, (u32)(c >> 32)}, base[4];
        philox(in[0], in[1], in[2], in[3], in[4], in[5], base);
        for (int bit = 0; bit < 192; bit++) {
            u32 t[6]; for (int k = 0; k < 6; k++) t[k] = in[k];
            t[bit >> 5] ^= 1u << (bit & 31);
            u32 o[4]; philox(t[0], t[1], t[2], t[3], t[4], t[5], o);
            for (int w = 0; w < 4; w++) { u32 d = o[w] ^ base[w]; for (int k = 0; k < 32; k++) flips[bit][w * 32 + k] += (d >> k) & 1; }
        }
    }
    double worst = 0, sd = sqrtd((double)SAMPLES) / 2.0; int worst_in = 0, worst_out = 0;
    for (int i = 0; i < 192; i++) for (int j = 0; j < 128; j++) {
        double z = ((double)flips[i][j] - (double)SAMPLES / 2.0) / sd; if (z < 0) z = -z;
        if (z > worst) { worst = z; worst_in = i; worst_out = j; }
    }
    str("strict avalanche: flip each of 128 counter bits and 64 key bits, "); num(SAMPLES); str(" random inputs, 128 output bits\n");
    str("  worst of 24576 (input bit, output bit) pairs is input "); num(worst_in); str(" -> output "); num(worst_out); str("\n");
    report("worst flip-probability deviation (limit 5)", worst, 5.0);
}

static void run(void) {
    str("Philox4x32 with "); num(ROUNDS); str(" rounds, "); num(N); str(" 32-bit words per stream\n");
    Stream a, b; stream_init(&a, 0, 0, 0, 0); stream_init(&b, 0xdeadbeef, 0x12345678, 0, 0);
    stream_tests("stream A: key (0,0), counters 0, 1, 2, ...", &a);
    stream_tests("stream B: key (deadbeef,12345678), counters 0, 1, 2, ...", &b);
    stream_init(&a, 0, 0, 0, 0); stream_init(&b, 1, 0, 0, 0);
    double sa = 0, sb = 0, saa = 0, sbb = 0, sab = 0; double n = (double)N;
    for (u32 i = 0; i < N; i++) { double x = (double)stream_next(&a) / 4294967296.0, y = (double)stream_next(&b) / 4294967296.0; sa += x; sb += y; saa += x * x; sbb += y * y; sab += x * y; }
    str("two streams whose keys differ in one bit (0,0) and (1,0)\n");
    double cov = n * sab - sa * sb, var = sqrtd((n * saa - sa * sa) * (n * sbb - sb * sb));
    report("correlation between the streams", (cov / var) * sqrtd(n), 4.0);
    avalanche();
    str(failures ? "FAIL\n" : "PASS\n");
    quit(failures != 0);
}
#ifdef HOST
int main(void) { run(); return 0; }
#else
void _start(void) { run(); }
#endif
