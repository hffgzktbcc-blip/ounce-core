// SM4 (the Chinese national block cipher standard GB/T 32907-2016, first published as the SMS4 cipher) written from the
// specification text in draft-ribose-cfrg-sm4 (sections 6-7). The S-box, FK and CK constants (data) and the draft's Appendix A examples
// (GB/T 32907 examples with every round key and round word; two million-fold iterations; ECB/CBC/OFB/CFB/CTR examples) are copied as
// data by script. Exit 0 only if everything matches.
#include "ounce_paper.h"

// draft-ribose-cfrg-sm4 section 6.2.3: S-box; 7.3.1: FK; 7.3.2: CK
static const u8 SBOX[256] = {
    0xD6, 0x90, 0xE9, 0xFE, 0xCC, 0xE1, 0x3D, 0xB7, 0x16, 0xB6, 0x14, 0xC2, 0x28, 0xFB, 0x2C, 0x05,
    0x2B, 0x67, 0x9A, 0x76, 0x2A, 0xBE, 0x04, 0xC3, 0xAA, 0x44, 0x13, 0x26, 0x49, 0x86, 0x06, 0x99,
    0x9C, 0x42, 0x50, 0xF4, 0x91, 0xEF, 0x98, 0x7A, 0x33, 0x54, 0x0B, 0x43, 0xED, 0xCF, 0xAC, 0x62,
    0xE4, 0xB3, 0x1C, 0xA9, 0xC9, 0x08, 0xE8, 0x95, 0x80, 0xDF, 0x94, 0xFA, 0x75, 0x8F, 0x3F, 0xA6,
    0x47, 0x07, 0xA7, 0xFC, 0xF3, 0x73, 0x17, 0xBA, 0x83, 0x59, 0x3C, 0x19, 0xE6, 0x85, 0x4F, 0xA8,
    0x68, 0x6B, 0x81, 0xB2, 0x71, 0x64, 0xDA, 0x8B, 0xF8, 0xEB, 0x0F, 0x4B, 0x70, 0x56, 0x9D, 0x35,
    0x1E, 0x24, 0x0E, 0x5E, 0x63, 0x58, 0xD1, 0xA2, 0x25, 0x22, 0x7C, 0x3B, 0x01, 0x21, 0x78, 0x87,
    0xD4, 0x00, 0x46, 0x57, 0x9F, 0xD3, 0x27, 0x52, 0x4C, 0x36, 0x02, 0xE7, 0xA0, 0xC4, 0xC8, 0x9E,
    0xEA, 0xBF, 0x8A, 0xD2, 0x40, 0xC7, 0x38, 0xB5, 0xA3, 0xF7, 0xF2, 0xCE, 0xF9, 0x61, 0x15, 0xA1,
    0xE0, 0xAE, 0x5D, 0xA4, 0x9B, 0x34, 0x1A, 0x55, 0xAD, 0x93, 0x32, 0x30, 0xF5, 0x8C, 0xB1, 0xE3,
    0x1D, 0xF6, 0xE2, 0x2E, 0x82, 0x66, 0xCA, 0x60, 0xC0, 0x29, 0x23, 0xAB, 0x0D, 0x53, 0x4E, 0x6F,
    0xD5, 0xDB, 0x37, 0x45, 0xDE, 0xFD, 0x8E, 0x2F, 0x03, 0xFF, 0x6A, 0x72, 0x6D, 0x6C, 0x5B, 0x51,
    0x8D, 0x1B, 0xAF, 0x92, 0xBB, 0xDD, 0xBC, 0x7F, 0x11, 0xD9, 0x5C, 0x41, 0x1F, 0x10, 0x5A, 0xD8,
    0x0A, 0xC1, 0x31, 0x88, 0xA5, 0xCD, 0x7B, 0xBD, 0x2D, 0x74, 0xD0, 0x12, 0xB8, 0xE5, 0xB4, 0xB0,
    0x89, 0x69, 0x97, 0x4A, 0x0C, 0x96, 0x77, 0x7E, 0x65, 0xB9, 0xF1, 0x09, 0xC5, 0x6E, 0xC6, 0x84,
    0x18, 0xF0, 0x7D, 0xEC, 0x3A, 0xDC, 0x4D, 0x20, 0x79, 0xEE, 0x5F, 0x3E, 0xD7, 0xCB, 0x39, 0x48,
};
static const u32 FKT[4] = {0xA3B1BAC6, 0x56AA3350, 0x677D9197, 0xB27022DC};
static const u32 CKT[32] = {
    0x00070E15, 0x1C232A31, 0x383F464D, 0x545B6269,
    0x70777E85, 0x8C939AA1, 0xA8AFB6BD, 0xC4CBD2D9,
    0xE0E7EEF5, 0xFC030A11, 0x181F262D, 0x343B4249,
    0x50575E65, 0x6C737A81, 0x888F969D, 0xA4ABB2B9,
    0xC0C7CED5, 0xDCE3EAF1, 0xF8FF060D, 0x141B2229,
    0x30373E45, 0x4C535A61, 0x686F767D, 0x848B9299,
    0xA0A7AEB5, 0xBCC3CAD1, 0xD8DFE6ED, 0xF4FB0209,
    0x10171E25, 0x2C333A41, 0x484F565D, 0x646B7279,
};
// Appendix A.1 examples 1, 2, 4, 5: round keys rk_0..rk_31 and the state words X_4..X_35 (in decryption listings the word shown
// next to rk_j, i.e. X_(j+4)). {key, input block, output block, rk[32], X[32]}
struct trace { const char *key, *in, *out; u32 rk[32], x[32]; };
static const struct trace TR[4] = {  // 1: encrypt, 2: decrypt, 4: encrypt, 5: decrypt
{ "0123456789ABCDEFFEDCBA9876543210", "0123456789ABCDEFFEDCBA9876543210", "681EDF34D206965E86B3E94F536E4246",
  {0xF12186F9,0x41662B61,0x5A6AB19A,0x7BA92077,0x367360F4,0x776A0C61,0xB6BB89B3,0x24763151,0xA520307C,0xB7584DBD,0xC30753ED,0x7EE55B57,0x6988608C,0x30D895B7,0x44BA14AF,0x104495A1,0xD120B428,0x73B55FA3,0xCC874966,0x92244439,0xE89E641F,0x98CA015A,0xC7159060,0x99E1FD2E,0xB79BD80C,0x1D2115B0,0x0E228AEB,0xF1780C81,0x428D3654,0x62293496,0x01CF72E5,0x9124A012},
  {0x27FAD345,0xA18B4CB2,0x11C1E22A,0xCC13E2EE,0xF87C5BD5,0x33220757,0x77F4C297,0x7A96F2EB,0x27DAC07F,0x42DD0F19,0xB8A5DA02,0x907127FA,0x8B952B83,0xD42B7C59,0x2FFC5831,0xF69E6888,0xAF2432C4,0xED1EC85E,0x55A3BA22,0x124B18AA,0x6AE7725F,0xF4CBA1F9,0x1DCDFA10,0x2FF60603,0xEFF24FDC,0x6FE46B75,0x893450AD,0x7B938F4C,0x536E4246,0x86B3E94F,0xD206965E,0x681EDF34} },
{ "0123456789ABCDEFFEDCBA9876543210", "681EDF34D206965E86B3E94F536E4246", "0123456789ABCDEFFEDCBA9876543210",
  {0xF12186F9,0x41662B61,0x5A6AB19A,0x7BA92077,0x367360F4,0x776A0C61,0xB6BB89B3,0x24763151,0xA520307C,0xB7584DBD,0xC30753ED,0x7EE55B57,0x6988608C,0x30D895B7,0x44BA14AF,0x104495A1,0xD120B428,0x73B55FA3,0xCC874966,0x92244439,0xE89E641F,0x98CA015A,0xC7159060,0x99E1FD2E,0xB79BD80C,0x1D2115B0,0x0E228AEB,0xF1780C81,0x428D3654,0x62293496,0x01CF72E5,0x9124A012},
  {0x01234567,0x89ABCDEF,0xFEDCBA98,0x76543210,0x27FAD345,0xA18B4CB2,0x11C1E22A,0xCC13E2EE,0xF87C5BD5,0x33220757,0x77F4C297,0x7A96F2EB,0x27DAC07F,0x42DD0F19,0xB8A5DA02,0x907127FA,0x8B952B83,0xD42B7C59,0x2FFC5831,0xF69E6888,0xAF2432C4,0xED1EC85E,0x55A3BA22,0x124B18AA,0x6AE7725F,0xF4CBA1F9,0x1DCDFA10,0x2FF60603,0xEFF24FDC,0x6FE46B75,0x893450AD,0x7B938F4C} },
{ "FEDCBA98765432100123456789ABCDEF", "000102030405060708090A0B0C0D0E0F", "F766678F13F01ADEAC1B3EA955ADB594",
  {0x0D8CC1B4,0xAC44F213,0x188C0C40,0x7537585E,0x627646F5,0x54D785AD,0x51B96DEE,0x0C385958,0x5E494992,0x32F3FE04,0x3A3A733D,0x0EDFB91D,0x6823CD6B,0x40F7D825,0x4BD68EE5,0x165A36C8,0x56608984,0x23F35FF4,0x8B592B3E,0x80F7388A,0x0415C409,0xAFDF1370,0xCF444772,0x9AF9901F,0xC457578C,0x95701C60,0x2B0F4EE1,0x7F826139,0xFA37F8D9,0xD18AF8CE,0x5BD5D8C6,0x711138B7},
  {0xF7EAEB6A,0xB4967C0F,0x5B9B2419,0xF46BECBA,0xA8013E25,0xB38E2ABE,0x3E7C99A1,0x6DD5F47F,0xB286430C,0xAB997DE3,0x80F8F21F,0x4EF7052E,0x4462FFAF,0x14DFD5EA,0x6D33EFED,0x3A4F8B3C,0x1A435088,0x4E64B153,0x0415CEDA,0xADD88955,0x73964EF1,0xB0085092,0x554A1293,0x4BC6D6A8,0x7BB650E1,0xDDFB8A61,0x5C4DFD78,0xFD9066FD,0x55ADB594,0xAC1B3EA9,0x13F01ADE,0xF766678F} },
{ "FEDCBA98765432100123456789ABCDEF", "F766678F13F01ADEAC1B3EA955ADB594", "000102030405060708090A0B0C0D0E0F",
  {0x0D8CC1B4,0xAC44F213,0x188C0C40,0x7537585E,0x627646F5,0x54D785AD,0x51B96DEE,0x0C385958,0x5E494992,0x32F3FE04,0x3A3A733D,0x0EDFB91D,0x6823CD6B,0x40F7D825,0x4BD68EE5,0x165A36C8,0x56608984,0x23F35FF4,0x8B592B3E,0x80F7388A,0x0415C409,0xAFDF1370,0xCF444772,0x9AF9901F,0xC457578C,0x95701C60,0x2B0F4EE1,0x7F826139,0xFA37F8D9,0xD18AF8CE,0x5BD5D8C6,0x711138B7},
  {0x00010203,0x04050607,0x08090A0B,0x0C0D0E0F,0xF7EAEB6A,0xB4967C0F,0x5B9B2419,0xF46BECBA,0xA8013E25,0xB38E2ABE,0x3E7C99A1,0x6DD5F47F,0xB286430C,0xAB997DE3,0x80F8F21F,0x4EF7052E,0x4462FFAF,0x14DFD5EA,0x6D33EFED,0x3A4F8B3C,0x1A435088,0x4E64B153,0x0415CEDA,0xADD88955,0x73964EF1,0xB0085092,0x554A1293,0x4BC6D6A8,0x7BB650E1,0xDDFB8A61,0x5C4DFD78,0xFD9066FD} },
};
// A.1.3 and A.1.6: one million repeated encryptions. {key, plaintext, ciphertext}
static const char *MILLION[2][3] = {
    {"0123456789ABCDEFFEDCBA9876543210", "0123456789ABCDEFFEDCBA9876543210", "595298C7C6FD271F0402F804C33D3F66"},
    {"FEDCBA98765432100123456789ABCDEF", "000102030405060708090A0B0C0D0E0F", "379A96D0A6A5A5060FB460C75D1879ED"},
};
// Appendix A.2 modes of operation examples. {mode, key, iv, plaintext, ciphertext}
static const char *MODES[10][5] = {
    {"ECB", "0123456789ABCDEFFEDCBA9876543210", "", "AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDDEEEEEEEEFFFFFFFFAAAAAAAABBBBBBBB", "5EC8143DE509CFF7B5179F8F474B86192F1D305A7FB17DF985F81C8482192304"},
    {"ECB", "FEDCBA98765432100123456789ABCDEF", "", "AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDDEEEEEEEEFFFFFFFFAAAAAAAABBBBBBBB", "C5876897E4A59BBBA72A10C83872245B12DD90BC2D200692B529A4155AC9E600"},
    {"CBC", "0123456789ABCDEFFEDCBA9876543210", "000102030405060708090A0B0C0D0E0F", "AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDDEEEEEEEEFFFFFFFFAAAAAAAABBBBBBBB", "78EBB11CC40B0A48312AAEB2040244CB4CB7016951909226979B0D15DC6A8F6D"},
    {"CBC", "FEDCBA98765432100123456789ABCDEF", "000102030405060708090A0B0C0D0E0F", "AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDDEEEEEEEEFFFFFFFFAAAAAAAABBBBBBBB", "0D3A6DDC2D21C698857215587B7BB59A91F2C147911A4144665E1FA1D40BAE38"},
    {"OFB", "0123456789ABCDEFFEDCBA9876543210", "000102030405060708090A0B0C0D0E0F", "AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDDEEEEEEEEFFFFFFFFAAAAAAAABBBBBBBB", "AC3236CB861DD316E6413B4E3C7524B71D01ACA2487CA582CBF5463E6698539B"},
    {"OFB", "FEDCBA98765432100123456789ABCDEF", "000102030405060708090A0B0C0D0E0F", "AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDDEEEEEEEEFFFFFFFFAAAAAAAABBBBBBBB", "5DCCCD25A84BA16560D7F2658870684933FA16BD5CD9C856CACAA1E101897A97"},
    {"CFB", "0123456789ABCDEFFEDCBA9876543210", "000102030405060708090A0B0C0D0E0F", "AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDDEEEEEEEEFFFFFFFFAAAAAAAABBBBBBBB", "AC3236CB861DD316E6413B4E3C7524B769D4C54ED433B9A0346009BEB37B2B3F"},
    {"CFB", "FEDCBA98765432100123456789ABCDEF", "000102030405060708090A0B0C0D0E0F", "AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDDEEEEEEEEFFFFFFFFAAAAAAAABBBBBBBB", "5DCCCD25A84BA16560D7F265887068490D9B86FF20C3BFE115FFA02CA6192CC5"},
    {"CTR", "0123456789ABCDEFFEDCBA9876543210", "000102030405060708090A0B0C0D0E0F", "AAAAAAAAAAAAAAAABBBBBBBBBBBBBBBBCCCCCCCCCCCCCCCCDDDDDDDDDDDDDDDDEEEEEEEEEEEEEEEEFFFFFFFFFFFFFFFFAAAAAAAAAAAAAAAABBBBBBBBBBBBBBBB", "AC3236CB970CC20791364C395A1342D1A3CBC1878C6F30CD074CCE385CDD70C7F234BC0E24C11980FD1286310CE37B926E02FCD0FAA0BAF38B2933851D824514"},
    {"CTR", "FEDCBA98765432100123456789ABCDEF", "000102030405060708090A0B0C0D0E0F", "AAAAAAAAAAAAAAAABBBBBBBBBBBBBBBBCCCCCCCCCCCCCCCCDDDDDDDDDDDDDDDDEEEEEEEEEEEEEEEEFFFFFFFFFFFFFFFFAAAAAAAAAAAAAAAABBBBBBBBBBBBBBBB", "5DCCCD25B95AB07417A08512EE160E2F8F661521CBBAB44CC87138445BC29E5C0AE0297205D62704173B21239B887F6C8CB5B800917A2488284BDE9E16EA2906"},
};

static int hexbytes(const char *s, u8 *out) {
    int n = 0;
    for (; s[0] && s[1]; s += 2) {
        int v = 0;
        for (int i = 0; i < 2; i++) { char c = s[i]; v = v * 16 + (c <= '9' ? c - '0' : c >= 'a' ? c - 'a' + 10 : c - 'A' + 10); }
        out[n++] = (u8)v;
    }
    return n;
}

// ---- SM4 (draft-ribose-cfrg-sm4 sections 6-7, the specification of GB/T 32907-2016) ----
#ifndef SM4_L2
#define SM4_L2 2               // first rotation of the linear transformation L
#endif
static u32 rol(u32 x, int n) { return (x << n) | (x >> (32 - n)); }
static u32 Lf(u32 b) { return b ^ rol(b, SM4_L2) ^ rol(b, 10) ^ rol(b, 18) ^ rol(b, 24); }
static u32 Lk(u32 b) { return b ^ rol(b, 13) ^ rol(b, 23); }
static u32 tau(u32 a) { return ((u32)SBOX[a >> 24] << 24) | ((u32)SBOX[(a >> 16) & 255] << 16) | ((u32)SBOX[(a >> 8) & 255] << 8) | SBOX[a & 255]; }
static u32 TT[4][256];                                  // TT[i][x] = L(S[x] placed in byte i), so T(a) = xor of four lookups
static void make_tables(void) { for (int i = 0; i < 4; i++) for (int x = 0; x < 256; x++) TT[i][x] = Lf((u32)SBOX[x] << (24 - 8 * i)); }
static u32 T(u32 a) { return TT[0][a >> 24] ^ TT[1][(a >> 16) & 255] ^ TT[2][(a >> 8) & 255] ^ TT[3][a & 255]; }
static u32 be32(const u8 *p) { return ((u32)p[0] << 24) | ((u32)p[1] << 16) | ((u32)p[2] << 8) | p[3]; }
static void put32(u8 *p, u32 v) { p[0] = (u8)(v >> 24); p[1] = (u8)(v >> 16); p[2] = (u8)(v >> 8); p[3] = (u8)v; }
static u32 RK[32];
static void sm4_key(const u8 *key) {
    u32 K[36];
    for (int i = 0; i < 4; i++) K[i] = be32(key + 4 * i) ^ FKT[i];
    for (int i = 0; i < 32; i++) { K[i + 4] = K[i] ^ Lk(tau(K[i + 1] ^ K[i + 2] ^ K[i + 3] ^ CKT[i])); RK[i] = K[i + 4]; }
}
// One run of the 32 rounds. rk_order[i] = index of the round key used in round i. x_out (optional) receives X_4..X_35 in the order produced.
static void sm4_rounds(const u8 *in, u8 *out, int decrypt, u32 *x_out) {
    u32 X[36];
    for (int i = 0; i < 4; i++) X[i] = be32(in + 4 * i);
    for (int i = 0; i < 32; i++) { X[i + 4] = X[i] ^ T(X[i + 1] ^ X[i + 2] ^ X[i + 3] ^ RK[decrypt ? 31 - i : i]); if (x_out) x_out[i] = X[i + 4]; }
    for (int i = 0; i < 4; i++) put32(out + 4 * i, X[35 - i]);   // reverse transformation R
}
static void sm4_enc(const u8 *in, u8 *out) { sm4_rounds(in, out, 0, 0); }
static void sm4_dec(const u8 *in, u8 *out) { sm4_rounds(in, out, 1, 0); }

static int total, bad;
static void cmp(u32 got, u32 want, const char *what, int a, int b) {
    total++;
    if (got != want) { bad++; if (bad <= 6) { pstr("FAIL "); pstr(what); pstr(" "); pdec(a); pstr("/"); pdec(b); pstr(": "); phex(got, 8); pstr(" expected "); phex(want, 8); pstr("\n"); } }
}
static int eqn(const u8 *a, const u8 *b, int n) { for (int i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }
static int streq3(const char *a, const char *b) { return a[0] == b[0] && a[1] == b[1] && a[2] == b[2]; }

int paper_main(void) {
    make_tables();
    // sanity: the draft's example S(EF) = 84, and CK_i bytes = (4i + j) * 7 mod 256
    cmp(SBOX[0xEF], 0x84, "S(EF)", 0, 0);
    for (int i = 0; i < 32; i++) { u32 w = 0; for (int j = 0; j < 4; j++) w = (w << 8) | (u32)(((4 * i + j) * 7) & 255); cmp(w, CKT[i], "CK formula", i, 0); }
    // Appendix A.1 examples 1, 2, 4, 5 with every round key and round word
    for (int e = 0; e < 4; e++) {
        const struct trace *E = &TR[e];
        int decrypt = (e == 1 || e == 3), t0 = total, b0 = bad;
        u8 key[16], in[16], out[16], want[16]; u32 x[32];
        hexbytes(E->key, key); hexbytes(E->in, in); hexbytes(E->out, want);
        sm4_key(key); sm4_rounds(in, out, decrypt, x);
        for (int j = 0; j < 32; j++) cmp(RK[j], E->rk[j], "round key", e + 1, j);
        // encryption lists X_4..X_35 in order; the decryption listings pair rk_j with the word X_(j+4), j = 31 first
        for (int i = 0; i < 32; i++) cmp(x[i], E->x[decrypt ? 31 - i : i], "round word", e + 1, i);
        total++; if (!eqn(out, want, 16)) { bad++; pcheck_bytes("output block", out, want, 16); }
        pstr("GB/T 32907 example "); pdec(e + 1 == 1 ? 1 : e + 1 == 2 ? 2 : e + 1 == 3 ? 4 : 5); pstr(decrypt ? " (decrypt): " : " (encrypt): "); phex_bytes(out, 16);
        pstr(", "); pdec(total - t0); pstr(" published values compared, "); pdec(bad - b0); pstr(" mismatches\n");
    }
    // modes of operation (Appendix A.2)
    int mok = 0;
    for (int m = 0; m < 10; m++) {
        u8 key[16], iv[16], pt[64], ct[64], got[64]; int n;
        hexbytes(MODES[m][1], key); if (MODES[m][2][0]) hexbytes(MODES[m][2], iv); n = hexbytes(MODES[m][3], pt); hexbytes(MODES[m][4], ct);
        const char *mode = MODES[m][0];
        sm4_key(key);
        u8 reg[16], ks[16];
        for (int i = 0; i < 16; i++) reg[i] = iv[i];
        if (streq3(mode, "ECB")) { for (int b = 0; b < n; b += 16) sm4_enc(pt + b, got + b); }
        else if (streq3(mode, "CBC")) { for (int b = 0; b < n; b += 16) { u8 x[16]; for (int i = 0; i < 16; i++) x[i] = pt[b + i] ^ reg[i]; sm4_enc(x, reg); for (int i = 0; i < 16; i++) got[b + i] = reg[i]; } }
        else if (streq3(mode, "OFB")) { for (int b = 0; b < n; b += 16) { sm4_enc(reg, ks); for (int i = 0; i < 16; i++) { reg[i] = ks[i]; got[b + i] = pt[b + i] ^ ks[i]; } } }
        else if (streq3(mode, "CFB")) { for (int b = 0; b < n; b += 16) { sm4_enc(reg, ks); for (int i = 0; i < 16; i++) { got[b + i] = pt[b + i] ^ ks[i]; reg[i] = got[b + i]; } } }
        else { for (int b = 0; b < n; b += 16) { sm4_enc(reg, ks); for (int i = 0; i < 16; i++) got[b + i] = pt[b + i] ^ ks[i]; for (int i = 15; i >= 0; i--) if (++reg[i]) break; } }   // CTR: 128-bit big-endian counter
        total++;
        if (eqn(got, ct, n)) mok++; else { bad++; pstr("FAIL mode example "); pstr(mode); pstr("\n"); }
    }
    pstr("Appendix A.2 modes of operation examples (ECB, CBC, OFB, CFB, CTR, two each): "); pdec(mok); pstr(" of 10 match\n");
    // examples 3 and 6: one million repeated encryptions of one block
    for (int e = 0; e < 2; e++) {
        u8 key[16], blk[16], want[16], o[16];
        hexbytes(MILLION[e][0], key); hexbytes(MILLION[e][1], blk); hexbytes(MILLION[e][2], want);
        sm4_key(key);
        {   // the same rounds as sm4_rounds, with the four state words kept in locals and the loop unrolled by four (this loop runs 2,000,000 times)
            u32 a0 = be32(blk), a1 = be32(blk + 4), a2 = be32(blk + 8), a3 = be32(blk + 12);
            for (long i = 0; i < 1000000; i++) {
                u32 x0 = a0, x1 = a1, x2 = a2, x3 = a3;
                for (int r = 0; r < 32; r += 4) {
                    x0 ^= T(x1 ^ x2 ^ x3 ^ RK[r]);
                    x1 ^= T(x2 ^ x3 ^ x0 ^ RK[r + 1]);
                    x2 ^= T(x3 ^ x0 ^ x1 ^ RK[r + 2]);
                    x3 ^= T(x0 ^ x1 ^ x2 ^ RK[r + 3]);
                }
                a0 = x3; a1 = x2; a2 = x1; a3 = x0;      // reverse transformation R: (X35, X34, X33, X32)
            }
            put32(blk, a0); put32(blk + 4, a1); put32(blk + 8, a2); put32(blk + 12, a3);
        }
        (void)o;
        total++;
        int ok = pcheck_bytes(e == 0 ? "GB/T 32907 example 3 (1,000,000 encryptions)" : "example 6 (1,000,000 encryptions)", blk, want, 16);
        if (!ok) bad++;
    }
    pstr("published values compared: "); pdec(total); pstr(", mismatches "); pdec(bad); pstr("\n");
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
