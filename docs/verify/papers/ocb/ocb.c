// OCB3 as specified in RFC 7253 (T. Krovetz, P. Rogaway, "The OCB Authenticated-Encryption Algorithm", May 2014; the
// algorithm descends from P. Rogaway, M. Bellare, J. Black, T. Krovetz, "OCB: A Block-Cipher Mode of Operation for Efficient
// Authenticated Encryption", CCS 2001, and Krovetz-Rogaway, "The Software Performance of Authenticated-Encryption Modes", FSE 2011).
// Written from the RFC's pseudocode (section 4: HASH and OCB-ENCRYPT, offsets from the Stretch of Ktop, L_i by doubling) and
// FIPS 197 AES (S-box computed, not tabulated). Checks the RFC's Appendix A values:
//   - 16 (N,A,P,C) tuples under the AES-128 key 00..0f (tag 128 bits);
//   - the 14 internal values the RFC prints for the last tuple (L_*, L_$, L_0, L_1, bottom, Ktop, Stretch, three offsets,
//     the final partial offset and three checksums);
//   - the 96-bit-tag tuple with the key 0f..00;
//   - the 9 outputs of the long loop for AES-128/192/256 x tag length 128/96/64 (each authenticates a ~20 KB string built from
//     384 earlier encryptions, so each value depends on all of them).
// Values are data parsed from the fetched RFC. Exit 0 only if everything matches.
#include "ounce_paper.h"

typedef struct { const char *n, *a, *p, *c; } OcbVec;
static const OcbVec TUP[16] = {
 {"BBAA99887766554433221100","","","785407BFFFC8AD9EDCC5520AC9111EE6"},
 {"BBAA99887766554433221101","0001020304050607","0001020304050607","6820B3657B6F615A5725BDA0D3B4EB3A257C9AF1F8F03009"},
 {"BBAA99887766554433221102","0001020304050607","","81017F8203F081277152FADE694A0A00"},
 {"BBAA99887766554433221103","","0001020304050607","45DD69F8F5AAE72414054CD1F35D82760B2CD00D2F99BFA9"},
 {"BBAA99887766554433221104","000102030405060708090A0B0C0D0E0F","000102030405060708090A0B0C0D0E0F","571D535B60B277188BE5147170A9A22C3AD7A4FF3835B8C5701C1CCEC8FC3358"},
 {"BBAA99887766554433221105","000102030405060708090A0B0C0D0E0F","","8CF761B6902EF764462AD86498CA6B97"},
 {"BBAA99887766554433221106","","000102030405060708090A0B0C0D0E0F","5CE88EC2E0692706A915C00AEB8B2396F40E1C743F52436BDF06D8FA1ECA343D"},
 {"BBAA99887766554433221107","000102030405060708090A0B0C0D0E0F1011121314151617","000102030405060708090A0B0C0D0E0F1011121314151617","1CA2207308C87C010756104D8840CE1952F09673A448A122C92C62241051F57356D7F3C90BB0E07F"},
 {"BBAA99887766554433221108","000102030405060708090A0B0C0D0E0F1011121314151617","","6DC225A071FC1B9F7C69F93B0F1E10DE"},
 {"BBAA99887766554433221109","","000102030405060708090A0B0C0D0E0F1011121314151617","221BD0DE7FA6FE993ECCD769460A0AF2D6CDED0C395B1C3CE725F32494B9F914D85C0B1EB38357FF"},
 {"BBAA9988776655443322110A","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F","BD6F6C496201C69296C11EFD138A467ABD3C707924B964DEAFFC40319AF5A48540FBBA186C5553C68AD9F592A79A4240"},
 {"BBAA9988776655443322110B","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F","","FE80690BEE8A485D11F32965BC9D2A32"},
 {"BBAA9988776655443322110C","","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F","2942BFC773BDA23CABC6ACFD9BFD5835BD300F0973792EF46040C53F1432BCDFB5E1DDE3BC18A5F840B52E653444D5DF"},
 {"BBAA9988776655443322110D","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F2021222324252627","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F2021222324252627","D5CA91748410C1751FF8A2F618255B68A0A12E093FF454606E59F9C1D0DDC54B65E8628E568BAD7AED07BA06A4A69483A7035490C5769E60"},
 {"BBAA9988776655443322110E","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F2021222324252627","","C5CD9D1850C141E358649994EE701B68"},
 {"BBAA9988776655443322110F","","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F2021222324252627","4412923493C57D5DE0D700F753CCE0D1D2D95060122E9F15A5DDBFC5787E50B5CC55EE507BCB084E479AD363AC366B95A98CA5F3000B1479"},
};
static const char *INT_NAMES[14] = {"L_*","L_$","L_0","L_1","bottom","Ktop","Stretch","Offset_0","Offset_1","Offset_2","Offset_*","Checksum_1","Checksum_2","Checksum_*"};
static const char *INT_VALS[14] = {"C6A13B37878F5B826F4F8162A1C8D879","8D42766F0F1EB704DE9F02C54391B075","1A84ECDE1E3D6E09BD3E058A8723606D","3509D9BC3C7ADC137A7C0B150E46C0DA","15","9862B0FDEE4E2DD56DBA6433F0125AA2","9862B0FDEE4E2DD56DBA6433F0125AA2FAD24D13A063F8B8","587EF72716EAB6DD3219F8092D517D69","42FA1BF908D7D8D48F27FD83AA721D04","77F3C24534AD04C7F55BF696A434DDDE","B152F972B3225F459A1477F405FC05A7","000102030405060708090A0B0C0D0E0F","10101010101010101010101010101010","30313233343536379010101010101010"};
static const OcbVec TAG96 = {"BBAA9988776655443322110D","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F2021222324252627","000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F2021222324252627","1792A4E31E0755FB03E31B22116E6C2DDF9EFD6E33D536F1A0124B0A55BAE884ED93481529C76B6AD0C515F4D1CDD4FDAC4F02AA"}; static const char TAG96_K[]="0F0E0D0C0B0A09080706050403020100";
typedef struct { int keylen, taglen; const char *out; } OcbBig;
static const OcbBig BIG[9] = {
 {16,128,"67E944D23256C5E0B6C61FA22FDF1EA2"},
 {24,128,"F673F2C3E7174AAE7BAE986CA9F29E17"},
 {32,128,"D90EB8E9C977C88B79DD793D7FFA161C"},
 {16,96,"77A3D8E73589158D25D01209"},
 {24,96,"05D56EAD2752C86BE6932C5E"},
 {32,96,"5458359AC23B0CBA9E6330DD"},
 {16,64,"192C9B7BD90BA06A"},
 {24,64,"0066BC6E0EF34E24"},
 {32,64,"7D4EA5D445501CBE"},
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

#ifndef OCB_RB
#define OCB_RB 0x87
#endif
static void dbl(const u8 in[16], u8 out[16]) {
    int msb = in[0] >> 7;
    for (int i = 0; i < 15; i++) out[i] = (u8)((in[i] << 1) | (in[i + 1] >> 7));
    out[15] = (u8)(in[15] << 1);
    if (msb) out[15] ^= OCB_RB;
}
static void xor16(u8 *d, const u8 *s) { for (int i = 0; i < 16; i++) d[i] ^= s[i]; }
static int ntz(unsigned i) { int n = 0; while (!(i & 1)) { n++; i >>= 1; } return n; }

typedef struct { u8 Ls[16], Ld[16], L0[16], L1[16], bottom, Ktop[16], Stretch[24], Off0[16], Off1[16], Off2[16], OffS[16], Ck1[16], Ck2[16], CkS[16]; int have; } OcbDbg;

typedef struct { AesKey k; u8 Ls[16], Ld[16], L[32][16]; } OcbCtx;
static void ocb_init(OcbCtx *c, const u8 *key, int klen) {
    static const u8 zero[16] = {0};
    aes_setkey(&c->k, key, klen);
    aes_enc(&c->k, zero, c->Ls); dbl(c->Ls, c->Ld); dbl(c->Ld, c->L[0]);
    for (int i = 1; i < 32; i++) dbl(c->L[i - 1], c->L[i]);
}
static void ocb_hash(const OcbCtx *c, const u8 *a, int alen, u8 sum[16]) {
    u8 off[16] = {0}, t[16]; for (int i = 0; i < 16; i++) sum[i] = 0;
    int m = alen / 16;
    for (int i = 1; i <= m; i++) {
        xor16(off, c->L[ntz((unsigned)i)]);
        for (int j = 0; j < 16; j++) t[j] = a[16 * (i - 1) + j] ^ off[j];
        aes_enc(&c->k, t, t); xor16(sum, t);
    }
    int rem = alen - 16 * m;
    if (rem > 0) {
        xor16(off, c->Ls);
        for (int j = 0; j < 16; j++) t[j] = 0;
        for (int j = 0; j < rem; j++) t[j] = a[16 * m + j];
        t[rem] = 0x80; xor16(t, off); aes_enc(&c->k, t, t); xor16(sum, t);
    }
}
// out: plen + taglen/8 bytes
static void ocb_encrypt(const OcbCtx *c, int taglen, const u8 *n, int nlen, const u8 *a, int alen, const u8 *p, int plen, u8 *out, OcbDbg *dbg) {
    u8 nonce[16] = {0}, ktop[16], stretch[24], off[16], ck[16] = {0}, t[16], tag[16], hsum[16];
    nonce[0] = (u8)((taglen % 128) << 1);
    nonce[(127 - 8 * nlen) / 8] |= (u8)(0x80 >> ((127 - 8 * nlen) % 8));
    for (int i = 0; i < nlen; i++) nonce[16 - nlen + i] = n[i];
    int bottom = nonce[15] & 63; nonce[15] &= 0xc0;
    aes_enc(&c->k, nonce, ktop);
    for (int i = 0; i < 16; i++) stretch[i] = ktop[i];
    for (int i = 0; i < 8; i++) stretch[16 + i] = ktop[i] ^ ktop[i + 1];
    int sb = bottom / 8, sr = bottom % 8;
    for (int i = 0; i < 16; i++) off[i] = (u8)((stretch[i + sb] << sr) | (sr ? stretch[i + sb + 1] >> (8 - sr) : 0));
    if (dbg) { dbg->have = 1; for (int i = 0; i < 16; i++) { dbg->Ls[i] = c->Ls[i]; dbg->Ld[i] = c->Ld[i]; dbg->L0[i] = c->L[0][i]; dbg->L1[i] = c->L[1][i]; dbg->Ktop[i] = ktop[i]; dbg->Off0[i] = off[i]; }
               dbg->bottom = (u8)bottom; for (int i = 0; i < 24; i++) dbg->Stretch[i] = stretch[i]; }
    int m = plen / 16;
    for (int i = 1; i <= m; i++) {
        xor16(off, c->L[ntz((unsigned)i)]);
        for (int j = 0; j < 16; j++) t[j] = p[16 * (i - 1) + j] ^ off[j];
        aes_enc(&c->k, t, t);
        for (int j = 0; j < 16; j++) out[16 * (i - 1) + j] = t[j] ^ off[j];
        for (int j = 0; j < 16; j++) ck[j] ^= p[16 * (i - 1) + j];
        if (dbg && i == 1) { for (int j = 0; j < 16; j++) { dbg->Off1[j] = off[j]; dbg->Ck1[j] = ck[j]; } }
        if (dbg && i == 2) { for (int j = 0; j < 16; j++) { dbg->Off2[j] = off[j]; dbg->Ck2[j] = ck[j]; } }
    }
    int rem = plen - 16 * m;
    if (rem > 0) {
        xor16(off, c->Ls);
        u8 pad[16]; aes_enc(&c->k, off, pad);
        u8 fin[16]; for (int j = 0; j < 16; j++) fin[j] = 0;
        for (int j = 0; j < rem; j++) { out[16 * m + j] = p[16 * m + j] ^ pad[j]; fin[j] = p[16 * m + j]; }
        fin[rem] = 0x80; xor16(ck, fin);
        if (dbg) for (int j = 0; j < 16; j++) { dbg->OffS[j] = off[j]; dbg->CkS[j] = ck[j]; }
    }
    for (int j = 0; j < 16; j++) t[j] = ck[j] ^ off[j] ^ c->Ld[j];
    aes_enc(&c->k, t, tag);
    ocb_hash(c, a, alen, hsum); xor16(tag, hsum);
    for (int j = 0; j < taglen / 8; j++) out[plen + j] = tag[j];
}

static u8 BIGC[24000], BIGOUT[24100];

int paper_main(void) {
    make_sbox();
    int good = 0, total = 0;
    u8 key[32], n[16], a[64], p[64], want[80], got[80];
    int kl = unhex("000102030405060708090A0B0C0D0E0F", key);
    OcbCtx ctx; ocb_init(&ctx, key, kl);
    OcbDbg dbg = {0};
    for (int i = 0; i < 16; i++) {
        int nl = unhex(TUP[i].n, n), al = unhex(TUP[i].a, a), pl = unhex(TUP[i].p, p), cl = unhex(TUP[i].c, want);
        ocb_encrypt(&ctx, 128, n, nl, a, al, p, pl, got, i == 15 ? &dbg : 0);
        int m = cl == pl + 16 && bytes_eq(got, want, cl);
        pstr(m ? "PASS " : "FAIL "); pstr("RFC 7253 A tuple "); pdec(i + 1); pstr(": A "); pdec(al); pstr(" bytes, P "); pdec(pl); pstr(" bytes: C "); phex_bytes(got, cl); pstr("\n");
        good += m; total++;
    }
    {   // the internal values printed for the last tuple
        const u8 *vals[14] = {dbg.Ls, dbg.Ld, dbg.L0, dbg.L1, &dbg.bottom, dbg.Ktop, dbg.Stretch, dbg.Off0, dbg.Off1, dbg.Off2, dbg.OffS, dbg.Ck1, dbg.Ck2, dbg.CkS};
        int ok = 0;
        for (int i = 0; i < 14; i++) {
            int m;
            if (i == 4) { int d = 0; for (const char *s = INT_VALS[i]; *s; s++) d = d * 10 + (*s - '0'); m = dbg.bottom == d; }
            else { u8 w[24]; int l = unhex(INT_VALS[i], w); m = bytes_eq(vals[i], w, l); }
            pstr(m ? "PASS " : "FAIL "); pstr("internal value "); pstr(INT_NAMES[i]); pstr("\n");
            ok += m;
        }
        good += ok; total += 14;
    }
    {   u8 k2[16]; int k2l = unhex(TAG96_K, k2); OcbCtx c2; ocb_init(&c2, k2, k2l);
        int nl = unhex(TAG96.n, n), al = unhex(TAG96.a, a), pl = unhex(TAG96.p, p), cl = unhex(TAG96.c, want);
        ocb_encrypt(&c2, 96, n, nl, a, al, p, pl, got, 0);
        int m = cl == pl + 12 && bytes_eq(got, want, cl);
        pstr(m ? "PASS " : "FAIL "); pstr("RFC 7253 A 96-bit-tag tuple: C "); phex_bytes(got, cl); pstr("\n"); good += m; total++; }
    for (int s = 0; s < 9; s++) {
        int keylen = BIG[s].keylen, taglen = BIG[s].taglen, tb = taglen / 8;
        u8 kk[32]; for (int i = 0; i < keylen; i++) kk[i] = 0; kk[keylen - 1] = (u8)taglen;
        OcbCtx cc; ocb_init(&cc, kk, keylen);
        static u8 S[128]; for (int i = 0; i < 128; i++) S[i] = 0;
        int clen = 0;
        for (int i = 0; i < 128; i++) {
            u8 nn[12] = {0}; unsigned v;
            v = 3u * (unsigned)i + 1; nn[11] = (u8)v; nn[10] = (u8)(v >> 8);
            ocb_encrypt(&cc, taglen, nn, 12, S, i, S, i, BIGC + clen, 0); clen += i + tb;
            v = 3u * (unsigned)i + 2; nn[11] = (u8)v; nn[10] = (u8)(v >> 8);
            ocb_encrypt(&cc, taglen, nn, 12, 0, 0, S, i, BIGC + clen, 0); clen += i + tb;
            v = 3u * (unsigned)i + 3; nn[11] = (u8)v; nn[10] = (u8)(v >> 8);
            ocb_encrypt(&cc, taglen, nn, 12, S, i, 0, 0, BIGC + clen, 0); clen += tb;
        }
        u8 nn[12] = {0}; nn[11] = (u8)385; nn[10] = (u8)(385 >> 8);
        ocb_encrypt(&cc, taglen, nn, 12, BIGC, clen, 0, 0, BIGOUT, 0);
        int wl = unhex(BIG[s].out, want);
        int m = wl == tb && bytes_eq(BIGOUT, want, tb);
        pstr(m ? "PASS " : "FAIL "); pstr("RFC 7253 A long loop AES-"); pdec(keylen * 8); pstr(" tag "); pdec(taglen); pstr(" bits ("); pdec(clen); pstr(" bytes authenticated): "); phex_bytes(BIGOUT, tb); pstr("\n");
        good += m; total++;
    }
    pstr("values matched: "); pdec(good); pstr(" of "); pdec(total); pstr("\n");
    int ok = good == total && total == 16 + 14 + 1 + 9;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
