/* elf-workload control: language-level control flow that stresses the code generator and libc
 * startup rather than any algorithm: setjmp/longjmp (nested, with values), atexit ordering at
 * exit time, variadic functions, function-pointer tables, deep recursion (Ackermann, mutual
 * recursion), struct passing and returning by value, bit-fields, switch jump tables, 64-bit
 * rotates and a large automatic array on the stack.
 * Provenance: original work written for ounce-core (MIT); no third-party source. */
#include <stdio.h>
#include <stdint.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>

static FILE *out;
static int atexit_log[4], atexit_n;
static void at1(void) { atexit_log[atexit_n++] = 1; fputs("atexit handler 1 (registered first, runs last)\n", out); }
static void at2(void) { atexit_log[atexit_n++] = 2; fputs("atexit handler 2\n", out); }
static void at3(void) { atexit_log[atexit_n++] = 3; fputs("atexit handler 3 (registered last, runs first)\n", out); fflush(out); }

static jmp_buf outer, inner;
static int depth_reached;
static void dive(int n) {
    depth_reached = n;
    if (n == 0) longjmp(inner, 42);
    dive(n - 1);
}

static long vsum(int count, ...) {
    va_list ap;
    va_start(ap, count);
    long s = 0;
    for (int i = 0; i < count; i++) s += va_arg(ap, int);
    va_end(ap);
    return s;
}
static double vavg(int count, ...) {
    va_list ap;
    va_start(ap, count);
    double s = 0;
    for (int i = 0; i < count; i++) s += va_arg(ap, double);
    va_end(ap);
    return s / count;
}

static unsigned ack(unsigned m, unsigned n) { return m == 0 ? n + 1 : n == 0 ? ack(m - 1, 1) : ack(m - 1, ack(m, n - 1)); }
static int is_odd(int n);
static int is_even(int n) { return n == 0 ? 1 : is_odd(n - 1); }
static int is_odd(int n) { return n == 0 ? 0 : is_even(n - 1); }

typedef struct { int32_t a; double b; char tag[12]; int64_t c; } rec;
static rec make_rec(int i) { rec r; memset(&r, 0, sizeof r); r.a = i; r.b = i * 0.5; snprintf(r.tag, sizeof r.tag, "rec-%d", i); r.c = (int64_t)i << 40; return r; }
static rec bump(rec r) { r.a += 100; r.b *= 2; r.c += 1; return r; }

struct flags { unsigned a : 3, b : 5, c : 1; int d : 7; unsigned e : 16; };

static int classify(int x) {
    switch (x) {
    case 0: return 10; case 1: return 11; case 2: return 12; case 3: return 13; case 4: return 14; case 5: return 15;
    case 6: return 16; case 7: return 17; case 100: return 200; case 101: return 201; case -1: return -10;
    default: return x < 0 ? -99 : 99;
    }
}
static int add(int a, int b) { return a + b; }
static int sub(int a, int b) { return a - b; }
static int mul(int a, int b) { return a * b; }

static uint64_t rotl64(uint64_t x, unsigned k) { return (x << (k & 63)) | (x >> ((64 - k) & 63)); }

int workload_run(FILE *o) {
    out = o;
    int fails = 0;

    atexit(at1);
    atexit(at2);
    atexit(at3);

    switch (setjmp(inner)) { /* setjmp is only defined in a controlling expression */
    case 0:
        fprintf(o, "setjmp first return\n");
        dive(200);
        fails++; /* dive() must longjmp and never return */
        break;
    case 42:
        fprintf(o, "longjmp delivered %d after reaching depth %d\n", 42, depth_reached);
        if (depth_reached != 0) fails++;
        break;
    default:
        fprintf(o, "longjmp delivered an unexpected value\n");
        fails++;
        break;
    }
    volatile int hops = 0, count = 0;
    if (setjmp(outer) < 3) {
        count++;
        hops = count;
        longjmp(outer, count + 0);     /* values 1, 2, then 3 ends the loop */
    }
    fprintf(o, "longjmp loop ran %d times (last hop %d)\n", count, hops);
    if (count != 3) fails++;

    fprintf(o, "vsum=%ld vavg=%.3f\n", vsum(5, 1, 2, 3, 4, 5), vavg(4, 1.0, 2.0, 4.0, 8.5));
    char tmp[64];
    snprintf(tmp, sizeof tmp, "%d-%s-%c-%.1f", 7, "x", 'y', 2.5);
    fprintf(o, "snprintf varargs=%s\n", tmp);

    fprintf(o, "ack(2,3)=%u ack(3,5)=%u even(1000)=%d odd(777)=%d\n", ack(2, 3), ack(3, 5), is_even(1000), is_odd(777));
    if (ack(2, 3) != 9 || ack(3, 5) != 253) fails++;

    int (*ops[])(int, int) = {add, sub, mul};
    int acc = 7;
    for (int i = 0; i < 9; i++) acc = ops[i % 3](acc, i + 2) & 0xFFFF;
    fprintf(o, "function-pointer table result=%d\n", acc);

    rec r = bump(bump(make_rec(5)));
    fprintf(o, "struct by value: a=%d b=%.1f tag=%s c=%lld\n", r.a, r.b, r.tag, (long long)r.c);
    if (r.a != 205 || r.b != 10.0 || r.c != ((int64_t)5 << 40) + 2) fails++;

    struct flags f = {5, 17, 1, -3, 65000};
    f.b += 20;
    f.d--;
    fprintf(o, "bitfields: a=%u b=%u c=%u d=%d e=%u sizeof=%zu\n", f.a, f.b, f.c, f.d, f.e, sizeof f);

    int sw = 0;
    for (int x = -3; x < 104; x++) sw += classify(x);
    fprintf(o, "switch sum=%d\n", sw);

    uint64_t rr = 0x0123456789ABCDEFull;
    for (unsigned k = 0; k < 130; k += 13) rr = rotl64(rr, k) ^ (rr >> 3);
    fprintf(o, "rotl chain=%016llx\n", (unsigned long long)rr);

    volatile uint8_t big[48 * 1024];       /* large frame */
    for (unsigned i = 0; i < sizeof big; i++) big[i] = (uint8_t)(i * 31);
    unsigned bs = 0;
    for (unsigned i = 0; i < sizeof big; i += 17) bs += big[i];
    fprintf(o, "stack array sum=%u\n", bs);

    fprintf(o, "fails=%d (atexit handlers run after this line, output goes through the same stream)\n", fails);
    fflush(o);
    if (fails) return fails;
    exit(0);   /* runs the atexit handlers; exit status 0 is the success path */
}
