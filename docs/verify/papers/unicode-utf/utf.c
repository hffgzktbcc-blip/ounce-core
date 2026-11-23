// UTF-8 (RFC 3629), UTF-16 (RFC 2781) and UTF-7 (RFC 2152) encoders and decoders, written from the RFCs' descriptions of the
// bit layouts (RFC 3629 section 3, RFC 2781 section 2.1 and 2.2, RFC 2152 sections 2 and 3). Checks the worked examples that
// the RFCs publish, in both directions: the 4 UTF-8 examples of RFC 3629 section 7 (including the BOM + U+233B4 one), the 4 UTF-16
// forms of the "*=Ra" example of RFC 2781 section 5 (BE, LE, BE with BOM, LE with BOM; the hieroglyph is U+12345), and the 3
// UTF-7 examples of RFC 2152 section 3. 11 published examples, 22 comparisons. Exit 0 only if everything matches.
#include "ounce_paper.h"

#ifndef NEG
#define NEG 0   // NEG=1: UTF-8 continuation bytes use the prefix 0xC0 instead of 0x80; NEG=2: UTF-16 high surrogates start at 0xD900
#endif
#define CONT (NEG == 1 ? 0xC0 : 0x80)

static int u8enc(const u32 *cp, int n, u8 *o) {            // RFC 3629 section 3
    int k = 0;
    for (int i = 0; i < n; i++) {
        u32 c = cp[i];
        if (c < 0x80) o[k++] = (u8)c;
        else if (c < 0x800) { o[k++] = (u8)(0xc0 | c >> 6); o[k++] = (u8)(CONT | (c & 0x3f)); }
        else if (c < 0x10000) { o[k++] = (u8)(0xe0 | c >> 12); o[k++] = (u8)(CONT | (c >> 6 & 0x3f)); o[k++] = (u8)(CONT | (c & 0x3f)); }
        else { o[k++] = (u8)(0xf0 | c >> 18); o[k++] = (u8)(CONT | (c >> 12 & 0x3f)); o[k++] = (u8)(CONT | (c >> 6 & 0x3f)); o[k++] = (u8)(CONT | (c & 0x3f)); }
    }
    return k;
}
static int u8dec(const u8 *b, int n, u32 *o) {              // strict: rejects overlong forms, surrogates, > 10FFFF, bad continuation
    int i = 0, k = 0;
    while (i < n) {
        u8 c = b[i]; u32 v; int len;
        if (c < 0x80) { v = c; len = 1; } else if (c >= 0xc2 && c < 0xe0) { v = c & 0x1f; len = 2; } else if (c >= 0xe0 && c < 0xf0) { v = c & 0x0f; len = 3; }
        else if (c >= 0xf0 && c < 0xf5) { v = c & 0x07; len = 4; } else return -1;
        if (i + len > n) return -1;
        for (int j = 1; j < len; j++) { if ((b[i + j] & 0xc0) != 0x80) return -1; v = (v << 6) | (b[i + j] & 0x3f); }
        if ((len == 3 && v < 0x800) || (len == 4 && v < 0x10000) || v > 0x10ffff || (v >= 0xd800 && v < 0xe000)) return -1;
        o[k++] = v; i += len;
    }
    return k;
}
static int u16enc(const u32 *cp, int n, int big, int bom, u8 *o) {   // RFC 2781 section 2.1
    int k = 0; u16 w[32]; int wn = 0;
    if (bom) w[wn++] = 0xfeff;
    for (int i = 0; i < n; i++) {
        u32 c = cp[i];
        if (c < 0x10000) w[wn++] = (u16)c;
        else { u32 u = c - 0x10000; w[wn++] = (u16)((NEG == 2 ? 0xd900 : 0xd800) | (u >> 10)); w[wn++] = (u16)(0xdc00 | (u & 0x3ff)); }
    }
    for (int i = 0; i < wn; i++) { if (big) { o[k++] = (u8)(w[i] >> 8); o[k++] = (u8)w[i]; } else { o[k++] = (u8)w[i]; o[k++] = (u8)(w[i] >> 8); } }
    return k;
}
static int u16dec(const u8 *b, int n, int *big_out, u32 *o) {         // RFC 2781 section 2.2, BOM detection when present (big endian default)
    int big = 1, i = 0, k = 0; if (n < 2 || (n & 1)) return -1;
    if (b[0] == 0xfe && b[1] == 0xff) { i = 2; } else if (b[0] == 0xff && b[1] == 0xfe) { big = 0; i = 2; }
    if (*big_out >= 0) big = *big_out;                                  // labelled UTF-16BE / UTF-16LE: no BOM processing, endianness given
    else *big_out = big;
    while (i < n) {
        u32 w1 = big ? (u32)(b[i] << 8 | b[i + 1]) : (u32)(b[i + 1] << 8 | b[i]); i += 2;
        if (w1 < 0xd800 || w1 > 0xdfff) { o[k++] = w1; continue; }
        if (w1 > 0xdbff || i >= n) return -1;
        u32 w2 = big ? (u32)(b[i] << 8 | b[i + 1]) : (u32)(b[i + 1] << 8 | b[i]); i += 2;
        if (w2 < 0xdc00 || w2 > 0xdfff) return -1;
        o[k++] = 0x10000 + ((w1 & 0x3ff) << 10 | (w2 & 0x3ff));
    }
    return k;
}
static const char B64[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
static int is_direct(u32 c) {   // RFC 2152 set D, rule 3 space, and set O (written directly here)
    return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '\'' || c == '(' || c == ')' || c == ',' || c == '-' || c == '.' || c == '/' || c == ':' || c == '?' || c == ' '
        || (c >= 33 && c <= 38 && c != 39) || c == '*' || c == ';' || c == '<' || c == '=' || c == '>' || c == '@' || c == '[' || c == ']' || c == '^' || c == '_' || c == '`' || c == '{' || c == '|' || c == '}';   // + Set O (! " # $ % & * ; < = > @ [ ] ^ _ ` { | }), written directly
}
static int is_b64(char c) { for (int i = 0; i < 64; i++) if (B64[i] == c) return 1; return 0; }
static int u7enc(const u32 *cp, int n, char *o) {                    // RFC 2152
    int k = 0, i = 0;
    while (i < n) {
        if (is_direct(cp[i])) { o[k++] = (char)cp[i]; i++; continue; }
        if (cp[i] == '+') { o[k++] = '+'; o[k++] = '-'; i++; continue; }
        o[k++] = '+'; u32 acc = 0; int bits = 0;
        while (i < n && !is_direct(cp[i]) && cp[i] != '+') {
            u32 c = cp[i++]; u32 w[2]; int wn = 0;
            if (c < 0x10000) w[wn++] = c; else { u32 u = c - 0x10000; w[wn++] = 0xd800 | (u >> 10); w[wn++] = 0xdc00 | (u & 0x3ff); }
            for (int j = 0; j < wn; j++) { acc = (acc << 16) | w[j]; bits += 16; while (bits >= 6) { o[k++] = B64[(acc >> (bits - 6)) & 63]; bits -= 6; acc &= (1u << bits) - 1; } }
        }
        if (bits) o[k++] = B64[(acc << (6 - bits)) & 63];
        if (i >= n || is_b64((char)cp[i]) || cp[i] == '-') o[k++] = '-';   // terminator needed before a base64 character or '-'; also written at the end of the text, as in the RFC's last example
    }
    o[k] = 0; return k;
}
static int u7dec(const char *s, u32 *o) {
    int k = 0; u32 acc = 0; int bits = 0, shifted = 0; u32 hi = 0;
    for (int i = 0; s[i]; i++) {
        char c = s[i];
        if (!shifted) {
            if (c == '+') { if (s[i + 1] == '-') { o[k++] = '+'; i++; } else { shifted = 1; acc = 0; bits = 0; hi = 0; } }
            else o[k++] = (u8)c;
        } else {
            int v = -1; for (int j = 0; j < 64; j++) if (B64[j] == c) v = j;
            if (v < 0) { shifted = 0; if (c != '-') o[k++] = (u8)c; continue; }
            acc = (acc << 6) | (u32)v; bits += 6;
            if (bits >= 16) { u32 w = (acc >> (bits - 16)) & 0xffff; bits -= 16; acc &= (1u << bits) - 1;
                if (hi) { o[k++] = 0x10000 + (((hi & 0x3ff) << 10) | (w & 0x3ff)); hi = 0; } else if (w >= 0xd800 && w < 0xdc00) hi = w; else o[k++] = w; }
        }
    }
    return k;
}
static int cpeq(const u32 *a, const u32 *b, int n) { for (int i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }
static int beq(const u8 *a, const u8 *b, int n) { for (int i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }

int paper_main(void) {
    int ok = 1; u8 buf[64]; u32 dec[32]; int pass = 0, total = 0;
    // RFC 3629 section 7
    static const u32 U1[] = {0x41, 0x2262, 0x391, 0x2e}, U2[] = {0xd55c, 0xad6d, 0xc5b4}, U3[] = {0x65e5, 0x672c, 0x8a9e}, U4[] = {0xfeff, 0x233b4};
    static const u8 B1[] = {0x41, 0xe2, 0x89, 0xa2, 0xce, 0x91, 0x2e}, B2[] = {0xed, 0x95, 0x9c, 0xea, 0xb5, 0xad, 0xec, 0x96, 0xb4}, B3[] = {0xe6, 0x97, 0xa5, 0xe6, 0x9c, 0xac, 0xe8, 0xaa, 0x9e}, B4[] = {0xef, 0xbb, 0xbf, 0xf0, 0xa3, 0x8e, 0xb4};
    static const struct { const u32 *cp; int n; const u8 *b; int bn; const char *name; } A[4] = {
        {U1, 4, B1, 7, "A<NOT IDENTICAL TO><ALPHA>."}, {U2, 3, B2, 9, "hangugeo"}, {U3, 3, B3, 9, "nihongo"}, {U4, 2, B4, 7, "BOM + U+233B4"}};
    for (int i = 0; i < 4; i++) {
        int n = u8enc(A[i].cp, A[i].n, buf); int e = n == A[i].bn && beq(buf, A[i].b, n);
        int m = u8dec(A[i].b, A[i].bn, dec); int d = m == A[i].n && cpeq(dec, A[i].cp, m);
        total += 2; pass += e + d; pstr("UTF-8 "); pstr(A[i].name); pstr(": encode "); pstr(e ? "match" : "DIFFERENT"); pstr(", decode "); pstr(d ? "match" : "DIFFERENT"); pstr("\n");
    }
    // RFC 2781 section 5: "*=Ra"
    static const u32 R[] = {0x12345, 0x3d, 0x52, 0x61};
    static const u8 BE[] = {0xd8, 0x08, 0xdf, 0x45, 0x00, 0x3d, 0x00, 0x52, 0x00, 0x61}, LE[] = {0x08, 0xd8, 0x45, 0xdf, 0x3d, 0x00, 0x52, 0x00, 0x61, 0x00};
    static const u8 BEB[] = {0xfe, 0xff, 0xd8, 0x08, 0xdf, 0x45, 0x00, 0x3d, 0x00, 0x52, 0x00, 0x61}, LEB[] = {0xff, 0xfe, 0x08, 0xd8, 0x45, 0xdf, 0x3d, 0x00, 0x52, 0x00, 0x61, 0x00};
    static const struct { const u8 *b; int bn; int big, bom; const char *name; } S[4] = {
        {BE, 10, 1, 0, "UTF-16BE without BOM"}, {LE, 10, 0, 0, "UTF-16LE without BOM"}, {BEB, 12, 1, 1, "UTF-16 big-endian with BOM"}, {LEB, 12, 0, 1, "UTF-16 little-endian with BOM"}};
    for (int i = 0; i < 4; i++) {
        int n = u16enc(R, 4, S[i].big, S[i].bom, buf); int e = n == S[i].bn && beq(buf, S[i].b, n);
        int endian = S[i].bom ? -1 : S[i].big; int m = u16dec(S[i].b, S[i].bn, &endian, dec); int d = m == 4 && cpeq(dec, R, 4) && endian == S[i].big;
        total += 2; pass += e + d; pstr(S[i].name); pstr(": encode "); pstr(e ? "match" : "DIFFERENT"); pstr(", decode "); pstr(d ? "match" : "DIFFERENT"); pstr("\n");
    }
    // RFC 2152 section 3
    static const u32 V1[] = {0x41, 0x2262, 0x391, 0x2e}, V2[] = {0x48, 0x69, 0x20, 0x4d, 0x6f, 0x6d, 0x20, 0x2d, 0x263a, 0x2d, 0x21}, V3[] = {0x65e5, 0x672c, 0x8a9e};
    static const struct { const u32 *cp; int n; const char *s; } W[3] = {{V1, 4, "A+ImIDkQ."}, {V2, 11, "Hi Mom -+Jjo--!"}, {V3, 3, "+ZeVnLIqe-"}};
    for (int i = 0; i < 3; i++) {
        char t[64]; u7enc(W[i].cp, W[i].n, t); int e = 1; { int j = 0; while (t[j] && W[i].s[j] && t[j] == W[i].s[j]) j++; e = t[j] == W[i].s[j]; }
        int m = u7dec(W[i].s, dec); int d = m == W[i].n && cpeq(dec, W[i].cp, m);
        total += 2; pass += e + d; pstr("UTF-7 "); pstr(W[i].s); pstr(": encode "); pstr(e ? "match" : "DIFFERENT"); pstr(", decode "); pstr(d ? "match" : "DIFFERENT"); pstr("\n");
    }
    pstr("published examples reproduced (encode and decode counted separately): "); pdec(pass); pstr(" of "); pdec(total); pstr("\n");
    ok &= pass == total && total == 22;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
