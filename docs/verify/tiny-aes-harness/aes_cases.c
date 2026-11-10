/* tiny-AES-c (AES-128) differential cases: NIST SP 800-38A known answers for ECB, CBC and CTR,
   plus generated buffers (all lengths 0..70 for CTR, many blocks for CBC, in-place round trips,
   chunked CTR equal to one-shot). Output is hex + FNV hash; nothing varies between runs. */
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include "aes.h"

static uint32_t st = 0xC0FFEE11u;
static uint32_t rnd(void) { st = st * 1664525u + 1013904223u; return st >> 8; }
static uint64_t fnv(const uint8_t *p, size_t n) {
  uint64_t h = 0xcbf29ce484222325ull;
  for (size_t i = 0; i < n; i++) { h ^= p[i]; h *= 0x100000001b3ull; }
  return h;
}
static void hex(FILE *o, const uint8_t *p, size_t n) { for (size_t i = 0; i < n; i++) fprintf(o, "%02x", p[i]); }

static const uint8_t key[16] = {0x2b,0x7e,0x15,0x16,0x28,0xae,0xd2,0xa6,0xab,0xf7,0x15,0x88,0x09,0xcf,0x4f,0x3c};
static const uint8_t iv[16] = {0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15};
static const uint8_t ctr_iv[16] = {0xf0,0xf1,0xf2,0xf3,0xf4,0xf5,0xf6,0xf7,0xf8,0xf9,0xfa,0xfb,0xfc,0xfd,0xfe,0xff};
static const uint8_t pt[64] = {
  0x6b,0xc1,0xbe,0xe2,0x2e,0x40,0x9f,0x96,0xe9,0x3d,0x7e,0x11,0x73,0x93,0x17,0x2a,
  0xae,0x2d,0x8a,0x57,0x1e,0x03,0xac,0x9c,0x9e,0xb7,0x6f,0xac,0x45,0xaf,0x8e,0x51,
  0x30,0xc8,0x1c,0x46,0xa3,0x5c,0xe4,0x11,0xe5,0xfb,0xc1,0x19,0x1a,0x0a,0x52,0xef,
  0xf6,0x9f,0x24,0x45,0xdf,0x4f,0x9b,0x17,0xad,0x2b,0x41,0x7b,0xe6,0x6c,0x37,0x10};
/* NIST SP 800-38A F.1.1, F.2.1, F.5.1 (AES-128 ciphertexts) */
static const uint8_t ecb_ct[64] = {
  0x3a,0xd7,0x7b,0xb4,0x0d,0x7a,0x36,0x60,0xa8,0x9e,0xca,0xf3,0x24,0x66,0xef,0x97,
  0xf5,0xd3,0xd5,0x85,0x03,0xb9,0x69,0x9d,0xe7,0x85,0x89,0x5a,0x96,0xfd,0xba,0xaf,
  0x43,0xb1,0xcd,0x7f,0x59,0x8e,0xce,0x23,0x88,0x1b,0x00,0xe3,0xed,0x03,0x06,0x88,
  0x7b,0x0c,0x78,0x5e,0x27,0xe8,0xad,0x3f,0x82,0x23,0x20,0x71,0x04,0x72,0x5d,0xd4};
static const uint8_t cbc_ct[64] = {
  0x76,0x49,0xab,0xac,0x81,0x19,0xb2,0x46,0xce,0xe9,0x8e,0x9b,0x12,0xe9,0x19,0x7d,
  0x50,0x86,0xcb,0x9b,0x50,0x72,0x19,0xee,0x95,0xdb,0x11,0x3a,0x91,0x76,0x78,0xb2,
  0x73,0xbe,0xd6,0xb8,0xe3,0xc1,0x74,0x3b,0x71,0x16,0xe6,0x9e,0x22,0x22,0x95,0x16,
  0x3f,0xf1,0xca,0xa1,0x68,0x1f,0xac,0x09,0x12,0x0e,0xca,0x30,0x75,0x86,0xe1,0xa7};
static const uint8_t ctr_ct[64] = {
  0x87,0x4d,0x61,0x91,0xb6,0x20,0xe3,0x26,0x1b,0xef,0x68,0x64,0x99,0x0d,0xb6,0xce,
  0x98,0x06,0xf6,0x6b,0x79,0x70,0xfd,0xff,0x86,0x17,0x18,0x7b,0xb9,0xff,0xfd,0xff,
  0x5a,0xe4,0xdf,0x3e,0xdb,0xd5,0xd3,0x5e,0x5b,0x4f,0x09,0x02,0x0d,0xb0,0x3e,0xab,
  0x1e,0x03,0x1d,0xda,0x2f,0xbe,0x03,0xd1,0x79,0x21,0x70,0xa0,0xf3,0x00,0x9c,0xee};

int aes_run_cases(FILE *o) {
  int fails = 0;
  struct AES_ctx c;
  uint8_t b[64];

  /* ECB known answer, per-block API */
  AES_init_ctx(&c, key);
  memcpy(b, pt, 64);
  for (int i = 0; i < 4; i++) AES_ECB_encrypt(&c, b + 16 * i);
  int ok = memcmp(b, ecb_ct, 64) == 0; fails += !ok;
  fprintf(o, "ecb-enc nist=%s ", ok ? "ok" : "BAD"); hex(o, b, 64); fprintf(o, "\n");
  for (int i = 0; i < 4; i++) AES_ECB_decrypt(&c, b + 16 * i);
  ok = memcmp(b, pt, 64) == 0; fails += !ok; fprintf(o, "ecb-dec roundtrip=%s\n", ok ? "ok" : "BAD");

  /* CBC known answer, one call over 64 bytes */
  AES_init_ctx_iv(&c, key, iv);
  memcpy(b, pt, 64); AES_CBC_encrypt_buffer(&c, b, 64);
  ok = memcmp(b, cbc_ct, 64) == 0; fails += !ok;
  fprintf(o, "cbc-enc nist=%s ", ok ? "ok" : "BAD"); hex(o, b, 64); fprintf(o, "\n");
  AES_ctx_set_iv(&c, iv); AES_CBC_decrypt_buffer(&c, b, 64);
  ok = memcmp(b, pt, 64) == 0; fails += !ok; fprintf(o, "cbc-dec roundtrip=%s\n", ok ? "ok" : "BAD");

  /* CTR known answer, whole and in 16/32-byte chunks (the library keeps no partial-block state, so chunks must be block multiples) */
  AES_init_ctx_iv(&c, key, ctr_iv);
  memcpy(b, pt, 64); AES_CTR_xcrypt_buffer(&c, b, 64);
  ok = memcmp(b, ctr_ct, 64) == 0; fails += !ok;
  fprintf(o, "ctr nist=%s ", ok ? "ok" : "BAD"); hex(o, b, 64); fprintf(o, "\n");
  AES_init_ctx_iv(&c, key, ctr_iv);
  memcpy(b, pt, 64);
  for (int p = 0, n; p < 64; p += n) { n = 16 * (1 + (int)(rnd() % 2)); if (n > 64 - p) n = 64 - p; AES_CTR_xcrypt_buffer(&c, b + p, (size_t)n); }
  ok = memcmp(b, ctr_ct, 64) == 0; fails += !ok; fprintf(o, "ctr chunked16=%s\n", ok ? "ok" : "BAD");

  /* generated keys/IVs: CBC over 1..40 blocks, CTR over every length 0..70 and larger buffers */
  static uint8_t buf[4096], ref[4096];
  for (int round = 0; round < 24; round++) {
    uint8_t k[16], v[16];
    for (int i = 0; i < 16; i++) { k[i] = (uint8_t)rnd(); v[i] = (uint8_t)rnd(); }
    if (round == 0) { memset(k, 0, 16); memset(v, 0, 16); }
    if (round == 1) { memset(k, 0xff, 16); memset(v, 0xff, 16); }    /* counter carries through every byte */
    size_t blocks = round < 20 ? (size_t)(round + 1) : (size_t)(64 * (round - 19));
    size_t n = blocks * 16;
    for (size_t i = 0; i < n; i++) ref[i] = (uint8_t)rnd();
    memcpy(buf, ref, n);
    AES_init_ctx_iv(&c, k, v); AES_CBC_encrypt_buffer(&c, buf, n);
    uint64_t h = fnv(buf, n);
    AES_ctx_set_iv(&c, v); AES_CBC_decrypt_buffer(&c, buf, n);
    ok = memcmp(buf, ref, n) == 0; fails += !ok;
    fprintf(o, "cbc round=%d blocks=%zu cthash=%016llx first=", round, blocks, (unsigned long long)h);
    hex(o, ref, 4); fprintf(o, " rt=%s\n", ok ? "ok" : "BAD");
  }
  for (size_t n = 0; n <= 70; n++) {
    uint8_t k[16], v[16];
    for (int i = 0; i < 16; i++) { k[i] = (uint8_t)rnd(); v[i] = (n % 3 == 0) ? 0xff : (uint8_t)rnd(); }
    for (size_t i = 0; i < n; i++) ref[i] = (uint8_t)rnd();
    memcpy(buf, ref, n);
    AES_init_ctx_iv(&c, k, v); AES_CTR_xcrypt_buffer(&c, buf, n);
    uint64_t h = fnv(buf, n);
    AES_ctx_set_iv(&c, v); AES_CTR_xcrypt_buffer(&c, buf, n);
    ok = memcmp(buf, ref, n) == 0; fails += !ok;
    fprintf(o, "ctr len=%zu cthash=%016llx rt=%s\n", n, (unsigned long long)h, ok ? "ok" : "BAD");
  }
  { uint8_t k[16], v[16]; for (int i = 0; i < 16; i++) { k[i] = (uint8_t)rnd(); v[i] = (uint8_t)rnd(); }
    for (size_t i = 0; i < 4096; i++) ref[i] = (uint8_t)rnd();
    memcpy(buf, ref, 4096); AES_init_ctx_iv(&c, k, v); AES_CTR_xcrypt_buffer(&c, buf, 4096);
    fprintf(o, "ctr 4096 cthash=%016llx\n", (unsigned long long)fnv(buf, 4096)); }
  fprintf(o, "fails=%d\n", fails);
  return fails;
}
