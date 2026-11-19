/* elf-workload compress: canonical Huffman coding with Kraft-sum and round-trip checks,
 * Burrows-Wheeler Transform + Move-To-Front forward/inverse round-trip, and Elias gamma coding.
 * Provenance: original work written for ounce-core (MIT); textbook compression algorithms. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* ---- Canonical Huffman ---- */
typedef struct { uint32_t freq; int left, right, sym, depth; } hnode;

static int hpop(const hnode *nodes, int *heap, int *hlen) {
    int top = heap[0];
    heap[0] = heap[--(*hlen)];
    int i = 0;
    for (;;) {
        int c = 2 * i + 1;
        if (c >= *hlen) break;
        if (c + 1 < *hlen && nodes[heap[c + 1]].freq < nodes[heap[c]].freq) c++;
        if (nodes[heap[c]].freq >= nodes[heap[i]].freq) break;
        int t = heap[i]; heap[i] = heap[c]; heap[c] = t; i = c;
    }
    return top;
}

static int test_canonical_huffman(FILE *o) {
    int fails = 0;
    const char *msg =
        "the quick brown fox jumps over the lazy dog; "
        "pack my box with five dozen liquor jugs; "
        "sphinx of black quartz, judge my vow! 0123456789 0011223344";
    const int n = (int)strlen(msg);
    uint32_t freq[256] = {0};
    for (int i = 0; i < n; i++) freq[(unsigned char)msg[i]]++;

    hnode nodes[512];
    int n_nodes = 0;
    int heap[256], hlen = 0;
    for (int c = 0; c < 256; c++) {
        if (!freq[c]) continue;
        int id = n_nodes++;
        nodes[id] = (hnode){freq[c], -1, -1, c, 0};
        int idx = hlen++;
        heap[idx] = id;
        while (idx > 0) {
            int p = (idx - 1) / 2;
            if (nodes[heap[p]].freq <= nodes[heap[idx]].freq) break;
            int tmp = heap[p]; heap[p] = heap[idx]; heap[idx] = tmp;
            idx = p;
        }
    }

    while (hlen > 1) {
        int a = hpop(nodes, heap, &hlen);
        int b = hpop(nodes, heap, &hlen);
        int id = n_nodes++;
        nodes[id] = (hnode){nodes[a].freq + nodes[b].freq, a, b, -1, 0};
        int idx = hlen++;
        heap[idx] = id;
        while (idx > 0) {
            int p = (idx - 1) / 2;
            if (nodes[heap[p]].freq <= nodes[heap[idx]].freq) break;
            int t = heap[p]; heap[p] = heap[idx]; heap[idx] = t; idx = p;
        }
    }
    if (hlen == 0) return 1;

    int root = heap[0];
    int len[256] = {0};
    int stk[512], sp = 0;
    nodes[root].depth = 0;
    stk[sp++] = root;
    int max_len = 0;
    while (sp) {
        int u = stk[--sp];
        if (nodes[u].sym >= 0) {
            len[nodes[u].sym] = nodes[u].depth ? nodes[u].depth : 1;
            if (len[nodes[u].sym] > max_len) max_len = len[nodes[u].sym];
        } else {
            int l = nodes[u].left, r = nodes[u].right;
            nodes[l].depth = nodes[u].depth + 1;
            nodes[r].depth = nodes[u].depth + 1;
            stk[sp++] = l;
            stk[sp++] = r;
        }
    }

    int bl_count[32] = {0};
    uint32_t kraft = 0;
    for (int c = 0; c < 256; c++) {
        if (len[c]) {
            bl_count[len[c]]++;
            kraft += 1u << (max_len - len[c]);
        }
    }
    if (kraft != (1u << max_len)) fails++;

    uint32_t next_code[32] = {0}, code = 0;
    for (int bits = 1; bits <= max_len; bits++) {
        code = (code + (uint32_t)bl_count[bits - 1]) << 1;
        next_code[bits] = code;
    }
    uint32_t sym_code[256] = {0};
    for (int c = 0; c < 256; c++) {
        if (len[c]) sym_code[c] = next_code[len[c]]++;
    }

    uint8_t bits_buf[512] = {0};
    int total_bits = 0;
    for (int i = 0; i < n; i++) {
        int c = (unsigned char)msg[i];
        int l = len[c];
        uint32_t sc = sym_code[c];
        for (int b = l - 1; b >= 0; b--) {
            if ((sc >> b) & 1u) bits_buf[total_bits >> 3] |= (uint8_t)(1u << (7 - (total_bits & 7)));
            total_bits++;
        }
    }

    /* Decode using canonical lookup */
    char decoded[256];
    int dec_len = 0, bit_pos = 0;
    while (bit_pos < total_bits) {
        uint32_t cur = 0;
        int matched = -1;
        for (int l = 1; l <= max_len && bit_pos < total_bits; l++) {
            uint32_t bit = (bits_buf[bit_pos >> 3] >> (7 - (bit_pos & 7))) & 1u;
            bit_pos++;
            cur = (cur << 1) | bit;
            for (int c = 0; c < 256; c++) {
                if (len[c] == l && sym_code[c] == cur) { matched = c; break; }
            }
            if (matched >= 0) break;
        }
        if (matched < 0 || dec_len >= n) { fails++; break; }
        decoded[dec_len++] = (char)matched;
    }
    decoded[dec_len] = 0;
    if (dec_len != n || memcmp(decoded, msg, (size_t)n) != 0) fails++;
    fprintf(o, "huffman: bytes=%d bits=%d max_len=%d kraft_ok=%d rt=%d\n",
            n, total_bits, max_len, kraft == (1u << max_len),
            (dec_len == n && memcmp(decoded, msg, (size_t)n) == 0));
    return fails;
}

/* ---- Burrows-Wheeler Transform + Move-To-Front ---- */
static const uint8_t *g_bwt_buf;
static int g_bwt_n;

static int cmp_rot(const void *pa, const void *pb) {
    int a = *(const int *)pa, b = *(const int *)pb;
    for (int k = 0; k < g_bwt_n; k++) {
        uint8_t ca = g_bwt_buf[(a + k) % g_bwt_n];
        uint8_t cb = g_bwt_buf[(b + k) % g_bwt_n];
        if (ca != cb) return (ca < cb) ? -1 : 1;
    }
    return (a < b) ? -1 : (a > b);
}

static int bwt_mtf_roundtrip(const uint8_t *in, int n, uint32_t *out_mtf_sum) {
    int idx[256];
    for (int i = 0; i < n; i++) idx[i] = i;
    g_bwt_buf = in;
    g_bwt_n = n;
    qsort(idx, (size_t)n, sizeof idx[0], cmp_rot);

    uint8_t bwt[256];
    int primary = -1;
    for (int i = 0; i < n; i++) {
        if (idx[i] == 0) primary = i;
        bwt[i] = in[(idx[i] + n - 1) % n];
    }

    /* MTF encode */
    uint8_t tbl[256], mtf[256];
    for (int i = 0; i < 256; i++) tbl[i] = (uint8_t)i;
    uint32_t msum = 0;
    for (int i = 0; i < n; i++) {
        uint8_t pos = 0;
        while (tbl[pos] != bwt[i]) pos++;
        mtf[i] = pos;
        msum += pos;
        uint8_t val = tbl[pos];
        for (int k = pos; k > 0; k--) tbl[k] = tbl[k - 1];
        tbl[0] = val;
    }
    *out_mtf_sum = msum;

    /* MTF decode */
    uint8_t bwt_dec[256];
    for (int i = 0; i < 256; i++) tbl[i] = (uint8_t)i;
    for (int i = 0; i < n; i++) {
        uint8_t pos = mtf[i], val = tbl[pos];
        bwt_dec[i] = val;
        for (int k = pos; k > 0; k--) tbl[k] = tbl[k - 1];
        tbl[0] = val;
    }
    if (memcmp(bwt, bwt_dec, (size_t)n) != 0) return 0;

    /* Inverse BWT via LF-mapping */
    int count[256] = {0}, occ[256];
    for (int i = 0; i < n; i++) {
        occ[i] = count[bwt_dec[i]]++;
    }
    int start[256], acc = 0;
    for (int c = 0; c < 256; c++) { start[c] = acc; acc += count[c]; }

    uint8_t orig[256];
    int p = primary;
    for (int i = n - 1; i >= 0; i--) {
        orig[i] = bwt_dec[p];
        p = start[bwt_dec[p]] + occ[p];
    }
    return memcmp(orig, in, (size_t)n) == 0;
}

static int test_bwt_mtf(FILE *o) {
    int fails = 0;
    const uint8_t s1[] = "banana$";
    const uint8_t s2[] = "abracadabra_mississippi_abracadabra";
    uint8_t s3[128];
    uint32_t rng = 0x55aa3311u;
    for (int i = 0; i < 128; i++) {
        rng = rng * 1664525u + 1013904223u;
        s3[i] = (uint8_t)('A' + ((rng >> 24) % 6));
    }
    uint32_t sum1 = 0, sum2 = 0, sum3 = 0;
    int ok1 = bwt_mtf_roundtrip(s1, 7, &sum1);
    int ok2 = bwt_mtf_roundtrip(s2, (int)sizeof s2 - 1, &sum2);
    int ok3 = bwt_mtf_roundtrip(s3, 128, &sum3);
    if (!ok1 || !ok2 || !ok3) fails++;
    fprintf(o, "bwt_mtf: ok=%d%d%d mtf_sums=(%u,%u,%u)\n", ok1, ok2, ok3, sum1, sum2, sum3);
    return fails;
}

/* ---- Elias gamma coding ---- */
static int test_elias_gamma(FILE *o) {
    int fails = 0;
    uint32_t vals[320];
    int nv = 0;
    for (uint32_t x = 1; x <= 300; x++) vals[nv++] = x;
    for (int k = 0; k <= 16; k++) vals[nv++] = 1u << k;

    uint8_t buf[2048] = {0};
    int bit_pos = 0, expected_bits = 0;
    for (int i = 0; i < nv; i++) {
        uint32_t v = vals[i];
        int k = 0;
        while ((1u << (k + 1)) <= v) k++;
        expected_bits += 2 * k + 1;
        bit_pos += k; /* k leading zero bits */
        for (int b = k; b >= 0; b--) {
            if ((v >> b) & 1u) buf[bit_pos >> 3] |= (uint8_t)(1u << (7 - (bit_pos & 7)));
            bit_pos++;
        }
    }
    if (bit_pos != expected_bits) fails++;

    int read_pos = 0, dec_ok = 1;
    for (int i = 0; i < nv; i++) {
        int zeros = 0;
        while (read_pos < bit_pos && ((buf[read_pos >> 3] >> (7 - (read_pos & 7))) & 1u) == 0) {
            zeros++;
            read_pos++;
        }
        uint32_t v = 0;
        for (int b = 0; b <= zeros; b++) {
            uint32_t bit = (buf[read_pos >> 3] >> (7 - (read_pos & 7))) & 1u;
            read_pos++;
            v = (v << 1) | bit;
        }
        if (v != vals[i]) dec_ok = 0;
    }
    if (read_pos != bit_pos || !dec_ok) fails++;
    fprintf(o, "elias_gamma: count=%d bits=%d expected=%d ok=%d\n", nv, bit_pos, expected_bits, dec_ok);
    return fails;
}

int workload_run(FILE *o) {
    int fails = 0;
    fails += test_canonical_huffman(o);
    fails += test_bwt_mtf(o);
    fails += test_elias_gamma(o);
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
