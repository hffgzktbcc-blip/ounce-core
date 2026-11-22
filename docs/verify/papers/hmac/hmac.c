// HMAC from M. Bellare, R. Canetti, H. Krawczyk, "Keying Hash Functions for Message Authentication" (CRYPTO 1996), as specified in
// RFC 2104 (H. Krawczyk, M. Bellare, R. Canetti): HMAC(K, m) = H((K' xor opad) || H((K' xor ipad) || m)), ipad = 0x36 repeated,
// opad = 0x5c repeated, K' = K zero-padded to the block size or H(K) padded when K is longer. Written from that definition with
// SHA-224, SHA-256, SHA-384 and SHA-512 implemented from FIPS 180-4's description; the SHA-2 constants are not typed in but
// computed from their definition (first 32 or 64 fractional bits of the cube roots of the first 64 or 80 primes, and of the
// square roots of the first 8 primes, with primes 9 to 16 for SHA-384; SHA-224 uses the low 32 bits of SHA-384's values) using
// a small integer big-number routine, so the check also exercises that derivation.
// Checks RFC 4231's seven test cases for all four hash functions (28 values; test case 5 is the 128-bit truncation, the rest
// are full length). Values are data parsed from the fetched RFC. Exit 0 only if all 28 match.
#include "ounce_paper.h"

typedef struct { const char *key, *data, *h224, *h256, *h384, *h512; } HmacVec;
static const HmacVec TV[7] = {
 {"0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b",
  "4869205468657265",
  "896fb1128abbdf196832107cd49df33f47b4b1169912ba4f53684b22","b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
  "afd03944d84895626b0825f4ab46907f15f9dadbe4101ec682aa034c7cebc59cfaea9ea9076ede7f4af152e8b2fa9cb6","87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cdedaa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854"},
 {"4a656665",
  "7768617420646f2079612077616e7420666f72206e6f7468696e673f",
  "a30e01098bc6dbbf45690f3a7e9e6d0f8bbea2a39e6148008fd05e44","5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
  "af45d2e376484031617f78d2b58a6b1b9c7ef464f5a01b47e42ec3736322445e8e2240ca5e69e2c78b3239ecfab21649","164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737"},
 {"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  "7fb3cb3588c6c1f6ffa9694d7d6ad2649365b0c1f65d69d1ec8333ea","773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
  "88062608d3e6ad8a0aa2ace014c8a86f0aa635d947ac9febe83ef4e55966144b2a5ab39dc13814b94e3ab6e101a34f27","fa73b0089d56a284efb0f0756c890be9b1b5dbdd8ee81a3655f83e33b2279d39bf3e848279a722c806b485a47e67c807b946a337bee8942674278859e13292fb"},
 {"0102030405060708090a0b0c0d0e0f10111213141516171819",
  "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
  "6c11506874013cac6a2abc1bb382627cec6a90d86efc012de7afec5a","82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b",
  "3e8a69b7783c25851933ab6290af6ca77a9981480850009cc5577c6e1f573b4e6801dd23c4a7d679ccf8a386c674cffb","b0ba465637458c6990e5a8c5f61d4af7e576d97ff94b872de76f8050361ee3dba91ca5c11aa25eb4d679275cc5788063a5f19741120c4f2de2adebeb10a298dd"},
 {"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c",
  "546573742057697468205472756e636174696f6e",
  "0e2aea68a90c8d37c988bcdb9fca6fa8","a3b6167473100ee06e0c796c2955552b",
  "3abf34c3503b2a23a46efc619baef897","415fad6271580a531d4179bc891d87a6"},
 {"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "54657374205573696e67204c6172676572205468616e20426c6f636b2d53697a65204b6579202d2048617368204b6579204669727374",
  "95e9a0db962095adaebe9b2d6f0dbce2d499f112f2d2b7273fa6870e","60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
  "4ece084485813e9088d2c63a041bc5b44f9ef1012a2b588f3cd11f05033ac4c60c2ef6ab4030fe8296248df163f44952","80b24263c7c1a3ebb71493c1dd7be8b49b46d1f41b4aeec1121b013783f8f3526b56d037e05f2598bd0fd2215d6a1e5295e64f73f63f0aec8b915a985d786598"},
 {"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "5468697320697320612074657374207573696e672061206c6172676572207468616e20626c6f636b2d73697a65206b657920616e642061206c6172676572207468616e20626c6f636b2d73697a6520646174612e20546865206b6579206e6565647320746f20626520686173686564206265666f7265206265696e6720757365642062792074686520484d414320616c676f726974686d2e",
  "3a854166ac5d9f023f54d517d0b39dbd946770db9c2b95c9f6f565d1","9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2",
  "6617178e941f020d351e2f254e8fd32c602420feb0b8fb9adccebb82461e99c5a678cc31e799176d3860e6110c46523e","e37b6a775dc87dbaa4dfa9f96e5e3ffddebd71f8867289865df5a32d20cdc944b6022cac3c4982b10d5eeb55c3e4de15134676fb6de0446065c97440fa8c6a58"},
};


#ifndef HMAC_IPAD
#define HMAC_IPAD 0x36
#endif

// ---- constants from their definitions ---------------------------------------------------------------------------------
// big numbers: little-endian u32 limbs, NL limbs
#define NL 10
static void big_mul(const u32 *a, const u32 *b, u32 *r) {
    u32 t[NL]; for (int i = 0; i < NL; i++) t[i] = 0;
    for (int i = 0; i < NL; i++) {
        u64 carry = 0;
        for (int j = 0; i + j < NL; j++) { u64 v = (u64)a[i] * b[j] + t[i + j] + carry; t[i + j] = (u32)v; carry = v >> 32; }
    }
    for (int i = 0; i < NL; i++) r[i] = t[i];
}
static int big_cmp(const u32 *a, const u32 *b) { for (int i = NL - 1; i >= 0; i--) if (a[i] != b[i]) return a[i] < b[i] ? -1 : 1; return 0; }
// floor of the k-th root (k = 2 or 3) of (p << shift), by bit-by-bit search; returns the low 64 bits of the root
static u64 root_frac(unsigned p, int shift, int k) {
    u32 N[NL]; for (int i = 0; i < NL; i++) N[i] = 0;
    for (int b = 0; b < 16; b++) if ((p >> b) & 1) { int pos = b + shift; N[pos / 32] |= 1u << (pos % 32); }
    u32 x[NL]; for (int i = 0; i < NL; i++) x[i] = 0;
    for (int bit = 3 * 32 - 1 + 8; bit >= 0; bit--) {            // roots here are below 2^(3*32+8)
        u32 y[NL]; for (int i = 0; i < NL; i++) y[i] = x[i];
        y[bit / 32] |= 1u << (bit % 32);
        u32 sq[NL], cu[NL]; big_mul(y, y, sq);
        if (k == 3) big_mul(sq, y, cu); else for (int i = 0; i < NL; i++) cu[i] = sq[i];
        if (big_cmp(cu, N) <= 0) for (int i = 0; i < NL; i++) x[i] = y[i];
    }
    return (u64)x[0] | (u64)x[1] << 32;
}
static unsigned PR[80];
static u32 K256[64]; static u64 K512[80], H512[8], H384[8]; static u32 H256[8], H224[8];
static void init_consts(void) {
    int n = 0; for (unsigned c = 2; n < 80; c++) { int pr = 1; for (int d = 2; d * d <= (int)c; d++) if (c % d == 0) pr = 0; if (pr) PR[n++] = c; }
    for (int i = 0; i < 64; i++) K256[i] = (u32)root_frac(PR[i], 96, 3);
    for (int i = 0; i < 80; i++) K512[i] = root_frac(PR[i], 192, 3);
    for (int i = 0; i < 8; i++) { H256[i] = (u32)root_frac(PR[i], 64, 2); H512[i] = root_frac(PR[i], 128, 2); H384[i] = root_frac(PR[8 + i], 128, 2); H224[i] = (u32)H384[i]; }
}

// ---- SHA-224/256 and SHA-384/512 --------------------------------------------------------------------------------------
#define ROR32(x, n) (((x) >> (n)) | ((x) << (32 - (n))))
#define ROR64(x, n) (((x) >> (n)) | ((x) << (64 - (n))))
static void sha256_block(u32 h[8], const u8 *p) {
    u32 w[64], a, b, c, d, e, f, g, hh;
    for (int i = 0; i < 16; i++) w[i] = (u32)p[4*i] << 24 | (u32)p[4*i+1] << 16 | (u32)p[4*i+2] << 8 | p[4*i+3];
    for (int i = 16; i < 64; i++) {
        u32 s0 = ROR32(w[i-15], 7) ^ ROR32(w[i-15], 18) ^ (w[i-15] >> 3), s1 = ROR32(w[i-2], 17) ^ ROR32(w[i-2], 19) ^ (w[i-2] >> 10);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    a = h[0]; b = h[1]; c = h[2]; d = h[3]; e = h[4]; f = h[5]; g = h[6]; hh = h[7];
    for (int i = 0; i < 64; i++) {
        u32 S1 = ROR32(e, 6) ^ ROR32(e, 11) ^ ROR32(e, 25), ch = (e & f) ^ (~e & g), t1 = hh + S1 + ch + K256[i] + w[i];
        u32 S0 = ROR32(a, 2) ^ ROR32(a, 13) ^ ROR32(a, 22), mj = (a & b) ^ (a & c) ^ (b & c), t2 = S0 + mj;
        hh = g; g = f; f = e; e = d + t1; d = c; c = b; b = a; a = t1 + t2;
    }
    h[0] += a; h[1] += b; h[2] += c; h[3] += d; h[4] += e; h[5] += f; h[6] += g; h[7] += hh;
}
static void sha512_block(u64 h[8], const u8 *p) {
    u64 w[80], a, b, c, d, e, f, g, hh;
    for (int i = 0; i < 16; i++) { w[i] = 0; for (int j = 0; j < 8; j++) w[i] = w[i] << 8 | p[8*i+j]; }
    for (int i = 16; i < 80; i++) {
        u64 s0 = ROR64(w[i-15], 1) ^ ROR64(w[i-15], 8) ^ (w[i-15] >> 7), s1 = ROR64(w[i-2], 19) ^ ROR64(w[i-2], 61) ^ (w[i-2] >> 6);
        w[i] = w[i-16] + s0 + w[i-7] + s1;
    }
    a = h[0]; b = h[1]; c = h[2]; d = h[3]; e = h[4]; f = h[5]; g = h[6]; hh = h[7];
    for (int i = 0; i < 80; i++) {
        u64 S1 = ROR64(e, 14) ^ ROR64(e, 18) ^ ROR64(e, 41), ch = (e & f) ^ (~e & g), t1 = hh + S1 + ch + K512[i] + w[i];
        u64 S0 = ROR64(a, 28) ^ ROR64(a, 34) ^ ROR64(a, 39), mj = (a & b) ^ (a & c) ^ (b & c), t2 = S0 + mj;
        hh = g; g = f; f = e; e = d + t1; d = c; c = b; b = a; a = t1 + t2;
    }
    h[0] += a; h[1] += b; h[2] += c; h[3] += d; h[4] += e; h[5] += f; h[6] += g; h[7] += hh;
}
// variants: 0 = SHA-224, 1 = SHA-256, 2 = SHA-384, 3 = SHA-512
static int blk_of(int v) { return v >= 2 ? 128 : 64; }
static int dig_of(int v) { static const int d[4] = {28, 32, 48, 64}; return d[v]; }
// hash the concatenation of up to two byte strings
static void hash2(int v, const u8 *a, int alen, const u8 *b, int blen, u8 *out) {
    static u8 buf[512]; int total = alen + blen, B = blk_of(v);
    int lenfield = B == 64 ? 8 : 16;
    int padded = ((total + 1 + lenfield + B - 1) / B) * B;
    for (int i = 0; i < padded; i++) buf[i] = 0;
    for (int i = 0; i < alen; i++) buf[i] = a[i];
    for (int i = 0; i < blen; i++) buf[alen + i] = b[i];
    buf[total] = 0x80;
    u64 bits = (u64)total * 8; for (int i = 0; i < 8; i++) buf[padded - 1 - i] = (u8)(bits >> (8 * i));
    if (B == 64) {
        u32 h[8]; for (int i = 0; i < 8; i++) h[i] = v == 0 ? H224[i] : H256[i];
        for (int o = 0; o < padded; o += 64) sha256_block(h, buf + o);
        for (int i = 0; i < dig_of(v); i++) out[i] = (u8)(h[i / 4] >> (24 - 8 * (i % 4)));
    } else {
        u64 h[8]; for (int i = 0; i < 8; i++) h[i] = v == 2 ? H384[i] : H512[i];
        for (int o = 0; o < padded; o += 128) sha512_block(h, buf + o);
        for (int i = 0; i < dig_of(v); i++) out[i] = (u8)(h[i / 8] >> (56 - 8 * (i % 8)));
    }
}
static void hmac(int v, const u8 *key, int klen, const u8 *msg, int mlen, u8 *out) {
    int B = blk_of(v), D = dig_of(v);
    u8 k[128], ip[128], op[128], inner[64];
    for (int i = 0; i < B; i++) k[i] = 0;
    if (klen > B) hash2(v, key, klen, 0, 0, k); else for (int i = 0; i < klen; i++) k[i] = key[i];
    for (int i = 0; i < B; i++) { ip[i] = k[i] ^ HMAC_IPAD; op[i] = k[i] ^ 0x5c; }
    static u8 tmp[512];
    for (int i = 0; i < B; i++) tmp[i] = ip[i];
    for (int i = 0; i < mlen; i++) tmp[B + i] = msg[i];
    hash2(v, tmp, B + mlen, 0, 0, inner);
    hash2(v, op, B, inner, D, out);
}

static int bytes_eq(const u8 *a, const u8 *b, int n) { for (int i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }
static int hexval(char c) { return c <= '9' ? c - '0' : (c | 32) - 'a' + 10; }
static int unhex(const char *h, u8 *out) { int n = 0; while (h[0] && h[1]) { out[n++] = (u8)(hexval(h[0]) * 16 + hexval(h[1])); h += 2; } return n; }

int paper_main(void) {
    init_consts();
    int good = 0, total = 0;
    static const char *names[4] = {"HMAC-SHA-224", "HMAC-SHA-256", "HMAC-SHA-384", "HMAC-SHA-512"};
    // constants against well-known first/last words are not used: the RFC values below are the check
    for (int c = 0; c < 7; c++) {
        u8 key[160], data[160], got[64], want[64];
        int kl = unhex(TV[c].key, key), dl = unhex(TV[c].data, data);
        const char *hs[4] = {TV[c].h224, TV[c].h256, TV[c].h384, TV[c].h512};
        for (int v = 0; v < 4; v++) {
            int wl = unhex(hs[v], want);               // 16 for the truncated test case 5
            hmac(v, key, kl, data, dl, got);
            int m = bytes_eq(got, want, wl);
            pstr(m ? "PASS " : "FAIL "); pstr("RFC 4231 test case "); pdec(c + 1); pstr(" "); pstr(names[v]); pstr(": "); phex_bytes(got, wl); pstr("\n");
            good += m; total++;
        }
    }
    pstr("HMAC values matched: "); pdec(good); pstr(" of "); pdec(total); pstr("\n");
    int ok = good == 28 && total == 28;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
