// CHAM from Koo, Roh, Kim, Jung, Lee, Kwon, "CHAM: A Family of Lightweight Block Ciphers for Resource-Constrained Devices"
// (ICISC 2017), written from the paper's description of the 4-branch generalized Feistel round and the key schedule.
// Checks the paper's three Appendix A test vectors (CHAM-64/128, CHAM-128/128, CHAM-128/256) and 27 more vectors (9 per
// variant) that the Crypto++ test-vector file labels "CHAM reference implementation", plus a decrypt round trip for each.
// Vectors are data copied as numbers, in the printed word order (key words K[0].., block words X[0]..X[3]); no code was copied.
#include "ounce_paper.h"

typedef struct { const char *name; int n, k, from_paper; const char *key, *pt, *ct; } vec;
// name, block bits, key bits, 1 if the entry is one of the paper's Appendix A vectors, key K[0].., plaintext X[0..3], ciphertext
static const vec VEC[] = {
    {"CHAM-64/128 paper vector", 64, 128, 1, "0100 0302 0504 0706 0908 0b0a 0d0c 0f0e", "1100 3322 5544 7766", "453c 63bc dcfa bf4e"},
    {"CHAM-64/128 reference impl #1", 64, 128, 0, "0277 0a9e a270 1fed 460c c269 9163 e519", "704a 4e91 eb9b 688d", "ceda d4dc 00e3 800d"},
    {"CHAM-64/128 reference impl #2", 64, 128, 0, "24cd a3e2 c167 92f3 40b6 0017 cabc 07c4", "115a 31e5 ee65 87f7", "e1af b96f 3079 4233"},
    {"CHAM-64/128 reference impl #3", 64, 128, 0, "0914 eef6 827c 21b9 c697 05ce b28b 7dd5", "e722 2e2b 0f2c ee49", "280d 763b 931b da81"},
    {"CHAM-64/128 reference impl #4", 64, 128, 0, "2fa2 fb02 1cd5 9124 ee27 1ec9 7307 6a13", "9fc0 8c64 f9f9 8163", "0277 8604 1b15 8cb9"},
    {"CHAM-64/128 reference impl #5", 64, 128, 0, "6035 af8d 6d97 6a47 1bc9 cb88 1a4a f2aa", "657b 5980 aad8 468b", "5684 2bf7 606d 67f8"},
    {"CHAM-64/128 reference impl #6", 64, 128, 0, "be3a cf23 eda6 9014 023e 098b 37c3 9b9e", "ff83 911e 2f35 84a5", "9263 2bf9 9819 783a"},
    {"CHAM-64/128 reference impl #7", 64, 128, 0, "ffde 8a15 21c5 fb5e b6b1 1ec9 05aa c629", "7802 c7a5 d52f 1868", "f810 fad0 80f0 19bd"},
    {"CHAM-64/128 reference impl #8", 64, 128, 0, "fc25 b83f 5058 9cb6 fe7a 5d6c 1635 5cfd", "1ab2 1791 a5d3 05aa", "e733 fd94 c357 d36c"},
    {"CHAM-64/128 reference impl #9", 64, 128, 0, "fae3 5e23 e357 3e33 4468 c725 80e5 4a6e", "5dc5 c55f 6b8d 31e2", "5bc2 2475 f93f 6cc8"},
    {"CHAM-128/128 paper vector", 128, 128, 1, "03020100 07060504 0b0a0908 0f0e0d0c", "33221100 77665544 bbaa9988 ffeeddcc", "c3746034 b55700c5 8d64ec32 489332f7"},
    {"CHAM-128/128 reference impl #1", 128, 128, 0, "a37beb01 15c49898 906f6f1c 73f68cf3", "463e4b34 efe3faa8 d8b74450 967f34d1", "30269e99 4d70c5de 7b0bc631 a96a1458"},
    {"CHAM-128/128 reference impl #2", 128, 128, 0, "ad97ae34 49a65961 46872c23 08a85a00", "7c241f66 85111245 83d76a6f 8498946b", "633b6cb9 00b390d8 d1bb84cb d84b9ccf"},
    {"CHAM-128/128 reference impl #3", 128, 128, 0, "012ddb51 d216a550 3b1632f3 69930aac", "440a7ce0 23b8499f 991482fd e6069cbe", "b2b21944 81c5becf 091d3c08 ee6d1749"},
    {"CHAM-128/128 reference impl #4", 128, 128, 0, "dfd8192b cab7764c 12632c23 95c96b55", "a72c2040 1e652249 6ceb83be d90a2816", "0416aae7 302a5219 cd20a3b8 6d879c22"},
    {"CHAM-128/128 reference impl #5", 128, 128, 0, "c3fe465e dff5a38c 308bcf68 a6d45ba7", "91167a4b b9641eb2 15195841 f3301521", "36d1dd06 e42c583d 9aea8e58 08ee2f3b"},
    {"CHAM-128/128 reference impl #6", 128, 128, 0, "e7365921 729a2e4b 5e9e2d42 6b53c079", "9fd5fb98 d2de3459 42cf3edb 2104e849", "60f3dd59 406e579e e45a2191 526c5693"},
    {"CHAM-128/128 reference impl #7", 128, 128, 0, "b880a315 e410aa2d 9d8686e4 ac033a6e", "05251c25 354ded83 67d50c4c 4a73b66f", "e3f879f5 8b41baf8 8b458da7 04343a03"},
    {"CHAM-128/128 reference impl #8", 128, 128, 0, "f0cb7d27 58d7ac44 937b882d 526fb9f8", "088ede84 315ef415 2e2e22b1 8b45e765", "4ff2532c 66a12b2e 869f476e aab2d53f"},
    {"CHAM-128/128 reference impl #9", 128, 128, 0, "28b841b2 9a5e552c e02170c8 fee72a87", "0fc61c4c fa1db413 9d007659 39df2ba9", "6fd504e7 091aca2e 32c88718 3e40b4a4"},
    {"CHAM-128/256 paper vector", 128, 256, 1, "03020100 07060504 0b0a0908 0f0e0d0c f3f2f1f0 f7f6f5f4 fbfaf9f8 fffefdfc", "33221100 77665544 bbaa9988 ffeeddcc", "a899c8a0 c929d55c ab670d38 0c4f7ac8"},
    {"CHAM-128/256 reference impl #1", 128, 256, 0, "4031c291 53a38799 8e0a6bad 6098a6c4 e4a852f8 7daf676e 873c3524 e1527db8", "aac76bc0 ec99e00e 9648a939 1a37c8db", "c993c682 1545b60c 456af36c b97628e7"},
    {"CHAM-128/256 reference impl #2", 128, 256, 0, "0c7be271 0ee365ff 061b8e43 5dbc63e3 52a08866 634223c9 8f4bcc4f a1223aee", "49eec4dd eb938769 a359a6bf f69353a9", "6252cf6f 6524f0ed 0b3a272a 33827bb4"},
    {"CHAM-128/256 reference impl #3", 128, 256, 0, "90c69c18 8fcac90f 7c061078 036f3279 5676641c 40358d9f d74867ca 5debd8fa", "c27e5d18 985bd57e 25b7164e 5acb6ceb", "c7c15e12 2287fd3d 45875c14 629a042c"},
    {"CHAM-128/256 reference impl #4", 128, 256, 0, "f7bae93e 170bbccd 42a1d993 a6247a9c ae609194 075045fc 95b22ad9 59e16c9c", "87cc88e7 9f159afe b2e8967b ff1ddd8b", "0887e541 4b68e67f a46d19ff 948290b6"},
    {"CHAM-128/256 reference impl #5", 128, 256, 0, "15747f3e 359c8462 151d0e6d f06abade 06f246e1 c817332b 1fa9102a 52263db4", "5c11eab6 3fa257df 7da90d0e 1bf46991", "3cabb22e 79c7ad8e d502abf8 74e7d3bf"},
    {"CHAM-128/256 reference impl #6", 128, 256, 0, "4c70d578 34042fbd 8f4b7c40 89ac864e 1dee8bac 4093f375 308aa073 655098f1", "68c7097e ab6c6043 87d2bc67 41dece87", "362a9742 dd8238a8 d916409a 4a3c11a1"},
    {"CHAM-128/256 reference impl #7", 128, 256, 0, "073359a0 5e54c5f8 882ef21f 01be08d5 7d5c5b87 533059a5 204e2bcd 5652dfc5", "ef94e8fb 7bf2aa7c ac73ef0a 294ad1f8", "5914594d db44ba25 ac0bfc05 1b92a9fe"},
    {"CHAM-128/256 reference impl #8", 128, 256, 0, "dc359e46 f5516d84 89885aee 191494fe 25350de0 69275496 1531e563 59f9e0ee", "d1c5fbd0 7d1e85fc 7922d741 6a5a44dc", "bbe95a67 06b38ff2 898dfda8 41fe29c7"},
    {"CHAM-128/256 reference impl #9", 128, 256, 0, "faf3682d cf6e656c 53bd8c06 de0f7f71 678c5a2d 34624762 d88daf37 21d5ad6c", "55b32441 7a787fbc 41b91ab2 9a5bf734", "2a7dab0b 6769e989 61578998 7e4be9a7"},
};
#define NVEC ((int)(sizeof VEC / sizeof VEC[0]))


#ifndef CHAM_ROT_A
#define CHAM_ROT_A 8   // rotation applied to the second branch in odd rounds (and the output rotation in even rounds)
#endif

static int hexwords(const char *s, int wbytes, u32 *out) {   // hex digits (spaces ignored) -> words; returns word count
    int n = 0, digits = 0; u32 cur = 0;
    for (; *s; s++) {
        int d;
        if (*s >= '0' && *s <= '9') d = *s - '0'; else if (*s >= 'a' && *s <= 'f') d = *s - 'a' + 10; else continue;
        cur = cur * 16 + (u32)d;
        if (++digits == 2 * wbytes) { out[n++] = cur; cur = 0; digits = 0; }
    }
    return n;
}
static u32 mask_of(int w) { return w == 32 ? 0xffffffffu : ((1u << w) - 1); }
static u32 rol(u32 x, int r, int w) { return ((x << r) | (x >> (w - r))) & mask_of(w); }

// n = block bits, k = key bits: w = n/4, KW = k/w, rounds 80 (CHAM-64/128, CHAM-128/128) or 96 (CHAM-128/256).
static void cham_keys(int w, int KW, const u32 *K, u32 *RK) {
    for (int i = 0; i < KW; i++) {
        RK[i] = K[i] ^ rol(K[i], 1, w) ^ rol(K[i], 8, w);
        RK[(i + KW) ^ 1] = K[i] ^ rol(K[i], 1, w) ^ rol(K[i], 11, w);
    }
}
static void cham_enc(int w, int KW, int R, const u32 *RK, u32 X[4]) {
    u32 mk = mask_of(w);
    for (int i = 0; i < R; i++) {
        u32 a = (i & 1) ? CHAM_ROT_A : 1, b = (i & 1) ? 1 : CHAM_ROT_A;
        u32 t = rol((((X[0] ^ (u32)i) + (rol(X[1], (int)a, w) ^ RK[i % (2 * KW)])) & mk), (int)b, w);
        X[0] = X[1]; X[1] = X[2]; X[2] = X[3]; X[3] = t;
    }
}
static void cham_dec(int w, int KW, int R, const u32 *RK, u32 X[4]) {
    u32 mk = mask_of(w);
    for (int i = R - 1; i >= 0; i--) {
        u32 a = (i & 1) ? CHAM_ROT_A : 1, b = (i & 1) ? 1 : CHAM_ROT_A;
        u32 t = X[3]; X[3] = X[2]; X[2] = X[1]; X[1] = X[0];
        u32 s = rol(t, w - (int)b, w);                       // undo the output rotation
        X[0] = ((s - (rol(X[1], (int)a, w) ^ RK[i % (2 * KW)])) & mk) ^ (u32)i;
    }
}

int paper_main(void) {
    int good = 0, dec_good = 0, paper_good = 0, paper_total = 0;
    for (int v = 0; v < NVEC; v++) {
        const vec *e = &VEC[v];
        int w = e->n / 4, KW = e->k / w, R = e->k == 256 ? 96 : 80, wb = w / 8;
        u32 K[8], RK[16], P[4], C[4];
        int kn = hexwords(e->key, wb, K), pn = hexwords(e->pt, wb, P); hexwords(e->ct, wb, C);
        if (kn != KW || pn != 4) { pstr("FAIL bad vector sizes\n"); return 1; }
        cham_keys(w, KW, K, RK);
        u32 X[4] = { P[0], P[1], P[2], P[3] };
        cham_enc(w, KW, R, RK, X);
        u8 got[16], want[16];
        for (int i = 0; i < 4; i++) for (int j = 0; j < wb; j++) {
            got[i * wb + j] = (u8)(X[i] >> (8 * (wb - 1 - j))); want[i * wb + j] = (u8)(C[i] >> (8 * (wb - 1 - j)));
        }
        int m = pcheck_bytes(e->name, got, want, 4 * wb);
        good += m;
        if (e->from_paper) { paper_total++; paper_good += m; }
        cham_dec(w, KW, R, RK, X);
        if (X[0] == P[0] && X[1] == P[1] && X[2] == P[2] && X[3] == P[3]) dec_good++;
    }
    pstr("paper's own vectors: "); pdec(paper_good); pstr(" of "); pdec(paper_total); pstr(" match\n");
    pstr("all vectors: "); pdec(good); pstr(" of "); pdec(NVEC); pstr(" match\n");
    pstr("decrypt round trips: "); pdec(dec_good); pstr(" of "); pdec(NVEC); pstr("\n");
    int ok = good == NVEC && dec_good == NVEC;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
