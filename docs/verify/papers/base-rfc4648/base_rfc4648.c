// Base16/Base32/Base32-hex/Base64 from RFC 4648 (Josefsson, "The Base16, Base32, and Base64 Data Encodings", 2006),
// written from the RFC's description of the bit grouping (section 4-8). Checks: the four alphabets (Tables 1-4, built here by
// rule and compared with the RFC's tables), the 28 test vectors of section 10 in both directions (encode and decode), and the three
// worked Base64 examples of section 9. vectors.inc is generated from the RFC text.
// Exit 0 only if everything matches.
#include "ounce_paper.h"
#include "vectors.inc"

#ifndef NEG
#define NEG 0   // NEG=1: pad with '.' instead of '='; NEG=2: base64 alphabet value 62 is '-' instead of '+'
#endif
#define PADCH (NEG == 1 ? '.' : '=')

static unsigned slen(const char *s) { unsigned n = 0; while (s[n]) n++; return n; }

// Encode: take `bits` bits (5 or 6) at a time from the input, MSB first; pad with zero bits on the right, then '=' to a full quantum.
// `quantum` = output characters per full group (8 for base32, 4 for base64).
static unsigned enc(const u8 *in, unsigned n, const char *alpha, int bits, int quantum, char *out) {
    unsigned o = 0; u64 acc = 0; int have = 0;
    for (unsigned i = 0; i < n; i++) {
        acc = (acc << 8) | in[i]; have += 8;
        while (have >= bits) { out[o++] = alpha[(acc >> (have - bits)) & ((1u << bits) - 1)]; have -= bits; }
    }
    if (have) out[o++] = alpha[(acc << (bits - have)) & ((1u << bits) - 1)];
    while (o % quantum) out[o++] = PADCH;
    out[o] = 0; return o;
}
// Decode: returns the number of bytes, or -1 on a bad character / bad padding.
static int dec(const char *s, const char *alpha, int bits, int quantum, u8 *out) {
    unsigned n = slen(s), data = n; int o = 0; u64 acc = 0; int have = 0;
    if (n % quantum) return -1;
    while (data && s[data - 1] == PADCH) data--;
    if (n - data > (unsigned)(quantum - 2) && n != data) return -1;
    for (unsigned i = 0; i < data; i++) {
        int v = -1; for (int k = 0; k < (1 << bits); k++) if (alpha[k] == s[i]) v = k;
        if (v < 0) return -1;
        acc = (acc << bits) | (u64)v; have += bits;
        if (have >= 8) { out[o++] = (u8)(acc >> (have - 8)); have -= 8; }
    }
    return o;
}
static unsigned enc16(const u8 *in, unsigned n, char *out) {
    for (unsigned i = 0; i < n; i++) { out[2 * i] = "0123456789ABCDEF"[in[i] >> 4]; out[2 * i + 1] = "0123456789ABCDEF"[in[i] & 15]; }
    out[2 * n] = 0; return 2 * n;
}
static int dec16(const char *s, u8 *out) {
    unsigned n = slen(s); if (n & 1) return -1;
    for (unsigned i = 0; i < n; i++) {
        char c = s[i]; int v = c >= '0' && c <= '9' ? c - '0' : c >= 'A' && c <= 'F' ? c - 'A' + 10 : -1;
        if (v < 0) return -1;
        if (i & 1) out[i / 2] |= (u8)v; else out[i / 2] = (u8)(v << 4);
    }
    return (int)(n / 2);
}
static int streq(const char *a, const char *b) { unsigned n = slen(a); if (n != slen(b)) return 0; for (unsigned i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }

int paper_main(void) {
    int ok = 1;
    // Alphabets built by rule, compared with RFC Tables 1-4 (value -> character).
    char a64[65], a64u[65], a32[33], a32h[33];
    for (int i = 0; i < 64; i++) {
        char c = i < 26 ? 'A' + i : i < 52 ? 'a' + (i - 26) : i < 62 ? '0' + (i - 52) : 0;
        a64[i] = a64u[i] = c;
    }
    a64[62] = NEG == 2 ? '-' : '+'; a64[63] = '/'; a64u[62] = '-'; a64u[63] = '_'; a64[64] = a64u[64] = 0;
    for (int i = 0; i < 32; i++) { a32[i] = i < 26 ? 'A' + i : '2' + (i - 26); a32h[i] = i < 10 ? '0' + i : 'A' + (i - 10); }
    a32[32] = a32h[32] = 0;
    int al = streq(a64, T64) + streq(a64u, T64URL) + streq(a32, T32) + streq(a32h, T32HEX);
    pstr("alphabets (RFC Tables 1-4) matching: "); pdec(al); pstr(" of 4\n"); ok &= al == 4;

    static const char *name[4] = {"base64", "base32", "base32hex", "base16"};
    int enc_ok[4] = {0, 0, 0, 0}, dec_ok[4] = {0, 0, 0, 0}, total[4] = {0, 0, 0, 0};
    for (unsigned v = 0; v < sizeof VEC / sizeof VEC[0]; v++) {
        int k = VEC[v].kind; const char *in = VEC[v].in; unsigned n = slen(in);
        char buf[64]; u8 back[32]; total[k]++;
        if (k == 0) enc((const u8 *)in, n, a64, 6, 4, buf);
        else if (k == 1) enc((const u8 *)in, n, a32, 5, 8, buf);
        else if (k == 2) enc((const u8 *)in, n, a32h, 5, 8, buf);
        else enc16((const u8 *)in, n, buf);
        int e = streq(buf, VEC[v].out);
        int m = k == 0 ? dec(VEC[v].out, a64, 6, 4, back) : k == 1 ? dec(VEC[v].out, a32, 5, 8, back)
              : k == 2 ? dec(VEC[v].out, a32h, 5, 8, back) : dec16(VEC[v].out, back);
        int d = m == (int)n; for (int i = 0; d && i < (int)n; i++) if (back[i] != (u8)in[i]) d = 0;
        enc_ok[k] += e; dec_ok[k] += d;
        if (!e || !d) { pstr("FAIL "); pstr(name[k]); pstr("(\""); pstr(in); pstr("\") -> \""); pstr(buf); pstr("\" expected \""); pstr(VEC[v].out); pstr("\"\n"); }
    }
    for (int k = 0; k < 4; k++) {
        pstr(name[k]); pstr(" vectors: encode "); pdec(enc_ok[k]); pstr("/"); pdec(total[k]);
        pstr(", decode "); pdec(dec_ok[k]); pstr("/"); pdec(total[k]); pstr("\n");
        ok &= enc_ok[k] == total[k] && dec_ok[k] == total[k] && total[k] == 7;
    }
    // Section 9 worked examples (input as hex, expected Base64 output).
    static const u8 w1[6] = {0x14,0xfb,0x9c,0x03,0xd9,0x7e};
    static const struct { int n; const char *out; } W[3] = {{6, "FPucA9l+"}, {5, "FPucA9k="}, {4, "FPucAw=="}};
    int wok = 0;
    for (int i = 0; i < 3; i++) { char buf[16]; enc(w1, (unsigned)W[i].n, a64, 6, 4, buf); wok += streq(buf, W[i].out); }
    pstr("section 9 worked examples: "); pdec(wok); pstr(" of 3\n"); ok &= wok == 3;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
