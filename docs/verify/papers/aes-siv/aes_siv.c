// AES-SIV (Synthetic Initialization Vector) deterministic authenticated encryption, from P. Rogaway and T. Shrimpton,
// "A Provable-Security Treatment of the Key-Wrap Problem" (EUROCRYPT 2006) which introduced SIV, as specified for AES in
// RFC 5297 (D. Harkins, "Synthetic Initialization Vector (SIV) Authenticated Encryption Using the Advanced Encryption Standard
// (AES)", October 2008): S2V over AES-CMAC (CMAC subkeys by doubling in GF(2^128), Rb = 0x87) to make the 128-bit tag/IV, then
// AES-CTR with the IV's bits 31 and 63 cleared. Written from the RFC's description (sections 2.3 to 2.6, 3) and FIPS 197
// AES-256 (S-box computed, not tabulated); no third-party code copied.
// Checks both RFC 5297 Appendix A test vectors (deterministic AEAD with one AD string; nonce-based with two AD strings and a
// nonce): for each, CMAC(zero), the S2V result (the IV), the CTR start value, the ciphertext and the full IV||C output,
// 5 values x 2 vectors = 10 values, all parsed mechanically from the fetched RFC. Exit 0 only if all 10 match.
#include "ounce_paper.h"

typedef struct { const char *key; const char *ad[3]; int nad; const char *pt, *cmac_zero, *v, *ctr, *ct, *ivc; } SivVec;
static const SivVec SV[2] = {
 {"fffefdfcfbfaf9f8f7f6f5f4f3f2f1f0f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff", {"101112131415161718191a1b1c1d1e1f2021222324252627","",""}, 1, "112233445566778899aabbccddee", "0e04dfafc1efbf040140582859bf073a", "85632d07c6e8f37f950acd320a2ecc93", "85632d07c6e8f37f150acd320a2ecc93", "40c02b9690c4dc04daef7f6afe5c", "85632d07c6e8f37f950acd320a2ecc9340c02b9690c4dc04daef7f6afe5c"},
 {"7f7e7d7c7b7a79787776757473727170404142434445464748494a4b4c4d4e4f", {"00112233445566778899aabbccddeeffdeaddadadeaddadaffeeddccbbaa99887766554433221100","102030405060708090a0","09f911029d74e35bd84156c5635688c0"}, 3, "7468697320697320736f6d6520706c61696e7465787420746f20656e6372797074207573696e67205349562d414553", "c8b43b5974960e7ce6a5dd85231e591a", "7bdb6e3b432667eb06f4d14bff2fbd0f", "7bdb6e3b432667eb06f4d14b7f2fbd0f", "cb900f2fddbe404326601965c889bf17dba77ceb094fa663b7a3f748ba8af829ea64ad544a272e9c485b62a3fd5c0d", "7bdb6e3b432667eb06f4d14bff2fbd0fcb900f2fddbe404326601965c889bf17dba77ceb094fa663b7a3f748ba8af829ea64ad544a272e9c485b62a3fd5c0d"},   // AD1, AD2, Nonce
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

#ifndef SIV_RB
#define SIV_RB 0x87
#endif
static void dbl(const u8 in[16], u8 out[16]) {
    int msb = in[0] >> 7;
    for (int i = 0; i < 15; i++) out[i] = (u8)((in[i] << 1) | (in[i + 1] >> 7));
    out[15] = (u8)(in[15] << 1);
    if (msb) out[15] ^= SIV_RB;
}
static void xor16(u8 *d, const u8 *s) { for (int i = 0; i < 16; i++) d[i] ^= s[i]; }
// AES-CMAC (RFC 4493 structure) over a byte string of length len
static void cmac(const AesKey *k, const u8 *m, int len, u8 tag[16]) {
    static const u8 zero[16] = {0};
    u8 L[16], k1[16], k2[16], x[16], blk[16], last[16];
    aes_enc(k, zero, L); dbl(L, k1); dbl(k1, k2);
    int n = (len + 15) / 16; if (n == 0) n = 1;
    int complete = len > 0 && len % 16 == 0;
    for (int i = 0; i < 16; i++) x[i] = 0;
    for (int b = 0; b < n - 1; b++) { for (int i = 0; i < 16; i++) blk[i] = x[i] ^ m[16 * b + i]; aes_enc(k, blk, x); }
    int rem = len - 16 * (n - 1);
    for (int i = 0; i < 16; i++) last[i] = 0;
    for (int i = 0; i < rem; i++) last[i] = m[16 * (n - 1) + i];
    if (complete) xor16(last, k1); else { last[rem] = 0x80; xor16(last, k2); }
    for (int i = 0; i < 16; i++) blk[i] = x[i] ^ last[i];
    aes_enc(k, blk, tag);
}
// S2V(K; S_1..S_n-1, S_n): the last string is the plaintext (n >= 2 here)
static void s2v(const AesKey *k, const u8 *const *s, const int *slen, int n, u8 v[16], u8 cmac_zero[16]) {
    static const u8 zero[16] = {0};
    static u8 tbuf[256];
    u8 d[16], t[16], dd[16];
    cmac(k, zero, 16, d);
    for (int i = 0; i < 16; i++) cmac_zero[i] = d[i];
    for (int i = 0; i < n - 1; i++) { dbl(d, dd); cmac(k, s[i], slen[i], t); xor16(dd, t); for (int j = 0; j < 16; j++) d[j] = dd[j]; }
    int ln = slen[n - 1];
    if (ln >= 16) {
        for (int i = 0; i < ln; i++) tbuf[i] = s[n - 1][i];
        for (int i = 0; i < 16; i++) tbuf[ln - 16 + i] ^= d[i];          // xorend
        cmac(k, tbuf, ln, v);
    } else {
        dbl(d, dd);
        u8 pad[16]; for (int i = 0; i < 16; i++) pad[i] = 0;
        for (int i = 0; i < ln; i++) pad[i] = s[n - 1][i];
        pad[ln] = 0x80; xor16(dd, pad);
        cmac(k, dd, 16, v);
    }
}
int paper_main(void) {
    make_sbox();
    int good = 0, total = 0;
    for (int t = 0; t < 2; t++) {
        static u8 key[32], ad[3][64], pt[64], v[16], cz[16], q[16], ct[64], out[96], want[96], ks[16], ctr[16];
        const u8 *strs[4]; int lens[4]; int ns = 0;
        int kl = unhex(SV[t].key, key);
        AesKey k1, k2; aes_setkey(&k1, key, kl / 2); aes_setkey(&k2, key + kl / 2, kl / 2);
        for (int i = 0; i < SV[t].nad; i++) { lens[ns] = unhex(SV[t].ad[i], ad[i]); strs[ns] = ad[i]; ns++; }
        int pl = unhex(SV[t].pt, pt); lens[ns] = pl; strs[ns] = pt; ns++;
        s2v(&k1, strs, lens, ns, v, cz);
        for (int i = 0; i < 16; i++) q[i] = v[i];
        q[8] &= 0x7f; q[12] &= 0x7f;
        for (int i = 0; i < 16; i++) ctr[i] = q[i];
        for (int off = 0; off < pl; off += 16) {
            aes_enc(&k2, ctr, ks);
            for (int i = 0; i < 16 && off + i < pl; i++) ct[off + i] = pt[off + i] ^ ks[i];
            for (int i = 15; i >= 0; i--) if (++ctr[i]) break;
        }
        for (int i = 0; i < 16; i++) out[i] = v[i];
        for (int i = 0; i < pl; i++) out[16 + i] = ct[i];
        int w;
        w = unhex(SV[t].cmac_zero, want); good += pcheck_bytes(t ? "RFC 5297 A.2 CMAC(zero)" : "RFC 5297 A.1 CMAC(zero)", cz, want, w); total++;
        w = unhex(SV[t].v, want); good += pcheck_bytes(t ? "RFC 5297 A.2 S2V result (IV)" : "RFC 5297 A.1 S2V result (IV)", v, want, w); total++;
        w = unhex(SV[t].ctr, want); good += pcheck_bytes(t ? "RFC 5297 A.2 CTR start" : "RFC 5297 A.1 CTR start", q, want, w); total++;
        w = unhex(SV[t].ct, want); good += pcheck_bytes(t ? "RFC 5297 A.2 ciphertext" : "RFC 5297 A.1 ciphertext", ct, want, w); total++;
        w = unhex(SV[t].ivc, want); good += (w == 16 + pl) && pcheck_bytes(t ? "RFC 5297 A.2 IV||C" : "RFC 5297 A.1 IV||C", out, want, w); total++;
    }
    pstr("values matched: "); pdec(good); pstr(" of "); pdec(total); pstr("\n");
    int ok = good == 10 && total == 10;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
