// MurmurHash2 family (Austin Appleby, public domain): MurmurHash2, MurmurHash2A, MurmurHash64A and MurmurHash64B, written from
// the algorithms as published in the author's SMHasher repository. Check: the author's "verification value" for each
// variant, by the SMHasher procedure: hash the keys {0}, {0,1}, ... of length 0..255 (seed = 256 - length), hash the
// concatenated outputs with seed 0 and read the first four bytes of that result as a little-endian integer. The four values
// are data copied as numbers; no code was copied. Exit 0 only if all four match.
#include "ounce_paper.h"

#ifndef M2_R      // negative control hook: the shift in the 32-bit block mix (published value: 24)
#define M2_R 24
#endif

static u32 rd32(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }
static u64 rd64(const u8 *p) { return (u64)rd32(p) | (u64)rd32(p + 4) << 32; }
static void wr32(u8 *p, u32 v) { for (int i = 0; i < 4; i++) p[i] = (u8)(v >> (8 * i)); }
static void wr64(u8 *p, u64 v) { for (int i = 0; i < 8; i++) p[i] = (u8)(v >> (8 * i)); }
static u32 tail32(const u8 *p, int n) { u32 t = 0; for (int i = n - 1; i >= 0; i--) t = (t << 8) | p[i]; return t; }

#define M32 0x5bd1e995u

static u32 mix32(u32 h, u32 k) { k *= M32; k ^= k >> M2_R; k *= M32; h *= M32; return h ^ k; }

static void murmur2(const u8 *d, int len, u32 seed, u8 *out) {
    u32 h = seed ^ (u32)len;
    int n = len;
    for (; n >= 4; n -= 4, d += 4) h = mix32(h, rd32(d));
    if (n) { h ^= tail32(d, n); h *= M32; }
    h ^= h >> 13; h *= M32; h ^= h >> 15;
    wr32(out, h);
}

static void murmur2a(const u8 *d, int len, u32 seed, u8 *out) {
    u32 h = seed;
    int n = len;
    for (; n >= 4; n -= 4, d += 4) h = mix32(h, rd32(d));
    h = mix32(h, tail32(d, n));
    h = mix32(h, (u32)len);
    h ^= h >> 13; h *= M32; h ^= h >> 15;
    wr32(out, h);
}

static void murmur64a(const u8 *d, int len, u32 seed, u8 *out) {
    const u64 m = 0xc6a4a7935bd1e995ULL;
    u64 h = (u64)seed ^ ((u64)len * m);
    int n = len;
    for (; n >= 8; n -= 8, d += 8) {
        u64 k = rd64(d);
        k *= m; k ^= k >> 47; k *= m;
        h ^= k; h *= m;
    }
    if (n) { u64 t = 0; for (int i = n - 1; i >= 0; i--) t = (t << 8) | d[i]; h ^= t; h *= m; }
    h ^= h >> 47; h *= m; h ^= h >> 47;
    wr64(out, h);
}

static void murmur64b(const u8 *d, int len, u32 seed, u8 *out) {
    u32 h1 = seed ^ (u32)len, h2 = 0;   // the seed argument is 32 bits in the verification, so its upper half is 0
    int n = len;
    for (; n >= 8; n -= 8, d += 8) { h1 = mix32(h1, rd32(d)); h2 = mix32(h2, rd32(d + 4)); }
    if (n >= 4) { h1 = mix32(h1, rd32(d)); n -= 4; d += 4; }
    if (n) { h2 ^= tail32(d, n); h2 *= M32; }
    h1 ^= h2 >> 18; h1 *= M32;
    h2 ^= h1 >> 22; h2 *= M32;
    h1 ^= h2 >> 17; h1 *= M32;
    h2 ^= h1 >> 19; h2 *= M32;
    wr64(out, ((u64)h1 << 32) | h2);
}

typedef void (*hashfn)(const u8 *, int, u32, u8 *);

static u32 verification(hashfn h, int hashbytes) {
    static u8 key[256], hashes[8 * 256];
    u8 fin[8];
    for (int i = 0; i < 256; i++) { key[i] = (u8)i; h(key, i, (u32)(256 - i), &hashes[i * hashbytes]); }
    h(hashes, hashbytes * 256, 0, fin);
    return rd32(fin);
}

int paper_main(void) {
    int ok = 1;
    ok &= pcheck_u64("MurmurHash2   verification value", verification(murmur2, 4), 0x27864C1Eu, 8);
    ok &= pcheck_u64("MurmurHash2A  verification value", verification(murmur2a, 4), 0x7FBD4396u, 8);
    ok &= pcheck_u64("MurmurHash64A verification value", verification(murmur64a, 8), 0x1F0D3804u, 8);
    ok &= pcheck_u64("MurmurHash64B verification value", verification(murmur64b, 8), 0xDD537C05u, 8);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
