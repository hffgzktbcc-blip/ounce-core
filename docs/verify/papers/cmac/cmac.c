// AES-CMAC, the One-Key CBC-MAC (OMAC1) of T. Iwata and K. Kurosawa (FSE 2003), as standardised for AES by NIST SP 800-38B and
// specified in RFC 4493 (J. Song, R. Poovendran, J. Lee, T. Iwata, "The AES-CMAC Algorithm", June 2006). Written from the
// algorithm in RFC 4493 section 2 (subkey generation by doubling in GF(2^128) with Rb = 0x87, CBC-MAC with the last block
// xor-ed with K1 or with the 10* padded block xor-ed with K2) and FIPS 197 AES (S-box computed, not tabulated).
// Checks: RFC 4493's four examples (subkeys K1, K2, AES(K,0) and 4 tags, messages of 0, 16, 40, 64 bytes) and the 12 examples of
// NIST's "CMAC-AES128/192/256" sample file (key length 128, 192, 256 bits; messages of 0, 16, 20, 64 bytes): L, the printed
// subkey and the tag. All values are data parsed from the fetched documents. Exit 0 only if every value matches.
#include "ounce_paper.h"

typedef struct { const char *key, *pt, *L, *sub; int which; const char *tag; } CmacVec;   // which: 1 = K1 printed, 2 = K2 printed
static const CmacVec NIST[12] = {
 {"2b7e151628aed2a6abf7158809cf4f3c","","7df76b0c1ab899b33e42f047b91b546f","f7ddac306ae266ccf90bc11ee46d513b",2,"bb1d6929e95937287fa37d129b756746"},
 {"2b7e151628aed2a6abf7158809cf4f3c","6bc1bee22e409f96e93d7e117393172a","7df76b0c1ab899b33e42f047b91b546f","fbeed618357133667c85e08f7236a8de",1,"070a16b46b4d4144f79bdd9dd04a287c"},
 {"2b7e151628aed2a6abf7158809cf4f3c","6bc1bee22e409f96e93d7e117393172aae2d8a57","7df76b0c1ab899b33e42f047b91b546f","f7ddac306ae266ccf90bc11ee46d513b",2,"7d85449ea6ea19c823a7bf78837dfade"},
 {"2b7e151628aed2a6abf7158809cf4f3c","6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710","7df76b0c1ab899b33e42f047b91b546f","fbeed618357133667c85e08f7236a8de",1,"51f0bebf7e3b9d92fc49741779363cfe"},
 {"8e73b0f7da0e6452c810f32b809079e562f8ead2522c6b7b","","22452d8e49a8a5939f7321ceea6d514b","8914b63926a2964e7dcc873ba9b5452c",2,"d17ddf46adaacde531cac483de7a9367"},
 {"8e73b0f7da0e6452c810f32b809079e562f8ead2522c6b7b","6bc1bee22e409f96e93d7e117393172a","22452d8e49a8a5939f7321ceea6d514b","448a5b1c93514b273ee6439dd4daa296",1,"9e99a7bf31e710900662f65e617c5184"},
 {"8e73b0f7da0e6452c810f32b809079e562f8ead2522c6b7b","6bc1bee22e409f96e93d7e117393172aae2d8a57","22452d8e49a8a5939f7321ceea6d514b","8914b63926a2964e7dcc873ba9b5452c",2,"3d75c194ed96070444a9fa7ec740ecf8"},
 {"8e73b0f7da0e6452c810f32b809079e562f8ead2522c6b7b","6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710","22452d8e49a8a5939f7321ceea6d514b","448a5b1c93514b273ee6439dd4daa296",1,"a1d5df0eed790f794d77589659f39a11"},
 {"603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4","","e568f68194cf76d6174d4cc04310a854","95a3da06533ddb585d3533010c42a0d9",2,"028962f61b7bf89efc6b551f4667d983"},
 {"603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4","6bc1bee22e409f96e93d7e117393172a","e568f68194cf76d6174d4cc04310a854","cad1ed03299eedac2e9a99808621502f",1,"28a7023f452e8f82bd4bf28d8c37c35c"},
 {"603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4","6bc1bee22e409f96e93d7e117393172aae2d8a57","e568f68194cf76d6174d4cc04310a854","95a3da06533ddb585d3533010c42a0d9",2,"156727dc0878944a023c1fe03bad6d93"},
 {"603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4","6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710","e568f68194cf76d6174d4cc04310a854","cad1ed03299eedac2e9a99808621502f",1,"e1992190549f6ed5696a2c056c315410"},
};
static const char RFC_K[]="2b7e151628aed2a6abf7158809cf4f3c", RFC_L[]="7df76b0c1ab899b33e42f047b91b546f", RFC_K1[]="fbeed618357133667c85e08f7236a8de", RFC_K2[]="f7ddac306ae266ccf90bc11ee46d513b";
static const char *RFC_M[4]={"","6bc1bee22e409f96e93d7e117393172a","6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411","6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710"};
static const char *RFC_T[4]={"bb1d6929e95937287fa37d129b756746","070a16b46b4d4144f79bdd9dd04a287c","dfa66747de9ae63030ca32611497c827","51f0bebf7e3b9d92fc49741779363cfe"};


// ---- AES encryption (FIPS 197); S-box computed from GF(2^8) inversion, key sizes 16, 24 and 32 bytes ------------------
static u8 SBOX[256];
static u8 xtime(u8 a) { return (u8)((a << 1) ^ ((a >> 7) * 0x1b)); }
static u8 gmul(u8 a, u8 b) { u8 p = 0; while (b) { if (b & 1) p ^= a; a = xtime(a); b >>= 1; } return p; }
static void make_sbox(void) {
    for (int x = 0; x < 256; x++) {
        u8 inv = 0;
        if (x) { u8 y = (u8)x, r = 1; for (int e = 254, b = 0; b < 8; b++, e >>= 1) { if (e & 1) r = gmul(r, y); y = gmul(y, y); } inv = r; }
        u8 s = inv, t = inv;
        for (int i = 0; i < 4; i++) { t = (u8)((t << 1) | (t >> 7)); s ^= t; }
        SBOX[x] = s ^ 0x63;
    }
}
typedef struct { u8 rk[240]; int rounds; } AesKey;
static void aes_setkey(AesKey *k, const u8 *key, int klen) {
    int nk = klen / 4; k->rounds = nk + 6;
    int total = 16 * (k->rounds + 1);
    for (int i = 0; i < klen; i++) k->rk[i] = key[i];
    u8 rc = 1;
    for (int i = klen; i < total; i += 4) {
        u8 t[4]; for (int j = 0; j < 4; j++) t[j] = k->rk[i - 4 + j];
        if ((i / 4) % nk == 0) { u8 a = t[0]; t[0] = SBOX[t[1]] ^ rc; t[1] = SBOX[t[2]]; t[2] = SBOX[t[3]]; t[3] = SBOX[a]; rc = xtime(rc); }
        else if (nk > 6 && (i / 4) % nk == 4) { for (int j = 0; j < 4; j++) t[j] = SBOX[t[j]]; }
        for (int j = 0; j < 4; j++) k->rk[i + j] = k->rk[i - klen + j] ^ t[j];
    }
}
static void aes_enc(const AesKey *k, const u8 in[16], u8 out[16]) {
    u8 s[16]; for (int i = 0; i < 16; i++) s[i] = in[i] ^ k->rk[i];
    for (int r = 1; r <= k->rounds; r++) {
        u8 t[16];
        for (int i = 0; i < 16; i++) t[i] = SBOX[s[i]];
        for (int c = 0; c < 4; c++) for (int row = 0; row < 4; row++) s[4*c + row] = t[4*((c + row) & 3) + row];
        if (r != k->rounds) for (int c = 0; c < 4; c++) {
            u8 a0 = s[4*c], a1 = s[4*c+1], a2 = s[4*c+2], a3 = s[4*c+3];
            s[4*c]   = xtime(a0) ^ (xtime(a1) ^ a1) ^ a2 ^ a3;
            s[4*c+1] = a0 ^ xtime(a1) ^ (xtime(a2) ^ a2) ^ a3;
            s[4*c+2] = a0 ^ a1 ^ xtime(a2) ^ (xtime(a3) ^ a3);
            s[4*c+3] = (xtime(a0) ^ a0) ^ a1 ^ a2 ^ xtime(a3);
        }
        for (int i = 0; i < 16; i++) s[i] ^= k->rk[16*r + i];
    }
    for (int i = 0; i < 16; i++) out[i] = s[i];
}
static int bytes_eq(const u8 *a, const u8 *b, int n) { for (int i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }
static int hexval(char c) { return c <= '9' ? c - '0' : (c | 32) - 'a' + 10; }
// decode hex string h into out, return length in bytes
static int unhex(const char *h, u8 *out) { int n = 0; while (h[0] && h[1]) { out[n++] = (u8)(hexval(h[0]) * 16 + hexval(h[1])); h += 2; } return n; }

#ifndef CMAC_RB
#define CMAC_RB 0x87
#endif
static void dbl(const u8 in[16], u8 out[16]) {
    int msb = in[0] >> 7;
    for (int i = 0; i < 15; i++) out[i] = (u8)((in[i] << 1) | (in[i + 1] >> 7));
    out[15] = (u8)(in[15] << 1);
    if (msb) out[15] ^= CMAC_RB;
}
static void subkeys(const AesKey *k, u8 L[16], u8 k1[16], u8 k2[16]) {
    static const u8 zero[16] = {0};
    aes_enc(k, zero, L); dbl(L, k1); dbl(k1, k2);
}
static void cmac(const AesKey *k, const u8 *m, int len, u8 tag[16]) {
    u8 L[16], k1[16], k2[16], x[16], blk[16];
    subkeys(k, L, k1, k2);
    int n = (len + 15) / 16; if (n == 0) n = 1;
    int complete = len > 0 && len % 16 == 0;
    for (int i = 0; i < 16; i++) x[i] = 0;
    for (int b = 0; b < n - 1; b++) { for (int i = 0; i < 16; i++) blk[i] = x[i] ^ m[16 * b + i]; aes_enc(k, blk, x); }
    u8 last[16]; int rem = len - 16 * (n - 1);
    for (int i = 0; i < 16; i++) last[i] = 0;
    for (int i = 0; i < rem; i++) last[i] = m[16 * (n - 1) + i];
    if (complete) for (int i = 0; i < 16; i++) last[i] ^= k1[i];
    else { last[rem] = 0x80; for (int i = 0; i < 16; i++) last[i] ^= k2[i]; }
    for (int i = 0; i < 16; i++) blk[i] = x[i] ^ last[i];
    aes_enc(k, blk, tag);
}

int paper_main(void) {
    make_sbox();
    int good = 0, total = 0;
    u8 key[32], msg[80], want[16], got[16], L[16], k1[16], k2[16];
    // RFC 4493
    {   int kl = unhex(RFC_K, key); AesKey ak; aes_setkey(&ak, key, kl); subkeys(&ak, L, k1, k2);
        u8 w[16];
        unhex(RFC_L, w); good += pcheck_bytes("RFC 4493 AES-128(K,0)", L, w, 16); total++;
        unhex(RFC_K1, w); good += pcheck_bytes("RFC 4493 K1", k1, w, 16); total++;
        unhex(RFC_K2, w); good += pcheck_bytes("RFC 4493 K2", k2, w, 16); total++;
        for (int e = 0; e < 4; e++) {
            int ml = unhex(RFC_M[e], msg); unhex(RFC_T[e], want); cmac(&ak, msg, ml, got);
            char nm2[24] = "RFC 4493 example N tag"; nm2[17] = (char)('1' + e);
            good += pcheck_bytes(nm2, got, want, 16); total++;
        }
    }
    // NIST SP 800-38B examples
    for (int e = 0; e < 12; e++) {
        int kl = unhex(NIST[e].key, key); AesKey ak; aes_setkey(&ak, key, kl);
        int ml = unhex(NIST[e].pt, msg);
        subkeys(&ak, L, k1, k2);
        u8 w[16];
        unhex(NIST[e].L, w); int okL = bytes_eq(L, w, 16);
        unhex(NIST[e].sub, w); int oks = bytes_eq(NIST[e].which == 1 ? k1 : k2, w, 16);
        unhex(NIST[e].tag, want); cmac(&ak, msg, ml, got);
        int okt = bytes_eq(got, want, 16);
        pstr(okL && oks && okt ? "PASS " : "FAIL "); pstr("NIST CMAC-AES"); pdec(kl * 8); pstr(" example, message "); pdec(ml); pstr(" bytes: tag "); phex_bytes(got, 16); pstr("\n");
        good += okL + oks + okt; total += 3;
    }
    pstr("values matched: "); pdec(good); pstr(" of "); pdec(total); pstr("\n");
    int ok = good == total && total == 3 + 4 + 36;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
