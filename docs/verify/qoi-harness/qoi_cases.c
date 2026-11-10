/* QOI differential cases: encode synthetic images, print size + hash + head of the encoding,
   decode them back (same and other channel counts), and feed the decoder malformed input.
   All data is generated here from a fixed LCG; nothing depends on the host. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#define QOI_IMPLEMENTATION
#define QOI_NO_STDIO
#include "qoi.h"

static uint32_t st = 0x2545F491u;
static uint32_t rnd(void) { st = st * 1664525u + 1013904223u; return st >> 8; }

static uint64_t fnv(const unsigned char *p, size_t n) {
  uint64_t h = 0xcbf29ce484222325ull;
  for (size_t i = 0; i < n; i++) { h ^= p[i]; h *= 0x100000001b3ull; }
  return h;
}

static void head(FILE *o, const unsigned char *p, int n, int max) {
  for (int i = 0; i < n && i < max; i++) fprintf(o, "%02x", p[i]);
}

/* pattern kinds exercise every QOI op: runs, index hits, small diffs, luma, full RGB(A) */
static void fill(unsigned char *px, int w, int h, int ch, int kind) {
  for (int y = 0; y < h; y++)
    for (int x = 0; x < w; x++) {
      unsigned char *p = px + ((size_t)y * w + x) * ch;
      unsigned r, g, b, a = 255;
      switch (kind) {
      case 0: r = g = b = 0; break;                                   /* solid black: long runs */
      case 1: r = x * 255 / (w > 1 ? w - 1 : 1); g = y * 255 / (h > 1 ? h - 1 : 1); b = (x + y) & 255; break; /* gradient: diff/luma */
      case 2: r = rnd() & 255; g = rnd() & 255; b = rnd() & 255; a = rnd() & 255; break; /* noise: full ops */
      case 3: { static const unsigned pal[4][4] = {{255,0,0,255},{0,255,0,128},{0,0,255,0},{9,9,9,255}};
                const unsigned *c = pal[(rnd() >> 5) & 3]; r = c[0]; g = c[1]; b = c[2]; a = c[3]; break; } /* palette: index */
      case 4: r = (x / 7) * 13; g = (y / 5) * 11; b = 128; a = (x & 1) ? 255 : 254; break; /* blocks + alpha flips */
      default: { unsigned v = (x * 3 + y * 5) & 255; r = v; g = (v + 2) & 255; b = (v - 3) & 255; a = (x % 70 < 66) ? 255 : 0; break; } /* luma + runs past 62 */
      }
      p[0] = (unsigned char)r; p[1] = (unsigned char)g; p[2] = (unsigned char)b;
      if (ch == 4) p[3] = (unsigned char)a;
    }
}

static int one(FILE *o, int w, int h, int ch, int cs, int kind) {
  size_t n = (size_t)w * h * ch;
  unsigned char *px = malloc(n ? n : 1);
  fill(px, w, h, ch, kind);
  qoi_desc d = { (unsigned)w, (unsigned)h, (unsigned char)ch, (unsigned char)cs };
  int len = -1, fails = 0;
  unsigned char *enc = qoi_encode(px, &d, &len);
  fprintf(o, "img %dx%d ch=%d cs=%d kind=%d raw=%llx", w, h, ch, cs, kind, (unsigned long long)fnv(px, n));
  if (!enc) { fprintf(o, " encode=NULL len=%d\n", len); free(px); return 0; }
  fprintf(o, " len=%d hash=%016llx head=", len, (unsigned long long)fnv(enc, (size_t)len));
  head(o, enc, len, 24);
  fprintf(o, " tail=");
  head(o, enc + (len > 8 ? len - 8 : 0), len, 8);
  for (int want = 0; want <= 4; want += (want == 0 ? 3 : 1)) {
    qoi_desc dd; memset(&dd, 0, sizeof dd);
    unsigned char *dec = qoi_decode(enc, len, &dd, want);
    int oc = want ? want : ch;
    if (!dec) { fprintf(o, " dec%d=NULL", want); fails++; continue; }
    size_t m = (size_t)dd.width * dd.height * oc;
    int ok = dd.width == (unsigned)w && dd.height == (unsigned)h && dd.channels == ch && dd.colorspace == cs;
    if (oc == ch) ok &= memcmp(dec, px, n) == 0;
    else for (size_t i = 0; i < (size_t)w * h; i++) {           /* channel conversion check */
      ok &= memcmp(dec + i * oc, px + i * ch, 3) == 0;
      if (oc == 4) ok &= dec[i * 4 + 3] == 255;
    }
    fprintf(o, " dec%d=%s:%llx", want, ok ? "ok" : "BAD", (unsigned long long)fnv(dec, m));
    if (!ok) fails++;
    free(dec);
  }
  fprintf(o, "\n");
  free(enc); free(px);
  return fails;
}

static void bad(FILE *o, const char *what, const unsigned char *p, int n, int ch) {
  qoi_desc d; memset(&d, 0, sizeof d);
  unsigned char *r = qoi_decode(p, n, &d, ch);
  fprintf(o, "malformed %s n=%d ch=%d -> %s", what, n, ch, r ? "decoded" : "NULL");
  if (r) fprintf(o, " %ux%u ch=%u hash=%016llx", d.width, d.height, d.channels,
                 (unsigned long long)fnv(r, (size_t)d.width * d.height * (ch ? ch : d.channels)));
  fprintf(o, "\n");
  free(r);
}

int qoi_run_cases(FILE *o) {
  int fails = 0;
  static const int dims[][2] = {{1,1},{2,1},{1,3},{7,5},{16,16},{63,1},{64,2},{100,37},{257,3},{320,240}};
  for (unsigned k = 0; k < sizeof dims / sizeof *dims; k++)
    for (int kind = 0; kind < 6; kind++)
      for (int ch = 3; ch <= 4; ch++)
        fails += one(o, dims[k][0], dims[k][1], ch, (kind + ch) & 1, kind);

  /* encoder rejects invalid descriptions */
  { unsigned char px[16] = {0}; int len = 7;
    qoi_desc d0 = {0, 4, 4, 0}, d1 = {4, 0, 3, 0}, d2 = {1, 1, 2, 0}, d3 = {1, 1, 4, 2}, d4 = {100000, 100000, 4, 0};
    qoi_desc *ds[] = {&d0, &d1, &d2, &d3, &d4};
    for (int i = 0; i < 5; i++) { len = 7; void *e = qoi_encode(px, ds[i], &len);
      fprintf(o, "encode-invalid %d -> %s len=%d\n", i, e ? "ENCODED" : "NULL", len); if (e) { fails++; free(e); } }
    void *e = qoi_encode(NULL, &d3, &len); fprintf(o, "encode-null -> %s\n", e ? "ENCODED" : "NULL"); free(e);
  }

  /* malformed decoder inputs derived from a valid 8x8 RGBA noise image */
  { unsigned char px[8 * 8 * 4]; fill(px, 8, 8, 4, 2);
    qoi_desc d = {8, 8, 4, 0}; int len = 0; unsigned char *enc = qoi_encode(px, &d, &len);
    unsigned char *b = malloc((size_t)len + 64);
    bad(o, "empty", enc, 0, 0);
    bad(o, "header-only", enc, 14, 0);
    bad(o, "short", enc, 21, 0);
    for (int cut = len - 8; cut > len - 40 && cut > 22; cut -= 7) bad(o, "truncated", enc, cut, 4);
    memcpy(b, enc, len); b[0] = 'x'; bad(o, "bad-magic", b, len, 0);
    memcpy(b, enc, len); b[12] = 5; bad(o, "bad-channels", b, len, 0);
    memcpy(b, enc, len); b[13] = 9; bad(o, "bad-colorspace", b, len, 0);
    memcpy(b, enc, len); b[4] = b[5] = b[6] = b[7] = 0; bad(o, "zero-width", b, len, 0);
    memcpy(b, enc, len); b[4] = 0x7f; b[8] = 0x7f; bad(o, "huge-dims", b, len, 0);
    memcpy(b, enc, len); bad(o, "bad-req-channels", b, len, 2);
    memcpy(b, enc, len); for (int i = 14; i < len - 8; i++) b[i] ^= (unsigned char)(rnd() & 0xff);
    bad(o, "scrambled", b, len, 0);
    memcpy(b, enc, len); memset(b + len - 8, 0xff, 8); bad(o, "bad-padding", b, len, 0);
    /* height grows: stream ends before all pixels, decoder must fill or stop without overrun */
    memcpy(b, enc, len); b[11] = 64; bad(o, "taller-than-data", b, len, 4);
    free(b); free(enc);
  }
  fprintf(o, "fails=%d\n", fails);
  return fails;
}
