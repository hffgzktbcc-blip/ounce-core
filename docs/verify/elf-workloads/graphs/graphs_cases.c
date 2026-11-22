/* elf-workload graphs: Tarjan strongly connected components, Kahn topological sort,
 * Bellman-Ford shortest paths & negative-cycle detection, Floyd-Warshall all-pairs shortest
 * paths, and Kruskal minimum spanning tree (cross-checked against Prim).
 * Provenance: original work written for ounce-core (MIT); textbook algorithms. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define INF_DIST 1000000000LL

/* ---- Tarjan SCC ---- */
enum { SCC_N = 12 };
static int scc_adj[SCC_N][SCC_N];
static int scc_idx[SCC_N], scc_low[SCC_N], scc_comp[SCC_N], scc_on_stk[SCC_N];
static int scc_stk[SCC_N], scc_top, scc_timer, scc_count;

static void tarjan_dfs(int u) {
    scc_idx[u] = scc_low[u] = ++scc_timer;
    scc_stk[scc_top++] = u;
    scc_on_stk[u] = 1;
    for (int v = 0; v < SCC_N; v++) {
        if (!scc_adj[u][v]) continue;
        if (!scc_idx[v]) {
            tarjan_dfs(v);
            if (scc_low[v] < scc_low[u]) scc_low[u] = scc_low[v];
        } else if (scc_on_stk[v]) {
            if (scc_idx[v] < scc_low[u]) scc_low[u] = scc_idx[v];
        }
    }
    if (scc_low[u] == scc_idx[u]) {
        for (;;) {
            int w = scc_stk[--scc_top];
            scc_on_stk[w] = 0;
            scc_comp[w] = scc_count;
            if (w == u) break;
        }
        scc_count++;
    }
}

static int test_tarjan_scc(FILE *o) {
    int fails = 0;
    memset(scc_adj, 0, sizeof scc_adj);
    /* 4 known SCCs: {0,1,2} -> {3,4} -> {5,6,7,8} -> {9,10}, plus isolated {11} with edge 11->3 */
    int edges[][2] = {
        {0, 1}, {1, 2}, {2, 0},
        {2, 3}, {3, 4}, {4, 3},
        {4, 5}, {5, 6}, {6, 7}, {7, 8}, {8, 5}, {6, 8},
        {8, 9}, {9, 10}, {10, 9},
        {11, 3}
    };
    for (unsigned i = 0; i < sizeof edges / sizeof edges[0]; i++) {
        scc_adj[edges[i][0]][edges[i][1]] = 1;
    }
    memset(scc_idx, 0, sizeof scc_idx);
    memset(scc_on_stk, 0, sizeof scc_on_stk);
    scc_top = scc_timer = scc_count = 0;
    for (int u = 0; u < SCC_N; u++) {
        if (!scc_idx[u]) tarjan_dfs(u);
    }
    /* Verify condensation has no forward-direction violations (Tarjan emits reverse topological order) */
    int dag_ok = 1;
    for (int u = 0; u < SCC_N; u++) {
        for (int v = 0; v < SCC_N; v++) {
            if (scc_adj[u][v] && scc_comp[u] != scc_comp[v] && scc_comp[u] < scc_comp[v]) {
                dag_ok = 0;
            }
        }
    }
    int same012 = (scc_comp[0] == scc_comp[1] && scc_comp[1] == scc_comp[2]);
    int same34 = (scc_comp[3] == scc_comp[4]);
    int same5678 = (scc_comp[5] == scc_comp[6] && scc_comp[6] == scc_comp[7] && scc_comp[7] == scc_comp[8]);
    int same910 = (scc_comp[9] == scc_comp[10]);
    if (scc_count != 5 || !dag_ok || !same012 || !same34 || !same5678 || !same910) fails++;
    fprintf(o, "tarjan_scc: count=%d dag_ok=%d groups=%d%d%d%d\n",
            scc_count, dag_ok, same012, same34, same5678, same910);
    return fails;
}

/* ---- Kahn topological sort ---- */
static int kahn_topo(int n, const int adj[16][16], int order[16]) {
    int indeg[16] = {0};
    for (int u = 0; u < n; u++) {
        for (int v = 0; v < n; v++) {
            if (adj[u][v]) indeg[v]++;
        }
    }
    int q[16], head = 0, tail = 0, out_n = 0;
    for (int u = 0; u < n; u++) {
        if (indeg[u] == 0) q[tail++] = u;
    }
    while (head < tail) {
        int u = q[head++];
        order[out_n++] = u;
        for (int v = 0; v < n; v++) {
            if (adj[u][v] && --indeg[v] == 0) {
                q[tail++] = v;
            }
        }
    }
    return out_n;
}

static int test_topo_sort(FILE *o) {
    int fails = 0;
    int adj[16][16] = {{0}};
    const int n = 10;
    int dag_edges[][2] = {
        {0, 2}, {0, 3}, {1, 3}, {1, 4}, {2, 5}, {3, 5}, {3, 6}, {4, 6},
        {5, 7}, {6, 7}, {6, 8}, {7, 9}, {8, 9}
    };
    for (unsigned i = 0; i < sizeof dag_edges / sizeof dag_edges[0]; i++) {
        adj[dag_edges[i][0]][dag_edges[i][1]] = 1;
    }
    int order[16], pos[16];
    int count = kahn_topo(n, adj, order);
    for (int i = 0; i < count; i++) pos[order[i]] = i;
    int valid = (count == n);
    for (int u = 0; u < n && valid; u++) {
        for (int v = 0; v < n; v++) {
            if (adj[u][v] && pos[u] >= pos[v]) valid = 0;
        }
    }
    /* Now add a cycle edge 9 -> 3 and verify cycle detection */
    adj[9][3] = 1;
    int cyc_count = kahn_topo(n, adj, order);
    if (!valid || cyc_count >= n) fails++;
    fprintf(o, "topo_sort: dag_count=%d valid=%d cycle_count=%d\n", count, valid, cyc_count);
    return fails;
}

/* ---- Bellman-Ford + Floyd-Warshall ---- */
typedef struct { int u, v; int64_t w; } w_edge;

static int test_shortest_paths(FILE *o) {
    int fails = 0;
    enum { N = 8 };
    const w_edge edges[] = {
        {0, 1, 6}, {0, 2, 7}, {1, 2, 8}, {1, 3, 5}, {1, 4, -4},
        {2, 3, -3}, {2, 4, 9}, {3, 1, -2}, {4, 0, 2}, {4, 3, 7},
        {3, 5, 4}, {5, 6, -1}, {6, 7, 3}, {4, 7, 10}, {1, 6, 12}
    };
    const int m = (int)(sizeof edges / sizeof edges[0]);

    int64_t bf[N];
    for (int i = 0; i < N; i++) bf[i] = INF_DIST;
    bf[0] = 0;
    for (int iter = 0; iter < N - 1; iter++) {
        for (int i = 0; i < m; i++) {
            if (bf[edges[i].u] < INF_DIST && bf[edges[i].u] + edges[i].w < bf[edges[i].v]) {
                bf[edges[i].v] = bf[edges[i].u] + edges[i].w;
            }
        }
    }
    int neg_cyc = 0;
    for (int i = 0; i < m; i++) {
        if (bf[edges[i].u] < INF_DIST && bf[edges[i].u] + edges[i].w < bf[edges[i].v]) {
            neg_cyc = 1;
        }
    }

    int64_t fw[N][N];
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) fw[i][j] = (i == j) ? 0 : INF_DIST;
    }
    for (int i = 0; i < m; i++) {
        if (edges[i].w < fw[edges[i].u][edges[i].v]) fw[edges[i].u][edges[i].v] = edges[i].w;
    }
    for (int k = 0; k < N; k++) {
        for (int i = 0; i < N; i++) {
            if (fw[i][k] >= INF_DIST) continue;
            for (int j = 0; j < N; j++) {
                if (fw[k][j] < INF_DIST && fw[i][k] + fw[k][j] < fw[i][j]) {
                    fw[i][j] = fw[i][k] + fw[k][j];
                }
            }
        }
    }
    int match = !neg_cyc;
    for (int i = 0; i < N; i++) {
        if (bf[i] != fw[0][i]) match = 0;
    }
    /* Negative cycle test: add edge 4 -> 1 with weight -10 so 1->4->1 has weight -14 */
    int64_t bf2[N];
    for (int i = 0; i < N; i++) bf2[i] = INF_DIST;
    bf2[0] = 0;
    for (int iter = 0; iter < N - 1; iter++) {
        for (int i = 0; i < m; i++) {
            if (bf2[edges[i].u] < INF_DIST && bf2[edges[i].u] + edges[i].w < bf2[edges[i].v]) {
                bf2[edges[i].v] = bf2[edges[i].u] + edges[i].w;
            }
        }
        if (bf2[4] < INF_DIST && bf2[4] - 10 < bf2[1]) bf2[1] = bf2[4] - 10;
    }
    int neg_detected = 0;
    for (int i = 0; i < m; i++) {
        if (bf2[edges[i].u] < INF_DIST && bf2[edges[i].u] + edges[i].w < bf2[edges[i].v]) neg_detected = 1;
    }
    if (bf2[4] < INF_DIST && bf2[4] - 10 < bf2[1]) neg_detected = 1;

    if (!match || !neg_detected) fails++;
    fprintf(o, "shortest_paths: bf_vs_fw=%d neg_cyc_detected=%d dist0=(%lld,%lld,%lld,%lld,%lld,%lld,%lld,%lld)\n",
            match, neg_detected,
            (long long)bf[0], (long long)bf[1], (long long)bf[2], (long long)bf[3],
            (long long)bf[4], (long long)bf[5], (long long)bf[6], (long long)bf[7]);
    return fails;
}

/* ---- Kruskal MST cross-checked with Prim ---- */
static int cmp_edge(const void *a, const void *b) {
    const w_edge *ea = (const w_edge *)a;
    const w_edge *eb = (const w_edge *)b;
    if (ea->w != eb->w) return (ea->w < eb->w) ? -1 : 1;
    if (ea->u != eb->u) return (ea->u < eb->u) ? -1 : 1;
    return (ea->v < eb->v) ? -1 : (ea->v > eb->v);
}

static int uf_find(int *p, int x) {
    while (p[x] != x) { p[x] = p[p[x]]; x = p[x]; }
    return x;
}

static int test_mst(FILE *o) {
    int fails = 0;
    enum { N = 16 };
    w_edge edges[80];
    int m = 0;
    int64_t adj[N][N];
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) adj[i][j] = INF_DIST;
    }
    uint32_t rng = 0x13572468u;
    for (int i = 0; i < N; i++) {
        int next = (i + 1) % N;
        rng = rng * 1664525u + 1013904223u;
        int64_t w = (int64_t)(1 + (rng >> 16) % 50);
        edges[m++] = (w_edge){i, next, w};
        adj[i][next] = adj[next][i] = w;
    }
    for (int i = 0; i < N; i++) {
        int a = i, b = (i * 5 + 3) % N;
        if (a == b || adj[a][b] < INF_DIST) continue;
        rng = rng * 1664525u + 1013904223u;
        int64_t w = (int64_t)(1 + (rng >> 16) % 90);
        edges[m++] = (w_edge){a, b, w};
        adj[a][b] = adj[b][a] = w;
    }
    qsort(edges, (size_t)m, sizeof edges[0], cmp_edge);
    int p[N];
    for (int i = 0; i < N; i++) p[i] = i;
    int64_t kruskal_w = 0;
    int k_edges = 0;
    for (int i = 0; i < m; i++) {
        int ru = uf_find(p, edges[i].u), rv = uf_find(p, edges[i].v);
        if (ru != rv) {
            p[ru] = rv;
            kruskal_w += edges[i].w;
            k_edges++;
        }
    }

    /* Prim's MST verification */
    int used[N] = {0};
    int64_t key[N];
    for (int i = 0; i < N; i++) key[i] = INF_DIST;
    key[0] = 0;
    int64_t prim_w = 0;
    for (int iter = 0; iter < N; iter++) {
        int best = -1;
        for (int u = 0; u < N; u++) {
            if (!used[u] && (best < 0 || key[u] < key[best])) best = u;
        }
        used[best] = 1;
        prim_w += key[best];
        for (int v = 0; v < N; v++) {
            if (!used[v] && adj[best][v] < key[v]) key[v] = adj[best][v];
        }
    }
    if (k_edges != N - 1 || kruskal_w != prim_w) fails++;
    fprintf(o, "mst: edges=%d kruskal=%lld prim=%lld\n", k_edges, (long long)kruskal_w, (long long)prim_w);
    return fails;
}

int workload_run(FILE *o) {
    int fails = 0;
    fails += test_tarjan_scc(o);
    fails += test_topo_sort(o);
    fails += test_shortest_paths(o);
    fails += test_mst(o);
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
