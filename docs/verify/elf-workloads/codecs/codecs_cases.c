/* elf-workload codecs: Base64 (RFC 4648 vectors), hexadecimal, run-length coding, LEB128 varints,
 * zig-zag integers and a small LZSS compressor/decompressor. Each codec round-trips a fixed
 * pseudo-random and a compressible buffer; the RFC 4648 test vectors are checked literally.
 * Provenance: original work written for ounce-core (MIT); the RFC 4648 vectors are the only external data. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static const char B64[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

static size_t b64_enc(const uint8_t *in, size_t n, char *out) {
    size_t o = 0;
    for (size_t i = 0; i < n; i += 3) {
        uint32_t v = (uint32_t)in[i] << 16 | (i + 1 < n ? (uint32_t)in[i + 1] << 8 : 0) | (i + 2 < n ? in[i + 2] : 0);
        out[o++] = B64[v >> 18];
        out[o++] = B64[(v >> 12) & 63];
        out[o++] = i + 1 < n ? B64[(v >> 6) & 63] : '=';
        out[o++] = i + 2 < n ? B64[v & 63] : '=';
    }
    out[o] = 0;
    return o;
}
static int b64_val(char c) {
    const char *p = c ? strchr(B64, c) : NULL;
    return p ? (int)(p - B64) : -1;
}
static long b64_dec(const char *in, uint8_t *out) {
    size_t n = strlen(in), o = 0;
    if (n % 4) return -1;
    for (size_t i = 0; i < n; i += 4) {
        int a = b64_val(in[i]), b = b64_val(in[i + 1]);
        int c = in[i + 2] == '=' ? -2 : b64_val(in[i + 2]), d = in[i + 3] == '=' ? -2 : b64_val(in[i + 3]);
        if (a < 0 || b < 0 || c == -1 || d == -1) return -1;
        out[o++] = (uint8_t)(a << 2 | b >> 4);
        if (c >= 0) out[o++] = (uint8_t)(b << 4 | c >> 2);
        if (d >= 0) out[o++] = (uint8_t)(c << 6 | d);
    }
    return (long)o;
}

static size_t rle_enc(const uint8_t *in, size_t n, uint8_t *out) {
    size_t o = 0;
    for (size_t i = 0; i < n;) {
        size_t j = i;
        while (j < n && in[j] == in[i] && j - i < 255) j++;
        out[o++] = (uint8_t)(j - i);
        out[o++] = in[i];
        i = j;
    }
    return o;
}
static size_t rle_dec(const uint8_t *in, size_t n, uint8_t *out) {
    size_t o = 0;
    for (size_t i = 0; i + 1 < n; i += 2) for (int k = 0; k < in[i]; k++) out[o++] = in[i + 1];
    return o;
}

static size_t leb_put(uint64_t v, uint8_t *out) {
    size_t n = 0;
    do { uint8_t b = v & 0x7F; v >>= 7; out[n++] = b | (v ? 0x80 : 0); } while (v);
    return n;
}
static size_t leb_get(const uint8_t *in, uint64_t *v) {
    size_t n = 0;
    int shift = 0;
    *v = 0;
    for (;;) { uint8_t b = in[n++]; *v |= (uint64_t)(b & 0x7F) << shift; shift += 7; if (!(b & 0x80)) return n; }
}
static uint64_t zz(int64_t v) { return ((uint64_t)v << 1) ^ (uint64_t)(v >> 63); }
static int64_t unzz(uint64_t u) { return (int64_t)(u >> 1) ^ -(int64_t)(u & 1); }

/* LZSS: flag byte covers 8 items; literal = byte, match = 2 bytes (12-bit offset, 4-bit length-3) */
static size_t lzss_enc(const uint8_t *in, size_t n, uint8_t *out) {
    size_t o = 0, i = 0;
    while (i < n) {
        size_t flag_pos = o++;
        uint8_t flags = 0;
        for (int bit = 0; bit < 8 && i < n; bit++) {
            size_t best = 0, boff = 0, start = i > 4095 ? i - 4095 : 0;
            for (size_t j = start; j < i; j++) {
                size_t l = 0;
                while (l < 18 && i + l < n && in[j + l] == in[i + l]) l++;
                if (l > best) { best = l; boff = i - j; }
            }
            if (best >= 3) {
                flags |= (uint8_t)(1 << bit);
                out[o++] = (uint8_t)(boff & 0xFF);
                out[o++] = (uint8_t)((boff >> 8) << 4 | (best - 3));
                i += best;
            } else out[o++] = in[i++];
        }
        out[flag_pos] = flags;
    }
    return o;
}
static size_t lzss_dec(const uint8_t *in, size_t n, uint8_t *out) {
    size_t o = 0, i = 0;
    while (i < n) {
        uint8_t flags = in[i++];
        for (int bit = 0; bit < 8 && i < n; bit++) {
            if (flags & (1 << bit)) {
                size_t off = in[i] | (size_t)(in[i + 1] >> 4) << 8, len = (in[i + 1] & 15) + 3;
                i += 2;
                for (size_t k = 0; k < len; k++, o++) out[o] = out[o - off];
            } else out[o++] = in[i++];
        }
    }
    return o;
}

int workload_run(FILE *o) {
    int fails = 0;
    static const struct { const char *in, *out; } rfc[] = {
        {"", ""}, {"f", "Zg=="}, {"fo", "Zm8="}, {"foo", "Zm9v"}, {"foob", "Zm9vYg=="}, {"fooba", "Zm9vYmE="}, {"foobar", "Zm9vYmFy"}};
    for (unsigned i = 0; i < sizeof rfc / sizeof *rfc; i++) {
        char enc[32];
        uint8_t dec[32];
        b64_enc((const uint8_t *)rfc[i].in, strlen(rfc[i].in), enc);
        long dn = b64_dec(enc, dec);
        int ok = !strcmp(enc, rfc[i].out) && dn == (long)strlen(rfc[i].in) && !memcmp(dec, rfc[i].in, (size_t)dn);
        fprintf(o, "base64(\"%s\")=\"%s\" %s\n", rfc[i].in, enc, ok ? "ok" : "BAD");
        if (!ok) fails++;
    }
    uint8_t dec[8];
    fprintf(o, "base64 reject: bad-length=%ld bad-char=%ld\n", b64_dec("Zm9", dec), b64_dec("Zm9*", dec));
    if (b64_dec("Zm9", dec) != -1 || b64_dec("Zm9*", dec) != -1) fails++;

    static uint8_t rnd[3000], flat[3000], enc[8000], dec2[8000];
    static char text[4200];
    uint32_t x = 99;
    for (int i = 0; i < 3000; i++) {
        x = x * 1664525u + 1013904223u;
        rnd[i] = (uint8_t)(x >> 24);
        flat[i] = (uint8_t)("AAAAAAAABBBCDDDDDDDDDDDDDDDD"[i % 28] + (i / 700));
    }
    uint64_t h = 0;

    size_t tl = b64_enc(rnd, sizeof rnd, text);
    long bl = b64_dec(text, dec2);
    int same = bl == 3000 && !memcmp(dec2, rnd, 3000);
    fprintf(o, "base64 3000 random bytes -> %zu chars, round trip=%d\n", tl, same);
    if (!same) fails++;

    for (int k = 0; k < 2; k++) {
        const uint8_t *src = k ? flat : rnd;
        const char *name = k ? "compressible" : "random";
        size_t en = rle_enc(src, 3000, enc), dn = rle_dec(enc, en, dec2);
        int ok = dn == 3000 && !memcmp(dec2, src, 3000);
        fprintf(o, "rle %-12s 3000 -> %zu round trip=%d\n", name, en, ok);
        if (!ok) fails++;
        en = lzss_enc(src, 3000, enc);
        dn = lzss_dec(enc, en, dec2);
        ok = dn == 3000 && !memcmp(dec2, src, 3000);
        h = 0;
        for (size_t i = 0; i < en; i++) h = h * 131 + enc[i];
        fprintf(o, "lzss %-12s 3000 -> %zu round trip=%d fold=%016llx\n", name, en, ok, (unsigned long long)h);
        if (!ok) fails++;
    }

    /* hex encode of the first bytes of the compressed stream */
    for (int i = 0; i < 12; i++) fprintf(o, "%02x", enc[i]);
    fputc('\n', o);

    /* varints and zig-zag */
    static const int64_t vals[] = {0, 1, -1, 63, -64, 64, 127, 128, 300, -300, 1 << 20, INT32_MIN, INT64_MAX, INT64_MIN};
    uint8_t vb[16];
    for (unsigned i = 0; i < sizeof vals / sizeof *vals; i++) {
        size_t n = leb_put(zz(vals[i]), vb);
        uint64_t u;
        size_t m = leb_get(vb, &u);
        int ok = n == m && unzz(u) == vals[i];
        fprintf(o, "varint %lld -> zz=%llu bytes=%zu %s\n", (long long)vals[i], (unsigned long long)zz(vals[i]), n, ok ? "ok" : "BAD");
        if (!ok) fails++;
    }
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
