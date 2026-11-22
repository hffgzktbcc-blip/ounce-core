// HPACK header compression from RFC 7541 (Peon and Ruellan, "HPACK: Header Compression for HTTP/2", 2015), written from
// the RFC's description: prefix-coded integers (5.1), string literals with optional Huffman coding (5.2), the indexed / literal
// representations (6), and the dynamic table with size accounting and eviction (4). Check: the 16 examples of Appendix C
// (C.2.1-C.2.4 representation examples, C.3 and C.4 three requests without and with Huffman coding, C.5 and C.6 three
// responses with a 256-octet table so that evictions happen). For each, the program's encoder must produce the published hex
// bytes, its decoder must turn those bytes into the published header list, and the dynamic table afterwards must equal the
// published one (entries with their sizes, and the total). The Huffman code of Appendix B (257 codes) is data in tables.inc,
// generated from the RFC text; it is also checked to be prefix-free and complete. Exit 0 only if everything matches.
#include "ounce_paper.h"
#include "tables.inc"

#ifndef NEG
#define NEG 0   // NEG=1: entry size overhead 31 instead of 32; NEG=2: Huffman padding with zero bits instead of ones
#endif
#define OVERHEAD (NEG == 1 ? 31 : 32)

static unsigned slen(const char *s) { unsigned n = 0; while (s[n]) n++; return n; }
static int seq(const char *a, unsigned an, const char *b, unsigned bn) { if (an != bn) return 0; for (unsigned i = 0; i < an; i++) if (a[i] != b[i]) return 0; return 1; }

// ---- dynamic table (RFC section 4): newest entry first, size = sum(len(name)+len(value)+32), evict oldest while over the limit
struct Ent { char n[48], v[96]; int nl, vl; };
struct Table { struct Ent e[64]; int cnt, size, max; };
static void tinit(struct Table *t, int max) { t->cnt = 0; t->size = 0; t->max = max; }
static void tadd(struct Table *t, const char *n, int nl, const char *v, int vl) {
    int sz = nl + vl + OVERHEAD;
    while (t->cnt && t->size + sz > t->max) { t->size -= t->e[t->cnt - 1].nl + t->e[t->cnt - 1].vl + OVERHEAD; t->cnt--; }
    if (sz > t->max) return;                                   // larger than the whole table: table stays empty
    for (int i = t->cnt; i > 0; i--) t->e[i] = t->e[i - 1];
    struct Ent *e = &t->e[0]; e->nl = nl; e->vl = vl;
    for (int i = 0; i < nl; i++) e->n[i] = n[i];
    for (int i = 0; i < vl; i++) e->v[i] = v[i];
    t->cnt++; t->size += sz;
}
// index 1..61 static, 62.. dynamic
static int idx_get(const struct Table *t, int idx, const char **n, int *nl, const char **v, int *vl) {
    if (idx >= 1 && idx <= 61) { *n = STATIC_NAME[idx]; *nl = (int)slen(*n); *v = STATIC_VAL[idx]; *vl = (int)slen(*v); return 1; }
    int d = idx - 62; if (d < 0 || d >= t->cnt) return 0;
    *n = t->e[d].n; *nl = t->e[d].nl; *v = t->e[d].v; *vl = t->e[d].vl; return 1;
}

// ---- output buffer
static u8 out[512]; static int olen;
static void put(u8 b) { out[olen++] = b; }
// 5.1 integer with an N-bit prefix; `flags` are the high bits of the first octet
static void enc_int(int prefix, u8 flags, u32 v) {
    u32 max = (1u << prefix) - 1;
    if (v < max) { put((u8)(flags | v)); return; }
    put((u8)(flags | max)); v -= max;
    while (v >= 128) { put((u8)(v % 128 + 128)); v /= 128; }
    put((u8)v);
}
// 5.2 Huffman encoding of a string, padded with the most significant bits of EOS (ones)
static int huff_enc(const char *s, int n, u8 *o) {
    u64 acc = 0; int bits = 0, len = 0;
    for (int i = 0; i < n; i++) {
        u8 c = (u8)s[i]; acc = (acc << HLEN[c]) | HCODE[c]; bits += HLEN[c];
        while (bits >= 8) { o[len++] = (u8)(acc >> (bits - 8)); bits -= 8; }
    }
    if (bits) o[len++] = (u8)((acc << (8 - bits)) | (NEG == 2 ? 0 : ((1u << (8 - bits)) - 1)));
    return len;
}
static void enc_str(const char *s, int n, int huff) {
    if (huff) { u8 t[256]; int hl = huff_enc(s, n, t); enc_int(7, 0x80, (u32)hl); for (int i = 0; i < hl; i++) put(t[i]); }
    else { enc_int(7, 0x00, (u32)n); for (int i = 0; i < n; i++) put((u8)s[i]); }
}
// Policy used by the examples C.3 to C.6: indexed if the whole field is in a table, else literal with incremental indexing.
static void enc_field(struct Table *t, const char *n, int nl, const char *v, int vl, int huff) {
    int full = 0, name = 0;
    for (int i = 1; i <= 61 + t->cnt; i++) {
        const char *a, *b; int al, bl; idx_get(t, i, &a, &al, &b, &bl);
        if (seq(a, (unsigned)al, n, (unsigned)nl)) { if (!name) name = i; if (seq(b, (unsigned)bl, v, (unsigned)vl)) { full = i; break; } }
    }
    if (full) { enc_int(7, 0x80, (u32)full); return; }
    enc_int(6, 0x40, (u32)name);
    if (!name) enc_str(n, nl, huff);
    enc_str(v, vl, huff);
    tadd(t, n, nl, v, vl);
}

// ---- decoding
static int dec_int(const u8 *p, int *pos, int end, int prefix, u32 *v) {
    u32 max = (1u << prefix) - 1; u32 x = p[*pos] & max; (*pos)++;
    if (x < max) { *v = x; return 1; }
    int m = 0;
    for (;;) { if (*pos >= end) return 0; u8 b = p[(*pos)++]; x += (u32)(b & 127) << m; m += 7; if (!(b & 128)) break; }
    *v = x; return 1;
}
static int huff_dec(const u8 *p, int n, char *o) {
    u32 code = 0; int len = 0, outn = 0;
    for (int i = 0; i < n; i++) for (int b = 7; b >= 0; b--) {
        code = (code << 1) | ((p[i] >> b) & 1); len++;
        for (int s = 0; s < 256; s++) if (HLEN[s] == len && HCODE[s] == code) { o[outn++] = (char)s; code = 0; len = 0; break; }
        if (len > 30) return -1;
    }
    if (len > 7 || code != (1u << len) - 1) return -1;          // padding: fewer than 8 bits, all ones (EOS prefix)
    return outn;
}
static int dec_str(const u8 *p, int *pos, int end, char *o) {
    int h = p[*pos] >> 7; u32 l; if (!dec_int(p, pos, end, 7, &l) || *pos + (int)l > end) return -1;
    int n; if (h) n = huff_dec(p + *pos, (int)l, o); else { n = (int)l; for (int i = 0; i < n; i++) o[i] = (char)p[*pos + i]; }
    *pos += (int)l; return n;
}
static char text[1024]; static int tlen;
static void tput(const char *s, int n) { for (int i = 0; i < n; i++) text[tlen++] = s[i]; }
// Decode one header block into "name: value\n" lines; updates the dynamic table. Returns 1 on success.
static int decode_block(struct Table *t, const u8 *p, int end) {
    int pos = 0; tlen = 0;
    while (pos < end) {
        u8 b = p[pos]; char n[96], v[96]; int nl = 0, vl = 0; const char *a, *c; int al, cl;
        if (b & 0x80) { u32 i; if (!dec_int(p, &pos, end, 7, &i) || !idx_get(t, (int)i, &a, &al, &c, &cl)) return 0; tput(a, al); tput(": ", 2); tput(c, cl); tput("\n", 1); continue; }
        int incr = (b & 0xc0) == 0x40; int prefix = incr ? 6 : 4;
        if ((b & 0xe0) == 0x20) return 0;                       // dynamic table size update: not used by the examples
        u32 i; if (!dec_int(p, &pos, end, prefix, &i)) return 0;
        if (i) { if (!idx_get(t, (int)i, &a, &al, &c, &cl)) return 0; nl = al; for (int k = 0; k < nl; k++) n[k] = a[k]; }
        else { nl = dec_str(p, &pos, end, n); if (nl < 0) return 0; }
        vl = dec_str(p, &pos, end, v); if (vl < 0) return 0;
        if (incr) tadd(t, n, nl, v, vl);
        tput(n, nl); tput(": ", 2); tput(v, vl); tput("\n", 1);
    }
    return 1;
}
static int unhex(const char *s, u8 *o) { int n = 0; while (s[2 * n]) { int a = s[2 * n], b = s[2 * n + 1]; a = a <= '9' ? a - '0' : a - 'a' + 10; b = b <= '9' ? b - '0' : b - 'a' + 10; o[n] = (u8)(a * 16 + b); n++; } return n; }
// table as text "size|name: value\n", newest first, like the dynamic table listing of the RFC
static char dump[1024]; static int dlen;
static void dump_table(const struct Table *t) {
    dlen = 0;
    for (int i = 0; i < t->cnt; i++) {
        int s = t->e[i].nl + t->e[i].vl + 32; char d[8]; int k = 0; do { d[k++] = (char)('0' + s % 10); s /= 10; } while (s);
        while (k) dump[dlen++] = d[--k];
        dump[dlen++] = '|';
        for (int j = 0; j < t->e[i].nl; j++) dump[dlen++] = t->e[i].n[j];
        dump[dlen++] = ':'; dump[dlen++] = ' ';
        for (int j = 0; j < t->e[i].vl; j++) dump[dlen++] = t->e[i].v[j];
        dump[dlen++] = '\n';
    }
}

int paper_main(void) {
    int ok = 1;
    // Huffman code of Appendix B: complete and prefix-free; every symbol decodes back from its own code
    u64 kraft = 0; int clash = 0, rt = 0;
    for (int s = 0; s <= 256; s++) kraft += 1ULL << (30 - HLEN[s]);
    for (int a = 0; a <= 256; a++) for (int b = 0; b <= 256; b++) if (a != b && HLEN[a] <= HLEN[b] && (HCODE[b] >> (HLEN[b] - HLEN[a])) == HCODE[a]) clash++;
    for (int s = 0; s < 256; s++) { char c = (char)s, o[4]; u8 t[8]; int n = huff_enc(&c, 1, t); rt += huff_dec(t, n, o) == 1 && o[0] == c; }
    pstr("Appendix B Huffman code: Kraft sum "); pdec((i64)kraft); pstr(" of "); pdec(1LL << 30); pstr(", prefix clashes "); pdec(clash); pstr(", single-byte round trips "); pdec(rt); pstr(" of 256\n");
    ok &= kraft == (1ULL << 30) && clash == 0 && rt == 256;

    struct Table te, td; int n_enc = 0, n_dec = 0, n_tab = 0, total = (int)(sizeof EX / sizeof EX[0]);
    for (int x = 0; x < total; x++) {
        const char *id = EX[x].id; int sec = id[2] - '0';
        int max = sec >= 5 ? 256 : 4096;
        if (sec == 2 || id[4] == '1') { tinit(&te, max); tinit(&td, max); }   // each C.2 example and the first of each sequence starts a new connection
        int huff = sec == 4 || sec == 6;
        olen = 0;
        if (sec == 2) {
            const char *h = EX[x].hdrs; unsigned hl = slen(h) - 1;               // one header per C.2 example, text "name: value\n"
            unsigned c = 1; while (c < hl && !(h[c] == ':' && h[c + 1] == ' ')) c++;
            const char *n = h, *v = h + c + 2; int nl = (int)c, vl = (int)(hl - c - 2);
            if (id[4] == '1') { put(0x40); enc_str(n, nl, 0); enc_str(v, vl, 0); tadd(&te, n, nl, v, vl); }                  // literal with indexing, new name
            else if (id[4] == '2') { enc_int(4, 0x00, 4); enc_str(v, vl, 0); }                                              // literal without indexing, name index 4 (:path)
            else if (id[4] == '3') { put(0x10); enc_str(n, nl, 0); enc_str(v, vl, 0); }                                     // literal never indexed, new name
            else { enc_int(7, 0x80, 2); }                                                                                    // indexed, static index 2 (:method: GET)
        } else {
            const char *h = EX[x].hdrs;
            while (*h) {
                unsigned c = 1; while (!(h[c] == ':' && h[c + 1] == ' ')) c++;
                unsigned e = c + 2; while (h[e] != '\n') e++;
                enc_field(&te, h, (int)c, h + c + 2, (int)(e - c - 2), huff);
                h += e + 1;
            }
        }
        u8 want[128]; int wn = unhex(EX[x].hex, want);
        int e_ok = olen == wn; for (int i = 0; e_ok && i < wn; i++) if (out[i] != want[i]) e_ok = 0;
        int d_ok = decode_block(&td, want, wn) && seq(text, (unsigned)tlen, EX[x].hdrs, slen(EX[x].hdrs));
        dump_table(&td);
        int t_ok = seq(dump, (unsigned)dlen, EX[x].dyn, slen(EX[x].dyn)) && td.size == EX[x].size;
        int te_ok = t_ok; { dump_table(&te); te_ok = seq(dump, (unsigned)dlen, EX[x].dyn, slen(EX[x].dyn)) && te.size == EX[x].size; }
        n_enc += e_ok; n_dec += d_ok; n_tab += t_ok && te_ok;
        pstr(id); pstr(": encoded "); pdec(olen); pstr(" bytes ("); pstr(e_ok ? "match" : "DIFFERENT"); pstr("), decoded list ");
        pstr(d_ok ? "match" : "DIFFERENT"); pstr(", dynamic table "); pdec(td.cnt); pstr(" entries, size "); pdec(td.size); pstr(t_ok && te_ok ? " (match)\n" : " (DIFFERENT)\n");
        if (!e_ok) { pstr("  got      "); phex_bytes(out, olen); pstr("\n  expected "); pstr(EX[x].hex); pstr("\n"); }
    }
    pstr("Appendix C examples: encoded bytes match "); pdec(n_enc); pstr("/"); pdec(total);
    pstr(", decoded header lists match "); pdec(n_dec); pstr("/"); pdec(total);
    pstr(", dynamic tables match "); pdec(n_tab); pstr("/"); pdec(total); pstr("\n");
    ok &= n_enc == total && n_dec == total && n_tab == total && total == 16;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
