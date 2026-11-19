/* elf-workload dp: matrix chain multiplication, edit distance (cross-checked 2D vs 1D DP),
 * longest increasing subsequence in O(n log n) with reconstruction (cross-checked vs O(n^2) DP),
 * subset sum with reconstruction, and Catalan numbers via DP recurrence vs binomial formula.
 * Provenance: original work written for ounce-core (MIT); textbook algorithms. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

/* ---- Matrix chain multiplication ---- */
static int64_t mcm_memo[16][16];
static int64_t mcm_rec(const int *p, int i, int j) {
    if (i == j) return 0;
    if (mcm_memo[i][j] >= 0) return mcm_memo[i][j];
    int64_t best = 0x7fffffffffffffffLL;
    for (int k = i; k < j; k++) {
        int64_t cost = mcm_rec(p, i, k) + mcm_rec(p, k + 1, j) + (int64_t)p[i - 1] * p[k] * p[j];
        if (cost < best) best = cost;
    }
    return mcm_memo[i][j] = best;
}

static int test_matrix_chain(FILE *o) {
    int fails = 0;
    const int p1[] = {30, 35, 15, 5, 10, 20, 25}; /* CLRS textbook: 6 matrices -> 15125 */
    const int n1 = 6;
    int64_t dp[16][16] = {{0}};
    for (int len = 2; len <= n1; len++) {
        for (int i = 1; i <= n1 - len + 1; i++) {
            int j = i + len - 1;
            dp[i][j] = 0x7fffffffffffffffLL;
            for (int k = i; k < j; k++) {
                int64_t cost = dp[i][k] + dp[k + 1][j] + (int64_t)p1[i - 1] * p1[k] * p1[j];
                if (cost < dp[i][j]) dp[i][j] = cost;
            }
        }
    }
    const int p2[] = {12, 5, 40, 8, 25, 10, 30, 7, 15, 22, 9};
    const int n2 = 10;
    int64_t dp2[16][16] = {{0}};
    for (int len = 2; len <= n2; len++) {
        for (int i = 1; i <= n2 - len + 1; i++) {
            int j = i + len - 1;
            dp2[i][j] = 0x7fffffffffffffffLL;
            for (int k = i; k < j; k++) {
                int64_t cost = dp2[i][k] + dp2[k + 1][j] + (int64_t)p2[i - 1] * p2[k] * p2[j];
                if (cost < dp2[i][j]) dp2[i][j] = cost;
            }
        }
    }
    for (int i = 0; i < 16; i++) for (int j = 0; j < 16; j++) mcm_memo[i][j] = -1;
    int64_t rec2 = mcm_rec(p2, 1, n2);
    if (dp[1][n1] != 15125 || dp2[1][n2] != rec2) fails++;
    fprintf(o, "matrix_chain: clrs=%lld seq10=%lld rec=%lld\n",
            (long long)dp[1][n1], (long long)dp2[1][n2], (long long)rec2);
    return fails;
}

/* ---- Edit distance (2D vs 1D space-optimized) ---- */
static int edit_2d(const char *a, const char *b) {
    int la = (int)strlen(a), lb = (int)strlen(b);
    int dp[64][64];
    for (int i = 0; i <= la; i++) dp[i][0] = i;
    for (int j = 0; j <= lb; j++) dp[0][j] = j;
    for (int i = 1; i <= la; i++) {
        for (int j = 1; j <= lb; j++) {
            int best = dp[i - 1][j - 1] + (a[i - 1] != b[j - 1]);
            if (dp[i - 1][j] + 1 < best) best = dp[i - 1][j] + 1;
            if (dp[i][j - 1] + 1 < best) best = dp[i][j - 1] + 1;
            dp[i][j] = best;
        }
    }
    return dp[la][lb];
}

static int edit_1d(const char *a, const char *b) {
    int la = (int)strlen(a), lb = (int)strlen(b);
    int row[64];
    for (int j = 0; j <= lb; j++) row[j] = j;
    for (int i = 1; i <= la; i++) {
        int prev = row[0];
        row[0] = i;
        for (int j = 1; j <= lb; j++) {
            int save = row[j];
            int best = prev + (a[i - 1] != b[j - 1]);
            if (row[j] + 1 < best) best = row[j] + 1;
            if (row[j - 1] + 1 < best) best = row[j - 1] + 1;
            row[j] = best;
            prev = save;
        }
    }
    return row[lb];
}

static int test_edit_distance(FILE *o) {
    int fails = 0;
    const char *pairs[][2] = {
        {"kitten", "sitting"},
        {"saturday", "sunday"},
        {"intention", "execution"},
        {"abracadabra", "avada_kedavra"},
        {"", "abcdef"},
    };
    const int want[] = {3, 3, 5, 7, 6};
    for (unsigned i = 0; i < sizeof pairs / sizeof pairs[0]; i++) {
        int d2 = edit_2d(pairs[i][0], pairs[i][1]);
        int d1 = edit_1d(pairs[i][0], pairs[i][1]);
        if (d2 != d1 || d2 != want[i]) fails++;
    }
    fprintf(o, "edit_distance: kitten=%d saturday=%d intention=%d fails=%d\n",
            edit_2d("kitten", "sitting"), edit_2d("saturday", "sunday"),
            edit_2d("intention", "execution"), fails);
    return fails;
}

/* ---- LIS O(n log n) with reconstruction vs O(n^2) DP ---- */
static int test_lis(FILE *o) {
    int fails = 0;
    enum { N = 128 };
    int a[N];
    uint32_t s = 0x9e3779b9u;
    for (int i = 0; i < N; i++) {
        s = s * 1664525u + 1013904223u;
        a[i] = (int)((s >> 16) % 200);
    }

    int tails[N], tail_idx[N], parent[N], len = 0;
    for (int i = 0; i < N; i++) {
        int lo = 0, hi = len;
        while (lo < hi) {
            int mid = lo + (hi - lo) / 2;
            if (tails[mid] < a[i]) lo = mid + 1;
            else hi = mid;
        }
        tails[lo] = a[i];
        tail_idx[lo] = i;
        parent[i] = (lo > 0) ? tail_idx[lo - 1] : -1;
        if (lo == len) len++;
    }

    int seq[N], idx = tail_idx[len - 1];
    for (int k = len - 1; k >= 0; k--) {
        seq[k] = a[idx];
        idx = parent[idx];
    }
    int strictly_inc = (idx == -1);
    for (int k = 1; k < len; k++) {
        if (seq[k] <= seq[k - 1]) strictly_inc = 0;
    }

    int dp[N], best_quad = 0;
    for (int i = 0; i < N; i++) {
        dp[i] = 1;
        for (int j = 0; j < i; j++) {
            if (a[j] < a[i] && dp[j] + 1 > dp[i]) dp[i] = dp[j] + 1;
        }
        if (dp[i] > best_quad) best_quad = dp[i];
    }

    if (!strictly_inc || len != best_quad) fails++;
    fprintf(o, "lis: nlogn=%d quad=%d strictly_inc=%d\n", len, best_quad, strictly_inc);
    return fails;
}

/* ---- Subset sum with reconstruction ---- */
static int test_subset_sum(FILE *o) {
    int fails = 0;
    const int w[] = {17, 29, 43, 61, 73, 97, 113, 131, 157, 181, 211, 241};
    const int nw = (int)(sizeof w / sizeof w[0]);
    enum { MAXT = 1500 };
    int prev[MAXT + 1];
    for (int s = 0; s <= MAXT; s++) prev[s] = -1;
    prev[0] = -2;
    for (int i = 0; i < nw; i++) {
        for (int s = MAXT; s >= w[i]; s--) {
            if (prev[s] == -1 && prev[s - w[i]] != -1) prev[s] = i;
        }
    }
    const int t_ok = 777;
    int check_sum = 0, used = 0;
    if (prev[t_ok] == -1) {
        fails++;
    } else {
        int cur = t_ok;
        while (cur > 0) {
            int i = prev[cur];
            check_sum += w[i];
            cur -= w[i];
            used++;
        }
    }
    /* All weights are > 16 except none; target 16 is impossible */
    int impossible = (prev[16] == -1);
    if (check_sum != t_ok || !impossible) fails++;
    fprintf(o, "subset_sum: target=%d recon=%d items=%d impossible16=%d\n",
            t_ok, check_sum, used, impossible);
    return fails;
}

/* ---- Catalan numbers ---- */
static int test_catalan(FILE *o) {
    int fails = 0;
    uint64_t cat[21] = {0};
    cat[0] = 1;
    for (int n = 1; n <= 20; n++) {
        for (int i = 0; i < n; i++) cat[n] += cat[i] * cat[n - 1 - i];
    }
    /* Cross-check C_n = C(2n, n) / (n + 1) using exact progressive Pascal row */
    uint64_t binom[41][41] = {{0}};
    for (int i = 0; i <= 40; i++) {
        binom[i][0] = binom[i][i] = 1;
        for (int j = 1; j < i; j++) binom[i][j] = binom[i - 1][j - 1] + binom[i - 1][j];
    }
    int match = 1;
    for (int n = 0; n <= 20; n++) {
        if (cat[n] != binom[2 * n][n] / (uint64_t)(n + 1)) match = 0;
    }
    if (!match || cat[10] != 16796ULL || cat[15] != 9694845ULL || cat[19] != 1767263190ULL) fails++;
    fprintf(o, "catalan: C10=%llu C15=%llu C20=%llu match=%d\n",
            (unsigned long long)cat[10], (unsigned long long)cat[15],
            (unsigned long long)cat[20], match);
    return fails;
}

int workload_run(FILE *o) {
    int fails = 0;
    fails += test_matrix_chain(o);
    fails += test_edit_distance(o);
    fails += test_lis(o);
    fails += test_subset_sum(o);
    fails += test_catalan(o);
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
