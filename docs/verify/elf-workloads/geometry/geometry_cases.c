/* elf-workload geometry: exact integer 2D cross products, Andrew's monotone-chain convex hull,
 * shoelace polygon area (doubled integer), exact segment intersection, and integer point-in-polygon.
 * Provenance: original work written for ounce-core (MIT); textbook computational geometry. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct { int64_t x, y; } pt;

static int64_t cross(pt a, pt b, pt c) {
    return (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
}

static int sgn64(int64_t v) { return (v > 0) - (v < 0); }

static int cmp_pt(const void *pa, const void *pb) {
    const pt *a = (const pt *)pa, *b = (const pt *)pb;
    if (a->x != b->x) return (a->x < b->x) ? -1 : 1;
    if (a->y != b->y) return (a->y < b->y) ? -1 : 1;
    return 0;
}

static int monotone_hull(pt *pts, int n, pt *hull) {
    qsort(pts, (size_t)n, sizeof pts[0], cmp_pt);
    int m = 0;
    for (int i = 0; i < n; i++) {
        if (i > 0 && pts[i].x == pts[i - 1].x && pts[i].y == pts[i - 1].y) continue;
        while (m >= 2 && cross(hull[m - 2], hull[m - 1], pts[i]) <= 0) m--;
        hull[m++] = pts[i];
    }
    int lower = m + 1;
    for (int i = n - 2; i >= 0; i--) {
        if (pts[i].x == pts[i + 1].x && pts[i].y == pts[i + 1].y) continue;
        while (m >= lower && cross(hull[m - 2], hull[m - 1], pts[i]) <= 0) m--;
        hull[m++] = pts[i];
    }
    return (m > 1) ? m - 1 : m;
}

static int64_t shoelace_2x(const pt *poly, int n) {
    int64_t sum = 0;
    for (int i = 0; i < n; i++) {
        int j = (i + 1) % n;
        sum += poly[i].x * poly[j].y - poly[j].x * poly[i].y;
    }
    return sum < 0 ? -sum : sum;
}

static int on_segment(pt a, pt b, pt p) {
    if (cross(a, b, p) != 0) return 0;
    int64_t minx = a.x < b.x ? a.x : b.x, maxx = a.x > b.x ? a.x : b.x;
    int64_t miny = a.y < b.y ? a.y : b.y, maxy = a.y > b.y ? a.y : b.y;
    return p.x >= minx && p.x <= maxx && p.y >= miny && p.y <= maxy;
}

static int seg_intersect(pt a, pt b, pt c, pt d) {
    int o1 = sgn64(cross(a, b, c)), o2 = sgn64(cross(a, b, d));
    int o3 = sgn64(cross(c, d, a)), o4 = sgn64(cross(c, d, b));
    if (o1 != o2 && o3 != o4) return 1;
    return on_segment(a, b, c) || on_segment(a, b, d) || on_segment(c, d, a) || on_segment(c, d, b);
}

/* Winding number in exact integer arithmetic: returns 1 inside, 0 outside, 2 on boundary. */
static int point_in_poly(const pt *poly, int n, pt p) {
    int wn = 0;
    for (int i = 0; i < n; i++) {
        pt a = poly[i], b = poly[(i + 1) % n];
        if (on_segment(a, b, p)) return 2;
        if (a.y <= p.y) {
            if (b.y > p.y && cross(a, b, p) > 0) wn++;
        } else {
            if (b.y <= p.y && cross(a, b, p) < 0) wn--;
        }
    }
    return wn != 0;
}

int workload_run(FILE *o) {
    int fails = 0;
    /* Cross product checks */
    int64_t c1 = cross((pt){0, 0}, (pt){10, 0}, (pt){10, 5});
    int64_t c2 = cross((pt){0, 0}, (pt){10, 0}, (pt){10, -5});
    int64_t c3 = cross((pt){0, 0}, (pt){4, 6}, (pt){8, 12});
    if (c1 != 50 || c2 != -50 || c3 != 0) fails++;

    /* Convex hull + shoelace on 128 deterministic points including collinear boundary points and duplicates */
    pt pts[128], orig[128], hull[130];
    uint32_t rng = 0x2468ace1u;
    for (int i = 0; i < 120; i++) {
        rng = rng * 1664525u + 1013904223u;
        int64_t x = (int64_t)((rng >> 16) % 201) - 100;
        rng = rng * 1664525u + 1013904223u;
        int64_t y = (int64_t)((rng >> 16) % 201) - 100;
        pts[i] = orig[i] = (pt){x, y};
    }
    /* Add known corners + collinear midpoint + duplicate */
    pts[120] = orig[120] = (pt){-100, -100};
    pts[121] = orig[121] = (pt){0, -100};
    pts[122] = orig[122] = (pt){100, -100};
    pts[123] = orig[123] = (pt){100, 100};
    pts[124] = orig[124] = (pt){-100, 100};
    pts[125] = orig[125] = (pt){-100, -100};
    pts[126] = orig[126] = (pt){100, 0};
    pts[127] = orig[127] = (pt){-50, 100};
    int hn = monotone_hull(pts, 128, hull);
    int64_t hull_2a = shoelace_2x(hull, hn);
    int all_inside = (hn == 4 && hull_2a == 80000);
    for (int i = 0; i < 128; i++) {
        for (int e = 0; e < hn; e++) {
            if (cross(hull[e], hull[(e + 1) % hn], orig[i]) < 0) all_inside = 0;
        }
    }
    if (!all_inside) fails++;

    /* L-shaped non-convex polygon: area = 100 - 25 = 75 -> 2x = 150 */
    const pt lpoly[] = {{0, 0}, {10, 0}, {10, 5}, {5, 5}, {5, 10}, {0, 10}};
    int64_t l2a = shoelace_2x(lpoly, 6);
    int pip_in1 = point_in_poly(lpoly, 6, (pt){2, 2});
    int pip_in2 = point_in_poly(lpoly, 6, (pt){2, 8});
    int pip_out = point_in_poly(lpoly, 6, (pt){8, 8});
    int pip_edge = point_in_poly(lpoly, 6, (pt){5, 7});
    int pip_vert = point_in_poly(lpoly, 6, (pt){5, 5});
    if (l2a != 150 || pip_in1 != 1 || pip_in2 != 1 || pip_out != 0 || pip_edge != 2 || pip_vert != 2) fails++;

    /* Segment intersection cases */
    int s_cross = seg_intersect((pt){0, 0}, (pt){4, 4}, (pt){0, 4}, (pt){4, 0});
    int s_col_ov = seg_intersect((pt){0, 0}, (pt){6, 0}, (pt){3, 0}, (pt){9, 0});
    int s_col_no = seg_intersect((pt){0, 0}, (pt){2, 0}, (pt){3, 0}, (pt){9, 0});
    int s_par = seg_intersect((pt){0, 0}, (pt){4, 4}, (pt){0, 1}, (pt){4, 5});
    int s_t = seg_intersect((pt){0, 2}, (pt){4, 2}, (pt){2, 2}, (pt){2, 6});
    if (!s_cross || !s_col_ov || s_col_no || s_par || !s_t) fails++;

    fprintf(o, "hull: n=%d area2=%lld all_inside=%d\n", hn, (long long)hull_2a, all_inside);
    fprintf(o, "lpoly: area2=%lld pip=%d,%d,%d,%d,%d seg=%d%d%d%d%d\n",
            (long long)l2a, pip_in1, pip_in2, pip_out, pip_edge, pip_vert,
            s_cross, s_col_ov, s_col_no, s_par, s_t);
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
