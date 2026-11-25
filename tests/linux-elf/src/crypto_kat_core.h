/* Cryptographic-extension known answers (FEAT_AES/SHA1/SHA2/SHA512). Shared by crypto_kat.c (guest) and a native
 * probe, so the expected output in ../crypto_kat.expected comes from real hardware executing the same source.
 * It prints: AES-128 encrypt and decrypt of the FIPS-197 appendix B block, SHA-1/SHA-256/SHA-512 of "abc" built only
 * from the instructions, then one 64-bit hash of NSWEEP pseudo-random inputs per instruction. */
typedef unsigned long u64;
typedef unsigned int u32;
typedef unsigned char u8;
static const u32 K256[64] = {
    0x428a2f98U, 0x71374491U, 0xb5c0fbcfU, 0xe9b5dba5U, 0x3956c25bU, 0x59f111f1U, 0x923f82a4U, 0xab1c5ed5U,
    0xd807aa98U, 0x12835b01U, 0x243185beU, 0x550c7dc3U, 0x72be5d74U, 0x80deb1feU, 0x9bdc06a7U, 0xc19bf174U,
    0xe49b69c1U, 0xefbe4786U, 0x0fc19dc6U, 0x240ca1ccU, 0x2de92c6fU, 0x4a7484aaU, 0x5cb0a9dcU, 0x76f988daU,
    0x983e5152U, 0xa831c66dU, 0xb00327c8U, 0xbf597fc7U, 0xc6e00bf3U, 0xd5a79147U, 0x06ca6351U, 0x14292967U,
    0x27b70a85U, 0x2e1b2138U, 0x4d2c6dfcU, 0x53380d13U, 0x650a7354U, 0x766a0abbU, 0x81c2c92eU, 0x92722c85U,
    0xa2bfe8a1U, 0xa81a664bU, 0xc24b8b70U, 0xc76c51a3U, 0xd192e819U, 0xd6990624U, 0xf40e3585U, 0x106aa070U,
    0x19a4c116U, 0x1e376c08U, 0x2748774cU, 0x34b0bcb5U, 0x391c0cb3U, 0x4ed8aa4aU, 0x5b9cca4fU, 0x682e6ff3U,
    0x748f82eeU, 0x78a5636fU, 0x84c87814U, 0x8cc70208U, 0x90befffaU, 0xa4506cebU, 0xbef9a3f7U, 0xc67178f2U
};
static const u32 H256[8] = {
    0x6a09e667U, 0xbb67ae85U, 0x3c6ef372U, 0xa54ff53aU, 0x510e527fU, 0x9b05688cU, 0x1f83d9abU, 0x5be0cd19U
};
static const u64 K512[80] = {
    0x428a2f98d728ae22UL, 0x7137449123ef65cdUL, 0xb5c0fbcfec4d3b2fUL, 0xe9b5dba58189dbbcUL,
    0x3956c25bf348b538UL, 0x59f111f1b605d019UL, 0x923f82a4af194f9bUL, 0xab1c5ed5da6d8118UL,
    0xd807aa98a3030242UL, 0x12835b0145706fbeUL, 0x243185be4ee4b28cUL, 0x550c7dc3d5ffb4e2UL,
    0x72be5d74f27b896fUL, 0x80deb1fe3b1696b1UL, 0x9bdc06a725c71235UL, 0xc19bf174cf692694UL,
    0xe49b69c19ef14ad2UL, 0xefbe4786384f25e3UL, 0x0fc19dc68b8cd5b5UL, 0x240ca1cc77ac9c65UL,
    0x2de92c6f592b0275UL, 0x4a7484aa6ea6e483UL, 0x5cb0a9dcbd41fbd4UL, 0x76f988da831153b5UL,
    0x983e5152ee66dfabUL, 0xa831c66d2db43210UL, 0xb00327c898fb213fUL, 0xbf597fc7beef0ee4UL,
    0xc6e00bf33da88fc2UL, 0xd5a79147930aa725UL, 0x06ca6351e003826fUL, 0x142929670a0e6e70UL,
    0x27b70a8546d22ffcUL, 0x2e1b21385c26c926UL, 0x4d2c6dfc5ac42aedUL, 0x53380d139d95b3dfUL,
    0x650a73548baf63deUL, 0x766a0abb3c77b2a8UL, 0x81c2c92e47edaee6UL, 0x92722c851482353bUL,
    0xa2bfe8a14cf10364UL, 0xa81a664bbc423001UL, 0xc24b8b70d0f89791UL, 0xc76c51a30654be30UL,
    0xd192e819d6ef5218UL, 0xd69906245565a910UL, 0xf40e35855771202aUL, 0x106aa07032bbd1b8UL,
    0x19a4c116b8d2d0c8UL, 0x1e376c085141ab53UL, 0x2748774cdf8eeb99UL, 0x34b0bcb5e19b48a8UL,
    0x391c0cb3c5c95a63UL, 0x4ed8aa4ae3418acbUL, 0x5b9cca4f7763e373UL, 0x682e6ff3d6b2b8a3UL,
    0x748f82ee5defb2fcUL, 0x78a5636f43172f60UL, 0x84c87814a1f0ab72UL, 0x8cc702081a6439ecUL,
    0x90befffa23631e28UL, 0xa4506cebde82bde9UL, 0xbef9a3f7b2c67915UL, 0xc67178f2e372532bUL,
    0xca273eceea26619cUL, 0xd186b8c721c0c207UL, 0xeada7dd6cde0eb1eUL, 0xf57d4f7fee6ed178UL,
    0x06f067aa72176fbaUL, 0x0a637dc5a2c898a6UL, 0x113f9804bef90daeUL, 0x1b710b35131c471bUL,
    0x28db77f523047d84UL, 0x32caab7b40c72493UL, 0x3c9ebe0a15c9bebcUL, 0x431d67c49c100d4cUL,
    0x4cc5d4becb3e42b6UL, 0x597f299cfc657e2aUL, 0x5fcb6fab3ad6faecUL, 0x6c44198c4a475817UL
};
static const u64 H512[8] = {
    0x6a09e667f3bcc908UL, 0xbb67ae8584caa73bUL, 0x3c6ef372fe94f82bUL, 0xa54ff53a5f1d36f1UL,
    0x510e527fade682d1UL, 0x9b05688c2b3e6c1fUL, 0x1f83d9abfb41bd6bUL, 0x5be0cd19137e2179UL
};
#ifndef NSWEEP
#define NSWEEP 2048
#endif
#define OP3(name, asmtxt) \
    static void name(u8 *d, const u8 *n, const u8 *m) { \
        __asm__ volatile(".arch_extension aes\n.arch_extension sha2\n.arch_extension sha3\n" \
            "ldr q0, [%0]\nldr q1, [%1]\nldr q2, [%2]\n" asmtxt "\nstr q0, [%0]\n" \
            : : "r"(d), "r"(n), "r"(m) : "memory", "v0", "v1", "v2"); }
OP3(op_aese, "aese v0.16b, v1.16b")
OP3(op_aesd, "aesd v0.16b, v1.16b")
OP3(op_aesmc, "aesmc v0.16b, v1.16b")
OP3(op_aesimc, "aesimc v0.16b, v1.16b")
OP3(op_sha1c, "sha1c q0, s1, v2.4s")
OP3(op_sha1p, "sha1p q0, s1, v2.4s")
OP3(op_sha1m, "sha1m q0, s1, v2.4s")
OP3(op_sha1h, "sha1h s0, s1")
OP3(op_sha1su0, "sha1su0 v0.4s, v1.4s, v2.4s")
OP3(op_sha1su1, "sha1su1 v0.4s, v1.4s")
OP3(op_sha256h, "sha256h q0, q1, v2.4s")
OP3(op_sha256h2, "sha256h2 q0, q1, v2.4s")
OP3(op_sha256su0, "sha256su0 v0.4s, v1.4s")
OP3(op_sha256su1, "sha256su1 v0.4s, v1.4s, v2.4s")
OP3(op_sha512h, "sha512h q0, q1, v2.2d")
OP3(op_sha512h2, "sha512h2 q0, q1, v2.2d")
OP3(op_sha512su0, "sha512su0 v0.2d, v1.2d")
OP3(op_sha512su1, "sha512su1 v0.2d, v1.2d, v2.2d")
typedef void (*op_fn)(u8 *, const u8 *, const u8 *);
static const op_fn OPS[18] = {op_aese, op_aesd, op_aesmc, op_aesimc, op_sha1c, op_sha1p, op_sha1m, op_sha1h,
    op_sha1su0, op_sha1su1, op_sha256h, op_sha256h2, op_sha256su0, op_sha256su1, op_sha512h, op_sha512h2,
    op_sha512su0, op_sha512su1};
static void cp(u8 *d, const u8 *s, int n) { for (int i = 0; i < n; i++) d[i] = s[i]; }
static void zero(u8 *d, int n) { for (int i = 0; i < n; i++) d[i] = 0; }
static u32 ld32(const u8 *p) { return p[0] | (p[1] << 8) | (p[2] << 16) | ((u32)p[3] << 24); }
static void st32(u8 *p, u32 v) { p[0] = v; p[1] = v >> 8; p[2] = v >> 16; p[3] = v >> 24; }
static u64 ld64(const u8 *p) { return (u64)ld32(p) | ((u64)ld32(p + 4) << 32); }
static void st64(u8 *p, u64 v) { st32(p, (u32)v); st32(p + 4, (u32)(v >> 32)); }
static void add4(u8 *d, const u8 *a, const u32 *k) { for (int i = 0; i < 4; i++) st32(d + 4 * i, ld32(a + 4 * i) + k[i]); }
static void add2(u8 *d, const u8 *a, const u64 *k) { for (int i = 0; i < 2; i++) st64(d + 8 * i, ld64(a + 8 * i) + k[i]); }
static u8 sub_byte(u8 b) { u8 z[16], n[16], r[16]; zero(z, 16); zero(n, 16); n[0] = b; cp(r, z, 16); op_aese(r, n, z); return r[0]; }

static void aes128_expand(const u8 *key, u8 rk[11][16]) {
    static const u8 rcon[10] = {1, 2, 4, 8, 16, 32, 64, 128, 0x1b, 0x36};
    cp(rk[0], key, 16);
    for (int r = 1; r <= 10; r++) {
        u8 t[4] = {sub_byte(rk[r - 1][13]) ^ rcon[r - 1], sub_byte(rk[r - 1][14]), sub_byte(rk[r - 1][15]), sub_byte(rk[r - 1][12])};
        for (int i = 0; i < 4; i++) rk[r][i] = rk[r - 1][i] ^ t[i];
        for (int i = 4; i < 16; i++) rk[r][i] = rk[r - 1][i] ^ rk[r][i - 4];
    }
}
static void aes128_encrypt(u8 rk[11][16], const u8 *in, u8 *out) {
    u8 s[16];
    cp(s, in, 16);
    for (int r = 0; r < 9; r++) { op_aese(s, rk[r], rk[r]); op_aesmc(s, s, s); }
    op_aese(s, rk[9], rk[9]);
    for (int i = 0; i < 16; i++) out[i] = s[i] ^ rk[10][i];
}
static void aes128_decrypt(u8 rk[11][16], const u8 *in, u8 *out) {
    u8 s[16];
    cp(s, in, 16);
    /* Equivalent inverse cipher: AESIMC runs on the state and on round keys 1..9 (IMC(s ^ k) = IMC(s) ^ IMC(k)). */
    u8 dk[11][16];
    for (int r = 1; r <= 9; r++) op_aesimc(dk[r], rk[r], rk[r]);
    op_aesd(s, rk[10], rk[10]);
    for (int r = 9; r >= 1; r--) { op_aesimc(s, s, s); op_aesd(s, dk[r], dk[r]); }
    for (int i = 0; i < 16; i++) out[i] = s[i] ^ rk[0][i];
}

static void sha1_abc(u8 *out) {
    static const u32 K[4][4] = {{0x5a827999, 0x5a827999, 0x5a827999, 0x5a827999}, {0x6ed9eba1, 0x6ed9eba1, 0x6ed9eba1, 0x6ed9eba1},
        {0x8f1bbcdc, 0x8f1bbcdc, 0x8f1bbcdc, 0x8f1bbcdc}, {0xca62c1d6, 0xca62c1d6, 0xca62c1d6, 0xca62c1d6}};
    u8 w[20][16], abcd[16], e[16], e1[16], wk[16], z[16];
    zero(z, 16);
    for (int i = 0; i < 20; i++) zero(w[i], 16);
    /* "abc" + 0x80 + zeros + 64-bit big-endian bit length 24; words are big-endian */
    {
        static const u32 blk[16] = {0x61626380U, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 24};
        for (int i = 0; i < 16; i++) st32(w[i / 4] + 4 * (i % 4), blk[i]);
    }
    for (int i = 4; i < 20; i++) {
        u8 t[16];
        cp(t, w[i - 4], 16);
        op_sha1su0(t, w[i - 3], w[i - 2]);
        op_sha1su1(t, w[i - 1], z);
        cp(w[i], t, 16);
    }
    static const u32 H[5] = {0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0};
    for (int i = 0; i < 4; i++) st32(abcd + 4 * i, H[i]);
    zero(e, 16); st32(e, H[4]);
    for (int i = 0; i < 20; i++) {
        add4(wk, w[i], K[i / 5]);
        op_sha1h(e1, abcd, z);
        if (i < 5) op_sha1c(abcd, e, wk); else if (i < 10) op_sha1p(abcd, e, wk);
        else if (i < 15) op_sha1m(abcd, e, wk); else op_sha1p(abcd, e, wk);
        cp(e, e1, 16);
    }
    for (int i = 0; i < 4; i++) st32(out + 4 * i, ld32(abcd + 4 * i) + H[i]);
    st32(out + 16, ld32(e) + H[4]);
}

static void sha256_abc(u8 *out) {
    u8 w[16][16], abcd[16], efgh[16], tmp[16], wk[16], t[16];
    static const u32 blk[16] = {0x61626380U, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 24};
    for (int i = 0; i < 16; i++) st32(w[i / 4] + 4 * (i % 4), blk[i]);
    for (int i = 0; i < 4; i++) { st32(abcd + 4 * i, H256[i]); st32(efgh + 4 * i, H256[4 + i]); }
    for (int i = 0; i < 16; i++) {
        u8 *cur = w[i % 4];
        if (i >= 4) { /* W[i] = su1(su0(W[i-4], W[i-3]), W[i-2], W[i-1]) from the 4-entry ring */
            cp(t, w[i % 4], 16);
            op_sha256su0(t, w[(i + 1) % 4], w[(i + 1) % 4]);
            op_sha256su1(t, w[(i + 2) % 4], w[(i + 3) % 4]);
            cp(w[i % 4], t, 16);
        }
        add4(wk, cur, K256 + 4 * i);
        cp(tmp, abcd, 16);
        op_sha256h(abcd, efgh, wk);
        op_sha256h2(efgh, tmp, wk);
    }
    for (int i = 0; i < 4; i++) { st32(out + 4 * i, ld32(abcd + 4 * i) + H256[i]); st32(out + 16 + 4 * i, ld32(efgh + 4 * i) + H256[4 + i]); }
}

static void ext8(u8 *d, const u8 *a, const u8 *b) { cp(d, a + 8, 8); cp(d + 8, b, 8); }
static void sha512_abc(u8 *out) {
    u8 w[40][16], ab[16], cd[16], ef[16], gh[16], wk[16], sw[16], v6[16], v7[16], n_ab[16], n_ef[16], t[16], m[16];
    zero(w[0], 16);
    for (int i = 0; i < 16; i++) st64(w[i / 2] + 8 * (i % 2), i == 0 ? 0x6162638000000000UL : i == 15 ? 24UL : 0UL);
    for (int i = 8; i < 40; i++) { /* pairs: P[k+8] = su1(su0(P[k], P[k+1]), P[k+7], ext(P[k+4], P[k+5])) */
        int k = i - 8;
        cp(t, w[k], 16);
        op_sha512su0(t, w[k + 1], w[k + 1]);
        ext8(m, w[k + 4], w[k + 5]);
        op_sha512su1(t, w[k + 7], m);
        cp(w[i], t, 16);
    }
    for (int i = 0; i < 2; i++) { st64(ab + 8 * i, H512[i]); st64(cd + 8 * i, H512[2 + i]); st64(ef + 8 * i, H512[4 + i]); st64(gh + 8 * i, H512[6 + i]); }
    for (int i = 0; i < 40; i++) {
        add2(wk, w[i], K512 + 2 * i);
        ext8(sw, wk, wk); /* swap lanes */
        ext8(v6, ef, gh);
        ext8(v7, cd, ef);
        for (int j = 0; j < 2; j++) st64(gh + 8 * j, ld64(gh + 8 * j) + ld64(sw + 8 * j));
        op_sha512h(gh, v6, v7);
        for (int j = 0; j < 2; j++) st64(n_ef + 8 * j, ld64(cd + 8 * j) + ld64(gh + 8 * j));
        op_sha512h2(gh, cd, ab);
        cp(n_ab, gh, 16);
        cp(gh, ef, 16); cp(ef, n_ef, 16); cp(cd, ab, 16); cp(ab, n_ab, 16);
    }
    for (int j = 0; j < 2; j++) {
        st64(out + 8 * j, ld64(ab + 8 * j) + H512[j]); st64(out + 16 + 8 * j, ld64(cd + 8 * j) + H512[2 + j]);
        st64(out + 32 + 8 * j, ld64(ef + 8 * j) + H512[4 + j]); st64(out + 48 + 8 * j, ld64(gh + 8 * j) + H512[6 + j]);
    }
}

static u64 xs_state = 0x9e3779b97f4a7c15UL;
static u64 xs_next(void) { xs_state ^= xs_state >> 12; xs_state ^= xs_state << 25; xs_state ^= xs_state >> 27; return xs_state * 0x2545f4914f6cdd1dUL; }
static u64 sweep(op_fn f) {
    u64 h = 0xcbf29ce484222325UL;
    for (int i = 0; i < NSWEEP; i++) {
        u8 d[16], n[16], m[16];
        for (int j = 0; j < 2; j++) { st64(d + 8 * j, xs_next()); st64(n + 8 * j, xs_next()); st64(m + 8 * j, xs_next()); }
        f(d, n, m);
        for (int j = 0; j < 16; j++) h = (h ^ d[j]) * 0x100000001b3UL;
    }
    return h;
}

/* Calls emit(bytes, n) for each record: 2 AES blocks, 3 digests, 18 sweep hashes (8 bytes, little-endian). */
static void be_words(u8 *p, int n, int w) {
    for (int i = 0; i < n; i += w) for (int j = 0; j < w / 2; j++) { u8 t = p[i + j]; p[i + j] = p[i + w - 1 - j]; p[i + w - 1 - j] = t; }
}
static void run_crypto_kat(void (*emit)(const u8 *, int)) {
    static const u8 key[16] = {0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c};
    static const u8 pt[16] = {0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d, 0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37, 0x07, 0x34};
    u8 rk[11][16], ct[16], back[16], dg[64], hb[8];
    aes128_expand(key, rk);
    aes128_encrypt(rk, pt, ct); emit(ct, 16);
    aes128_decrypt(rk, ct, back); emit(back, 16);
    sha1_abc(dg); be_words(dg, 20, 4); emit(dg, 20);
    sha256_abc(dg); be_words(dg, 32, 4); emit(dg, 32);
    sha512_abc(dg); be_words(dg, 64, 8); emit(dg, 64);
    for (int i = 0; i < 18; i++) { st64(hb, sweep(OPS[i])); emit(hb, 8); }
}
