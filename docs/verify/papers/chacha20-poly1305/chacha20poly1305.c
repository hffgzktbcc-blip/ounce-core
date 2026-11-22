// ChaCha20-Poly1305 as specified in RFC 8439 (Y. Nir, A. Langley, "ChaCha20 and Poly1305 for IETF Protocols", June 2018): the IETF
// variant (32-bit block counter, 96-bit nonce) of D. J. Bernstein's ChaCha (SASC 2008), Bernstein's Poly1305 one-time
// authenticator (the polynomial mod 2^130-5 of "The Poly1305-AES message-authentication code", FSE 2005, with a one-time s added),
// and the AEAD construction that derives the Poly1305 key from ChaCha20 block 0. Written from the RFC's description (sections
// 2.1 to 2.8); no third-party code copied.
// Checks the RFC's Appendix A and section 2.8.2 values:
//   A.1  ChaCha20 block function, 5 keystream blocks (counters 0, 1, 1, 2, 0);
//   A.2  ChaCha20 encryption, 3 vectors (messages of 64, 375 and 127 bytes, counters 0, 1, 42);
//   A.3  Poly1305, 11 vectors: 4 long text vectors and the 7 edge cases (RFC vectors 5-11: partial reduction, s overflow,
//        carry, 2^130-5 and 2^130-6 results, 131-bit intermediates);
//   A.4  Poly1305 key generation using ChaCha20, 3 vectors;
//   A.5  AEAD decryption of a 265-byte message (tag verifies, plaintext equals the printed one): 2 values;
//   2.8.2 AEAD encryption example: Poly1305 key, ciphertext and tag: 3 values.
// All values are data parsed from the fetched RFC. Exit 0 only if all 27 match.
#include "ounce_paper.h"

typedef struct { const char *key, *nonce; unsigned ctr; const char *ks; } CcBlock;
static const CcBlock A1[5] = {
 {"0000000000000000000000000000000000000000000000000000000000000000","000000000000000000000000",0,"76b8e0ada0f13d90405d6ae55386bd28bdd219b8a08ded1aa836efcc8b770dc7da41597c5157488d7724e03fb8d84a376a43b8f41518a11cc387b669b2ee6586"},
 {"0000000000000000000000000000000000000000000000000000000000000000","000000000000000000000000",1,"9f07e7be5551387a98ba977c732d080dcb0f29a048e3656912c6533e32ee7aed29b721769ce64e43d57133b074d839d531ed1f28510afb45ace10a1f4b794d6f"},
 {"0000000000000000000000000000000000000000000000000000000000000001","000000000000000000000000",1,"3aeb5224ecf849929b9d828db1ced4dd832025e8018b8160b82284f3c949aa5a8eca00bbb4a73bdad192b5c42f73f2fd4e273644c8b36125a64addeb006c13a0"},
 {"00ff000000000000000000000000000000000000000000000000000000000000","000000000000000000000000",2,"72d54dfbf12ec44b362692df94137f328fea8da73990265ec1bbbea1ae9af0ca13b25aa26cb4a648cb9b9d1be65b2c0924a66c54d545ec1b7374f4872e99f096"},
 {"0000000000000000000000000000000000000000000000000000000000000000","000000000000000000000002",0,"c2c64d378cd536374ae204b9ef933fcd1a8b2288b3dfa49672ab765b54ee27c78a970e0e955c14f3a88e741b97c286f75f8fc299e8148362fa198a39531bed6d"},
};
typedef struct { const char *key, *nonce; unsigned ctr; const char *pt, *ct; } CcEnc;
static const CcEnc A2[3] = {
 {"0000000000000000000000000000000000000000000000000000000000000000","000000000000000000000000",0,
  "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "76b8e0ada0f13d90405d6ae55386bd28bdd219b8a08ded1aa836efcc8b770dc7da41597c5157488d7724e03fb8d84a376a43b8f41518a11cc387b669b2ee6586"},
 {"0000000000000000000000000000000000000000000000000000000000000001","000000000000000000000002",1,
  "416e79207375626d697373696f6e20746f20746865204945544620696e74656e6465642062792074686520436f6e7472696275746f7220666f72207075626c69636174696f6e20617320616c6c206f722070617274206f6620616e204945544620496e7465726e65742d4472616674206f722052464320616e6420616e792073746174656d656e74206d6164652077697468696e2074686520636f6e74657874206f6620616e204945544620616374697669747920697320636f6e7369646572656420616e20224945544620436f6e747269627574696f6e222e20537563682073746174656d656e747320696e636c756465206f72616c2073746174656d656e747320696e20494554462073657373696f6e732c2061732077656c6c206173207772697474656e20616e6420656c656374726f6e696320636f6d6d756e69636174696f6e73206d61646520617420616e792074696d65206f7220706c6163652c207768696368206172652061646472657373656420746f",
  "a3fbf07df3fa2fde4f376ca23e82737041605d9f4f4f57bd8cff2c1d4b7955ec2a97948bd3722915c8f3d337f7d370050e9e96d647b7c39f56e031ca5eb6250d4042e02785ececfa4b4bb5e8ead0440e20b6e8db09d881a7c6132f420e52795042bdfa7773d8a9051447b3291ce1411c680465552aa6c405b7764d5e87bea85ad00f8449ed8f72d0d662ab052691ca66424bc86d2df80ea41f43abf937d3259dc4b2d0dfb48a6c9139ddd7f76966e928e635553ba76c5c879d7b35d49eb2e62b0871cdac638939e25e8a1e0ef9d5280fa8ca328b351c3c765989cbcf3daa8b6ccc3aaf9f3979c92b3720fc88dc95ed84a1be059c6499b9fda236e7e818b04b0bc39c1e876b193bfe5569753f88128cc08aaa9b63d1a16f80ef2554d7189c411f5869ca52c5b83fa36ff216b9c1d30062bebcfd2dc5bce0911934fda79a86f6e698ced759c3ff9b6477338f3da4f9cd8514ea9982ccafb341b2384dd902f3d1ab7ac61dd29c6f21ba5b862f3730e37cfdc4fd806c22f221"},
 {"1c9240a5eb55d38af333888604f6b5f0473917c1402b80099dca5cbc207075c0","000000000000000000000002",42,
  "2754776173206272696c6c69672c20616e642074686520736c6974687920746f7665730a446964206779726520616e642067696d626c6520696e2074686520776162653a0a416c6c206d696d737920776572652074686520626f726f676f7665732c0a416e6420746865206d6f6d65207261746873206f757467726162652e",
  "62e6347f95ed87a45ffae7426f27a1df5fb69110044c0d73118effa95b01e5cf166d3df2d721caf9b21e5fb14c616871fd84c54f9d65b283196c7fe4f60553ebf39c6402c42234e32a356b3e764312a61a5532055716ead6962568f87d3f3f7704c6a8d1bcd1bf4d50d6154b6da731b187b58dfd728afa36757a797ac188d1"},
};
typedef struct { const char *key, *text, *tag; } CcMac;
static const CcMac A3[11] = {
 {"0000000000000000000000000000000000000000000000000000000000000000",
  "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "00000000000000000000000000000000"},
 {"0000000000000000000000000000000036e5f6b5c5e06070f0efca96227a863e",
  "416e79207375626d697373696f6e20746f20746865204945544620696e74656e6465642062792074686520436f6e7472696275746f7220666f72207075626c69636174696f6e20617320616c6c206f722070617274206f6620616e204945544620496e7465726e65742d4472616674206f722052464320616e6420616e792073746174656d656e74206d6164652077697468696e2074686520636f6e74657874206f6620616e204945544620616374697669747920697320636f6e7369646572656420616e20224945544620436f6e747269627574696f6e222e20537563682073746174656d656e747320696e636c756465206f72616c2073746174656d656e747320696e20494554462073657373696f6e732c2061732077656c6c206173207772697474656e20616e6420656c656374726f6e696320636f6d6d756e69636174696f6e73206d61646520617420616e792074696d65206f7220706c6163652c207768696368206172652061646472657373656420746f",
  "36e5f6b5c5e06070f0efca96227a863e"},
 {"36e5f6b5c5e06070f0efca96227a863e00000000000000000000000000000000",
  "416e79207375626d697373696f6e20746f20746865204945544620696e74656e6465642062792074686520436f6e7472696275746f7220666f72207075626c69636174696f6e20617320616c6c206f722070617274206f6620616e204945544620496e7465726e65742d4472616674206f722052464320616e6420616e792073746174656d656e74206d6164652077697468696e2074686520636f6e74657874206f6620616e204945544620616374697669747920697320636f6e7369646572656420616e20224945544620436f6e747269627574696f6e222e20537563682073746174656d656e747320696e636c756465206f72616c2073746174656d656e747320696e20494554462073657373696f6e732c2061732077656c6c206173207772697474656e20616e6420656c656374726f6e696320636f6d6d756e69636174696f6e73206d61646520617420616e792074696d65206f7220706c6163652c207768696368206172652061646472657373656420746f",
  "f3477e7cd95417af89a6b8794c310cf0"},
 {"1c9240a5eb55d38af333888604f6b5f0473917c1402b80099dca5cbc207075c0",
  "2754776173206272696c6c69672c20616e642074686520736c6974687920746f7665730a446964206779726520616e642067696d626c6520696e2074686520776162653a0a416c6c206d696d737920776572652074686520626f726f676f7665732c0a416e6420746865206d6f6d65207261746873206f757467726162652e",
  "4541669a7eaaee61e708dc7cbcc5eb62"},
 {"0200000000000000000000000000000000000000000000000000000000000000",
  "ffffffffffffffffffffffffffffffff",
  "03000000000000000000000000000000"},   // vector #5 (R, S, data, tag format)
 {"02000000000000000000000000000000ffffffffffffffffffffffffffffffff",
  "02000000000000000000000000000000",
  "03000000000000000000000000000000"},   // vector #6 (R, S, data, tag format)
 {"0100000000000000000000000000000000000000000000000000000000000000",
  "fffffffffffffffffffffffffffffffff0ffffffffffffffffffffffffffffff11000000000000000000000000000000",
  "05000000000000000000000000000000"},   // vector #7 (R, S, data, tag format)
 {"0100000000000000000000000000000000000000000000000000000000000000",
  "fffffffffffffffffffffffffffffffffbfefefefefefefefefefefefefefefe01010101010101010101010101010101",
  "00000000000000000000000000000000"},   // vector #8 (R, S, data, tag format)
 {"0200000000000000000000000000000000000000000000000000000000000000",
  "fdffffffffffffffffffffffffffffff",
  "faffffffffffffffffffffffffffffff"},   // vector #9 (R, S, data, tag format)
 {"0100000000000000040000000000000000000000000000000000000000000000",
  "e33594d7505e43b900000000000000003394d7505e4379cd01000000000000000000000000000000000000000000000001000000000000000000000000000000",
  "14000000000000005500000000000000"},   // vector #10 (R, S, data, tag format)
 {"0100000000000000040000000000000000000000000000000000000000000000",
  "e33594d7505e43b900000000000000003394d7505e4379cd010000000000000000000000000000000000000000000000",
  "13000000000000000000000000000000"},   // vector #11 (R, S, data, tag format)
};
typedef struct { const char *key, *nonce, *otk; } CcKeygen;
static const CcKeygen A4[3] = {
 {"0000000000000000000000000000000000000000000000000000000000000000","000000000000000000000000","76b8e0ada0f13d90405d6ae55386bd28bdd219b8a08ded1aa836efcc8b770dc7"},
 {"0000000000000000000000000000000000000000000000000000000000000001","000000000000000000000002","ecfa254f845f647473d3cb140da9e87606cb33066c447b87bc2666dde3fbb739"},
 {"1c9240a5eb55d38af333888604f6b5f0473917c1402b80099dca5cbc207075c0","000000000000000000000002","965e3bc6f9ec7ed9560808f4d229f94b137ff275ca9b3fcbdd59deaad23310ae"},
};
typedef struct { const char *key, *nonce, *aad, *pt, *ct, *tag, *otk; } CcAead;
// A.5 (ciphertext, nonce, AAD, received tag, decrypted plaintext) and RFC section 2.8.2 (encryption example)
static const CcAead AEAD[2] = {
 {"1c9240a5eb55d38af333888604f6b5f0473917c1402b80099dca5cbc207075c0","000000000102030405060708","f33388860000000000004e91",
  "496e7465726e65742d4472616674732061726520647261667420646f63756d656e74732076616c696420666f722061206d6178696d756d206f6620736978206d6f6e74687320616e64206d617920626520757064617465642c207265706c616365642c206f72206f62736f6c65746564206279206f7468657220646f63756d656e747320617420616e792074696d652e20497420697320696e617070726f70726961746520746f2075736520496e7465726e65742d447261667473206173207265666572656e6365206d6174657269616c206f7220746f2063697465207468656d206f74686572207468616e206173202fe2809c776f726b20696e2070726f67726573732e2fe2809d",
  "64a0861575861af460f062c79be643bd5e805cfd345cf389f108670ac76c8cb24c6cfc18755d43eea09ee94e382d26b0bdb7b73c321b0100d4f03b7f355894cf332f830e710b97ce98c8a84abd0b948114ad176e008d33bd60f982b1ff37c8559797a06ef4f0ef61c186324e2b3506383606907b6a7c02b0f9f6157b53c867e4b9166c767b804d46a59b5216cde7a4e99040c5a40433225ee282a1b0a06c523eaf4534d7f83fa1155b0047718cbc546a0d072b04b3564eea1b422273f548271a0bb2316053fa76991955ebd63159434ecebb4e466dae5a1073a6727627097a1049e617d91d361094fa68f0ff77987130305beaba2eda04df997b714d6c6f2c29a6ad5cb4022b02709b","eead9d67890cbb22392336fea1851f38",""},
 {"808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f","070000004041424344454647","50515253c0c1c2c3c4c5c6c7",
  "4c616469657320616e642047656e746c656d656e206f662074686520636c617373206f66202739393a204966204920636f756c64206f6666657220796f75206f6e6c79206f6e652074697020666f7220746865206675747572652c2073756e73637265656e20776f756c642062652069742e",
  "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b6116","1ae10b594f09e26a7e902ecbd0600691","7bac2b252db447af09b67a55a4e955840ae1d6731075d9eb2a9375783ed553ff"},
};


#ifndef CC_ROT_B
#define CC_ROT_B 12
#endif
#ifndef POLY_FOLD
#define POLY_FOLD 5          // 2^130 = 5 mod (2^130-5)
#endif
#define ROTL32(x, b) (((x) << (b)) | ((x) >> (32 - (b))))
#define QR(a, b, c, d) do { a += b; d ^= a; d = ROTL32(d, 16); c += d; b ^= c; b = ROTL32(b, CC_ROT_B); \
                            a += b; d ^= a; d = ROTL32(d, 8);  c += d; b ^= c; b = ROTL32(b, 7); } while (0)

static int bytes_eq(const u8 *a, const u8 *b, int n) { for (int i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }
static int hexval(char c) { return c <= '9' ? c - '0' : (c | 32) - 'a' + 10; }
static int unhex(const char *h, u8 *out) { int n = 0; while (h[0] && h[1]) { out[n++] = (u8)(hexval(h[0]) * 16 + hexval(h[1])); h += 2; } return n; }

static u32 le32c(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }
static void chacha20_block(const u8 key[32], const u8 nonce[12], u32 ctr, u8 out[64]) {
    static const u8 expand[16] = {'e','x','p','a','n','d',' ','3','2','-','b','y','t','e',' ','k'};
    u32 s[16], x[16];
    for (int i = 0; i < 4; i++) s[i] = le32c(expand + 4 * i);
    for (int i = 0; i < 8; i++) s[4 + i] = le32c(key + 4 * i);
    s[12] = ctr; s[13] = le32c(nonce); s[14] = le32c(nonce + 4); s[15] = le32c(nonce + 8);
    for (int i = 0; i < 16; i++) x[i] = s[i];
    for (int r = 0; r < 10; r++) {
        QR(x[0], x[4], x[8], x[12]); QR(x[1], x[5], x[9], x[13]); QR(x[2], x[6], x[10], x[14]); QR(x[3], x[7], x[11], x[15]);
        QR(x[0], x[5], x[10], x[15]); QR(x[1], x[6], x[11], x[12]); QR(x[2], x[7], x[8], x[13]); QR(x[3], x[4], x[9], x[14]);
    }
    for (int i = 0; i < 16; i++) { u32 w = x[i] + s[i]; for (int j = 0; j < 4; j++) out[4 * i + j] = (u8)(w >> (8 * j)); }
}
static void chacha20_xor(const u8 key[32], const u8 nonce[12], u32 ctr, const u8 *in, int len, u8 *out) {
    u8 ks[64];
    for (int off = 0; off < len; off += 64, ctr++) {
        chacha20_block(key, nonce, ctr, ks);
        for (int i = 0; i < 64 && off + i < len; i++) out[off + i] = in[off + i] ^ ks[i];
    }
}

// ---- Poly1305 (arithmetic mod 2^130-5 in five 26-bit limbs) -------------------------------------------------------
static u32 le32(const u8 *p) { return (u32)p[0] | (u32)p[1] << 8 | (u32)p[2] << 16 | (u32)p[3] << 24; }
static void clamp_r(u8 r[16]) {
    r[3] &= 15; r[7] &= 15; r[11] &= 15; r[15] &= 15;
    r[4] &= 252; r[8] &= 252; r[12] &= 252;
}
static void limbs(const u8 b[16], u32 l[5]) {
    l[0] = le32(b) & 0x3ffffff; l[1] = (le32(b + 3) >> 2) & 0x3ffffff; l[2] = (le32(b + 6) >> 4) & 0x3ffffff;
    l[3] = (le32(b + 9) >> 6) & 0x3ffffff; l[4] = (le32(b + 12) >> 8) & 0x3ffffff;
}
// h = (c1 r^q + ... + cq r) mod 2^130-5, fully reduced, as 17 little-endian bytes (only the low 2 bits of byte 16 used)
static void poly_eval(const u8 r_in[16], const u8 *m, unsigned long len, u8 h_out[17]) {
    u8 r8[16]; for (int i = 0; i < 16; i++) r8[i] = r_in[i];
    clamp_r(r8);
    u32 r[5], h[5] = {0, 0, 0, 0, 0}; limbs(r8, r);
    u64 s1 = POLY_FOLD * (u64)r[1], s2 = POLY_FOLD * (u64)r[2], s3 = POLY_FOLD * (u64)r[3], s4 = POLY_FOLD * (u64)r[4];
    for (unsigned long off = 0; off < len; off += 16) {
        u8 blk[16]; unsigned long n = len - off < 16 ? len - off : 16;
        for (unsigned long i = 0; i < 16; i++) blk[i] = 0;
        for (unsigned long i = 0; i < n; i++) blk[i] = m[off + i];
        u32 c[5];
        if (n == 16) { limbs(blk, c); c[4] |= 1u << 24; }
        else { u8 t[17]; for (int i = 0; i < 17; i++) t[i] = 0; for (unsigned long i = 0; i < n; i++) t[i] = blk[i]; t[n] = 1;
               limbs(t, c); c[4] |= (u32)t[16] << 24; }
        for (int i = 0; i < 5; i++) h[i] += c[i];
        u64 d0 = (u64)h[0]*r[0] + (u64)h[1]*s4 + (u64)h[2]*s3 + (u64)h[3]*s2 + (u64)h[4]*s1;
        u64 d1 = (u64)h[0]*r[1] + (u64)h[1]*r[0] + (u64)h[2]*s4 + (u64)h[3]*s3 + (u64)h[4]*s2;
        u64 d2 = (u64)h[0]*r[2] + (u64)h[1]*r[1] + (u64)h[2]*r[0] + (u64)h[3]*s4 + (u64)h[4]*s3;
        u64 d3 = (u64)h[0]*r[3] + (u64)h[1]*r[2] + (u64)h[2]*r[1] + (u64)h[3]*r[0] + (u64)h[4]*s4;
        u64 d4 = (u64)h[0]*r[4] + (u64)h[1]*r[3] + (u64)h[2]*r[2] + (u64)h[3]*r[1] + (u64)h[4]*r[0];
        u64 cy;
        cy = d0 >> 26; h[0] = (u32)d0 & 0x3ffffff; d1 += cy;
        cy = d1 >> 26; h[1] = (u32)d1 & 0x3ffffff; d2 += cy;
        cy = d2 >> 26; h[2] = (u32)d2 & 0x3ffffff; d3 += cy;
        cy = d3 >> 26; h[3] = (u32)d3 & 0x3ffffff; d4 += cy;
        cy = d4 >> 26; h[4] = (u32)d4 & 0x3ffffff; h[0] += (u32)(cy * POLY_FOLD);
        cy = h[0] >> 26; h[0] &= 0x3ffffff; h[1] += (u32)cy;
    }
    // full carry, then conditional subtraction of p = 2^130-5
    u32 cy;
    cy = h[1] >> 26; h[1] &= 0x3ffffff; h[2] += cy;
    cy = h[2] >> 26; h[2] &= 0x3ffffff; h[3] += cy;
    cy = h[3] >> 26; h[3] &= 0x3ffffff; h[4] += cy;
    cy = h[4] >> 26; h[4] &= 0x3ffffff; h[0] += cy * POLY_FOLD;
    cy = h[0] >> 26; h[0] &= 0x3ffffff; h[1] += cy;
    u32 g[5];
    g[0] = h[0] + 5; cy = g[0] >> 26; g[0] &= 0x3ffffff;
    g[1] = h[1] + cy; cy = g[1] >> 26; g[1] &= 0x3ffffff;
    g[2] = h[2] + cy; cy = g[2] >> 26; g[2] &= 0x3ffffff;
    g[3] = h[3] + cy; cy = g[3] >> 26; g[3] &= 0x3ffffff;
    g[4] = h[4] + cy - (1u << 26);
    u32 mask = (g[4] >> 31) - 1;        // all ones if h >= p (no borrow), else zero
    for (int i = 0; i < 5; i++) h[i] = (h[i] & ~mask) | (g[i] & mask);
    u64 acc = 0; int bits = 0, o = 0;
    for (int i = 0; i < 5; i++) { acc |= (u64)h[i] << bits; bits += 26; while (bits >= 8 && o < 16) { h_out[o++] = (u8)acc; acc >>= 8; bits -= 8; } }
    h_out[16] = (u8)acc;
}


// Poly1305 tag with a 32-byte one-time key r || s
static void poly1305_tag(const u8 key[32], const u8 *m, int len, u8 tag[16]) {
    u8 h[17]; poly_eval(key, m, (unsigned long)len, h);
    unsigned c = 0; for (int i = 0; i < 16; i++) { c += (unsigned)h[i] + key[16 + i]; tag[i] = (u8)c; c >>= 8; }
}
static u8 MACBUF[1024];
static void aead_tag(const u8 key[32], const u8 nonce[12], const u8 *aad, int alen, const u8 *ct, int clen, u8 tag[16], u8 otk[32]) {
    u8 blk[64]; chacha20_block(key, nonce, 0, blk);
    for (int i = 0; i < 32; i++) otk[i] = blk[i];
    int n = 0;
    for (int i = 0; i < alen; i++) MACBUF[n++] = aad[i];
    while (n % 16) MACBUF[n++] = 0;
    for (int i = 0; i < clen; i++) MACBUF[n++] = ct[i];
    while (n % 16) MACBUF[n++] = 0;
    for (int i = 0; i < 8; i++) MACBUF[n++] = (u8)((u64)alen >> (8 * i));
    for (int i = 0; i < 8; i++) MACBUF[n++] = (u8)((u64)clen >> (8 * i));
    poly1305_tag(otk, MACBUF, n, tag);
}

static int report(const char *what, int idx, int ok) { pstr(ok ? "PASS " : "FAIL "); pstr(what); pstr(" "); pdec(idx); pstr("\n"); return ok; }

int paper_main(void) {
    int good = 0, total = 0;
    static u8 key[32], nonce[12], a[512], b[512], c[512];
    for (int i = 0; i < 5; i++) {
        unhex(A1[i].key, key); unhex(A1[i].nonce, nonce); unhex(A1[i].ks, a);
        chacha20_block(key, nonce, A1[i].ctr, b);
        good += report("RFC 8439 A.1 block function vector", i + 1, bytes_eq(a, b, 64)); total++;
    }
    for (int i = 0; i < 3; i++) {
        unhex(A2[i].key, key); unhex(A2[i].nonce, nonce); int pl = unhex(A2[i].pt, a); int cl = unhex(A2[i].ct, b);
        chacha20_xor(key, nonce, A2[i].ctr, a, pl, c);
        good += report("RFC 8439 A.2 encryption vector", i + 1, pl == cl && bytes_eq(c, b, cl)); total++;
    }
    for (int i = 0; i < 11; i++) {
        u8 k[32], tag[16], want[16]; unhex(A3[i].key, k); int tl = unhex(A3[i].text, a); unhex(A3[i].tag, want);
        poly1305_tag(k, a, tl, tag);
        good += report("RFC 8439 A.3 Poly1305 vector", i + 1, bytes_eq(tag, want, 16)); total++;
    }
    for (int i = 0; i < 3; i++) {
        u8 otk[64]; unhex(A4[i].key, key); unhex(A4[i].nonce, nonce); unhex(A4[i].otk, a);
        chacha20_block(key, nonce, 0, otk);
        good += report("RFC 8439 A.4 Poly1305 key generation vector", i + 1, bytes_eq(otk, a, 32)); total++;
    }
    {   // A.5: decryption of the received ciphertext
        u8 aad[32], tag[16], want[16], otk[32];
        unhex(AEAD[0].key, key); unhex(AEAD[0].nonce, nonce); int al = unhex(AEAD[0].aad, aad); int cl = unhex(AEAD[0].ct, a); unhex(AEAD[0].tag, want);
        int pl = unhex(AEAD[0].pt, b);
        aead_tag(key, nonce, aad, al, a, cl, tag, otk);
        good += report("RFC 8439 A.5 received tag verifies, vector", 1, bytes_eq(tag, want, 16)); total++;
        chacha20_xor(key, nonce, 1, a, cl, c);
        good += report("RFC 8439 A.5 decrypted plaintext, vector", 1, pl == cl && bytes_eq(c, b, cl)); total++;
    }
    {   // 2.8.2: encryption example
        u8 aad[32], tag[16], want[16], otk[32], wotk[32];
        unhex(AEAD[1].key, key); unhex(AEAD[1].nonce, nonce); int al = unhex(AEAD[1].aad, aad); int pl = unhex(AEAD[1].pt, a); int cl = unhex(AEAD[1].ct, b);
        unhex(AEAD[1].tag, want); unhex(AEAD[1].otk, wotk);
        chacha20_xor(key, nonce, 1, a, pl, c);
        int okc = pl == cl && bytes_eq(c, b, cl);
        aead_tag(key, nonce, aad, al, c, pl, tag, otk);
        good += report("RFC 8439 2.8.2 Poly1305 one-time key, vector", 1, bytes_eq(otk, wotk, 32)); total++;
        good += report("RFC 8439 2.8.2 ciphertext, vector", 1, okc); total++;
        good += report("RFC 8439 2.8.2 tag, vector", 1, bytes_eq(tag, want, 16)); total++;
    }
    pstr("values matched: "); pdec(good); pstr(" of "); pdec(total); pstr("\n");
    int ok = good == total && total == 27;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
