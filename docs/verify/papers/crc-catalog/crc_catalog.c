// The parametrised CRC model of Ross N. Williams, "A Painless Guide to CRC Error Detection Algorithms" (1993): a CRC algorithm is
// fixed by width, poly, init, refin, refout and xorout. This program implements the model bit by bit (message bits are fed into a
// register of the stated width, least significant bit of each byte first when refin is set; the register is reflected when refout
// is set and then XORed with xorout) and checks it against two sets of published numbers:
//   1. the guide itself: the section 6 worked example (message 1101011011, poly 10011: checksum 1110, and the transmission
//      11010110111110 leaves remainder 0) and the two complete parameter sets that come with a check value ("CRC-16": poly 8005,
//      init 0000, reflected, check BB3D, and "CRC-32": poly 04C11DB7, init FFFFFFFF, reflected, xorout FFFFFFFF, check
//      CBF43926), 4 values;
//   2. all 113 entries of the RevEng "Catalogue of parametrised CRC algorithms" (widths 3 to 82), each with its published check
//      value (the CRC of the nine ASCII bytes "123456789") and its published residue (the register, reflected if refout, after
//      the message and then its own CRC have been fed in, before xorout), 226 values.
// All numbers are data copied from those sources; none of the guide's code, and no other code, was copied. Exit 0 only if all 230 match.
#include "ounce_paper.h"

#ifdef CRC_IGNORE_REFIN     // second negative control hook: feed every byte most significant bit first, whatever refin says
#define REFIN(m) 0
#else
#define REFIN(m) ((m)->refin)
#endif
#ifndef GUIDE_CRC32_POLY    // negative control hook: poly of the guide's CRC-32 example (published: 0x04C11DB7)
#define GUIDE_CRC32_POLY 0x04C11DB7ULL
#endif

typedef unsigned __int128 u128;
typedef struct { const char *name; int width, refin, refout; u64 poly[2], init[2], xorout[2], check[2], residue[2]; } entry;

static const entry CAT[113] = {
#include "catalog.inc"
};

static u128 v128(const u64 p[2]) { return (u128)p[0] | ((u128)p[1] << 64); }
static u128 reflect(u128 x, int w) { u128 r = 0; for (int i = 0; i < w; i++) if ((x >> i) & 1) r |= (u128)1 << (w - 1 - i); return r; }

typedef struct { int width, refin, refout; u128 poly, init, xorout; } model;

static u128 feed_bit(u128 reg, int bit, const model *m) {
    u128 mask = (m->width == 128) ? ~(u128)0 : (((u128)1 << m->width) - 1);
    int top = (int)((reg >> (m->width - 1)) & 1);
    reg = (reg << 1) & mask;
    if (top ^ bit) reg ^= m->poly;
    return reg;
}
// Run the message bits through the register; returns the register before reflection and xorout.
static u128 run(const u8 *msg, int n, const model *m, int extra_bits, u128 extra) {
    u128 reg = m->init;
    for (int i = 0; i < n; i++)
        for (int k = 0; k < 8; k++) reg = feed_bit(reg, (msg[i] >> (REFIN(m) ? k : 7 - k)) & 1, m);
    for (int i = 0; i < extra_bits; i++) reg = feed_bit(reg, (int)((extra >> (extra_bits - 1 - i)) & 1), m);   // most significant bit first
    return reg;
}
static u128 crc_of(const u8 *msg, int n, const model *m) {
    u128 r = run(msg, n, m, 0, 0);
    if (m->refout) r = reflect(r, m->width);
    return r ^ m->xorout;
}
// residue: feed the message followed by its own CRC (as the bit stream the CRC occupies when transmitted), read the register
// back (reflected if refout), without xorout
static u128 residue_of(const u8 *msg, int n, const model *m) {
    u128 r0 = run(msg, n, m, 0, 0);
    u128 xo = m->refout ? reflect(m->xorout, m->width) : m->xorout;
    u128 r = run(msg, n, m, m->width, r0 ^ xo);
    return m->refout ? reflect(r, m->width) : r;
}

static void p128(u128 v, int width) {     // hex, minimal digits for the width
    int digits = (width + 3) / 4;
    for (int i = digits - 1; i >= 0; i--) { char c = "0123456789abcdef"[(int)((v >> (4 * i)) & 15)]; paper_write(&c, 1); }
}

int paper_main(void) {
    static const u8 MSG[9] = {'1', '2', '3', '4', '5', '6', '7', '8', '9'};
    int ok = 1, matched = 0, shown = 0;
    // 1a. the guide's section 6 worked example: 4-bit poly 10011, init 0, message and transmission given as bit strings
    {
        static const model W4 = {4, 0, 0, 0x3, 0, 0};
        u128 reg = 0;
        const char *msg = "1101011011", *sent = "11010110111110";
        for (const char *q = msg; *q; q++) reg = feed_bit(reg, *q - '0', &W4);
        ok &= pcheck_u64("Williams guide section 6, checksum of 1101011011", (u64)reg, 0xE, 1);
        reg = 0;
        for (const char *q = sent; *q; q++) reg = feed_bit(reg, *q - '0', &W4);
        ok &= pcheck_u64("Williams guide section 6, remainder of 11010110111110", (u64)reg, 0x0, 1);
    }
    // 1b. the guide's own two complete parameter sets
    static const model G16 = {16, 1, 1, 0x8005, 0x0000, 0x0000};
    model g32 = {32, 1, 1, GUIDE_CRC32_POLY, 0xFFFFFFFFu, 0xFFFFFFFFu};
    u128 c16 = crc_of(MSG, 9, &G16), c32 = crc_of(MSG, 9, &g32);
    ok &= pcheck_u64("Williams guide, CRC-16 check", (u64)c16, 0xBB3D, 4);
    ok &= pcheck_u64("Williams guide, CRC-32 check", (u64)c32, 0xCBF43926u, 8);
    // 2. the catalogue
    for (int i = 0; i < 113; i++) {
        const entry *e = &CAT[i];
        model m = {e->width, e->refin, e->refout, v128(e->poly), v128(e->init), v128(e->xorout)};
        u128 c = crc_of(MSG, 9, &m), r = residue_of(MSG, 9, &m);
        int good = c == v128(e->check) && r == v128(e->residue);
        matched += good;
        if (!good && shown < 6) {
            shown++; pstr("FAIL "); pstr(e->name); pstr(": check "); p128(c, e->width); pstr(" expected "); p128(v128(e->check), e->width);
            pstr(", residue "); p128(r, e->width); pstr(" expected "); p128(v128(e->residue), e->width); pstr("\n");
        }
    }
    pstr("catalogue entries with matching check and residue: "); pdec(matched); pstr(" of 113\n");
    ok &= matched == 113;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
