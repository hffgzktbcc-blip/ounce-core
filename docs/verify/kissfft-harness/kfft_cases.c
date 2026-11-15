/* KISS FFT (BSD-3) differential cases in single precision: complex forward/inverse transforms of
 * generated signals for sizes with radix 2, 3, 4, 5 and generic factors, real-input transforms, and
 * a forward/inverse round trip. Floating-point results are printed as raw IEEE bit patterns.
 * The expected output comes from running the same AArch64 ELF under qemu-user (`elf_reference`:
 * "qemu"), because the compiler may contract multiply-adds on AArch64 and the C library's sin/cos
 * differ between hosts, so a host x86-64 build is not a bit-exact reference for this program.
 * A mathematical check (DFT of a pure tone, Parseval's identity) is printed as an independent oracle. */
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stdint.h>
#include <math.h>
#include "kiss_fft.h"
#include "kiss_fftr.h"

static uint32_t bits(float f) { uint32_t u; memcpy(&u, &f, 4); return u; }
static uint32_t rng = 0xC0FFEEu;
static float rnd(void) { rng = rng * 1664525u + 1013904223u; return (float)((int32_t)(rng >> 8) - (1 << 23)) / (float)(1 << 23); }

static uint32_t digest(const kiss_fft_cpx *v, int n) {
    uint32_t h = 2166136261u;
    for (int i = 0; i < n; i++) { h = (h ^ bits(v[i].r)) * 16777619u; h = (h ^ bits(v[i].i)) * 16777619u; }
    return h;
}

int kfft_run_cases(FILE *o) {
    static const int sizes[] = {1, 2, 3, 4, 5, 6, 8, 9, 12, 15, 16, 20, 30, 45, 60, 64, 100, 125, 128, 210, 256, 360, 512, 1000, 1024};
    int fails = 0;
    for (unsigned s = 0; s < sizeof sizes / sizeof *sizes; s++) {
        int n = sizes[s];
        kiss_fft_cpx *in = malloc(sizeof *in * n), *out = malloc(sizeof *out * n), *back = malloc(sizeof *back * n);
        kiss_fft_cfg fwd = kiss_fft_alloc(n, 0, NULL, NULL), inv = kiss_fft_alloc(n, 1, NULL, NULL);
        for (int i = 0; i < n; i++) { in[i].r = rnd(); in[i].i = rnd(); }
        kiss_fft(fwd, in, out);
        kiss_fft(inv, out, back);
        double err = 0, e_in = 0, e_out = 0;
        for (int i = 0; i < n; i++) {
            err = fmax(err, fabs(back[i].r / n - in[i].r) + fabs(back[i].i / n - in[i].i));
            e_in += (double)in[i].r * in[i].r + (double)in[i].i * in[i].i;
            e_out += (double)out[i].r * out[i].r + (double)out[i].i * out[i].i;
        }
        /* Parseval: sum|X|^2 = n * sum|x|^2, round trip returns n*x */
        int ok = err < 1e-4 && fabs(e_out / n - e_in) < 1e-3 * (1 + e_in);
        fails += !ok;
        fprintf(o, "fft[%d] digest=%08x X0=(%08x,%08x) X1=(%08x,%08x) roundtrip+parseval=%s\n", n, digest(out, n),
                bits(out[0].r), bits(out[0].i), bits(out[n > 1 ? 1 : 0].r), bits(out[n > 1 ? 1 : 0].i), ok ? "ok" : "FAIL");
        free(in); free(out); free(back); free(fwd); free(inv);
    }
    /* pure tone: energy lands in exactly bins k and n-k */
    {
        int n = 64, k = 5;
        kiss_fft_cpx in[64], out[64];
        for (int i = 0; i < n; i++) { in[i].r = cosf(6.2831853f * (float)(k * i) / (float)n); in[i].i = sinf(6.2831853f * (float)(k * i) / (float)n); }
        kiss_fft_cfg c = kiss_fft_alloc(n, 0, NULL, NULL);
        kiss_fft(c, in, out);
        int peak = 0;
        for (int i = 1; i < n; i++) if (out[i].r * out[i].r + out[i].i * out[i].i > out[peak].r * out[peak].r + out[peak].i * out[peak].i) peak = i;
        int ok = peak == k;
        fprintf(o, "tone peak bin=%d (expected %d): %s\n", peak, k, ok ? "PASS" : "FAIL");
        fails += !ok;
        free(c);
    }
    static const int rsizes[] = {2, 4, 8, 16, 30, 64, 100, 256, 1000};
    for (unsigned s = 0; s < sizeof rsizes / sizeof *rsizes; s++) {
        int n = rsizes[s];
        float *in = malloc(sizeof *in * n), *back = malloc(sizeof *back * n);
        kiss_fft_cpx *out = malloc(sizeof *out * (n / 2 + 1));
        kiss_fftr_cfg f = kiss_fftr_alloc(n, 0, NULL, NULL), r = kiss_fftr_alloc(n, 1, NULL, NULL);
        for (int i = 0; i < n; i++) in[i] = rnd();
        kiss_fftr(f, in, out);
        kiss_fftri(r, out, back);
        double err = 0;
        for (int i = 0; i < n; i++) err = fmax(err, fabs(back[i] / n - in[i]));
        int ok = err < 1e-4; fails += !ok;
        fprintf(o, "fftr[%d] digest=%08x roundtrip=%s\n", n, digest(out, n / 2 + 1), ok ? "ok" : "FAIL");
        free(in); free(back); free(out); free(f); free(r);
    }
    fprintf(o, "TOTAL FAILS=%d\n", fails);
    return fails;
}
