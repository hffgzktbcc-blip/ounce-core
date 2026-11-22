// RC5 from Rivest, "The RC5 Encryption Algorithm" (FSE 1994), written from the paper's description of RC5-32/r/b: data-dependent
// rotations, a key expansion that mixes the secret key into the table S three times over, and the magic constants P and Q.
// Checks the 29 RC5-CBC / RC5-CBC-Pad test vectors of RFC 2040 (Baldwin and Rivest, 1996), section 9.3 (32-bit words, r = 0 to
// 16 rounds, keys of 1 to 16 bytes). Vectors and the constants P32, Q32 are data copied as numbers from the RFC; the RFC's
// test program and reference code were not copied.
#include "ounce_paper.h"

typedef struct { const char *name; int pad, rounds; const char *key, *iv, *pt, *ct; } vec;
// name, 1 if RC5-CBC-Pad, rounds r, key, IV, plaintext, ciphertext, all as byte strings (RFC 2040 section 9.3)
static const vec VEC[] = {
    {"RC5-32/0/1 #1", 0, 0, "00", "0000000000000000", "0000000000000000", "7a7bba4d79111d1e"},
    {"RC5-32/0/1 #2", 0, 0, "00", "0000000000000000", "ffffffffffffffff", "797bba4d78111d1e"},
    {"RC5-32/0/1 #3", 0, 0, "00", "0000000000000001", "0000000000000000", "7a7bba4d79111d1f"},
    {"RC5-32/0/1 #4", 0, 0, "00", "0000000000000000", "0000000000000001", "7a7bba4d79111d1f"},
    {"RC5-32/0/1 #5", 0, 0, "00", "0102030405060708", "1020304050607080", "8b9ded91ce7794a6"},
    {"RC5-32/1/1 #6", 0, 1, "11", "0000000000000000", "0000000000000000", "2f759fe7ad86a378"},
    {"RC5-32/2/1 #7", 0, 2, "00", "0000000000000000", "0000000000000000", "dca2694bf40e0788"},
    {"RC5-32/2/4 #8", 0, 2, "00000000", "0000000000000000", "0000000000000000", "dca2694bf40e0788"},
    {"RC5-32/8/1 #9", 0, 8, "00", "0000000000000000", "0000000000000000", "dcfe098577eca5ff"},
    {"RC5-32/8/1 #10", 0, 8, "00", "0102030405060708", "1020304050607080", "9646fb77638f9ca8"},
    {"RC5-32/12/1 #11", 0, 12, "00", "0102030405060708", "1020304050607080", "b2b3209db6594da4"},
    {"RC5-32/16/1 #12", 0, 16, "00", "0102030405060708", "1020304050607080", "545f7f32a5fc3836"},
    {"RC5-32/8/4 #13", 0, 8, "01020304", "0000000000000000", "ffffffffffffffff", "8285e7c1b5bc7402"},
    {"RC5-32/12/4 #14", 0, 12, "01020304", "0000000000000000", "ffffffffffffffff", "fc586f92f7080934"},
    {"RC5-32/16/4 #15", 0, 16, "01020304", "0000000000000000", "ffffffffffffffff", "cf270ef9717ff7c4"},
    {"RC5-32/12/8 #16", 0, 12, "0102030405060708", "0000000000000000", "ffffffffffffffff", "e493f1c1bb4d6e8c"},
    {"RC5-32/8/8 #17", 0, 8, "0102030405060708", "0102030405060708", "1020304050607080", "5c4c041e0f217ac3"},
    {"RC5-32/12/8 #18", 0, 12, "0102030405060708", "0102030405060708", "1020304050607080", "921f12485373b4f7"},
    {"RC5-32/16/8 #19", 0, 16, "0102030405060708", "0102030405060708", "1020304050607080", "5ba0ca6bbe7f5fad"},
    {"RC5-32/8/16 #20", 0, 8, "01020304050607081020304050607080", "0102030405060708", "1020304050607080", "c533771cd0110e63"},
    {"RC5-32/12/16 #21", 0, 12, "01020304050607081020304050607080", "0102030405060708", "1020304050607080", "294ddb46b3278d60"},
    {"RC5-32/16/16 #22", 0, 16, "01020304050607081020304050607080", "0102030405060708", "1020304050607080", "dad6bda9dfe8f7e8"},
    {"RC5-32/12/5 #23", 0, 12, "0102030405", "0000000000000000", "ffffffffffffffff", "97e0787837ed317f"},
    {"RC5-32/8/5 #24", 0, 8, "0102030405", "0000000000000000", "ffffffffffffffff", "7875dbf6738c6478"},
    {"RC5-32/8/5 #25", 0, 8, "0102030405", "7875dbf6738c6478", "0808080808080808", "8f34c3c681c99695"},
    {"RC5-32/8/5 pad #26", 1, 8, "0102030405", "0000000000000000", "ffffffffffffffff", "7875dbf6738c64788f34c3c681c99695"},
    {"RC5-32/8/5 #27", 0, 8, "0102030405", "0000000000000000", "0000000000000000", "7cb3f1df34f94811"},
    {"RC5-32/8/5 #28", 0, 8, "0102030405", "7cb3f1df34f94811", "1122334455667701", "7fd1a023a5bba217"},
    {"RC5-32/8/5 pad #29", 1, 8, "0102030405", "0000000000000000", "ffffffffffffffff7875dbf6738c647811223344556677", "7875dbf6738c64787cb3f1df34f948117fd1a023a5bba217"},
};
#define NVEC ((int)(sizeof VEC / sizeof VEC[0]))


#ifndef RC5_P32
#define RC5_P32 0xb7e15163u   // Odd((e - 2) * 2^32)
#endif
#define RC5_Q32 0x9e3779b9u   // Odd((phi - 1) * 2^32)

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
static u32 rol(u32 x, u32 r) { r &= 31; return r ? (x << r) | (x >> (32 - r)) : x; }
static u32 le32(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }
static void put_le32(u8 *p, u32 v) { p[0] = (u8)v; p[1] = (u8)(v >> 8); p[2] = (u8)(v >> 16); p[3] = (u8)(v >> 24); }

typedef struct { int r; u32 S[2 * 16 + 2]; } rc5_ctx;

static void rc5_setkey(rc5_ctx *c, const u8 *key, int b, int r) {
    u32 L[64]; int cw = b ? (b + 3) / 4 : 1, t = 2 * (r + 1);
    c->r = r;
    for (int i = 0; i < cw; i++) L[i] = 0;
    for (int i = b - 1; i >= 0; i--) L[i / 4] = (L[i / 4] << 8) + key[i];
    c->S[0] = RC5_P32;
    for (int i = 1; i < t; i++) c->S[i] = c->S[i - 1] + RC5_Q32;
    u32 A = 0, B = 0; int i = 0, j = 0, n = 3 * (t > cw ? t : cw);
    for (int k = 0; k < n; k++) {
        A = c->S[i] = rol(c->S[i] + A + B, 3);
        B = L[j] = rol(L[j] + A + B, A + B);
        i = (i + 1) % t; j = (j + 1) % cw;
    }
}
static void rc5_enc(const rc5_ctx *c, const u8 *in, u8 *out) {
    u32 A = le32(in) + c->S[0], B = le32(in + 4) + c->S[1];
    for (int i = 1; i <= c->r; i++) {
        A = rol(A ^ B, B) + c->S[2 * i];
        B = rol(B ^ A, A) + c->S[2 * i + 1];
    }
    put_le32(out, A); put_le32(out + 4, B);
}

int paper_main(void) {
    int good = 0, npad = 0, goodpad = 0;
    for (int v = 0; v < NVEC; v++) {
        const vec *e = &VEC[v];
        u8 key[32], iv[8], pt[64], want[64], got[64];
        int kn = hexbytes(e->key, key);
        int n = hexbytes(e->pt, pt), cn = hexbytes(e->ct, want);
        if (hexbytes(e->iv, iv) != 8) { pstr("FAIL bad iv size\n"); return 1; }
        if (e->pad) {   // RC5-CBC-Pad: append 1 to 8 bytes, each equal to the pad length, to reach a multiple of 8
            int p = 8 - (n & 7); for (int i = 0; i < p; i++) pt[n + i] = (u8)p; n += p;
        }
        if ((n & 7) || n != cn) { pstr("FAIL bad vector sizes\n"); return 1; }
        rc5_ctx c; rc5_setkey(&c, key, kn, e->rounds);
        u8 prev[8]; for (int i = 0; i < 8; i++) prev[i] = iv[i];
        for (int b = 0; b < n; b += 8) {
            u8 t[8]; for (int i = 0; i < 8; i++) t[i] = pt[b + i] ^ prev[i];
            rc5_enc(&c, t, got + b); for (int i = 0; i < 8; i++) prev[i] = got[b + i];
        }
        int m = pcheck_bytes(e->name, got, want, n);
        good += m;
        if (e->pad) { npad++; goodpad += m; }
    }
    pstr("RFC 2040 vectors: "); pdec(good); pstr(" of "); pdec(NVEC); pstr(" match (");
    pdec(goodpad); pstr(" of "); pdec(npad); pstr(" with padding)\n");
    int ok = good == NVEC;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
