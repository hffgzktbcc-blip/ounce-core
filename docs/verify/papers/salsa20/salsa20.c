// Salsa20 from D. J. Bernstein, "Salsa20 specification" (2005; cr.yp.to/snuffle/spec.pdf), written from the specification's
// definitions. Checks every worked example in the specification: quarterround (7), rowround (2), columnround (2),
// doubleround (2), littleendian (3 values), the Salsa20 hash (4, including the one-million-fold iterate) and the expansion
// function (2). The example values are data parsed from the fetched PDF text and copied as numbers; none of the author's
// code was copied. Exit 0 only if all 20 example groups match (the 3 littleendian values count as one).
#include "ounce_paper.h"

static const u32 QR[][2][4] = {
  {{0x00000000,0x00000000,0x00000000,0x00000000},{0x00000000,0x00000000,0x00000000,0x00000000}},
  {{0x00000001,0x00000000,0x00000000,0x00000000},{0x08008145,0x00000080,0x00010200,0x20500000}},
  {{0x00000000,0x00000001,0x00000000,0x00000000},{0x88000100,0x00000001,0x00000200,0x00402000}},
  {{0x00000000,0x00000000,0x00000001,0x00000000},{0x80040000,0x00000000,0x00000001,0x00002000}},
  {{0x00000000,0x00000000,0x00000000,0x00000001},{0x00048044,0x00000080,0x00010000,0x20100001}},
  {{0xe7e8c006,0xc4f9417d,0x6479b4b2,0x68c67137},{0xe876d72b,0x9361dfd5,0xf1460244,0x948541a3}},
  {{0xd3917c5b,0x55f1c407,0x52a58a7a,0x8f887a3b},{0x3e2f308c,0xd90a8f36,0x6ab2a923,0x2883524c}},
};

static const u32 ROW[][2][16] = {
  {{0x00000001,0x00000000,0x00000000,0x00000000,0x00000001,0x00000000,0x00000000,0x00000000,0x00000001,0x00000000,0x00000000,0x00000000,0x00000001,0x00000000,0x00000000,0x00000000},{0x08008145,0x00000080,0x00010200,0x20500000,0x20100001,0x00048044,0x00000080,0x00010000,0x00000001,0x00002000,0x80040000,0x00000000,0x00000001,0x00000200,0x00402000,0x88000100}},
  {{0x08521bd6,0x1fe88837,0xbb2aa576,0x3aa26365,0xc54c6a5b,0x2fc74c2f,0x6dd39cc3,0xda0a64f6,0x90a2f23d,0x067f95a6,0x06b35f61,0x41e4732e,0xe859c100,0xea4d84b7,0x0f619bff,0xbc6e965a},{0xa890d39d,0x65d71596,0xe9487daa,0xc8ca6a86,0x949d2192,0x764b7754,0xe408d9b9,0x7a41b4d1,0x3402e183,0x3c3af432,0x50669f96,0xd89ef0a8,0x0040ede5,0xb545fbce,0xd257ed4f,0x1818882d}},
};

static const u32 COL[][2][16] = {
  {{0x00000001,0x00000000,0x00000000,0x00000000,0x00000001,0x00000000,0x00000000,0x00000000,0x00000001,0x00000000,0x00000000,0x00000000,0x00000001,0x00000000,0x00000000,0x00000000},{0x10090288,0x00000000,0x00000000,0x00000000,0x00000101,0x00000000,0x00000000,0x00000000,0x00020401,0x00000000,0x00000000,0x00000000,0x40a04001,0x00000000,0x00000000,0x00000000}},
  {{0x08521bd6,0x1fe88837,0xbb2aa576,0x3aa26365,0xc54c6a5b,0x2fc74c2f,0x6dd39cc3,0xda0a64f6,0x90a2f23d,0x067f95a6,0x06b35f61,0x41e4732e,0xe859c100,0xea4d84b7,0x0f619bff,0xbc6e965a},{0x8c9d190a,0xce8e4c90,0x1ef8e9d3,0x1326a71a,0x90a20123,0xead3c4f3,0x63a091a0,0xf0708d69,0x789b010c,0xd195a681,0xeb7d5504,0xa774135c,0x481c2027,0x53a8e4b5,0x4c1f89c5,0x3f78c9c8}},
};

static const u32 DBL[][2][16] = {
  {{0x00000001,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000,0x00000000},{0x8186a22d,0x0040a284,0x82479210,0x06929051,0x08000090,0x02402200,0x00004000,0x00800000,0x00010200,0x20400000,0x08008104,0x00000000,0x20500000,0xa0000040,0x0008180a,0x612a8020}},
  {{0xde501066,0x6f9eb8f7,0xe4fbbd9b,0x454e3f57,0xb75540d3,0x43e93a4c,0x3a6f2aa0,0x726d6b36,0x9243f484,0x9145d1e8,0x4fa9d247,0xdc8dee11,0x054bf545,0x254dd653,0xd9421b6d,0x67b276c1},{0xccaaf672,0x23d960f7,0x9153e63a,0xcd9a60d0,0x50440492,0xf07cad19,0xae344aa0,0xdf4cfdfc,0xca531c29,0x8e7943db,0xac1680cd,0xd503ca00,0xa74b2ad6,0xbc331c5c,0x1dda24c7,0xee928277}},
};

static const u8 H0_IN[64]={0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0};
static const u8 H0_OUT[64]={0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0};
static const u8 H1_IN[64]={211,159,13,115,76,55,82,183,3,117,222,37,191,187,234,136,49,237,179,48,1,106,178,219,175,199,166,48,86,16,179,207,31,240,32,63,15,83,93,161,116,147,48,113,238,55,204,36,79,201,235,79,3,81,156,47,203,26,244,243,88,118,104,54};
static const u8 H1_OUT[64]={109,42,178,168,156,240,248,238,168,196,190,203,26,110,170,154,29,29,150,26,150,30,235,249,190,163,251,48,69,144,51,57,118,40,152,157,180,57,27,94,107,42,236,35,27,111,114,114,219,236,232,135,111,155,110,18,24,232,95,158,179,19,48,202};
static const u8 H2_IN[64]={88,118,104,54,79,201,235,79,3,81,156,47,203,26,244,243,191,187,234,136,211,159,13,115,76,55,82,183,3,117,222,37,86,16,179,207,49,237,179,48,1,106,178,219,175,199,166,48,238,55,204,36,31,240,32,63,15,83,93,161,116,147,48,113};
static const u8 H2_OUT[64]={179,19,48,202,219,236,232,135,111,155,110,18,24,232,95,158,26,110,170,154,109,42,178,168,156,240,248,238,168,196,190,203,69,144,51,57,29,29,150,26,150,30,235,249,190,163,251,48,27,111,114,114,118,40,152,157,180,57,27,94,107,42,236,35};
static const u8 HM_IN[64]={6,124,83,146,38,191,9,50,4,161,47,222,122,182,223,185,75,27,0,216,16,122,7,89,162,104,101,147,213,21,54,95,225,253,139,176,105,132,23,116,76,41,176,207,221,34,157,108,94,94,99,52,90,117,91,220,146,190,239,143,196,176,130,186};
static const u8 HM_OUT[64]={8,18,38,199,119,76,215,67,173,127,144,162,103,212,176,217,192,19,233,33,159,197,154,160,128,243,219,65,171,136,135,225,123,11,68,86,237,82,20,155,133,189,9,83,167,116,194,78,122,127,195,185,185,204,188,90,245,9,183,248,226,85,245,104};
static const u8 X0_IN[64]={101,120,112,97,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,110,100,32,51,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,50,45,98,121,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,116,101,32,107};
static const u8 X0_OUT[64]={69,37,68,39,41,15,107,193,255,139,122,6,170,233,217,98,89,144,182,106,21,51,200,65,239,49,222,34,215,114,40,126,104,197,7,225,197,153,31,2,102,78,76,176,84,245,246,184,177,160,133,130,6,72,149,119,192,195,132,236,234,103,246,74};
static const u8 X1_IN[64]={101,120,112,97,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,110,100,32,49,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,54,45,98,121,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,116,101,32,107};
static const u8 X1_OUT[64]={39,173,46,248,30,200,82,17,48,67,254,239,37,18,13,247,241,200,61,144,10,55,50,185,6,47,246,253,143,86,187,225,134,85,110,246,161,163,43,235,231,94,171,51,145,214,112,29,14,232,5,16,151,140,183,141,171,9,122,181,104,182,177,193};



#ifndef S_ROUNDS
#define S_ROUNDS 10          // doubleround count; Salsa20/20 is 10 double rounds
#endif
#ifndef S_ROT1
#define S_ROT1 7
#endif
#define ROTL32(x, b) (((x) << (b)) | ((x) >> (32 - (b))))

#define QR(a, b, c, d) do { b ^= ROTL32(a + d, S_ROT1); c ^= ROTL32(b + a, 9); d ^= ROTL32(c + b, 13); a ^= ROTL32(d + c, 18); } while (0)
static void quarterround(u32 *y0, u32 *y1, u32 *y2, u32 *y3) { u32 a = *y0, b = *y1, c = *y2, d = *y3; QR(a, b, c, d); *y0 = a; *y1 = b; *y2 = c; *y3 = d; }
// the spec's quarterround on (y0,y1,y2,y3): z1 = y1 ^ ((y0+y3)<<<7), z2 = y2 ^ ((z1+y0)<<<9), z3 = y3 ^ ((z2+z1)<<<13), z0 = y0 ^ ((z3+z2)<<<18)
static void rowround(u32 y[16]) {
    quarterround(&y[0], &y[1], &y[2], &y[3]);
    quarterround(&y[5], &y[6], &y[7], &y[4]);
    quarterround(&y[10], &y[11], &y[8], &y[9]);
    quarterround(&y[15], &y[12], &y[13], &y[14]);
}
static void columnround(u32 x[16]) {
    quarterround(&x[0], &x[4], &x[8], &x[12]);
    quarterround(&x[5], &x[9], &x[13], &x[1]);
    quarterround(&x[10], &x[14], &x[2], &x[6]);
    quarterround(&x[15], &x[3], &x[7], &x[11]);
}
static void doubleround(u32 x[16]) { columnround(x); rowround(x); }
// same thing on registers, for the one-million-fold iterate: Salsa20(x) = x + doubleround^10(x) on 16 words
static void hash_words(u32 x[16]) {
    u32 x0=x[0],x1=x[1],x2=x[2],x3=x[3],x4=x[4],x5=x[5],x6=x[6],x7=x[7],x8=x[8],x9=x[9],x10=x[10],x11=x[11],x12=x[12],x13=x[13],x14=x[14],x15=x[15];
    for (int i = 0; i < S_ROUNDS; i++) {
        QR(x0, x4, x8, x12); QR(x5, x9, x13, x1); QR(x10, x14, x2, x6); QR(x15, x3, x7, x11);
        QR(x0, x1, x2, x3); QR(x5, x6, x7, x4); QR(x10, x11, x8, x9); QR(x15, x12, x13, x14);
    }
    x[0]+=x0;x[1]+=x1;x[2]+=x2;x[3]+=x3;x[4]+=x4;x[5]+=x5;x[6]+=x6;x[7]+=x7;x[8]+=x8;x[9]+=x9;x[10]+=x10;x[11]+=x11;x[12]+=x12;x[13]+=x13;x[14]+=x14;x[15]+=x15;
}
static u32 littleendian(u8 b0, u8 b1, u8 b2, u8 b3) { return (u32)b0 | (u32)b1 << 8 | (u32)b2 << 16 | (u32)b3 << 24; }

static void salsa20_hash(const u8 in[64], u8 out[64]) {
    u32 x[16], z[16];
    for (int i = 0; i < 16; i++) x[i] = z[i] = littleendian(in[4*i], in[4*i+1], in[4*i+2], in[4*i+3]);
    for (int i = 0; i < S_ROUNDS; i++) doubleround(z);
    for (int i = 0; i < 16; i++) { u32 w = z[i] + x[i]; for (int j = 0; j < 4; j++) out[4*i+j] = (u8)(w >> (8*j)); }
}
// Salsa20_{k0,k1}(n) for 32-byte keys, Salsa20_k(n) for 16-byte keys (section 9)
static void expansion(const u8 *k, int klen, const u8 n[16], u8 out[64]) {
    static const u8 sigma[4][4] = {{101,120,112,97},{110,100,32,51},{50,45,98,121},{116,101,32,107}};
    static const u8 tau[4][4]   = {{101,120,112,97},{110,100,32,49},{54,45,98,121},{116,101,32,107}};
    const u8 (*c)[4] = klen == 32 ? sigma : tau;
    const u8 *k1 = klen == 32 ? k + 16 : k;
    u8 in[64];
    for (int i = 0; i < 4; i++) { in[i] = c[0][i]; in[20+i] = c[1][i]; in[40+i] = c[2][i]; in[60+i] = c[3][i]; }
    for (int i = 0; i < 16; i++) { in[4+i] = k[i]; in[24+i] = n[i]; in[44+i] = k1[i]; }
    salsa20_hash(in, out);
}

static int words_eq(const u32 *a, const u32 *b, int n) { for (int i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }
static int bytes_eq(const u8 *a, const u8 *b, int n) { for (int i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }
static int report(const char *name, int idx, int ok) {
    pstr(ok ? "PASS " : "FAIL "); pstr(name); pstr(" example "); pdec(idx); pstr("\n"); return ok;
}

int paper_main(void) {
    int ok = 1, good = 0, total = 0;
    for (int i = 0; i < 7; i++) {
        u32 y[4]; for (int j = 0; j < 4; j++) y[j] = QR[i][0][j];
        quarterround(&y[0], &y[1], &y[2], &y[3]);
        int m = words_eq(y, QR[i][1], 4); good += report("quarterround", i + 1, m); total++;
    }
    for (int i = 0; i < 2; i++) {
        u32 y[16]; for (int j = 0; j < 16; j++) y[j] = ROW[i][0][j];
        rowround(y); int m = words_eq(y, ROW[i][1], 16); good += report("rowround", i + 1, m); total++;
    }
    for (int i = 0; i < 2; i++) {
        u32 y[16]; for (int j = 0; j < 16; j++) y[j] = COL[i][0][j];
        columnround(y); int m = words_eq(y, COL[i][1], 16); good += report("columnround", i + 1, m); total++;
    }
    for (int i = 0; i < 2; i++) {
        u32 y[16]; for (int j = 0; j < 16; j++) y[j] = DBL[i][0][j];
        doubleround(y); int m = words_eq(y, DBL[i][1], 16); good += report("doubleround", i + 1, m); total++;
    }
    {   int m = littleendian(0,0,0,0) == 0x00000000u && littleendian(86,75,30,9) == 0x091e4b56u
             && littleendian(255,255,255,250) == 0xfaffffffu;
        good += m; total++; pstr(m ? "PASS" : "FAIL"); pstr(" littleendian examples (3 values)\n"); }
    {   u8 o[64]; salsa20_hash(H0_IN, o); int m = bytes_eq(o, H0_OUT, 64); good += report("Salsa20 hash", 1, m); total++; }
    {   u8 o[64]; salsa20_hash(H1_IN, o); int m = bytes_eq(o, H1_OUT, 64); good += report("Salsa20 hash", 2, m); total++; }
    {   u8 o[64]; salsa20_hash(H2_IN, o); int m = bytes_eq(o, H2_OUT, 64); good += report("Salsa20 hash", 3, m); total++; }
    {   u32 w[16]; for (int i = 0; i < 16; i++) w[i] = littleendian(HM_IN[4*i], HM_IN[4*i+1], HM_IN[4*i+2], HM_IN[4*i+3]);
        for (int r = 0; r < 1000000; r++) hash_words(w);
        u8 a[64]; for (int i = 0; i < 16; i++) for (int j = 0; j < 4; j++) a[4*i+j] = (u8)(w[i] >> (8*j));
        int m = bytes_eq(a, HM_OUT, 64); good += report("Salsa20^1000000 hash", 1, m); total++; }
    {   u8 k[32], n[16], o[64];
        for (int i = 0; i < 16; i++) { k[i] = (u8)(1 + i); k[16+i] = (u8)(201 + i); n[i] = (u8)(101 + i); }
        expansion(k, 32, n, o); int m = bytes_eq(o, X0_OUT, 64); good += report("expansion (32-byte key)", 1, m); total++; }
    {   u8 k[16], n[16], o[64];
        for (int i = 0; i < 16; i++) { k[i] = (u8)(1 + i); n[i] = (u8)(101 + i); }
        expansion(k, 16, n, o); int m = bytes_eq(o, X1_OUT, 64); good += report("expansion (16-byte key)", 1, m); total++; }
    pstr("spec examples: "); pdec(good); pstr(" of "); pdec(total); pstr(" match\n");
    ok = good == total && total == 20;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
