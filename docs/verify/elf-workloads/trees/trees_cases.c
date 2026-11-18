/* elf-workload trees: pointer-heavy data structures: an AVL tree (insert, delete, lookup, in-order
 * walk, height bound), a binary min-heap used as a priority queue (heap sort and Dijkstra on a
 * small generated graph), union-find with path compression, and a trie with prefix counts.
 * Provenance: original work written for ounce-core (MIT); textbook algorithms. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static uint32_t st = 777;
static uint32_t rnd(void) { st ^= st << 13; st ^= st >> 17; st ^= st << 5; return st; }

typedef struct avl { int key; int h; struct avl *l, *r; } avl;
static int H(avl *n) { return n ? n->h : 0; }
static void upd(avl *n) { n->h = 1 + (H(n->l) > H(n->r) ? H(n->l) : H(n->r)); }
static avl *rot_r(avl *y) { avl *x = y->l; y->l = x->r; x->r = y; upd(y); upd(x); return x; }
static avl *rot_l(avl *x) { avl *y = x->r; x->r = y->l; y->l = x; upd(x); upd(y); return y; }
static avl *bal(avl *n) {
    upd(n);
    int b = H(n->l) - H(n->r);
    if (b > 1) { if (H(n->l->l) < H(n->l->r)) n->l = rot_l(n->l); return rot_r(n); }
    if (b < -1) { if (H(n->r->r) < H(n->r->l)) n->r = rot_r(n->r); return rot_l(n); }
    return n;
}
static avl *ins(avl *n, int k) {
    if (!n) { n = calloc(1, sizeof *n); n->key = k; n->h = 1; return n; }
    if (k < n->key) n->l = ins(n->l, k); else if (k > n->key) n->r = ins(n->r, k); else return n;
    return bal(n);
}
static avl *min_node(avl *n) { while (n->l) n = n->l; return n; }
static avl *del(avl *n, int k) {
    if (!n) return NULL;
    if (k < n->key) n->l = del(n->l, k);
    else if (k > n->key) n->r = del(n->r, k);
    else {
        if (!n->l || !n->r) { avl *c = n->l ? n->l : n->r; free(n); return c; }
        avl *m = min_node(n->r);
        n->key = m->key;
        n->r = del(n->r, m->key);
    }
    return bal(n);
}
static int walk(avl *n, int *prev, int *count) {
    if (!n) return 1;
    int ok = walk(n->l, prev, count);
    if (*count && n->key <= *prev) ok = 0;
    *prev = n->key; (*count)++;
    return ok & walk(n->r, prev, count);
}
static void avl_free(avl *n) { if (n) { avl_free(n->l); avl_free(n->r); free(n); } }

typedef struct { uint32_t pri; int id; } item;
typedef struct { item a[512]; int n; } heap;
static void hpush(heap *h, item x) {
    int i = h->n++;
    while (i && h->a[(i - 1) / 2].pri > x.pri) { h->a[i] = h->a[(i - 1) / 2]; i = (i - 1) / 2; }
    h->a[i] = x;
}
static item hpop(heap *h) {
    item top = h->a[0], x = h->a[--h->n];
    int i = 0;
    for (;;) {
        int c = 2 * i + 1;
        if (c >= h->n) break;
        if (c + 1 < h->n && h->a[c + 1].pri < h->a[c].pri) c++;
        if (h->a[c].pri >= x.pri) break;
        h->a[i] = h->a[c]; i = c;
    }
    if (h->n) h->a[i] = x;
    return top;
}

static int uf[1000];
static int find(int x) { while (uf[x] != x) { uf[x] = uf[uf[x]]; x = uf[x]; } return x; }

typedef struct trie { struct trie *c[26]; int pass, end; } trie;

int workload_run(FILE *o) {
    int fails = 0;
    avl *root = NULL;
    for (int i = 0; i < 2000; i++) root = ins(root, (int)(rnd() % 5000));
    int prev = 0, count = 0;
    int ordered = walk(root, &prev, &count);
    fprintf(o, "avl: %d keys ordered=%d height=%d (bound %d)\n", count, ordered, H(root), 2 * 12);
    if (!ordered || H(root) > 24) fails++;
    int removed = count;
    for (int k = 0; k < 5000; k += 3) root = del(root, k);
    prev = 0; count = 0;
    ordered = walk(root, &prev, &count);
    fprintf(o, "avl after deleting multiples of 3: removed=%d left=%d ordered=%d height=%d\n", removed - count, count, ordered, H(root));
    if (!ordered) fails++;
    avl_free(root);

    heap hp;
    hp.n = 0;
    uint32_t vals[400];
    for (int i = 0; i < 400; i++) { vals[i] = rnd() % 100000; hpush(&hp, (item){vals[i], i}); }
    uint32_t last = 0;
    int sorted = 1;
    uint64_t fold = 0;
    for (int i = 0; i < 400; i++) { item it = hpop(&hp); if (it.pri < last) sorted = 0; last = it.pri; fold = fold * 1000003u + it.pri; }
    fprintf(o, "heap pop order sorted=%d last=%u fold=%016llx\n", sorted, last, (unsigned long long)fold);
    if (!sorted) fails++;

    /* Dijkstra on a ring with chords: node i connects to i+1 (w 1..9) and i*7 mod N (w 10..49) */
    enum { N = 300 };
    uint32_t dist[N];
    for (int i = 0; i < N; i++) dist[i] = 0xFFFFFFFFu;
    static uint32_t w1[N], w2[N];
    for (int i = 0; i < N; i++) { w1[i] = 1 + rnd() % 9; w2[i] = 10 + rnd() % 40; }
    dist[0] = 0;
    hp.n = 0;
    hpush(&hp, (item){0, 0});
    while (hp.n) {
        item it = hpop(&hp);
        if (it.pri > dist[it.id]) continue;
        int nb[2] = {(it.id + 1) % N, (it.id * 7) % N};
        uint32_t w[2] = {w1[it.id], w2[it.id]};
        for (int k = 0; k < 2; k++) {
            if (hp.n > 500) break;
            if (dist[it.id] + w[k] < dist[nb[k]]) { dist[nb[k]] = dist[it.id] + w[k]; hpush(&hp, (item){dist[nb[k]], nb[k]}); }
        }
    }
    uint32_t far = 0; int farid = 0; uint64_t dsum = 0;
    for (int i = 0; i < N; i++) { dsum += dist[i]; if (dist[i] != 0xFFFFFFFFu && dist[i] > far) { far = dist[i]; farid = i; } }
    fprintf(o, "dijkstra: farthest node %d at %u, distance sum=%llu\n", farid, far, (unsigned long long)dsum);
    if (dist[1] > w1[0]) fails++;

    for (int i = 0; i < 1000; i++) uf[i] = i;
    int comps = 1000;
    for (int i = 0; i < 700; i++) { int a = find((int)(rnd() % 1000)), b = find((int)(rnd() % 1000)); if (a != b) { uf[a] = b; comps--; } }
    fprintf(o, "union-find: %d components after 700 random unions\n", comps);

    static const char *words[] = {"car", "card", "care", "careful", "cart", "cat", "dog", "dot", "dote", "do", "zebra", "zeta"};
    trie *tr = calloc(1, sizeof *tr);
    for (unsigned i = 0; i < sizeof words / sizeof *words; i++) {
        trie *t = tr;
        for (const char *p = words[i]; *p; p++) {
            if (!t->c[*p - 'a']) t->c[*p - 'a'] = calloc(1, sizeof *t);
            t = t->c[*p - 'a'];
            t->pass++;
        }
        t->end++;
    }
    static const char *prefixes[] = {"car", "ca", "do", "dote", "z", "x", "careful"};
    for (unsigned i = 0; i < sizeof prefixes / sizeof *prefixes; i++) {
        trie *t = tr;
        for (const char *p = prefixes[i]; *p && t; p++) t = t->c[*p - 'a'];
        fprintf(o, "trie prefix \"%s\": %d words\n", prefixes[i], t ? t->pass : 0);
    }
    trie *stack[256]; int sp = 0, freed = 0;
    stack[sp++] = tr;
    while (sp) { trie *t = stack[--sp]; for (int c = 0; c < 26; c++) if (t->c[c]) stack[sp++] = t->c[c]; free(t); freed++; }
    fprintf(o, "trie nodes freed=%d\n", freed);

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
