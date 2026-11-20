// Canonical Huffman codes and the fixed code / length / distance tables of DEFLATE, from RFC 1951 (Deutsch, "DEFLATE Compressed
// Data Format Specification version 1.3", 1996), sections 3.2.2, 3.2.5 and 3.2.6, written from the RFC's description. Checks:
// the worked example of 3.2.2 (alphabet ABCDEFGH, lengths 3,3,3,3,3,2,4,4: bl_count, next_code and the 8 codes), the (2,1,3,3)
// example, the fixed literal/length code of 3.2.6 (endpoints of the four published ranges, and completeness), the 29 length
// codes and 30 distance codes of 3.2.5 (tables built here by rule and compared with the RFC tables in tables.inc), and a
// decode round trip of all 288 fixed codes. Exit 0 only if everything matches.
#include "ounce_paper.h"
#include "tables.inc"

#ifndef NEG
#define NEG 0   // NEG=1: step 2 of the code assignment forgets the left shift
#endif
#define MAXB 15

// RFC 1951 3.2.2: assign canonical codes from bit lengths. Returns count; fills bl_count/next_code for inspection.
static void canon(const unsigned char *len, int n, unsigned *code, unsigned *bl_count, unsigned *next_code) {
    for (int i = 0; i <= MAXB; i++) bl_count[i] = 0;
    for (int i = 0; i < n; i++) bl_count[len[i]]++;
    bl_count[0] = 0;
    unsigned c = 0;
    for (int bits = 1; bits <= MAXB; bits++) { c = (c + bl_count[bits - 1]) << (NEG ? 0 : 1); next_code[bits] = c; }
    unsigned work[MAXB + 1];   // step 3 advances the counters; keep next_code as computed by step 2 for inspection
    for (int i = 0; i <= MAXB; i++) work[i] = next_code[i];
    for (int i = 0; i < n; i++) { if (len[i]) code[i] = work[len[i]]++; else code[i] = 0; }
}
static int bits_eq(unsigned code, int len, const char *s) {   // compare with a published bit string such as "0010111"
    int n = 0; while (s[n]) n++;
    if (n != len) return 0;
    for (int i = 0; i < n; i++) if (((code >> (n - 1 - i)) & 1) != (unsigned)(s[i] - '0')) return 0;
    return 1;
}
static unsigned parse_bits(const char *s) { unsigned v = 0; while (*s) v = v * 2 + (unsigned)(*s++ - '0'); return v; }

int paper_main(void) {
    int ok = 1;
    unsigned code[288], blc[MAXB + 1], nxt[MAXB + 1];
    // 3.2.2 worked example
    static const unsigned char abc[8] = {3, 3, 3, 3, 3, 2, 4, 4};
    static const char *pub[8] = {"010", "011", "100", "101", "110", "00", "1110", "1111"};
    canon(abc, 8, code, blc, nxt);
    int g = 0;
    g += blc[2] == 1; g += blc[3] == 5; g += blc[4] == 2;                       // bl_count table: N=2:1, N=3:5, N=4:2
    g += nxt[1] == 0; g += nxt[2] == 0; g += nxt[3] == 2; g += nxt[4] == 14;    // next_code table
    pstr("RFC 3.2.2 example ABCDEFGH: bl_count and next_code values matching "); pdec(g); pstr(" of 7\n"); ok &= g == 7;
    int c8 = 0;
    for (int i = 0; i < 8; i++) c8 += bits_eq(code[i], abc[i], pub[i]);
    pstr("RFC 3.2.2 example ABCDEFGH: codes matching "); pdec(c8); pstr(" of 8\n"); ok &= c8 == 8;
    // (2,1,3,3) -> A=10 B=0 C=110 D=111
    static const unsigned char abcd[4] = {2, 1, 3, 3};
    static const char *pub4[4] = {"10", "0", "110", "111"};
    canon(abcd, 4, code, blc, nxt);
    int c4 = 0; for (int i = 0; i < 4; i++) c4 += bits_eq(code[i], abcd[i], pub4[i]);
    pstr("RFC 3.2.2 example ABCD (2,1,3,3): codes matching "); pdec(c4); pstr(" of 4\n"); ok &= c4 == 4;

    // 3.2.6 fixed literal/length code
    unsigned char fl[288];
    for (int i = 0; i < 288; i++) fl[i] = i < 144 ? 8 : i < 256 ? 9 : i < 280 ? 7 : 8;
    canon(fl, 288, code, blc, nxt);
    static const struct { int lo, hi, len; const char *first, *last; } R[4] = {
        {0, 143, 8, "00110000", "10111111"}, {144, 255, 9, "110010000", "111111111"},
        {256, 279, 7, "0000000", "0010111"}, {280, 287, 8, "11000000", "11000111"}};
    int rg = 0;
    for (int r = 0; r < 4; r++) {
        int a = code[R[r].lo] == parse_bits(R[r].first) && fl[R[r].lo] == R[r].len;
        int b = code[R[r].hi] == parse_bits(R[r].last) && fl[R[r].hi] == R[r].len;
        int consecutive = 1; for (int i = R[r].lo; i < R[r].hi; i++) if (code[i + 1] != code[i] + 1) consecutive = 0;
        rg += a && b && consecutive;
    }
    pstr("RFC 3.2.6 fixed code: ranges with published first and last code (and consecutive) "); pdec(rg); pstr(" of 4\n"); ok &= rg == 4;
    unsigned kraft = 0; for (int i = 0; i < 288; i++) kraft += 1u << (9 - fl[i]);
    pstr("fixed code Kraft sum (units of 2^-9): "); pdec(kraft); pstr(kraft == 512 ? " (complete)\n" : " (not complete)\n"); ok &= kraft == 512;
    // decode round trip: read bits MSB first, canonical decoding by length (first code of each length)
    int rt = 0;
    for (int s = 0; s < 288; s++) {
        unsigned v = 0; int l = 0, found = -1;
        while (found < 0 && l < 9) {
            l++; v = (code[s] >> (fl[s] - l)) & ((1u << l) - 1);
            // symbols of length l occupy codes next_code_start[l] .. +count-1; look up by scanning
            for (int t = 0; t < 288; t++) if (fl[t] == l && code[t] == v) { found = t; break; }
        }
        rt += found == s && l == fl[s];
    }
    pstr("fixed code round trip (bit-by-bit decode of all 288 codes): "); pdec(rt); pstr(" of 288\n"); ok &= rt == 288;

    // 3.2.5 length codes 257..285 and distance codes 0..29, built by rule
    int lt = 0; unsigned lo = 3;
    for (int c = 0; c < 29; c++) {
        unsigned extra = c < 8 ? 0 : c == 28 ? 0 : (unsigned)((c - 4) / 4);
        unsigned hi = lo + (1u << extra) - 1;
        if (c == 27) hi = 257;                     // length 258 has its own code, 285
        if (c == 28) { lo = 258; hi = 258; }
        lt += LENT[c][0] == extra && LENT[c][1] == lo && LENT[c][2] == hi;
        lo = hi + 1;
    }
    pstr("RFC 3.2.5 length codes (extra bits and range) matching: "); pdec(lt); pstr(" of 29\n"); ok &= lt == 29;
    int dt = 0; unsigned dlo = 1;
    for (int c = 0; c < 30; c++) {
        unsigned extra = c < 4 ? 0 : (unsigned)((c - 2) / 2);
        unsigned hi = dlo + (1u << extra) - 1;
        dt += DIST[c][0] == extra && DIST[c][1] == dlo && DIST[c][2] == hi;
        dlo = hi + 1;
    }
    pstr("RFC 3.2.5 distance codes (extra bits and range) matching: "); pdec(dt); pstr(" of 30\n"); ok &= dt == 30;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
