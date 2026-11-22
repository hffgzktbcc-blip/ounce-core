// XXH64 written from the algorithm description in the xxHash specification (Yann Collet, xxhash_spec.md). Check: the
// project's published sanity vectors, all 8322 of them: the XXH64 digest of the first n bytes of the project's pseudo-random
// test buffer for every length n = 0..4160 with seed 0 and with seed 0x9E3779B1. The buffer generator is the one the project
// states must never change: byte i = top byte of the 64-bit state, the state starting at 2654435761 and multiplied by
// 11400714785074694797 (mod 2^64) after each byte. The vectors are data copied as numbers; no code was copied. Exit 0 only if
// all 8322 digests match.
#include "ounce_paper.h"

#ifndef XXH64_ROT    // negative control hook: rotation in the stripe round (published value: 31)
#define XXH64_ROT 31
#endif
#define P1 0x9E3779B185EBCA87ULL
#define P2 0xC2B2AE3D27D4EB4FULL
#define P3 0x165667B19E3779F9ULL
#define P4 0x85EBCA77C2B2AE63ULL
#define P5 0x27D4EB2F165667C5ULL

static const u64 V0[4161] = {
#include "vectors_seed0.inc"
};
static const u64 V1[4161] = {
#include "vectors_seed1.inc"
};

static u64 rotl(u64 x, int r) { return (x << r) | (x >> (64 - r)); }
static u32 rd32(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }
static u64 rd64(const u8 *p) { return (u64)rd32(p) | (u64)rd32(p + 4) << 32; }
static u64 rnd(u64 acc, u64 lane) { acc += lane * P2; acc = rotl(acc, XXH64_ROT); return acc * P1; }
static u64 merge(u64 acc, u64 a) { acc ^= rnd(0, a); acc *= P1; return acc + P4; }

static u64 xxh64(const u8 *p, unsigned long n, u64 seed) {
    unsigned long rem = n;
    u64 acc;
    if (n >= 32) {
        u64 a1 = seed + P1 + P2, a2 = seed + P2, a3 = seed, a4 = seed - P1;
        for (; rem >= 32; rem -= 32, p += 32) {
            a1 = rnd(a1, rd64(p)); a2 = rnd(a2, rd64(p + 8)); a3 = rnd(a3, rd64(p + 16)); a4 = rnd(a4, rd64(p + 24));
        }
        acc = rotl(a1, 1) + rotl(a2, 7) + rotl(a3, 12) + rotl(a4, 18);
        acc = merge(acc, a1); acc = merge(acc, a2); acc = merge(acc, a3); acc = merge(acc, a4);
    } else {
        acc = seed + P5;
    }
    acc += (u64)n;
    for (; rem >= 8; rem -= 8, p += 8) { acc ^= rnd(0, rd64(p)); acc = rotl(acc, 27) * P1; acc += P4; }
    if (rem >= 4) { acc ^= (u64)rd32(p) * P1; acc = rotl(acc, 23) * P2; acc += P3; rem -= 4; p += 4; }
    for (; rem >= 1; rem -= 1, p += 1) { acc ^= (u64)*p * P5; acc = rotl(acc, 11) * P1; }
    acc ^= acc >> 33; acc *= P2; acc ^= acc >> 29; acc *= P3; acc ^= acc >> 32;
    return acc;
}

static u8 buf[4200];

int paper_main(void) {
    u64 st = 2654435761ULL;
    for (int i = 0; i < 4200; i++) { buf[i] = (u8)(st >> 56); st *= 11400714785074694797ULL; }
    int good0 = 0, good1 = 0, shown = 0;
    for (int n = 0; n <= 4160; n++) {
        u64 h0 = xxh64(buf, (unsigned long)n, 0), h1 = xxh64(buf, (unsigned long)n, 0x9E3779B1u);
        good0 += h0 == V0[n]; good1 += h1 == V1[n];
        if ((h0 != V0[n] || h1 != V1[n]) && shown < 4) {
            shown++; pstr("FAIL length "); pdec(n); pstr(": seed0 "); phex(h0, 16); pstr(" expected "); phex(V0[n], 16);
            pstr(", seed 0x9E3779B1 "); phex(h1, 16); pstr(" expected "); phex(V1[n], 16); pstr("\n");
        }
    }
    pstr("XXH64 seed 0: "); pdec(good0); pstr(" of 4161 match\n");
    pstr("XXH64 seed 0x9E3779B1: "); pdec(good1); pstr(" of 4161 match\n");
    int ok = good0 == 4161 && good1 == 4161;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
