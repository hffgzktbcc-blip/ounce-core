// Threefry-N x W-R counter-based generators (Salmon, Moraes, Dror and Shaw, "Parallel random numbers: as easy as 1, 2, 3",
// SC '11, 2011; Threefry is the paper's Threefish-derived generator), written from the paper's construction: add the key to the
// counter, then R rounds of add / rotate / xor mixing on word pairs with a key injection after every 4 rounds. Checks all 36
// known-answer vectors the authors' repository lists for Threefry: 2x32, 4x32, 2x64 and 4x64, each at 13, 20 and 32 (2-word) or
// 72 (4-word) rounds, each for the counter/key pairs (0,0), (all ones) and (digits of pi). The values are data copied as numbers.
// No code was copied. Exit 0 only if all 36 vectors match.
#include "ounce_paper.h"

typedef struct { unsigned char n, w, rounds; u64 ctr[4], key[4], want[4]; } KAT;
static const KAT KATS[36] = {
    {2,32,13,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x9d1c5ec6ULL,0x8bd50731ULL,0x0ULL,0x0ULL}},
    {2,32,13,{0xffffffffULL,0xffffffffULL,0x0ULL,0x0ULL},{0xffffffffULL,0xffffffffULL,0x0ULL,0x0ULL},{0xfd36d048ULL,0x2d17272cULL,0x0ULL,0x0ULL}},
    {2,32,13,{0x243f6a88ULL,0x85a308d3ULL,0x0ULL,0x0ULL},{0x13198a2eULL,0x3707344ULL,0x0ULL,0x0ULL},{0xba3e4725ULL,0xf27d669eULL,0x0ULL,0x0ULL}},
    {2,32,20,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x6b200159ULL,0x99ba4efeULL,0x0ULL,0x0ULL}},
    {2,32,20,{0xffffffffULL,0xffffffffULL,0x0ULL,0x0ULL},{0xffffffffULL,0xffffffffULL,0x0ULL,0x0ULL},{0x1cb996fcULL,0xbb002be7ULL,0x0ULL,0x0ULL}},
    {2,32,20,{0x243f6a88ULL,0x85a308d3ULL,0x0ULL,0x0ULL},{0x13198a2eULL,0x3707344ULL,0x0ULL,0x0ULL},{0xc4923a9cULL,0x483df7a0ULL,0x0ULL,0x0ULL}},
    {2,32,32,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0xcee3d47eULL,0xa23dfd5cULL,0x0ULL,0x0ULL}},
    {2,32,32,{0xffffffffULL,0xffffffffULL,0x0ULL,0x0ULL},{0xffffffffULL,0xffffffffULL,0x0ULL,0x0ULL},{0x6e2fe0d0ULL,0xb1b76f82ULL,0x0ULL,0x0ULL}},
    {2,32,32,{0x243f6a88ULL,0x85a308d3ULL,0x0ULL,0x0ULL},{0x13198a2eULL,0x3707344ULL,0x0ULL,0x0ULL},{0xe2827716ULL,0xc3c05cdfULL,0x0ULL,0x0ULL}},
    {4,32,13,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x531c7e4fULL,0x39491ee5ULL,0x2c855a92ULL,0x3d6abf9aULL}},
    {4,32,13,{0xffffffffULL,0xffffffffULL,0xffffffffULL,0xffffffffULL},{0xffffffffULL,0xffffffffULL,0xffffffffULL,0xffffffffULL},{0xc4189358ULL,0x1c9cc83aULL,0xd5881c67ULL,0x6a0a89e0ULL}},
    {4,32,13,{0x243f6a88ULL,0x85a308d3ULL,0x13198a2eULL,0x3707344ULL},{0xa4093822ULL,0x299f31d0ULL,0x82efa98ULL,0xec4e6c89ULL},{0x4aa71d8fULL,0x734738c2ULL,0x431fc6a8ULL,0xae6debf1ULL}},
    {4,32,20,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x9c6ca96aULL,0xe17eae66ULL,0xfc10ecd4ULL,0x5256a7d8ULL}},
    {4,32,20,{0xffffffffULL,0xffffffffULL,0xffffffffULL,0xffffffffULL},{0xffffffffULL,0xffffffffULL,0xffffffffULL,0xffffffffULL},{0x2a881696ULL,0x57012287ULL,0xf6c7446eULL,0xa16a6732ULL}},
    {4,32,20,{0x243f6a88ULL,0x85a308d3ULL,0x13198a2eULL,0x3707344ULL},{0xa4093822ULL,0x299f31d0ULL,0x82efa98ULL,0xec4e6c89ULL},{0x59cd1dbbULL,0xb8879579ULL,0x86b5d00cULL,0xac8b6d84ULL}},
    {4,32,72,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x93171da6ULL,0x9220326dULL,0xb392b7b1ULL,0xff58a002ULL}},
    {4,32,72,{0xffffffffULL,0xffffffffULL,0xffffffffULL,0xffffffffULL},{0xffffffffULL,0xffffffffULL,0xffffffffULL,0xffffffffULL},{0x60743f3dULL,0x9961e684ULL,0xaab21c34ULL,0x8c65fb7dULL}},
    {4,32,72,{0x243f6a88ULL,0x85a308d3ULL,0x13198a2eULL,0x3707344ULL},{0xa4093822ULL,0x299f31d0ULL,0x82efa98ULL,0xec4e6c89ULL},{0x9930adfULL,0x7f27bd55ULL,0x9ed68ce1ULL,0x97f803f6ULL}},
    {2,64,13,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0xf167b032c3b480bdULL,0xe91f9fee4b7a6fb5ULL,0x0ULL,0x0ULL}},
    {2,64,13,{0xffffffffffffffffULL,0xffffffffffffffffULL,0x0ULL,0x0ULL},{0xffffffffffffffffULL,0xffffffffffffffffULL,0x0ULL,0x0ULL},{0xccdec5c917a874b1ULL,0x4df53abca26ceb01ULL,0x0ULL,0x0ULL}},
    {2,64,13,{0x243f6a8885a308d3ULL,0x13198a2e03707344ULL,0x0ULL,0x0ULL},{0xa4093822299f31d0ULL,0x82efa98ec4e6c89ULL,0x0ULL,0x0ULL},{0xc3aac71561042993ULL,0x3fe7ae8801aff316ULL,0x0ULL,0x0ULL}},
    {2,64,20,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0xc2b6e3a8c2c69865ULL,0x6f81ed42f350084dULL,0x0ULL,0x0ULL}},
    {2,64,20,{0xffffffffffffffffULL,0xffffffffffffffffULL,0x0ULL,0x0ULL},{0xffffffffffffffffULL,0xffffffffffffffffULL,0x0ULL,0x0ULL},{0xe02cb7c4d95d277aULL,0xd06633d0893b8b68ULL,0x0ULL,0x0ULL}},
    {2,64,20,{0x243f6a8885a308d3ULL,0x13198a2e03707344ULL,0x0ULL,0x0ULL},{0xa4093822299f31d0ULL,0x82efa98ec4e6c89ULL,0x0ULL,0x0ULL},{0x263c7d30bb0f0af1ULL,0x56be8361d3311526ULL,0x0ULL,0x0ULL}},
    {2,64,32,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x38ba854d7f13cfb3ULL,0xd02fca729d54fadcULL,0x0ULL,0x0ULL}},
    {2,64,32,{0xffffffffffffffffULL,0xffffffffffffffffULL,0x0ULL,0x0ULL},{0xffffffffffffffffULL,0xffffffffffffffffULL,0x0ULL,0x0ULL},{0x6b532f4f6e288646ULL,0x388f1ec135ee18eULL,0x0ULL,0x0ULL}},
    {2,64,32,{0x243f6a8885a308d3ULL,0x13198a2e03707344ULL,0x0ULL,0x0ULL},{0xa4093822299f31d0ULL,0x82efa98ec4e6c89ULL,0x0ULL,0x0ULL},{0xdad492f32efbd0c4ULL,0xb6d7d0cd1f193e84ULL,0x0ULL,0x0ULL}},
    {4,64,13,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x4071fabee1dc8e05ULL,0x2ed3113695c9c62ULL,0x397311b5b89f9d49ULL,0xe21292c3258024bcULL}},
    {4,64,13,{0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL},{0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL},{0x7eaed935479722b5ULL,0x90994358c429f31cULL,0x496381083e07a75bULL,0x627ed0d746821121ULL}},
    {4,64,13,{0x243f6a8885a308d3ULL,0x13198a2e03707344ULL,0xa4093822299f31d0ULL,0x82efa98ec4e6c89ULL},{0x452821e638d01377ULL,0xbe5466cf34e90c6cULL,0xc0ac29b7c97c50ddULL,0x3f84d5b5b5470917ULL},{0x4361288ef9c1900cULL,0x8717291521782833ULL,0xd19db18c20cf47eULL,0xa0b41d63ac8581e5ULL}},
    {4,64,20,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x9218ebde6c85537ULL,0x55941f5266d86105ULL,0x4bd25e16282434dcULL,0xee29ec846bd2e40bULL}},
    {4,64,20,{0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL},{0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL},{0x29c24097942bba1bULL,0x371bbfb0f6f4e11ULL,0x3c231ffa33f83a1cULL,0xcd29113fde32d168ULL}},
    {4,64,20,{0x243f6a8885a308d3ULL,0x13198a2e03707344ULL,0xa4093822299f31d0ULL,0x82efa98ec4e6c89ULL},{0x452821e638d01377ULL,0xbe5466cf34e90c6cULL,0xbe5466cf34e90c6cULL,0xc0ac29b7c97c50ddULL},{0xa7e8fde591651bd9ULL,0xbaafd0c30138319bULL,0x84a5c1a729e685b9ULL,0x901d406ccebc1ba4ULL}},
    {4,64,72,{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x0ULL,0x0ULL,0x0ULL,0x0ULL},{0x94eeea8b1f2ada84ULL,0xadf103313eae6670ULL,0x952419a1f4b16d53ULL,0xd83f13e63c9f6b11ULL}},
    {4,64,72,{0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL},{0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0xffffffffffffffffULL},{0x11518c034bc1ff4cULL,0x193f10b8bcdcc9f7ULL,0xd024229cb58f20d8ULL,0x563ed6e48e05183fULL}},
    {4,64,72,{0x243f6a8885a308d3ULL,0x13198a2e03707344ULL,0xa4093822299f31d0ULL,0x82efa98ec4e6c89ULL},{0x452821e638d01377ULL,0xbe5466cf34e90c6cULL,0xbe5466cf34e90c6cULL,0xc0ac29b7c97c50ddULL},{0xacf412ccaa3b2270ULL,0xc9e99bd53f2e9173ULL,0x43dad469dc825948ULL,0xfbb19d06c8a2b4dcULL}},
};

#ifndef TF_INJECT
#define TF_INJECT 4            // a key is injected after every 4 rounds
#endif
// rotation constants of the Threefish-derived mixing functions, indexed [round mod 8]
static const unsigned char R2x32[8] = {13, 15, 26, 6, 17, 29, 16, 24};
static const unsigned char R2x64[8] = {16, 42, 12, 31, 16, 32, 24, 21};
static const unsigned char R4x32[8][2] = {{10, 26}, {11, 21}, {13, 27}, {23, 5}, {6, 20}, {17, 11}, {25, 10}, {18, 20}};
static const unsigned char R4x64[8][2] = {{14, 16}, {52, 57}, {23, 40}, {5, 37}, {25, 33}, {46, 12}, {58, 22}, {32, 32}};

static u64 rotl(u64 x, int r, int w) { u64 m = w == 64 ? ~0ULL : 0xffffffffULL; x &= m; return ((x << r) | (x >> (w - r))) & m; }

static void threefry(const KAT *t, u64 *out) {
    const int n = t->n, w = t->w;
    const u64 m = w == 64 ? ~0ULL : 0xffffffffULL;
    u64 ks[5], x[4];
    ks[n] = w == 64 ? 0x1bd11bdaa9fc1a22ULL : 0x1bd11bdaULL;       // key schedule parity constant
    for (int i = 0; i < n; i++) { ks[i] = t->key[i] & m; ks[n] ^= ks[i]; x[i] = (t->ctr[i] + ks[i]) & m; }
    for (int r = 0; r < t->rounds; r++) {
        if (n == 2) {
            int rot = w == 64 ? R2x64[r & 7] : R2x32[r & 7];
            x[0] = (x[0] + x[1]) & m; x[1] = rotl(x[1], rot, w); x[1] ^= x[0];
        } else {
            const unsigned char *rot = w == 64 ? R4x64[r & 7] : R4x32[r & 7];
            int a = (r & 1) ? 3 : 1, b = (r & 1) ? 1 : 3;     // even rounds mix (0,1) and (2,3); odd rounds mix (0,3) and (2,1)
            x[0] = (x[0] + x[a]) & m; x[a] = rotl(x[a], rot[0], w); x[a] ^= x[0];
            x[2] = (x[2] + x[b]) & m; x[b] = rotl(x[b], rot[1], w); x[b] ^= x[2];
        }
        if ((r + 1) % TF_INJECT == 0) {
            int s = (r + 1) / TF_INJECT;
            for (int i = 0; i < n; i++) x[i] = (x[i] + ks[(s + i) % (n + 1)]) & m;
            x[n - 1] = (x[n - 1] + (u64)s) & m;
        }
    }
    for (int i = 0; i < n; i++) out[i] = x[i];
}

int paper_main(void) {
    int good = 0, total = (int)(sizeof(KATS) / sizeof(KATS[0]));
    for (int k = 0; k < total; k++) {
        const KAT *t = &KATS[k];
        u64 got[4]; threefry(t, got);
        int ok = 1; for (int i = 0; i < t->n; i++) if (got[i] != t->want[i]) ok = 0;
        good += ok;
        pstr(ok ? "PASS " : "FAIL "); pstr("threefry"); pdec(t->n); pstr("x"); pdec(t->w); pstr("-"); pdec(t->rounds);
        pstr(" ctr0="); phex(t->ctr[0], t->w / 4); pstr(": ");
        for (int i = 0; i < t->n; i++) { if (i) pstr(" "); phex(got[i], t->w / 4); }
        if (!ok) { pstr(" expected "); for (int i = 0; i < t->n; i++) { if (i) pstr(" "); phex(t->want[i], t->w / 4); } }
        pstr("\n");
    }
    pstr("known-answer vectors: "); pdec(good); pstr(" of "); pdec(total); pstr(" match\n");
    int ok = good == total;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
