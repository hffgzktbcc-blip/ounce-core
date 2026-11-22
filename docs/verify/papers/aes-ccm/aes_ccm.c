// CCM (Counter with CBC-MAC) from D. Whiting, R. Housley, N. Ferguson, "AES Encryption & Authentication Using CTR Mode & CBC-MAC"
// (IEEE 802.11 submission 02/001r2, 2002), as published in RFC 3610 (D. Whiting, R. Housley, N. Ferguson, "Counter with
// CBC-MAC (CCM)", September 2003). Written from the RFC's description (sections 2.2 to 2.4: the B_0 block with its flags byte,
// the length-prefixed header, CBC-MAC of the padded blocks, CTR keystream blocks A_i, tag encrypted with A_0) and FIPS 197
// AES-128 (S-box computed, not tabulated); no third-party code copied.
// Checks all 24 packet vectors of RFC 3610 section 8 (8 or 12 header octets, 31 to 33 byte packets, tag length 8 and 10, 13-byte
// nonces): for each, the printed B_0 ("CBC IV in"), the CBC-MAC value, the encrypted MAC mask S_0 (CTR[MAC]) and the
// complete authenticated-and-encrypted output packet: 4 values x 24 vectors = 96 values, parsed mechanically from the
// fetched RFC. Exit 0 only if all 96 match.
#include "ounce_paper.h"

typedef struct { const char *key, *nonce; int hdr; const char *in, *b0, *cbcmac, *ctrmac, *out; } CcmVec;
static const CcmVec CV[24] = {
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","00000003020100a0a1a2a3a4a5",8,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e",
  "5900000003020100a0a1a2a3a4a50017","2dc697e411ca83a8","3a2e46c8ec33a548",
  "0001020304050607588c979a61c663d2f066d0c2c0f989806d5f6b61dac38417e8d12cfdf926e0"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","00000004030201a0a1a2a3a4a5",8,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
  "5900000004030201a0a1a2a3a4a50018","f7b9056a86926cf3","5728d00496d265e5",
  "000102030405060772c91a36e135f8cf291ca894085c87e3cc15c439c9e43a3ba091d56e10400916"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","00000005040302a0a1a2a3a4a5",8,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
  "5900000005040302a0a1a2a3a4a50019","73b46775c026deaa","396ec01a7db96e6f",
  "000102030405060751b1e5f44a197d1da46b0f8e2d282ae871e838bb64da8596574adaa76fbd9fb0c5"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","00000006050403a0a1a2a3a4a5",12,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e",
  "5900000006050403a0a1a2a3a4a50013","4b4f4b39b593e6bf","dd872a807c75f84e",
  "000102030405060708090a0ba28c6865939a9a79faaa5c4c2a9d4a91cdac8c96c861b9c9e61ef1"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","00000007060504a0a1a2a3a4a5",12,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
  "5900000007060504a0a1a2a3a4a50014","48b9e88255054ab5","1951d78528996726",
  "000102030405060708090a0bdcf1fb7b5d9e23fb9d4e131253658ad86ebdca3e51e83f077d9c2d93"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","00000008070605a0a1a2a3a4a5",12,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
  "5900000008070605a0a1a2a3a4a50015","a527d8156ac359bf","e57ddc56c652922b",
  "000102030405060708090a0b6fc1b011f006568b5171a42d953d469b2570a4bd87405a0443ac91cb94"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","00000009080706a0a1a2a3a4a5",8,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e",
  "6100000009080706a0a1a2a3a4a50017","898bd6454e2720bbd27e","8d07802562b08c00a6ee",
  "00010203040506070135d1b2c95f41d5d1d4fec185d166b8094e999dfed96c048c56602c97acbb7490"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","0000000a090807a0a1a2a3a4a5",8,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
  "610000000a090807a0a1a2a3a4a50018","e016e81c7f7b8a38a538","216dac2f8b4f1c07918c",
  "00010203040506077b75399ac0831dd2f0bbd75879a2fd8f6cae6b6cd9b7db24c17b4433f434963f34b4"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","0000000b0a0908a0a1a2a3a4a5",8,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
  "610000000b0a0908a0a1a2a3a4a50019","46a1f6e2b16e75f81cf5","ac3df107da30c48643bb",
  "000102030405060782531a60cc24945a4b8279181ab5c84df21ce7f9b73f42e197ea9c07e56b5eb17e5f4e"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","0000000c0b0a09a0a1a2a3a4a5",12,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e",
  "610000000c0b0a09a0a1a2a3a4a50013","4163893662edd7ebcd6e","1709207609a04e7245b3",
  "000102030405060708090a0b07342594157785152b074098330abb141b947b566aa9406b4d999988dd"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","0000000d0c0b0aa0a1a2a3a4a5",12,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
  "610000000d0c0b0aa0a1a2a3a4a50014","6b5e243412ccc2ad6f1b","9e6486dd02b649c16d37",
  "000102030405060708090a0b676bb20380b0e301e8ab79590a396da78b834934f53aa2e9107a8b6c022c"},
 {"c0c1c2c3c4c5c6c7c8c9cacbcccdcecf","0000000e0d0c0ba0a1a2a3a4a5",12,
  "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
  "610000000e0d0c0ba0a1a2a3a4a50015","9d6692410108d5b6a145","507c315763ef78d3779e",
  "000102030405060708090a0bc0ffa0d6f05bdb67f24d43a4338d2aa4bed7b20e43cd1aa31662e7ad65d6db"},
 {"d7828d13b2b0bdc325a76236df93cc6b","00412b4ea9cdbe3c9696766cfa",8,
  "0be1a88bace018b108e8cf97d820ea258460e96ad9cf5289054d895ceac47c",
  "5900412b4ea9cdbe3c9696766cfa0017","14bddb6bf901634d","f3312ca0f5dcb4fe",
  "0be1a88bace018b14cb97f86a2a4689a877947ab8091ef5386a6ffbdd080f8e78cf7cb0cddd7b3"},
 {"d7828d13b2b0bdc325a76236df93cc6b","0033568ef7b2633c9696766cfa",8,
  "63018f76dc8a1bcb9020ea6f91bdd85afa0039ba4baff9bfb79c7028949cd0ec",
  "590033568ef7b2633c9696766cfa0018","d4cc3b82df9fcc56","11e2d39fa2e80cdc",
  "63018f76dc8a1bcb4ccb1e7ca981befaa0726c55d378061298c85c92814abc33c52ee81d7d77c08a"},
 {"d7828d13b2b0bdc325a76236df93cc6b","00103fe41336713c9696766cfa",8,
  "aa6cfa36cae86b40b916e0eacc1c00d7dcec68ec0b3bbb1a02de8a2d1aa346132e",
  "5900103fe41336713c9696766cfa0019","4729cb0031f181c1","e05fb26eea83b4c7",
  "aa6cfa36cae86b40b1d23a2220ddc0ac900d9aa03c61fcf4a559a4417767089708a776796edb723506"},
 {"d7828d13b2b0bdc325a76236df93cc6b","00764c63b8058e3c9696766cfa",12,
  "d0d0735c531e1becf049c24412daac5630efa5396f770ce1a66b21f7b2101c",
  "5900764c63b8058e3c9696766cfa0013","c18592d984cd6780","5beadb4edf07b92f",
  "d0d0735c531e1becf049c24414d253c3967b70609b7cbb7c499160283245269a6f49975bcadeaf"},
 {"d7828d13b2b0bdc325a76236df93cc6b","00f8b678094e3b3c9696766cfa",12,
  "77b60f011c03e1525899bcaee88b6a46c78d63e52eb8c546efb5de6f75e9cc0d",
  "5900f8b678094e3b3c9696766cfa0014","6adb15b67181b2e2","1cf719c1357fccde",
  "77b60f011c03e1525899bcae5545ff1a085ee2efbf52b2e04bee1e2336c73e3f762c0c7744fe7e3c"},
 {"d7828d13b2b0bdc325a76236df93cc6b","00d560912d3f703c9696766cfa",12,
  "cd9044d2b71fdb8120ea60c06435acbafb11a82e2f071d7ca4a5ebd93a803ba87f",
  "5900d560912d3f703c9696766cfa0015","b04a1c23bc39b651","f74382795c49f300",
  "cd9044d2b71fdb8120ea60c0009769ecabdf48625594c59251e6035722675e04c847099e5ae0704551"},
 {"d7828d13b2b0bdc325a76236df93cc6b","0042fff8f1951c3c9696766cfa",8,
  "d85bc7e69f944fb88a19b950bcf71a018e5e6701c91787659809d67dbedd18",
  "610042fff8f1951c3c9696766cfa0017","9acb03f8b9dbc8d2d2d7","f9b7fa467b9b4045146d",
  "d85bc7e69f944fb8bc218daa947427b6db386a99ac1aef23ade0b52939cb6a637cf9bec2408897c6ba"},
 {"d7828d13b2b0bdc325a76236df93cc6b","00920f40e56cdc3c9696766cfa",8,
  "74a0ebc9069f5b371761433c37c5a35fc1f39f406302eb907c6163be38c98437",
  "6100920f40e56cdc3c9696766cfa0018","8fad0c23e9637e87fa21","7f097befc6aac1d37365",
  "74a0ebc9069f5b375810e6fd25874022e80361a478e3e9cf484ab04f447efff6f0a477cc2fc9bf548944"},
 {"d7828d13b2b0bdc325a76236df93cc6b","0027ca0c7120bc3c9696766cfa",8,
  "44a3aa3aae6475caa434a8e58500c6e41530538862d686ea9e81301b5ae4226bfa",
  "610027ca0c7120bc3c9696766cfa0019","685f8d79d22b9b7421df","251edcdd3f1110f39811",
  "44a3aa3aae6475caf2beed7bc5098e83feb5b31608f8e29c38819a89c8e776f1544d4151a4ed3a8b87b9ce"},
 {"d7828d13b2b0bdc325a76236df93cc6b","005b8ccbcd9af83c9696766cfa",12,
  "ec46bb63b02520c33c49fd70b96b49e21d621741632875db7f6c9243d2d7c2",
  "61005b8ccbcd9af83c9696766cfa0013","219ef890ea64c211a537","5cbc30986602a9f464a0",
  "ec46bb63b02520c33c49fd7031d750a09da3ed7fddd49a2032aabf17ec8ebf7d22c8088c666be5c197"},
 {"d7828d13b2b0bdc325a76236df93cc6b","003ebe94044b9a3c9696766cfa",12,
  "47a65ac78b3d594227e85e71e2fcfbb880442c731bf95167c8ffd7895e337076",
  "61003ebe94044b9a3c9696766cfa0014","664f27163e360f72620d","b93127caf0f1a120fa70",
  "47a65ac78b3d594227e85e71e882f1dbd38ce3eda7c23f04dd65071eb41342acdf7e00dccec7ae52987d"},
 {"d7828d13b2b0bdc325a76236df93cc6b","008d493b30ae8b3c9696766cfa",12,
  "6e37a6ef546d955d34ab6059abf21c0b02feb88f856df4a37381bce3cc128517d4",
  "61008d493b30ae8b3c9696766cfa0015","23e33050bc57dc2c3d3e","4e2daed253f6b18a1d67",
  "6e37a6ef546d955d34ab6059f32905b88a641b04b9c9ffb58cc390900f3da12ab16dce9e82efa16da62059"},
};


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

#ifndef CCM_ADATA_BIT
#define CCM_ADATA_BIT 64
#endif
static u8 B0OUT[16], MACOUT[16], A1OUT[16], S0OUT[16];
// packet = header (hdr bytes) || message; out = header || ciphertext || tag(M); returns packet length
static int ccm_encrypt(const AesKey *k, const u8 *nonce, int nlen, int M, const u8 *pkt, int len, int hdr, u8 *out) {
    int L = 15 - nlen, mlen = len - hdr;
    u8 x[16], blk[16], a[16], s[16];
    // B_0
    blk[0] = (u8)((hdr > 0 ? CCM_ADATA_BIT : 0) + 8 * ((M - 2) / 2) + (L - 1));
    for (int i = 0; i < nlen; i++) blk[1 + i] = nonce[i];
    for (int i = 0; i < L; i++) blk[15 - i] = (u8)((unsigned)mlen >> (8 * i));
    for (int i = 0; i < 16; i++) B0OUT[i] = blk[i];
    aes_enc(k, blk, x);
    // header with 2-byte length prefix (valid for 0 < hdr < 2^16 - 2^8), zero padded to blocks
    static u8 buf[96]; int n = 0;
    if (hdr > 0) { buf[n++] = (u8)(hdr >> 8); buf[n++] = (u8)hdr; for (int i = 0; i < hdr; i++) buf[n++] = pkt[i]; while (n % 16) buf[n++] = 0; }
    for (int i = 0; i < mlen; i++) buf[n++] = pkt[hdr + i];
    while (n % 16) buf[n++] = 0;
    for (int o = 0; o < n; o += 16) { for (int i = 0; i < 16; i++) blk[i] = x[i] ^ buf[o + i]; aes_enc(k, blk, x); }
    for (int i = 0; i < 16; i++) MACOUT[i] = x[i];
    // CTR
    a[0] = (u8)(L - 1); for (int i = 0; i < nlen; i++) a[1 + i] = nonce[i];
    for (int i = 0; i < L; i++) a[15 - i] = 0;
    aes_enc(k, a, s); for (int i = 0; i < 16; i++) S0OUT[i] = s[i];
    for (int i = 0; i < hdr; i++) out[i] = pkt[i];
    for (int off = 0, ctr = 1; off < mlen; off += 16, ctr++) {
        for (int i = 0; i < L; i++) a[15 - i] = (u8)((unsigned)ctr >> (8 * i));
        if (ctr == 1) for (int i = 0; i < 16; i++) A1OUT[i] = a[i];
        aes_enc(k, a, s);
        for (int i = 0; i < 16 && off + i < mlen; i++) out[hdr + off + i] = pkt[hdr + off + i] ^ s[i];
    }
    for (int i = 0; i < M; i++) out[len + i] = MACOUT[i] ^ S0OUT[i];
    return len + M;
}

int paper_main(void) {
    make_sbox();
    int good = 0, total = 0, vecs_ok = 0;
    for (int v = 0; v < 24; v++) {
        u8 key[16], nonce[16], pkt[64], out[80], want[80], wb0[16], wmac[16], wctr[16];
        unhex(CV[v].key, key); int nl = unhex(CV[v].nonce, nonce); int pl = unhex(CV[v].in, pkt); int wl = unhex(CV[v].out, want);
        int M = wl - pl;
        AesKey k; aes_setkey(&k, key, 16);
        int ol = ccm_encrypt(&k, nonce, nl, M, pkt, pl, CV[v].hdr, out);
        int m0 = unhex(CV[v].b0, wb0) == 16 && bytes_eq(B0OUT, wb0, 16);
        int m1 = unhex(CV[v].cbcmac, wmac) == M && bytes_eq(MACOUT, wmac, M);
        unhex(CV[v].ctrmac, wctr);
        int m3 = bytes_eq(S0OUT, wctr, M);
        int m4 = ol == wl && bytes_eq(out, want, wl);
        pstr(m0 && m1 && m3 && m4 ? "PASS " : "FAIL "); pstr("RFC 3610 packet vector "); pdec(v + 1); pstr(": nonce "); pdec(nl); pstr(" bytes, header "); pdec(CV[v].hdr);
        pstr(", tag "); pdec(M); pstr(" bytes, output "); phex_bytes(out, ol); pstr("\n");
        good += m0 + m1 + m3 + m4; total += 4; vecs_ok += m0 && m1 && m3 && m4;
    }
    pstr("values matched: "); pdec(good); pstr(" of "); pdec(total); pstr(" (24 vectors x B_0, CBC-MAC, S_0 mask, output packet); vectors fully matching: "); pdec(vecs_ok); pstr(" of 24\n");
    int ok = good == 96 && total == 96;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
