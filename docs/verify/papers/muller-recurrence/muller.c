// Reproduction of the recurrence table in W. Kahan, "How Futile are Mindless Assessments of Roundoff in Floating-Point
// Computation?" (work-in-progress note, version of 11 January 2006), section 5 (J.-M. Muller's recurrence):
//   x_{n+1} = 108 - (815 - 1500/x_{n-1})/x_n,  x_0 = 4, x_1 = 4.25.
// Written for ounce-core from the recurrence in the paper. Everything but the 53-bit run is exact integer arithmetic in a small
// unsigned big-number type written here: the true values x_n = (3^(n+1)+5^(n+1))/(3^n+5^n), the exact fraction for x_80, and
// the paper's gamma_n = ((x_n-3)3^n + (x_n-5)5^n)/((100-x_n)100^n) evaluated exactly at the computed doubles.
// Build (aarch64): clang --target=aarch64-linux-gnu -O1 -ffreestanding -fno-builtin -ffp-contract=off -I../common ...
// Negative control: -DNEG_CONST changes the constant 815 to 814.
#include "ounce_paper.h"

#define L 12
typedef struct { u32 w[L]; } Big;
static void b_set(Big *a, u64 v) { for (int i = 0; i < L; i++) a->w[i] = 0; a->w[0] = (u32)v; a->w[1] = (u32)(v >> 32); }
static int b_zero(const Big *a) { for (int i = 0; i < L; i++) if (a->w[i]) return 0; return 1; }
static void b_mul(Big *a, u32 m) { u64 c = 0; for (int i = 0; i < L; i++) { u64 t = (u64)a->w[i] * m + c; a->w[i] = (u32)t; c = t >> 32; } }
static void b_shl(Big *a, int s) {   // s < 32 per step
    while (s > 0) { int t = s > 31 ? 31 : s; u32 c = 0; for (int i = 0; i < L; i++) { u32 n = (a->w[i] << t) | c; c = a->w[i] >> (32 - t); a->w[i] = n; } s -= t; }
}
static void b_add(Big *a, const Big *b) { u64 c = 0; for (int i = 0; i < L; i++) { u64 t = (u64)a->w[i] + b->w[i] + c; a->w[i] = (u32)t; c = t >> 32; } }
static int b_cmp(const Big *a, const Big *b) { for (int i = L - 1; i >= 0; i--) if (a->w[i] != b->w[i]) return a->w[i] < b->w[i] ? -1 : 1; return 0; }
static void b_sub(Big *a, const Big *b) { i64 br = 0; for (int i = 0; i < L; i++) { i64 t = (i64)a->w[i] - b->w[i] - br; br = t < 0; a->w[i] = (u32)(t + (br ? (1LL << 32) : 0)); } }
static void b_pow(Big *a, u32 base, int e) { b_set(a, 1); while (e-- > 0) b_mul(a, base); }
static void b_div2(Big *a) { u32 c = 0; for (int i = L - 1; i >= 0; i--) { u32 n = (a->w[i] >> 1) | (c << 31); c = a->w[i] & 1; a->w[i] = n; } }
static void b_dec(const Big *a, char *out) {   // decimal string (exact)
    Big t = *a; char tmp[120]; int n = 0;
    if (b_zero(&t)) { out[0] = '0'; out[1] = 0; return; }
    while (!b_zero(&t)) { u64 r = 0; for (int i = L - 1; i >= 0; i--) { u64 cur = (r << 32) | t.w[i]; t.w[i] = (u32)(cur / 10); r = cur % 10; } tmp[n++] = (char)('0' + r); }
    for (int i = 0; i < n; i++) out[i] = tmp[n - 1 - i];
    out[n] = 0;
}
// digits of num/den: value = d[0].d[1]d[2]... * 10^e10. Returns number of digits written (nd); *round_up is 1 if the
// remainder after nd digits is at least half; *exact is 1 if the remainder is zero. num, den > 0 required (num may be 0).
static int ratio(Big num, Big den, int nd, char *d, int *e10, int *round_up, int *exact) {
    if (b_zero(&num)) { for (int i = 0; i < nd; i++) d[i] = 0; *e10 = 0; *round_up = 0; *exact = 1; return nd; }
    int e = 0;
    while (b_cmp(&num, &den) < 0) { b_mul(&num, 10); e--; }
    for (;;) { Big t = den; b_mul(&t, 10); if (b_cmp(&num, &t) < 0) break; den = t; e++; }
    for (int i = 0; i < nd; i++) { int q = 0; while (b_cmp(&num, &den) >= 0) { b_sub(&num, &den); q++; } d[i] = (char)q; b_mul(&num, 10); }
    // after the loop num holds 10*remainder: round up when remainder >= den/2, i.e. 10*rem >= 5*den
    Big five = den; b_mul(&five, 5);
    *round_up = b_cmp(&num, &five) >= 0;
    *exact = b_zero(&num);
    *e10 = e; return nd;
}
// Format digits as a plain decimal with nd significant digits (value >= 1 in magnitude here). If strip, drop trailing zeros.
static void fmt(char *out, int neg, const char *d, int nd, int e10, int strip) {
    int n = 0;
    if (neg) out[n++] = '-';
    int last = nd - 1; if (strip) while (last > 0 && d[last] == 0) last--;
    int ip = e10 + 1;   // digits before the point (e10 >= 0 assumed)
    for (int i = 0; i <= last || i < ip; i++) { if (i == ip) out[n++] = '.'; out[n++] = (char)('0' + (i < nd ? d[i] : 0)); }
    out[n] = 0;
}
static int streq(const char *a, const char *b) { while (*a && *a == *b) { a++; b++; } return *a == *b; }

// n, true x_n (truncated to 14 sig. digits), MATLAB x_n (14 sig. digits), MATLAB gamma_n (9 sig. digits); copied from Kahan's table
struct Row { int n; const char *tru, *mat, *gam; };
static const struct Row rows[] = {
    {0, "4", "4", "0"},
    {1, "4.25", "4.25", "0"},
    {2, "4.4705882352941", "4.4705882352941", "-5.95035788e-20"},
    {3, "4.6447368421052", "4.6447368421052", "-7.27269462e-20"},
    {4, "4.7705382436260", "4.7705382436250", "-7.26081334e-20"},
    {5, "4.8557007125890", "4.8557007125685", "-7.26054934e-20"},
    {6, "4.9108474990827", "4.9108474986606", "-7.26062074e-20"},
    {7, "4.9455374041239", "4.9455373955305", "-7.26061505e-20"},
    {8, "4.9669625817627", "4.9669624080410", "-7.26061478e-20"},
    {9, "4.9800457013556", "4.9800422042930", "-7.26061478e-20"},
    {10, "4.9879794484783", "4.9879092327957", "-7.26061478e-20"},
    {11, "4.9927702880620", "4.9913626413145", "-7.26061478e-20"},
    {12, "4.9956558915066", "4.9674550955522", "-7.26061478e-20"},
    {13, "4.9973912683813", "4.4296904983088", "-7.26061478e-20"},
    {14, "4.9984339439448", "-7.8172365784593", "-7.26061478e-20"},
    {15, "4.9990600719708", "168.93916767106", "-7.26061478e-20"},
    {16, "4.9994359371468", "102.03996315205", "-7.26061478e-20"},
    {17, "4.9996615241037", "100.09994751625", "-7.26061478e-20"},
    {18, "4.9997969007134", "100.00499204097", "-7.26061478e-20"},
    {19, "4.9998781354779", "100.00024957923", "-7.26061478e-20"},
    {20, "4.9999268795046", "100.00001247862", "-7.26061479e-20"},
    {21, "4.9999561270611", "100.00000062392", "-7.26061486e-20"},
    {22, "4.9999736760057", "100.00000003119", "-7.26061591e-20"},
    {23, "4.9999842055202", "100.00000000156", "-7.26060665e-20"},
    {24, "4.9999905232822", "100.00000000007", "-7.26058323e-20"},
    {25, "4.9999943139585", "100.00000000000", "-7.27116855e-20"},
    {26, "4.9999965883712", "100.00000000000", "-7.11534953e-20"},
    {27, "4.9999979530213", "100.00000000000", "-4.98074120e-20"},
    {28, "4.9999987718123", "100", "Infinity"},
    {29, "4.9999992630872", "100", "Infinity"},
    {30, "4.9999995578522", "100", "Infinity"},
    {31, "4.9999997347113", "100", "Infinity"},
    {32, "4.9999998408267", "100", "Infinity"},
    {33, "4.9999999044960", "100", "Infinity"},
    {34, "4.9999999426976", "100", "Infinity"},
    {74, "4.9999999999999", "100", "Infinity"},
    {75, "4.9999999999999", "100", "Infinity"},
};

#define NROWS ((int)(sizeof rows / sizeof rows[0]))

static void round_digits(char *d, int nd, int *e10) {   // add one unit in the last place
    int i = nd - 1;
    while (i >= 0) { if (d[i] == 9) { d[i] = 0; i--; } else { d[i]++; break; } }
    if (i < 0) { d[0] = 1; (*e10)++; }
}
static void decomp(double x, int *neg, Big *m, int *k) {   // x = (-1)^neg * m / 2^k exactly (finite, |x| < 2^52)
    union { double d; u64 u; } v; v.d = x; *neg = (int)(v.u >> 63);
    int ef = (int)((v.u >> 52) & 2047); u64 fr = v.u & 0xFFFFFFFFFFFFFull;
    u64 mant = ef ? (fr | (1ull << 52)) : fr; int e = ef - 1075;
    if (mant == 0) { b_set(m, 0); *k = 0; return; }
    while (e < 0 && !(mant & 1)) { mant >>= 1; e++; }
    if (e >= 0) { b_set(m, mant); b_shl(m, e); *k = 0; } else { b_set(m, mant); *k = -e; }
}
static void pow2big(Big *a, int k) { b_set(a, 1); b_shl(a, k); }
// 14 significant digits of x (as printed by the paper's MATLAB column): mode 0 round to nearest, 1 truncate; trailing zeros are
// stripped when the exact decimal expansion ends within 14 digits.
static void show14(double x, int mode, char *out) {
    int neg, k; Big m, den; decomp(x, &neg, &m, &k); pow2big(&den, k);
    char d[16]; int e10, ru, ex; ratio(m, den, 14, d, &e10, &ru, &ex);
    if (mode == 0 && ru) round_digits(d, 14, &e10);
    fmt(out, neg && !b_zero(&m), d, 14, e10, ex);
}
static void show_true(int n, int mode, char *out) {
    Big N, D, t; b_pow(&N, 3, n + 1); b_pow(&t, 5, n + 1); b_add(&N, &t); b_pow(&D, 3, n); b_pow(&t, 5, n); b_add(&D, &t);
    char d[16]; int e10, ru, ex; ratio(N, D, 14, d, &e10, &ru, &ex);
    if (mode == 0 && ru) round_digits(d, 14, &e10);
    fmt(out, 0, d, 14, e10, ex);
}
// gamma_n evaluated exactly at the double x: ((x-3)3^n + (x-5)5^n) / ((100-x)100^n), printed with 9 significant digits
static void show_gamma(double x, int n, int mode, char *out) {
    int neg, k; Big m, p2; decomp(x, &neg, &m, &k); pow2big(&p2, k);
    Big S, T, t; b_pow(&S, 3, n); b_pow(&t, 5, n); b_add(&S, &t); b_pow(&T, 3, n + 1); b_pow(&t, 5, n + 1); b_add(&T, &t);
    // num = sign*m*S - p2*T
    Big a = m; { Big s = S; // m*S via repeated addition of shifted S would be long; multiply limb-wise instead
        Big acc; b_set(&acc, 0);
        for (int bit = 0; bit < 64; bit++) { /* m < 2^64 */ }
        (void)s; (void)acc; }
    Big mS; b_set(&mS, 0);
    { Big cur = S; for (int i = 0; i < L; i++) for (int bit = 0; bit < 32; bit++) { if ((m.w[i] >> bit) & 1) b_add(&mS, &cur); b_shl(&cur, 1); } }
    Big p2T; b_set(&p2T, 0);
    { Big cur = T; for (int i = 0; i < L; i++) for (int bit = 0; bit < 32; bit++) { if ((p2.w[i] >> bit) & 1) b_add(&p2T, &cur); b_shl(&cur, 1); } }
    (void)a;
    Big num; int nneg;
    if (!neg) { if (b_cmp(&mS, &p2T) >= 0) { num = mS; b_sub(&num, &p2T); nneg = 0; } else { num = p2T; b_sub(&num, &mS); nneg = 1; } }
    else { num = mS; b_add(&num, &p2T); nneg = 1; }
    // den = (100*p2 - sign*m) * 100^n
    Big h = p2; b_mul(&h, 100); Big dd; int dneg;
    if (!neg) { if (b_cmp(&h, &m) >= 0) { dd = h; b_sub(&dd, &m); dneg = 0; } else { dd = m; b_sub(&dd, &h); dneg = 1; } }
    else { dd = h; b_add(&dd, &m); dneg = 0; }
    Big pw; b_pow(&pw, 100, n);
    Big den; b_set(&den, 0);
    { Big cur = pw; for (int i = 0; i < L; i++) for (int bit = 0; bit < 32; bit++) { if ((dd.w[i] >> bit) & 1) b_add(&den, &cur); b_shl(&cur, 1); } }
    if (b_zero(&den)) { const char *s = "Infinity"; int i = 0; while ((out[i] = s[i])) i++; return; }
    if (b_zero(&num)) { out[0] = '0'; out[1] = 0; return; }
    char d[12]; int e10, ru, ex; ratio(num, den, 9, d, &e10, &ru, &ex);
    if (mode == 0 && ru) round_digits(d, 9, &e10);
    int p = 0; if (nneg != dneg) out[p++] = '-';
    out[p++] = (char)('0' + d[0]); out[p++] = '.';
    for (int i = 1; i < 9; i++) out[p++] = (char)('0' + d[i]);
    out[p++] = 'e'; int ee = e10; if (ee < 0) { out[p++] = '-'; ee = -ee; } else out[p++] = '+';
    out[p++] = (char)('0' + ee / 10); out[p++] = (char)('0' + ee % 10); out[p] = 0;
}

// |published - exact| for two strings like -7.26062074e-20, in units of the 9th significant digit (large when the exponent or
// sign differ, or the strings are not of that form)
static int gamma_gap(const char *a, const char *b) {
    if (a[0] == 'I' || b[0] == 'I' || a[0] == '0' || b[0] == '0') return 1000000;
    int sa = a[0] == '-', sb = b[0] == '-'; if (sa != sb) return 1000000;
    a += sa; b += sb;
    i64 ma = 0, mb = 0; int i = 0;
    for (; a[i] && a[i] != 'e'; i++) if (a[i] != '.') ma = ma * 10 + (a[i] - '0');
    const char *ea = a + i; i = 0;
    for (; b[i] && b[i] != 'e'; i++) if (b[i] != '.') mb = mb * 10 + (b[i] - '0');
    const char *eb = b + i;
    if (!streq(ea, eb)) return 1000000;
    i64 d = ma > mb ? ma - mb : mb - ma; return d > 1000000 ? 1000000 : (int)d;
}

static double step(double xn, double xm) {   // Oe(y, z) = 108 - (815 - 1500/z)/y with y = x_n, z = x_{n-1}
#ifdef NEG_CONST
    double c = 814.0;
#else
    double c = 815.0;
#endif
    return 108.0 - (c - 1500.0 / xm) / xn;
}

int paper_main(void) {
    int bad = 0;
    double x[81]; x[0] = 4.0; x[1] = 4.25;
    for (int n = 1; n < 80; n++) x[n + 1] = step(x[n], x[n - 1]);
    pstr("n   true x_n (truncated)   53-bit x_n (14 sig. digits)   gamma_n (9 sig. digits)   vs published\n");
    int ok_true = 0, ok_x = 0, ok_g = 0, x_round = 0, g_round = 0, n_dev = 0;
    for (int r = 0; r < NROWS; r++) {
        int n = rows[r].n; char a[40], a1[40], b0[40], b1[40], g0[40], g1[40];
        show_true(n, 0, a); show_true(n, 1, a1);
        int t_ok = streq(a, rows[r].tru) || streq(a1, rows[r].tru);
        show14(x[n < 81 ? (n <= 34 ? n : 80) : 80], 0, b0); show14(x[n <= 34 ? n : 80], 1, b1);
        int xr = streq(b0, rows[r].mat), xt = streq(b1, rows[r].mat);
        show_gamma(x[n <= 34 ? n : 80], n <= 34 ? n : 80, 0, g0); show_gamma(x[n <= 34 ? n : 80], n <= 34 ? n : 80, 1, g1);
        int gr = streq(g0, rows[r].gam), gt = streq(g1, rows[r].gam);
        int x_ok = xr || xt, g_ok = gr || gt;
        int g_dev = 0;   // published gamma differs from the exact value: measure the gap in units of the 9th digit
        if (!g_ok) g_dev = gamma_gap(g0, rows[r].gam);
        // for n = 74, 75 only the true column and x = 100 are published; gamma is Infinity there
        ok_true += t_ok; ok_x += x_ok; ok_g += g_ok; x_round += xr; g_round += gr;
        pdec(n); pstr("  "); pstr(a1); pstr("  "); pstr(b0); pstr("  "); pstr(g0);
        if (t_ok && x_ok && g_ok) pstr("  ok\n");
        else {
            pstr("  DIFFERS, published: "); pstr(rows[r].tru); pstr("  "); pstr(rows[r].mat); pstr("  "); pstr(rows[r].gam);
            if (t_ok && x_ok && g_dev > 0) { pstr("  (gamma gap "); pdec(g_dev); pstr(" units of the 9th digit)\n"); n_dev++; if (g_dev > 20) bad++; }
            else { pstr("\n"); bad++; }
        }
    }
    pstr("true x_n column: "); pdec(ok_true); pstr(" of "); pdec(NROWS); pstr(" rows match\n");
    pstr("53-bit x_n column: "); pdec(ok_x); pstr(" of "); pdec(NROWS); pstr(" rows match (rounded to 14 digits: "); pdec(x_round); pstr(")\n");
    pstr("gamma_n column: "); pdec(ok_g); pstr(" of "); pdec(NROWS); pstr(" rows match exactly (rounded to 9 digits: "); pdec(g_round); pstr("), ");
    pdec(n_dev); pstr(" differ by at most 20 units of the 9th digit\n");

    // exact x_80 as a quotient of integers, reduced by 2, against the fraction printed in the paper
    Big N, D, t; b_pow(&N, 3, 81); b_pow(&t, 5, 81); b_add(&N, &t); b_pow(&D, 3, 80); b_pow(&t, 5, 80); b_add(&D, &t);
    b_div2(&N); b_div2(&D);
    char sn[100], sd[100];
    b_dec(&N, sn); b_dec(&D, sd);
    int f_ok = streq(sn, "206795153138256918939565417139009598365577843034794672964") &&
               streq(sd, "41359030627651383817474849310671104336332210648235594113");
    pstr("exact x_80 = "); pstr(sn); pstr("/"); pstr(sd); pstr(f_ok ? ": matches the fraction in the paper\n" : ": DIFFERS from the paper\n");
    if (!f_ok) bad++;
    { char d[28]; int e10, ru, ex; ratio(N, D, 25, d, &e10, &ru, &ex);
      char s[40]; fmt(s, 0, d, 25, e10, 0);
      const char *pub = "4.9999999999999999964263";   // as printed in the paper (23 significant digits)
      int i = 0; while (pub[i + 1]) i++;                // index of the last published digit
      int same = 1; for (int j = 0; j < i; j++) if (s[j] != pub[j]) same = 0;
      pstr("exact x_80 = "); pstr(s); pstr("... ; paper: "); pstr(pub); pstr("...\n");
      pstr(same ? "  the first 22 significant digits agree; the paper's 23rd digit is 3, the exact value has 2 followed by 01 (off by one in the paper's last digit)\n"
                : "  the first 22 significant digits DIFFER\n");
      if (!same || s[i] != '2' || pub[i] != '3') bad++; }
    // the 53-bit run ends at 100, the false limit
    { union { double d; u64 u; } v; v.d = x[80]; int ok = x[80] == 100.0; pstr("53-bit x_80 bit pattern "); phex(v.u, 16); pstr(ok ? " = 100.0 (the paper: all fast hardware alleges 100)\n" : " is not 100.0\n"); if (!ok) bad++; }
    if (ok_true != NROWS || ok_x != NROWS || ok_g + n_dev != NROWS || n_dev > 1) bad++;
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
