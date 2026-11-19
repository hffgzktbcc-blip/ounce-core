/* elf-workload sorts: insertion, merge, heap and quick sort written for this repo, plus libc
 * qsort and bsearch, over a fixed xorshift sequence. Every result is checked against the others.
 * Provenance: original work written for ounce-core (MIT); algorithms are textbook. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define N 1500

static uint32_t state = 0x2545F491u;
static uint32_t rnd(void) {
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    return state;
}

static void insertion(int32_t *a, int n) {
    for (int i = 1; i < n; i++) {
        int32_t v = a[i];
        int j = i - 1;
        while (j >= 0 && a[j] > v) { a[j + 1] = a[j]; j--; }
        a[j + 1] = v;
    }
}

static void merge_sort(int32_t *a, int32_t *tmp, int n) {
    if (n < 2) return;
    int h = n / 2;
    merge_sort(a, tmp, h);
    merge_sort(a + h, tmp, n - h);
    int i = 0, j = h, k = 0;
    while (i < h && j < n) tmp[k++] = a[i] <= a[j] ? a[i++] : a[j++];
    while (i < h) tmp[k++] = a[i++];
    while (j < n) tmp[k++] = a[j++];
    memcpy(a, tmp, (size_t)n * sizeof *a);
}

static void sift(int32_t *a, int root, int n) {
    for (;;) {
        int c = 2 * root + 1;
        if (c >= n) return;
        if (c + 1 < n && a[c + 1] > a[c]) c++;
        if (a[root] >= a[c]) return;
        int32_t t = a[root]; a[root] = a[c]; a[c] = t;
        root = c;
    }
}
static void heap_sort(int32_t *a, int n) {
    for (int i = n / 2 - 1; i >= 0; i--) sift(a, i, n);
    for (int e = n - 1; e > 0; e--) {
        int32_t t = a[0]; a[0] = a[e]; a[e] = t;
        sift(a, 0, e);
    }
}

static void quick(int32_t *a, int lo, int hi) {
    while (lo < hi) {
        int32_t p = a[lo + (hi - lo) / 2];
        int i = lo, j = hi;
        while (i <= j) {
            while (a[i] < p) i++;
            while (a[j] > p) j--;
            if (i <= j) { int32_t t = a[i]; a[i] = a[j]; a[j] = t; i++; j--; }
        }
        if (j - lo < hi - i) { quick(a, lo, j); lo = i; }
        else { quick(a, i, hi); hi = j; }
    }
}

static int cmp_i32(const void *x, const void *y) {
    int32_t a = *(const int32_t *)x, b = *(const int32_t *)y;
    return (a > b) - (a < b);
}

static uint64_t fold(const int32_t *a, int n) {
    uint64_t h = 1469598103934665603ull;
    for (int i = 0; i < n; i++) h = (h ^ (uint32_t)a[i]) * 1099511628211ull;
    return h;
}

typedef struct { int32_t key; int32_t order; } rec;
static int cmp_rec_key(const void *x, const void *y) {
    const rec *a = x, *b = y;
    if (a->key != b->key) return a->key < b->key ? -1 : 1;
    return a->order < b->order ? -1 : a->order > b->order; /* explicit tie-break: qsort is not stable */
}

int workload_run(FILE *o) {
    static int32_t src[N], a[N], tmp[N];
    int fails = 0;
    for (int i = 0; i < N; i++) src[i] = (int32_t)rnd() % 5000; /* signed, many duplicates */

    uint64_t ref;
    memcpy(a, src, sizeof src);
    qsort(a, N, sizeof *a, cmp_i32);
    ref = fold(a, N);
    fprintf(o, "qsort    %016llx min=%d max=%d median=%d\n", (unsigned long long)ref, a[0], a[N - 1], a[N / 2]);

    const char *names[] = {"insertion", "merge", "heap", "quick"};
    for (int k = 0; k < 4; k++) {
        memcpy(a, src, sizeof src);
        if (k == 0) insertion(a, N);
        else if (k == 1) merge_sort(a, tmp, N);
        else if (k == 2) heap_sort(a, N);
        else quick(a, 0, N - 1);
        uint64_t h = fold(a, N);
        int sorted = 1;
        for (int i = 1; i < N; i++) sorted &= a[i - 1] <= a[i];
        fprintf(o, "%-9s %016llx sorted=%d match=%d\n", names[k], (unsigned long long)h, sorted, h == ref);
        if (!sorted || h != ref) fails++;
    }

    /* bsearch for present and absent keys in the sorted array */
    int found = 0, absent_ok = 0;
    for (int v = -5000; v < 5000; v += 7) {
        int32_t *p = bsearch(&v, a, N, sizeof *a, cmp_i32);
        int truth = 0;
        for (int i = 0; i < N && !truth; i++) truth = a[i] == v;
        if ((p != NULL) != truth) fails++;
        if (p) found++; else absent_ok++;
    }
    fprintf(o, "bsearch found=%d absent=%d\n", found, absent_ok);

    /* sort records by key with an explicit tie-break, check order is the stable order */
    static rec r[400];
    for (int i = 0; i < 400; i++) { r[i].key = (int32_t)(rnd() % 20); r[i].order = i; }
    qsort(r, 400, sizeof *r, cmp_rec_key);
    int stable = 1;
    uint64_t rh = 0;
    for (int i = 0; i < 400; i++) {
        if (i && r[i - 1].key == r[i].key && r[i - 1].order > r[i].order) stable = 0;
        rh = rh * 131 + (uint64_t)r[i].order;
    }
    fprintf(o, "records stable=%d fold=%016llx\n", stable, (unsigned long long)rh);
    if (!stable) fails++;

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
