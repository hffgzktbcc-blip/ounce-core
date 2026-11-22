// XXH32 written from the algorithm description in the xxHash specification (Yann Collet, xxhash_spec.md). Check: the
// project's published sanity vectors, all 8322 of them: the XXH32 digest of the first n bytes of the project's pseudo-random
// test buffer for every length n = 0..4160 with seed 0 and with seed 0x9E3779B1. The buffer generator is the one the project
// states must never change: byte i = top byte of the 64-bit state, the state starting at 2654435761 and multiplied by
// 11400714785074694797 (mod 2^64) after each byte. The vectors are data copied as numbers; no code was copied. Exit 0 only if
// all 8322 digests match.
#include "ounce_paper.h"

#ifndef XXH32_P2     // negative control hook: PRIME32_2 (published value: 0x85EBCA77)
#define XXH32_P2 0x85EBCA77u
#endif
#define P1 0x9E3779B1u
#define P2 XXH32_P2
#define P3 0xC2B2AE3Du
#define P4 0x27D4EB2Fu
#define P5 0x165667B1u

static const u32 V0[4161] = {
#include "vectors_seed0.inc"
};
static const u32 V1[4161] = {
#include "vectors_seed1.inc"
};

static u32 rotl(u32 x, int r) { return (x << r) | (x >> (32 - r)); }
static u32 rd32(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }
static u32 round1(u32 acc, u32 lane) { acc += lane * P2; acc = rotl(acc, 13); return acc * P1; }

static u32 xxh32(const u8 *p, unsigned long n, u32 seed) {
    unsigned long rem = n;
    u32 acc;
    if (n >= 16) {
        u32 a1 = seed + P1 + P2, a2 = seed + P2, a3 = seed, a4 = seed - P1;
        for (; rem >= 16; rem -= 16, p += 16) {
            a1 = round1(a1, rd32(p)); a2 = round1(a2, rd32(p + 4)); a3 = round1(a3, rd32(p + 8)); a4 = round1(a4, rd32(p + 12));
        }
        acc = rotl(a1, 1) + rotl(a2, 7) + rotl(a3, 12) + rotl(a4, 18);
    } else {
        acc = seed + P5;
    }
    acc += (u32)n;
    for (; rem >= 4; rem -= 4, p += 4) { acc += rd32(p) * P3; acc = rotl(acc, 17) * P4; }
    for (; rem >= 1; rem -= 1, p += 1) { acc += (u32)*p * P5; acc = rotl(acc, 11) * P1; }
    acc ^= acc >> 15; acc *= P2; acc ^= acc >> 13; acc *= P3; acc ^= acc >> 16;
    return acc;
}

static u8 buf[4200];

int paper_main(void) {
    u64 st = 2654435761ULL;
    for (int i = 0; i < 4200; i++) { buf[i] = (u8)(st >> 56); st *= 11400714785074694797ULL; }
    int good0 = 0, good1 = 0, shown = 0;
    for (int n = 0; n <= 4160; n++) {
        u32 h0 = xxh32(buf, (unsigned long)n, 0), h1 = xxh32(buf, (unsigned long)n, 0x9E3779B1u);
        good0 += h0 == V0[n]; good1 += h1 == V1[n];
        if ((h0 != V0[n] || h1 != V1[n]) && shown < 4) {
            shown++; pstr("FAIL length "); pdec(n); pstr(": seed0 "); phex(h0, 8); pstr(" expected "); phex(V0[n], 8);
            pstr(", seed 0x9E3779B1 "); phex(h1, 8); pstr(" expected "); phex(V1[n], 8); pstr("\n");
        }
    }
    pstr("XXH32 seed 0: "); pdec(good0); pstr(" of 4161 match\n");
    pstr("XXH32 seed 0x9E3779B1: "); pdec(good1); pstr(" of 4161 match\n");
    int ok = good0 == 4161 && good1 == 4161;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
