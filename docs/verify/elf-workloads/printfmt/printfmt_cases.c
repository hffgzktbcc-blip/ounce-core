/* elf-workload printfmt: printf/snprintf/sprintf integer, string, character and exactly
 * representable floating-point conversions with flags, widths and precisions.
 * Only conversions whose output is fixed by the C standard are used, so the host libc and musl
 * must agree byte for byte.
 * Provenance: original work written for ounce-core (MIT); no third-party source. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <stdarg.h>

static int fails;

static void expect(FILE *o, const char *want, const char *fmt, ...) {
    char buf[256];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    int ok = !strcmp(buf, want);
    if (!ok) fails++;
    fprintf(o, "[%s] %s\n", buf, ok ? "ok" : "MISMATCH");
}

int workload_run(FILE *o) {
    expect(o, "42", "%d", 42);
    expect(o, "-42", "%d", -42);
    expect(o, "   42", "%5d", 42);
    expect(o, "42   |", "%-5d|", 42);
    expect(o, "00042", "%05d", 42);
    expect(o, "+42", "%+d", 42);
    expect(o, " 42", "% d", 42);
    expect(o, "  042", "%5.3d", 42);
    expect(o, "ff", "%x", 255);
    expect(o, "FF", "%X", 255);
    expect(o, "0xff", "%#x", 255);
    expect(o, "377", "%o", 255);
    expect(o, "0377", "%#o", 255);
    expect(o, "4294967295", "%u", 0xFFFFFFFFu);
    expect(o, "-9223372036854775808", "%lld", (long long)INT64_MIN);
    expect(o, "18446744073709551615", "%llu", (unsigned long long)UINT64_MAX);
    expect(o, "ffffffffffffffff", "%llx", (unsigned long long)UINT64_MAX);
    expect(o, "123456789012", "%zu", (size_t)123456789012ull);
    expect(o, "-5", "%hhd", (signed char)-5);
    expect(o, "65535", "%hu", (unsigned short)65535);
    expect(o, "hello", "%s", "hello");
    expect(o, "  hello", "%7s", "hello");
    expect(o, "hello  |", "%-7s|", "hello");
    expect(o, "hel", "%.3s", "hello");
    expect(o, "  hel", "%5.3s", "hello");
    expect(o, "ab   ", "%-*s", 5, "ab");
    expect(o, "x", "%c", 'x');
    expect(o, "100%", "%d%%", 100);
    expect(o, "1.500000", "%f", 1.5);
    expect(o, "0.25", "%.2f", 0.25);
    expect(o, "3", "%.0f", 3.0);
    expect(o, "-0.125000", "%f", -0.125);
    expect(o, "1024.000", "%.3f", 1024.0);
    expect(o, "  2.50", "%6.2f", 2.5);
    expect(o, "1.000000e+03", "%e", 1000.0);
    expect(o, "1.25e-01", "%.2e", 0.125);
    expect(o, "1e+06", "%g", 1000000.0);
    expect(o, "100000", "%g", 100000.0);
    expect(o, "0.5", "%g", 0.5);
    expect(o, "0x1p+0", "%a", 1.0);
    expect(o, "inf", "%f", __builtin_inf());
    expect(o, "-inf", "%f", -__builtin_inf());

    /* snprintf return value and truncation, sprintf length, repeated appends */
    char buf[16];
    int r = snprintf(buf, sizeof buf, "%s=%d", "value", 123456);
    fprintf(o, "snprintf r=%d buf=%s\n", r, buf);
    r = snprintf(buf, 5, "%s", "truncate");
    fprintf(o, "snprintf r=%d buf=%s\n", r, buf);
    r = snprintf(NULL, 0, "%08.3f|%-6d|%x", 3.14159, 7, 48879);
    fprintf(o, "measure r=%d\n", r);
    if (r != 20) fails++;
    char line[160];
    int off = 0;
    for (int i = 1; i <= 8; i++) off += sprintf(line + off, "%d^2=%-3d", i, i * i);
    fprintf(o, "row(%d): %s\n", off, line);

    /* a small formatted table */
    static const struct { const char *name; int qty; double price; } items[] = {
        {"bolt", 120, 0.25}, {"washer", 1000, 0.0625}, {"gear", 7, 12.5}, {"axle", 3, 100.0}};
    double total = 0;
    for (unsigned i = 0; i < sizeof items / sizeof *items; i++) {
        double line_total = items[i].qty * items[i].price;
        total += line_total;
        fprintf(o, "%-8s %5d x %8.4f = %10.2f\n", items[i].name, items[i].qty, items[i].price, line_total);
    }
    fprintf(o, "%-8s %28.2f\n", "total", total);

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
