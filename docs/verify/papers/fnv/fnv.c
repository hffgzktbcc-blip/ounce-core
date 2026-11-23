// FNV (Fowler/Noll/Vo) hash from the IETF draft "The FNV Non-Cryptographic Hash Algorithm" (draft-eastlake-fnv-21), written from
// the algorithm description in the draft. One generic n-bit implementation (n = 32 .. 1024, multi-limb arithmetic, the prime
// applied as shifts and adds) is checked three ways against numbers printed in the draft:
//   1. Appendix C: FNV-1a 32-bit and 64-bit of "", "a", "foobar", without and with a terminating zero byte (12 values).
//   2. Table 1: the six FNV primes, rebuilt from their stated form 2^s + 2^8 + b (6 values).
//   3. Table 2: the six offset_basis values (32, 64, 128, 256, 512, 1024 bits), recomputed as the draft defines them: the n-bit
//      FNV-0 hash of the 32 octets "chongo <Landon Curt Noll> /\../\" (6 values).
// The numbers are data copied from the draft (the two tables as hex strings by a script); none of the draft's code was copied.
// Exit 0 only if all 24 checks match.
#include "ounce_paper.h"

#ifndef FNV_B_ADJ    // negative control hook: added to the low-byte term b of the 32-bit prime (published: 0)
#define FNV_B_ADJ 0
#endif

#include "fnv_tables.inc"

#define MAXL 32                    // 32 limbs of 32 bits = 1024 bits
typedef struct { int n; u32 v[MAXL]; } big;   // n limbs, little-endian limbs

static const int SIZES[6] = {32, 64, 128, 256, 512, 1024};
static const int PRIME_S[6] = {24, 40, 88, 168, 344, 680};              // prime = 2^s + 2^8 + b, from the draft
static const int PRIME_B[6] = {0x93 + FNV_B_ADJ, 0xB3, 0x3B, 0x63, 0x57, 0x8D};

static const char *const P_NAME[6] = {"Table 1: FNV prime, 32 bits", "Table 1: FNV prime, 64 bits", "Table 1: FNV prime, 128 bits",
    "Table 1: FNV prime, 256 bits", "Table 1: FNV prime, 512 bits", "Table 1: FNV prime, 1024 bits"};
static const char *const B_NAME[6] = {"Table 2: offset_basis, 32 bits", "Table 2: offset_basis, 64 bits", "Table 2: offset_basis, 128 bits",
    "Table 2: offset_basis, 256 bits", "Table 2: offset_basis, 512 bits", "Table 2: offset_basis, 1024 bits"};

static void big_zero(big *x, int bits) { x->n = bits / 32; for (int i = 0; i < MAXL; i++) x->v[i] = 0; }
static int hexval(char c) { return c <= '9' ? c - '0' : c - 'a' + 10; }
static void big_from_hex(big *x, int bits, const char *h) {
    big_zero(x, bits);
    int digits = bits / 4;
    for (int i = 0; i < digits; i++) {                   // h[0] is the most significant digit
        int pos = digits - 1 - i;
        x->v[pos / 8] |= (u32)hexval(h[i]) << (4 * (pos % 8));
    }
}
static int big_eq(const big *a, const big *b) { for (int i = 0; i < a->n; i++) if (a->v[i] != b->v[i]) return 0; return 1; }
static void big_print(const big *x) { for (int i = x->n - 1; i >= 0; i--) phex(x->v[i], 8); }

static void big_prime(big *p, int bits, int s, int b) {   // 2^s + 2^8 + b
    big_zero(p, bits);
    p->v[s / 32] |= 1u << (s % 32);
    u64 c = 0;
    u64 add = 256u + (u64)b;
    for (int i = 0; i < p->n; i++) { c += (u64)p->v[i] + (i == 0 ? add : 0); p->v[i] = (u32)c; c >>= 32; }
}

// r = h * prime mod 2^bits, using the sparse form: (h << s) + (h << 8) + h*b
static void big_mul_prime(big *h, int s, int b) {
    int n = h->n;
    u32 t[MAXL];
    u64 c = 0;
    for (int i = 0; i < n; i++) {                          // t = h * b
        c += (u64)h->v[i] * (u64)b; t[i] = (u32)c; c >>= 32;
    }
    for (int k = 0; k < 2; k++) {                          // add h<<8 and h<<s
        int sh = k == 0 ? 8 : s, limbs = sh / 32, bitsh = sh % 32;
        u64 carry = 0;
        for (int i = 0; i < n; i++) {
            u32 w = 0;
            int j = i - limbs;
            if (j >= 0) {
                w = h->v[j] << bitsh;
                if (bitsh && j >= 1) w |= h->v[j - 1] >> (32 - bitsh);
            }
            carry += (u64)t[i] + w; t[i] = (u32)carry; carry >>= 32;
        }
    }
    for (int i = 0; i < n; i++) h->v[i] = t[i];
}

// FNV-0 / FNV-1 (multiply then xor) and FNV-1a (xor then multiply), n-bit
static void fnv(big *h, int idx, const big *basis, const u8 *m, int len, int variant_1a) {
    *h = *basis;
    for (int i = 0; i < len; i++) {
        if (variant_1a) { h->v[0] ^= m[i]; big_mul_prime(h, PRIME_S[idx], PRIME_B[idx]); }
        else { big_mul_prime(h, PRIME_S[idx], PRIME_B[idx]); h->v[0] ^= m[i]; }
    }
}

static int check_big(const char *name, const big *got, const char *want_hex, int bits) {
    big want; big_from_hex(&want, bits, want_hex);
    int ok = big_eq(got, &want);
    pstr(ok ? "PASS " : "FAIL "); pstr(name); pstr(": "); big_print(got);
    if (!ok) { pstr(" expected "); big_print(&want); }
    pstr("\n"); return ok;
}

static const char *const A32_NAME[2][3] = {
    {"Appendix C, FNV-1a 32-bit, \"\"", "Appendix C, FNV-1a 32-bit, \"a\"", "Appendix C, FNV-1a 32-bit, \"foobar\""},
    {"Appendix C, FNV-1a 32-bit, \"\" + NUL", "Appendix C, FNV-1a 32-bit, \"a\" + NUL", "Appendix C, FNV-1a 32-bit, \"foobar\" + NUL"}};
static const char *const A64_NAME[2][3] = {
    {"Appendix C, FNV-1a 64-bit, \"\"", "Appendix C, FNV-1a 64-bit, \"a\"", "Appendix C, FNV-1a 64-bit, \"foobar\""},
    {"Appendix C, FNV-1a 64-bit, \"\" + NUL", "Appendix C, FNV-1a 64-bit, \"a\" + NUL", "Appendix C, FNV-1a 64-bit, \"foobar\" + NUL"}};

int paper_main(void) {
    int ok = 1;
    // 2. Table 1 primes
    for (int i = 0; i < 6; i++) {
        big p; big_prime(&p, SIZES[i], PRIME_S[i], PRIME_B[i]);
        const char *name = P_NAME[i];
        ok &= check_big(name, &p, PRIME_HEX[i], SIZES[i]);
    }
    // 3. Table 2 offset_basis = n-bit FNV-0 of the 32-octet string
    static const u8 SIG[33] = "chongo <Landon Curt Noll> /\\../\\";
    for (int i = 0; i < 6; i++) {
        big zero, h; big_zero(&zero, SIZES[i]);
        fnv(&h, i, &zero, SIG, 32, 0);
        const char *name = B_NAME[i];
        ok &= check_big(name, &h, BASIS_HEX[i], SIZES[i]);
    }
    // 1. Appendix C: FNV-1a 32 and 64 of "", "a", "foobar", without and with a terminating zero byte
    static const char *const STR[3] = {"", "a", "foobar"};
    static const u32 W32[2][3] = {{0x811c9dc5u, 0xe40c292cu, 0xbf9cf968u}, {0x050c5d1fu, 0x2b24d044u, 0x0c1c9eb8u}};
    static const u64 W64[2][3] = {
        {0xcbf29ce484222325ULL, 0xaf63dc4c8601ec8cULL, 0x85944171f73967e8ULL},
        {0xaf63bd4c8601b7dfULL, 0x089be207b544f1e4ULL, 0x34531ca7168b8f38ULL}};
    big b32, b64; big_from_hex(&b32, 32, BASIS_HEX[0]); big_from_hex(&b64, 64, BASIS_HEX[1]);
    for (int t = 0; t < 2; t++) {
        for (int s = 0; s < 3; s++) {
            u8 m[8]; int len = 0; for (const char *q = STR[s]; *q; q++) m[len++] = (u8)*q;
            if (t == 1) m[len++] = 0;
            big h;
            const char *n32 = A32_NAME[t][s], *n64 = A64_NAME[t][s];
            fnv(&h, 0, &b32, m, len, 1);
            big w; big_zero(&w, 32); w.v[0] = W32[t][s];
            int ok32 = big_eq(&h, &w);
            pstr(ok32 ? "PASS " : "FAIL "); pstr(n32); pstr(": "); big_print(&h);
            if (!ok32) { pstr(" expected "); big_print(&w); }
            pstr("\n"); ok &= ok32;
            fnv(&h, 1, &b64, m, len, 1);
            big_zero(&w, 64); w.v[0] = (u32)W64[t][s]; w.v[1] = (u32)(W64[t][s] >> 32);
            int ok64 = big_eq(&h, &w);
            pstr(ok64 ? "PASS " : "FAIL "); pstr(n64); pstr(": "); big_print(&h);
            if (!ok64) { pstr(" expected "); big_print(&w); }
            pstr("\n"); ok &= ok64;
        }
    }
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
