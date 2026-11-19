/* elf-workload heapmix: malloc/calloc/realloc/free patterns on musl's allocator: a growing
 * vector, a singly linked list, a chained hash table, aligned allocations, large (mmap-backed)
 * blocks, zero-fill and content preservation across realloc. Only values (never pointers) are printed.
 * Provenance: original work written for ounce-core (MIT); no third-party source. */
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static uint32_t st = 0xC0FFEEu;
static uint32_t rnd(void) { st = st * 1664525u + 1013904223u; return st >> 8; }

typedef struct node { uint32_t key, val; struct node *next; } node;

int workload_run(FILE *o) {
    int fails = 0;

    /* growing vector with realloc: contents must survive every move */
    uint32_t *v = NULL;
    size_t cap = 0, n = 0;
    for (uint32_t i = 0; i < 50000; i++) {
        if (n == cap) {
            cap = cap ? cap * 2 : 4;
            uint32_t *nv = realloc(v, cap * sizeof *v);
            if (!nv) return 1;
            v = nv;
        }
        v[n++] = i * 2654435761u;
    }
    uint64_t h = 0;
    for (size_t i = 0; i < n; i++) { h = h * 1099511628211ull + v[i]; if (v[i] != (uint32_t)i * 2654435761u) fails++; }
    fprintf(o, "vector n=%zu cap=%zu fold=%016llx\n", n, cap, (unsigned long long)h);
    v = realloc(v, 100 * sizeof *v);     /* shrink */
    for (size_t i = 0; i < 100; i++) if (v[i] != (uint32_t)i * 2654435761u) fails++;
    free(v);

    /* linked list: build, reverse, sum, free */
    node *head = NULL;
    for (uint32_t i = 0; i < 3000; i++) {
        node *x = malloc(sizeof *x);
        x->key = i; x->val = rnd(); x->next = head; head = x;
    }
    node *prev = NULL;
    while (head) { node *nx = head->next; head->next = prev; prev = head; head = nx; }
    uint64_t sum = 0;
    unsigned expect_key = 0;
    for (node *x = prev; x; x = x->next) { sum += x->val; if (x->key != expect_key++) fails++; }
    fprintf(o, "list reversed ok=%d sum=%llu\n", expect_key == 3000, (unsigned long long)sum);
    while (prev) { node *nx = prev->next; free(prev); prev = nx; }

    /* chained hash table with insert, lookup, delete */
    enum { B = 257 };
    node **tab = calloc(B, sizeof *tab);
    for (uint32_t i = 0; i < 4000; i++) {
        uint32_t k = rnd() % 6000;
        node **pp = &tab[k % B];
        while (*pp && (*pp)->key != k) pp = &(*pp)->next;
        if (*pp) (*pp)->val++;
        else { node *x = malloc(sizeof *x); x->key = k; x->val = 1; x->next = NULL; *pp = x; }
    }
    unsigned distinct = 0, longest = 0, maxcount = 0;
    for (int b = 0; b < B; b++) {
        unsigned len = 0;
        for (node *x = tab[b]; x; x = x->next) { len++; if (x->val > maxcount) maxcount = x->val; }
        distinct += len;
        if (len > longest) longest = len;
    }
    fprintf(o, "hash distinct=%u longest-chain=%u max-count=%u\n", distinct, longest, maxcount);
    unsigned deleted = 0;
    for (int b = 0; b < B; b++) {
        node **pp = &tab[b];
        while (*pp) {
            if ((*pp)->key % 2) { node *d = *pp; *pp = d->next; free(d); deleted++; } else pp = &(*pp)->next;
        }
    }
    fprintf(o, "hash deleted-odd=%u remaining=%u\n", deleted, distinct - deleted);
    for (int b = 0; b < B; b++) while (tab[b]) { node *d = tab[b]; tab[b] = d->next; free(d); }
    free(tab);

    /* calloc zero-fill, including a large block likely served by mmap */
    size_t big = 3u << 20;
    uint8_t *z = calloc(big, 1);
    unsigned nz = 0;
    for (size_t i = 0; i < big; i += 4093) nz += z[i] != 0;
    z[big - 1] = 0xAB;
    uint8_t *z2 = realloc(z, big * 2);
    if (!z2) {
        free(z);
        fprintf(o, "realloc failed\n");
        return fails + 1;
    }
    fprintf(o, "calloc nonzero=%u kept=%02x\n", nz, z2[big - 1]);
    if (nz || z2[big - 1] != 0xAB) fails++;
    free(z2);

    /* aligned allocations */
    int al_ok = 1;
    for (size_t a = 16; a <= 4096; a *= 2) {
        void *p = NULL;
        if (posix_memalign(&p, a, a * 3 + 5) != 0 || ((uintptr_t)p & (a - 1))) al_ok = 0;
        else memset(p, 0x5A, a * 3 + 5);
        free(p);
    }
    void *ac = aligned_alloc(64, 256);
    al_ok &= ac != NULL && !((uintptr_t)ac & 63);
    free(ac);
    fprintf(o, "aligned ok=%d\n", al_ok);
    if (!al_ok) fails++;

    /* random alloc/free churn with content checks: every block holds its own id */
    enum { K = 200 };
    uint8_t *blk[K] = {0};
    uint32_t sz[K] = {0};
    unsigned corrupt = 0, allocs = 0, frees = 0;
    for (int step = 0; step < 6000; step++) {
        int i = (int)(rnd() % K);
        if (blk[i]) {
            for (uint32_t j = 0; j < sz[i]; j++) if (blk[i][j] != (uint8_t)(i + j)) { corrupt++; break; }
            if (rnd() & 1) { free(blk[i]); blk[i] = NULL; frees++; }
            else {
                uint32_t ns = 1 + rnd() % 3000;
                uint8_t *nb = realloc(blk[i], ns);
                uint32_t keep = ns < sz[i] ? ns : sz[i];
                for (uint32_t j = 0; j < keep; j++) if (nb[j] != (uint8_t)(i + j)) { corrupt++; break; }
                for (uint32_t j = 0; j < ns; j++) nb[j] = (uint8_t)(i + j);
                blk[i] = nb; sz[i] = ns;
            }
        } else {
            sz[i] = 1 + rnd() % 3000;
            blk[i] = malloc(sz[i]);
            for (uint32_t j = 0; j < sz[i]; j++) blk[i][j] = (uint8_t)(i + j);
            allocs++;
        }
    }
    unsigned live = 0;
    for (int i = 0; i < K; i++) if (blk[i]) { live++; free(blk[i]); }
    fprintf(o, "churn allocs=%u frees=%u live-at-end=%u corrupt=%u\n", allocs, frees, live, corrupt);
    if (corrupt) fails++;

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
