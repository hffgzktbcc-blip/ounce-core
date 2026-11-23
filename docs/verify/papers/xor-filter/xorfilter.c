// Xor filters, from Graf and Lemire, "Xor Filters: Faster and Smaller Than Bloom and Cuckoo Filters" (ACM J. Exp. Algorithmics
// 2020; arXiv:1912.08258), written from the paper's Algorithms 1 to 4 (membership test, construction by peeling a random
// 3-partite hypergraph, with capacity floor(1.23 n) + 32) and, for the comparison rows of its Table 3, standard Bloom filters
// with 8, 12 and 16 bits per key and the optimal number of probes. Checks, on n = 100000 generated keys and 1000000 generated
// non-member queries (fixed seeds): no false negatives; the false-positive percentage of Xor 8, Xor 16, Bloom 8, Bloom 12 and
// Bloom 16 lies in the range fixed below, around the values in Table 3 of the paper; the bits per key of Xor 8 and Xor 16.
// Ranges (fixed before the first run, in percent): Xor 8 [0.35, 0.43] (Table 3: 0.389 and 0.391), Xor 16 [0.0005, 0.0025]
// (0.002 and 0.001), Bloom 8 [2.10, 2.27] (2.161 and 2.205), Bloom 12 [0.29, 0.36] (0.313 and 0.339), Bloom 16 [0.040, 0.060]
// (0.046 and 0.053); bits per key Xor 8 [9.80, 9.90] (9.8), Xor 16 [19.60, 19.80] (19.7).
// Exit 0 only if everything is in range.
#include "ounce_paper.h"

#ifndef NEG
#define NEG 0   // NEG=1: Xor 8 keeps only 7 fingerprint bits; NEG=2: Bloom filters use half the optimal number of probes
#endif
#define N 100000
#define Q 1000000
#define CAP (N * 123 / 100 + 32)

static u64 mix(u64 x) { x += 0x9e3779b97f4a7c15ULL; x = (x ^ (x >> 30)) * 0xbf58476d1ce4e5b9ULL; x = (x ^ (x >> 27)) * 0x94d049bb133111ebULL; return x ^ (x >> 31); }
static u64 keyof(u64 i, int member) { u64 k = mix(i * 2 + 0x1000); return member ? (k & 0x7fffffffffffffffULL) : (k | 0x8000000000000000ULL); }
static u32 reduce(u32 h, u32 n) { return (u32)(((u64)h * (u64)n) >> 32); }   // map a 32-bit hash to [0, n)

// ---- Xor filter (Algorithms 1 to 4), fingerprint width FB bits stored in u16 cells
static u16 B[CAP]; static u32 cnt[CAP]; static u64 msk[CAP]; static u32 stk_i[N]; static u64 stk_x[N]; static u32 que[CAP];
static u64 keys[N];
static u32 blocklen;
static void hashes(u64 key, u64 seed, u32 *h0, u32 *h1, u32 *h2, u32 *fp16) {
    u64 a = mix(key ^ seed), b = mix(a ^ 0x243f6a8885a308d3ULL);
    *h0 = reduce((u32)a, blocklen); *h1 = blocklen + reduce((u32)(a >> 32), blocklen); *h2 = 2 * blocklen + reduce((u32)b, blocklen);
    *fp16 = (u32)(b >> 32) & 0xffff;
}
static int build_xor(u64 seed, int fb) {
    blocklen = CAP / 3;
    for (u32 i = 0; i < CAP; i++) { cnt[i] = 0; msk[i] = 0; B[i] = 0; }
    for (int j = 0; j < N; j++) { u32 h[3], f; hashes(keys[j], seed, &h[0], &h[1], &h[2], &f); for (int t = 0; t < 3; t++) { cnt[h[t]]++; msk[h[t]] ^= keys[j]; } }
    u32 qh = 0, qt = 0; for (u32 i = 0; i < 3 * blocklen; i++) if (cnt[i] == 1) que[qt++] = i;
    int sp = 0;
    while (qh < qt) {
        u32 i = que[qh++]; if (cnt[i] != 1) continue;
        u64 x = msk[i]; stk_i[sp] = i; stk_x[sp] = x; sp++;
        u32 h[3], f; hashes(x, seed, &h[0], &h[1], &h[2], &f);
        for (int t = 0; t < 3; t++) { cnt[h[t]]--; msk[h[t]] ^= x; if (cnt[h[t]] == 1) que[qt++] = h[t]; }
    }
    if (sp != N) return 0;
    for (int s = N - 1; s >= 0; s--) {                      // Algorithm 4: assign in reverse order
        u32 h[3], f; hashes(stk_x[s], seed, &h[0], &h[1], &h[2], &f); u16 mask = (u16)((1u << fb) - 1);
        u16 v = (u16)(f & mask); u32 i = stk_i[s];
        for (int t = 0; t < 3; t++) if (h[t] != i) v ^= B[h[t]];
        B[i] = v;
    }
    return 1;
}
static int xor_query(u64 key, u64 seed, int fb) {
    u32 h0, h1, h2, f; hashes(key, seed, &h0, &h1, &h2, &f); u16 mask = (u16)((1u << fb) - 1);
    return (u16)(f & mask) == (u16)(B[h0] ^ B[h1] ^ B[h2]);
}

// ---- Bloom filter, bpk bits per key, k probes by double hashing
static u8 bits[(N * 16) / 8 + 8];
static void bloom_probe(u64 key, int k, u64 m, u64 *pos) { u64 a = mix(key), b = mix(a ^ 0x13198a2e03707344ULL) | 1; for (int i = 0; i < k; i++) pos[i] = (a + (u64)i * b) % m; }
static int bloom_run(int bpk, int k, u64 *fp_count) {
    u64 m = (u64)N * (u64)bpk, pos[16];
    for (u64 i = 0; i < m / 8 + 8; i++) bits[i] = 0;
    for (int j = 0; j < N; j++) { bloom_probe(keys[j], k, m, pos); for (int i = 0; i < k; i++) bits[pos[i] >> 3] |= (u8)(1u << (pos[i] & 7)); }
    int fn = 0; for (int j = 0; j < N; j++) { bloom_probe(keys[j], k, m, pos); int in = 1; for (int i = 0; i < k; i++) if (!(bits[pos[i] >> 3] >> (pos[i] & 7) & 1)) in = 0; fn += !in; }
    u64 fp = 0;
    for (u64 q = 0; q < Q; q++) { bloom_probe(keyof(q, 0), k, m, pos); int in = 1; for (int i = 0; i < k; i++) if (!(bits[pos[i] >> 3] >> (pos[i] & 7) & 1)) { in = 0; break; } fp += (u64)in; }
    *fp_count = fp; return fn;
}
// print count/Q as a percentage with 4 decimals
static void ppct(u64 fp) { u64 v = fp * 1000000ULL / Q; pdec((i64)(v / 10000)); pstr("."); char d[4]; for (int i = 3; i >= 0; i--) { d[i] = (char)('0' + v % 10); v /= 10; } paper_write(d, 4); }
static int in_range(u64 fp, u64 lo4, u64 hi4) { u64 v = fp * 1000000ULL / Q; return v >= lo4 && v <= hi4; }   // bounds in units of 0.0001 percent

int paper_main(void) {
    int ok = 1;
    for (int j = 0; j < N; j++) keys[j] = keyof((u64)j, 1);
    static const struct { int fb; u64 lo, hi; const char *name; } X[2] = {{NEG == 1 ? 7 : 8, 3500, 4300, "Xor 8"}, {16, 5, 25, "Xor 16"}};
    for (int t = 0; t < 2; t++) {
        int fb = X[t].fb, tries = 0, built = 0; u64 seed = 0x1234567;
        while (!built && tries < 20) { tries++; built = build_xor(seed + (u64)tries, fb); }
        int fn = 0; u64 fp = 0;
        if (built) {
            for (int j = 0; j < N; j++) fn += !xor_query(keys[j], seed + (u64)tries, fb);
            for (u64 q = 0; q < Q; q++) fp += (u64)xor_query(keyof(q, 0), seed + (u64)tries, fb);
        }
        u64 bpk100 = (u64)(t ? 16 : 8) * (u64)CAP * 100 / N;   // bits per key x 100 (fingerprint width as in Table 3)
        int r = built && fn == 0 && in_range(fp, X[t].lo, X[t].hi) && bpk100 >= (t ? 1960u : 980u) && bpk100 <= (t ? 1980u : 990u);
        pstr(X[t].name); pstr(": built after "); pdec(tries); pstr(" attempt(s), false negatives "); pdec(fn); pstr(", false positives "); pdec((i64)fp); pstr(" of "); pdec(Q);
        pstr(" = "); ppct(fp); pstr(" %, bits per key "); pdec((i64)(bpk100 / 100)); pstr("."); pdec((i64)(bpk100 % 100 / 10)); pdec((i64)(bpk100 % 10)); pstr(r ? " in range\n" : " OUT OF RANGE\n");
        ok &= r;
    }
    static const struct { int bpk, k; u64 lo, hi; const char *name; } BL[3] = {{8, 6, 21000, 22700, "Bloom 8"}, {12, 8, 2900, 3600, "Bloom 12"}, {16, 11, 400, 600, "Bloom 16"}};
    for (int t = 0; t < 3; t++) {
        u64 fp; int k = NEG == 2 ? BL[t].k / 2 : BL[t].k; int fn = bloom_run(BL[t].bpk, k, &fp);
        int r = fn == 0 && in_range(fp, BL[t].lo, BL[t].hi);
        pstr(BL[t].name); pstr(" ("); pdec(k); pstr(" probes): false negatives "); pdec(fn); pstr(", false positives "); pdec((i64)fp); pstr(" of "); pdec(Q); pstr(" = "); ppct(fp); pstr(r ? " % in range\n" : " % OUT OF RANGE\n");
        ok &= r;
    }
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
