// Thresholds for k-ary cuckoo hashing from Dietzfelbinger, Goerdt, Mitzenmacher, Montanari, Pagh and Rink, "Tight Thresholds
// for Cuckoo Hashing via XORSAT" (ICALP 2010; arXiv:0912.0287v3), written from the paper's definitions (equations (2) to (4),
// Propositions 1 and 2). Checks: (1) the paper's table of c_{k,l} (l = 2..7, k = 2..7, 35 values) is recomputed from
// equations (3) and (4) with plain double arithmetic (own exp(), bisection) and must agree with every published value to
// within 5.1e-11 (half a unit of the 10th decimal, plus 1e-12); tolerance fixed before the first run.
// (2) Random 3-uniform hypergraphs (n = 40000 keys, fixed seed) are peeled: at loads 0.70 and 0.78, below the 2-core threshold
// min g of Proposition 1, the 2-core must be empty; at loads 0.88, 0.96 and 1.00 the 2-core's vertex and edge counts must be
// within 3 percent of the Proposition 2 values m*Pr[Po(beta)>=2] and n*Pr[Po(beta)>=1]^3 (tolerance fixed before the first
// run; the paper states the equalities up to o(m)), and the core must have fewer edges than vertices at 0.88 < c_{3,2} and
// more edges than vertices at 0.96 and 1.00 > c_{3,2} (more edges than vertices means no key placement exists).
// Exit 0 only if everything matches.
#include "ounce_paper.h"
#include "table.inc"

#ifndef NEG
#define NEG 0   // NEG=1: the Poisson tail uses l instead of l-1 in the numerator of g (a wrong reading of equation (3))
#endif

// exp() from + - * / only: exp(x) = 2^n * exp(r), r = x - n ln2, ln2 from its series sum 1/(j 2^j)
static double LN2;
static double my_exp(double x) {
    double nf = x / LN2; long n = (long)(nf < 0 ? nf - 0.5 : nf + 0.5); double r = x - (double)n * LN2;
    double term = 1.0, sum = 1.0; for (int j = 1; j < 30; j++) { term = term * r / (double)j; sum += term; }
    while (n > 0) { sum *= 2.0; n--; } while (n < 0) { sum /= 2.0; n++; }
    return sum;
}
static double tail(double b, int j) {            // Pr[Po(b) >= j]
    double eb = my_exp(-b), term = 1.0, s = 0.0;
    for (int i = 0; i < j; i++) { s += term; term = term * b / (double)(i + 1); }
    return 1.0 - eb * s;
}
static double dpow(double x, int e) { double r = 1.0; while (e-- > 0) r *= x; return r; }
static double g(double b, int k, int l) { int lt = NEG ? l : l - 1; return b / ((double)k * dpow(tail(b, lt), k - 1)); }      // equation (3)
static double fdens(double b, int k, int l) { return b * tail(b, l - 1) / ((double)k * tail(b, l)); }                         // left side of equation (4)
static double ck(int k, int l) {                 // c_{k,l}: g(beta) at the beta where the core's edge density equals l-1
    double lo = 1e-3, hi = 60.0;
    for (int it = 0; it < 200; it++) { double mid = (lo + hi) / 2; if (fdens(mid, k, l) < (double)(l - 1)) lo = mid; else hi = mid; }
    return g(lo, k, l);
}
static double beta_of_c(double c, int k, int l) {   // unique beta > beta* with g(beta) = c (g convex, beta* its minimiser)
    double lo = 0.01, hi = 20.0;
    for (int it = 0; it < 200; it++) { double m1 = lo + (hi - lo) / 3, m2 = hi - (hi - lo) / 3; if (g(m1, k, l) < g(m2, k, l)) hi = m2; else lo = m1; }
    double bs = (lo + hi) / 2; lo = bs; hi = 60.0;
    for (int it = 0; it < 200; it++) { double mid = (lo + hi) / 2; if (g(mid, k, l) < c) lo = mid; else hi = mid; }
    return (lo + hi) / 2;
}
static double parse(const char *s) { double v = 0, f = 1; int dot = 0; for (; *s; s++) { if (*s == '.') { dot = 1; continue; } if (dot) { f /= 10; v += f * (*s - '0'); } else v = v * 10 + (*s - '0'); } return v; }

// ---- random 3-uniform hypergraph and peeling
#define NK 40000
#define MAXM 60000
static int ev[NK][3], deg[MAXM], first[MAXM + 1], inc[3 * NK], pos[MAXM], alive[NK], queue[MAXM];
static u64 rng;
static u32 rnd(void) { rng = rng * 6364136223846793005ULL + 1442695040888963407ULL; return (u32)(rng >> 33); }
static void core(int m, int *core_edges, int *core_verts) {
    for (int v = 0; v < m; v++) deg[v] = 0;
    for (int e = 0; e < NK; e++) {
        int a, b, c; a = (int)(rnd() % (u32)m); do b = (int)(rnd() % (u32)m); while (b == a); do c = (int)(rnd() % (u32)m); while (c == a || c == b);
        ev[e][0] = a; ev[e][1] = b; ev[e][2] = c; deg[a]++; deg[b]++; deg[c]++; alive[e] = 1;
    }
    first[0] = 0; for (int v = 0; v < m; v++) { first[v + 1] = first[v] + deg[v]; pos[v] = first[v]; }
    for (int e = 0; e < NK; e++) for (int j = 0; j < 3; j++) inc[pos[ev[e][j]]++] = e;
    int qh = 0, qt = 0; for (int v = 0; v < m; v++) if (deg[v] == 1) queue[qt++] = v;
    while (qh < qt) {
        int v = queue[qh++]; if (deg[v] != 1) continue;
        int e = -1; for (int p = first[v]; p < first[v + 1]; p++) if (alive[inc[p]]) { e = inc[p]; break; }
        alive[e] = 0;
        for (int j = 0; j < 3; j++) { int w = ev[e][j]; deg[w]--; if (deg[w] == 1) queue[qt++] = w; }
    }
    int ce = 0, cv = 0; for (int e = 0; e < NK; e++) ce += alive[e]; for (int v = 0; v < m; v++) cv += deg[v] >= 2;
    *core_edges = ce; *core_verts = cv;
}

int paper_main(void) {
    int ok = 1;
    { double s = 0, p = 1; for (int j = 1; j < 60; j++) { p /= 2; s += p / (double)j; } LN2 = s; }
    int good = 0, cells = 0;
    pstr("table of c_{k,l} recomputed (computed / published, tolerance 5.1e-11):\n");
    for (int l = 2; l <= 7; l++) for (int k = 2; k <= 7; k++) {
        if (PUB[l - 2][k - 2][0] == '0' && PUB[l - 2][k - 2][1] == 0) continue;
        double c = ck(k, l), p = parse(PUB[l - 2][k - 2]); double d = c > p ? c - p : p - c;
        int m = d <= 5.1e-11; cells++; good += m;
        // print the computed value rounded to 10 decimals
        u64 r = (u64)(c * 1e10 + 0.5); pstr("  l="); pdec(l); pstr(" k="); pdec(k); pstr(": "); pdec((i64)(r / 10000000000ULL)); pstr("."); 
        { u64 fr = r % 10000000000ULL; char dg[10]; for (int i = 9; i >= 0; i--) { dg[i] = (char)('0' + fr % 10); fr /= 10; } paper_write(dg, 10); }
        pstr(m ? " ok\n" : " DIFFERENT, published "); if (!m) pstr(PUB[l - 2][k - 2]); if (!m) pstr("\n");
    }
    pstr("published c_{k,l} values reproduced: "); pdec(good); pstr(" of "); pdec(cells); pstr("\n"); ok &= good == cells && cells == 35;

    // empirical 3-ary hypergraphs (c = n/m). The 2-core appears above c*_{3,2} = min g (Proposition 1); a table can be built
    // while the core has fewer edges than vertices, and the paper's c_{3,2} = 0.9179... is where the core's edge density reaches 1.
    double bstar; { double lo = 0.01, hi = 20.0; for (int it = 0; it < 200; it++) { double m1 = lo + (hi - lo) / 3, m2 = hi - (hi - lo) / 3; if (g(m1, 3, 2) < g(m2, 3, 2)) hi = m2; else lo = m1; } bstar = (lo + hi) / 2; }
    double cstar = g(bstar, 3, 2);
    rng = 0x2545f4914f6cdd1dULL;
    int empty_runs = 0;
    static const double LOW[2] = {0.70, 0.78};
    for (int run = 0; run < 2; run++) { int m = (int)((double)NK / LOW[run]), ce, cv; core(m, &ce, &cv); empty_runs += ce == 0 && LOW[run] < cstar; pstr(run ? "c=0.78" : "c=0.70"); pstr(": 2-core edges "); pdec(ce); pstr("\n"); }
    pstr("loads below the 2-core threshold min g (computed "); pdec((i64)(cstar * 10000 + 0.5)); pstr(" /10000) with an empty 2-core: "); pdec(empty_runs); pstr(" of 2\n"); ok &= empty_runs == 2;
    static const double CS[3] = {0.88, 0.96, 1.00}; static const char *CN[3] = {"0.88", "0.96", "1.00"}; int pred_ok = 0, side_ok = 0;
    for (int t = 0; t < 3; t++) {
        double c = CS[t]; int m = (int)((double)NK / c), ce, cv; core(m, &ce, &cv);
        double b = beta_of_c(c, 3, 2), pv = tail(b, 2) * (double)m, pe = dpow(tail(b, 1), 3) * (double)NK;
        double dv = (double)cv - pv; if (dv < 0) dv = -dv; double de = (double)ce - pe; if (de < 0) de = -de;
        int good2 = ce > 0 && dv <= 0.03 * pv && de <= 0.03 * pe; pred_ok += good2;
        int below = CS[t] < 0.9179352767; int dens_ok = below ? ce < cv : ce > cv; side_ok += dens_ok;
        pstr("c="); pstr(CN[t]); pstr(": 2-core vertices "); pdec(cv); pstr(" (Prop. 2 predicts "); pdec((i64)(pv + 0.5)); pstr("), edges ");
        pdec(ce); pstr(" (predicts "); pdec((i64)(pe + 0.5)); pstr(good2 ? ") within 3%; " : ") OUTSIDE 3%; "); pstr(ce < cv ? "core has fewer edges than vertices (placeable)\n" : "core has more edges than vertices (no placement exists)\n");
    }
    pstr("loads with 2-core size within 3% of Proposition 2: "); pdec(pred_ok); pstr(" of 3\n"); ok &= pred_ok == 3;
    pstr("loads on the side of c_{3,2} the paper predicts (edges < vertices below, edges > vertices above): "); pdec(side_ok); pstr(" of 3\n"); ok &= side_ok == 3;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
