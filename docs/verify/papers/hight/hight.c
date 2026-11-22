// HIGHT from Hong, Sung, Hong, Lim, Lee, Koo, Lee, Chang, Lee, Jeong, Kim, Kim, Chee, "HIGHT: A New Block Cipher Suitable for
// Low-Resource Device" (CHES 2006), written from the paper's description of the 64-bit block, 128-bit key, 32-round generalized Feistel
// structure (whitening keys, LFSR constants, F0/F1 diffusion). Checks the 9 distinct single-block known-answer vectors in the
// Crypto++ test-vector file that it attributes to the KISA reference code (KISA_HIGHT_ECB.c), and the CBC (8) and
// CTR (44) vectors the same file attributes to the KISA files KISA_HIGHT_CBC.c / KISA_HIGHT_CTR.c. Byte order: key[i] = MK_i,
// plaintext[i] = P_i, ciphertext[i] = C_i (the array order of those files). Vectors are data copied as numbers; no code was copied.
#include "ounce_paper.h"

typedef struct { const char *name; int mode; const char *key, *iv, *pt, *ct; } vec;
// name, mode (0 ECB, 1 CBC, 2 CTR), key, IV (empty for ECB), plaintext, ciphertext, all as byte strings
static const vec VEC[] = {
    {"HIGHT ECB #1", 0, "88e34f8f081779f1e9f394370ad40589", "", "d76d0d18327ec562", "e4bc2e312277e4dd"},
    {"HIGHT ECB #2", 0, "2923be84e16cd6ae529049f1f1bbe9eb", "", "b3a6db3c870c3e99", "23cad1a3cddf7eab"},
    {"HIGHT ECB #3", 0, "245e0d1c06b747deb3124dc843bb8ba6", "", "1f035a7d0938251f", "52bd91bb26f8ed99"},
    {"HIGHT ECB #4", 0, "5dd4cbfc96f5453b130d890a1cdbae32", "", "209a50ee407836fd", "9502b46d87b44167"},
    {"HIGHT ECB #5", 0, "124932f69e7d49dcad4f14f2444066d0", "", "6bc430b7323ba122", "20cbcf416524e5a6"},
    {"HIGHT ECB #6", 0, "f622919de18b1fdab0ca9902b9729d49", "", "2c807ec599d5e980", "f31c89e66c4f5a6f"},
    {"HIGHT ECB #7", 0, "b2eac9cc53bf67d6bf14d67e2ddc8e66", "", "83ef574961ff698f", "3921c83bfe5e596e"},
    {"HIGHT ECB #8", 0, "61cdd11e9d9c167272e61df0844f4a77", "", "02d7e8392c53cbc9", "982fd756174497bf"},
    {"HIGHT ECB #9", 0, "121e33749e0cf4d5d49fd4a4597e35cf", "", "3222f4cccfd3902d", "bb80f50b35115ba8"},
    {"HIGHT CBC #1", 1, "88e34f8f081779f1e9f394370ad40589", "268d66a735a81a81", "0001020304050607", "ce1595085a188c28"},
    {"HIGHT CBC #2", 1, "88e34f8f081779f1e9f394370ad40589", "268d66a735a81a81", "000102030405060708090a0b0c0d0e0f", "ce1595085a188c28c18d7708d9c12586"},
    {"HIGHT CBC #3", 1, "88e34f8f081779f1e9f394370ad40589", "268d66a735a81a81", "000102030405060708090a0b0c0d0e0f0001020304050607", "ce1595085a188c28c18d7708d9c125864b3daf2bf20d5247"},
    {"HIGHT CBC #4", 1, "88e34f8f081779f1e9f394370ad40589", "268d66a735a81a81", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f", "ce1595085a188c28c18d7708d9c125864b3daf2bf20d5247634a00012fa0e9f0"},
    {"HIGHT CBC #5", 1, "88e34f8f081779f1e9f394370ad40589", "268d66a735a81a81", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f0001020304050607", "ce1595085a188c28c18d7708d9c125864b3daf2bf20d5247634a00012fa0e9f0449de2bcc168516c"},
    {"HIGHT CBC #6", 1, "88e34f8f081779f1e9f394370ad40589", "268d66a735a81a81", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f", "ce1595085a188c28c18d7708d9c125864b3daf2bf20d5247634a00012fa0e9f0449de2bcc168516c0e2745ac37c1a674"},
    {"HIGHT CBC #7", 1, "88e34f8f081779f1e9f394370ad40589", "268d66a735a81a81", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f0001020304050607", "ce1595085a188c28c18d7708d9c125864b3daf2bf20d5247634a00012fa0e9f0449de2bcc168516c0e2745ac37c1a6743a1ea51eb407d14e"},
    {"HIGHT CBC #8", 1, "88e34f8f081779f1e9f394370ad40589", "268d66a735a81a81", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f", "ce1595085a188c28c18d7708d9c125864b3daf2bf20d5247634a00012fa0e9f0449de2bcc168516c0e2745ac37c1a6743a1ea51eb407d14ec8c4b435ca05cc62"},
    {"HIGHT CTR #1", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "00", "b3"},
    {"HIGHT CTR #2", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "0001", "b3d1"},
    {"HIGHT CTR #3", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102", "b3d1ff"},
    {"HIGHT CTR #4", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "00010203", "b3d1fffc"},
    {"HIGHT CTR #5", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "0001020304", "b3d1fffcc2"},
    {"HIGHT CTR #6", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405", "b3d1fffcc2a1"},
    {"HIGHT CTR #7", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "00010203040506", "b3d1fffcc2a19b"},
    {"HIGHT CTR #8", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "0001020304050607", "b3d1fffcc2a19bc0"},
    {"HIGHT CTR #9", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708", "b3d1fffcc2a19bc013"},
    {"HIGHT CTR #10", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "00010203040506070809", "b3d1fffcc2a19bc0130d"},
    {"HIGHT CTR #11", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a", "b3d1fffcc2a19bc0130dc1"},
    {"HIGHT CTR #12", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b", "b3d1fffcc2a19bc0130dc162"},
    {"HIGHT CTR #13", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c", "b3d1fffcc2a19bc0130dc1621c"},
    {"HIGHT CTR #14", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d", "b3d1fffcc2a19bc0130dc1621c58"},
    {"HIGHT CTR #15", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e", "b3d1fffcc2a19bc0130dc1621c5839"},
    {"HIGHT CTR #16", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f", "b3d1fffcc2a19bc0130dc1621c583998"},
    {"HIGHT CTR #17", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f00", "b3d1fffcc2a19bc0130dc1621c5839988a"},
    {"HIGHT CTR #18", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f0001", "b3d1fffcc2a19bc0130dc1621c5839988ad7"},
    {"HIGHT CTR #19", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102", "b3d1fffcc2a19bc0130dc1621c5839988ad7c5"},
    {"HIGHT CTR #20", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f00010203", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b"},
    {"HIGHT CTR #21", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f0001020304", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40"},
    {"HIGHT CTR #22", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2"},
    {"HIGHT CTR #23", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f00010203040506", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5"},
    {"HIGHT CTR #24", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f0001020304050607", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9"},
    {"HIGHT CTR #25", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b957"},
    {"HIGHT CTR #26", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f00010203040506070809", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577a"},
    {"HIGHT CTR #27", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf"},
    {"HIGHT CTR #28", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09"},
    {"HIGHT CTR #29", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6"},
    {"HIGHT CTR #30", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19c"},
    {"HIGHT CTR #31", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3"},
    {"HIGHT CTR #32", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f00", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d7"},
    {"HIGHT CTR #33", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f00010203040506", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b"},
    {"HIGHT CTR #34", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f0001020304050607", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c"},
    {"HIGHT CTR #35", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c78"},
    {"HIGHT CTR #36", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c784e51b768d31c"},
    {"HIGHT CTR #37", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c784e51b768d31cef"},
    {"HIGHT CTR #38", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f00", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c784e51b768d31cefb3"},
    {"HIGHT CTR #39", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f00010203040506", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c784e51b768d31cefb35ef7fa5f075b"},
    {"HIGHT CTR #40", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f0001020304050607", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c784e51b768d31cefb35ef7fa5f075b09"},
    {"HIGHT CTR #41", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c784e51b768d31cefb35ef7fa5f075b099f"},
    {"HIGHT CTR #42", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c784e51b768d31cefb35ef7fa5f075b099f60491e609fc2"},
    {"HIGHT CTR #43", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c784e51b768d31cefb35ef7fa5f075b099f60491e609fc213"},
    {"HIGHT CTR #44", 2, "88e34f8f081779f1e9f394370ad40589", "00000000000000fe", "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f00", "b3d1fffcc2a19bc0130dc1621c5839988ad7c59b40a2d5b9577adf09b6a19ca3d76a453bf70b0b6c784e51b768d31cefb35ef7fa5f075b099f60491e609fc213f2"},
};
#define NVEC ((int)(sizeof VEC / sizeof VEC[0]))


#ifndef HIGHT_DELTA0
#define HIGHT_DELTA0 0x5a   // first constant, delta_0 = 1011010b; the other 127 come from the 7-bit LFSR
#endif
#ifndef HIGHT_ROUNDS
#define HIGHT_ROUNDS 32
#endif

static int hexbytes(const char *s, u8 *out) {   // hex digits (spaces ignored) -> bytes; returns byte count
    int n = 0, half = 0, cur = 0;
    for (; *s; s++) {
        int d;
        if (*s >= '0' && *s <= '9') d = *s - '0'; else if (*s >= 'a' && *s <= 'f') d = *s - 'a' + 10; else continue;
        cur = cur * 16 + d;
        if (half) { out[n++] = (u8)cur; cur = 0; }
        half ^= 1;
    }
    return n;
}
static u8 rol8(u8 x, int r) { return (u8)((x << r) | (x >> (8 - r))); }
static u8 F0(u8 x) { return rol8(x, 1) ^ rol8(x, 2) ^ rol8(x, 7); }
static u8 F1(u8 x) { return rol8(x, 3) ^ rol8(x, 4) ^ rol8(x, 6); }

static void hight_keys(const u8 *mk, u8 *wk, u8 *sk) {
    // delta_i = s_{i+6} ... s_0 with s_0..s_6 the bits of delta_0 (s_0 least significant) and s_{i+7} = s_{i+3} ^ s_i
    u8 s[128 + 7]; u8 d[128];
    for (int i = 0; i < 7; i++) s[i] = (HIGHT_DELTA0 >> i) & 1;
    for (int i = 7; i < 128 + 7; i++) s[i] = s[i - 4] ^ s[i - 7];
    for (int i = 0; i < 128; i++) { u8 v = 0; for (int b = 0; b < 7; b++) v |= (u8)(s[i + b] << b); d[i] = v; }
    for (int i = 0; i < 4; i++) wk[i] = mk[i + 12];
    for (int i = 4; i < 8; i++) wk[i] = mk[i - 4];
    for (int i = 0; i < 8; i++) for (int j = 0; j < 8; j++) {
        sk[16 * i + j] = (u8)(mk[(j - i) & 7] + d[16 * i + j]);
        sk[16 * i + j + 8] = (u8)(mk[((j - i) & 7) + 8] + d[16 * i + j + 8]);
    }
}
static void hight_enc(const u8 *wk, const u8 *sk, const u8 *p, u8 *c) {
    u8 x[8];
    for (int i = 0; i < 8; i++) x[i] = p[i];
    x[0] = (u8)(x[0] + wk[0]); x[2] ^= wk[1]; x[4] = (u8)(x[4] + wk[2]); x[6] ^= wk[3];
    for (int i = 0; i < HIGHT_ROUNDS; i++) {
        u8 u0 = x[7] ^ (u8)(F0(x[6]) + sk[4 * i + 3]);
        u8 u2 = (u8)(x[1] + (F1(x[0]) ^ sk[4 * i]));
        u8 u4 = x[3] ^ (u8)(F0(x[2]) + sk[4 * i + 1]);
        u8 u6 = (u8)(x[5] + (F1(x[4]) ^ sk[4 * i + 2]));
        if (i < HIGHT_ROUNDS - 1) {   // rounds 0..30 rotate the bytes
            u8 y[8] = { u0, x[0], u2, x[2], u4, x[4], u6, x[6] };
            for (int k = 0; k < 8; k++) x[k] = y[k];
        } else {                      // the last round does not rotate
            u8 y[8] = { x[0], u2, x[2], u4, x[4], u6, x[6], u0 };
            for (int k = 0; k < 8; k++) x[k] = y[k];
        }
    }
    x[0] = (u8)(x[0] + wk[4]); x[2] ^= wk[5]; x[4] = (u8)(x[4] + wk[6]); x[6] ^= wk[7];
    for (int i = 0; i < 8; i++) c[i] = x[i];
}

int paper_main(void) {
    int good = 0, groups[3] = {0, 0, 0}, goodg[3] = {0, 0, 0};
    for (int v = 0; v < NVEC; v++) {
        const vec *e = &VEC[v];
        u8 mk[16], iv[8], pt[128], want[128], got[128], wk[8], sk[128];
        if (hexbytes(e->key, mk) != 16) { pstr("FAIL bad key size\n"); return 1; }
        int n = hexbytes(e->pt, pt); int cn = hexbytes(e->ct, want);
        if (e->mode != 0 && hexbytes(e->iv, iv) != 8) { pstr("FAIL bad iv size\n"); return 1; }
        if (n != cn || (e->mode != 2 && (n & 7))) { pstr("FAIL bad vector sizes\n"); return 1; }
        hight_keys(mk, wk, sk);
        if (e->mode == 0) {              // ECB
            for (int b = 0; b < n; b += 8) hight_enc(wk, sk, pt + b, got + b);
        } else if (e->mode == 1) {       // CBC
            u8 prev[8]; for (int i = 0; i < 8; i++) prev[i] = iv[i];
            for (int b = 0; b < n; b += 8) {
                u8 t[8]; for (int i = 0; i < 8; i++) t[i] = pt[b + i] ^ prev[i];
                hight_enc(wk, sk, t, got + b); for (int i = 0; i < 8; i++) prev[i] = got[b + i];
            }
        } else {                         // CTR: 64-bit big-endian counter block incremented per block
            u8 ctr[8]; for (int i = 0; i < 8; i++) ctr[i] = iv[i];
            for (int b = 0; b < n; b += 8) {
                u8 ks[8]; hight_enc(wk, sk, ctr, ks);
                for (int i = 0; i < 8 && b + i < n; i++) got[b + i] = pt[b + i] ^ ks[i];
                for (int i = 7; i >= 0; i--) if (++ctr[i]) break;
            }
        }
        groups[e->mode]++;
        int m = pcheck_bytes(e->name, got, want, n);
        good += m; goodg[e->mode] += m;
    }
    pstr("ECB vectors: "); pdec(goodg[0]); pstr(" of "); pdec(groups[0]); pstr(" match\n");
    pstr("CBC vectors: "); pdec(goodg[1]); pstr(" of "); pdec(groups[1]); pstr(" match\n");
    pstr("CTR vectors: "); pdec(goodg[2]); pstr(" of "); pdec(groups[2]); pstr(" match\n");
    int ok = good == NVEC;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
