// MD5 (Rivest, "The MD5 Message-Digest Algorithm", RFC 1321, April 1992), written from the RFC's algorithm description.
// Checks the 7 test-suite digests printed in RFC 1321 section A.5. The digests and the 64 table constants T[i] (the RFC's
// integer part of 2^32*abs(sin(i))) are data copied as numbers; no code from the RFC's reference implementation was copied.
// Exit 0 only if all 7 match. -DMD5_ROUNDS=3 is the negative control (drops round 4).
#include "ounce_paper.h"

struct tv { const char *msg; const char *md; };
static int hexval(char c) { return c <= '9' ? c - '0' : c - 'a' + 10; }
static unsigned long slen(const char *s) { unsigned long n = 0; while (s[n]) n++; return n; }
#define ROL(x, s) (((x) << (s)) | ((x) >> (32 - (s))))
static u32 ld32le(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }

#ifndef MD5_ROUNDS
#define MD5_ROUNDS 4
#endif
static const struct tv V[] = {
    {"", "d41d8cd98f00b204e9800998ecf8427e"},
    {"a", "0cc175b9c0f1b6a831c399e269772661"},
    {"abc", "900150983cd24fb0d6963f7d28e17f72"},
    {"message digest", "f96b697d7cb7938d525a2f31aaf161d0"},
    {"abcdefghijklmnopqrstuvwxyz", "c3fcd3d76192e4007dfb496cca67e13b"},
    {"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", "d174ab98d277d9f5a5611c2c9f419d9f"},
    {"12345678901234567890123456789012345678901234567890123456789012345678901234567890", "57edf4a22be3c955ac49da2e2107b67a"}
};
#define NV 7
static const u32 T[64] = {
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee,
    0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
    0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa,
    0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
    0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
    0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05,
    0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039,
    0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
    0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391
};
static const int S[4][4] = {{7, 12, 17, 22}, {5, 9, 14, 20}, {4, 11, 16, 23}, {6, 10, 15, 21}};
// Operation [abcd k s i]: a = b + ((a + f(b,c,d) + X[k] + T[i]) <<< s); roles of a,b,c,d rotate each step (A D C B).
static void compress(u32 st[4], const u8 *blk) {
    u32 X[16], v[4] = {st[0], st[1], st[2], st[3]};
    for (int i = 0; i < 16; i++) X[i] = ld32le(blk + 4 * i);
    for (int i = 0; i < 16 * MD5_ROUNDS; i++) {
        int rd = i >> 4, j = i & 15, r = i & 3;
        int ia = (4 - r) & 3, ib = (5 - r) & 3, ic = (6 - r) & 3, id = (7 - r) & 3;
        u32 a = v[ia], b = v[ib], c = v[ic], d = v[id], f; int k;
        if (rd == 0) { f = (b & c) | (~b & d); k = j; }
        else if (rd == 1) { f = (b & d) | (c & ~d); k = (1 + 5 * j) & 15; }
        else if (rd == 2) { f = b ^ c ^ d; k = (5 + 3 * j) & 15; }
        else { f = c ^ (b | ~d); k = (7 * j) & 15; }
        u32 t = a + f + X[k] + T[i];
        v[ia] = b + ROL(t, S[rd][r]);
    }
    for (int i = 0; i < 4; i++) st[i] += v[i];
}

// Padding as in the RFC: 0x80, zeros to 56 mod 64, then the bit length as a 64-bit little-endian value.
static void md5(const u8 *m, unsigned long len, u8 out[16]) {
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
        md5((const u8 *)V[i].msg, slen(V[i].msg), got);
        // name: NAME("<first 24 chars of message>")
        int k = 0; const char *p = "MD5(\"";
        while (*p) name[k++] = *p++;
        for (int j = 0; j < 24 && V[i].msg[j]; j++) name[k++] = V[i].msg[j];
        if (slen(V[i].msg) > 24) { name[k++] = '.'; name[k++] = '.'; name[k++] = '.'; }
        name[k++] = '"'; name[k++] = ')'; name[k] = 0;
        good += pcheck_bytes(name, got, want, 16);
    }
    pstr("RFC 1321 test suite: "); pdec(good); pstr(" of "); pdec(NV); pstr(" match\n");
    pstr(good == NV ? "PASS\n" : "FAIL\n");
    return good == NV ? 0 : 1;
}
