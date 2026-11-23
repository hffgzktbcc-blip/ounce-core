// Bech32 and Bech32m checksummed base32 strings and native SegWit addresses, from BIP-0173 (Wuille and Maxwell, "Base32
// address format for native v0-16 witness outputs", 2017) and BIP-0350 (Wuille, "Bech32m format for v1+ witness addresses",
// 2020), written from the BIPs' prose description: a BCH code over GF(32) with generator constants and final xor 1 (Bech32) or
// 0x2bc830a3 (Bech32m). This is a format specification with published test vectors, not a peer-reviewed paper.
// Checks, all 79 published vectors: 14 valid Bech32 / Bech32m strings (accepted, identified as the right variant, and rebuilt by
// the encoder from hrp and data), the 12 + 14 invalid strings (rejected, with the published reason class), the 6 + 8 valid segwit
// addresses (scriptPubKey equal to the published hex) and the 10 + 15 invalid ones (rejected, with the published reason class),
// and that each valid string decodes as exactly one of the two variants. Exit 0 only if everything matches.
#include "ounce_paper.h"
#include "vectors.inc"

#ifndef NEG
#define NEG 0   // NEG=1: Bech32m final xor 0x2bc830a2 instead of 0x2bc830a3; NEG=2: generator constant 0x26508e6d replaced by 0x26508e6c
#endif
static const char CHARSET[] = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
static const u32 GEN[5] = {0x3b6a57b2, 0x26508e6d + 0u - (NEG == 2 ? 1u : 0u), 0x1ea119fa, 0x3d4233dd, 0x2a1462b3};
#define CONST_M (NEG == 1 ? 0x2bc830a2u : 0x2bc830a3u)

enum { OK = 0, E_LEN, E_MIXED, E_NOSEP, E_EMPTY_HRP, E_HRP_RANGE, E_SHORT, E_CHAR, E_CHECKSUM, E_HRP, E_EMPTY_DATA, E_VERSION, E_PAD_LONG, E_PAD_NZ, E_PROGLEN };
static unsigned slen(const char *s) { unsigned n = 0; while (s[n]) n++; return n; }
static int starts(const char *s, const char *p) { while (*p) if (*s++ != *p++) return 0; return 1; }
static int seq(const char *a, const char *b) { return slen(a) == slen(b) && starts(a, b); }

static u32 polymod(const u8 *v, int n) {
    u32 chk = 1;
    for (int i = 0; i < n; i++) {
        u32 top = chk >> 25; chk = (chk & 0x1ffffff) << 5 ^ v[i];
        for (int j = 0; j < 5; j++) if ((top >> j) & 1) chk ^= GEN[j];
    }
    return chk;
}
static int hrp_expand(const char *hrp, int hl, u8 *o) { int n = 0; for (int i = 0; i < hl; i++) o[n++] = (u8)((u8)hrp[i] >> 5); o[n++] = 0; for (int i = 0; i < hl; i++) o[n++] = (u8)((u8)hrp[i] & 31); return n; }
static char lower(char c) { return c >= 'A' && c <= 'Z' ? (char)(c + 32) : c; }

// Decode. Returns OK or an error class; *variant = 1 for Bech32, 2 for Bech32m; hrp/data (5-bit values, checksum removed) filled.
static int b32_decode(const char *s, char *hrp, int *hl_out, u8 *data, int *dl_out, int *variant) {
    int n = (int)slen(s);
    if (n > 90) return E_LEN;
    int hasl = 0, hasu = 0; for (int i = 0; i < n; i++) { if (s[i] >= 'a' && s[i] <= 'z') hasl = 1; if (s[i] >= 'A' && s[i] <= 'Z') hasu = 1; }
    if (hasl && hasu) return E_MIXED;
    int sep = -1; for (int i = 0; i < n; i++) if (s[i] == '1') sep = i;
    if (sep < 0) return E_NOSEP;
    if (sep == 0) return E_EMPTY_HRP;
    for (int i = 0; i < sep; i++) if ((u8)s[i] < 33 || (u8)s[i] > 126) return E_HRP_RANGE;
    if (n - sep - 1 < 6) return E_SHORT;
    u8 vals[300]; int vn = 0; int hl = sep;
    for (int i = 0; i < hl; i++) hrp[i] = lower(s[i]);
    hrp[hl] = 0; vn = hrp_expand(hrp, hl, vals);
    int dl = n - sep - 1;
    for (int i = 0; i < dl; i++) {
        char c = lower(s[sep + 1 + i]); int d = -1; for (int k = 0; k < 32; k++) if (CHARSET[k] == c) d = k;
        if (d < 0 || (u8)s[sep + 1 + i] > 126) return E_CHAR;
        vals[vn++] = (u8)d; if (i < dl - 6) data[i] = (u8)d;
    }
    u32 pm = polymod(vals, vn);
    *hl_out = hl; *dl_out = dl - 6;
    if (pm == 1) { *variant = 1; return OK; }
    if (pm == CONST_M) { *variant = 2; return OK; }
    return E_CHECKSUM;
}
static int b32_encode(const char *hrp, int hl, const u8 *data, int dl, int variant, char *out) {
    u8 vals[300]; int vn = hrp_expand(hrp, hl, vals); for (int i = 0; i < dl; i++) vals[vn++] = data[i];
    for (int i = 0; i < 6; i++) vals[vn++] = 0;
    u32 pm = polymod(vals, vn) ^ (variant == 1 ? 1u : CONST_M); int o = 0;
    for (int i = 0; i < hl; i++) out[o++] = hrp[i];
    out[o++] = '1'; for (int i = 0; i < dl; i++) out[o++] = CHARSET[data[i]];
    for (int i = 0; i < 6; i++) out[o++] = CHARSET[(pm >> (5 * (5 - i))) & 31];
    out[o] = 0; return o;
}
// 5-bit groups -> bytes, no padding allowed beyond 4 zero bits. Returns OK, E_PAD_LONG or E_PAD_NZ.
static int to8(const u8 *d, int n, u8 *o, int *on) {
    u32 acc = 0; int bits = 0, k = 0;
    for (int i = 0; i < n; i++) { acc = (acc << 5) | d[i]; bits += 5; if (bits >= 8) { bits -= 8; o[k++] = (u8)(acc >> bits); acc &= (1u << bits) - 1; } }
    *on = k;
    if (bits >= 5) return E_PAD_LONG;
    if (acc) return E_PAD_NZ;
    return OK;
}
// mode 173: any witness version needs Bech32; mode 350: version 0 needs Bech32, others Bech32m
static int segwit_decode(const char *s, int mode, u8 *spk, int *spk_len) {
    char hrp[100]; u8 data[100], prog[100]; int hl, dl, variant, plen;
    int e = b32_decode(s, hrp, &hl, data, &dl, &variant); if (e) return e;
    if (!(seq(hrp, "bc") || seq(hrp, "tb"))) return E_HRP;
    if (dl == 0) return E_EMPTY_DATA;
    int ver = data[0]; if (ver > 16) return E_VERSION;
    int want = (mode == 173 || ver == 0) ? 1 : 2; if (variant != want) return E_CHECKSUM;
    e = to8(data + 1, dl - 1, prog, &plen); if (e) return e;
    if (plen < 2 || plen > 40) return E_PROGLEN;
    if (ver == 0 && plen != 20 && plen != 32) return E_PROGLEN;
    spk[0] = (u8)(ver ? 0x50 + ver : 0); spk[1] = (u8)plen; for (int i = 0; i < plen; i++) spk[2 + i] = prog[i]; *spk_len = plen + 2; return OK;
}
static int reason_class(const char *r) {
    static const struct { const char *p; int e; } T[] = {
        {"HRP character out of range", E_HRP_RANGE}, {"overall max length exceeded", E_LEN}, {"No separator character", E_NOSEP}, {"Empty HRP", E_EMPTY_HRP}, {"empty HRP", E_EMPTY_HRP},
        {"Invalid data character", E_CHAR}, {"Too short checksum", E_SHORT}, {"Invalid character in checksum", E_CHAR}, {"checksum calculated with uppercase form of HRP", E_CHECKSUM},
        {"Invalid human-readable part", E_HRP}, {"Invalid checksum", E_CHECKSUM}, {"Invalid witness version", E_VERSION}, {"Invalid program length", E_PROGLEN}, {"Mixed case", E_MIXED},
        {"zero padding of more than 4 bits", E_PAD_LONG}, {"Non-zero padding in 8-to-5 conversion", E_PAD_NZ}, {"Empty data section", E_EMPTY_DATA}};
    for (unsigned i = 0; i < sizeof T / sizeof T[0]; i++) if (starts(r, T[i].p)) return T[i].e;
    return -1;
}
static int hexv(char c) { return c <= '9' ? c - '0' : c - 'a' + 10; }
static int hexeq(const u8 *b, int n, const char *h) { if ((int)slen(h) != 2 * n) return 0; for (int i = 0; i < n; i++) if (b[i] != hexv(h[2 * i]) * 16 + hexv(h[2 * i + 1])) return 0; return 1; }

#define CNT(a) ((int)(sizeof(a) / sizeof((a)[0])))
static int valid_list(const char *name, const struct V *L, int n, int variant) {
    int acc = 0;
    for (int i = 0; i < n; i++) {
        char hrp[100]; u8 d[100]; int hl, dl, v; int e = b32_decode(L[i].s, hrp, &hl, d, &dl, &v);
        char re[120]; int good = 0;
        if (e == OK && v == variant) { b32_encode(hrp, hl, d, dl, variant, re); char lo[120]; int k = 0; for (; L[i].s[k]; k++) lo[k] = lower(L[i].s[k]); lo[k] = 0; good = seq(re, lo); }
        acc += good;
    }
    pstr(name); pstr(": accepted as the right variant and rebuilt by the encoder "); pdec(acc); pstr(" of "); pdec(n); pstr("\n");
    return acc == n;
}
static int invalid_list(const char *name, const struct V *L, int n) {
    int rej = 0, cls = 0;
    for (int i = 0; i < n; i++) {
        char hrp[100]; u8 d[100]; int hl, dl, v; int e = b32_decode(L[i].s, hrp, &hl, d, &dl, &v);
        rej += e != OK; cls += e == reason_class(L[i].x);
        if (e == OK || e != reason_class(L[i].x)) { pstr("  FAIL "); pstr(L[i].s); pstr(": got class "); pdec(e); pstr(", published '"); pstr(L[i].x); pstr("'\n"); }
    }
    pstr(name); pstr(": rejected "); pdec(rej); pstr(" of "); pdec(n); pstr(", with the published reason class "); pdec(cls); pstr(" of "); pdec(n); pstr("\n");
    return rej == n && cls == n;
}
static int seg_valid(const char *name, const struct V *L, int n, int mode) {
    int good = 0;
    for (int i = 0; i < n; i++) { u8 spk[60]; int sl = 0; int e = segwit_decode(L[i].s, mode, spk, &sl); good += e == OK && hexeq(spk, sl, L[i].x); }
    pstr(name); pstr(": scriptPubKey equal to the published hex "); pdec(good); pstr(" of "); pdec(n); pstr("\n"); return good == n;
}
static int seg_invalid(const char *name, const struct V *L, int n, int mode) {
    int rej = 0, cls = 0;
    for (int i = 0; i < n; i++) {
        u8 spk[60]; int sl = 0; int e = segwit_decode(L[i].s, mode, spk, &sl); rej += e != OK; cls += e == reason_class(L[i].x);
        if (e == OK || e != reason_class(L[i].x)) { pstr("  FAIL "); pstr(L[i].s); pstr(": got class "); pdec(e); pstr(", published '"); pstr(L[i].x); pstr("'\n"); }
    }
    pstr(name); pstr(": rejected "); pdec(rej); pstr(" of "); pdec(n); pstr(", with the published reason class "); pdec(cls); pstr(" of "); pdec(n); pstr("\n");
    return rej == n && cls == n;
}
int paper_main(void) {
    int ok = 1;
    ok &= valid_list("BIP-173 valid Bech32 strings", B32_VALID, CNT(B32_VALID), 1);
    ok &= invalid_list("BIP-173 invalid Bech32 strings", B32_INVALID, CNT(B32_INVALID));
    ok &= seg_valid("BIP-173 valid segwit addresses", SEG173_VALID, CNT(SEG173_VALID), 173);
    ok &= seg_invalid("BIP-173 invalid segwit addresses", SEG173_INVALID, CNT(SEG173_INVALID), 173);
    ok &= valid_list("BIP-350 valid Bech32m strings", B32M_VALID, CNT(B32M_VALID), 2);
    ok &= invalid_list("BIP-350 invalid Bech32m strings", B32M_INVALID, CNT(B32M_INVALID));
    ok &= seg_valid("BIP-350 valid segwit addresses", SEG350_VALID, CNT(SEG350_VALID), 350);
    ok &= seg_invalid("BIP-350 invalid segwit addresses", SEG350_INVALID, CNT(SEG350_INVALID), 350);
    // no string is valid as both variants: every valid Bech32 string decodes only as variant 1, every Bech32m string only as variant 2
    int only1 = 0, only2 = 0;
    for (int i = 0; i < CNT(B32_VALID); i++) { char h[100]; u8 d[100]; int hl, dl, v; only1 += b32_decode(B32_VALID[i].s, h, &hl, d, &dl, &v) == OK && v == 1; }
    for (int i = 0; i < CNT(B32M_VALID); i++) { char h[100]; u8 d[100]; int hl, dl, v; only2 += b32_decode(B32M_VALID[i].s, h, &hl, d, &dl, &v) == OK && v == 2; }
    pstr("valid strings identified as exactly one variant (Bech32 "); pdec(only1); pstr(" of 7, Bech32m "); pdec(only2); pstr(" of 7)\n");
    ok &= only1 == 7 && only2 == 7;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
