// RIPEMD-160 from Dobbertin, Bosselaers and Preneel, "RIPEMD-160: a strengthened version of RIPEMD" (Fast Software
// Encryption 1996, LNCS 1039), written from the pseudo-code of the paper's Appendix A. The message-word selections r, r',
// rotation amounts s, s' and constants K, K' are the paper's Appendix A tables (data, extracted as numbers). Checks the 9
// test values of the paper's Appendix B: "", "a", "abc", "message digest", the lowercase alphabet, the 56-character
// message, upper+lower+digits, 8 times "1234567890", and 1 million times "a". Exit 0 only if all 9 match.
// Negative control: -DRMD_S0=12 changes the first left-line rotation amount s(0) from 11 to 12.
#include "ounce_paper.h"

struct tv { const char *msg; u32 rep; const char *md; };
static const struct tv V[] = {
    {"", 1, "9c1185a5c5e9fc54612808977ee8f548b2258d31"},
    {"a", 1, "0bdc9d2d256b3ee9daae347be6f4dc835a467ffe"},
    {"abc", 1, "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc"},
    {"message digest", 1, "5d0689ef49d2fae572b881b123a85ffa21595f36"},
    {"abcdefghijklmnopqrstuvwxyz", 1, "f71c27109c692c1b56bbdceb5b9d2865b3708dbc"},
    {"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq", 1, "12a053384a9c0c88e405a06c27dcf49ada62eb2b"},
    {"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", 1, "b0e20b6e3116640286ed3a87a5713079b21f5189"},
    {"1234567890", 8, "9b752e45573d4b39f4dbd3323cab82bf63326bfb"},
    {"a", 1000000, "52783243c1697bdbe16d37f97f68f08325dc1528"}
};
#define NV 9
#ifndef RMD_S0
#define RMD_S0 11
#endif
static const u8 R[80]  = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 7, 4, 13, 1, 10, 6, 15, 3, 12, 0, 9, 5, 2, 14, 11, 8, 3, 10, 14, 4, 9, 15, 8, 1, 2, 7, 0, 6, 13, 11, 5, 12, 1, 9, 11, 10, 0, 8, 12, 4, 13, 3, 7, 15, 14, 5, 6, 2, 4, 0, 5, 9, 7, 12, 2, 10, 14, 1, 3, 8, 11, 6, 15, 13};
static const u8 RP[80] = {5, 14, 7, 0, 9, 2, 11, 4, 13, 6, 15, 8, 1, 10, 3, 12, 6, 11, 3, 7, 0, 13, 5, 10, 14, 15, 8, 12, 4, 9, 1, 2, 15, 5, 1, 3, 7, 14, 6, 9, 11, 8, 12, 2, 10, 0, 4, 13, 8, 6, 4, 1, 3, 11, 15, 0, 5, 12, 2, 13, 9, 7, 10, 14, 12, 15, 10, 4, 1, 5, 8, 7, 6, 2, 13, 14, 0, 3, 9, 11};
static const u8 S[80]  = {RMD_S0, 14, 15, 12, 5, 8, 7, 9, 11, 13, 14, 15, 6, 7, 9, 8, 7, 6, 8, 13, 11, 9, 7, 15, 7, 12, 15, 9, 11, 7, 13, 12, 11, 13, 6, 7, 14, 9, 13, 15, 14, 8, 13, 6, 5, 12, 7, 5, 11, 12, 14, 15, 14, 15, 9, 8, 9, 14, 5, 6, 8, 6, 5, 12, 9, 15, 5, 11, 6, 8, 13, 12, 5, 12, 13, 14, 11, 8, 5, 6};
static const u8 SP[80] = {8, 9, 9, 11, 13, 15, 15, 5, 7, 7, 8, 11, 14, 14, 12, 6, 9, 13, 15, 7, 12, 8, 9, 11, 7, 7, 12, 7, 6, 15, 13, 11, 9, 7, 15, 11, 8, 6, 6, 14, 12, 13, 5, 14, 13, 13, 7, 5, 15, 5, 8, 11, 14, 14, 6, 14, 6, 9, 12, 9, 12, 5, 15, 8, 8, 5, 12, 9, 12, 5, 14, 6, 8, 13, 6, 5, 15, 13, 11, 11};
static const u32 KL[5] = {0x00000000u, 0x5A827999u, 0x6ED9EBA1u, 0x8F1BBCDCu, 0xA953FD4Eu};
static const u32 KR[5] = {0x50A28BE6u, 0x5C4DD124u, 0x6D703EF3u, 0x7A6D76E9u, 0x00000000u};
#define ROL(x, s) (((x) << (s)) | ((x) >> (32 - (s))))
static u32 ld32le(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }
static int hexval(char c) { return c <= '9' ? c - '0' : c - 'a' + 10; }
static u32 f(int j, u32 x, u32 y, u32 z) {
    switch (j >> 4) {
    case 0: return x ^ y ^ z;
    case 1: return (x & y) | (~x & z);
    case 2: return (x | ~y) ^ z;
    case 3: return (x & z) | (y & ~z);
    default: return x ^ (y | ~z);
    }
}
// The paper's pseudo-code for one 16-word block: two parallel lines of 80 steps, then the cross-combination of the results.
static void compress(u32 h[5], const u8 *blk) {
    u32 X[16];
    for (int i = 0; i < 16; i++) X[i] = ld32le(blk + 4 * i);
    u32 A = h[0], B = h[1], C = h[2], D = h[3], E = h[4];
    u32 A2 = h[0], B2 = h[1], C2 = h[2], D2 = h[3], E2 = h[4], T;
    for (int j = 0; j < 80; j++) {
        T = ROL(A + f(j, B, C, D) + X[R[j]] + KL[j >> 4], S[j]) + E;
        A = E; E = D; D = ROL(C, 10); C = B; B = T;
        T = ROL(A2 + f(79 - j, B2, C2, D2) + X[RP[j]] + KR[j >> 4], SP[j]) + E2;
        A2 = E2; E2 = D2; D2 = ROL(C2, 10); C2 = B2; B2 = T;
    }
    T = h[1] + C + D2; h[1] = h[2] + D + E2; h[2] = h[3] + E + A2; h[3] = h[4] + A + B2; h[4] = h[0] + B + C2; h[0] = T;
}
// MD4-style padding: 0x80, zeros to 56 mod 64, 64-bit little-endian bit length. The message is `rep` copies of `m`.
static void rmd160(const char *m, u32 rep, u8 out[20]) {
    u32 h[5] = {0x67452301u, 0xEFCDAB89u, 0x98BADCFEu, 0x10325476u, 0xC3D2E1F0u};
    u8 blk[64]; u32 n = 0; u64 total = 0;
    u32 ml = 0; while (m[ml]) ml++;
    for (u32 r = 0; r < rep; r++) for (u32 i = 0; i < ml; i++) { blk[n++] = (u8)m[i]; total++; if (n == 64) { compress(h, blk); n = 0; } }
    blk[n++] = 0x80;
    if (n > 56) { while (n < 64) blk[n++] = 0; compress(h, blk); n = 0; }
    while (n < 56) blk[n++] = 0;
    u64 bits = total * 8; for (int i = 0; i < 8; i++) blk[56 + i] = (u8)(bits >> (8 * i));
    compress(h, blk);
    for (int i = 0; i < 5; i++) for (int j = 0; j < 4; j++) out[4 * i + j] = (u8)(h[i] >> (8 * j));
}
int paper_main(void) {
    int good = 0;
    for (int i = 0; i < NV; i++) {
        u8 got[20], want[20]; char name[64]; int k = 0;
        for (int j = 0; j < 20; j++) want[j] = (u8)(hexval(V[i].md[2 * j]) * 16 + hexval(V[i].md[2 * j + 1]));
        rmd160(V[i].msg, V[i].rep, got);
        const char *p = "RIPEMD-160 test "; while (*p) name[k++] = *p++;
        name[k++] = (char)('1' + i); name[k] = 0;
        good += pcheck_bytes(name, got, want, 20);
    }
    pstr("paper Appendix B: "); pdec(good); pstr(" of "); pdec(NV); pstr(" match\n");
    pstr(good == NV ? "PASS\n" : "FAIL\n");
    return good == NV ? 0 : 1;
}
