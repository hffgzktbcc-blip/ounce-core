// RC6 from Rivest, Robshaw, Sidney, Yin, "The RC6 Block Cipher" (AES submission, v1.1, August 20, 1998), written from the paper's
// encryption/decryption and key-schedule pseudocode. Checks the six published encryption test vectors of RC6-32/20/b with
// 16-, 24- and 32-byte keys (copied as data by script) and that decryption inverts each. Exit 0 only if everything matches.
#include "ounce_paper.h"

// Appendix 'Test vectors for encryption with RC6' (RC6-32/20/b): {user key, plaintext, ciphertext}
static const char *VEC[][3] = {
    {"00000000000000000000000000000000", "00000000000000000000000000000000", "8fc3a53656b1f778c129df4e9848a41e"},
    {"0123456789abcdef0112233445566778", "02132435465768798a9bacbdcedfe0f1", "524e192f4715c6231f51f6367ea43f18"},
    {"000000000000000000000000000000000000000000000000", "00000000000000000000000000000000", "6cd61bcb190b30384e8a3f168690ae82"},
    {"0123456789abcdef0112233445566778899aabbccddeeff0", "02132435465768798a9bacbdcedfe0f1", "688329d019e505041e52e92af95291d4"},
    {"0000000000000000000000000000000000000000000000000000000000000000", "00000000000000000000000000000000", "8f5fbd0510d15fa893fa3fda6e857ec2"},
    {"0123456789abcdef0112233445566778899aabbccddeeff01032547698badcfe", "02132435465768798a9bacbdcedfe0f1", "c8241816f0d7e48920ad16a1674e5d48"},
};

static int hexbytes(const char *s, u8 *out) {
    int n = 0;
    for (; s[0] && s[1]; s += 2) {
        int v = 0;
        for (int i = 0; i < 2; i++) { char c = s[i]; v = v * 16 + (c <= '9' ? c - '0' : c >= 'a' ? c - 'a' + 10 : c - 'A' + 10); }
        out[n++] = (u8)v;
    }
    return n;
}

// ---- RC6-32/r/b (Rivest, Robshaw, Sidney, Yin 1998, sections 2.2 and the Appendix key schedule) ----
#ifndef RC6_ROUNDS
#define RC6_ROUNDS 20
#endif
#define R RC6_ROUNDS
#define PW 0xB7E15163u          // P32
#define QW 0x9E3779B9u          // Q32
static u32 S[2 * R + 4];
static u32 rol(u32 x, u32 n) { n &= 31; return n ? (x << n) | (x >> (32 - n)) : x; }
static u32 ror(u32 x, u32 n) { n &= 31; return n ? (x >> n) | (x << (32 - n)) : x; }
static void rc6_key(const u8 *key, int b) {
    u32 L[8]; int c = b ? (b + 3) / 4 : 1;
    for (int i = 0; i < 8; i++) L[i] = 0;
    for (int i = 0; i < b; i++) L[i / 4] |= (u32)key[i] << (8 * (i % 4));      // little-endian words, zero padded
    S[0] = PW;
    for (int i = 1; i < 2 * R + 4; i++) S[i] = S[i - 1] + QW;
    u32 A = 0, B = 0; int i = 0, j = 0, v = 3 * (c > 2 * R + 4 ? c : 2 * R + 4);
    for (int s = 1; s <= v; s++) {
        A = S[i] = rol(S[i] + A + B, 3);
        B = L[j] = rol(L[j] + A + B, A + B);
        i = (i + 1) % (2 * R + 4); j = (j + 1) % c;
    }
}
static u32 ld(const u8 *p) { return (u32)p[0] | ((u32)p[1] << 8) | ((u32)p[2] << 16) | ((u32)p[3] << 24); }
static void st(u8 *p, u32 v) { for (int i = 0; i < 4; i++) p[i] = (u8)(v >> (8 * i)); }
static void rc6_enc(const u8 *in, u8 *out) {
    u32 A = ld(in), B = ld(in + 4), C = ld(in + 8), D = ld(in + 12);
    B += S[0]; D += S[1];
    for (int i = 1; i <= R; i++) {
        u32 t = rol(B * (2 * B + 1), 5), u = rol(D * (2 * D + 1), 5);
        A = rol(A ^ t, u) + S[2 * i]; C = rol(C ^ u, t) + S[2 * i + 1];
        u32 tmp = A; A = B; B = C; C = D; D = tmp;
    }
    A += S[2 * R + 2]; C += S[2 * R + 3];
    st(out, A); st(out + 4, B); st(out + 8, C); st(out + 12, D);
}
static void rc6_dec(const u8 *in, u8 *out) {
    u32 A = ld(in), B = ld(in + 4), C = ld(in + 8), D = ld(in + 12);
    C -= S[2 * R + 3]; A -= S[2 * R + 2];
    for (int i = R; i >= 1; i--) {
        u32 tmp = D; D = C; C = B; B = A; A = tmp;
        u32 u = rol(D * (2 * D + 1), 5), t = rol(B * (2 * B + 1), 5);
        C = ror(C - S[2 * i + 1], t) ^ u; A = ror(A - S[2 * i], u) ^ t;
    }
    D -= S[1]; B -= S[0];
    st(out, A); st(out + 4, B); st(out + 8, C); st(out + 12, D);
}

int paper_main(void) {
    int ok = 0, dok = 0, n = (int)(sizeof VEC / sizeof VEC[0]);
    for (int v = 0; v < n; v++) {
        u8 key[32], pt[16], want[16], got[16], back[16];
        int kl = hexbytes(VEC[v][0], key); hexbytes(VEC[v][1], pt); hexbytes(VEC[v][2], want);
        rc6_key(key, kl); rc6_enc(pt, got); rc6_dec(got, back);
        char name[24] = "RC6-32/20/00 key";
        name[10] = (char)('0' + kl / 10); name[11] = (char)('0' + kl % 10);
        name[7] = (char)('0' + R / 10); name[8] = (char)('0' + R % 10);
        if (pcheck_bytes(name, got, want, 16)) ok++;
        int d = 1; for (int j = 0; j < 16; j++) if (back[j] != pt[j]) d = 0;
        if (d) dok++;
    }
    pstr("published encryption vectors: "); pdec(ok); pstr(" of "); pdec(n); pstr(" match; decryption returns the plaintext for "); pdec(dok); pstr(" of "); pdec(n); pstr("\n");
    int bad = (ok != n) || (dok != n);
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
