/* elf-workload puzzles: backtracking and dynamic programming with well-known answers:
 * N-queens (8 -> 92, 9 -> 352), permutation and subset counts, a sudoku solver, Conway's Life
 * (glider returns after 4 generations shifted by (1,1)), 0/1 knapsack, longest common
 * subsequence, Levenshtein distance ("kitten"/"sitting" = 3), coin change and Pascal's triangle.
 * Provenance: original work written for ounce-core (MIT); textbook algorithms; the sudoku grid is
 * a widely published easy example. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

static int queens(int n, int row, unsigned cols, unsigned d1, unsigned d2) {
    if (row == n) return 1;
    int c = 0;
    for (int i = 0; i < n; i++) {
        unsigned a = 1u << i, b = 1u << (row + i), d = 1u << (row - i + n - 1);
        if ((cols & a) || (d1 & b) || (d2 & d)) continue;
        c += queens(n, row + 1, cols | a, d1 | b, d2 | d);
    }
    return c;
}

static int perm_count(int n, unsigned used, int prev) { /* permutations with no adjacent consecutive values */
    if (used == (1u << n) - 1) return 1;
    int c = 0;
    for (int i = 0; i < n; i++) {
        if (used & (1u << i)) continue;
        if (prev >= 0 && (i == prev + 1 || i == prev - 1)) continue;
        c += perm_count(n, used | (1u << i), i);
    }
    return c;
}

static int grid[9][9];
static int ok(int r, int c, int v) {
    for (int i = 0; i < 9; i++) if (grid[r][i] == v || grid[i][c] == v) return 0;
    int br = r / 3 * 3, bc = c / 3 * 3;
    for (int i = 0; i < 3; i++) for (int j = 0; j < 3; j++) if (grid[br + i][bc + j] == v) return 0;
    return 1;
}
static unsigned long nodes;
static int solve(int pos) {
    while (pos < 81 && grid[pos / 9][pos % 9]) pos++;
    if (pos == 81) return 1;
    int r = pos / 9, c = pos % 9;
    for (int v = 1; v <= 9; v++) {
        if (!ok(r, c, v)) continue;
        nodes++;
        grid[r][c] = v;
        if (solve(pos + 1)) return 1;
        grid[r][c] = 0;
    }
    return 0;
}

#define W 12
#define H 12
static void life_step(unsigned char a[H][W], unsigned char b[H][W]) {
    for (int y = 0; y < H; y++) for (int x = 0; x < W; x++) {
        int n = 0;
        for (int dy = -1; dy <= 1; dy++) for (int dx = -1; dx <= 1; dx++)
            if (dx || dy) n += a[(y + dy + H) % H][(x + dx + W) % W];
        b[y][x] = a[y][x] ? (n == 2 || n == 3) : (n == 3);
    }
}

static int lcs(const char *a, const char *b) {
    int la = (int)strlen(a), lb = (int)strlen(b);
    static int t[40][40];
    for (int i = 0; i <= la; i++) for (int j = 0; j <= lb; j++)
        t[i][j] = !i || !j ? 0 : a[i - 1] == b[j - 1] ? t[i - 1][j - 1] + 1 : (t[i - 1][j] > t[i][j - 1] ? t[i - 1][j] : t[i][j - 1]);
    return t[la][lb];
}
static int lev(const char *a, const char *b) {
    int la = (int)strlen(a), lb = (int)strlen(b);
    static int t[40][40];
    for (int i = 0; i <= la; i++) for (int j = 0; j <= lb; j++) {
        if (!i) { t[i][j] = j; continue; }
        if (!j) { t[i][j] = i; continue; }
        int best = t[i - 1][j - 1] + (a[i - 1] != b[j - 1]);
        if (t[i - 1][j] + 1 < best) best = t[i - 1][j] + 1;
        if (t[i][j - 1] + 1 < best) best = t[i][j - 1] + 1;
        t[i][j] = best;
    }
    return t[la][lb];
}

int workload_run(FILE *o) {
    int fails = 0;
    int q8 = queens(8, 0, 0, 0, 0), q9 = queens(9, 0, 0, 0, 0);
    fprintf(o, "queens(8)=%d queens(9)=%d\n", q8, q9);
    if (q8 != 92 || q9 != 352) fails++;
    fprintf(o, "perms(8) with no adjacent consecutive values=%d\n", perm_count(8, 0, -1));

    static const char *puzzle[9] = {"530070000", "600195000", "098000060", "800060003", "400803001",
                                    "700020006", "060000280", "000419005", "000080079"};
    for (int r = 0; r < 9; r++) for (int c = 0; c < 9; c++) grid[r][c] = puzzle[r][c] - '0';
    int solved = solve(0);
    fprintf(o, "sudoku solved=%d nodes=%lu\n", solved, nodes);
    for (int r = 0; r < 9; r++) {
        for (int c = 0; c < 9; c++) fputc('0' + grid[r][c], o);
        fputc('\n', o);
    }
    if (!solved) fails++;
    int row_ok = 1;
    for (int r = 0; r < 9; r++) { int s = 0; for (int c = 0; c < 9; c++) s += grid[r][c]; row_ok &= s == 45; }
    if (!row_ok) fails++;

    unsigned char a[H][W] = {{0}}, b[H][W];
    a[0][1] = a[1][2] = a[2][0] = a[2][1] = a[2][2] = 1;     /* glider */
    unsigned char start[H][W];
    memcpy(start, a, sizeof a);
    for (int g = 0; g < 4; g++) { life_step(a, b); memcpy(a, b, sizeof a); }
    int shifted = 1, pop = 0;
    for (int y = 0; y < H; y++) for (int x = 0; x < W; x++) {
        pop += a[y][x];
        if (a[(y + 1) % H][(x + 1) % W] != start[y][x]) shifted = 0;
    }
    fprintf(o, "life glider after 4 gens: population=%d shifted-by-(1,1)=%d\n", pop, shifted);
    if (pop != 5 || !shifted) fails++;
    for (int g = 0; g < 44; g++) { life_step(a, b); memcpy(a, b, sizeof a); } /* 48 gens = one full lap of the 12x12 torus */
    fprintf(o, "after 48 gens identical to start=%d\n", !memcmp(a, start, sizeof a));
    if (memcmp(a, start, sizeof a)) fails++;

    /* 0/1 knapsack */
    static const int wt[] = {12, 7, 11, 8, 9, 6, 14, 3}, val[] = {24, 13, 23, 15, 16, 11, 28, 4};
    int best[51] = {0};
    for (int i = 0; i < 8; i++) for (int c = 50; c >= wt[i]; c--) if (best[c - wt[i]] + val[i] > best[c]) best[c] = best[c - wt[i]] + val[i];
    fprintf(o, "knapsack(50)=%d\n", best[50]);

    fprintf(o, "lcs(AGGTAB,GXTXAYB)=%d lcs(ABCBDAB,BDCABA)=%d\n", lcs("AGGTAB", "GXTXAYB"), lcs("ABCBDAB", "BDCABA"));
    if (lcs("AGGTAB", "GXTXAYB") != 4 || lcs("ABCBDAB", "BDCABA") != 4) fails++;
    fprintf(o, "levenshtein kitten/sitting=%d flaw/lawn=%d empty/abc=%d\n", lev("kitten", "sitting"), lev("flaw", "lawn"), lev("", "abc"));
    if (lev("kitten", "sitting") != 3 || lev("flaw", "lawn") != 2) fails++;

    /* ways to make change for 200 with UK coins, and Pascal row 20 */
    static const int coins[] = {1, 2, 5, 10, 20, 50, 100, 200};
    long ways[201] = {1};
    for (int i = 0; i < 8; i++) for (int c = coins[i]; c <= 200; c++) ways[c] += ways[c - coins[i]];
    fprintf(o, "coin change(200)=%ld\n", ways[200]);
    if (ways[200] != 73682) fails++;
    uint64_t row[21] = {1};
    for (int n = 1; n <= 20; n++) for (int k = n; k > 0; k--) row[k] += row[k - 1];
    fprintf(o, "pascal row 20:");
    for (int k = 0; k <= 20; k++) fprintf(o, " %llu", (unsigned long long)row[k]);
    fputc('\n', o);
    if (row[10] != 184756) fails++;

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
