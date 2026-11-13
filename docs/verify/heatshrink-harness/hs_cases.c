/* heatshrink differential cases: compress generated inputs with several window/lookahead settings,
   print compressed size + hash, decompress (also with 1-byte-at-a-time input and tiny output chunks)
   and check the round trip; plus malformed/truncated streams and API misuse. Fixed LCG; deterministic. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "heatshrink_encoder.h"
#include "heatshrink_decoder.h"

static uint32_t st = 0x1badb002u;
static uint32_t rnd(void) { st = st * 1664525u + 1013904223u; return st >> 8; }
static uint64_t fnv(const uint8_t *p, size_t n) {
  uint64_t h = 0xcbf29ce484222325ull;
  for (size_t i = 0; i < n; i++) { h ^= p[i]; h *= 0x100000001b3ull; }
  return h;
}

static void gen(uint8_t *p, size_t n, int kind) {
  for (size_t i = 0; i < n; i++) switch (kind) {
    case 0: p[i] = 0; break;                                  /* zeros */
    case 1: p[i] = (uint8_t)rnd(); break;                     /* noise */
    case 2: p[i] = "the quick brown fox jumps over the lazy dog. "[i % 45]; break;  /* repetitive text */
    case 3: p[i] = (uint8_t)((i * i) >> 4); break;            /* structured */
    default: p[i] = (rnd() & 7) ? (uint8_t)(i % 13) : (uint8_t)rnd(); break;  /* mixed */
  }
}

/* returns compressed length, or -1 */
static long enc(const uint8_t *in, size_t n, int w, int l, uint8_t *out, size_t cap, int chunk) {
  heatshrink_encoder *e = heatshrink_encoder_alloc((uint8_t)w, (uint8_t)l);
  if (!e) return -1;
  size_t si = 0, so = 0; int bad = 0;
  while (si < n) {
    size_t c = n - si > (size_t)chunk ? (size_t)chunk : n - si, got = 0;
    if (heatshrink_encoder_sink(e, (uint8_t *)in + si, c, &got) < 0) { bad = 1; break; }
    si += got;
    HSE_poll_res pr;
    do { size_t w2 = 0; pr = heatshrink_encoder_poll(e, out + so, cap - so, &w2); so += w2; if (pr < 0) bad = 1; } while (pr == HSER_POLL_MORE && !bad);
  }
  while (!bad && heatshrink_encoder_finish(e) == HSER_FINISH_MORE) {
    size_t w2 = 0; HSE_poll_res pr = heatshrink_encoder_poll(e, out + so, cap - so, &w2); so += w2; if (pr < 0) bad = 1;
  }
  heatshrink_encoder_free(e);
  return bad ? -1 : (long)so;
}

/* in_chunk: bytes sunk at a time; out_chunk: poll buffer size */
static long dec(const uint8_t *in, size_t n, int w, int l, uint8_t *out, size_t cap, int in_chunk, int out_chunk, int *poll_err) {
  heatshrink_decoder *d = heatshrink_decoder_alloc(64, (uint8_t)w, (uint8_t)l);
  if (!d) return -1;
  size_t si = 0, so = 0; int bad = 0; *poll_err = 0;
  while (si < n && !bad) {
    size_t c = n - si > (size_t)in_chunk ? (size_t)in_chunk : n - si, got = 0;
    if (heatshrink_decoder_sink(d, (uint8_t *)in + si, c, &got) < 0) { bad = 1; break; }
    si += got;
    HSD_poll_res pr;
    do {
      size_t w2 = 0, room = cap - so; if (room > (size_t)out_chunk) room = (size_t)out_chunk;
      pr = heatshrink_decoder_poll(d, out + so, room, &w2); so += w2;
      if (pr < 0) { bad = 1; *poll_err = pr; }
      if (so >= cap && pr == HSDR_POLL_MORE) { bad = 1; break; }
    } while (pr == HSDR_POLL_MORE && !bad);
  }
  HSD_finish_res fr = bad ? HSDR_FINISH_DONE : heatshrink_decoder_finish(d);
  heatshrink_decoder_free(d);
  return bad ? -1 : (fr == HSDR_FINISH_DONE ? (long)so : -2);
}

int hs_run_cases(FILE *o) {
  int fails = 0;
  static uint8_t in[20000], comp[40000], back[30000];
  static const struct { int w, l; } cfg[] = {{8,4},{8,3},{10,5},{11,4},{4,3},{15,8},{12,6}};
  static const size_t lens[] = {0, 1, 2, 3, 17, 255, 256, 1000, 4097, 20000};
  for (unsigned c = 0; c < sizeof cfg / sizeof *cfg; c++)
    for (int kind = 0; kind < 5; kind++)
      for (unsigned li = 0; li < sizeof lens / sizeof *lens; li++) {
        size_t n = lens[li];
        if (c > 2 && (n == 20000 || n == 4097) && kind != 2) continue;   /* keep the run time modest */
        gen(in, n, kind);
        int chunk = 1 + (int)((c * 7 + li * 13) % 200);
        long cl = enc(in, n, cfg[c].w, cfg[c].l, comp, sizeof comp, chunk);
        fprintf(o, "w=%d l=%d kind=%d n=%zu chunk=%d clen=%ld", cfg[c].w, cfg[c].l, kind, n, chunk, cl);
        if (cl < 0) { fprintf(o, " ENCODE-FAIL\n"); fails++; continue; }
        fprintf(o, " chash=%016llx", (unsigned long long)fnv(comp, (size_t)cl));
        int pe;
        long d1 = dec(comp, (size_t)cl, cfg[c].w, cfg[c].l, back, sizeof back, 1000000, 100000, &pe);
        int ok1 = d1 == (long)n && memcmp(back, in, n) == 0; fails += !ok1;
        long d2 = dec(comp, (size_t)cl, cfg[c].w, cfg[c].l, back, sizeof back, 1, 1, &pe);
        int ok2 = d2 == (long)n && memcmp(back, in, n) == 0; fails += !ok2;
        fprintf(o, " dec=%s dec1=%s\n", ok1 ? "ok" : "BAD", ok2 ? "ok" : "BAD");
      }

  /* malformed streams: decode noise and truncations; the result (length+hash or error) must match the host */
  for (int i = 0; i < 40; i++) {
    size_t n = 1 + rnd() % 300;
    for (size_t k = 0; k < n; k++) comp[k] = (uint8_t)rnd();
    int pe; long r = dec(comp, n, 8, 4, back, 8000, 7, 33, &pe);
    fprintf(o, "noise %d n=%zu -> %ld", i, n, r);
    if (r >= 0) fprintf(o, " hash=%016llx", (unsigned long long)fnv(back, (size_t)r));
    fprintf(o, "\n");
  }
  gen(in, 3000, 2);
  { long cl = enc(in, 3000, 8, 4, comp, sizeof comp, 100);
    for (long cut = cl; cut > cl - 30 && cut > 0; cut -= 5) {
      int pe; long r = dec(comp, (size_t)cut, 8, 4, back, sizeof back, 100000, 100000, &pe);
      fprintf(o, "truncated cut=%ld/%ld -> %ld", cut, cl, r);
      if (r >= 0) fprintf(o, " hash=%016llx", (unsigned long long)fnv(back, (size_t)r));
      fprintf(o, "\n");
    } }

  /* API misuse and bad parameters */
  fprintf(o, "alloc bad-window %d\n", heatshrink_encoder_alloc(3, 2) != NULL);
  fprintf(o, "alloc bad-lookahead %d\n", heatshrink_encoder_alloc(8, 8) != NULL);
  fprintf(o, "alloc window16 %d\n", heatshrink_encoder_alloc(16, 4) != NULL);
  fprintf(o, "dec-alloc zero-buf %d\n", heatshrink_decoder_alloc(0, 8, 4) != NULL);
  { size_t got = 0; uint8_t x[4] = {1,2,3,4};
    fprintf(o, "sink-null enc=%d dec=%d\n", (int)heatshrink_encoder_sink(NULL, x, 4, &got), (int)heatshrink_decoder_sink(NULL, x, 4, &got));
    heatshrink_encoder *e = heatshrink_encoder_alloc(8, 4);
    heatshrink_encoder_sink(e, x, 4, &got); heatshrink_encoder_finish(e);
    fprintf(o, "sink-after-finish=%d got=%zu\n", (int)heatshrink_encoder_sink(e, x, 4, &got), got);
    heatshrink_encoder_free(e); }
  fprintf(o, "fails=%d\n", fails);
  return fails;
}
