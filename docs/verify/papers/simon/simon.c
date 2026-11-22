// Simon from Beaulieu, Shors, Smith, Treatman-Clark, Weeks, Wingers, "The SIMON and SPECK Families of Lightweight Block
// Ciphers" (IACR ePrint 2013/404), written from the paper's description of the Feistel round and key schedule.
// Checks the five Simon known-answer vectors of the paper's appendix that have a public transcription (Simon64/96, 64/128,
// 128/128, 128/192, 128/256), in the paper's printed word order (key words l_{m-2}..l_0 k_0, block words x y), plus a decrypt
// round trip for each. The vectors and the three key-schedule constant sequences z2, z3, z4 are data copied as numbers;
// none of the NSA authors' code was copied.
#include "ounce_paper.h"

typedef struct { const char *name; int n, m, T, j; const char *key, *pt, *ct; } vec;
// name, word bits n, key words m, rounds T, constant sequence index j, key, plaintext, ciphertext (paper word order)
static const vec VEC[] = {
    {"Simon64/96", 32, 3, 42, 2, "13121110 0b0a0908 03020100", "6f722067 6e696c63", "5ca2e27f 111a8fc8"},
    {"Simon64/128", 32, 4, 44, 3, "1b1a1918 13121110 0b0a0908 03020100", "656b696c 20646e75", "44c8fc20 b9dfa07a"},
    {"Simon128/128", 64, 2, 68, 2, "0f0e0d0c0b0a0908 0706050403020100", "6373656420737265 6c6c657661727420", "49681b1e1e54fe3f 65aa832af84e0bbc"},
    {"Simon128/192", 64, 3, 69, 3, "1716151413121110 0f0e0d0c0b0a0908 0706050403020100", "206572656874206e 6568772065626972", "c4ac61effcdc0d4f 6c9c8d6e2597b85b"},
    {"Simon128/256", 64, 4, 72, 4, "1f1e1d1c1b1a1918 1716151413121110 0f0e0d0c0b0a0908 0706050403020100", "74206e69206d6f6f 6d69732061207369", "8d2b5579afc8a3a0 3bf72a87efe7b868"},
};
#define NVEC ((int)(sizeof VEC / sizeof VEC[0]))


// Key-schedule constant sequences (62-bit, bit i = z_i, period 62); z2 serves 64/96 and 128/128, z3 serves 64/128 and
// 128/192, z4 serves 128/256. Transcribed as hexadecimal numbers from the Crypto++ source file simon.cpp (see paper.json).
static const u64 Z[5] = { 0, 0, 0x7369f885192c0ef5ULL, 0xfc2ce51207a635dbULL, 0xfdc94c3a046d678bULL };

#ifndef SIMON_FROT1
#define SIMON_FROT1 1   // f(x) = (S^1 x & S^8 x) ^ S^2 x
#endif
#define SIMON_FROT8 8
#define SIMON_FROT2 2

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
static u64 word_be(const u8 *p, int nbytes) { u64 r = 0; for (int i = 0; i < nbytes; i++) r = (r << 8) | p[i]; return r; }
static void put_be(u8 *p, int nbytes, u64 w) { for (int i = nbytes - 1; i >= 0; i--) { p[i] = (u8)w; w >>= 8; } }

typedef struct { int n, m, T, j; } simon_params;   // word size n bits, key words m, rounds T, constant sequence index j

static u64 mask_of(int n) { return n == 64 ? ~(u64)0 : (((u64)1 << n) - 1); }
static u64 ror(u64 x, int r, int n) { return ((x >> r) | (x << (n - r))) & mask_of(n); }
static u64 rol(u64 x, int r, int n) { return ((x << r) | (x >> (n - r))) & mask_of(n); }
static u64 fround(u64 x, int n) { return (rol(x, SIMON_FROT1, n) & rol(x, SIMON_FROT8, n)) ^ rol(x, SIMON_FROT2, n); }

static void simon_keys(const simon_params *p, const u64 *keywords /* paper order: l_{m-2} .. l_0, k_0 */, u64 *k) {
    int n = p->n, m = p->m; u64 c = mask_of(n) ^ 3;   // c = 2^n - 4
    for (int i = 0; i < m; i++) k[i] = keywords[m - 1 - i];
    for (int i = m; i < p->T; i++) {
        u64 t = ror(k[i - 1], 3, n);
        if (m == 4) t ^= k[i - 3];
        t ^= ror(t, 1, n);
        u64 z = (Z[p->j] >> ((i - m) % 62)) & 1;
        k[i] = c ^ z ^ k[i - m] ^ t;
    }
}
static void simon_enc(const simon_params *p, const u64 *k, u64 *x, u64 *y) {
    for (int i = 0; i < p->T; i++) { u64 t = *x; *x = *y ^ fround(*x, p->n) ^ k[i]; *y = t; }
}
static void simon_dec(const simon_params *p, const u64 *k, u64 *x, u64 *y) {
    for (int i = p->T - 1; i >= 0; i--) { u64 t = *y; *y = *x ^ fround(*y, p->n) ^ k[i]; *x = t; }
}

int paper_main(void) {
    int ok = 1, good = 0, dec_good = 0;
    for (int v = 0; v < NVEC; v++) {
        const vec *t = &VEC[v];
        simon_params p = { t->n, t->m, t->T, t->j };
        int wb = p.n / 8;
        u8 kb[64], pb[16], cb[16];
        int kn = hexbytes(t->key, kb), pn = hexbytes(t->pt, pb); hexbytes(t->ct, cb);
        if (kn != p.m * wb || pn != 2 * wb) { pstr("FAIL bad vector sizes\n"); return 1; }
        u64 kw[8], rk[80];
        for (int i = 0; i < p.m; i++) kw[i] = word_be(kb + i * wb, wb);
        simon_keys(&p, kw, rk);
        u64 x = word_be(pb, wb), y = word_be(pb + wb, wb);
        simon_enc(&p, rk, &x, &y);
        u8 got[16]; put_be(got, wb, x); put_be(got + wb, wb, y);
        good += pcheck_bytes(t->name, got, cb, 2 * wb);
        u64 dx = x, dy = y; simon_dec(&p, rk, &dx, &dy);
        if (dx == word_be(pb, wb) && dy == word_be(pb + wb, wb)) dec_good++;
    }
    pstr("known-answer vectors: "); pdec(good); pstr(" of "); pdec(NVEC); pstr(" match\n");
    pstr("decrypt round trips: "); pdec(dec_good); pstr(" of "); pdec(NVEC); pstr("\n");
    ok = good == NVEC && dec_good == NVEC;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
