// Tiny runtime for paper checks: runs as a freestanding static aarch64 Linux ELF on ounce-core, or natively with -DHOST.
// Write the paper's algorithm in a file that includes this header and defines `int paper_main(void)`; return 0 only if every
// published value matched. Output must be deterministic (no clock, no host files). Pure integer code gives identical output on
// both; for floating point build with -ffp-contract=off and avoid libm (use fsqrt via paper_sqrt).
#ifndef OUNCE_PAPER_H
#define OUNCE_PAPER_H
typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;
typedef unsigned long long u64;
typedef long long i64;
int paper_main(void);

#ifdef HOST
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
static void paper_write(const char *s, long n) { fwrite(s, 1, (size_t)n, stdout); }
static double paper_sqrt(double x) { return __builtin_sqrt(x); }
int main(void) { int r = paper_main(); fflush(stdout); return r; }
#else
static void paper_write(const char *s, long n) {
    register long x0 __asm__("x0") = 1;
    register const char *x1 __asm__("x1") = s;
    register long x2 __asm__("x2") = n;
    register long x8 __asm__("x8") = 64;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x8) : "memory");
}
static double paper_sqrt(double x) { double r; __asm__("fsqrt %d0, %d1" : "=w"(r) : "w"(x)); return r; }
void *memset(void *d, int c, unsigned long n) { unsigned char *p = d; while (n--) *p++ = (unsigned char)c; return d; }
void *memcpy(void *d, const void *s, unsigned long n) { unsigned char *p = d; const unsigned char *q = s; while (n--) *p++ = *q++; return d; }
void *memmove(void *d, const void *s, unsigned long n) {
    unsigned char *p = d; const unsigned char *q = s;
    if (p < q) { while (n--) *p++ = *q++; } else { p += n; q += n; while (n--) *--p = *--q; }
    return d;
}
int memcmp(const void *a, const void *b, unsigned long n) {
    const unsigned char *p = a, *q = b;
    for (; n; n--, p++, q++) if (*p != *q) return *p < *q ? -1 : 1;
    return 0;
}
__attribute__((noreturn)) static void paper_exit(int code) {
    register long x0 __asm__("x0") = code;
    register long x8 __asm__("x8") = 93;
    __asm__ volatile("svc #0" : : "r"(x0), "r"(x8));
    for (;;) {}
}
void _start(void) { paper_exit(paper_main()); }
#endif

static void pstr(const char *s) { long n = 0; while (s[n]) n++; paper_write(s, n); }
static void pdec(i64 v) {
    char b[24]; int n = 0; u64 u = v < 0 ? (u64)0 - (u64)v : (u64)v;
    if (v < 0) paper_write("-", 1);
    do { b[n++] = (char)('0' + u % 10); u /= 10; } while (u);
    while (n) paper_write(&b[--n], 1);
}
static void phex(u64 v, int digits) {   // fixed width, lowercase
    for (int i = digits - 1; i >= 0; i--) { char c = "0123456789abcdef"[(v >> (4 * i)) & 15]; paper_write(&c, 1); }
}
static void phex_bytes(const u8 *p, int n) { for (int i = 0; i < n; i++) phex(p[i], 2); }
// Compare a computed byte string with a published one, print one PASS/FAIL line, return 1 on match.
static int pcheck_bytes(const char *name, const u8 *got, const u8 *want, int n) {
    int ok = 1; for (int i = 0; i < n; i++) if (got[i] != want[i]) ok = 0;
    pstr(ok ? "PASS " : "FAIL "); pstr(name); pstr(": "); phex_bytes(got, n);
    if (!ok) { pstr(" expected "); phex_bytes(want, n); }
    pstr("\n"); return ok;
}
static int pcheck_u64(const char *name, u64 got, u64 want, int digits) {
    int ok = got == want; pstr(ok ? "PASS " : "FAIL "); pstr(name); pstr(": "); phex(got, digits);
    if (!ok) { pstr(" expected "); phex(want, digits); }
    pstr("\n"); return ok;
}
#endif
