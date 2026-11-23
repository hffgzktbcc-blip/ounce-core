// SHA-1 (FIPS 180-1 / RFC 3174, "US Secure Hash Algorithm 1 (SHA1)", Eastlake and Jones, 2001), written from the algorithm
// description. Checks the 14 SHA-1 values in the test driver of RFC 6234 section 8.5 (Eastlake and Hansen, 2011): ten
// known-answer digests (the first four are the four tests of RFC 3174, including one million 'a'; the rest add bit-length
// messages of 5 and 3 extra bits) and four chained "random test" digests. The digests, messages and seed are data copied as
// numbers; no code from either RFC was copied. Exit 0 only if all 14 match.
// Negative control: -DSHA1_K3=0x8F1BBCDDu changes one bit of the round constant for rounds 40-59.
#include "ounce_paper.h"

struct tv { const u8 *msg; u32 len; u32 rep; u8 extra; u8 nb; const char *md; };
static int hexval(char c) { return c <= '9' ? c - '0' : c - 'a' + 10; }
static u32 be32(const u8 *p) { return (u32)p[0] << 24 | (u32)p[1] << 16 | (u32)p[2] << 8 | p[3]; }
static u64 be64(const u8 *p) { return (u64)be32(p) << 32 | be32(p + 4); }
#define ROL32(x, s) (((x) << (s)) | ((x) >> (32 - (s))))
#define ROR32(x, s) (((x) >> (s)) | ((x) << (32 - (s))))
#define ROR64(x, s) (((x) >> (s)) | ((x) << (64 - (s))))
#define CH(x, y, z) (((x) & (y)) ^ (~(x) & (z)))
#define MAJ(x, y, z) (((x) & (y)) ^ ((x) & (z)) ^ ((y) & (z)))

// Streaming context shared by all variants: block size 64 (SHA-1, SHA-224/256) or 128 (SHA-384/512).
// kind 0 = SHA-1, 1 = SHA-224/256, 2 = SHA-384/512.
struct ctx { u64 h[8]; u8 buf[128]; u32 n; u64 total; int kind; };
static void compress(struct ctx *c, const u8 *p);
static void ctx_init(struct ctx *c, int kind, const u64 *iv, int words) {
    c->kind = kind; c->n = 0; c->total = 0;
    for (int i = 0; i < words; i++) c->h[i] = iv[i];
}
static int blocksize(const struct ctx *c) { return c->kind == 2 ? 128 : 64; }
static void ctx_update(struct ctx *c, const u8 *d, u32 len) {
    int bs = blocksize(c);
    c->total += len;
    while (len) {
        c->buf[c->n++] = *d++; len--;
        if (c->n == (u32)bs) { compress(c, c->buf); c->n = 0; }
    }
}
// Finish: the message so far plus `nb` (0..7) extra bits taken from the high end of `extra`, then the standard padding:
// a 1 bit, zeros, and the total bit length big-endian (64 bits; 128 bits for SHA-384/512).
static void ctx_final(struct ctx *c, u8 extra, int nb, u8 *out, int outlen) {
    int bs = blocksize(c), lenbytes = c->kind == 2 ? 16 : 8;
    u64 bits = c->total * 8 + (u64)nb;
    u8 last = nb ? (u8)((extra & (0xff << (8 - nb))) | (0x80 >> nb)) : 0x80;
    c->buf[c->n++] = last;
    if (c->n > (u32)(bs - lenbytes)) { while (c->n < (u32)bs) c->buf[c->n++] = 0; compress(c, c->buf); c->n = 0; }
    while (c->n < (u32)bs) c->buf[c->n++] = 0;
    for (int i = 0; i < 8; i++) c->buf[bs - 1 - i] = (u8)(bits >> (8 * i));
    compress(c, c->buf);
    int wb = c->kind == 0 ? 4 : c->kind == 1 ? 4 : 8;
    for (int i = 0; i < outlen; i++) out[i] = (u8)(c->h[i / wb] >> (8 * (wb - 1 - (i % wb))));
}
static void unhex(const char *s, u8 *o, int n) { for (int i = 0; i < n; i++) o[i] = (u8)(hexval(s[2 * i]) * 16 + hexval(s[2 * i + 1])); }
static void fmt_name(char *name, const char *alg, const char *what, int idx) {
    int k = 0; while (*alg) name[k++] = *alg++;
    name[k++] = ' '; while (*what) name[k++] = *what++;
    name[k++] = ' ';
    if (idx >= 10) name[k++] = (char)('0' + idx / 10);
    name[k++] = (char)('0' + idx % 10); name[k] = 0;
}

#ifndef SHA1_K3
#define SHA1_K3 0x8F1BBCDCu
#endif
static const u64 IV1[5] = {0x67452301ull, 0xefcdab89ull, 0x98badcfeull, 0x10325476ull, 0xc3d2e1f0ull};
static void compress(struct ctx *c, const u8 *p) {
    u32 w[80], a = (u32)c->h[0], b = (u32)c->h[1], cc = (u32)c->h[2], d = (u32)c->h[3], e = (u32)c->h[4];
    for (int t = 0; t < 16; t++) w[t] = be32(p + 4 * t);
    for (int t = 16; t < 80; t++) w[t] = ROL32(w[t - 3] ^ w[t - 8] ^ w[t - 14] ^ w[t - 16], 1);
    for (int t = 0; t < 80; t++) {
        u32 f, k;
        if (t < 20) { f = CH(b, cc, d); k = 0x5a827999u; }
        else if (t < 40) { f = b ^ cc ^ d; k = 0x6ed9eba1u; }
        else if (t < 60) { f = MAJ(b, cc, d); k = SHA1_K3; }
        else { f = b ^ cc ^ d; k = 0xca62c1d6u; }
        u32 tmp = ROL32(a, 5) + f + e + k + w[t];
        e = d; d = cc; cc = ROL32(b, 30); b = a; a = tmp;
    }
    c->h[0] = (u32)(c->h[0] + a); c->h[1] = (u32)(c->h[1] + b); c->h[2] = (u32)(c->h[2] + cc);
    c->h[3] = (u32)(c->h[3] + d); c->h[4] = (u32)(c->h[4] + e);
}
static const u8 m_sha1_0[] = {0x61, 0x62, 0x63};
static const u8 m_sha1_1[] = {0x61, 0x62, 0x63, 0x64, 0x62, 0x63, 0x64, 0x65, 0x63, 0x64, 0x65, 0x66, 0x64, 0x65, 0x66, 0x67, 0x65, 0x66, 0x67, 0x68, 0x66, 0x67, 0x68, 0x69, 0x67, 0x68, 0x69, 0x6a, 0x68, 0x69, 0x6a, 0x6b, 0x69, 0x6a, 0x6b, 0x6c, 0x6a, 0x6b, 0x6c, 0x6d, 0x6b, 0x6c, 0x6d, 0x6e, 0x6c, 0x6d, 0x6e, 0x6f, 0x6d, 0x6e, 0x6f, 0x70, 0x6e, 0x6f, 0x70, 0x71};
static const u8 m_sha1_2[] = {0x61};
static const u8 m_sha1_3[] = {0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37};
static const u8 m_sha1_5[] = {0x5e};
static const u8 m_sha1_6[] = {0x49, 0xb2, 0xae, 0xc2, 0x59, 0x4b, 0xbe, 0x3a, 0x3b, 0x11, 0x75, 0x42, 0xd9, 0x4a, 0xc8};
static const u8 m_sha1_7[] = {0x9a, 0x7d, 0xfd, 0xf1, 0xec, 0xea, 0xd0, 0x6e, 0xd6, 0x46, 0xaa, 0x55, 0xfe, 0x75, 0x71, 0x46};
static const u8 m_sha1_8[] = {0x65, 0xf9, 0x32, 0x99, 0x5b, 0xa4, 0xce, 0x2c, 0xb1, 0xb4, 0xa2, 0xe7, 0x1a, 0xe7, 0x02, 0x20, 0xaa, 0xce, 0xc8, 0x96, 0x2d, 0xd4, 0x49, 0x9c, 0xbd, 0x7c, 0x88, 0x7a, 0x94, 0xea, 0xaa, 0x10, 0x1e, 0xa5, 0xaa, 0xbc, 0x52, 0x9b, 0x4e, 0x7e, 0x43, 0x66, 0x5a, 0x5a, 0xf2, 0xcd, 0x03, 0xfe, 0x67, 0x8e, 0xa6, 0xa5, 0x00, 0x5b, 0xba, 0x3b, 0x08, 0x22, 0x04, 0xc2, 0x8b, 0x91, 0x09, 0xf4, 0x69, 0xda, 0xc9, 0x2a, 0xaa, 0xb3, 0xaa, 0x7c, 0x11, 0xa1, 0xb3, 0x2a};
static const u8 m_sha1_9[] = {0xf7, 0x8f, 0x92, 0x14, 0x1b, 0xcd, 0x17, 0x0a, 0xe8, 0x9b, 0x4f, 0xba, 0x15, 0xa1, 0xd5, 0x9f, 0x3f, 0xd8, 0x4d, 0x22, 0x3c, 0x92, 0x51, 0xbd, 0xac, 0xbb, 0xae, 0x61, 0xd0, 0x5e, 0xd1, 0x15, 0xa0, 0x6a, 0x7c, 0xe1, 0x17, 0xb7, 0xbe, 0xea, 0xd2, 0x44, 0x21, 0xde, 0xd9, 0xc3, 0x25, 0x92, 0xbd, 0x57, 0xed, 0xea, 0xe3, 0x9c, 0x39, 0xfa, 0x1f, 0xe8, 0x94, 0x6a, 0x84, 0xd0, 0xcf, 0x1f, 0x7b, 0xee, 0xad, 0x17, 0x13, 0xe2, 0xe0, 0x95, 0x98, 0x97, 0x34, 0x7f, 0x67, 0xc8, 0x0b, 0x04, 0x00, 0xc2, 0x09, 0x81, 0x5d, 0x6b, 0x10, 0xa6, 0x83, 0x83, 0x6f, 0xd5, 0x56, 0x2a, 0x56, 0xca, 0xb1, 0xa2, 0x8e, 0x81, 0xb6, 0x57, 0x66, 0x54, 0x63, 0x1c, 0xf1, 0x65, 0x66, 0xb8, 0x6e, 0x3b, 0x33, 0xa1, 0x08, 0xb0, 0x53, 0x07, 0xc0, 0x0a, 0xff, 0x14, 0xa7, 0x68, 0xed, 0x73, 0x50, 0x60, 0x6a, 0x0f, 0x85, 0xe6, 0xa9, 0x1d, 0x39, 0x6f, 0x5b, 0x5c, 0xbe, 0x57, 0x7f, 0x9b, 0x38, 0x80, 0x7c, 0x7d, 0x52, 0x3d, 0x6d, 0x79, 0x2f, 0x6e, 0xbc, 0x24, 0xa4, 0xec, 0xf2, 0xb3, 0xa4, 0x27, 0xcd, 0xbb, 0xfb};
static const u8 seed_sha1[] = {0xd0, 0x56, 0x9c, 0xb3, 0x66, 0x5a, 0x8a, 0x43, 0xeb, 0x6e, 0xa2, 0x3d, 0x75, 0xa3, 0xc4, 0xd2, 0x05, 0x4a, 0x0d, 0x7d};
static const struct tv tests_sha1[10] = {
    {m_sha1_0, 3, 1, 0x00, 0, "a9993e364706816aba3e25717850c26c9cd0d89d"},
    {m_sha1_1, 56, 1, 0x00, 0, "84983e441c3bd26ebaae4aa1f95129e5e54670f1"},
    {m_sha1_2, 1, 1000000, 0x00, 0, "34aa973cd4c4daa4f61eeb2bdbad27316534016f"},
    {m_sha1_3, 64, 10, 0x00, 0, "dea356a2cddd90c7a7ecedc5ebb563934f460452"},
    {0, 0, 0, 0x98, 5, "29826b003b906e660eff4027ce98af3531ac75ba"},
    {m_sha1_5, 1, 1, 0x00, 0, "5e6f80a34a9798cafc6a5db96cc57ba4c4db59c2"},
    {m_sha1_6, 15, 1, 0x80, 3, "6239781e03729919c01955b3ffa8acb60b988340"},
    {m_sha1_7, 16, 1, 0x00, 0, "82abff6605dbe1c17def12a394fa22a82b544a35"},
    {m_sha1_8, 76, 1, 0xe0, 3, "8c5b2a5ddae5a97fc7f9d85661c672adbf7933d4"},
    {m_sha1_9, 163, 1, 0x00, 0, "cb0082c8f197d260991ba6a460e76e202bad27b3"},
};
static const char *const rand_sha1[4] = {
    "e216836819477c7f78e0d843fe4ff1b6d6c14cd4",
    "a2dbc7a5b1c6c0a8bcb7aaa41252a6a7d0690dbc",
    "db1f9050bb863dfef4ce37186044e2eeb17ee013",
    "127fdedf43d372a51d5747c48fbffe38ef6cdf7b"
};

// The RFC 6234 section 8.5 driver: 10 known-answer tests per hash (the message repeated `rep` times, then `nb` extra bits),
// and 4 chained "random" results: starting from the seed, 1000 times hash the concatenation of the previous three digests
// (the first three are the seed), repeated 4 times, each time reporting the last digest and using it as the next seed.
static int run(const char *label, int kind, const u64 *iv, int words, int outlen, const struct tv *tests, const u8 *seed, const char *const *rnd) {
    int good = 0, total = 0;
    static u8 md[1003][64];
    for (int i = 0; i < 10; i++) {
        struct ctx c; u8 got[64], want[64]; char name[48];
        ctx_init(&c, kind, iv, words);
        for (u32 r = 0; r < tests[i].rep; r++) ctx_update(&c, tests[i].msg, tests[i].len);
        ctx_final(&c, tests[i].extra, tests[i].nb, got, outlen);
        unhex(tests[i].md, want, outlen);
        fmt_name(name, label, "test", i + 1);
        good += pcheck_bytes(name, got, want, outlen); total++;
    }
    for (int j = 0; j < 4; j++) {
        u8 want[64], got[64]; char name[48];
        if (j == 0) for (int b = 0; b < outlen; b++) md[0][b] = seed[b];
        else for (int b = 0; b < outlen; b++) md[0][b] = md[1002][b];
        for (int b = 0; b < outlen; b++) { md[1][b] = md[0][b]; md[2][b] = md[0][b]; }
        for (int i = 3; i < 1003; i++) {
            struct ctx c; ctx_init(&c, kind, iv, words);
            ctx_update(&c, md[i - 3], (u32)outlen); ctx_update(&c, md[i - 2], (u32)outlen); ctx_update(&c, md[i - 1], (u32)outlen);
            ctx_final(&c, 0, 0, md[i], outlen);
        }
        for (int b = 0; b < outlen; b++) got[b] = md[1002][b];
        unhex(rnd[j], want, outlen);
        fmt_name(name, label, "random", j);
        good += pcheck_bytes(name, got, want, outlen); total++;
    }
    pstr(label); pstr(": "); pdec(good); pstr(" of "); pdec(total); pstr(" match\n");
    return good == total ? 0 : 1;
}

int paper_main(void) {
    int bad = 0;
    bad |= run("SHA-1", 0, IV1, 5, 20, tests_sha1, seed_sha1, rand_sha1);
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad;
}
