// Reproduction of the certified linear least squares results for the classic test problems of J. W. Longley (1967, "An Appraisal of
// Least Squares Programs for the Electronic Computer from the Viewpoint of the User", JASA 62, 819-841) and R. H. Wampler (1970,
// "A Report on the Accuracy of Some Widely-Used Least Squares Computer Programs", JASA 65, 549-565), plus the NIST Filip problem,
// with the certified values published by NIST (Statistical Reference Datasets, linear regression suite): estimates, their standard
// deviations, the residual standard deviation and R-squared.
// Written for ounce-core: a plain Householder QR least-squares solver (Golub 1965 style), no pivoting, no iterative refinement, no
// third-party code. Accuracy is measured as the log relative error LRE = -log10(|computed - certified| / |certified|), capped at 15.
// Thresholds on LRE were written before the first run (see paper.json). Negative control: -DNEG_NORMAL solves the normal equations
// instead (Cholesky of A^T A), the numerically poor method these programs were criticised for.
#include "ounce_paper.h"
static const double Longley_est[] = {-3482258.63459582,15.0618722713733,-0.358191792925910E-01,-2.02022980381683,-1.03322686717359,-0.511041056535807E-01,1829.15146461355};
static const double Longley_sd[] = {890420.383607373,84.9149257747669,0.334910077722432E-01,0.488399681651699,0.214274163161675,0.226073200069370,455.478499142212};
static const double Longley_resid = 304.854073561965, Longley_r2 = 0.995479004577296;
static const double Longley_data[] = {60323,83.0,234289,2356,1590,107608,1947,61122,88.5,259426,2325,1456,108632,1948,60171,88.2,258054,3682,1616,109773,1949,61187,89.5,284599,3351,1650,110929,1950,63221,96.2,328975,2099,3099,112075,1951,63639,98.1,346999,1932,3594,113270,1952,64989,99.0,365385,1870,3547,115094,1953,63761,100.0,363112,3578,3350,116219,1954,66019,101.2,397469,2904,3048,117388,1955,67857,104.6,419180,2822,2857,118734,1956,68169,108.4,442769,2936,2798,120445,1957,66513,110.8,444546,4681,2637,121950,1958,68655,112.6,482704,3813,2552,123366,1959,69564,114.2,502601,3931,2514,125368,1960,69331,115.7,518173,4806,2572,127852,1961,70551,116.9,554894,4007,2827,130081,1962};
#define Longley_N 16
#define Longley_C 7
static const double Wampler1_est[] = {1.00000000000000,1.00000000000000,1.00000000000000,1.00000000000000,1.00000000000000,1.00000000000000};
static const double Wampler1_sd[] = {0.000000000000000,0.000000000000000,0.000000000000000,0.000000000000000,0.000000000000000,0.000000000000000};
static const double Wampler1_resid = 0.000000000000000, Wampler1_r2 = 1.00000000000000;
static const double Wampler1_data[] = {1,0,6,1,63,2,364,3,1365,4,3906,5,9331,6,19608,7,37449,8,66430,9,111111,10,177156,11,271453,12,402234,13,579195,14,813616,15,1118481,16,1508598,17,2000719,18,2613660,19,3368421,20};
#define Wampler1_N 21
#define Wampler1_C 2
static const double Wampler4_est[] = {1.00000000000000,1.00000000000000,1.00000000000000,1.00000000000000,1.00000000000000,1.00000000000000};
static const double Wampler4_sd[] = {215232.624678170,236355.173469681,77934.3524331583,10147.5507550350,564.566512170752,11.2324854679312};
static const double Wampler4_resid = 236014.502379268, Wampler4_r2 = 0.957478440825662;
static const double Wampler4_data[] = {75901,0,-204794,1,204863,2,-204436,3,253665,4,-200894,5,214131,6,-185192,7,221249,8,-138370,9,315911,10,-27644,11,455253,12,197434,13,783995,14,608816,15,1370781,16,1303798,17,2205519,18,2408860,19,3444321,20};
#define Wampler4_N 21
#define Wampler4_C 2
static const double Wampler5_est[] = {1.00000000000000,1.00000000000000,1.00000000000000,1.00000000000000,1.00000000000000,1.00000000000000};
static const double Wampler5_sd[] = {21523262.4678170,23635517.3469681,7793435.24331583,1014755.07550350,56456.6512170752,1123.24854679312};
static const double Wampler5_resid = 23601450.2379268, Wampler5_r2 = 0.224668921574940E-02;
static const double Wampler5_data[] = {7590001,0,-20479994,1,20480063,2,-20479636,3,25231365,4,-20476094,5,20489331,6,-20460392,7,18417449,8,-20413570,9,20591111,10,-20302844,11,18651453,12,-20077766,13,21059195,14,-19666384,15,26348481,16,-18971402,17,22480719,18,-17866340,19,10958421,20};
#define Wampler5_N 21
#define Wampler5_C 2
static const double Filip_est[] = {-1467.48961422980,-2772.17959193342,-2316.37108160893,-1127.97394098372,-354.478233703349,-75.1242017393757,-10.8753180355343,-1.06221498588947,-0.670191154593408E-01,-0.246781078275479E-02,-0.402962525080404E-04};
static const double Filip_sd[] = {298.084530995537,559.779865474950,466.477572127796,227.204274477751,71.6478660875927,15.2897178747400,2.23691159816033,0.221624321934227,0.142363763154724E-01,0.535617408889821E-03,0.896632837373868E-05};
static const double Filip_resid = 0.334801051324544E-02, Filip_r2 = 0.996727416185620;
static const double Filip_data[] = {0.8116,-6.860120914,0.9072,-4.324130045,0.9052,-4.358625055,0.9039,-4.358426747,0.8053,-6.955852379,0.8377,-6.661145254,0.8667,-6.355462942,0.8809,-6.118102026,0.7975,-7.115148017,0.8162,-6.815308569,0.8515,-6.519993057,0.8766,-6.204119983,0.8885,-5.853871964,0.8859,-6.109523091,0.8959,-5.79832982,0.8913,-5.482672118,0.8959,-5.171791386,0.8971,-4.851705903,0.9021,-4.517126416,0.909,-4.143573228,0.9139,-3.709075441,0.9199,-3.499489089,0.8692,-6.300769497,0.8872,-5.953504836,0.89,-5.642065153,0.891,-5.031376979,0.8977,-4.680685696,0.9035,-4.329846955,0.9078,-3.928486195,0.7675,-8.56735134,0.7705,-8.363211311,0.7713,-8.107682739,0.7736,-7.823908741,0.7775,-7.522878745,0.7841,-7.218819279,0.7971,-6.920818754,0.8329,-6.628932138,0.8641,-6.323946875,0.8804,-5.991399828,0.7668,-8.781464495,0.7633,-8.663140179,0.7678,-8.473531488,0.7697,-8.247337057,0.77,-7.971428747,0.7749,-7.676129393,0.7796,-7.352812702,0.7897,-7.072065318,0.8131,-6.774174009,0.8498,-6.478861916,0.8741,-6.159517513,0.8061,-6.835647144,0.846,-6.53165267,0.8751,-6.224098421,0.8856,-5.910094889,0.8919,-5.598599459,0.8934,-5.290645224,0.894,-4.974284616,0.8957,-4.64454848,0.9047,-4.290560426,0.9129,-3.885055584,0.9209,-3.408378962,0.9219,-3.13200249,0.7739,-8.726767166,0.7681,-8.66695597,0.7665,-8.511026475,0.7703,-8.165388579,0.7702,-7.886056648,0.7761,-7.588043762,0.7809,-7.283412422,0.7961,-6.995678626,0.8253,-6.691862621,0.8602,-6.392544977,0.8809,-6.067374056,0.8301,-6.684029655,0.8664,-6.378719832,0.8834,-6.065855188,0.8898,-5.752272167,0.8964,-5.132414673,0.8963,-4.811352704,0.9074,-4.098269308,0.9119,-3.66174277,0.9228,-3.2644011};
#define Filip_N 82
#define Filip_C 2

#define MAXN 82
#define MAXP 11
static double A[MAXN][MAXP], Y[MAXN];
static int bad = 0;
static double dabs(double x) { return x < 0 ? -x : x; }
static double logd(double y) {   // natural logarithm, y > 0
    union { double d; u64 u; } v; v.d = y; int e = (int)((v.u >> 52) & 2047) - 1023; v.u = (v.u & 0xFFFFFFFFFFFFFull) | (1023ull << 52);
    double m = v.d; if (m > 1.4142135623730951) { m *= 0.5; e++; }
    double z = (m - 1.0) / (m + 1.0), z2 = z * z, t = z, s = z;
    for (int i = 1; i < 30; i++) { t *= z2; s += t / (double)(2 * i + 1); }
    return 2.0 * s + (double)e * 0.6931471805599453;
}
static double lre(double got, double cert) {
    if (cert == 0.0) return -99.0;
    double e = dabs(got - cert) / dabs(cert);
    if (e == 0.0) return 15.0;
    double v = -logd(e) / 2.302585092994046;
    return v > 15.0 ? 15.0 : v;
}
static void p1(double v) { if (v < 0) { pstr("-"); v = -v; } i64 t = (i64)(v * 10.0 + 0.5); pdec(t / 10); pstr("."); pdec(t % 10); }

// least squares for A (n x p), Y: returns coefficients b, standard deviations sd, residual sd, r2
static void solve_lls(int n, int p, double *b, double *sd, double *rs, double *r2) {
    double ty = 0; for (int i = 0; i < n; i++) ty += Y[i]; double ym = ty / (double)n; double tss = 0; for (int i = 0; i < n; i++) tss += (Y[i] - ym) * (Y[i] - ym);
    double R[MAXP][MAXP]; double qty[MAXN]; double rss = 0;
#ifdef NEG_NORMAL
    double M[MAXP][MAXP + 1];
    for (int i = 0; i < p; i++) { for (int j = 0; j < p; j++) { double s = 0; for (int k = 0; k < n; k++) s += A[k][i] * A[k][j]; M[i][j] = s; } double s = 0; for (int k = 0; k < n; k++) s += A[k][i] * Y[k]; M[i][p] = s; }
    // Cholesky M = L L^T, then solve
    double Lc[MAXP][MAXP];
    for (int i = 0; i < p; i++) for (int j = 0; j <= i; j++) { double s = M[i][j]; for (int k = 0; k < j; k++) s -= Lc[i][k] * Lc[j][k]; if (i == j) Lc[i][j] = paper_sqrt(s > 1e-300 ? s : 1e-300); else Lc[i][j] = s / Lc[j][j]; }
    double z[MAXP]; for (int i = 0; i < p; i++) { double s = M[i][p]; for (int k = 0; k < i; k++) s -= Lc[i][k] * z[k]; z[i] = s / Lc[i][i]; }
    for (int i = p - 1; i >= 0; i--) { double s = z[i]; for (int k = i + 1; k < p; k++) s -= Lc[k][i] * b[k]; b[i] = s / Lc[i][i]; }
    for (int i = 0; i < n; i++) { double f = Y[i]; for (int j = 0; j < p; j++) f -= A[i][j] * b[j]; rss += f * f; }
    double sig = paper_sqrt(rss / (double)(n - p));
    // (A^T A)^-1 diagonal from the Cholesky factor: solve for each unit vector
    for (int c = 0; c < p; c++) { double e[MAXP], w[MAXP]; for (int i = 0; i < p; i++) e[i] = i == c ? 1.0 : 0.0;
        for (int i = 0; i < p; i++) { double s = e[i]; for (int k = 0; k < i; k++) s -= Lc[i][k] * w[k]; w[i] = s / Lc[i][i]; }
        double x[MAXP]; for (int i = p - 1; i >= 0; i--) { double s = w[i]; for (int k = i + 1; k < p; k++) s -= Lc[k][i] * x[k]; x[i] = s / Lc[i][i]; }
        sd[c] = sig * paper_sqrt(x[c]); }
    (void)R; (void)qty;
#else
    for (int i = 0; i < n; i++) qty[i] = Y[i];
    for (int k = 0; k < p; k++) {
        double nrm = 0; for (int i = k; i < n; i++) nrm += A[i][k] * A[i][k]; nrm = paper_sqrt(nrm);
        if (nrm == 0.0) continue;
        double alpha = A[k][k] > 0 ? -nrm : nrm;
        double v[MAXN]; for (int i = k; i < n; i++) v[i] = A[i][k]; v[k] -= alpha;
        double vv = 0; for (int i = k; i < n; i++) vv += v[i] * v[i];
        if (vv == 0.0) continue;
        for (int j = k; j < p; j++) { double d = 0; for (int i = k; i < n; i++) d += v[i] * A[i][j]; double beta = 2.0 * d / vv; for (int i = k; i < n; i++) A[i][j] -= beta * v[i]; }
        { double d = 0; for (int i = k; i < n; i++) d += v[i] * qty[i]; double beta = 2.0 * d / vv; for (int i = k; i < n; i++) qty[i] -= beta * v[i]; }
    }
    for (int i = 0; i < p; i++) for (int j = 0; j < p; j++) R[i][j] = j >= i ? A[i][j] : 0.0;
    for (int i = p - 1; i >= 0; i--) { double s = qty[i]; for (int k = i + 1; k < p; k++) s -= R[i][k] * b[k]; b[i] = s / R[i][i]; }
    for (int i = p; i < n; i++) rss += qty[i] * qty[i];
    double sig = paper_sqrt(rss / (double)(n - p));
    // R^-1 by back substitution, then sd_j = sigma * norm of row j of R^-1
    double Ri[MAXP][MAXP];
    for (int c = 0; c < p; c++) { for (int i = p - 1; i >= 0; i--) { double s = i == c ? 1.0 : 0.0; for (int k = i + 1; k < p; k++) s -= R[i][k] * Ri[k][c]; Ri[i][c] = s / R[i][i]; } }
    for (int j = 0; j < p; j++) { double s = 0; for (int c = 0; c < p; c++) s += Ri[j][c] * Ri[j][c]; sd[j] = sig * paper_sqrt(s); }
#endif
    *rs = paper_sqrt(rss / (double)(n - p)); *r2 = 1.0 - rss / tss;
}

static void run(const char *name, int n, int p, const double *est, const double *sdc, double residc, double r2c, double t_est, double t_sd, double t_r) {
    double b[MAXP], sd[MAXP], rs, r2;
    solve_lls(n, p, b, sd, &rs, &r2);
    double worst_e = 99, worst_s = 99;
    for (int j = 0; j < p; j++) { double l = lre(b[j], est[j]); if (l < worst_e) worst_e = l; if (sdc[j] != 0.0) { double ls = lre(sd[j], sdc[j]); if (ls < worst_s) worst_s = ls; } }
    double lr = residc != 0.0 ? lre(rs, residc) : 99.0, l2 = lre(r2, r2c);
    pstr(name); pstr(": n = "); pdec(n); pstr(", parameters = "); pdec(p); pstr("\n");
    pstr("  worst LRE of the estimates "); p1(worst_e); pstr(" (required "); p1(t_est); pstr(")\n");
    if (worst_s < 90) { pstr("  worst LRE of their standard deviations "); p1(worst_s); pstr(" (required "); p1(t_sd); pstr(")\n"); }
    if (residc != 0.0) { pstr("  LRE of the residual standard deviation "); p1(lr); pstr(", of R-squared "); p1(l2); pstr(" (required "); p1(t_r); pstr(")\n"); }
    else { pstr("  certified residual standard deviation is 0 (computed "); { double a = rs; union { double d; u64 u; } v; v.d = a; phex(v.u, 16); } pstr(" as a bit pattern, relative LRE undefined)\n"); }
    int ok = worst_e >= t_est && (worst_s > 90 || worst_s >= t_sd) && (residc == 0.0 || (lr >= t_r && l2 >= t_r));
    pstr(ok ? "  ok\n" : "  FAIL\n"); if (!ok) bad++;
}

int paper_main(void) {
    // Longley: y, x1..x6, intercept
    for (int i = 0; i < Longley_N; i++) { Y[i] = Longley_data[i * Longley_C]; A[i][0] = 1.0; for (int j = 1; j < 7; j++) A[i][j] = Longley_data[i * Longley_C + j]; }
    run("Longley", Longley_N, 7, Longley_est, Longley_sd, Longley_resid, Longley_r2, 9.0, 8.0, 9.0);
    // Wampler 1, 4, 5: polynomial of degree 5 in x = 0..20
    for (int s = 0; s < 3; s++) {
        const double *d = s == 0 ? Wampler1_data : (s == 1 ? Wampler4_data : Wampler5_data);
        for (int i = 0; i < 21; i++) { Y[i] = d[i * 2]; double x = d[i * 2 + 1], pw = 1.0; for (int j = 0; j < 6; j++) { A[i][j] = pw; pw *= x; } }
        if (s == 0) run("Wampler1", 21, 6, Wampler1_est, Wampler1_sd, Wampler1_resid, Wampler1_r2, 7.0, 0.0, 0.0);
        else if (s == 1) run("Wampler4", 21, 6, Wampler4_est, Wampler4_sd, Wampler4_resid, Wampler4_r2, 5.0, 5.0, 6.0);
        else run("Wampler5", 21, 6, Wampler5_est, Wampler5_sd, Wampler5_resid, Wampler5_r2, 5.0, 5.0, 6.0);
    }
    // Filip: polynomial of degree 10
    for (int i = 0; i < Filip_N; i++) { Y[i] = Filip_data[i * 2]; double x = Filip_data[i * 2 + 1], pw = 1.0; for (int j = 0; j < 11; j++) { A[i][j] = pw; pw *= x; } }
    run("Filip", Filip_N, 11, Filip_est, Filip_sd, Filip_resid, Filip_r2, 5.0, 4.0, 5.0);
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
