// CBOR encoding from RFC 8949 (Bormann and Hoffman, "Concise Binary Object Representation (CBOR)", 2020), written from the
// RFC's description of the initial byte, argument encoding, preferred float serialization and indefinite-length items.
// Check: the 81 examples of Appendix A (Table 6) are built here from their diagnostic notation, with the program's own
// encoder, and compared byte for byte with the table's encoded column (examples.inc is generated from the RFC text); each
// published encoding is also walked by a well-formedness decoder that must consume exactly its bytes, and the float
// examples are decoded back to the value in diagnostic notation. Exit 0 only if everything matches.
#include "ounce_paper.h"
#include "examples.inc"

#ifndef NEG
#define NEG 0   // NEG=1: negative integers are encoded as -n instead of -1-n; NEG=2: floats are never shortened
#endif

static u8 buf[256]; static int blen;
static void put(u8 b) { buf[blen++] = b; }
// Initial byte: major type in the top 3 bits, argument in the low 5 bits or in 1, 2, 4 or 8 following bytes (shortest form).
static void head(int major, u64 v) {
    int m = major << 5;
    if (v < 24) put((u8)(m | v));
    else if (v < 0x100) { put((u8)(m | 24)); put((u8)v); }
    else if (v < 0x10000) { put((u8)(m | 25)); put((u8)(v >> 8)); put((u8)v); }
    else if (v < 0x100000000ULL) { put((u8)(m | 26)); for (int s = 24; s >= 0; s -= 8) put((u8)(v >> s)); }
    else { put((u8)(m | 27)); for (int s = 56; s >= 0; s -= 8) put((u8)(v >> s)); }
}
static void UINT(u64 v) { head(0, v); }
static void NINT(u64 n) { head(1, n); }                                  // the value -1-n
static void INT(i64 v) { if (v >= 0) UINT((u64)v); else NINT(NEG == 1 ? (u64)(-v) : (u64)(-1 - v)); }
static void BYTES(const u8 *p, int n) { head(2, (u64)n); for (int i = 0; i < n; i++) put(p[i]); }
static void TAG(u64 t) { head(6, t); }
static void ARR(int n) { head(4, (u64)n); }
static void MAP(int n) { head(5, (u64)n); }
static void SIMPLE(int v) { if (v < 24) put((u8)(0xe0 | v)); else { put(0xf8); put((u8)v); } }
static void BRK(void) { put(0xff); }
static void IBYTES(void) { put(0x5f); } static void ITEXT(void) { put(0x7f); } static void IARR(void) { put(0x9f); } static void IMAP(void) { put(0xbf); }
static u32 utf8(u32 cp, u8 *o) {                                          // code point -> 1 to 4 bytes
    if (cp < 0x80) { o[0] = (u8)cp; return 1; }
    if (cp < 0x800) { o[0] = (u8)(0xc0 | cp >> 6); o[1] = (u8)(0x80 | (cp & 63)); return 2; }
    if (cp < 0x10000) { o[0] = (u8)(0xe0 | cp >> 12); o[1] = (u8)(0x80 | (cp >> 6 & 63)); o[2] = (u8)(0x80 | (cp & 63)); return 3; }
    o[0] = (u8)(0xf0 | cp >> 18); o[1] = (u8)(0x80 | (cp >> 12 & 63)); o[2] = (u8)(0x80 | (cp >> 6 & 63)); o[3] = (u8)(0x80 | (cp & 63)); return 4;
}
static void TEXT(const char *s) { int n = 0; while (s[n]) n++; head(3, (u64)n); for (int i = 0; i < n; i++) put((u8)s[i]); }
static void TEXTCP(u32 cp) { u8 t[4]; u32 n = utf8(cp, t); head(3, n); for (u32 i = 0; i < n; i++) put(t[i]); }

static u64 dbits(double d) { u64 u; __builtin_memcpy(&u, &d, 8); return u; }
static double bitsd(u64 u) { double d; __builtin_memcpy(&d, &u, 8); return d; }
static u32 fbits(float f) { u32 u; __builtin_memcpy(&u, &f, 4); return u; }
static int isnan_(double d) { return d != d; }
// Is d exactly a binary16 value? (RFC 8949 section 4.1 / Appendix D; zero, infinities, normals and subnormals.)
static int to_half(double d, u16 *out) {
    u16 s = (u16)((dbits(d) >> 63) << 15); double a = d < 0 ? -d : d;
    if (a == 0) { *out = s; return 1; }
    if (a == bitsd(0x7ff0000000000000ULL)) { *out = (u16)(s | 0x7c00); return 1; }
    if (a >= 65520.0) return 0;
    if (a >= 6.103515625e-05) {                        // normal: 2^e <= a < 2^(e+1), e in -14..15
        int e = 0; double p = 1.0;
        while (p * 2 <= a) { p *= 2; e++; }
        while (p > a) { p /= 2; e--; }
        double t = (a / p) * 1024.0 - 1024.0;           // 10-bit significand
        u32 m = (u32)t; if ((double)m != t) return 0;
        *out = (u16)(s | (u16)((e + 15) << 10) | (u16)m); return 1;
    }
    double t = a * 16777216.0;                         // subnormal: multiple of 2^-24
    u32 m = (u32)t; if ((double)m != t || m == 0 || m > 1023) return 0;
    *out = (u16)(s | (u16)m); return 1;
}
static void F16(u16 h) { put(0xf9); put((u8)(h >> 8)); put((u8)h); }
static void F32(double d) { float f = (float)d; u32 u = fbits(f); put(0xfa); for (int s = 24; s >= 0; s -= 8) put((u8)(u >> s)); }
static void F64(double d) { u64 u = dbits(d); put(0xfb); for (int s = 56; s >= 0; s -= 8) put((u8)(u >> s)); }
// Preferred serialization: the shortest of half, single, double that keeps the value exactly (NaN as the half 7e00).
static void FP(double d) {
    u16 h;
    if (isnan_(d)) { F16(0x7e00); return; }
    if (NEG != 2 && to_half(d, &h)) { F16(h); return; }
    if (NEG != 2 && (double)(float)d == d) { F32(d); return; }
    F64(d);
}
static double half_to_double(u16 h) {
    int e = (h >> 10) & 31; double m = h & 1023, v;
    if (e == 0) v = m * 5.9604644775390625e-08;        // m * 2^-24
    else if (e == 31) v = m == 0 ? bitsd(0x7ff0000000000000ULL) : bitsd(0x7ff8000000000000ULL);
    else { v = 1.0 + m / 1024.0; int x = e - 15; while (x > 0) { v *= 2; x--; } while (x < 0) { v /= 2; x++; } }
    return (h & 0x8000) ? -v : v;
}

static int hexv(char c) { return c <= '9' ? c - '0' : c - 'a' + 10; }
static int unhex(const char *s, u8 *o) { int n = 0; while (s[2 * n]) { o[n] = (u8)(hexv(s[2 * n]) * 16 + hexv(s[2 * n + 1])); n++; } return n; }
static u64 arg(const u8 *p, int ai, int *len) {            // argument value of an initial byte; *len = extra bytes
    if (ai < 24) { *len = 0; return (u64)ai; }
    int n = ai == 24 ? 1 : ai == 25 ? 2 : ai == 26 ? 4 : 8; *len = n; u64 v = 0; for (int i = 0; i < n; i++) v = (v << 8) | p[1 + i]; return v;
}
// Well-formedness walk: returns bytes consumed by one data item at p (end = limit) or -1.
static int skip(const u8 *p, int avail) {
    if (avail < 1) return -1;
    int mt = p[0] >> 5, ai = p[0] & 31, len, pos = 1;
    if (mt == 7 && ai >= 28 && ai != 31) return -1;
    if (ai == 31) {                                         // indefinite length
        if (mt == 0 || mt == 1 || mt == 6) return -1;
        if (mt == 7) return -1;                             // a lone break is not an item
        for (;;) {
            if (pos >= avail) return -1;
            if (p[pos] == 0xff) return pos + 1;
            if (mt == 2 || mt == 3) { if ((p[pos] >> 5) != mt || (p[pos] & 31) == 31) return -1; }
            int k = skip(p + pos, avail - pos); if (k < 0) return -1; pos += k;
            if (mt == 5) { k = skip(p + pos, avail - pos); if (k < 0) return -1; pos += k; }
        }
    }
    if (ai > 27) return -1;
    u64 v = arg(p, ai, &len); pos = 1 + len; if (pos > avail) return -1;
    if (mt == 2 || mt == 3) { if ((u64)avail - (u64)pos < v) return -1; return pos + (int)v; }
    if (mt == 4 || mt == 5) { u64 cnt = mt == 5 ? 2 * v : v; for (u64 i = 0; i < cnt; i++) { int k = skip(p + pos, avail - pos); if (k < 0) return -1; pos += k; } return pos; }
    if (mt == 6) { int k = skip(p + pos, avail - pos); return k < 0 ? -1 : pos + k; }
    return pos;                                              // 0, 1, 7: no payload beyond the argument
}

static int row, rows_ok, wf_ok, fl_checked, fl_ok;
static void done(void) {
    u8 want[128]; int n = unhex(EX[row].hex, want);
    int ok = n == blen; for (int i = 0; ok && i < n; i++) if (buf[i] != want[i]) ok = 0;
    if (ok) rows_ok++;
    else { pstr("FAIL "); pdec(row); pstr(" "); pstr(EX[row].diag); pstr(": got "); phex_bytes(buf, blen); pstr(" expected "); pstr(EX[row].hex); pstr("\n"); }
    wf_ok += skip(want, n) == n;
    row++; blen = 0;
}
// Float examples: encode (preferred serialization, or an explicit width) and also decode the published bytes back to the value.
static int dec_ok(double d) {
    u8 w[16]; int n = unhex(EX[row].hex, w); double got = 0; int ok = 0;
    if (w[0] == 0xf9 && n == 3) { got = half_to_double((u16)(w[1] << 8 | w[2])); ok = 1; }
    if (w[0] == 0xfa && n == 5) {
        u32 u = (u32)w[1] << 24 | (u32)w[2] << 16 | (u32)w[3] << 8 | w[4]; u64 m = u & 0x7fffff; int e = (u >> 23) & 255; double v;
        if (e == 255) v = m ? bitsd(0x7ff8000000000000ULL) : bitsd(0x7ff0000000000000ULL);
        else { v = e ? 1.0 + (double)m / 8388608.0 : (double)m / 8388608.0; int x = (e ? e : 1) - 127; while (x > 0) { v *= 2; x--; } while (x < 0) { v /= 2; x++; } }
        got = (u >> 31) ? -v : v; ok = 1; }
    if (w[0] == 0xfb && n == 9) { u64 u = 0; for (int i = 0; i < 8; i++) u = u << 8 | w[1 + i]; got = bitsd(u); ok = 1; }
    return ok && (isnan_(d) ? isnan_(got) : dbits(got) == dbits(d));
}
static void flt(double d) { FP(d); fl_checked++; fl_ok += dec_ok(d); done(); }
static void flt32(double d) { F32(d); fl_checked++; fl_ok += dec_ok(d); done(); }
static void flt64(double d) { F64(d); fl_checked++; fl_ok += dec_ok(d); done(); }

int paper_main(void) {
    const double INF = bitsd(0x7ff0000000000000ULL), NAN_ = bitsd(0x7ff8000000000000ULL);
    static const u8 big[9] = {1, 0, 0, 0, 0, 0, 0, 0, 0};
    // integers
    UINT(0); done(); UINT(1); done(); UINT(10); done(); UINT(23); done(); UINT(24); done(); UINT(25); done(); UINT(100); done(); UINT(1000); done();
    UINT(1000000); done(); UINT(1000000000000ULL); done(); UINT(18446744073709551615ULL); done();
    TAG(2); BYTES(big, 9); done();                                  // 18446744073709551616 = 2^64 as a bignum
    NINT(18446744073709551615ULL); done();                          // -18446744073709551616
    TAG(3); BYTES(big, 9); done();                                  // -18446744073709551617 = -1 - 2^64
    INT(-1); done(); INT(-10); done(); INT(-100); done(); INT(-1000); done();
    // floats
    flt(0.0); flt(-0.0); flt(1.0); flt(1.1); flt(1.5); flt(65504.0); flt(100000.0); flt(3.4028234663852886e+38); flt(1.0e+300);
    flt(5.960464477539063e-8); flt(0.00006103515625); flt(-4.0); flt(-4.1);
    flt(INF); flt(NAN_); flt(-INF);
    flt32(INF); flt32(NAN_); flt32(-INF); flt64(INF); flt64(NAN_); flt64(-INF);
    // simple values (RFC Table 4: false=20, true=21, null=22, undefined=23)
    SIMPLE(20); done(); SIMPLE(21); done(); SIMPLE(22); done(); SIMPLE(23); done(); SIMPLE(16); done(); SIMPLE(255); done();
    // tags
    TAG(0); TEXT("2013-03-21T20:04:00Z"); done();
    TAG(1); UINT(1363896240); done();
    TAG(1); FP(1363896240.5); done();
    { static const u8 a[4] = {1, 2, 3, 4}; TAG(23); BYTES(a, 4); done(); }
    { static const u8 a[5] = {0x64, 0x49, 0x45, 0x54, 0x46}; TAG(24); BYTES(a, 5); done(); }
    TAG(32); TEXT("http://www.example.com"); done();
    // byte and text strings
    BYTES(0, 0); done();
    { static const u8 a[4] = {1, 2, 3, 4}; BYTES(a, 4); done(); }
    TEXT(""); done(); TEXT("a"); done(); TEXT("IETF"); done(); TEXT("\"\\"); done();
    TEXTCP(0x00fc); done(); TEXTCP(0x6c34); done(); TEXTCP(0x10151); done();
    // arrays and maps
    ARR(0); done();
    ARR(3); UINT(1); UINT(2); UINT(3); done();
    ARR(3); UINT(1); ARR(2); UINT(2); UINT(3); ARR(2); UINT(4); UINT(5); done();
    ARR(25); for (int i = 1; i <= 25; i++) UINT((u64)i); done();
    MAP(0); done();
    MAP(2); UINT(1); UINT(2); UINT(3); UINT(4); done();
    MAP(2); TEXT("a"); UINT(1); TEXT("b"); ARR(2); UINT(2); UINT(3); done();
    ARR(2); TEXT("a"); MAP(1); TEXT("b"); TEXT("c"); done();
    MAP(5); TEXT("a"); TEXT("A"); TEXT("b"); TEXT("B"); TEXT("c"); TEXT("C"); TEXT("d"); TEXT("D"); TEXT("e"); TEXT("E"); done();
    // indefinite length
    { static const u8 a[2] = {1, 2}, b[3] = {3, 4, 5}; IBYTES(); BYTES(a, 2); BYTES(b, 3); BRK(); done(); }
    ITEXT(); TEXT("strea"); TEXT("ming"); BRK(); done();
    IARR(); BRK(); done();
    IARR(); UINT(1); ARR(2); UINT(2); UINT(3); IARR(); UINT(4); UINT(5); BRK(); BRK(); done();
    IARR(); UINT(1); ARR(2); UINT(2); UINT(3); ARR(2); UINT(4); UINT(5); BRK(); done();
    ARR(3); UINT(1); ARR(2); UINT(2); UINT(3); IARR(); UINT(4); UINT(5); BRK(); done();
    ARR(3); UINT(1); IARR(); UINT(2); UINT(3); BRK(); ARR(2); UINT(4); UINT(5); done();
    IARR(); for (int i = 1; i <= 25; i++) UINT((u64)i); BRK(); done();
    IMAP(); TEXT("a"); UINT(1); TEXT("b"); IARR(); UINT(2); UINT(3); BRK(); BRK(); done();
    ARR(2); TEXT("a"); IMAP(); TEXT("b"); TEXT("c"); BRK(); done();
    IMAP(); TEXT("Fun"); SIMPLE(21); TEXT("Amt"); INT(-2); BRK(); done();

    int total = (int)(sizeof EX / sizeof EX[0]);
    int ok = row == total && rows_ok == total && wf_ok == total && fl_ok == fl_checked;
    pstr("Appendix A examples built and matching: "); pdec(rows_ok); pstr(" of "); pdec(total); pstr("\n");
    pstr("published encodings that decode as exactly one well-formed item: "); pdec(wf_ok); pstr(" of "); pdec(total); pstr("\n");
    pstr("float examples whose published bytes decode to the diagnostic value: "); pdec(fl_ok); pstr(" of "); pdec(fl_checked); pstr("\n");
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
