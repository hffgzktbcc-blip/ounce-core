/* SHA3IUF (MIT) differential cases: SHA3-256/384/512 and Keccak (pre-NIST padding) on generated
 * inputs around the rate boundaries (rate is 136/104/72 bytes), one-shot and incremental, plus the
 * NIST empty-string and "abc" SHA3-256 vectors as an independent check. */
#include <stdio.h>
#include <string.h>
#include "sha3.h"

static void hex(FILE *o, const char *label, const unsigned char *p, size_t n) {
    fprintf(o, "%s=", label);
    for (size_t i = 0; i < n; i++) fprintf(o, "%02x", p[i]);
    fputc('\n', o);
}

int sha3_run_cases(FILE *o) {
    static unsigned char data[2000];
    int fails = 0;
    for (size_t i = 0; i < sizeof data; i++) data[i] = (unsigned char)(i * 37 + (i >> 4));
    static const size_t lens[] = {0, 1, 2, 71, 72, 73, 103, 104, 105, 135, 136, 137, 143, 144, 200, 271, 272, 273, 1000, 1999};
    static const unsigned bits[] = {256, 384, 512};
    for (unsigned b = 0; b < 3; b++)
        for (int keccak = 0; keccak < 2; keccak++)
            for (size_t k = 0; k < sizeof lens / sizeof *lens; k++) {
                size_t n = lens[k];
                sha3_context c;
                sha3_Init(&c, bits[b]);
                if (keccak) sha3_SetFlags(&c, SHA3_FLAGS_KECCAK);
                size_t half = n / 3;
                sha3_Update(&c, data, half);
                sha3_Update(&c, data + half, n - half);
                const unsigned char *h = sha3_Finalize(&c);
                unsigned char again[64];
                sha3_HashBuffer(bits[b], keccak ? SHA3_FLAGS_KECCAK : SHA3_FLAGS_NONE, data, n, again, bits[b] / 8);
                char label[40];
                snprintf(label, sizeof label, "%s%u[%zu]", keccak ? "keccak" : "sha3-", bits[b], n);
                hex(o, label, h, bits[b] / 8);
                if (memcmp(h, again, bits[b] / 8)) { fprintf(o, "one-shot-mismatch %s\n", label); fails++; }
            }
    {
        unsigned char h[32];
        sha3_HashBuffer(256, SHA3_FLAGS_NONE, "", 0, h, 32);
        int ok = !memcmp(h, "\xa7\xff\xc6\xf8\xbf\x1e\xd7\x66\x51\xc1\x47\x56\xa0\x61\xd6\x62\xf5\x80\xff\x4d\xe4\x3b\x49\xfa\x82\xd8\x0a\x4b\x80\xf8\x43\x4a", 32);
        fprintf(o, "known-answer sha3-256(empty): %s\n", ok ? "PASS" : "FAIL"); fails += !ok;
        sha3_HashBuffer(256, SHA3_FLAGS_NONE, "abc", 3, h, 32);
        ok = !memcmp(h, "\x3a\x98\x5d\xa7\x4f\xe2\x25\xb2\x04\x5c\x17\x2d\x6b\xd3\x90\xbd\x85\x5f\x08\x6e\x3e\x9d\x52\x5b\x46\xbf\xe2\x45\x11\x43\x15\x32", 32);
        fprintf(o, "known-answer sha3-256(abc): %s\n", ok ? "PASS" : "FAIL"); fails += !ok;
    }
    fprintf(o, "TOTAL FAILS=%d\n", fails);
    return fails;
}
