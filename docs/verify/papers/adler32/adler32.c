// Adler-32 as defined in RFC 1950 section 2.2 and 8.2 (P. Deutsch and J-L. Gailly, "ZLIB Compressed Data Format Specification
// version 3.3", 1996): s1 starts at 1, s2 at 0, for each byte s1 = (s1 + byte) mod 65521 and s2 = (s2 + s1) mod 65521, and the
// checksum is s2 * 65536 + s1. Written from that description in two forms that must agree: the plain form with a reduction after
// every byte, and the deferred form that reduces only every NMAX bytes, with NMAX computed here as the largest n for which
// 255*n*(n+1)/2 + (n+1)*(65521-1) still fits in 32 bits (this gives 5552). Check: the 43 golden values of the Go standard
// library's hash/adler32 test table (inputs of 0 to 260000 bytes, among them the runs of 0xff bytes whose lengths straddle 5552,
// the point where a deferred-reduction implementation first overflows if NMAX is wrong). The RFC itself prints no test values;
// these come from the Go project's test file and are data copied as numbers. No code was copied. Exit 0 only if both forms match
// all 43 values.
#include "ounce_paper.h"

#ifndef ADLER_BASE     // negative control hook: the modulus (RFC 1950: 65521, the largest prime below 65536)
#define ADLER_BASE 65521u
#endif

typedef struct { u32 want; const char *plain; const char *pat; int patlen; int count; const char *suffix; } vec;
static const vec V[43] = {
#include "vectors.inc"
};

static u8 buf[260100];

static u32 adler_plain(const u8 *p, unsigned long n) {
    u32 s1 = 1, s2 = 0;
    for (unsigned long i = 0; i < n; i++) { s1 = (s1 + p[i]) % ADLER_BASE; s2 = (s2 + s1) % ADLER_BASE; }
    return (s2 << 16) | s1;
}

static unsigned long nmax(void) {
    unsigned long n = 1;
    while (255ULL * (n + 1) * (n + 2) / 2 + (n + 2) * (u64)(ADLER_BASE - 1) <= 0xffffffffULL) n++;
    return n;
}
static u32 adler_deferred(const u8 *p, unsigned long n, unsigned long nm) {
    u32 s1 = 1, s2 = 0;
    while (n) {
        unsigned long k = n < nm ? n : nm;
        for (unsigned long i = 0; i < k; i++) { s1 += p[i]; s2 += s1; }
        s1 %= ADLER_BASE; s2 %= ADLER_BASE;
        p += k; n -= k;
    }
    return (s2 << 16) | s1;
}

static unsigned long slen(const char *s) { unsigned long n = 0; while (s[n]) n++; return n; }

int paper_main(void) {
    unsigned long nm = nmax();
    pstr("deferred-reduction block length NMAX derived from the 32-bit bound: "); pdec((i64)nm); pstr("\n");
    int good_plain = 0, good_def = 0, shown = 0;
    for (int i = 0; i < 43; i++) {
        const vec *v = &V[i];
        unsigned long n = 0;
        if (v->plain) { n = slen(v->plain); for (unsigned long j = 0; j < n; j++) buf[j] = (u8)v->plain[j]; }
        else {
            for (int c = 0; c < v->count; c++) for (int k = 0; k < v->patlen; k++) buf[n++] = (u8)v->pat[k];
            for (const char *q = v->suffix; *q; q++) buf[n++] = (u8)*q;
        }
        u32 a = adler_plain(buf, n), b = adler_deferred(buf, n, nm);
        good_plain += a == v->want; good_def += b == v->want;
        if ((a != v->want || b != v->want) && shown < 4) {
            shown++; pstr("FAIL vector "); pdec(i); pstr(" (length "); pdec((i64)n); pstr("): plain "); phex(a, 8); pstr(", deferred ");
            phex(b, 8); pstr(", expected "); phex(v->want, 8); pstr("\n");
        }
    }
    pstr("plain form: "); pdec(good_plain); pstr(" of 43 golden values match\n");
    pstr("deferred form: "); pdec(good_def); pstr(" of 43 golden values match\n");
    int ok = good_plain == 43 && good_def == 43;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
