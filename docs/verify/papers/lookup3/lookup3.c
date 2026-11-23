// lookup3 (Bob Jenkins, May 2006, public domain): hashlittle() and hashlittle2(), written from the algorithm in the author's
// lookup3.c (the mix() and final() steps and the 12-byte block structure). Checks: (1) the hash values the author records as
// comments beside the eight calls in his own driver5() (key "" and "Four score and seven years ago" with several initial values;
// the six hashlittle2 calls give two 32-bit values each, the two hashlittle calls one each: 14 numbers), and (2) the author's SMHasher verification value 0x3D83917A for lookup3 (hash the keys {0}, {0,1},
// ... of length 0..255 with seed 256-length, hash the concatenated outputs with seed 0, take the first 4 bytes little-endian).
// The numbers are data copied from those two sources; no code was copied. Exit 0 only if all 9 checks (15 published numbers) match.
#include "ounce_paper.h"

#ifndef L3_ROT1      // negative control hook: first rotation of mix() (published value: 4)
#define L3_ROT1 4
#endif

static u32 rot(u32 x, int k) { return (x << k) | (x >> (32 - k)); }
static u32 le32(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }

static void mix(u32 *a, u32 *b, u32 *c) {
    *a -= *c; *a ^= rot(*c, L3_ROT1); *c += *b;
    *b -= *a; *b ^= rot(*a, 6);  *a += *c;
    *c -= *b; *c ^= rot(*b, 8);  *b += *a;
    *a -= *c; *a ^= rot(*c, 16); *c += *b;
    *b -= *a; *b ^= rot(*a, 19); *a += *c;
    *c -= *b; *c ^= rot(*b, 4);  *b += *a;
}
static void final_mix(u32 *a, u32 *b, u32 *c) {
    *c ^= *b; *c -= rot(*b, 14);
    *a ^= *c; *a -= rot(*c, 11);
    *b ^= *a; *b -= rot(*a, 25);
    *c ^= *b; *c -= rot(*b, 16);
    *a ^= *c; *a -= rot(*c, 4);
    *b ^= *a; *b -= rot(*a, 14);
    *c ^= *b; *c -= rot(*b, 24);
}

// hashlittle2: *pc, *pb are the two initial values on entry and the two hash values on return.
static void hashlittle2(const u8 *k, unsigned long len, u32 *pc, u32 *pb) {
    u32 a, b, c;
    a = b = c = 0xdeadbeefu + (u32)len + *pc;
    c += *pb;
    while (len > 12) {
        a += le32(k); b += le32(k + 4); c += le32(k + 8);
        mix(&a, &b, &c);
        len -= 12; k += 12;
    }
    if (len == 0) { *pc = c; *pb = b; return; }     // zero-length tail: no final mixing
    u32 w[3] = {0, 0, 0};
    for (unsigned long i = 0; i < len; i++) w[i / 4] |= (u32)k[i] << (8 * (i % 4));
    a += w[0]; b += w[1]; c += w[2];
    final_mix(&a, &b, &c);
    *pc = c; *pb = b;
}
static u32 hashlittle(const u8 *k, unsigned long len, u32 initval) { u32 c = initval, b = 0; hashlittle2(k, len, &c, &b); return c; }

static void lookup3_verif(const u8 *key, int len, u32 seed, u8 *out) {
    u32 h = hashlittle(key, (unsigned long)len, seed);
    for (int i = 0; i < 4; i++) out[i] = (u8)(h >> (8 * i));
}
static u32 verification(void) {
    static u8 key[256], hashes[4 * 256];
    u8 fin[4];
    for (int i = 0; i < 256; i++) { key[i] = (u8)i; lookup3_verif(key, i, (u32)(256 - i), &hashes[4 * i]); }
    lookup3_verif(hashes, 4 * 256, 0, fin);
    return le32(fin);
}

static int two(const char *name, const char *msg, unsigned long n, u32 c0, u32 b0, u32 want_c, u32 want_b) {
    u32 c = c0, b = b0;
    hashlittle2((const u8 *)msg, n, &c, &b);
    int ok = c == want_c && b == want_b;
    pstr(ok ? "PASS " : "FAIL "); pstr(name); pstr(": "); phex(c, 8); pstr(" "); phex(b, 8);
    if (!ok) { pstr(" expected "); phex(want_c, 8); pstr(" "); phex(want_b, 8); }
    pstr("\n"); return ok;
}

int paper_main(void) {
    static const char *S = "Four score and seven years ago";
    int ok = 1;
    ok &= two("hashlittle2 \"\", c=0 b=0                 ", "", 0, 0, 0, 0xdeadbeefu, 0xdeadbeefu);
    ok &= two("hashlittle2 \"\", c=0 b=0xdeadbeef        ", "", 0, 0, 0xdeadbeefu, 0xbd5b7ddeu, 0xdeadbeefu);
    ok &= two("hashlittle2 \"\", c=0xdeadbeef b=0xdeadbeef", "", 0, 0xdeadbeefu, 0xdeadbeefu, 0x9c093ccdu, 0xbd5b7ddeu);
    ok &= two("hashlittle2 \"Four score...\", c=0 b=0     ", S, 30, 0, 0, 0x17770551u, 0xce7226e6u);
    ok &= two("hashlittle2 \"Four score...\", c=0 b=1     ", S, 30, 0, 1, 0xe3607caeu, 0xbd371de4u);
    ok &= two("hashlittle2 \"Four score...\", c=1 b=0     ", S, 30, 1, 0, 0xcd628161u, 0x6cbea4b3u);
    ok &= pcheck_u64("hashlittle  \"Four score...\", initval 0", hashlittle((const u8 *)S, 30, 0), 0x17770551u, 8);
    ok &= pcheck_u64("hashlittle  \"Four score...\", initval 1", hashlittle((const u8 *)S, 30, 1), 0xcd628161u, 8);
    ok &= pcheck_u64("SMHasher verification value for lookup3  ", verification(), 0x3D83917Au, 8);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
