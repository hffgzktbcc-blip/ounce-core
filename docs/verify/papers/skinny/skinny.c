// SKINNY-128 from Beierle, Jean, Koelbl, Leander, Moradi, Peyrin, Sasaki, Sim, Todo, et al.: "The SKINNY Family of Block Ciphers
// and its Low-Latency Variant MANTIS" (CRYPTO 2016). This program implements SKINNY-128-256 and SKINNY-128-384 as
// specified in section 2.3 of the SKINNY-AEAD and SKINNY-Hash NIST submission document (by the SKINNY designers), written from
// that description: the tweakey state with its cell permutation PT and LFSRs, the five-step round (SubCells, AddConstants,
// AddRoundTweakey, ShiftRows, MixColumns), 48 / 56 rounds. Checks that document's two Appendix B test vectors, the 8-bit S-box
// of its Appendix A (all 256 entries, against the S-box rebuilt from its bit-level description), and the 56 - 16 = 40-round
// variant SKINNY-128-384+ with the test vector printed in the Romulus submission document (same designers' family).
// Tables and vectors are data copied as numbers; no code was copied.
#include "ounce_paper.h"

// S8 (Appendix A of the SKINNY-AEAD document).
static const u8 S8[256] = {
    0x65, 0x4c, 0x6a, 0x42, 0x4b, 0x63, 0x43, 0x6b, 0x55, 0x75, 0x5a, 0x7a, 0x53, 0x73, 0x5b, 0x7b,
    0x35, 0x8c, 0x3a, 0x81, 0x89, 0x33, 0x80, 0x3b, 0x95, 0x25, 0x98, 0x2a, 0x90, 0x23, 0x99, 0x2b,
    0xe5, 0xcc, 0xe8, 0xc1, 0xc9, 0xe0, 0xc0, 0xe9, 0xd5, 0xf5, 0xd8, 0xf8, 0xd0, 0xf0, 0xd9, 0xf9,
    0xa5, 0x1c, 0xa8, 0x12, 0x1b, 0xa0, 0x13, 0xa9, 0x05, 0xb5, 0x0a, 0xb8, 0x03, 0xb0, 0x0b, 0xb9,
    0x32, 0x88, 0x3c, 0x85, 0x8d, 0x34, 0x84, 0x3d, 0x91, 0x22, 0x9c, 0x2c, 0x94, 0x24, 0x9d, 0x2d,
    0x62, 0x4a, 0x6c, 0x45, 0x4d, 0x64, 0x44, 0x6d, 0x52, 0x72, 0x5c, 0x7c, 0x54, 0x74, 0x5d, 0x7d,
    0xa1, 0x1a, 0xac, 0x15, 0x1d, 0xa4, 0x14, 0xad, 0x02, 0xb1, 0x0c, 0xbc, 0x04, 0xb4, 0x0d, 0xbd,
    0xe1, 0xc8, 0xec, 0xc5, 0xcd, 0xe4, 0xc4, 0xed, 0xd1, 0xf1, 0xdc, 0xfc, 0xd4, 0xf4, 0xdd, 0xfd,
    0x36, 0x8e, 0x38, 0x82, 0x8b, 0x30, 0x83, 0x39, 0x96, 0x26, 0x9a, 0x28, 0x93, 0x20, 0x9b, 0x29,
    0x66, 0x4e, 0x68, 0x41, 0x49, 0x60, 0x40, 0x69, 0x56, 0x76, 0x58, 0x78, 0x50, 0x70, 0x59, 0x79,
    0xa6, 0x1e, 0xaa, 0x11, 0x19, 0xa3, 0x10, 0xab, 0x06, 0xb6, 0x08, 0xba, 0x00, 0xb3, 0x09, 0xbb,
    0xe6, 0xce, 0xea, 0xc2, 0xcb, 0xe3, 0xc3, 0xeb, 0xd6, 0xf6, 0xda, 0xfa, 0xd3, 0xf3, 0xdb, 0xfb,
    0x31, 0x8a, 0x3e, 0x86, 0x8f, 0x37, 0x87, 0x3f, 0x92, 0x21, 0x9e, 0x2e, 0x97, 0x27, 0x9f, 0x2f,
    0x61, 0x48, 0x6e, 0x46, 0x4f, 0x67, 0x47, 0x6f, 0x51, 0x71, 0x5e, 0x7e, 0x57, 0x77, 0x5f, 0x7f,
    0xa2, 0x18, 0xae, 0x16, 0x1f, 0xa7, 0x17, 0xaf, 0x01, 0xb2, 0x0e, 0xbe, 0x07, 0xb7, 0x0f, 0xbf,
    0xe2, 0xca, 0xee, 0xc6, 0xcf, 0xe7, 0xc7, 0xef, 0xd2, 0xf2, 0xde, 0xfe, 0xd7, 0xf7, 0xdf, 0xff,
};
typedef struct { const char *name; int ntk, rounds; const char *tk, *pt, *ct; } vec;
// name, tweakey blocks, rounds, tweakey, plaintext, ciphertext (byte strings, 16 bytes per tweakey block)
static const vec VEC[] = {
    {"SKINNY-128-256 (SKINNY-AEAD spec, App. B)", 2, 48, "009cec81605d4ac1d2ae9e3085d7a1f31ac123ebfc00fddcf01046ceeddfcab3", "3a0c47767a26a68dd382a695e7022e25", "b731d98a4bde147a7ed4a6f16b9b587f"},
    {"SKINNY-128-384 (SKINNY-AEAD spec, App. B)", 3, 56, "df889548cfc7ea52d296339301797449ab588a34a47f1ab2dfe9c8293fbea9a5ab1afac2611012cd8cef952618c3ebe8", "a3994b66ad85a3459f44e92b08f550cb", "94ecf589e2017c601b38c6346a10dcfa"},
    {"SKINNY-128-384+, 40 rounds (Romulus spec)", 3, 40, "df889548cfc7ea52d296339301797449ab588a34a47f1ab2dfe9c8293fbea9a5ab1afac2611012cd8cef952618c3ebe8", "a3994b66ad85a3459f44e92b08f550cb", "ff38d1d24c864c4352a853690fe36e5e"},
};
#define NVEC ((int)(sizeof VEC / sizeof VEC[0]))


#ifndef SKINNY_LFSR_TK2_TAP
#define SKINNY_LFSR_TK2_TAP 5   // TK2 LFSR: (x7..x0) -> (x6..x0 || x7 ^ x5)
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
static const u8 PT[16] = { 9, 15, 8, 13, 10, 14, 12, 11, 0, 1, 2, 3, 4, 5, 6, 7 };
static const u8 SR[16] = { 0, 1, 2, 3, 7, 4, 5, 6, 10, 11, 8, 9, 13, 14, 15, 12 };

// S8 from its bit-level description: four iterations of (x4 ^= NOR(x7, x6), x0 ^= NOR(x3, x2)) followed by the bit permutation
// (x7..x0) -> (x2 x1 x7 x6 x4 x0 x3 x5), except that the last iteration only swaps x1 and x2.
static u8 sbox_built(u8 v) {
    int x[8]; for (int i = 0; i < 8; i++) x[i] = (v >> i) & 1;
    for (int it = 0; it < 4; it++) {
        x[4] ^= 1 ^ (x[7] | x[6]); x[0] ^= 1 ^ (x[3] | x[2]);   // NOR/XOR, as the document says in its design section
        int n[8];
        if (it < 3) { n[7] = x[2]; n[6] = x[1]; n[5] = x[7]; n[4] = x[6]; n[3] = x[4]; n[2] = x[0]; n[1] = x[3]; n[0] = x[5]; }
        else { for (int i = 0; i < 8; i++) n[i] = x[i]; n[1] = x[2]; n[2] = x[1]; }
        for (int i = 0; i < 8; i++) x[i] = n[i];
    }
    u8 r = 0; for (int i = 0; i < 8; i++) r |= (u8)(x[i] << i); return r;
}
static u8 lfsr2(u8 x) { return (u8)((x << 1) | (((x >> 7) ^ (x >> SKINNY_LFSR_TK2_TAP)) & 1)); }
static u8 lfsr3(u8 x) { return (u8)((x >> 1) | ((((x >> 0) ^ (x >> 6)) & 1) << 7)); }

// tk: 16 * ntk bytes (TK1 first); returns the ciphertext in out.
static void skinny128(const u8 *tk, int ntk, int rounds, const u8 *in, u8 *out) {
    u8 IS[16], TK[3][16];
    for (int i = 0; i < 16; i++) IS[i] = in[i];
    for (int z = 0; z < ntk; z++) for (int i = 0; i < 16; i++) TK[z][i] = tk[16 * z + i];
    u8 rc = 0;
    for (int r = 0; r < rounds; r++) {
        for (int i = 0; i < 16; i++) IS[i] = S8[IS[i]];                       // SubCells
        rc = (u8)(((rc << 1) & 0x3f) | (((rc >> 5) ^ (rc >> 4) ^ 1) & 1));    // AddConstants
        IS[0] ^= rc & 15; IS[4] ^= (rc >> 4) & 3; IS[8] ^= 2;
        for (int i = 0; i < 8; i++) { u8 k = TK[0][i]; for (int z = 1; z < ntk; z++) k ^= TK[z][i]; IS[i] ^= k; }   // AddRoundTweakey
        for (int z = 0; z < ntk; z++) {                                       // tweakey update: permute, then LFSR on rows 0 and 1
            u8 t[16]; for (int i = 0; i < 16; i++) t[i] = TK[z][PT[i]];
            for (int i = 0; i < 16; i++) TK[z][i] = t[i];
            if (z == 1) for (int i = 0; i < 8; i++) TK[z][i] = lfsr2(TK[z][i]);
            if (z == 2) for (int i = 0; i < 8; i++) TK[z][i] = lfsr3(TK[z][i]);
        }
        u8 t[16]; for (int i = 0; i < 16; i++) t[i] = IS[SR[i]];              // ShiftRows
        for (int c = 0; c < 4; c++) {                                         // MixColumns with M = [1 0 1 1; 1 0 0 0; 0 1 1 0; 1 0 1 0]
            u8 a = t[c], b = t[4 + c], d = t[8 + c], e = t[12 + c];
            IS[c] = a ^ d ^ e; IS[4 + c] = a; IS[8 + c] = b ^ d; IS[12 + c] = a ^ d;
        }
    }
    for (int i = 0; i < 16; i++) out[i] = IS[i];
}

int paper_main(void) {
    int ok = 1;
    int sg = 0; for (int i = 0; i < 256; i++) sg += sbox_built((u8)i) == S8[i];
    pstr(sg == 256 ? "PASS " : "FAIL "); pdec(sg); pstr(" of 256 S-box entries match the Appendix A table\n");
    ok &= sg == 256;
    int good = 0;
    for (int v = 0; v < NVEC; v++) {
        const vec *e = &VEC[v];
        u8 tk[48], pt[16], want[16], got[16];
        if (hexbytes(e->tk, tk) != 16 * e->ntk || hexbytes(e->pt, pt) != 16 || hexbytes(e->ct, want) != 16) { pstr("FAIL bad vector sizes\n"); return 1; }
        skinny128(tk, e->ntk, e->rounds, pt, got);
        good += pcheck_bytes(e->name, got, want, 16);
    }
    pstr("test vectors: "); pdec(good); pstr(" of "); pdec(NVEC); pstr(" match\n");
    ok &= good == NVEC;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
