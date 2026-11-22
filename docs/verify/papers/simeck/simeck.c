// Simeck from Yang, Zhu, Suder, Aagaard, Gong, "The Simeck Family of Lightweight Block Ciphers" (CHES 2015), written from the
// paper's description: a Feistel round with f(x) = (x & (x <<< 5)) ^ (x <<< 1), and a key schedule that runs the same round
// function on the key words with a round constant. Checks the paper's own test vector for Simeck32/64 and for Simeck64/128,
// and 18 more vectors (9 per variant) that the Crypto++ test-vector file labels "SIMECK reference implementation", plus a
// decrypt round trip for each. Vectors and the two constant sequences are data copied as numbers; no code was copied.
#include "ounce_paper.h"

typedef struct { const char *name; int n, from_paper; const char *key, *pt, *ct; } vec;
// name, word bits n, 1 if the entry is the paper's own vector, key t3 t2 t1 t0, plaintext (x y), ciphertext (paper word order)
static const vec VEC[] = {
    {"Simeck32/64 paper vector", 16, 1, "1918 1110 0908 0100", "6565 6877", "770d 2c76"},
    {"Simeck32/64 reference impl #1", 16, 0, "3d6c 4ae1 6784 18be", "4823 0029", "6535 9de9"},
    {"Simeck32/64 reference impl #2", 16, 0, "6df1 1649 5f90 6952", "72ae 2cd6", "0ab0 73ca"},
    {"Simeck32/64 reference impl #3", 16, 0, "2ea6 0bb3 01eb 26e9", "41bb 5af1", "6ed0 bc2e"},
    {"Simeck32/64 reference impl #4", 16, 0, "0099 0f3e 390c 7e87", "153c 12db", "7637 4119"},
    {"Simeck32/64 reference impl #5", 16, 0, "4db7 4d06 491c 440d", "305e 0124", "8252 aa91"},
    {"Simeck32/64 reference impl #6", 16, 0, "4dc8 074d 2d12 39b3", "54de 1547", "e288 e7ea"},
    {"Simeck32/64 reference impl #7", 16, 0, "5d03 701f 26a6 428b", "66bb 6443", "b730 99ae"},
    {"Simeck32/64 reference impl #8", 16, 0, "1e1f 3b25 1238 4509", "767d 7a5a", "058a 62df"},
    {"Simeck32/64 reference impl #9", 16, 0, "7ff5 7f96 6bfc 63cb", "1ad4 6e5d", "60c4 43f2"},
    {"Simeck64/128 paper vector", 32, 1, "1b1a1918 13121110 0b0a0908 03020100", "656b696c 20646e75", "45ce6902 5f7ab7ed"},
    {"Simeck64/128 reference impl #1", 32, 0, "0938251f 43bb8ba6 06b747de 870c3e99", "f1bbe9eb e16cd6ae", "4d11c6b9 da2f7e28"},
    {"Simeck64/128 reference impl #2", 32, 0, "323ba122 444066d0 9e7d49dc 407836fd", "1cdbae32 96f5453b", "1e6a0792 f5a717c5"},
    {"Simeck64/128 reference impl #3", 32, 0, "61ff698f 2ddc8e66 53bf67d6 99d5e980", "b9729d49 e18b1fda", "fca0fa81 94bda9c7"},
    {"Simeck64/128 reference impl #4", 32, 0, "cfd3902d 597e35cf 9e0cf4d5 2c53cbc9", "844f4a77 9d9c1672", "562b1caa 75266241"},
    {"Simeck64/128 reference impl #5", 32, 0, "f8466a04 6454ceb1 3b33821f d4618dbe", "78818744 e6d91d2a", "d946fa49 41516d8e"},
    {"Simeck64/128 reference impl #6", 32, 0, "97278a59 28ce0bf5 2543e53c adae2488", "d0576876 162f6768", "ca3e5050 126fa61b"},
    {"Simeck64/128 reference impl #7", 32, 0, "a786c2b5 c19be1c0 978c2ff1 1128c18c", "08614014 c9cd68d4", "a307ab5a a10f5c29"},
    {"Simeck64/128 reference impl #8", 32, 0, "63b126df 89a98279 0c9bb447 9cfed971", "d96ca166 d923d155", "5e47b40d 9854418a"},
    {"Simeck64/128 reference impl #9", 32, 0, "463608dc 1b2861c9 3f410784 28a11e20", "3f895ef1 62e09612", "c5fd5a6c 32056800"},
};
#define NVEC ((int)(sizeof VEC / sizeof VEC[0]))


// Round-constant bit sequences, bit i is used in round i (32 bits for Simeck32/64, 44 bits for Simeck64/128).
// Transcribed as hexadecimal numbers from the Crypto++ source file simeck.cpp (see paper.json).
#ifndef SIMECK_SEQ32
#define SIMECK_SEQ32 0x9a42bb1fULL
#endif
#define SIMECK_SEQ64 0x938bca3083fULL

#ifndef SIMECK_ROT5
#define SIMECK_ROT5 5
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
static u64 word_be(const u8 *p, int nbytes) { u64 r = 0; for (int i = 0; i < nbytes; i++) r = (r << 8) | p[i]; return r; }
static void put_be(u8 *p, int nbytes, u64 w) { for (int i = nbytes - 1; i >= 0; i--) { p[i] = (u8)w; w >>= 8; } }

static u64 mask_of(int n) { return (((u64)1 << n) - 1); }
static u64 rol(u64 x, int r, int n) { return ((x << r) | (x >> (n - r))) & mask_of(n); }
// One Feistel round with round key k: (left, right) -> (f(left) ^ right ^ k, left)
static void round_fn(int n, u64 k, u64 *l, u64 *r) {
    u64 t = *l;
    *l = ((*l & rol(*l, SIMECK_ROT5, n)) ^ rol(*l, 1, n) ^ *r ^ k) & mask_of(n);
    *r = t;
}

// Key words come in paper order t3 t2 t1 t0 (four words for both variants).
static void simeck_keys(int n, int T, u64 seq, const u64 *kw, u64 *rk) {
    u64 t[4] = { kw[3], kw[2], kw[1], kw[0] };
    u64 c = mask_of(n) ^ 3;   // 2^n - 4
    for (int i = 0; i < T; i++) {
        rk[i] = t[0];
        u64 cons = c | ((seq >> i) & 1);
        u64 l = t[1], r = t[0];
        round_fn(n, cons, &l, &r);
        t[0] = r; t[1] = t[2]; t[2] = t[3]; t[3] = l;   // new t0 = old t1, new t3 = f(old t1) ^ old t0 ^ constant
    }
}

int paper_main(void) {
    int good = 0, dec_good = 0, paper_good = 0, paper_total = 0;
    for (int v = 0; v < NVEC; v++) {
        const vec *e = &VEC[v];
        int n = e->n, T = n == 16 ? 32 : 44, wb = n / 8;
        u64 seq = n == 16 ? SIMECK_SEQ32 : SIMECK_SEQ64;
        u8 kb[16], pb[8], cb[8];
        int kn = hexbytes(e->key, kb), pn = hexbytes(e->pt, pb); hexbytes(e->ct, cb);
        if (kn != 4 * wb || pn != 2 * wb) { pstr("FAIL bad vector sizes\n"); return 1; }
        u64 kw[4], rk[44];
        for (int i = 0; i < 4; i++) kw[i] = word_be(kb + i * wb, wb);
        simeck_keys(n, T, seq, kw, rk);
        u64 l = word_be(pb, wb), r = word_be(pb + wb, wb);
        for (int i = 0; i < T; i++) round_fn(n, rk[i], &l, &r);
        u8 got[8]; put_be(got, wb, l); put_be(got + wb, wb, r);
        int m = pcheck_bytes(e->name, got, cb, 2 * wb);
        good += m;
        if (e->from_paper) { paper_total++; paper_good += m; }
        // decrypt: undo the rounds (the Feistel step is inverted by swapping halves, applying the round, swapping back)
        u64 a = r, b = l;
        for (int i = T - 1; i >= 0; i--) round_fn(n, rk[i], &a, &b);
        if (a == word_be(pb + wb, wb) && b == word_be(pb, wb)) dec_good++;
    }
    pstr("paper's own vectors: "); pdec(paper_good); pstr(" of "); pdec(paper_total); pstr(" match\n");
    pstr("all vectors: "); pdec(good); pstr(" of "); pdec(NVEC); pstr(" match\n");
    pstr("decrypt round trips: "); pdec(dec_good); pstr(" of "); pdec(NVEC); pstr("\n");
    int ok = good == NVEC && dec_good == NVEC;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
