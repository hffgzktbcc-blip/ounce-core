/* NOTE: build with -fwrapv. stb_image_write's JPEG encoder shifts a signed int left past INT_MAX (undefined
   behaviour, flagged by UBSan); without -fwrapv different compilers/optimisation levels emit different JPEG bytes
   for q=100, which is a compiler difference, not a CPU one.
   stb_image differential cases. Test images are generated here (no files): PNGs with every filter
   type, bit depths 1-16 and colour types 0/2/3/4/6 are built by a tiny writer with stored deflate
   blocks; BMPs (24/32-bit, top-down/bottom-up) are built by hand; JPEGs and compressed PNGs come
   from stb_image_write (same pinned stb commit). Each is decoded with every requested channel count
   and printed as size + FNV hash + first pixels. Malformed inputs print the failure reason. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#define STB_IMAGE_IMPLEMENTATION
#define STBI_NO_STDIO
#define STBI_NO_HDR
#define STBI_NO_LINEAR
#include "stb_image.h"
#define STB_IMAGE_WRITE_IMPLEMENTATION
#define STBI_WRITE_NO_STDIO
#include "stb_image_write.h"

static uint32_t st = 0x5eed1234u;
static uint32_t rnd(void) { st = st * 1664525u + 1013904223u; return st >> 8; }
static uint64_t fnv(const unsigned char *p, size_t n) {
  uint64_t h = 0xcbf29ce484222325ull;
  for (size_t i = 0; i < n; i++) { h ^= p[i]; h *= 0x100000001b3ull; }
  return h;
}

typedef struct { unsigned char *p; size_t n, cap; } buf;
static void put(buf *b, const void *d, size_t n) {
  if (b->n + n > b->cap) { b->cap = (b->n + n) * 2 + 64; b->p = realloc(b->p, b->cap); }
  memcpy(b->p + b->n, d, n); b->n += n;
}
static void put8(buf *b, unsigned v) { unsigned char c = (unsigned char)v; put(b, &c, 1); }
static void put32be(buf *b, uint32_t v) { put8(b, v >> 24); put8(b, v >> 16); put8(b, v >> 8); put8(b, v); }
static uint32_t crc32_(const unsigned char *p, size_t n) {
  uint32_t c = 0xffffffffu;
  for (size_t i = 0; i < n; i++) { c ^= p[i]; for (int k = 0; k < 8; k++) c = (c >> 1) ^ (0xedb88320u & (0u - (c & 1))); }
  return ~c;
}
static void chunk(buf *b, const char *type, const unsigned char *d, size_t n) {
  put32be(b, (uint32_t)n); size_t at = b->n; put(b, type, 4); if (n) put(b, d, n);
  put32be(b, crc32_(b->p + at, n + 4));
}
static uint32_t adler(const unsigned char *p, size_t n) {
  uint32_t a = 1, s = 0; for (size_t i = 0; i < n; i++) { a = (a + p[i]) % 65521; s = (s + a) % 65521; } return (s << 16) | a;
}
static int paeth(int a, int b, int c) { int p = a + b - c, pa = abs(p - a), pb = abs(p - b), pc = abs(p - c); return (pa <= pb && pa <= pc) ? a : (pb <= pc ? b : c); }

/* filter: 0-4 fixed, 5 = a different one per row. Palette (ct 3) must hold 2^bd entries (16 for bd 8). */
static buf make_png(int w, int h, int ct, int bd, int filter, const unsigned char *plte, int nplte, int trns) {
  int chans = ct == 0 ? 1 : ct == 2 ? 3 : ct == 3 ? 1 : ct == 4 ? 2 : 4;
  size_t bpp = (size_t)(chans * bd + 7) / 8, stride = ((size_t)w * chans * bd + 7) / 8;
  unsigned char *raw = malloc((size_t)h * (stride + 1)), *prev = calloc(stride, 1), *cur = malloc(stride);
  for (int y = 0; y < h; y++) {
    for (size_t i = 0; i < stride; i++) cur[i] = (unsigned char)((y * 7 + i * 13 + (rnd() & 31)) ^ (i > 3 ? cur[i - 3] : 0));
    if (ct == 3) for (size_t i = 0; i < stride; i++) cur[i] = bd == 8 ? (unsigned char)(rnd() % 16) : (unsigned char)rnd();
    int f = filter == 5 ? y % 5 : filter;
    unsigned char *o = raw + (size_t)y * (stride + 1); o[0] = (unsigned char)f;
    for (size_t i = 0; i < stride; i++) {
      int a = i >= bpp ? cur[i - bpp] : 0, b = prev[i], c = i >= bpp ? prev[i - bpp] : 0, pr = 0;
      switch (f) { case 1: pr = a; break; case 2: pr = b; break; case 3: pr = (a + b) / 2; break; case 4: pr = paeth(a, b, c); break; }
      o[1 + i] = (unsigned char)(cur[i] - pr);
    }
    memcpy(prev, cur, stride);
  }
  buf z = {0}; size_t rn = (size_t)h * (stride + 1);
  put8(&z, 0x78); put8(&z, 0x01);
  for (size_t p = 0; p < rn;) { size_t c = rn - p > 65535 ? 65535 : rn - p; put8(&z, p + c >= rn); put8(&z, c); put8(&z, c >> 8); put8(&z, ~c); put8(&z, (~c) >> 8); put(&z, raw + p, c); p += c; }
  put32be(&z, adler(raw, rn));
  buf out = {0}; static const unsigned char sig[8] = {137,80,78,71,13,10,26,10}; put(&out, sig, 8);
  unsigned char ihdr[13] = {0}; ihdr[0] = w >> 24; ihdr[1] = w >> 16; ihdr[2] = w >> 8; ihdr[3] = (unsigned char)w;
  ihdr[4] = h >> 24; ihdr[5] = h >> 16; ihdr[6] = h >> 8; ihdr[7] = (unsigned char)h; ihdr[8] = (unsigned char)bd; ihdr[9] = (unsigned char)ct;
  chunk(&out, "IHDR", ihdr, 13);
  if (plte) chunk(&out, "PLTE", plte, (size_t)nplte * 3);
  if (trns && ct == 3) { unsigned char t[4] = {0, 128, 255, 7}; chunk(&out, "tRNS", t, 4); }
  if (trns && ct == 2) { unsigned char t[6] = {0, 10, 0, 20, 0, 30}; chunk(&out, "tRNS", t, 6); }
  chunk(&out, "IDAT", z.p, z.n); chunk(&out, "IEND", NULL, 0);
  free(raw); free(prev); free(cur); free(z.p);
  return out;
}

static buf make_bmp(int w, int h, int bits, int topdown) {
  int bytes = bits / 8, stride = (w * bytes + 3) & ~3; size_t pix = (size_t)stride * h;
  unsigned char *d = calloc(54 + pix, 1);
  d[0] = 'B'; d[1] = 'M'; uint32_t fs = (uint32_t)(54 + pix); memcpy(d + 2, &fs, 4); d[10] = 54; d[14] = 40;
  int32_t ww = w, hh = topdown ? -h : h; memcpy(d + 18, &ww, 4); memcpy(d + 22, &hh, 4); d[26] = 1; d[28] = (unsigned char)bits;
  for (size_t i = 0; i < pix; i++) d[54 + i] = (unsigned char)rnd();
  buf b = { d, 54 + pix, 54 + pix }; return b;
}

/* nohash: print only dimensions and success. Used for corrupt JPEGs: stb_image leaves the undecoded part of the
   output buffer uninitialised there, so its bytes vary between runs even on the host. */
static int nohash;
static void report(FILE *o, const char *name, const unsigned char *data, int len) {
  int w, h, n;
  if (!stbi_info_from_memory(data, len, &w, &h, &n)) { fprintf(o, "%s info=FAIL (%s)\n", name, stbi_failure_reason()); return; }
  fprintf(o, "%s info=%dx%d/%d", name, w, h, n);
  for (int req = 0; req <= 4; req++) {
    int w2, h2, n2; unsigned char *px = stbi_load_from_memory(data, len, &w2, &h2, &n2, req);
    if (!px) { fprintf(o, " r%d=FAIL(%s)", req, stbi_failure_reason()); continue; }
    int oc = req ? req : n2;
    if (nohash) { fprintf(o, " r%d=%dx%d/%d:ok", req, w2, h2, n2); stbi_image_free(px); continue; }
    fprintf(o, " r%d=%dx%d/%d:%016llx:", req, w2, h2, n2, (unsigned long long)fnv(px, (size_t)w2 * h2 * oc));
    for (long i = 0; i < (long)w2 * h2 * oc && i < 8; i++) fprintf(o, "%02x", px[i]);
    stbi_image_free(px);
  }
  fprintf(o, "\n");
}

static void wr(void *ctx, void *d, int n) { put((buf *)ctx, d, (size_t)n); }

int stbi_run_cases(FILE *o) {
  int fails = 0;
  unsigned char plte[16 * 3]; for (int i = 0; i < 16 * 3; i++) plte[i] = (unsigned char)(i * 37 + 11);
  char name[96];

  static const struct { int ct, bd; } kinds[] = {{0,8},{0,16},{0,1},{0,4},{2,8},{2,16},{3,8},{3,4},{3,2},{4,8},{4,16},{6,8},{6,16}};
  for (unsigned k = 0; k < sizeof kinds / sizeof *kinds; k++)
    for (int f = 0; f <= 5; f++) {
      int w = 1 + (int)((k * 5 + f * 3) % 19), h = 1 + (int)((k + f * 2) % 11);
      int np = kinds[k].bd == 8 ? 16 : 1 << kinds[k].bd;
      buf p = make_png(w, h, kinds[k].ct, kinds[k].bd, f, kinds[k].ct == 3 ? plte : NULL, np, (int)((k + f) & 1));
      snprintf(name, sizeof name, "png ct=%d bd=%d filter=%d", kinds[k].ct, kinds[k].bd, f);
      report(o, name, p.p, (int)p.n);
      free(p.p);
    }
  { buf p = make_png(300, 40, 6, 8, 5, NULL, 0, 0); report(o, "png wide 300x40 cycling filters", p.p, (int)p.n); free(p.p); }
  { buf p = make_png(100, 200, 6, 8, 4, NULL, 0, 0); report(o, "png multi-block deflate 100x200", p.p, (int)p.n); free(p.p); }

  for (int bits = 24; bits <= 32; bits += 8) for (int td = 0; td < 2; td++) for (int w = 1; w <= 9; w += 4) {
    buf b = make_bmp(w, 5, bits, td); snprintf(name, sizeof name, "bmp %dbit w=%d topdown=%d", bits, w, td);
    report(o, name, b.p, (int)b.n); free(b.p);
  }

  static const int dims[][2] = {{8,8},{16,16},{17,9},{64,48},{100,75}};
  for (unsigned d = 0; d < sizeof dims / sizeof *dims; d++)
    for (int ch = 1; ch <= 4; ch++) {
      int w = dims[d][0], h = dims[d][1]; unsigned char *px = malloc((size_t)w * h * ch);
      for (int y = 0; y < h; y++) for (int x = 0; x < w; x++) for (int c = 0; c < ch; c++)
        px[((size_t)y * w + x) * ch + c] = (unsigned char)((x * (3 + c) + y * (5 - c) + ((x / 8 + y / 8) & 1) * 60 + (rnd() & 7)) & 255);
      buf pb = {0}; stbi_write_png_to_func(wr, &pb, w, h, ch, px, w * ch);
      snprintf(name, sizeof name, "zpng %dx%d ch=%d enc=%zu:%016llx", w, h, ch, pb.n, (unsigned long long)fnv(pb.p, pb.n)); report(o, name, pb.p, (int)pb.n); free(pb.p);
      for (int q = 10; q <= 100; q += 45) {
        buf jb = {0}; stbi_write_jpg_to_func(wr, &jb, w, h, ch, px, q);
        snprintf(name, sizeof name, "jpg %dx%d ch=%d q=%d enc=%zu:%016llx", w, h, ch, q, jb.n, (unsigned long long)fnv(jb.p, jb.n)); report(o, name, jb.p, (int)jb.n); free(jb.p);
      }
      free(px);
    }

  { buf p = make_png(9, 9, 6, 8, 5, NULL, 0, 0);
    unsigned char *c = malloc(p.n);
    report(o, "empty", c, 0);
    for (long cut = (long)p.n - 1; cut > 8; cut -= (long)p.n / 6 + 1) { snprintf(name, sizeof name, "png truncated at %ld/%zu", cut, p.n); report(o, name, p.p, (int)cut); }
    memcpy(c, p.p, p.n); c[1] = 'X'; report(o, "png bad signature", c, (int)p.n);
    memcpy(c, p.p, p.n); c[24] = 5; report(o, "png bad bit depth", c, (int)p.n);
    memcpy(c, p.p, p.n); c[25] = 7; report(o, "png bad colour type", c, (int)p.n);
    memcpy(c, p.p, p.n); c[16] = 0x7f; report(o, "png huge width", c, (int)p.n);
    memcpy(c, p.p, p.n); c[p.n - 20] ^= 0x55; report(o, "png corrupt idat", c, (int)p.n);
    for (int i = 0; i < 12; i++) { memcpy(c, p.p, p.n); size_t at = 33 + rnd() % (p.n - 33); c[at] ^= (unsigned char)(1 + rnd() % 255);
      snprintf(name, sizeof name, "png bitflip@%zu", at); report(o, name, c, (int)p.n); }
    free(c); free(p.p); }
  nohash = 1;
  { buf jb = {0}; unsigned char px[16 * 16 * 3]; for (int i = 0; i < 16 * 16 * 3; i++) px[i] = (unsigned char)(i * 7);
    stbi_write_jpg_to_func(wr, &jb, 16, 16, 3, px, 80);
    unsigned char *c = malloc(jb.n);
    for (long cut = (long)jb.n - 1; cut > 20; cut -= (long)jb.n / 5 + 1) { snprintf(name, sizeof name, "jpg truncated at %ld/%zu", cut, jb.n); report(o, name, jb.p, (int)cut); }
    for (int i = 0; i < 12; i++) { memcpy(c, jb.p, jb.n); size_t at = 20 + rnd() % (jb.n - 20); c[at] ^= (unsigned char)(1 + rnd() % 255);
      snprintf(name, sizeof name, "jpg bitflip@%zu", at); report(o, name, c, (int)jb.n); }
    free(c); free(jb.p); }
  nohash = 0;
  { buf b = make_bmp(4, 4, 24, 0); b.p[28] = 7; report(o, "bmp bad bpp", b.p, (int)b.n); b.p[28] = 24; b.p[10] = 255; report(o, "bmp bad offset", b.p, (int)b.n); free(b.p); }
  { unsigned char junk[64]; for (int i = 0; i < 64; i++) junk[i] = (unsigned char)rnd(); report(o, "junk", junk, 64); }

  fprintf(o, "fails=%d\n", fails);
  return fails;
}
