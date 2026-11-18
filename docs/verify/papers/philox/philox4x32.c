// Philox4x32-10 counter-based random number generator, written for ounce-core from the algorithm in
//   J. K. Salmon, M. A. Moraes, R. O. Dror, D. E. Shaw, "Parallel random numbers: as easy as 1, 2, 3", SC'11.
// Ten rounds of: two 32x32->64 multiplies (constants M0, M1), then a xor of the high halves with the other counter words
// and the round key; the key advances by the Weyl constants W0, W1 between rounds. Freestanding aarch64 Linux: it prints one
// line per case ("<counter> <key> -> <output> ok|MISMATCH") and exits 0 only if every output equals the published value.
// The three expected outputs are the published known-answer vectors for philox4x32 10 (random123 tests/kat_vectors).
typedef unsigned int u32;
typedef unsigned long u64;

#define M0 0xD2511F53u
#define M1 0xCD9E8D57u
#define W0 0x9E3779B9u
#define W1 0xBB67AE85u

static void philox4x32_10(const u32 ctr[4], const u32 key[2], u32 out[4]) {
    u32 c0 = ctr[0], c1 = ctr[1], c2 = ctr[2], c3 = ctr[3], k0 = key[0], k1 = key[1];
    for (int round = 0; round < 10; round++) {
        u64 p0 = (u64)M0 * c0, p1 = (u64)M1 * c2;
        u32 n0 = (u32)(p1 >> 32) ^ c1 ^ k0, n1 = (u32)p1, n2 = (u32)(p0 >> 32) ^ c3 ^ k1, n3 = (u32)p0;
        c0 = n0; c1 = n1; c2 = n2; c3 = n3;
        k0 += W0; k1 += W1;
    }
    out[0] = c0; out[1] = c1; out[2] = c2; out[3] = c3;
}

static void put(const char *s, long n) {
    register long x0 __asm__("x0") = 1;
    register const char *x1 __asm__("x1") = s;
    register long x2 __asm__("x2") = n;
    register long x8 __asm__("x8") = 64;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x8) : "memory");
}
static void str(const char *s) { long n = 0; while (s[n]) n++; put(s, n); }
static void hex8(u32 v) {
    char b[8];
    for (int i = 0; i < 8; i++) b[i] = "0123456789abcdef"[(v >> (28 - 4 * i)) & 15];
    put(b, 8);
}
static void words(const u32 *w, int n) {
    for (int i = 0; i < n; i++) { if (i) str(" "); hex8(w[i]); }
}

static const struct { u32 ctr[4], key[2], want[4]; } kat[] = {
    {{0, 0, 0, 0}, {0, 0}, {0x6627e8d5, 0xe169c58d, 0xbc57ac4c, 0x9b00dbd8}},
    {{0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff}, {0xffffffff, 0xffffffff}, {0x408f276d, 0x41c83b0e, 0xa20bc7c6, 0x6d5451fd}},
    {{0x243f6a88, 0x85a308d3, 0x13198a2e, 0x03707344}, {0xa4093822, 0x299f31d0}, {0xd16cfe09, 0x94fdcceb, 0x5001e420, 0x24126ea1}},
};

void _start(void) {
    int fails = 0;
    for (unsigned i = 0; i < sizeof kat / sizeof kat[0]; i++) {
        u32 got[4];
        philox4x32_10(kat[i].ctr, kat[i].key, got);
        int ok = got[0] == kat[i].want[0] && got[1] == kat[i].want[1] && got[2] == kat[i].want[2] && got[3] == kat[i].want[3];
        words(kat[i].ctr, 4); str(" "); words(kat[i].key, 2); str(" -> "); words(got, 4);
        str(ok ? " ok\n" : " MISMATCH\n");
        fails += !ok;
    }
    str(fails ? "FAIL\n" : "PASS\n");
    register long x0 __asm__("x0") = fails != 0;
    register long x8 __asm__("x8") = 93;
    __asm__ volatile("svc #0" : : "r"(x0), "r"(x8));
    for (;;) {}
}
