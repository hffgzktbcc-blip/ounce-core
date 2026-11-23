// HighwayHash (64-bit output) from Alakuijala, Cox and Wassenberg, "Fast keyed hash/pseudo-random function using SIMD multiply
// and permute" (arXiv:1612.06257, 2016/2017). Written from the paper's description: the Update step (v1 += packet; v1 += mul0;
// mul0 ^= lo32(v1) * hi32(v0); v0 += mul1; mul1 ^= lo32(v0) * hi32(v1); the two ZipperMerge additions), the zipper-merge byte
// permutation 7 8 6 9 D A 4 B 0 F 1 E 5 2 C 3, the four finalization rounds with the 128-bit-half / 32-bit-half swap of v0, and
// the initialisation from the key. The paper does not print the pi-derived state constants or the exact layout of the final
// partial packet, so those two details were taken from the authors' published C reference code (the constants as data; for the
// tail: size mod 32 is added to v0, the 32-bit halves of v1 are rotated by it, and the leftover bytes are copied into a zero
// packet by a fixed rule, written out below in the program's own words). No code was
// copied. Checks: the authors' 65 published HighwayHash64 values for the key 00..1f and the messages 00 01 02 ... of length
// 0..64 (these exercise every remainder length and the one- and two-packet paths), plus their one published value for a second
// key {1,2,3,4} and a 33-byte message 0x80..0xa0. Exit 0 only if all 66 match.
#include "ounce_paper.h"

#ifndef HH_ROUNDS     // negative control hook: finalization rounds (paper: 4)
#define HH_ROUNDS 4
#endif

static const u64 KEXP[65] = {
#include "vectors.inc"
};

typedef struct { u64 v0[4], v1[4], mul0[4], mul1[4]; } hh_state;

static u64 rd64(const u8 *p) { u64 r = 0; for (int i = 7; i >= 0; i--) r = (r << 8) | p[i]; return r; }
static u64 swap32(u64 x) { return (x >> 32) | (x << 32); }

static void hh_reset(hh_state *s, const u64 key[4]) {
    static const u64 C0[4] = {0xdbe6d5d5fe4cce2fULL, 0xa4093822299f31d0ULL, 0x13198a2e03707344ULL, 0x243f6a8885a308d3ULL};
    static const u64 C1[4] = {0x3bd39e10cb0ef593ULL, 0xc0acf169b5f18a8cULL, 0xbe5466cf34e90c6cULL, 0x452821e638d01377ULL};
    for (int i = 0; i < 4; i++) {
        s->mul0[i] = C0[i]; s->mul1[i] = C1[i];
        s->v0[i] = C0[i] ^ key[i];
        s->v1[i] = C1[i] ^ swap32(key[i]);           // key lane rotated by 32 bits
    }
}

// ZipperMerge of the lane pair (lo, hi) = 16 source bytes S; result byte i is S[P_i], P_15..P_0 = 7 8 6 9 D A 4 B 0 F 1 E 5 2 C 3.
// The result is added to the two lanes of 'dst' (dst_lo gets result bytes 0..7, dst_hi bytes 8..15).
static void zipper_add(u64 lo, u64 hi, u64 *dst_lo, u64 *dst_hi) {
    static const u8 P[16] = {3, 0xC, 2, 5, 0xE, 1, 0xF, 0, 0xB, 4, 0xA, 0xD, 9, 6, 8, 7};   // P[i] for i = 0..15
    u8 S[16], R[16];
    for (int i = 0; i < 8; i++) { S[i] = (u8)(lo >> (8 * i)); S[8 + i] = (u8)(hi >> (8 * i)); }
    for (int i = 0; i < 16; i++) R[i] = S[P[i]];
    *dst_lo += rd64(R); *dst_hi += rd64(R + 8);
}

static void update(hh_state *s, const u64 pk[4]) {
    for (int i = 0; i < 4; i++) {
        s->v1[i] += pk[i];
        s->v1[i] += s->mul0[i];
        s->mul0[i] ^= (s->v1[i] & 0xffffffffULL) * (s->v0[i] >> 32);
        s->v0[i] += s->mul1[i];
        s->mul1[i] ^= (s->v0[i] & 0xffffffffULL) * (s->v1[i] >> 32);
    }
    zipper_add(s->v1[0], s->v1[1], &s->v0[0], &s->v0[1]);
    zipper_add(s->v1[2], s->v1[3], &s->v0[2], &s->v0[3]);
    zipper_add(s->v0[0], s->v0[1], &s->v1[0], &s->v1[1]);
    zipper_add(s->v0[2], s->v0[3], &s->v1[2], &s->v1[3]);
}

static u32 rotl32(u32 x, unsigned r) { return (x << r) | (x >> (32 - r)); }

static void update_remainder(hh_state *s, const u8 *tail, unsigned n) {   // 1 <= n <= 31 bytes left over
    for (int i = 0; i < 4; i++) s->v0[i] += ((u64)n << 32) + n;
    for (int i = 0; i < 4; i++) {                                         // rotate both 32-bit halves of each v1 lane by n
        u32 a = (u32)s->v1[i], b = (u32)(s->v1[i] >> 32);
        s->v1[i] = (u64)rotl32(a, n) | ((u64)rotl32(b, n) << 32);
    }
    u8 pk[32];
    for (int i = 0; i < 32; i++) pk[i] = 0;
    unsigned whole = n & ~3u, rest = n & 3u;
    for (unsigned i = 0; i < whole; i++) pk[i] = tail[i];
    if (n & 16) { for (unsigned i = 0; i < 4; i++) pk[28 + i] = tail[n - 4 + i]; }   // last 4 input bytes
    else if (rest) { pk[16] = tail[whole]; pk[17] = tail[whole + (rest >> 1)]; pk[18] = tail[whole + rest - 1]; }
    u64 lanes[4] = {rd64(pk), rd64(pk + 8), rd64(pk + 16), rd64(pk + 24)};
    update(s, lanes);
}

static u64 highwayhash64(const u8 *d, unsigned long n, const u64 key[4]) {
    hh_state s;
    hh_reset(&s, key);
    unsigned long i = 0;
    for (; i + 32 <= n; i += 32) { u64 l[4] = {rd64(d + i), rd64(d + i + 8), rd64(d + i + 16), rd64(d + i + 24)}; update(&s, l); }
    if (n & 31) update_remainder(&s, d + i, (unsigned)(n & 31));
    for (int r = 0; r < HH_ROUNDS; r++) {
        u64 perm[4] = {swap32(s.v0[2]), swap32(s.v0[3]), swap32(s.v0[0]), swap32(s.v0[1])};   // swap 128-bit halves and 32-bit halves
        update(&s, perm);
    }
    return s.v0[0] + s.v1[0] + s.mul0[0] + s.mul1[0];
}

int paper_main(void) {
    static const u64 KEY1[4] = {0x0706050403020100ULL, 0x0F0E0D0C0B0A0908ULL, 0x1716151413121110ULL, 0x1F1E1D1C1B1A1918ULL};
    static const u64 KEY2[4] = {1, 2, 3, 4};
    u8 data[65];
    for (int i = 0; i < 65; i++) data[i] = (u8)i;
    int good = 0;
    for (int n = 0; n <= 64; n++) {
        u64 h = highwayhash64(data, (unsigned long)n, KEY1);
        if (h == KEXP[n]) good++;
        else { pstr("FAIL length "); pdec(n); pstr(": "); phex(h, 16); pstr(" expected "); phex(KEXP[n], 16); pstr("\n"); }
    }
    pstr("key 00..1f, lengths 0..64: "); pdec(good); pstr(" of 65 match\n");
    u8 d2[33];
    for (int i = 0; i < 33; i++) d2[i] = (u8)(128 + i);
    int ok = good == 65;
    ok &= pcheck_u64("key {1,2,3,4}, 33 bytes 0x80..0xa0", highwayhash64(d2, 33, KEY2), 0x53c516cce478cad7ULL, 16);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
