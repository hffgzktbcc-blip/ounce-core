/* elf-workload bigint: arbitrary-precision unsigned integers in base 10^9 (add, multiply by a small
 * number, schoolbook multiply, decimal print). Computes factorials, Fibonacci numbers and powers
 * and checks well-known values: 25! = 15511210043330985984000000, F(100) = 354224848179261915075,
 * digit sum of 100! = 648, digit sum of 2^1000 = 1366.
 * Provenance: original work written for ounce-core (MIT); textbook algorithms. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

#define LIMBS 80          /* 80 * 9 = 720 decimal digits, enough for 2^1000 and 300! */
#define BASE 1000000000u

typedef struct { uint32_t d[LIMBS]; int n; } big;

static void big_set(big *a, uint32_t v) {
    memset(a, 0, sizeof *a);
    a->d[0] = v % BASE;
    a->d[1] = v / BASE;
    a->n = a->d[1] ? 2 : 1;
}
static void big_mul_small(big *a, uint32_t m) {
    uint64_t carry = 0;
    for (int i = 0; i < a->n; i++) {
        uint64_t t = (uint64_t)a->d[i] * m + carry;
        a->d[i] = (uint32_t)(t % BASE);
        carry = t / BASE;
    }
    while (carry) { a->d[a->n++] = (uint32_t)(carry % BASE); carry /= BASE; }
}
static void big_add(big *r, const big *a, const big *b) {
    uint32_t carry = 0;
    int n = a->n > b->n ? a->n : b->n;
    for (int i = 0; i < n; i++) {
        uint32_t t = (i < a->n ? a->d[i] : 0) + (i < b->n ? b->d[i] : 0) + carry;
        carry = t >= BASE;
        r->d[i] = carry ? t - BASE : t;
    }
    r->n = n;
    if (carry) r->d[r->n++] = 1;
    for (int i = r->n; i < LIMBS; i++) r->d[i] = 0;
}
static void big_mul(big *r, const big *a, const big *b) {
    uint64_t acc[LIMBS] = {0};
    for (int i = 0; i < a->n; i++) {
        uint64_t carry = 0;
        for (int j = 0; j < b->n && i + j < LIMBS; j++) {
            uint64_t t = acc[i + j] + (uint64_t)a->d[i] * b->d[j] + carry;
            acc[i + j] = t % BASE;
            carry = t / BASE;
        }
        if (i + b->n < LIMBS) acc[i + b->n] += carry;
    }
    int n = LIMBS;
    while (n > 1 && acc[n - 1] == 0) n--;
    for (int i = 0; i < LIMBS; i++) r->d[i] = (uint32_t)acc[i];
    r->n = n;
}
static int big_str(const big *a, char *out) {
    int len = sprintf(out, "%u", a->d[a->n - 1]);
    for (int i = a->n - 2; i >= 0; i--) len += sprintf(out + len, "%09u", a->d[i]);
    return len;
}
static int digit_sum(const char *s) {
    int t = 0;
    while (*s) t += *s++ - '0';
    return t;
}

int workload_run(FILE *o) {
    static char s[LIMBS * 9 + 2];
    int fails = 0;
    big f;
    big_set(&f, 1);
    for (uint32_t i = 2; i <= 300; i++) {
        big_mul_small(&f, i);
        if (i == 25 || i == 50 || i == 100 || i == 300) {
            int len = big_str(&f, s);
            fprintf(o, "%u! (%d digits) digitsum=%d\n", i, len, digit_sum(s));
            if (len < 70) fprintf(o, "  %s\n", s);
            if (i == 25 && strcmp(s, "15511210043330985984000000")) fails++;
            if (i == 100 && digit_sum(s) != 648) fails++;
        }
    }

    big a, b, t;
    big_set(&a, 0);
    big_set(&b, 1);
    for (int i = 1; i <= 500; i++) {
        big_add(&t, &a, &b);
        a = b;
        b = t;
        if (i == 100 || i == 200 || i == 500) {
            int len = big_str(&a, s);
            fprintf(o, "F(%d) (%d digits) digitsum=%d\n", i, len, digit_sum(s));
            if (i == 100 && strcmp(s, "354224848179261915075")) fails++;
        }
    }

    big p;
    big_set(&p, 1);
    for (int i = 0; i < 1000; i++) big_mul_small(&p, 2);
    big_str(&p, s);
    fprintf(o, "2^1000 digitsum=%d\n", digit_sum(s));
    if (digit_sum(s) != 1366) fails++;

    /* big_mul against repeated small multiplication: 3^200 * 7^100 two ways */
    big x, y, prod, ref;
    big_set(&x, 1); for (int i = 0; i < 200; i++) big_mul_small(&x, 3);
    big_set(&y, 1); for (int i = 0; i < 100; i++) big_mul_small(&y, 7);
    big_mul(&prod, &x, &y);
    big_set(&ref, 1);
    for (int i = 0; i < 200; i++) big_mul_small(&ref, 3);
    for (int i = 0; i < 100; i++) big_mul_small(&ref, 7);
    char s2[LIMBS * 9 + 2];
    int l1 = big_str(&prod, s), l2 = big_str(&ref, s2);
    fprintf(o, "3^200*7^100 (%d digits) agree=%d digitsum=%d\n", l1, l1 == l2 && !strcmp(s, s2), digit_sum(s));
    if (l1 != l2 || strcmp(s, s2)) fails++;

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
