/* NOTE: build with -fwrapv -ffp-contract=off so signed shifts and FMA contraction do not diverge.
 * stb_truetype (MIT/Unlicense) differential cases: builds a synthetic TrueType SFNT font in memory
 * (simple multi-contour glyphs, quadratic Bezier outlines, consecutive off-curve points, and a
 * compound scaled glyph), then exercises outline extraction, subpixel bitmap rasterization,
 * atlas packing, and signed-distance field (SDF) generation. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#define STB_TRUETYPE_IMPLEMENTATION
#include "stb_truetype.h"

static uint64_t fnv1a(const unsigned char *p, size_t n) {
    uint64_t h = 0xcbf29ce484222325ull;
    for (size_t i = 0; i < n; i++) {
        h ^= p[i];
        h *= 0x100000001b3ull;
    }
    return h;
}

typedef struct { uint8_t *p; size_t n, cap; } buf;
static void put(buf *b, const void *d, size_t n) {
    if (b->n + n > b->cap) {
        b->cap = (b->n + n) * 2 + 128;
        b->p = realloc(b->p, b->cap);
    }
    memcpy(b->p + b->n, d, n);
    b->n += n;
}
static void put8(buf *b, unsigned v) { uint8_t c = (uint8_t)v; put(b, &c, 1); }
static void put16(buf *b, int v) { put8(b, (unsigned)v >> 8); put8(b, (unsigned)v); }
static void put32(buf *b, uint32_t v) { put16(b, (int)(v >> 16)); put16(b, (int)(v & 0xffffu)); }
static void pad4(buf *b) { while (b->n & 3) put8(b, 0); }

typedef struct { int16_t x, y; uint8_t on; } pt;

static void add_simple_glyph(buf *glyf, const int *ends, int ncont, const pt *pts) {
    int npts = ends[ncont - 1] + 1;
    int16_t xmin = pts[0].x, ymin = pts[0].y, xmax = pts[0].x, ymax = pts[0].y;
    for (int i = 1; i < npts; i++) {
        if (pts[i].x < xmin) xmin = pts[i].x;
        if (pts[i].x > xmax) xmax = pts[i].x;
        if (pts[i].y < ymin) ymin = pts[i].y;
        if (pts[i].y > ymax) ymax = pts[i].y;
    }
    put16(glyf, ncont);
    put16(glyf, xmin);
    put16(glyf, ymin);
    put16(glyf, xmax);
    put16(glyf, ymax);
    for (int i = 0; i < ncont; i++) put16(glyf, ends[i]);
    put16(glyf, 0); /* instructionLength = 0 */
    for (int i = 0; i < npts; i++) put8(glyf, pts[i].on ? 1 : 0);
    int16_t px = 0, py = 0;
    for (int i = 0; i < npts; i++) { put16(glyf, pts[i].x - px); px = pts[i].x; }
    for (int i = 0; i < npts; i++) { put16(glyf, pts[i].y - py); py = pts[i].y; }
    pad4(glyf);
}

static buf build_test_ttf(void) {
    buf head = {0}, hhea = {0}, maxp = {0}, hmtx = {0}, cmap = {0}, loca = {0}, glyf = {0};
    const int nglyphs = 5;
    uint32_t goff[6];

    /* Glyph 0: .notdef box */
    goff[0] = (uint32_t)glyf.n;
    {
        const int ends[] = {3};
        const pt pts[] = {{50, 0, 1}, {50, 700, 1}, {450, 700, 1}, {450, 0, 1}};
        add_simple_glyph(&glyf, ends, 1, pts);
    }

    /* Glyph 1 ('A', U+0041): outer triangle + inner hole */
    goff[1] = (uint32_t)glyf.n;
    {
        const int ends[] = {2, 5};
        const pt pts[] = {
            {20, 0, 1}, {260, 720, 1}, {500, 0, 1},
            {160, 220, 1}, {360, 220, 1}, {260, 520, 1}
        };
        add_simple_glyph(&glyf, ends, 2, pts);
    }

    /* Glyph 2 ('O', U+004F): quadratic Bezier outer + inner contours */
    goff[2] = (uint32_t)glyf.n;
    {
        const int ends[] = {7, 15};
        const pt pts[] = {
            {260, 0, 1}, {60, 0, 0}, {60, 360, 1}, {60, 720, 0},
            {260, 720, 1}, {460, 720, 0}, {460, 360, 1}, {460, 0, 0},
            {260, 120, 1}, {360, 120, 0}, {360, 360, 1}, {360, 600, 0},
            {260, 600, 1}, {160, 600, 0}, {160, 360, 1}, {160, 120, 0}
        };
        add_simple_glyph(&glyf, ends, 2, pts);
    }

    /* Glyph 3 ('S', U+0053): curvy Bezier wave with consecutive off-curve points */
    goff[3] = (uint32_t)glyf.n;
    {
        const int ends[] = {9};
        const pt pts[] = {
            {80, 80, 1}, {40, 240, 0}, {440, 360, 0}, {440, 560, 1}, {440, 700, 0},
            {240, 720, 1}, {80, 700, 0}, {120, 520, 1}, {360, 300, 0}, {280, 40, 0}
        };
        add_simple_glyph(&glyf, ends, 1, pts);
    }

    /* Glyph 4 ('@', U+0040): compound glyph combining glyph 1 and scaled/shifted glyph 2 */
    goff[4] = (uint32_t)glyf.n;
    {
        put16(&glyf, -1); put16(&glyf, 0); put16(&glyf, 0); put16(&glyf, 700); put16(&glyf, 750);
        put16(&glyf, 0x0002 | 0x0020); /* ARGS_ARE_XY_VALUES | MORE_COMPONENTS */
        put16(&glyf, 1); put8(&glyf, 0); put8(&glyf, 0);
        put16(&glyf, 0x0001 | 0x0002 | 0x0008); /* ARG_1_AND_2_ARE_WORDS | ARGS_ARE_XY_VALUES | WE_HAVE_A_SCALE */
        put16(&glyf, 2); put16(&glyf, 180); put16(&glyf, 40); put16(&glyf, 0x3000); /* 0.75 in F2Dot14 */
        pad4(&glyf);
    }
    goff[5] = (uint32_t)glyf.n;

    for (int i = 0; i <= nglyphs; i++) put32(&loca, goff[i]);

    /* head (54 bytes): indexToLocFormat = 1 (long offsets) */
    put32(&head, 0x00010000); put32(&head, 0x00010000); put32(&head, 0); put32(&head, 0x5F0F3CF5);
    put16(&head, 0); put16(&head, 1000);
    put32(&head, 0); put32(&head, 0); put32(&head, 0); put32(&head, 0);
    put16(&head, 0); put16(&head, 0); put16(&head, 800); put16(&head, 800);
    put16(&head, 0); put16(&head, 8); put16(&head, 2); put16(&head, 1); put16(&head, 0);

    /* hhea (36 bytes): ascent=800, descent=-200, lineGap=50, numberOfHMetrics=nglyphs */
    put32(&hhea, 0x00010000); put16(&hhea, 800); put16(&hhea, -200); put16(&hhea, 50);
    for (int i = 0; i < 12; i++) put16(&hhea, 0);
    put16(&hhea, nglyphs);

    /* maxp (6 bytes) */
    put32(&maxp, 0x00005000); put16(&maxp, nglyphs);

    /* hmtx */
    for (int i = 0; i < nglyphs; i++) { put16(&hmtx, 540 + i * 20); put16(&hmtx, 20 + i * 5); }

    /* cmap format 4: U+0040->4, U+0041->1, U+004F->2, U+0053->3 */
    put16(&cmap, 0); put16(&cmap, 1);
    put16(&cmap, 3); put16(&cmap, 1); put32(&cmap, 12);
    put16(&cmap, 4); put16(&cmap, 16 + 5 * 8); put16(&cmap, 0);
    put16(&cmap, 10); put16(&cmap, 8); put16(&cmap, 2); put16(&cmap, 2);
    const uint16_t endc[5]   = {0x0040, 0x0041, 0x004F, 0x0053, 0xFFFF};
    const uint16_t startc[5] = {0x0040, 0x0041, 0x004F, 0x0053, 0xFFFF};
    const int16_t  delta[5]  = {4 - 0x40, 1 - 0x41, 2 - 0x4F, 3 - 0x53, 1};
    for (int i = 0; i < 5; i++) put16(&cmap, endc[i]);
    put16(&cmap, 0);
    for (int i = 0; i < 5; i++) put16(&cmap, startc[i]);
    for (int i = 0; i < 5; i++) put16(&cmap, delta[i]);
    for (int i = 0; i < 5; i++) put16(&cmap, 0);

    struct { const char *tag; buf *b; } tabs[7] = {
        {"cmap", &cmap}, {"glyf", &glyf}, {"head", &head}, {"hhea", &hhea},
        {"hmtx", &hmtx}, {"loca", &loca}, {"maxp", &maxp}
    };
    buf out = {0};
    put32(&out, 0x00010000); put16(&out, 7); put16(&out, 64); put16(&out, 2); put16(&out, 48);
    uint32_t off = 12 + 7 * 16;
    for (int i = 0; i < 7; i++) {
        put(&out, tabs[i].tag, 4);
        put32(&out, 0);
        put32(&out, off);
        put32(&out, (uint32_t)tabs[i].b->n);
        off += ((uint32_t)tabs[i].b->n + 3u) & ~3u;
    }
    for (int i = 0; i < 7; i++) {
        put(&out, tabs[i].b->p, tabs[i].b->n);
        pad4(&out);
        free(tabs[i].b->p);
    }
    return out;
}

int stbtt_run_cases(FILE *o) {
    int fails = 0;

    /* Invalid font rejection */
    {
        unsigned char zeros[128] = {0};
        stbtt_fontinfo bad;
        int nfonts = stbtt_GetNumberOfFonts(zeros);
        int ok = stbtt_InitFont(&bad, zeros, 0);
        fprintf(o, "invalid nfonts=%d init=%d\n", nfonts, ok);
        if (ok != 0) fails++;
    }

    buf ttf = build_test_ttf();
    stbtt_fontinfo f;
    int ok = stbtt_InitFont(&f, ttf.p, 0);
    if (!ok) {
        fprintf(o, "InitFont failed\n");
        free(ttf.p);
        return 1;
    }
    int asc = 0, desc = 0, gap = 0;
    stbtt_GetFontVMetrics(&f, &asc, &desc, &gap);
    fprintf(o, "ttf bytes=%zu init=%d vmetrics=%d,%d,%d\n", ttf.n, ok, asc, desc, gap);

    static const int cps[] = {0x41, 0x4F, 0x53, 0x40, 0x5A};
    static const float heights[] = {12.0f, 24.0f, 36.0f};
    static const float shifts[] = {0.0f, 0.25f, 0.5f, 0.75f};

    for (unsigned ci = 0; ci < sizeof cps / sizeof *cps; ci++) {
        int cp = cps[ci];
        int g = stbtt_FindGlyphIndex(&f, cp);
        int adv = 0, lsb = 0, x0 = 0, y0 = 0, x1 = 0, y1 = 0;
        stbtt_GetGlyphHMetrics(&f, g, &adv, &lsb);
        int has_box = stbtt_GetGlyphBox(&f, g, &x0, &y0, &x1, &y1);
        int empty = stbtt_IsGlyphEmpty(&f, g);
        stbtt_vertex *verts = NULL;
        int nverts = stbtt_GetGlyphShape(&f, g, &verts);
        int nmove = 0, nline = 0, ncurve = 0;
        for (int vi = 0; vi < nverts; vi++) {
            if (verts[vi].type == STBTT_vmove) nmove++;
            else if (verts[vi].type == STBTT_vline) nline++;
            else if (verts[vi].type == STBTT_vcurve) ncurve++;
        }
        stbtt_FreeShape(&f, verts);
        fprintf(o, "cp=%02x g=%d adv=%d lsb=%d box=%d(%d,%d,%d,%d) empty=%d shape=%d(m=%d,l=%d,c=%d)\n",
                cp, g, adv, lsb, has_box, x0, y0, x1, y1, empty, nverts, nmove, nline, ncurve);
        if (g == 0) continue;

        for (unsigned hi = 0; hi < sizeof heights / sizeof *heights; hi++) {
            float sc = stbtt_ScaleForPixelHeight(&f, heights[hi]);
            float shift = shifts[(ci + hi) % 4];
            int bx0 = 0, by0 = 0, bx1 = 0, by1 = 0;
            stbtt_GetGlyphBitmapBoxSubpixel(&f, g, sc, sc, shift, shift * 0.5f, &bx0, &by0, &bx1, &by1);
            int w = 0, h = 0, xoff = 0, yoff = 0;
            unsigned char *bm = stbtt_GetGlyphBitmapSubpixel(&f, sc, sc, shift, shift * 0.5f, g, &w, &h, &xoff, &yoff);
            unsigned sum = 0;
            uint64_t hash = 0;
            if (bm && w > 0 && h > 0) {
                for (int k = 0; k < w * h; k++) sum += bm[k];
                hash = fnv1a(bm, (size_t)w * (size_t)h);
            } else {
                fails++;
            }
            fprintf(o, "  h=%.0f shift=%.2f box=(%d,%d,%d,%d) bm=%dx%d(%d,%d) sum=%u fnv=%016llx\n",
                    heights[hi], shift, bx0, by0, bx1, by1, w, h, xoff, yoff, sum, (unsigned long long)hash);
            stbtt_FreeBitmap(bm, NULL);
        }

        /* Signed Distance Field rasterization */
        int sw = 0, sh = 0, sx = 0, sy = 0;
        float sdf_sc = stbtt_ScaleForMappingEmToPixels(&f, 24.0f);
        unsigned char *sdf = stbtt_GetGlyphSDF(&f, sdf_sc, g, 4, 128, 32.0f, &sw, &sh, &sx, &sy);
        unsigned ssum = 0;
        uint64_t shash = 0;
        if (sdf && sw > 0 && sh > 0) {
            for (int k = 0; k < sw * sh; k++) ssum += sdf[k];
            shash = fnv1a(sdf, (size_t)sw * (size_t)sh);
        } else {
            fails++;
        }
        fprintf(o, "  sdf=%dx%d(%d,%d) sum=%u fnv=%016llx\n",
                sw, sh, sx, sy, ssum, (unsigned long long)shash);
        stbtt_FreeSDF(sdf, NULL);
    }

    /* Render a multi-glyph atlas via stbtt_MakeCodepointBitmap */
    {
        unsigned char atlas[128 * 40];
        memset(atlas, 0, sizeof atlas);
        float sc = stbtt_ScaleForPixelHeight(&f, 28.0f);
        int x = 2;
        for (int i = 0; i < 4; i++) {
            int adv = 0, lsb = 0, x0 = 0, y0 = 0, x1 = 0, y1 = 0;
            stbtt_GetCodepointHMetrics(&f, cps[i], &adv, &lsb);
            stbtt_GetCodepointBitmapBox(&f, cps[i], sc, sc, &x0, &y0, &x1, &y1);
            int bw = x1 - x0, bh = y1 - y0;
            if (x + bw < 128 && bh <= 36) {
                stbtt_MakeCodepointBitmap(&f, atlas + 2 * 128 + x, bw, bh, 128, sc, sc, cps[i]);
            }
            x += (int)floorf((float)adv * sc) + 2;
        }
        fprintf(o, "atlas fnv=%016llx\n", (unsigned long long)fnv1a(atlas, sizeof atlas));
    }

    free(ttf.p);
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
