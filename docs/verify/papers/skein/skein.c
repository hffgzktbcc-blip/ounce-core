// Skein (Ferguson, Lucks, Schneier, Whiting, Bellare, Kohno, Callas, Walker, "The Skein Hash Function Family", version 1.3,
// 1 October 2010; a SHA-3 finalist), written from the specification's Section 3: Threefish-256/512/1024 (rotation table,
// word permutation, key schedule with C240), the UBI chaining mode with its tweak fields, the configuration string and
// the output function. The rotation constants, word permutations and C240 are the specification's tables (data). Checks
// the specification's published values: the 13 precomputed initial chaining values of Appendix B (the state after the
// configuration block, for Skein-256 with 128/160/224/256-bit output, Skein-512 with 128/160/224/256/384/512 and
// Skein-1024 with 384/512/1024) and the 9 test vectors of Appendix C (Skein-256-256, Skein-512-512 and Skein-1024-1024,
// each on the messages FF, FF FE FD ... of 1 byte and of the state size in bytes, plus twice the state size). 22 published values in all. The values
// are data copied as numbers; no code was copied. Exit 0 only if all 22 match.
// Negative control: -DSKEIN_ROT0=15 changes the first Threefish-256 rotation constant (14 -> 15).
#include "ounce_paper.h"

#ifndef SKEIN_ROT0
#define SKEIN_ROT0 14
#endif
static const u8 ROT_TAB[8][14] = {
    {14, 16, 46, 36, 19, 37, 24, 13, 8, 47, 8, 17, 22, 37},
    {52, 57, 33, 27, 14, 42, 38, 19, 10, 55, 49, 18, 23, 52},
    {23, 40, 17, 49, 36, 39, 33, 4, 51, 13, 34, 41, 59, 17},
    {5, 37, 44, 9, 54, 56, 5, 20, 48, 41, 47, 28, 16, 25},
    {25, 33, 39, 30, 34, 24, 41, 9, 37, 31, 12, 47, 44, 30},
    {46, 12, 13, 50, 10, 17, 16, 34, 56, 51, 4, 53, 42, 41},
    {58, 22, 25, 29, 39, 43, 31, 44, 47, 46, 19, 42, 44, 25},
    {32, 32, 8, 35, 56, 22, 9, 48, 35, 52, 23, 31, 37, 20}
};
static const u8 PI4[4] = {0, 3, 2, 1}, PI8[8] = {2, 1, 4, 7, 6, 5, 0, 3}, PI16[16] = {0, 9, 2, 13, 6, 11, 4, 15, 10, 7, 12, 3, 14, 5, 8, 1};
#define C240 0x1BD11BDAA9FC1A22ull
struct iv { int state_bits, out_bits; u64 w[16]; };
static const struct iv IVS[13] = {
    {256, 128, {0xE1111906964D7260ull, 0x883DAAA77C8D811Cull, 0x10080DF491960F7Aull, 0xCCF7DDE5B45BC1C2ull}},
    {256, 160, {0x1420231472825E98ull, 0x2AC4E9A25A77E590ull, 0xD47A58568838D63Eull, 0x2DD2E4968586AB7Dull}},
    {256, 224, {0xC6098A8C9AE5EA0Bull, 0x876D568608C5191Cull, 0x99CB88D7D7F53884ull, 0x384BDDB1AEDDB5DEull}},
    {256, 256, {0xFC9DA860D048B449ull, 0x2FCA66479FA7D833ull, 0xB33BC3896656840Full, 0x6A54E920FDE8DA69ull}},
    {512, 128, {0xA8BC7BF36FBF9F52ull, 0x1E9872CEBD1AF0AAull, 0x309B1790B32190D3ull, 0xBCFBB8543F94805Cull, 0x0DA61BCD6E31B11Bull, 0x1A18EBEAD46A32E3ull, 0xA2CC5B18CE84AA82ull, 0x6982AB289D46982Dull}},
    {512, 160, {0x28B81A2AE013BD91ull, 0xC2F11668B5BDF78Full, 0x1760D8F3F6A56F12ull, 0x4FB747588239904Full, 0x21EDE07F7EAF5056ull, 0xD908922E63ED70B8ull, 0xB8EC76FFECCB52FAull, 0x01A47BB8A3F27A6Eull}},
    {512, 224, {0xCCD0616248677224ull, 0xCBA65CF3A92339EFull, 0x8CCD69D652FF4B64ull, 0x398AED7B3AB890B4ull, 0x0F59D1B1457D2BD0ull, 0x6776FE6575D4EB3Dull, 0x99FBC70E997413E9ull, 0x9E2CFCCFE1C41EF7ull}},
    {512, 256, {0xCCD044A12FDB3E13ull, 0xE83590301A79A9EBull, 0x55AEA0614F816E6Full, 0x2A2767A4AE9B94DBull, 0xEC06025E74DD7683ull, 0xE7A436CDC4746251ull, 0xC36FBAF9393AD185ull, 0x3EEDBA1833EDFC13ull}},
    {512, 384, {0xA3F6C6BF3A75EF5Full, 0xB0FEF9CCFD84FAA4ull, 0x9D77DD663D770CFEull, 0xD798CBF3B468FDDAull, 0x1BC4A6668A0E4465ull, 0x7ED7D434E5807407ull, 0x548FC1ACD4EC44D6ull, 0x266E17546AA18FF8ull}},
    {512, 512, {0x4903ADFF749C51CEull, 0x0D95DE399746DF03ull, 0x8FD1934127C79BCEull, 0x9A255629FF352CB1ull, 0x5DB62599DF6CA7B0ull, 0xEABE394CA9D5C3F4ull, 0x991112C71A75B523ull, 0xAE18A40B660FCC33ull}},
    {1024, 384, {0x5102B6B8C1894A35ull, 0xFEEBC9E3FE8AF11Aull, 0x0C807F06E32BED71ull, 0x60C13A52B41A91F6ull, 0x9716D35DD4917C38ull, 0xE780DF126FD31D3Aull, 0x797846B6C898303Aull, 0xB172C2A8B3572A3Bull, 0xC9BC8203A6104A6Cull, 0x65909338D75624F4ull, 0x94BCC5684B3F81A0ull, 0x3EBBF51E10ECFD46ull, 0x2DF50F0BEEB08542ull, 0x3B5A65300DBC6516ull, 0x484B9CD2167BBCE1ull, 0x2D136947D4CBAFEAull}},
    {1024, 512, {0xCAEC0E5D7C1B1B18ull, 0xA01B0E045F03E802ull, 0x33840451ED912885ull, 0x374AFB04EAEC2E1Cull, 0xDF25A0E2813581F7ull, 0xE40040938B12F9D2ull, 0xA662D539C2ED39B6ull, 0xFA8B85CF45D8C75Aull, 0x8316ED8E29EDE796ull, 0x053289C02E9F91B8ull, 0xC3F8EF1D6D518B73ull, 0xBDCEC3C4D5EF332Eull, 0x549A7E5222974487ull, 0x670708725B749816ull, 0xB9CD28FBF0581BD1ull, 0x0E2940B815804974ull}},
    {1024, 1024, {0xD593DA0741E72355ull, 0x15B5E511AC73E00Cull, 0x5180E5AEBAF2C4F0ull, 0x03BD41D3FCBCAFAFull, 0x1CAEC6FD1983A898ull, 0x6E510B8BCDD0589Full, 0x77E2BDFDC6394ADAull, 0xC11E1DB524DCB0A3ull, 0xD6D14AF9C6329AB5ull, 0x6A9B0BFC6EB67E0Dull, 0x9243C60DCCFF1332ull, 0x1A1F1DDE743F02D4ull, 0x0996753C10ED0BB8ull, 0x6572DD22F2B4969Aull, 0x61FD3062D00A579Aull, 0x1DE0536E8682E539ull}}
};
struct vec { int state_bits, msg_len; const char *out; };
static const struct vec VECS[9] = {
    {256, 1, "0b98dcd198ea0e50a7a244c444e25c23da30c10fc9a1f270a6637f1f34e67ed2"},
    {256, 32, "8d0fa4ef777fd759dfd4044e6f6a5ac3c774aec943dcfc07927b723b5dbf408b"},
    {256, 64, "df28e916630d0b44c4a849dc9a02f07a07cb30f732318256b15d865ac4ae162f"},
    {512, 1, "71b7bce6fe6452227b9ced6014249e5bf9a9754c3ad618ccc4e0aae16b316cc8ca698d864307ed3e80b6ef1570812ac5272dc409b5a012df2a579102f340617a"},
    {512, 64, "45863ba3be0c4dfc27e75d358496f4ac9a736a505d9313b42b2f5eada79fc17f63861e947afb1d056aa199575ad3f8c9a3cc1780b5e5fa4cae050e989876625b"},
    {512, 128, "91cca510c263c4ddd010530a33073309628631f308747e1bcbaa90e451cab92e5188087af4188773a332303e6667a7a210856f742139000071f48e8ba2a5adb7"},
    {1024, 1, "e62c05802ea0152407cdd8787fda9e35703de862a4fbc119cff8590afe79250bccc8b3faf1bd2422ab5c0d263fb2f8afb3f796f048000381531b6f00d85161bc0fff4bef2486b1ebcd3773fabf50ad4ad5639af9040e3f29c6c931301bf79832e9da09857e831e82ef8b4691c235656515d437d2bda33bcec001c67ffde15ba8"},
    {1024, 128, "1f3e02c46fb80a3fcd2dfbbc7c173800b40c60c2354af551189ebf433c3d85f9ff1803e6d920493179ed7ae7fce69c3581a5a2f82d3e0c7a295574d0cd7d217c484d2f6313d59a7718ead07d0729c24851d7e7d2491b902d489194e6b7d369db0ab7aa106f0ee0a39a42efc54f18d93776080985f907574f995ec6a37153a578"},
    {1024, 256, "842a53c99c12b0cf80cf69491be5e2f7515de8733b6ea9422dfd676665b5fa42ffb3a9c48c217777950848cecdb48f640f81fb92bef6f88f7a85c1f7cd1446c9161c0afe8f25ae444f40d3680081c35aa43f640fd5fa3c3c030bcc06abac01d098bcc984ebd8322712921e00b1ba07d6d01f26907050255ef2c8e24f716c52a5"}
};
#define ROL64(x, n) (((x) << (n)) | ((x) >> (64 - (n))))
static int hexval(char c) { return c <= '9' ? c - '0' : c >= 'a' ? c - 'a' + 10 : c - 'A' + 10; }

// Threefish encryption E(K, T, P) on Nw 64-bit words (specification Section 3.3).
static void threefish(int nw, const u64 *key, u64 t0, u64 t1, const u64 *pt, u64 *ct) {
    u64 k[17], t[3], v[16], f[16];
    int nr = nw == 16 ? 80 : 72;
    const u8 *perm = nw == 4 ? PI4 : nw == 8 ? PI8 : PI16;
    int rc = nw == 4 ? 0 : nw == 8 ? 2 : 6;     /* column offset of this Nw in the rotation table: Nw/2 constants each */
    k[nw] = C240; for (int i = 0; i < nw; i++) { k[i] = key[i]; k[nw] ^= key[i]; }
    t[0] = t0; t[1] = t1; t[2] = t0 ^ t1;
    for (int i = 0; i < nw; i++) v[i] = pt[i];
    for (int d = 0; d < nr; d++) {
        if (d % 4 == 0) {
            int s = d / 4;
            for (int i = 0; i < nw; i++) {
                u64 ks = k[(s + i) % (nw + 1)];
                if (i == nw - 3) ks += t[s % 3]; else if (i == nw - 2) ks += t[(s + 1) % 3]; else if (i == nw - 1) ks += (u64)s;
                v[i] += ks;
            }
        }
        for (int j = 0; j < nw / 2; j++) {
            int r = ROT_TAB[d % 8][rc + j]; if (nw == 4 && d % 8 == 0 && j == 0) r = SKEIN_ROT0;
            u64 y0 = v[2 * j] + v[2 * j + 1];
            u64 y1 = ROL64(v[2 * j + 1], r) ^ y0;
            f[2 * j] = y0; f[2 * j + 1] = y1;
        }
        for (int i = 0; i < nw; i++) v[i] = f[perm[i]];
    }
    { int s = nr / 4;
      for (int i = 0; i < nw; i++) {
        u64 ks = k[(s + i) % (nw + 1)];
        if (i == nw - 3) ks += t[s % 3]; else if (i == nw - 2) ks += t[(s + 1) % 3]; else if (i == nw - 1) ks += (u64)s;
        ct[i] = v[i] + ks;
      } }
}
static u64 ld64(const u8 *p) { u64 r = 0; for (int i = 7; i >= 0; i--) r = (r << 8) | p[i]; return r; }
// UBI(G, M, Ts) for a byte-aligned message (Section 3.4). `type` is the type value (tweak bits 120-125).
// H_{i+1} = E(H_i, tweak_i, M_i) xor M_i; tweak = type<<120 + position, with First on block 0 and Final on the last block.
static void ubi(int nw, u64 *g, const u8 *m, int len, int type) {
    int nb = nw * 8; u8 blk[128]; u64 pt[16], ct[16];
    int nblocks = len == 0 ? 1 : (len + nb - 1) / nb;
    for (int b = 0; b < nblocks; b++) {
        for (int i = 0; i < nb; i++) { int idx = b * nb + i; blk[i] = idx < len ? m[idx] : 0; }
        for (int i = 0; i < nw; i++) pt[i] = ld64(blk + 8 * i);
        int pos = (b + 1) * nb < len ? (b + 1) * nb : len;
        u64 t0 = (u64)pos, t1 = (u64)type << 56;
        if (b == 0) t1 |= 1ull << 62;
        if (b == nblocks - 1) t1 |= 1ull << 63;
        threefish(nw, g, t0, t1, pt, ct);
        for (int i = 0; i < nw; i++) g[i] = ct[i] ^ pt[i];
    }
}
// Skein with output length out_bits (multiple of 8): K' = 0, G0 = UBI(K', C, Tcfg), G1 = UBI(G0, M, Tmsg), Output.
static void config_state(int nw, int out_bits, u64 *g) {
    u8 cfg[32];
    for (int i = 0; i < 32; i++) cfg[i] = 0;
    cfg[0] = 0x53; cfg[1] = 0x48; cfg[2] = 0x41; cfg[3] = 0x33; cfg[4] = 1;      /* "SHA3", version 1 */
    for (int i = 0; i < 8; i++) cfg[8 + i] = (u8)((u64)out_bits >> (8 * i));       /* No, tree parameters 0 */
    for (int i = 0; i < nw; i++) g[i] = 0;
    ubi(nw, g, cfg, 32, 4);
}
static void skein(int nw, int out_bits, const u8 *m, int len, u8 *out) {
    u64 g[16], h[16]; int nb = nw * 8, nbytes = (out_bits + 7) / 8;
    config_state(nw, out_bits, g);
    ubi(nw, g, m, len, 48);
    for (int c = 0; c * nb < nbytes; c++) {
        u8 ctr[8]; for (int i = 0; i < 8; i++) ctr[i] = (u8)((u64)c >> (8 * i));
        for (int i = 0; i < nw; i++) h[i] = g[i];
        ubi(nw, h, ctr, 8, 63);
        for (int i = 0; i < nb && c * nb + i < nbytes; i++) out[c * nb + i] = (u8)(h[i / 8] >> (8 * (i % 8)));
    }
}
int paper_main(void) {
    int good = 0;
    for (int i = 0; i < 13; i++) {
        int nw = IVS[i].state_bits / 64; u64 g[16]; int ok = 1;
        config_state(nw, IVS[i].out_bits, g);
        for (int j = 0; j < nw; j++) if (g[j] != IVS[i].w[j]) ok = 0;
        good += ok;
        pstr(ok ? "PASS" : "FAIL"); pstr(" initial chaining value Skein-"); pdec(IVS[i].state_bits); pstr("-"); pdec(IVS[i].out_bits); pstr("\n");
    }
    int ivgood = good;
    static u8 msg[256], got[128], want[128];
    for (int i = 0; i < 256; i++) msg[i] = (u8)(255 - i);
    for (int i = 0; i < 9; i++) {
        int nw = VECS[i].state_bits / 64, ob = VECS[i].state_bits, ol = ob / 8;
        for (int j = 0; j < ol; j++) want[j] = (u8)(hexval(VECS[i].out[2 * j]) * 16 + hexval(VECS[i].out[2 * j + 1]));
        skein(nw, ob, msg, VECS[i].msg_len, got);
        char nm[48]; int k = 0; const char *p = "Skein-"; while (*p) nm[k++] = *p++;
        for (int rep = 0; rep < 2; rep++) {
            if (ob >= 1000) nm[k++] = '1';
            nm[k++] = (char)('0' + (ob / 100) % 10); nm[k++] = (char)('0' + (ob / 10) % 10); nm[k++] = (char)('0' + ob % 10);
            if (rep == 0) nm[k++] = '-';
        }
        nm[k++] = ' '; if (VECS[i].msg_len >= 100) nm[k++] = (char)('0' + VECS[i].msg_len / 100);
        if (VECS[i].msg_len >= 10) nm[k++] = (char)('0' + (VECS[i].msg_len / 10) % 10);
        nm[k++] = (char)('0' + VECS[i].msg_len % 10); p = " bytes"; while (*p) nm[k++] = *p++; nm[k] = 0;
        good += pcheck_bytes(nm, got, want, ol);
    }
    pstr("Appendix B initial chaining values: "); pdec(ivgood); pstr(" of 13 match\n");
    pstr("Appendix C test vectors: "); pdec(good - ivgood); pstr(" of 9 match\n");
    pstr(good == 22 ? "PASS\n" : "FAIL\n");
    return good == 22 ? 0 : 1;
}
