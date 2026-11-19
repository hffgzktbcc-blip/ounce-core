/* elf-workload hashes: CRC-32, Adler-32, FNV-1a (32/64), djb2, SDBM, Jenkins one-at-a-time,
 * and SHA-256, each implemented from its public specification and checked against published
 * check values (CRC-32 of "123456789" = CBF43926, Adler-32 of "Wikipedia" = 11E60398,
 * FNV-1a, SHA-256 of "abc" from FIPS 180-4).
 * Provenance: original work written for ounce-core (MIT); algorithms are public specifications. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

static uint32_t crc32_bitwise(const uint8_t *p, size_t n) {
    uint32_t c = 0xFFFFFFFFu;
    while (n--) {
        c ^= *p++;
        for (int k = 0; k < 8; k++) c = (c >> 1) ^ (0xEDB88320u & (0u - (c & 1u)));
    }
    return ~c;
}

static uint32_t crc_table[256];
static void crc_init(void) {
    for (uint32_t i = 0; i < 256; i++) {
        uint32_t c = i;
        for (int k = 0; k < 8; k++) c = (c >> 1) ^ (0xEDB88320u & (0u - (c & 1u)));
        crc_table[i] = c;
    }
}
static uint32_t crc32_table(const uint8_t *p, size_t n) {
    uint32_t c = 0xFFFFFFFFu;
    while (n--) c = crc_table[(c ^ *p++) & 0xFF] ^ (c >> 8);
    return ~c;
}

static uint32_t adler32(const uint8_t *p, size_t n) {
    uint32_t a = 1, b = 0;
    while (n--) { a = (a + *p++) % 65521u; b = (b + a) % 65521u; }
    return (b << 16) | a;
}

static uint32_t fnv1a32(const uint8_t *p, size_t n) {
    uint32_t h = 2166136261u;
    while (n--) { h ^= *p++; h *= 16777619u; }
    return h;
}
static uint64_t fnv1a64(const uint8_t *p, size_t n) {
    uint64_t h = 14695981039346656037ull;
    while (n--) { h ^= *p++; h *= 1099511628211ull; }
    return h;
}
static uint32_t djb2(const uint8_t *p, size_t n) {
    uint32_t h = 5381;
    while (n--) h = h * 33 + *p++;
    return h;
}
static uint32_t sdbm(const uint8_t *p, size_t n) {
    uint32_t h = 0;
    while (n--) h = *p++ + (h << 6) + (h << 16) - h;
    return h;
}
static uint32_t jenkins_oaat(const uint8_t *p, size_t n) {
    uint32_t h = 0;
    while (n--) { h += *p++; h += h << 10; h ^= h >> 6; }
    h += h << 3; h ^= h >> 11; h += h << 15;
    return h;
}

/* SHA-256 (FIPS 180-4) */
static const uint32_t K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2};
#define ROR(x, n) (((x) >> (n)) | ((x) << (32 - (n))))
static void sha256_block(uint32_t h[8], const uint8_t *b) {
    uint32_t w[64];
    for (int i = 0; i < 16; i++)
        w[i] = (uint32_t)b[4 * i] << 24 | (uint32_t)b[4 * i + 1] << 16 | (uint32_t)b[4 * i + 2] << 8 | b[4 * i + 3];
    for (int i = 16; i < 64; i++) {
        uint32_t s0 = ROR(w[i - 15], 7) ^ ROR(w[i - 15], 18) ^ (w[i - 15] >> 3);
        uint32_t s1 = ROR(w[i - 2], 17) ^ ROR(w[i - 2], 19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16] + s0 + w[i - 7] + s1;
    }
    uint32_t a = h[0], bb = h[1], c = h[2], d = h[3], e = h[4], f = h[5], g = h[6], hh = h[7];
    for (int i = 0; i < 64; i++) {
        uint32_t t1 = hh + (ROR(e, 6) ^ ROR(e, 11) ^ ROR(e, 25)) + ((e & f) ^ (~e & g)) + K[i] + w[i];
        uint32_t t2 = (ROR(a, 2) ^ ROR(a, 13) ^ ROR(a, 22)) + ((a & bb) ^ (a & c) ^ (bb & c));
        hh = g; g = f; f = e; e = d + t1; d = c; c = bb; bb = a; a = t1 + t2;
    }
    h[0] += a; h[1] += bb; h[2] += c; h[3] += d; h[4] += e; h[5] += f; h[6] += g; h[7] += hh;
}
static void sha256(const uint8_t *msg, size_t n, char hex[65]) {
    uint32_t h[8] = {0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19};
    uint8_t blk[128];
    size_t full = n / 64;
    for (size_t i = 0; i < full; i++) sha256_block(h, msg + 64 * i);
    size_t rem = n - full * 64;
    memcpy(blk, msg + full * 64, rem);
    blk[rem] = 0x80;
    size_t pad = rem < 56 ? 64 : 128;
    memset(blk + rem + 1, 0, pad - rem - 1);
    uint64_t bits = (uint64_t)n * 8;
    for (int i = 0; i < 8; i++) blk[pad - 1 - i] = (uint8_t)(bits >> (8 * i));
    for (size_t i = 0; i < pad; i += 64) sha256_block(h, blk + i);
    for (int i = 0; i < 8; i++) snprintf(hex + 8 * i, 9, "%08x", h[i]);
}

int workload_run(FILE *o) {
    int fails = 0;
    const char *chk = "123456789";
    crc_init();
    uint32_t c1 = crc32_bitwise((const uint8_t *)chk, 9), c2 = crc32_table((const uint8_t *)chk, 9);
    fprintf(o, "crc32(\"123456789\") bitwise=%08X table=%08X\n", c1, c2);
    if (c1 != 0xCBF43926u || c2 != 0xCBF43926u) fails++;
    uint32_t ad = adler32((const uint8_t *)"Wikipedia", 9);
    fprintf(o, "adler32(\"Wikipedia\")=%08X\n", ad);
    if (ad != 0x11E60398u) fails++;
    uint32_t f1 = fnv1a32((const uint8_t *)"a", 1);
    uint64_t f2 = fnv1a64((const uint8_t *)"a", 1);
    fprintf(o, "fnv1a(\"a\") 32=%08X 64=%016llX\n", f1, (unsigned long long)f2);
    if (f1 != 0xE40C292Cu || f2 != 0xAF63DC4C8601EC8Cull) fails++;
    fprintf(o, "fnv1a(\"\")=%08X djb2(\"\")=%u\n", fnv1a32((const uint8_t *)"", 0), djb2((const uint8_t *)"", 0));

    /* A 4 KiB deterministic buffer through every hash; the two CRC implementations must agree. */
    static uint8_t buf[4096];
    uint32_t x = 12345;
    for (int i = 0; i < 4096; i++) { x = x * 1103515245u + 12345u; buf[i] = (uint8_t)(x >> 16); }
    for (size_t n = 0; n <= 4096; n += (n < 8 ? 1 : 1023)) {
        uint32_t a = crc32_bitwise(buf, n), b = crc32_table(buf, n);
        fprintf(o, "n=%4zu crc=%08X adler=%08X fnv32=%08X fnv64=%016llX djb2=%08X sdbm=%08X oaat=%08X\n",
                n, a, adler32(buf, n), fnv1a32(buf, n), (unsigned long long)fnv1a64(buf, n),
                djb2(buf, n), sdbm(buf, n), jenkins_oaat(buf, n));
        if (a != b) fails++;
    }

    char hex[65];
    sha256((const uint8_t *)"abc", 3, hex);
    fprintf(o, "sha256(\"abc\")=%s\n", hex);
    if (strcmp(hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")) fails++;
    sha256((const uint8_t *)"", 0, hex);
    fprintf(o, "sha256(\"\")=%s\n", hex);
    if (strcmp(hex, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")) fails++;
    const char *two = "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"; /* FIPS two-block message */
    sha256((const uint8_t *)two, strlen(two), hex);
    fprintf(o, "sha256(two-block)=%s\n", hex);
    if (strcmp(hex, "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1")) fails++;
    sha256(buf, 4096, hex);
    fprintf(o, "sha256(buf4096)=%s\n", hex);

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
