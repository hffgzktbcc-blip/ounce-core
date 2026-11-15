/* FastLZ differential cases: both levels on generated inputs, compressed size and hash,
 * round trip, and decoder behaviour on truncated and random streams. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "fastlz.h"

static uint32_t rng_state = 0x1234567u;
static uint32_t rnd(void) { rng_state ^= rng_state << 13; rng_state ^= rng_state >> 17; rng_state ^= rng_state << 5; return rng_state; }
static uint32_t fnv(const uint8_t *p, size_t n) { uint32_t h = 2166136261u; while (n--) h = (h ^ *p++) * 16777619u; return h; }

static void gen(int kind, uint8_t *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        switch (kind) {
        case 0: b[i] = 0; break;
        case 1: b[i] = (uint8_t)rnd(); break;
        case 2: b[i] = "the quick brown fox jumps over the lazy dog. "[i % 45]; break;
        case 3: b[i] = (uint8_t)((i / 7) ^ (i % 13)); break;
        default: b[i] = (rnd() & 7) ? (uint8_t)(i % 31) : (uint8_t)rnd(); break;
        }
    }
}

int flz_run_cases(FILE *o) {
    static const int sizes[] = {16, 17, 32, 100, 1000, 4096, 10000, 65536, 70000};
    static uint8_t in[70000], back[70000], comp[70000 * 2 + 1024];
    int fails = 0;
    for (int kind = 0; kind < 5; kind++)
        for (size_t s = 0; s < sizeof sizes / sizeof *sizes; s++)
            for (int level = 1; level <= 2; level++) {
                int n = sizes[s];
                gen(kind, in, (size_t)n);
                int c = fastlz_compress_level(level, in, n, comp);
                int d = fastlz_decompress(comp, c, back, sizeof back);
                int ok = d == n && memcmp(in, back, (size_t)n) == 0;
                fails += !ok;
                fprintf(o, "kind=%d n=%d L%d -> %d hash=%08x rt=%s\n", kind, n, level, c, fnv(comp, (size_t)c), ok ? "ok" : "FAIL");
            }
    /* decoder on truncated valid streams and on noise: output length and hash are the observation */
    gen(2, in, 3000);
    int c = fastlz_compress_level(2, in, 3000, comp);
    for (int cut = 1; cut < c; cut += c / 9 + 1) {
        memset(back, 0xAA, 3100);
        int d = fastlz_decompress(comp, cut, back, 3100);
        fprintf(o, "truncated cut=%d -> %d hash=%08x\n", cut, d, d > 0 ? fnv(back, (size_t)d) : 0u);
    }
    for (int t = 0; t < 40; t++) {
        uint8_t noise[200];
        for (int i = 0; i < 200; i++) noise[i] = (uint8_t)rnd();
        noise[0] = (uint8_t)((noise[0] & 0x1f) | ((t & 1) ? 0x20 : 0x00));
        memset(back, 0, 1024);
        int d = fastlz_decompress(noise, 200, back, 1024);
        fprintf(o, "noise[%d] -> %d hash=%08x\n", t, d, d > 0 ? fnv(back, (size_t)d) : 0u);
    }
    fprintf(o, "TOTAL FAILS=%d\n", fails);
    return fails;
}
