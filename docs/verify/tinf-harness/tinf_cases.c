/* tinf (zlib-licensed inflate) differential cases: raw/zlib/gzip streams made once with Python's
 * zlib (tinf_data.h), decoded here; plus bit-flipped, truncated and undersized-output variants
 * (error codes and the partial output hash are the observation), and tinf's CRC-32 / Adler-32. */
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include "tinf.h"
#include "tinf_data.h"

static uint32_t fnv(const uint8_t *p, size_t n) { uint32_t h = 2166136261u; while (n--) h = (h ^ *p++) * 16777619u; return h; }

struct item { const char *name; const unsigned char *data; unsigned len; unsigned plain; const char *kind; };
static const struct item items[] = {
    {"text-raw", d_text_raw, sizeof d_text_raw, 2900, "raw"},
    {"text-zlib", d_text_zlib, sizeof d_text_zlib, 2900, "zlib"},
    {"text-gzip", d_text_gzip, sizeof d_text_gzip, 2900, "gzip"},
    {"text-fixed", d_text_fixed, sizeof d_text_fixed, 2900, "fixed"},
    {"pattern-zlib", d_pattern_zlib, sizeof d_pattern_zlib, 5000, "zlib"},
    {"pattern-fixed", d_pattern_fixed, sizeof d_pattern_fixed, 5000, "fixed"},
    {"zeros-gzip", d_zeros_gzip, sizeof d_zeros_gzip, 20000, "gzip"},
    {"noise-stored", d_noise_stored, sizeof d_noise_stored, 3000, "stored"},
    {"mixed-raw", d_mixed_raw, sizeof d_mixed_raw, 1343, "raw"},
    {"mixed-zlib", d_mixed_zlib, sizeof d_mixed_zlib, 1343, "zlib"},
};

static int decode(const struct item *it, const unsigned char *src, unsigned srclen, unsigned char *dst, unsigned *dlen) {
    if (!strcmp(it->kind, "gzip")) return tinf_gzip_uncompress(dst, dlen, src, srclen);
    if (!strcmp(it->kind, "zlib")) return tinf_zlib_uncompress(dst, dlen, src, srclen);
    return tinf_uncompress(dst, dlen, src, srclen);
}

int tinf_run_cases(FILE *o) {
    static unsigned char out[21000], mut[6000];
    int fails = 0;
    tinf_init();
    for (unsigned i = 0; i < sizeof items / sizeof *items; i++) {
        const struct item *it = &items[i];
        unsigned dlen = sizeof out;
        memset(out, 0, sizeof out);
        int r = decode(it, it->data, it->len, out, &dlen);
        int ok = r == TINF_OK && dlen == it->plain;
        fails += !ok;
        fprintf(o, "%s: rc=%d len=%u fnv=%08x crc=%08x adler=%08x %s\n", it->name, r, dlen, fnv(out, dlen),
                tinf_crc32(out, dlen), tinf_adler32(out, dlen), ok ? "ok" : "FAIL");
        /* output buffer one byte too small */
        dlen = it->plain - 1;
        r = decode(it, it->data, it->len, out, &dlen);
        fprintf(o, "  small-buffer rc=%d\n", r);
        /* truncated input */
        dlen = sizeof out;
        r = decode(it, it->data, it->len / 2, out, &dlen);
        fprintf(o, "  truncated rc=%d\n", r);
        /* flip single bits at spread positions */
        for (unsigned f = 0; f < 6 && it->len <= sizeof mut; f++) {
            memcpy(mut, it->data, it->len);
            unsigned pos = (it->len * (f + 1)) / 7;
            mut[pos] ^= (unsigned char)(1u << (f % 8));
            dlen = sizeof out;
            r = decode(it, mut, it->len, out, &dlen);
            fprintf(o, "  flip@%u rc=%d len=%u fnv=%08x\n", pos, r, r == TINF_OK ? dlen : 0u, r == TINF_OK ? fnv(out, dlen) : 0u);
        }
    }
    static const char *s = "123456789";
    fprintf(o, "crc32(123456789)=%08x (check value cbf43926) adler32=%08x (check value 091e01de)\n",
            tinf_crc32(s, 9), tinf_adler32(s, 9));
    fails += tinf_crc32(s, 9) != 0xcbf43926u || tinf_adler32(s, 9) != 0x091e01deu;
    fprintf(o, "TOTAL FAILS=%d\n", fails);
    return fails;
}
