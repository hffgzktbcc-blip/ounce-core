// Speck from Beaulieu, Shors, Smith, Treatman-Clark, Weeks, Wingers, "The SIMON and SPECK Families of Lightweight Block
// Ciphers" (IACR ePrint 2013/404), written from the paper's description of the round function and key schedule.
// Checks the five Speck known-answer vectors of the paper's appendix that have a public transcription (Speck64/96, 64/128,
// 128/128, 128/192, 128/256), in the paper's printed word order (key words l_{m-2}..l_0 k_0, block words x y), plus a decrypt
// round trip for each. The vectors are data copied as numbers; none of the NSA authors' code was copied.
#include "ounce_paper.h"

typedef struct { const char *name; int n, m, T; const char *key, *pt, *ct; } vec;
// name, word bits n, key words m, rounds T, key, plaintext, ciphertext (paper word order)
static const vec VEC[] = {
    {"Speck64/96", 32, 3, 26, "13121110 0b0a0908 03020100", "74614620 736e6165", "9f7952ec 4175946c"},
    {"Speck64/128", 32, 4, 27, "1b1a1918 13121110 0b0a0908 03020100", "3b726574 7475432d", "8c6fa548 454e028b"},
    {"Speck128/128", 64, 2, 32, "0f0e0d0c0b0a0908 0706050403020100", "6c61766975716520 7469206564616d20", "a65d985179783265 7860fedf5c570d18"},
    {"Speck128/192", 64, 3, 33, "1716151413121110 0f0e0d0c0b0a0908 0706050403020100", "7261482066656968 43206f7420746e65", "1be4cf3a13135566 f9bc185de03c1886"},
    {"Speck128/256", 64, 4, 34, "1f1e1d1c1b1a1918 1716151413121110 0f0e0d0c0b0a0908 0706050403020100", "65736f6874206e49 202e72656e6f6f70", "4109010405c0f53e 4eeeb48d9c188f43"},
};
#define NVEC ((int)(sizeof VEC / sizeof VEC[0]))


#ifndef SPECK_ALPHA
#define SPECK_ALPHA 8   // rotation constants for block sizes >= 48 bits (the 32-bit block uses 7 and 2)
#endif
#ifndef SPECK_BETA
#define SPECK_BETA 3
#endif

static int hexbytes(const char *s, u8 *out) {   // hex digits (spaces ignored) -> bytes; returns byte count
    int n = 0, half = 0, cur = 0;
    for (; *s; s++) {
        int d;
        if (*s >= '0' && *s <= '9') d = *s - '0'; else if (*s >= 'a' && *s <= 'f') d = *s - 'a' + 10; else continue;
        cur = cur * 16 + d;
        if (half) { out[n++] = (u8)cur; cur = 0; }
        half ^= 1;
    }
    return n;
}
// Big-endian bytes (as printed in the paper) -> one n-bit word.
static u64 word_be(const u8 *p, int nbytes) { u64 r = 0; for (int i = 0; i < nbytes; i++) r = (r << 8) | p[i]; return r; }
static void put_be(u8 *p, int nbytes, u64 w) { for (int i = nbytes - 1; i >= 0; i--) { p[i] = (u8)w; w >>= 8; } }

typedef struct { int n, m, T; } speck_params;   // word size n bits, key words m, rounds T

static u64 mask_of(int n) { return n == 64 ? ~(u64)0 : (((u64)1 << n) - 1); }
static u64 ror(u64 x, int r, int n) { return ((x >> r) | (x << (n - r))) & mask_of(n); }
static u64 rol(u64 x, int r, int n) { return ((x << r) | (x >> (n - r))) & mask_of(n); }

static void speck_keys(const speck_params *p, const u64 *keywords /* paper order: l_{m-2} .. l_0, k_0 */, u64 *rk) {
    int n = p->n, m = p->m; u64 mk = mask_of(n);
    u64 l[64 + 8]; u64 k = keywords[m - 1];
    for (int i = 0; i < m - 1; i++) l[i] = keywords[m - 2 - i];   // l[0] = l_0 ...
    for (int i = 0; i < p->T; i++) {
        rk[i] = k;
        if (i == p->T - 1) break;
        l[i + m - 1] = ((k + ror(l[i], SPECK_ALPHA, n)) & mk) ^ (u64)i;
        k = rol(k, SPECK_BETA, n) ^ l[i + m - 1];
    }
}
static void speck_enc(const speck_params *p, const u64 *rk, u64 *x, u64 *y) {
    int n = p->n; u64 mk = mask_of(n);
    for (int i = 0; i < p->T; i++) { *x = ((ror(*x, SPECK_ALPHA, n) + *y) & mk) ^ rk[i]; *y = rol(*y, SPECK_BETA, n) ^ *x; }
}
static void speck_dec(const speck_params *p, const u64 *rk, u64 *x, u64 *y) {
    int n = p->n; u64 mk = mask_of(n);
    for (int i = p->T - 1; i >= 0; i--) { *y = ror(*y ^ *x, SPECK_BETA, n); *x = rol(((*x ^ rk[i]) - *y) & mk, SPECK_ALPHA, n); }
}

int paper_main(void) {
    int ok = 1, good = 0, dec_good = 0;
    for (int v = 0; v < NVEC; v++) {
        const vec *t = &VEC[v];
        speck_params p = { t->n, t->m, t->T };
        int wb = p.n / 8;
        u8 kb[64], pb[16], cb[16];
        int kn = hexbytes(t->key, kb), pn = hexbytes(t->pt, pb); hexbytes(t->ct, cb);
        if (kn != p.m * wb || pn != 2 * wb) { pstr("FAIL bad vector sizes\n"); return 1; }
        u64 kw[8], rk[64];
        for (int i = 0; i < p.m; i++) kw[i] = word_be(kb + i * wb, wb);
        speck_keys(&p, kw, rk);
        u64 x = word_be(pb, wb), y = word_be(pb + wb, wb);
        speck_enc(&p, rk, &x, &y);
        u8 got[16]; put_be(got, wb, x); put_be(got + wb, wb, y);
        int m = pcheck_bytes(t->name, got, cb, 2 * wb);
        good += m;
        u64 dx = x, dy = y; speck_dec(&p, rk, &dx, &dy);
        if (dx == word_be(pb, wb) && dy == word_be(pb + wb, wb)) dec_good++;
    }
    pstr("known-answer vectors: "); pdec(good); pstr(" of "); pdec(NVEC); pstr(" match\n");
    pstr("decrypt round trips: "); pdec(dec_good); pstr(" of "); pdec(NVEC); pstr("\n");
    ok = good == NVEC && dec_good == NVEC;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
