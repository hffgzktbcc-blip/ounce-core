/* LodePNG differential cases (C build of lodepng.cpp: LODEPNG_NO_COMPILE_CPP, no disk I/O).
   Generated images are encoded with many colour types, bit depths, filter strategies, deflate settings
   and Adam7 interlacing; the output size and hash of the PNG are printed, then decoded back (native
   format and RGBA8) and compared with the source. Also: corrupt/truncated PNGs with the error code,
   the standalone zlib compress/decompress, and the colour converter. Deterministic (fixed LCG). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "lodepng.h"

static uint32_t st = 0xabcdef01u;
static uint32_t rnd(void) { st = st * 1664525u + 1013904223u; return st >> 8; }
static uint64_t fnv(const unsigned char *p, size_t n) {
  uint64_t h = 0xcbf29ce484222325ull;
  for (size_t i = 0; i < n; i++) { h ^= p[i]; h *= 0x100000001b3ull; }
  return h;
}

static size_t bits_per_px(LodePNGColorType ct, unsigned bd) {
  unsigned ch = ct == LCT_GREY ? 1 : ct == LCT_RGB ? 3 : ct == LCT_PALETTE ? 1 : ct == LCT_GREY_ALPHA ? 2 : 4;
  return (size_t)ch * bd;
}

/* Fill a raw image in format (ct,bd). LodePNG's raw buffers are tightly packed: for bit depths below 8 there is
   NO padding at the end of each row, only after the last pixel. */
static unsigned char *make_raw(unsigned w, unsigned h, LodePNGColorType ct, unsigned bd, int kind, size_t *size) {
  size_t bits = (size_t)w * h * bits_per_px(ct, bd); *size = (bits + 7) / 8;
  unsigned char *p = malloc(*size ? *size : 1);
  size_t line = (w * bits_per_px(ct, bd) + 7) / 8;  /* only used to shape the pattern */
  for (size_t i = 0; i < *size; i++) {
    unsigned v; size_t y = i / line, x = i % line;
    switch (kind) {
      case 0: v = 0; break;
      case 1: v = (unsigned)(x * 3 + y * 5); break;
      case 2: v = rnd(); break;
      case 3: v = ((x / 6) + (y / 4)) & 1 ? 200 : 40; break;
      default: v = (unsigned)(x * 2 + y) + (rnd() & 3); break;
    }
    p[i] = (unsigned char)v;
  }
  if (ct == LCT_PALETTE && bd == 8) for (size_t i = 0; i < *size; i++) p[i] &= 15;   /* 16-entry palette; packed 1/2/4-bit indices are always valid */
  if (bits % 8) p[*size - 1] &= (unsigned char)(0xff << (8 - bits % 8));              /* zero the padding after the last pixel */
  return p;
}

static int one(FILE *o, unsigned w, unsigned h, LodePNGColorType ct, unsigned bd, int kind, int filter, int interlace, int btype, int lz77, unsigned win) {
  size_t rawsize; unsigned char *raw = make_raw(w, h, ct, bd, kind, &rawsize);
  LodePNGState s; lodepng_state_init(&s);
  s.info_raw.colortype = ct; s.info_raw.bitdepth = bd;
  s.info_png.color.colortype = ct; s.info_png.color.bitdepth = bd;
  s.info_png.interlace_method = (unsigned)interlace;
  s.encoder.auto_convert = 0;
  s.encoder.filter_strategy = (LodePNGFilterStrategy)filter;
  s.encoder.zlibsettings.btype = (unsigned)btype;
  s.encoder.zlibsettings.use_lz77 = (unsigned)lz77;
  s.encoder.zlibsettings.windowsize = win;
  if (ct == LCT_PALETTE) {
    unsigned n = bd == 8 ? 16 : 1u << bd;
    for (unsigned i = 0; i < n; i++) {
      lodepng_palette_add(&s.info_png.color, (unsigned char)(i * 17), (unsigned char)(255 - i * 9), (unsigned char)(i * 31), (unsigned char)(i == 1 ? 77 : 255));
      lodepng_palette_add(&s.info_raw, (unsigned char)(i * 17), (unsigned char)(255 - i * 9), (unsigned char)(i * 31), (unsigned char)(i == 1 ? 77 : 255));
    }
  }
  unsigned char *png = NULL; size_t pngsize = 0;
  unsigned err = lodepng_encode(&png, &pngsize, raw, w, h, &s);
  fprintf(o, "enc ct=%d bd=%u %ux%u kind=%d filter=%d il=%d btype=%d lz77=%d win=%u -> err=%u", (int)ct, bd, w, h, kind, filter, interlace, btype, lz77, win, err);
  int fails = 0;
  if (!err) {
    fprintf(o, " size=%zu hash=%016llx", pngsize, (unsigned long long)fnv(png, pngsize));
    unsigned char *back = NULL; unsigned w2, h2; LodePNGState d; lodepng_state_init(&d);
    d.info_raw.colortype = ct; d.info_raw.bitdepth = bd;
    if (ct == LCT_PALETTE) { /* decode keeps the file palette in info_png; ask for raw indices */ }
    unsigned e2 = lodepng_decode(&back, &w2, &h2, &d, png, pngsize);
    int ok = !e2 && w2 == w && h2 == h && memcmp(back, raw, rawsize) == 0; fails += !ok;
    fprintf(o, " dec=%s", ok ? "ok" : "BAD");
    if (e2) fprintf(o, "(err %u)", e2);
    free(back);
    unsigned char *rgba = NULL; unsigned e3 = lodepng_decode_memory(&rgba, &w2, &h2, png, pngsize, LCT_RGBA, 8);
    fprintf(o, " rgba8=%s:%016llx", e3 ? "ERR" : "ok", e3 ? 0ull : (unsigned long long)fnv(rgba, (size_t)w2 * h2 * 4));
    free(rgba);
    lodepng_state_cleanup(&d);
  } else fprintf(o, " (%s)", lodepng_error_text(err));
  fprintf(o, "\n");
  free(png); free(raw); lodepng_state_cleanup(&s);
  return fails;
}

int lp_run_cases(FILE *o) {
  int fails = 0;
  /* colour formats x filter strategies x deflate settings */
  static const struct { LodePNGColorType ct; unsigned bd; } fmt[] = {
    {LCT_GREY,1},{LCT_GREY,2},{LCT_GREY,4},{LCT_GREY,8},{LCT_GREY,16},{LCT_RGB,8},{LCT_RGB,16},
    {LCT_PALETTE,1},{LCT_PALETTE,2},{LCT_PALETTE,4},{LCT_PALETTE,8},{LCT_GREY_ALPHA,8},{LCT_GREY_ALPHA,16},{LCT_RGBA,8},{LCT_RGBA,16}};
  for (unsigned f = 0; f < sizeof fmt / sizeof *fmt; f++)
    for (int kind = 0; kind < 5; kind++) {
      unsigned w = 1 + (f * 7 + kind * 5) % 37, h = 1 + (f * 3 + kind * 11) % 29;
      int filter = (f + kind) % 7;  /* LFS_ZERO..LFS_PREDEFINED */
      if (filter == LFS_PREDEFINED) filter = LFS_ENTROPY;
      fails += one(o, w, h, fmt[f].ct, fmt[f].bd, kind, filter, (f + kind) & 1, 2, 1, 2048);
    }
  /* deflate settings on one image */
  static const struct { int btype, lz77; unsigned win; } z[] = {{0,0,32768},{1,0,32768},{1,1,256},{2,1,32768},{2,1,2},{2,0,32768},{2,1,1024}};
  for (unsigned i = 0; i < sizeof z / sizeof *z; i++)
    for (int kind = 1; kind < 5; kind += 1)
      fails += one(o, 130, 60, LCT_RGBA, 8, kind, i % 5, 0, z[i].btype, z[i].lz77, z[i].win);
  /* filter strategies: ZERO ONE TWO THREE FOUR MINSUM ENTROPY BRUTE_FORCE on a larger gradient */
  for (int f = 0; f <= 7; f++) fails += one(o, 160, 90, LCT_RGB, 8, 4, f, 0, 2, 1, 32768);
  /* automatic format reduction */
  { unsigned char rgba[16 * 16 * 4];
    for (int mode = 0; mode < 4; mode++) {
      for (int i = 0; i < 16 * 16; i++) {
        unsigned char g = (unsigned char)(i & 1 ? 255 : 0);
        rgba[i*4] = mode == 0 ? g : (unsigned char)(i * 3); rgba[i*4+1] = mode == 0 ? g : (mode == 1 ? rgba[i*4] : (unsigned char)(i * 5)); rgba[i*4+2] = mode < 2 ? rgba[i*4] : (unsigned char)(i * 7);
        rgba[i*4+3] = mode == 3 ? (unsigned char)(i * 11) : 255; }
      unsigned char *png; size_t n; unsigned err = lodepng_encode_memory(&png, &n, rgba, 16, 16, LCT_RGBA, 8);
      LodePNGState d; lodepng_state_init(&d); unsigned char *img = NULL; unsigned w, h;
      unsigned e2 = lodepng_decode(&img, &w, &h, &d, png, n);
      fprintf(o, "auto mode=%d err=%u size=%zu hash=%016llx file-colortype=%d bd=%u dec-err=%u\n", mode, err, n, (unsigned long long)fnv(png, n), (int)d.info_png.color.colortype, d.info_png.color.bitdepth, e2);
      free(img); free(png); lodepng_state_cleanup(&d); } }

  /* corrupt inputs: error code and text */
  { unsigned char raw[24 * 24 * 4]; for (unsigned i = 0; i < sizeof raw; i++) raw[i] = (unsigned char)(i * 7 + (i >> 5));
    unsigned char *png; size_t n; lodepng_encode_memory(&png, &n, raw, 24, 24, LCT_RGBA, 8);
    unsigned char *c = malloc(n);
    for (long cut = (long)n - 1; cut >= 0; cut -= (long)n / 9 + 1) {
      unsigned char *o2; unsigned w, h; unsigned e = lodepng_decode32(&o2, &w, &h, png, (size_t)cut);
      fprintf(o, "truncated %ld/%zu -> %u %s\n", cut, n, e, lodepng_error_text(e)); if (!e) free(o2); }
    for (int i = 0; i < 40; i++) {
      memcpy(c, png, n); size_t at = rnd() % n; c[at] ^= (unsigned char)(1 + rnd() % 255);
      unsigned char *o2 = NULL; unsigned w = 0, h = 0; unsigned e = lodepng_decode32(&o2, &w, &h, c, n);
      fprintf(o, "flip@%zu -> %u %s", at, e, lodepng_error_text(e)); if (!e) fprintf(o, " %ux%u %016llx", w, h, (unsigned long long)fnv(o2, (size_t)w * h * 4)); fprintf(o, "\n"); free(o2); }
    memcpy(c, png, n); memset(c + 8, 0, 4); { unsigned char *o2; unsigned w, h; unsigned e = lodepng_decode32(&o2, &w, &h, c, n); fprintf(o, "zeroed IHDR len -> %u\n", e); }
    { unsigned char *o2; unsigned w, h; unsigned e = lodepng_decode_memory(&o2, &w, &h, png, n, LCT_PALETTE, 8); fprintf(o, "decode as palette -> %u\n", e); }
    { unsigned char *o2; unsigned w, h; unsigned e = lodepng_decode_memory(&o2, &w, &h, png, n, (LodePNGColorType)5, 8); fprintf(o, "decode bad colortype -> %u\n", e); }
    free(c); free(png); }

  /* standalone zlib */
  { static unsigned char in[20000]; for (int i = 0; i < 20000; i++) in[i] = (unsigned char)((i % 97) * (i % 5 ? 1 : rnd() & 3));
    for (int lvl = 0; lvl < 3; lvl++) {
      LodePNGCompressSettings cs; lodepng_compress_settings_init(&cs); cs.btype = lvl == 0 ? 0 : lvl == 1 ? 1 : 2; cs.use_lz77 = lvl != 0; cs.nicematch = 258; cs.lazymatching = lvl == 2;
      unsigned char *z = NULL; size_t zs = 0; unsigned e = lodepng_zlib_compress(&z, &zs, in, 20000, &cs);
      LodePNGDecompressSettings ds; lodepng_decompress_settings_init(&ds);
      unsigned char *back = NULL; size_t bs = 0; unsigned e2 = lodepng_zlib_decompress(&back, &bs, z, zs, &ds);
      int ok = !e && !e2 && bs == 20000 && memcmp(back, in, 20000) == 0; fails += !ok;
      fprintf(o, "zlib level=%d err=%u size=%zu hash=%016llx rt=%s crc=%08x\n", lvl, e, zs, (unsigned long long)fnv(z, zs), ok ? "ok" : "BAD", lodepng_crc32(in, 20000));
      free(z); free(back); } }

  /* colour converter */
  { unsigned char src[64]; for (int i = 0; i < 64; i++) src[i] = (unsigned char)rnd();
    static const struct { LodePNGColorType a; unsigned ab; LodePNGColorType b; unsigned bb; } cv[] = {
      {LCT_RGBA,16,LCT_RGB,8},{LCT_RGBA,8,LCT_GREY,8},{LCT_RGB,8,LCT_GREY_ALPHA,16},{LCT_GREY,16,LCT_RGBA,8},{LCT_GREY_ALPHA,8,LCT_RGBA,16},{LCT_RGBA,8,LCT_GREY,1},{LCT_GREY,2,LCT_RGB,8}};
    for (unsigned i = 0; i < sizeof cv / sizeof *cv; i++) {
      LodePNGColorMode in, out; lodepng_color_mode_init(&in); lodepng_color_mode_init(&out);
      in.colortype = cv[i].a; in.bitdepth = cv[i].ab; out.colortype = cv[i].b; out.bitdepth = cv[i].bb;
      unsigned char dst[256]; memset(dst, 0, sizeof dst);
      unsigned e = lodepng_convert(dst, src, &out, &in, 2, 2);
      size_t outbytes = (4 * bits_per_px(out.colortype, out.bitdepth) + 7) / 8;
      fprintf(o, "convert %d/%u -> %d/%u err=%u hash=%016llx\n", (int)cv[i].a, cv[i].ab, (int)cv[i].b, cv[i].bb, e, (unsigned long long)fnv(dst, e ? 0 : (outbytes > 64 ? 64 : outbytes)));
      lodepng_color_mode_cleanup(&in); lodepng_color_mode_cleanup(&out); } }

  fprintf(o, "fails=%d\n", fails);
  return fails;
}
