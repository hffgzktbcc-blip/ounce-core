// Punycode (Bootstring with the RFC's parameters) from RFC 3492 (Costello, "Punycode: A Bootstring encoding of Unicode for
// Internationalized Domain Names in Applications (IDNA)", 2003), written from the procedures of sections 6.1 to 6.3. Checks:
// the 19 sample strings (A) to (S) of section 7.1 encode to the published Punycode (with the u+/U+ case flags of the RFC
// applied to the last digit of each non-basic code point's delta) and decode back; and the published decoding traces (7.2) and
// encoding traces (7.3) of samples B and L: every delta, every bias after adapt(), every intermediate string.
// Exit 0 only if everything matches.
#include "ounce_paper.h"
#include "samples.inc"

#ifndef NEG
#define NEG 0   // NEG=1: adapt() uses damp 70 instead of 700 on the first call
#endif
#define BASE 36
#define TMIN 1
#define TMAX 26
#define SKEW 38
#define DAMP (NEG == 1 ? 70 : 700)
#define INITIAL_BIAS 72
#define INITIAL_N 128

static unsigned slen(const char *s) { unsigned n = 0; while (s[n]) n++; return n; }
static int seq(const char *a, const char *b) { unsigned n = slen(a); if (n != slen(b)) return 0; for (unsigned i = 0; i < n; i++) if (a[i] != b[i]) return 0; return 1; }

static unsigned adapt(unsigned delta, unsigned numpoints, int firsttime) {            // RFC 6.1
    delta = firsttime ? delta / DAMP : delta / 2;
    delta += delta / numpoints;
    unsigned k = 0;
    while (delta > ((BASE - TMIN) * TMAX) / 2) { delta /= BASE - TMIN; k += BASE; }
    return k + (((BASE - TMIN + 1) * delta) / (delta + SKEW));
}
static char digit_char(unsigned d, int upper) { return d < 26 ? (char)((upper ? 'A' : 'a') + d) : (char)('0' + d - 26); }
static int char_digit(char c) { return c >= '0' && c <= '9' ? c - '0' + 26 : c >= 'a' && c <= 'z' ? c - 'a' : c >= 'A' && c <= 'Z' ? c - 'A' : -1; }

// Encoding trace (section 7.3 data) recorded for comparison
static unsigned tr_m[64], tr_delta[64], tr_bias[64]; static char tr_enc[64][16]; static int tr_n;
// RFC 6.3. `flag[i]` selects an upper-case final digit for code point i (the RFC's U+ marking). Returns length, or -1.
static int encode(const unsigned *in, const unsigned char *flag, int n, char *out) {
    unsigned nn = INITIAL_N, delta = 0, bias = INITIAL_BIAS; int h = 0, o = 0; tr_n = 0;
    for (int i = 0; i < n; i++) if (in[i] < 0x80) out[o++] = (char)in[i];
    int b = o; h = b; if (b > 0) out[o++] = '-';
    while (h < n) {
        unsigned m = 0xffffffffu; for (int i = 0; i < n; i++) if (in[i] >= nn && in[i] < m) m = in[i];
        delta += (m - nn) * (unsigned)(h + 1); nn = m;
        for (int i = 0; i < n; i++) {
            if (in[i] < nn) delta++;
            if (in[i] == nn) {
                unsigned q = delta; int start = o;
                for (unsigned k = BASE;; k += BASE) {
                    unsigned t = k <= bias ? TMIN : k >= bias + TMAX ? TMAX : k - bias;
                    if (q < t) break;
                    out[o++] = digit_char(t + (q - t) % (BASE - t), 0); q = (q - t) / (BASE - t);
                }
                out[o++] = digit_char(q, flag[i]);
                bias = adapt(delta, (unsigned)(h + 1), h == b);
                if (tr_n < 64) { tr_m[tr_n] = nn; tr_delta[tr_n] = delta; tr_bias[tr_n] = bias; int l = 0; for (int p = start; p < o && l < 15; p++) tr_enc[tr_n][l++] = (char)(out[p] >= 'A' && out[p] <= 'Z' ? out[p] + 32 : out[p]); tr_enc[tr_n][l] = 0; tr_n++; }
                delta = 0; h++;
            }
        }
        delta++; nn++;
    }
    out[o] = 0; return o;
}
// Decoding trace recorded for comparison with section 7.2
static unsigned dt_delta[64], dt_bias[64]; static char dt_digits[64][16], dt_state[64][200]; static int dt_n;
static void hex4(unsigned v, char *o) { for (int i = 3; i >= 0; i--) *o++ = "0123456789ABCDEF"[(v >> (4 * i)) & 15]; }
// RFC 6.2. Returns number of code points or -1.
static int decode(const char *in, unsigned *out) {
    unsigned n = INITIAL_N, i = 0, bias = INITIAL_BIAS; int len = 0; dt_n = 0;
    int lastd = -1; unsigned L = slen(in); for (unsigned k = 0; k < L; k++) if (in[k] == '-') lastd = (int)k;
    unsigned p = 0;
    if (lastd > 0) { for (int k = 0; k < lastd; k++) { if ((u8)in[k] >= 0x80) return -1; out[len++] = (u8)in[k]; } p = (unsigned)lastd + 1; }
    else if (lastd == 0) p = 1;
    while (p < L) {
        unsigned oldi = i, w = 1; int sd = (int)p;
        for (unsigned k = BASE;; k += BASE) {
            if (p >= L) return -1;
            int d = char_digit(in[p++]); if (d < 0) return -1;
            i += (unsigned)d * w;
            unsigned t = k <= bias ? TMIN : k >= bias + TMAX ? TMAX : k - bias;
            if ((unsigned)d < t) break;
            w *= BASE - t;
        }
        unsigned fulldelta = i - oldi;
        bias = adapt(i - oldi, (unsigned)(len + 1), oldi == 0);
        n += i / (unsigned)(len + 1); i %= (unsigned)(len + 1);
        for (int k = len; k > (int)i; k--) out[k] = out[k - 1];
        out[i] = n; len++;
        if (dt_n < 64) {
            dt_delta[dt_n] = fulldelta; dt_bias[dt_n] = bias;
            int l = 0; for (int q = sd; q < (int)p && l < 15; q++) dt_digits[dt_n][l++] = in[q]; dt_digits[dt_n][l] = 0;
            char *s = dt_state[dt_n]; int sl = 0;
            for (int k = 0; k < len; k++) { if (k) s[sl++] = ' '; hex4(out[k], s + sl); sl += 4; if (k == (int)i) { s[sl++] = ' '; s[sl++] = '*'; } }
            s[sl] = 0; dt_n++;
        }
        i++;
    }
    return len;
}

int paper_main(void) {
    int ok = 1; char buf[200]; unsigned dec[64];
    int enc_ok = 0, dec_ok = 0, total = (int)(sizeof SAMPLES / sizeof SAMPLES[0]);
    for (int s = 0; s < total; s++) {
        const struct Sample *S = &SAMPLES[s];
        int n = encode(S->cp, S->flag, S->n, buf);
        int e = n >= 0 && seq(buf, S->puny);
        int m = decode(S->puny, dec); int d = m == S->n; for (int i = 0; d && i < m; i++) if (dec[i] != S->cp[i]) d = 0;
        enc_ok += e; dec_ok += d;
        pstr("("); paper_write(&S->label, 1); pstr(") "); pstr(e ? "encode ok" : "ENCODE DIFFERENT"); pstr(", "); pstr(d ? "decode ok" : "DECODE DIFFERENT"); pstr(": "); pstr(buf); pstr("\n");
    }
    pstr("section 7.1 samples: encode "); pdec(enc_ok); pstr("/"); pdec(total); pstr(", decode "); pdec(dec_ok); pstr("/"); pdec(total); pstr("\n");
    ok &= enc_ok == total && dec_ok == total && total == 19;

    // traces of samples B (index 1) and L (index 11)
    int steps = 0, steps_ok = 0;
    for (int w = 0; w < 2; w++) {
        const struct Sample *S = &SAMPLES[w ? 11 : 1];
        const struct DStep *D = w ? DEC_L : DEC_B; const struct EStep *E = w ? ENC_L : ENC_B; int cnt = w ? 6 : 9;
        encode(S->cp, S->flag, S->n, buf);
        int ne = tr_n;
        decode(S->puny, dec);
        int nd = dt_n;
        for (int k = 0; k < cnt; k++) {
            int e_ok = k < ne && tr_m[k] == E[k].m && tr_delta[k] == E[k].delta && seq(tr_enc[k], E[k].enc) && tr_bias[k] == E[k].bias;
                        int d_ok = k < nd && seq(dt_digits[k], D[k].digits) && dt_delta[k] == D[k].delta && dt_bias[k] == D[k].bias && seq(dt_state[k], D[k].state);
            steps += 2; steps_ok += e_ok + d_ok;
            if (!e_ok || !d_ok) { pstr("FAIL trace "); pstr(w ? "L" : "B"); pstr(" step "); pdec(k); pstr("\n"); }
        }
        ok &= ne == cnt && nd == cnt;
    }
    pstr("published trace steps (encoding: code point, delta, digits, bias; decoding: digits, delta, bias, string): "); pdec(steps_ok); pstr(" of "); pdec(steps); pstr("\n");
    ok &= steps_ok == steps && steps == 30;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
