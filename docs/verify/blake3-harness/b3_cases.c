/* BLAKE3 differential cases. On AArch64 the library selects its NEON compression
 * functions; the host reference is built with the portable C path (BLAKE3_NO_* defines in
 * differential.json), so agreement also compares NEON against portable code.
 * Two official test vectors (empty input, one byte of the 251-byte counting pattern) are
 * checked against constants from the BLAKE3 test_vectors.json as an independent check. */
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include "blake3.h"

static void hex(FILE *o, const char *label, const uint8_t *p, size_t n) {
    fprintf(o, "%s=", label);
    for (size_t i = 0; i < n; i++) fprintf(o, "%02x", p[i]);
    fputc('\n', o);
}

static void fill(uint8_t *b, size_t n) { for (size_t i = 0; i < n; i++) b[i] = (uint8_t)(i % 251); }

int b3_run_cases(FILE *o) {
    static uint8_t in[102400];
    uint8_t out[131], key[BLAKE3_KEY_LEN];
    int fails = 0;
    fill(in, sizeof in);
    for (int i = 0; i < BLAKE3_KEY_LEN; i++) key[i] = (uint8_t)("whats the Elvish word for friend"[i]);

    static const size_t lens[] = {0, 1, 2, 3, 4, 5, 7, 8, 63, 64, 65, 127, 128, 129, 1023, 1024, 1025, 2047, 2048,
                                  2049, 3072, 3073, 4095, 4096, 4097, 5120, 8191, 8192, 8193, 16384, 31744, 102400};
    for (size_t k = 0; k < sizeof lens / sizeof *lens; k++) {
        size_t n = lens[k];
        char label[48];
        blake3_hasher h;
        blake3_hasher_init(&h);
        blake3_hasher_update(&h, in, n);
        blake3_hasher_finalize(&h, out, 32);
        snprintf(label, sizeof label, "hash[%zu]", n);
        hex(o, label, out, 32);
        if (n == 0 || n == 1) {
            static const char *kat[2] = {"af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
                                         "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213"};
            char got[65];
            for (int i = 0; i < 32; i++) snprintf(got + 2 * i, 3, "%02x", out[i]);
            int ok = strcmp(got, kat[n]) == 0;
            fprintf(o, "official-vector[%zu]: %s\n", n, ok ? "PASS" : "FAIL");
            fails += !ok;
        }
        /* same input fed in odd-sized pieces must agree */
        blake3_hasher_init(&h);
        for (size_t at = 0, step = 1; at < n; at += step, step = step * 3 + 1) {
            size_t take = n - at < step ? n - at : step;
            blake3_hasher_update(&h, in + at, take);
        }
        uint8_t again[32];
        blake3_hasher_finalize(&h, again, 32);
        if (memcmp(again, out, 32)) { fprintf(o, "chunking-mismatch[%zu]\n", n); fails++; }
        if (n == 0 || n == 1 || n == 64 || n == 1025 || n == 8193 || n == 102400) {
            blake3_hasher_init_keyed(&h, key);
            blake3_hasher_update(&h, in, n);
            blake3_hasher_finalize(&h, out, 32);
            snprintf(label, sizeof label, "keyed[%zu]", n);
            hex(o, label, out, 32);
            blake3_hasher_init_derive_key(&h, "ounce-core differential context");
            blake3_hasher_update(&h, in, n);
            blake3_hasher_finalize(&h, out, 32);
            snprintf(label, sizeof label, "derive[%zu]", n);
            hex(o, label, out, 32);
        }
    }
    /* extendable output: 131 bytes, and seeking must match the stream */
    {
        blake3_hasher h;
        uint8_t seek[40];
        blake3_hasher_init(&h);
        blake3_hasher_update(&h, in, 2049);
        blake3_hasher_finalize(&h, out, sizeof out);
        hex(o, "xof[2049]", out, sizeof out);
        blake3_hasher_finalize_seek(&h, 64, seek, sizeof seek);
        int ok = memcmp(seek, out + 64, sizeof seek) == 0;
        fprintf(o, "xof-seek: %s\n", ok ? "PASS" : "FAIL");
        fails += !ok;
    }
    fprintf(o, "TOTAL FAILS=%d\n", fails);
    return fails;
}
