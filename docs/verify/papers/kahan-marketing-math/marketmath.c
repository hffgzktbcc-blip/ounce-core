// Reproduction of numerical statements in W. Kahan, "Marketing versus Mathematics" (lecture notes, 27 August 2000):
//  (1) the literal 25.4 rounded to binary with 24, 53, 64 and 113 significant bits is 25.399999619..., 25.39999999999999858...,
//      25.399999999999999999653... and 25.3999999999999999999999999999999877...;
//  (2) displayed to 17 significant decimals, "0.8" is 0.80000000000000004, "0.0009875" is 0.00098750000000000001 and 32200/32.2 is
//      999.99999999999989;
//  (3) 1024 - 4/2^43 = 1023.9999 9999 9999 5453... and 1024 + 4/2^43 = 1024.0000 0000 0000 4547...: seven consecutive doubles lie
//      between them and 16 significant decimals do not distinguish them (17 do).
// Written for ounce-core from the statements in the paper; the exact decimal expansions use a small big-number type written here
// (round to nearest even, then digits of N*5^s), the double and float cases are also compared with the hardware conversion of the
// same literal. No third-party code.
// Negative control: -DNEG_BITS converts the double with a 52-bit instead of a 53-bit significand.
#include "ounce_paper.h"
#ifndef HOST
// the compiler calls these for 128-bit division; the freestanding build has no compiler runtime library
static unsigned __int128 udivmod128(unsigned __int128 n, unsigned __int128 d, unsigned __int128 *rem) {
    unsigned __int128 q = 0, r = 0;
    for (int i = 127; i >= 0; i--) { r = (r << 1) | ((n >> i) & 1); if (r >= d) { r -= d; q |= (unsigned __int128)1 << i; } }
    if (rem) *rem = r; return q;
}
unsigned __int128 __udivti3(unsigned __int128 n, unsigned __int128 d) { return udivmod128(n, d, 0); }
unsigned __int128 __umodti3(unsigned __int128 n, unsigned __int128 d) { unsigned __int128 r; udivmod128(n, d, &r); return r; }
#endif
// ---- exact decimal expansion of a double (no libm, no printf) ----
// digits of |x| (finite, 2^-100 < |x| < 2^52): value = d[0].d[1]d[2]... * 10^e10; *ru = 1 when the discarded remainder is >= half
static int dbl_digits(double x, int nd, char *d, int *e10, int *ru) {
    union { double f; u64 u; } v; v.f = x;
    int ef = (int)((v.u >> 52) & 2047); u64 m = v.u & 0xFFFFFFFFFFFFFull; int e = ef - 1075;
    if (ef) m |= 1ull << 52; else e = -1074;
    if (m == 0) { for (int i = 0; i < nd; i++) d[i] = 0; *e10 = 0; *ru = 0; return 0; }
    int k = e < 0 ? -e : 0; unsigned __int128 num = (unsigned __int128)m << (e > 0 ? e : 0);
    unsigned __int128 ip = k ? (num >> k) : num;
    unsigned __int128 rem = k ? (num & (((unsigned __int128)1 << k) - 1)) : 0;
    int n = 0, ex = 0; char tmp[48]; int nt = 0;
    if (ip) { while (ip) { tmp[nt++] = (char)(ip % 10); ip /= 10; } ex = nt - 1; for (int i = nt - 1; i >= 0 && n < nd; i--) d[n++] = tmp[i]; }
    else { ex = -1; for (;;) { rem *= 10; int dg = (int)(rem >> k); rem &= (((unsigned __int128)1 << k) - 1); if (dg) { d[n++] = (char)dg; break; } ex--; } }
    while (n < nd) { rem *= 10; d[n++] = (char)(rem >> k); rem &= (((unsigned __int128)1 << k) - 1); }
    // remainder after the digits taken (only exact if the integer part had <= nd digits, which holds in our uses)
    unsigned __int128 half = k ? ((unsigned __int128)1 << (k - 1)) : 0;
    *ru = k ? rem >= half && (rem > half || 1) : 0;   // ties round up
    *e10 = ex; return 1;
}
static void digits_round(char *d, int nd, int *e10) {
    int i = nd - 1;
    while (i >= 0) { if (d[i] == 9) { d[i] = 0; i--; } else { d[i]++; break; } }
    if (i < 0) { d[0] = 1; (*e10)++; }
}
// plain positional formatting of digits with value d0.d1d2.. * 10^e10, for -5 <= e10 <= 15
static void dig_fmt(char *out, int neg, const char *d, int nd, int e10) {
    int n = 0; if (neg) out[n++] = '-';
    if (e10 < 0) { out[n++] = '0'; out[n++] = '.'; for (int i = -1; i > e10; i--) out[n++] = '0'; for (int i = 0; i < nd; i++) out[n++] = (char)('0' + d[i]); }
    else { for (int i = 0; i < nd; i++) { if (i == e10 + 1) out[n++] = '.'; out[n++] = (char)('0' + d[i]); } for (int i = nd; i <= e10; i++) out[n++] = '0'; }
    out[n] = 0;
}
// x printed with nd significant digits, rounded (mode 0) or truncated (mode 1)
static void dbl_str(double x, int nd, int mode, char *out) {
    char d[32]; int e10, ru; int neg = x < 0; if (neg) x = -x;
    if (x == 0) { out[0] = '0'; out[1] = 0; return; }
    dbl_digits(x, nd, d, &e10, &ru);
    if (mode == 0 && ru) digits_round(d, nd, &e10);
    dig_fmt(out, neg, d, nd, e10);
}
static int streq(const char *a, const char *b) { while (*a && *a == *b) { a++; b++; } return *a == *b; }

#define L 16
typedef struct { u32 w[L]; } Big;
static void b_set(Big *a, u64 v) { for (int i = 0; i < L; i++) a->w[i] = 0; a->w[0] = (u32)v; a->w[1] = (u32)(v >> 32); }
static int b_zero(const Big *a) { for (int i = 0; i < L; i++) if (a->w[i]) return 0; return 1; }
static void b_mul(Big *a, u32 m) { u64 c = 0; for (int i = 0; i < L; i++) { u64 t = (u64)a->w[i] * m + c; a->w[i] = (u32)t; c = t >> 32; } }
static void b_shl1(Big *a) { u32 c = 0; for (int i = 0; i < L; i++) { u32 n = (a->w[i] << 1) | c; c = a->w[i] >> 31; a->w[i] = n; } }
static u32 b_divsmall(Big *a, u32 d) { u64 r = 0; for (int i = L - 1; i >= 0; i--) { u64 cur = (r << 32) | a->w[i]; a->w[i] = (u32)(cur / d); r = cur % d; } return (u32)r; }
static void b_inc(Big *a) { for (int i = 0; i < L; i++) { if (++a->w[i]) break; } }
static int b_dec(const Big *a, char *out) {
    Big t = *a; char tmp[200]; int n = 0;
    if (b_zero(&t)) { out[0] = '0'; out[1] = 0; return 1; }
    while (!b_zero(&t)) tmp[n++] = (char)('0' + b_divsmall(&t, 10));
    for (int i = 0; i < n; i++) out[i] = tmp[n - 1 - i];
    out[n] = 0; return n;
}
// 25.4 rounded to p bits: N / 2^s with 2^(p-1) <= N < 2^p. Returns the decimal expansion of N/2^s in out (exact), and N in *Nout (low 64 bits)
static void round254(int p, char *out, u64 *Nlow) {
    int s = p - 5;   // 25.4 is in [16, 32)
    Big T; b_set(&T, 254); for (int i = 0; i < s; i++) b_shl1(&T);
    u32 r = b_divsmall(&T, 10);   // T = floor(254 * 2^s / 10), remainder r (of 10)
#ifdef NEG_TRUNC
    (void)r;
#else
    if (2 * r > 10 || (2 * r == 10 && (T.w[0] & 1))) b_inc(&T);
#endif
    *Nlow = (u64)T.w[0] | ((u64)T.w[1] << 32);
    Big M = T; for (int i = 0; i < s; i++) b_mul(&M, 5);   // N * 5^s ; value = that / 10^s
    char digits[200]; int len = b_dec(&M, digits);
    int ip = len - s; int n = 0;
    for (int i = 0; i < ip; i++) out[n++] = digits[i];
    out[n++] = '.'; for (int i = ip; i < len; i++) out[n++] = digits[i];
    out[n] = 0;
}
static int prefix_of(const char *pub, const char *exact) { int i = 0; while (pub[i]) { if (pub[i] != exact[i]) return 0; i++; } return 1; }
// the published decimal text equals the exact expansion cut after the same number of digits, truncated or rounded to nearest
static int trunc_or_round(const char *pub, const char *exact) {
    if (prefix_of(pub, exact)) return 1;
    int n = 0; while (pub[n]) n++;
    char t[300]; for (int i = 0; i < n; i++) t[i] = exact[i]; t[n] = 0;
    if (exact[n] == '.') return 0;
    int round_up = exact[n] >= '5';
    if (!round_up) return 0;
    int i = n - 1;   // add one in the last place, skipping the point
    while (i >= 0) { if (t[i] == '.') { i--; continue; } if (t[i] == '9') { t[i] = '0'; i--; } else { t[i]++; break; } }
    if (i < 0) return 0;
    return prefix_of(pub, t) && prefix_of(t, pub);
}

static int bad = 0;
static void verdict(const char *what, int ok) { pstr(ok ? "  ok: " : "  FAIL: "); pstr(what); pstr("\n"); if (!ok) bad++; }

int paper_main(void) {
    pstr("(1) the literal 25.4 in four binary formats\n");
#ifdef NEG_BITS
    static const int bitsv[4] = {24, 52, 64, 113};   // negative control: 52 instead of 53 bits for the double
#else
    static const int bitsv[4] = {24, 53, 64, 113};
#endif
    static const char *pub[4] = {"25.399999619", "25.39999999999999858", "25.399999999999999999653", "25.3999999999999999999999999999999877"};
    static const char *name[4] = {"4-byte float", "8-byte double", "10-byte long double (64-bit significand)", "16-byte quadruple (113-bit significand)"};
    u64 N[4];
    for (int i = 0; i < 4; i++) {
        char ex[260]; round254(bitsv[i], ex, &N[i]);
        pstr("  "); pstr(name[i]); pstr(": "); pstr(ex); pstr("\n");
        char what[160]; int k = 0; const char *a = "paper's printed digits equal the exact expansion, truncated or rounded, for "; while (a[k]) { what[k] = a[k]; k++; } for (int j = 0; name[i][j] && k < 150; j++) what[k++] = name[i][j]; what[k] = 0;
        if (i < 3) verdict(what, trunc_or_round(pub[i], ex));
        else {   // the paper's quadruple string 25.3999999999999999999999999999999877 has 30 nines after the 3; the exact value has 31
            char fixed[64]; int n = 0; for (int j = 0; pub[i][j]; j++) { if (j == 34) fixed[n++] = '9'; fixed[n++] = pub[i][j]; } fixed[n] = 0;
            verdict("documented discrepancy: the paper's quadruple string lacks one 9; with a 31st nine it equals the exact expansion rounded, as printed it does not",
                    !trunc_or_round(pub[i], ex) && trunc_or_round(fixed, ex));
        }
    }
    { union { float f; u32 u; } v; v.f = 25.4f; verdict("the 24-bit significand equals the hardware float 25.4f", (u64)(v.u & 0x7FFFFFu) + 0x800000u == N[0]);
      union { double d; u64 u; } w; w.d = 25.4; verdict("the 53-bit significand equals the hardware double 25.4", (w.u & 0xFFFFFFFFFFFFFull) + (1ull << 52) == N[1]); }

    pstr("(2) 17-significant-decimal displays\n");
    { volatile double a = 0.8, b = 0.0009875, c = 32200.0, d = 32.2; double q = c / d;
      char s1[48], s2[48], s3[48], s2b[48]; dbl_str(a, 17, 0, s1); dbl_str(b, 17, 0, s2); dbl_str(q, 17, 0, s3); dbl_str(b, 16, 0, s2b);
      pstr("  0.8 -> "); pstr(s1); pstr("\n  0.0009875 -> "); pstr(s2); pstr(" (17 digits), "); pstr(s2b); pstr(" (16 digits)\n  32200/32.2 -> "); pstr(s3); pstr("\n");
      verdict("\"0.8\" displays as 0.80000000000000004", streq(s1, "0.80000000000000004"));
      // the paper prints 0.00098750000000000001 "to 16 sig. dec."; that string has 17 significant digits ending ...0001, but the exact value
      // is 0.0009875000000000000964..., whose 16-digit display is 0.0009875000000000001 (one zero fewer) and whose 17-digit display ends ...0010
      verdict("\"0.0009875\" displays as 0.0009875000000000001 with 16 digits (the paper's string has one zero too many; documented discrepancy)", streq(s2b, "0.0009875000000000001") && !streq(s2, "0.00098750000000000001"));
      verdict("\"32200/32.2\" displays as 999.99999999999989", streq(s3, "999.99999999999989")); }

    pstr("(3) the doubles around 1024\n");
    { union { double d; u64 u; } v; double vals[7]; v.d = 1024.0;
      u64 base = v.u; for (int k = -4; k <= 2; k++) { v.u = base + (u64)(k); vals[k + 4] = v.d; }
      // base-4 .. base-1 are 1024 - 4*2^-43 ... 1024 - 2^-43 (ulp below 1024 is 2^-43); base+1, base+2 are 1024 + 2^-42, 1024 + 2*2^-42
      char lo[48], hi[48]; dbl_str(vals[0], 22, 0, lo); dbl_str(vals[6], 22, 0, hi);
      pstr("  lowest "); pstr(lo); pstr("\n  highest "); pstr(hi); pstr("\n");
      verdict("lowest of the seven is 1024 - 4/2^43 = 1023.9999 9999 9999 5453... (paper)", prefix_of("1023.99999999999954525", lo));
      verdict("highest of the seven is 1024 + 4/2^43 = 1024.0000 0000 0000 4547... (paper)", prefix_of("1024.0000000000004547", hi));
      int same16 = 1, distinct17 = 1; char first[48]; dbl_str(vals[0], 16, 0, first);
      for (int i = 0; i < 7; i++) { char s[48]; dbl_str(vals[i], 16, 0, s); if (!streq(s, first)) same16 = 0; for (int j = i + 1; j < 7; j++) { char t[48], u[48]; dbl_str(vals[i], 17, 0, t); dbl_str(vals[j], 17, 0, u); if (streq(t, u)) distinct17 = 0; } }
      pstr("  all seven displayed with 16 significant decimals: "); pstr(first); pstr("\n");
      verdict("16 significant decimals do not distinguish the seven consecutive doubles", same16);
      verdict("17 significant decimals distinguish them", distinct17);
      char sa[48], sb[48]; { union { double d; u64 u; } w; w.u = base - 5; dbl_str(w.d, 16, 0, sa); w.u = base + 3; dbl_str(w.d, 16, 0, sb); }
      verdict("the neighbours on either side do display differently at 16 decimals (exactly seven)", !streq(sa, first) && !streq(sb, first)); }
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
