// SKIPJACK from the NIST document "SKIPJACK and KEA Algorithm Specifications", version 2.0, 29 May 1998 (the algorithm's
// designers' public specification; it is not a journal paper), written from sections B.2 to B.7: 32 steps alternating the two
// stepping rules A and B, the four-round Feistel G-permutation built on the F-table, and the 10-byte key used cyclically.
// Checks the 80 variable-key known-answer ciphertexts of NIST SP 800-17 Appendix B Table 6 (plaintext 0, key = a single set
// bit in each of the 80 positions), plus a decrypt round trip for each. The values come from the Crypto++ test-vector file that
// cites SP 800-17 Table 6 and were cross-read against the SP 800-17 PDF (see paper.json). The F-table (spec section B.7) and
// the vectors are data copied as numbers; no code was copied.
#include "ounce_paper.h"

typedef struct { const char *name; const char *key, *pt, *ct; } vec;
// name, key (10 bytes), plaintext (8 bytes), ciphertext (8 bytes)
static const vec VEC[] = {
    {"Table 6 round 0", "80000000000000000000", "0000000000000000", "7a00e49441461f5a"},
    {"Table 6 round 1", "40000000000000000000", "0000000000000000", "a14ff8bcd1bc9ef9"},
    {"Table 6 round 2", "20000000000000000000", "0000000000000000", "d7e810385a42aaea"},
    {"Table 6 round 3", "10000000000000000000", "0000000000000000", "28fe2c3332aabd35"},
    {"Table 6 round 4", "08000000000000000000", "0000000000000000", "3fc0f05ee6ce788a"},
    {"Table 6 round 5", "04000000000000000000", "0000000000000000", "443dd0cb7526f74b"},
    {"Table 6 round 6", "02000000000000000000", "0000000000000000", "ad819e677cf90305"},
    {"Table 6 round 7", "01000000000000000000", "0000000000000000", "9891755e5eba5b1d"},
    {"Table 6 round 8", "00800000000000000000", "0000000000000000", "0e64b494633bf2cb"},
    {"Table 6 round 9", "00400000000000000000", "0000000000000000", "63381a08a47fc48d"},
    {"Table 6 round 10", "00200000000000000000", "0000000000000000", "f4108b099b047040"},
    {"Table 6 round 11", "00100000000000000000", "0000000000000000", "740216614ed0e25b"},
    {"Table 6 round 12", "00080000000000000000", "0000000000000000", "8000917b2e16b92a"},
    {"Table 6 round 13", "00040000000000000000", "0000000000000000", "a9769b62b3a0be4e"},
    {"Table 6 round 14", "00020000000000000000", "0000000000000000", "42fdb872ea314121"},
    {"Table 6 round 15", "00010000000000000000", "0000000000000000", "1d672ba0156ab39d"},
    {"Table 6 round 16", "00008000000000000000", "0000000000000000", "f44441d7c777f057"},
    {"Table 6 round 17", "00004000000000000000", "0000000000000000", "ea487ddc360d1594"},
    {"Table 6 round 18", "00002000000000000000", "0000000000000000", "324b0e785ff2b908"},
    {"Table 6 round 19", "00001000000000000000", "0000000000000000", "1af59ec2b9d64c4f"},
    {"Table 6 round 20", "00000800000000000000", "0000000000000000", "819b7e102e76a0ee"},
    {"Table 6 round 21", "00000400000000000000", "0000000000000000", "0b0bfe0d4a37aa9e"},
    {"Table 6 round 22", "00000200000000000000", "0000000000000000", "12b43e3760d30da6"},
    {"Table 6 round 23", "00000100000000000000", "0000000000000000", "3177256c461541ee"},
    {"Table 6 round 24", "00000080000000000000", "0000000000000000", "3600eb92836ca026"},
    {"Table 6 round 25", "00000040000000000000", "0000000000000000", "75a435ad22ecf793"},
    {"Table 6 round 26", "00000020000000000000", "0000000000000000", "7190aa9913c1f9ec"},
    {"Table 6 round 27", "00000010000000000000", "0000000000000000", "aba718b185a11dd0"},
    {"Table 6 round 28", "00000008000000000000", "0000000000000000", "40f67abfcc3b873c"},
    {"Table 6 round 29", "00000004000000000000", "0000000000000000", "38a0a58fb09728f2"},
    {"Table 6 round 30", "00000002000000000000", "0000000000000000", "ca702e49bf6fa645"},
    {"Table 6 round 31", "00000001000000000000", "0000000000000000", "455d93f039ea0860"},
    {"Table 6 round 32", "00000000800000000000", "0000000000000000", "5347643fe803883f"},
    {"Table 6 round 33", "00000000400000000000", "0000000000000000", "f40ff1dcba2bc1e5"},
    {"Table 6 round 34", "00000000200000000000", "0000000000000000", "574a4848369d412e"},
    {"Table 6 round 35", "00000000100000000000", "0000000000000000", "b2be936e36670636"},
    {"Table 6 round 36", "00000000080000000000", "0000000000000000", "5c88517d2742e619"},
    {"Table 6 round 37", "00000000040000000000", "0000000000000000", "993c89d09a2fe556"},
    {"Table 6 round 38", "00000000020000000000", "0000000000000000", "1a3f72da694c9fc7"},
    {"Table 6 round 39", "00000000010000000000", "0000000000000000", "9659d5228f4cb151"},
    {"Table 6 round 40", "00000000008000000000", "0000000000000000", "7c13f49e750f5c30"},
    {"Table 6 round 41", "00000000004000000000", "0000000000000000", "3500bd407bcd01f6"},
    {"Table 6 round 42", "00000000002000000000", "0000000000000000", "85c58e3c49442028"},
    {"Table 6 round 43", "00000000001000000000", "0000000000000000", "8413840a2d48abea"},
    {"Table 6 round 44", "00000000000800000000", "0000000000000000", "832850e6e5c4ae5a"},
    {"Table 6 round 45", "00000000000400000000", "0000000000000000", "29e97f0d9f0fdc5f"},
    {"Table 6 round 46", "00000000000200000000", "0000000000000000", "2c45230437ff2e04"},
    {"Table 6 round 47", "00000000000100000000", "0000000000000000", "10c409fb872a984f"},
    {"Table 6 round 48", "00000000000080000000", "0000000000000000", "14693b30c3af7470"},
    {"Table 6 round 49", "00000000000040000000", "0000000000000000", "913a9050d585bab9"},
    {"Table 6 round 50", "00000000000020000000", "0000000000000000", "5bfb0f83ab0c6eea"},
    {"Table 6 round 51", "00000000000010000000", "0000000000000000", "6c0ca7284d836aae"},
    {"Table 6 round 52", "00000000000008000000", "0000000000000000", "ac5727d612e185e8"},
    {"Table 6 round 53", "00000000000004000000", "0000000000000000", "38d7d596a3d29d90"},
    {"Table 6 round 54", "00000000000002000000", "0000000000000000", "78badad3bc436ca2"},
    {"Table 6 round 55", "00000000000001000000", "0000000000000000", "e405778741b04ba0"},
    {"Table 6 round 56", "00000000000000800000", "0000000000000000", "72ffe43dea02afa5"},
    {"Table 6 round 57", "00000000000000400000", "0000000000000000", "52e931df248ce4c7"},
    {"Table 6 round 58", "00000000000000200000", "0000000000000000", "4bb165fdb3bff65c"},
    {"Table 6 round 59", "00000000000000100000", "0000000000000000", "7cfafa6861d7b47d"},
    {"Table 6 round 60", "00000000000000080000", "0000000000000000", "48d1755231f87a2a"},
    {"Table 6 round 61", "00000000000000040000", "0000000000000000", "413207da1c9b6ab5"},
    {"Table 6 round 62", "00000000000000020000", "0000000000000000", "63f818e9382a2778"},
    {"Table 6 round 63", "00000000000000010000", "0000000000000000", "edaf2b85fc30eb09"},
    {"Table 6 round 64", "00000000000000008000", "0000000000000000", "11fc5993820763f7"},
    {"Table 6 round 65", "00000000000000004000", "0000000000000000", "e539c3969915092f"},
    {"Table 6 round 66", "00000000000000002000", "0000000000000000", "506f6a1e834ad8f7"},
    {"Table 6 round 67", "00000000000000001000", "0000000000000000", "8b15ba3047fa3195"},
    {"Table 6 round 68", "00000000000000000800", "0000000000000000", "130be15c393e4b7a"},
    {"Table 6 round 69", "00000000000000000400", "0000000000000000", "8895ec3104ca1041"},
    {"Table 6 round 70", "00000000000000000200", "0000000000000000", "e440acdf4b64c9c9"},
    {"Table 6 round 71", "00000000000000000100", "0000000000000000", "c23280ebe093f002"},
    {"Table 6 round 72", "00000000000000000080", "0000000000000000", "5264a65741fe78e3"},
    {"Table 6 round 73", "00000000000000000040", "0000000000000000", "80892e768547ce61"},
    {"Table 6 round 74", "00000000000000000020", "0000000000000000", "0911412d72093475"},
    {"Table 6 round 75", "00000000000000000010", "0000000000000000", "9f21aa764783e649"},
    {"Table 6 round 76", "00000000000000000008", "0000000000000000", "4ca9fabead2c02c6"},
    {"Table 6 round 77", "00000000000000000004", "0000000000000000", "59ce10973a7b1fd5"},
    {"Table 6 round 78", "00000000000000000002", "0000000000000000", "683b2934e0ccbeaa"},
    {"Table 6 round 79", "00000000000000000001", "0000000000000000", "74d0e7c2e3b450a8"},
};
#define NVEC ((int)(sizeof VEC / sizeof VEC[0]))


// F-table, spec section B.7, row = high nibble of the input, column = low nibble (the spec's own example: F(0x7a) = 0xd6).
static const u8 F[256] = {
    0xa3,0xd7,0x09,0x83,0xf8,0x48,0xf6,0xf4,0xb3,0x21,0x15,0x78,0x99,0xb1,0xaf,0xf9,
    0xe7,0x2d,0x4d,0x8a,0xce,0x4c,0xca,0x2e,0x52,0x95,0xd9,0x1e,0x4e,0x38,0x44,0x28,
    0x0a,0xdf,0x02,0xa0,0x17,0xf1,0x60,0x68,0x12,0xb7,0x7a,0xc3,0xe9,0xfa,0x3d,0x53,
    0x96,0x84,0x6b,0xba,0xf2,0x63,0x9a,0x19,0x7c,0xae,0xe5,0xf5,0xf7,0x16,0x6a,0xa2,
    0x39,0xb6,0x7b,0x0f,0xc1,0x93,0x81,0x1b,0xee,0xb4,0x1a,0xea,0xd0,0x91,0x2f,0xb8,
    0x55,0xb9,0xda,0x85,0x3f,0x41,0xbf,0xe0,0x5a,0x58,0x80,0x5f,0x66,0x0b,0xd8,0x90,
    0x35,0xd5,0xc0,0xa7,0x33,0x06,0x65,0x69,0x45,0x00,0x94,0x56,0x6d,0x98,0x9b,0x76,
    0x97,0xfc,0xb2,0xc2,0xb0,0xfe,0xdb,0x20,0xe1,0xeb,0xd6,0xe4,0xdd,0x47,0x4a,0x1d,
    0x42,0xed,0x9e,0x6e,0x49,0x3c,0xcd,0x43,0x27,0xd2,0x07,0xd4,0xde,0xc7,0x67,0x18,
    0x89,0xcb,0x30,0x1f,0x8d,0xc6,0x8f,0xaa,0xc8,0x74,0xdc,0xc9,0x5d,0x5c,0x31,0xa4,
    0x70,0x88,0x61,0x2c,0x9f,0x0d,0x2b,0x87,0x50,0x82,0x54,0x64,0x26,0x7d,0x03,0x40,
    0x34,0x4b,0x1c,0x73,0xd1,0xc4,0xfd,0x3b,0xcc,0xfb,0x7f,0xab,0xe6,0x3e,0x5b,0xa5,
    0xad,0x04,0x23,0x9c,0x14,0x51,0x22,0xf0,0x29,0x79,0x71,0x7e,0xff,0x8c,0x0e,0xe2,
    0x0c,0xef,0xbc,0x72,0x75,0x6f,0x37,0xa1,0xec,0xd3,0x8e,0x62,0x8b,0x86,0x10,0xe8,
    0x08,0x77,0x11,0xbe,0x92,0x4f,0x24,0xc5,0x32,0x36,0x9d,0xcf,0xf3,0xa6,0xbb,0xac,
    0x5e,0x6c,0xa9,0x13,0x57,0x25,0xb5,0xe3,0xbd,0xa8,0x3a,0x01,0x05,0x59,0x2a,0x46,
};

#ifndef SKIPJACK_COUNTER0
#define SKIPJACK_COUNTER0 1   // the counter starts at 1 when encrypting
#endif

static int hexbytes(const char *s, u8 *out) {   // hex digits (spaces ignored) -> bytes; returns byte count
    int n = 0, half = 0, cur = 0;
    for (; *s; s++) {
        int d;
        if (*s >= '0' && *s <= '9') d = *s - '0'; else if (*s >= 'a' && *s <= 'f') d = *s - 'a' + 10; else if (*s >= 'A' && *s <= 'F') d = *s - 'A' + 10; else continue;
        cur = cur * 16 + d;
        if (half) { out[n++] = (u8)cur; cur = 0; }
        half ^= 1;
    }
    return n;
}
// G^k: four Feistel rounds on the two bytes of w; g_i = F(g_{i-1} ^ cv[(4k + i - 3) mod 10]) ^ g_{i-2}, output g5 || g6.
static u16 G(const u8 *cv, int k, u16 w) {
    u8 g1 = (u8)(w >> 8), g2 = (u8)w, g3, g4, g5, g6;
    g3 = F[g2 ^ cv[(4 * k) % 10]] ^ g1;
    g4 = F[g3 ^ cv[(4 * k + 1) % 10]] ^ g2;
    g5 = F[g4 ^ cv[(4 * k + 2) % 10]] ^ g3;
    g6 = F[g5 ^ cv[(4 * k + 3) % 10]] ^ g4;
    return (u16)(g5 << 8 | g6);
}
static u16 Ginv(const u8 *cv, int k, u16 w) {   // g_{i-2} = F(g_{i-1} ^ cv[...]) ^ g_i, from g5 || g6 back to g1 || g2
    u8 g5 = (u8)(w >> 8), g6 = (u8)w, g1, g2, g3, g4;
    g4 = F[g5 ^ cv[(4 * k + 3) % 10]] ^ g6;
    g3 = F[g4 ^ cv[(4 * k + 2) % 10]] ^ g5;
    g2 = F[g3 ^ cv[(4 * k + 1) % 10]] ^ g4;
    g1 = F[g2 ^ cv[(4 * k) % 10]] ^ g3;
    return (u16)(g1 << 8 | g2);
}
static int rule_a(int k) { return (k / 8) % 2 == 0; }   // steps 0-7 and 16-23 use rule A, 8-15 and 24-31 use rule B

static void sj_enc(const u8 *cv, const u8 *in, u8 *out) {
    u16 w1 = (u16)(in[0] << 8 | in[1]), w2 = (u16)(in[2] << 8 | in[3]), w3 = (u16)(in[4] << 8 | in[5]), w4 = (u16)(in[6] << 8 | in[7]);
    for (int k = 0; k < 32; k++) {
        u16 counter = (u16)(k + SKIPJACK_COUNTER0);
        u16 g = G(cv, k, w1);
        if (rule_a(k)) { u16 n1 = g ^ w4 ^ counter; w4 = w3; w3 = w2; w2 = g; w1 = n1; }
        else { u16 n3 = w1 ^ w2 ^ counter; w1 = w4; w4 = w3; w3 = n3; w2 = g; }
    }
    out[0] = (u8)(w1 >> 8); out[1] = (u8)w1; out[2] = (u8)(w2 >> 8); out[3] = (u8)w2;
    out[4] = (u8)(w3 >> 8); out[5] = (u8)w3; out[6] = (u8)(w4 >> 8); out[7] = (u8)w4;
}
static void sj_dec(const u8 *cv, const u8 *in, u8 *out) {
    u16 w1 = (u16)(in[0] << 8 | in[1]), w2 = (u16)(in[2] << 8 | in[3]), w3 = (u16)(in[4] << 8 | in[5]), w4 = (u16)(in[6] << 8 | in[7]);
    for (int k = 31; k >= 0; k--) {
        u16 counter = (u16)(k + SKIPJACK_COUNTER0);
        if (rule_a(k)) {   // undo rule A: w2 = G(w1_old), w3_new = w2_old, w4_new = w3_old, w1_new = G ^ w4_old ^ counter
            u16 o1 = Ginv(cv, k, w2), o2 = w3, o3 = w4, o4 = w1 ^ w2 ^ counter;
            w1 = o1; w2 = o2; w3 = o3; w4 = o4;
        } else {           // undo rule B: w1_new = w4_old, w2_new = G(w1_old), w3_new = w1_old ^ w2_old ^ counter, w4_new = w3_old
            u16 o1 = Ginv(cv, k, w2), o4 = w1, o3 = w4, o2 = w3 ^ o1 ^ counter;
            w1 = o1; w2 = o2; w3 = o3; w4 = o4;
        }
    }
    out[0] = (u8)(w1 >> 8); out[1] = (u8)w1; out[2] = (u8)(w2 >> 8); out[3] = (u8)w2;
    out[4] = (u8)(w3 >> 8); out[5] = (u8)w3; out[6] = (u8)(w4 >> 8); out[7] = (u8)w4;
}

int paper_main(void) {
    int good = 0, dec_good = 0, perm_ok = 1;
    // the F-table must be a permutation of 0..255 and reproduce the spec's example F(0x7a) = 0xd6
    u8 seen[256]; for (int i = 0; i < 256; i++) seen[i] = 0;
    for (int i = 0; i < 256; i++) seen[F[i]]++;
    for (int i = 0; i < 256; i++) if (seen[i] != 1) perm_ok = 0;
    if (F[0x7a] != 0xd6) perm_ok = 0;
    pstr(perm_ok ? "PASS " : "FAIL "); pstr("F-table is a permutation and F(7a) = d6\n");
    for (int v = 0; v < NVEC; v++) {
        const vec *e = &VEC[v];
        u8 key[10], pt[8], want[8], got[8], back[8];
        if (hexbytes(e->key, key) != 10 || hexbytes(e->pt, pt) != 8 || hexbytes(e->ct, want) != 8) { pstr("FAIL bad vector sizes\n"); return 1; }
        // SP 800-17 prints keys and blocks as numbers, most significant byte first; the specification's byte 0 (cv_0, the first
        // byte of w1) is the least significant byte, so the table bytes are reversed on the way in and out.
        u8 k2[10], p2[8], raw[8], rawback[8];
        for (int i = 0; i < 10; i++) k2[i] = key[9 - i];
        for (int i = 0; i < 8; i++) p2[i] = pt[7 - i];
        sj_enc(k2, p2, raw);
        for (int i = 0; i < 8; i++) got[i] = raw[7 - i];
        good += pcheck_bytes(e->name, got, want, 8);
        sj_dec(k2, raw, rawback);
        for (int i = 0; i < 8; i++) back[i] = rawback[7 - i];
        int same = 1; for (int i = 0; i < 8; i++) if (back[i] != pt[i]) same = 0;
        dec_good += same;
    }
    pstr("SP 800-17 Table 6 vectors: "); pdec(good); pstr(" of "); pdec(NVEC); pstr(" match\n");
    pstr("decrypt round trips: "); pdec(dec_good); pstr(" of "); pdec(NVEC); pstr("\n");
    int ok = perm_ok && good == NVEC && dec_good == NVEC;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
