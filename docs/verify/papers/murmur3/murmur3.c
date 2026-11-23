// MurmurHash3 (Austin Appleby, public domain; the three variants x86_32, x86_128 and x64_128), written from the algorithm
// as published in the author's SMHasher repository. Check: the author's own "verification value" for each variant, the
// SMHasher procedure: hash the keys {0}, {0,1}, ... of length 0..255 (seed = 256 - length), hash the concatenated outputs
// with seed 0 and read the first four bytes of that result as a little-endian integer. Each verification value therefore
// depends on 256 hashes of different lengths and seeds. The three values are data copied as numbers; no code was copied.
// Exit 0 only if all three match.
#include "ounce_paper.h"

#ifndef M3_ROT32      // negative control hook: rotation of the 32-bit block mix (published value: 15)
#define M3_ROT32 15
#endif

static u32 rotl32(u32 x, int r) { return (x << r) | (x >> (32 - r)); }
static u64 rotl64(u64 x, int r) { return (x << r) | (x >> (64 - r)); }
static u32 fmix32(u32 h) { h ^= h >> 16; h *= 0x85ebca6bu; h ^= h >> 13; h *= 0xc2b2ae35u; h ^= h >> 16; return h; }
static u64 fmix64(u64 k) {
    k ^= k >> 33; k *= 0xff51afd7ed558ccdULL; k ^= k >> 33; k *= 0xc4ceb9fe1a85ec53ULL; k ^= k >> 33; return k;
}
static u32 rd32(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }
static u64 rd64(const u8 *p) { return (u64)rd32(p) | (u64)rd32(p + 4) << 32; }
static void wr32(u8 *p, u32 v) { for (int i = 0; i < 4; i++) p[i] = (u8)(v >> (8 * i)); }
static void wr64(u8 *p, u64 v) { for (int i = 0; i < 8; i++) p[i] = (u8)(v >> (8 * i)); }

static void murmur3_x86_32(const u8 *d, int len, u32 seed, u8 *out) {
    u32 h = seed;
    const u32 c1 = 0xcc9e2d51u, c2 = 0x1b873593u;
    int nb = len / 4;
    for (int i = 0; i < nb; i++) {
        u32 k = rd32(d + 4 * i);
        k *= c1; k = rotl32(k, M3_ROT32); k *= c2;
        h ^= k; h = rotl32(h, 13); h = h * 5 + 0xe6546b64u;
    }
    u32 k = 0;
    const u8 *t = d + 4 * nb;
    for (int i = (len & 3) - 1; i >= 0; i--) k = (k << 8) | t[i];
    if (len & 3) { k *= c1; k = rotl32(k, 15); k *= c2; h ^= k; }
    h ^= (u32)len;
    wr32(out, fmix32(h));
}

static void murmur3_x86_128(const u8 *d, int len, u32 seed, u8 *out) {
    u32 h1 = seed, h2 = seed, h3 = seed, h4 = seed;
    const u32 c1 = 0x239b961bu, c2 = 0xab0e9789u, c3 = 0x38b34ae5u, c4 = 0xa1e38b93u;
    int nb = len / 16;
    for (int i = 0; i < nb; i++) {
        u32 k1 = rd32(d + 16 * i), k2 = rd32(d + 16 * i + 4), k3 = rd32(d + 16 * i + 8), k4 = rd32(d + 16 * i + 12);
        k1 *= c1; k1 = rotl32(k1, 15); k1 *= c2; h1 ^= k1; h1 = rotl32(h1, 19); h1 += h2; h1 = h1 * 5 + 0x561ccd1bu;
        k2 *= c2; k2 = rotl32(k2, 16); k2 *= c3; h2 ^= k2; h2 = rotl32(h2, 17); h2 += h3; h2 = h2 * 5 + 0x0bcaa747u;
        k3 *= c3; k3 = rotl32(k3, 17); k3 *= c4; h3 ^= k3; h3 = rotl32(h3, 15); h3 += h4; h3 = h3 * 5 + 0x96cd1c35u;
        k4 *= c4; k4 = rotl32(k4, 18); k4 *= c1; h4 ^= k4; h4 = rotl32(h4, 13); h4 += h1; h4 = h4 * 5 + 0x32ac3b17u;
    }
    const u8 *t = d + 16 * nb;
    int rem = len & 15;
    u32 k[4] = {0, 0, 0, 0};
    for (int i = rem - 1; i >= 0; i--) k[i >> 2] = (k[i >> 2] << 8) | t[i];
    if (rem > 12) { k[3] *= c4; k[3] = rotl32(k[3], 18); k[3] *= c1; h4 ^= k[3]; }
    if (rem > 8)  { k[2] *= c3; k[2] = rotl32(k[2], 17); k[2] *= c4; h3 ^= k[2]; }
    if (rem > 4)  { k[1] *= c2; k[1] = rotl32(k[1], 16); k[1] *= c3; h2 ^= k[1]; }
    if (rem > 0)  { k[0] *= c1; k[0] = rotl32(k[0], 15); k[0] *= c2; h1 ^= k[0]; }
    h1 ^= (u32)len; h2 ^= (u32)len; h3 ^= (u32)len; h4 ^= (u32)len;
    h1 += h2; h1 += h3; h1 += h4; h2 += h1; h3 += h1; h4 += h1;
    h1 = fmix32(h1); h2 = fmix32(h2); h3 = fmix32(h3); h4 = fmix32(h4);
    h1 += h2; h1 += h3; h1 += h4; h2 += h1; h3 += h1; h4 += h1;
    wr32(out, h1); wr32(out + 4, h2); wr32(out + 8, h3); wr32(out + 12, h4);
}

static void murmur3_x64_128(const u8 *d, int len, u32 seed, u8 *out) {
    u64 h1 = seed, h2 = seed;
    const u64 c1 = 0x87c37b91114253d5ULL, c2 = 0x4cf5ad432745937fULL;
    int nb = len / 16;
    for (int i = 0; i < nb; i++) {
        u64 k1 = rd64(d + 16 * i), k2 = rd64(d + 16 * i + 8);
        k1 *= c1; k1 = rotl64(k1, 31); k1 *= c2; h1 ^= k1; h1 = rotl64(h1, 27); h1 += h2; h1 = h1 * 5 + 0x52dce729u;
        k2 *= c2; k2 = rotl64(k2, 33); k2 *= c1; h2 ^= k2; h2 = rotl64(h2, 31); h2 += h1; h2 = h2 * 5 + 0x38495ab5u;
    }
    const u8 *t = d + 16 * nb;
    int rem = len & 15;
    u64 k[2] = {0, 0};
    for (int i = rem - 1; i >= 0; i--) k[i >> 3] = (k[i >> 3] << 8) | t[i];
    if (rem > 8) { k[1] *= c2; k[1] = rotl64(k[1], 33); k[1] *= c1; h2 ^= k[1]; }
    if (rem > 0) { k[0] *= c1; k[0] = rotl64(k[0], 31); k[0] *= c2; h1 ^= k[0]; }
    h1 ^= (u64)len; h2 ^= (u64)len;
    h1 += h2; h2 += h1;
    h1 = fmix64(h1); h2 = fmix64(h2);
    h1 += h2; h2 += h1;
    wr64(out, h1); wr64(out + 8, h2);
}

typedef void (*hashfn)(const u8 *, int, u32, u8 *);

// The SMHasher verification procedure; returns the 32-bit verification value.
static u32 verification(hashfn h, int hashbytes) {
    static u8 key[256], hashes[16 * 256];
    u8 fin[16];
    for (int i = 0; i < 256; i++) { key[i] = (u8)i; h(key, i, (u32)(256 - i), &hashes[i * hashbytes]); }
    h(hashes, hashbytes * 256, 0, fin);
    return rd32(fin);
}

int paper_main(void) {
    int ok = 1;
    ok &= pcheck_u64("MurmurHash3_x86_32  verification value", verification(murmur3_x86_32, 4), 0xB0F57EE3u, 8);
    ok &= pcheck_u64("MurmurHash3_x86_128 verification value", verification(murmur3_x86_128, 16), 0xB3ECE62Au, 8);
    ok &= pcheck_u64("MurmurHash3_x64_128 verification value", verification(murmur3_x64_128, 16), 0x6384BA69u, 8);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
