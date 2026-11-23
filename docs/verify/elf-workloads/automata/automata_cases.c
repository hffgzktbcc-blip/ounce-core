/* elf-workload automata: Knuth-Morris-Pratt, Boyer-Moore-Horspool, Aho-Corasick multi-pattern
 * search (all cross-checked against brute-force scans), and a DFA tokenizing lexer.
 * Provenance: original work written for ounce-core (MIT); textbook string/automata algorithms. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

static int naive_count(const char *txt, const char *pat, int *pos_out, int max_out) {
    int n = (int)strlen(txt), m = (int)strlen(pat), cnt = 0;
    for (int i = 0; i + m <= n; i++) {
        if (memcmp(txt + i, pat, (size_t)m) == 0) {
            if (pos_out && cnt < max_out) pos_out[cnt] = i;
            cnt++;
        }
    }
    return cnt;
}

static int kmp_search(const char *txt, const char *pat, int *pos_out, int max_out) {
    int n = (int)strlen(txt), m = (int)strlen(pat);
    int pi[128] = {0};
    for (int i = 1, k = 0; i < m; i++) {
        while (k > 0 && pat[i] != pat[k]) k = pi[k - 1];
        if (pat[i] == pat[k]) k++;
        pi[i] = k;
    }
    int cnt = 0;
    for (int i = 0, q = 0; i < n; i++) {
        while (q > 0 && txt[i] != pat[q]) q = pi[q - 1];
        if (txt[i] == pat[q]) q++;
        if (q == m) {
            if (pos_out && cnt < max_out) pos_out[cnt] = i - m + 1;
            cnt++;
            q = pi[q - 1];
        }
    }
    return cnt;
}

static int bmh_search(const char *txt, const char *pat, int *pos_out, int max_out) {
    int n = (int)strlen(txt), m = (int)strlen(pat);
    int shift[256];
    for (int c = 0; c < 256; c++) shift[c] = m;
    for (int i = 0; i < m - 1; i++) shift[(unsigned char)pat[i]] = m - 1 - i;
    int i = 0, cnt = 0;
    while (i + m <= n) {
        int j = m - 1;
        while (j >= 0 && txt[i + j] == pat[j]) j--;
        if (j < 0) {
            if (pos_out && cnt < max_out) pos_out[cnt] = i;
            cnt++;
            i += 1; /* advance by 1 to enumerate overlapping matches */
        } else {
            i += shift[(unsigned char)txt[i + m - 1]];
        }
    }
    return cnt;
}

/* ---- Aho-Corasick ---- */
enum { AC_MAX = 128 };
static int ac_next[AC_MAX][26], ac_fail[AC_MAX], ac_out[AC_MAX], ac_nodes;

static void ac_build(const char *const *pats, int np) {
    memset(ac_next, -1, sizeof ac_next);
    memset(ac_fail, 0, sizeof ac_fail);
    memset(ac_out, 0, sizeof ac_out);
    ac_nodes = 1;
    for (int i = 0; i < np; i++) {
        int cur = 0;
        for (const char *p = pats[i]; *p; p++) {
            int c = *p - 'a';
            if (ac_next[cur][c] < 0) ac_next[cur][c] = ac_nodes++;
            cur = ac_next[cur][c];
        }
        ac_out[cur]++;
    }
    int q[AC_MAX], hd = 0, tl = 0;
    for (int c = 0; c < 26; c++) {
        if (ac_next[0][c] >= 0) {
            ac_fail[ac_next[0][c]] = 0;
            q[tl++] = ac_next[0][c];
        } else {
            ac_next[0][c] = 0;
        }
    }
    while (hd < tl) {
        int u = q[hd++];
        for (int c = 0; c < 26; c++) {
            int v = ac_next[u][c];
            if (v >= 0) {
                ac_fail[v] = ac_next[ac_fail[u]][c];
                ac_out[v] += ac_out[ac_fail[v]];
                q[tl++] = v;
            } else {
                ac_next[u][c] = ac_next[ac_fail[u]][c];
            }
        }
    }
}

static int ac_count(const char *txt) {
    int cur = 0, total = 0;
    for (const char *p = txt; *p; p++) {
        cur = ac_next[cur][*p - 'a'];
        total += ac_out[cur];
    }
    return total;
}

/* ---- DFA lexer ---- */
typedef enum { TOK_ID, TOK_NUM, TOK_OP2, TOK_OP1, TOK_ERR } tok_kind;

static int dfa_lex(const char *src, tok_kind *kinds, int max_tok) {
    enum { S_START, S_ID, S_NUM, S_EQ, S_BANG, S_LT, S_GT, S_AMP, S_BAR } st = S_START;
    int nt = 0;
    for (int i = 0; ; i++) {
        unsigned char ch = (unsigned char)src[i];
        for (;;) {
            if (st == S_START) {
                if (ch == 0) return nt;
                if (ch == ' ' || ch == '\t' || ch == '\n') break;
                if ((ch >= 'a' && ch <= 'z') || ch == '_') { st = S_ID; break; }
                if (ch >= '0' && ch <= '9') { st = S_NUM; break; }
                if (ch == '=') { st = S_EQ; break; }
                if (ch == '!') { st = S_BANG; break; }
                if (ch == '<') { st = S_LT; break; }
                if (ch == '>') { st = S_GT; break; }
                if (ch == '&') { st = S_AMP; break; }
                if (ch == '|') { st = S_BAR; break; }
                if (nt < max_tok) kinds[nt++] = (ch == '+' || ch == '-' || ch == '*' || ch == ';' || ch == '(' || ch == ')') ? TOK_OP1 : TOK_ERR;
                break;
            } else if (st == S_ID) {
                if ((ch >= 'a' && ch <= 'z') || (ch >= '0' && ch <= '9') || ch == '_') break;
                if (nt < max_tok) kinds[nt++] = TOK_ID;
                st = S_START;
            } else if (st == S_NUM) {
                if (ch >= '0' && ch <= '9') break;
                if (nt < max_tok) kinds[nt++] = TOK_NUM;
                st = S_START;
            } else if (st == S_EQ || st == S_BANG || st == S_LT || st == S_GT) {
                if (ch == '=') { if (nt < max_tok) kinds[nt++] = TOK_OP2; st = S_START; break; }
                if (nt < max_tok) kinds[nt++] = TOK_OP1;
                st = S_START;
            } else if (st == S_AMP) {
                if (ch == '&') { if (nt < max_tok) kinds[nt++] = TOK_OP2; st = S_START; break; }
                if (nt < max_tok) kinds[nt++] = TOK_OP1;
                st = S_START;
            } else { /* S_BAR */
                if (ch == '|') { if (nt < max_tok) kinds[nt++] = TOK_OP2; st = S_START; break; }
                if (nt < max_tok) kinds[nt++] = TOK_OP1;
                st = S_START;
            }
        }
    }
}

int workload_run(FILE *o) {
    int fails = 0;
    const char *texts[] = {"ababaababaaba", "aaaaaa", "mississippi_ississippi"};
    const char *pats[] = {"aba", "aaa", "issi"};
    int match_sum = 0;
    for (unsigned i = 0; i < 3; i++) {
        int p0[32], p1[32], p2[32];
        int c0 = naive_count(texts[i], pats[i], p0, 32);
        int c1 = kmp_search(texts[i], pats[i], p1, 32);
        int c2 = bmh_search(texts[i], pats[i], p2, 32);
        if (c0 != c1 || c0 != c2 || memcmp(p0, p1, (size_t)c0 * sizeof(int)) != 0
            || memcmp(p0, p2, (size_t)c0 * sizeof(int)) != 0) fails++;
        match_sum += c0;
    }

    const char *dict[] = {"he", "she", "hers", "his", "a", "ab", "bc", "bca", "c", "caa"};
    ac_build(dict, 10);
    const char *ac_inputs[] = {"ushers", "abccabcaabbc", "shehishehers"};
    int ac_total = 0;
    for (unsigned ti = 0; ti < 3; ti++) {
        int got = ac_count(ac_inputs[ti]);
        int want = 0;
        for (int p = 0; p < 10; p++) want += naive_count(ac_inputs[ti], dict[p], NULL, 0);
        if (got != want) fails++;
        ac_total += got;
    }

    tok_kind toks[64];
    const char *code = "x_1 = 42 + y2 * (z <= 10 && w != 0 || a == 3);";
    int ntok = dfa_lex(code, toks, 64);
    const tok_kind want_toks[] = {
        TOK_ID, TOK_OP1, TOK_NUM, TOK_OP1, TOK_ID, TOK_OP1, TOK_OP1,
        TOK_ID, TOK_OP2, TOK_NUM, TOK_OP2, TOK_ID, TOK_OP2, TOK_NUM,
        TOK_OP2, TOK_ID, TOK_OP2, TOK_NUM, TOK_OP1, TOK_OP1
    };
    if (ntok != (int)(sizeof want_toks / sizeof want_toks[0])
        || memcmp(toks, want_toks, sizeof want_toks) != 0) fails++;

    fprintf(o, "string_match: total=%d ac_total=%d dfa_tokens=%d\n", match_sum, ac_total, ntok);
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
