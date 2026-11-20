// MD4 (Rivest, "The MD4 Message-Digest Algorithm", RFC 1320, April 1992), written from the RFC's algorithm description.
// Checks the 7 test-suite digests printed in RFC 1320 section A.5. The digests are data copied as numbers; no code from the
// RFC's reference implementation was copied. Exit 0 only if all 7 match.
// Negative control: -DMD4_K3=0x6ed9eba2u changes one bit of the round-3 constant.
#include "ounce_paper.h"

struct tv { const char *msg; const char *md; };
static int hexval(char c) { return c <= '9' ? c - '0' : c - 'a' + 10; }
static unsigned long slen(const char *s) { unsigned long n = 0; while (s[n]) n++; return n; }
#define ROL(x, s) (((x) << (s)) | ((x) >> (32 - (s))))
static u32 ld32le(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }

#ifndef MD4_K2
#define MD4_K2 0x5a827999u
#endif
#ifndef MD4_K3
#define MD4_K3 0x6ed9eba1u
#endif
static const struct tv V[] = {
    {"", "31d6cfe0d16ae931b73c59d7e0c089c0"},
    {"a", "bde52cb31de33e46245e05fbdbd6fb24"},
    {"abc", "a448017aaf21d8525fc10ae87aa6729d"},
    {"message digest", "d9130a8164549fe818874806e1c7014b"},
    {"abcdefghijklmnopqrstuvwxyz", "d79e1c308aa5bbcdeea8ed63df412da9"},
    {"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", "043f8582f241db351ce627e153e7f0e4"},
    {"12345678901234567890123456789012345678901234567890123456789012345678901234567890", "e33b4ddc9c38f2199c3e7b164fcc0536"}
};
#define NV 7
#define F(x, y, z) (((x) & (y)) | (~(x) & (z)))
#define G(x, y, z) (((x) & (y)) | ((x) & (z)) | ((y) & (z)))
#define H(x, y, z) ((x) ^ (y) ^ (z))
// One RFC operation [abcd k s]: a = (a + f(b,c,d) + X[k] + K) <<< s, with the register roles rotating A,D,C,B.
static void compress(u32 st[4], const u8 *blk) {
    u32 X[16], v[4] = {st[0], st[1], st[2], st[3]};
    for (int i = 0; i < 16; i++) X[i] = ld32le(blk + 4 * i);
    static const int s1[4] = {3, 7, 11, 19}, s2[4] = {3, 5, 9, 13}, s3[4] = {3, 9, 11, 15};
    static const int k3[16] = {0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15};
    for (int i = 0; i < 48; i++) {
        int r = i & 3;                       // which register is updated: A, D, C, B
        int ia = (4 - r) & 3, ib = (5 - r) & 3, ic = (6 - r) & 3, id = (7 - r) & 3;   // indices into v for a,b,c,d
        u32 a = v[ia], b = v[ib], c = v[ic], d = v[id], t;
        if (i < 16) { t = a + F(b, c, d) + X[i]; t = ROL(t, s1[r]); }
        else if (i < 32) { int j = i - 16; int k = (j & 3) * 4 + (j >> 2); t = a + G(b, c, d) + X[k] + MD4_K2; t = ROL(t, s2[r]); }
        else { int j = i - 32; t = a + H(b, c, d) + X[k3[j]] + MD4_K3; t = ROL(t, s3[r]); }
        v[ia] = t;
    }
    for (int i = 0; i < 4; i++) st[i] += v[i];
}

// Padding as in the RFC: 0x80, zeros to 56 mod 64, then the bit length as a 64-bit little-endian value.
static void md4(const u8 *m, unsigned long len, u8 out[16]) {
    u32 s[4] = {0x67452301u, 0xefcdab89u, 0x98badcfeu, 0x10325476u};
    u8 blk[64];
    unsigned long done = 0;
    while (1) {
        unsigned long rem = len - done;
        int last = 0;
        if (rem >= 64) { for (int i = 0; i < 64; i++) blk[i] = m[done + i]; }
        else {
            for (unsigned long i = 0; i < rem; i++) blk[i] = m[done + i];
            blk[rem] = 0x80;
            for (unsigned long i = rem + 1; i < 64; i++) blk[i] = 0;
            if (rem < 56) { u64 bits = (u64)len * 8; for (int i = 0; i < 8; i++) blk[56 + i] = (u8)(bits >> (8 * i)); last = 1; }
        }
        compress(s, blk);
        if (last) break;
        if (rem >= 64) { done += 64; continue; }
        // padding spilled into a second block
        for (int i = 0; i < 56; i++) blk[i] = 0;
        { u64 bits = (u64)len * 8; for (int i = 0; i < 8; i++) blk[56 + i] = (u8)(bits >> (8 * i)); }
        compress(s, blk);
        break;
    }
    for (int i = 0; i < 4; i++) for (int j = 0; j < 4; j++) out[4 * i + j] = (u8)(s[i] >> (8 * j));
}

int paper_main(void) {
    int good = 0;
    for (int i = 0; i < NV; i++) {
        u8 want[16], got[16]; char name[96];
        for (int j = 0; j < 16; j++) want[j] = (u8)(hexval(V[i].md[2 * j]) * 16 + hexval(V[i].md[2 * j + 1]));
        md4((const u8 *)V[i].msg, slen(V[i].msg), got);
        // name: NAME("<first 24 chars of message>")
        int k = 0; const char *p = "MD4(\"";
        while (*p) name[k++] = *p++;
        for (int j = 0; j < 24 && V[i].msg[j]; j++) name[k++] = V[i].msg[j];
        if (slen(V[i].msg) > 24) { name[k++] = '.'; name[k++] = '.'; name[k++] = '.'; }
        name[k++] = '"'; name[k++] = ')'; name[k] = 0;
        good += pcheck_bytes(name, got, want, 16);
    }
    pstr("RFC 1320 test suite: "); pdec(good); pstr(" of "); pdec(NV); pstr(" match\n");
    pstr(good == NV ? "PASS\n" : "FAIL\n");
    return good == NV ? 0 : 1;
}
