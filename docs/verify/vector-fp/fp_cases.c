#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <arm_neon.h>
/* Vector floating-point differential: 400 operand sets through single (4S, 2S) and double (2D) forms. */
static uint32_t st = 91u;
static uint32_t rnd(void) { st = st * 1664525u + 1013904223u; return st; }
/* FMLA/FMLS use inline asm so host (clang) and guest (gcc) emit the same operand order: the intrinsic leaves it to the compiler and
   the sign of a NaN product input then differs between compilers. */
/* Commutative FADD/FMUL use inline asm for the same reason as FMLA/FMLS: with two quiet-NaN operands the architecture (FPProcessNaNs)
   returns the first operand, and gcc on aarch64 emits `fadd v0, Vm, Vn` for vaddq_f64(n, m) at -O1 (issue #257), so the intrinsic
   makes the host reference depend on the compiler. The asm pins Vn = first operand on every host. */
#define PIN2(insn, T, a, b) ({ T r_; __asm__(insn : "=w"(r_) : "w"(a), "w"(b)); r_; })
/* edge f32 bit patterns: 0, -0, 1, -1, inf, -inf, qNaN (+payload), sNaN, -NaN, smallest/largest subnormal,
   FLT_MIN, FLT_MAX, 2^31, 2^32, int boundaries, 0.5, 1.5, 2.5, -1.5 */
static const uint32_t E32[] = {
  0x00000000u, 0x80000000u, 0x3F800000u, 0xBF800000u, 0x7F800000u, 0xFF800000u, 0x7FC00000u, 0x7FC12345u,
  0x7F800001u, 0xFFC00000u, 0x00000001u, 0x007FFFFFu, 0x80000001u, 0x00800000u, 0x7F7FFFFFu, 0xFF7FFFFFu,
  0x4F000000u, 0xCF000000u, 0x4F800000u, 0x4EFFFFFFu, 0x3F000000u, 0x3FC00000u, 0x40200000u, 0xBFC00000u,
  0x4B800000u, 0x3F7FFFFFu, 0x3F800001u };
static const uint64_t E64[] = {
  0x0000000000000000ull, 0x8000000000000000ull, 0x3FF0000000000000ull, 0xBFF0000000000000ull,
  0x7FF0000000000000ull, 0xFFF0000000000000ull, 0x7FF8000000000000ull, 0x7FF8000000012345ull,
  0x7FF0000000000001ull, 0xFFF8000000000000ull, 0x0000000000000001ull, 0x000FFFFFFFFFFFFFull,
  0x8000000000000001ull, 0x0010000000000000ull, 0x7FEFFFFFFFFFFFFFull, 0xFFEFFFFFFFFFFFFFull,
  0x43E0000000000000ull, 0xC3E0000000000000ull, 0x43F0000000000000ull, 0x41E0000000000000ull,
  0x3FE0000000000000ull, 0x3FF8000000000000ull, 0x4004000000000000ull, 0xBFF8000000000000ull,
  0x41DFFFFFFFC00000ull, 0x3FEFFFFFFFFFFFFFull, 0x3FF0000000000001ull };
static uint32_t pick32(void) {
  uint32_t r = rnd();
  switch (r & 3) {
    case 0: return E32[(r >> 8) % (sizeof E32 / sizeof *E32)];
    case 1: { /* random finite small-ish value so mul/fma/div exercise rounding */
      uint32_t x = rnd(); x = (x & 0x807FFFFFu) | ((0x70u + ((x >> 23) & 0x1F)) << 23); return x; }
    default: return rnd();
  }
}
static uint64_t pick64(void) {
  uint32_t r = rnd();
  switch (r & 3) {
    case 0: return E64[(r >> 8) % (sizeof E64 / sizeof *E64)];
    case 1: { uint64_t x = ((uint64_t)rnd() << 32) | rnd();
      x = (x & 0x800FFFFFFFFFFFFFull) | ((uint64_t)(0x3F0u + ((x >> 52) & 0x1F)) << 52); return x; }
    default: return ((uint64_t)rnd() << 32) | rnd();
  }
}
static void v128(uint8_t *b, int dbl) {
  if (dbl) { for (int i = 0; i < 2; i++) { uint64_t w = pick64(); memcpy(b + 8 * i, &w, 8); } }
  else { for (int i = 0; i < 4; i++) { uint32_t w = pick32(); memcpy(b + 4 * i, &w, 4); } }
}
static void put(FILE *o, const char *n, int i, uint8x16_t r) {
  uint8_t b[16]; vst1q_u8(b, r); fprintf(o, "%s %d ", n, i);
  for (int k = 15; k >= 0; k--) fprintf(o, "%02x", b[k]);
  fputc('\n', o);
}
#define F(x) vreinterpretq_u8_f32(x)
#define D(x) vreinterpretq_u8_f64(x)
#define U(x) vreinterpretq_u8_u32(x)
#define UD(x) vreinterpretq_u8_u64(x)
#define S(x) vreinterpretq_u8_s32(x)
#define SD(x) vreinterpretq_u8_s64(x)
#define Z(x) vreinterpretq_u8_f32(vcombine_f32(x, vdup_n_f32(0)))
#define ZU(x) vreinterpretq_u8_u32(vcombine_u32(x, vdup_n_u32(0)))
int fp_run_cases(FILE *o) {
  for (int i = 0; i < 400; i++) {
    uint8_t a[16], b[16], c[16], a2[16], b2[16], c2[16];
    v128(a, 0); v128(b, 0); v128(c, 0); v128(a2, 1); v128(b2, 1); v128(c2, 1);
    float32x4_t n = vreinterpretq_f32_u8(vld1q_u8(a)), m = vreinterpretq_f32_u8(vld1q_u8(b)), d = vreinterpretq_f32_u8(vld1q_u8(c));
    float64x2_t nd = vreinterpretq_f64_u8(vld1q_u8(a2)), md = vreinterpretq_f64_u8(vld1q_u8(b2)), dd = vreinterpretq_f64_u8(vld1q_u8(c2));
    float32x2_t nl = vget_low_f32(n), ml = vget_low_f32(m);
    /* single, 4S */
    put(o, "fcmeq4s", i, U(vceqq_f32(n, m)));
    put(o, "fcmge4s", i, U(vcgeq_f32(n, m)));
    put(o, "fcmgt4s", i, U(vcgtq_f32(n, m)));
    put(o, "facge4s", i, U(vcageq_f32(n, m)));
    put(o, "facgt4s", i, U(vcagtq_f32(n, m)));
    put(o, "fadd4s", i, F(PIN2("fadd %0.4s, %1.4s, %2.4s", float32x4_t, n, m)));
    put(o, "fsub4s", i, F(vsubq_f32(n, m)));
    put(o, "fmul4s", i, F(PIN2("fmul %0.4s, %1.4s, %2.4s", float32x4_t, n, m)));
    put(o, "fdiv4s", i, F(vdivq_f32(n, m)));
    put(o, "fabd4s", i, F(vabdq_f32(n, m)));
    put(o, "faddp4s", i, F(vpaddq_f32(n, m)));
    { float32x4_t r = d; __asm__("fmla %0.4s, %1.4s, %2.4s" : "+w"(r) : "w"(n), "w"(m)); put(o, "fmla4s", i, F(r)); }
    { float32x4_t r = d; __asm__("fmls %0.4s, %1.4s, %2.4s" : "+w"(r) : "w"(n), "w"(m)); put(o, "fmls4s", i, F(r)); }
    put(o, "fmax4s", i, F(vmaxq_f32(n, m)));
    put(o, "fmin4s", i, F(vminq_f32(n, m)));
    put(o, "fmaxnm4s", i, F(vmaxnmq_f32(n, m)));
    put(o, "fminnm4s", i, F(vminnmq_f32(n, m)));
    put(o, "fcmeq0_4s", i, U(vceqzq_f32(n)));
    put(o, "fcmge0_4s", i, U(vcgezq_f32(n)));
    put(o, "fcmgt0_4s", i, U(vcgtzq_f32(n)));
    put(o, "fcmle0_4s", i, U(vclezq_f32(n)));
    put(o, "fcmlt0_4s", i, U(vcltzq_f32(n)));
    put(o, "fabs4s", i, F(vabsq_f32(n)));
    put(o, "fneg4s", i, F(vnegq_f32(n)));
    put(o, "fsqrt4s", i, F(vsqrtq_f32(n)));
    put(o, "fcvtzs4s", i, S(vcvtq_s32_f32(n)));
    put(o, "fcvtzu4s", i, U(vcvtq_u32_f32(n)));
    /* single, 2S (upper half must be zero) */
    put(o, "fcmeq2s", i, ZU(vceq_f32(nl, ml)));
    put(o, "fcmge2s", i, ZU(vcge_f32(nl, ml)));
    put(o, "fcmgt2s", i, ZU(vcgt_f32(nl, ml)));
    put(o, "fmul2s", i, Z(PIN2("fmul %0.2s, %1.2s, %2.2s", float32x2_t, nl, ml)));
    { float32x2_t r = vget_low_f32(d); __asm__("fmla %0.2s, %1.2s, %2.2s" : "+w"(r) : "w"(nl), "w"(ml)); put(o, "fmla2s", i, Z(r)); }
    { float32x2_t r = vget_low_f32(d); __asm__("fmls %0.2s, %1.2s, %2.2s" : "+w"(r) : "w"(nl), "w"(ml)); put(o, "fmls2s", i, Z(r)); }
    put(o, "fmax2s", i, Z(vmax_f32(nl, ml)));
    put(o, "fmaxnm2s", i, Z(vmaxnm_f32(nl, ml)));
    put(o, "fcvtzs2s", i, ZU(vreinterpret_u32_s32(vcvt_s32_f32(nl))));
    /* double, 2D */
    put(o, "fcmeq2d", i, UD(vceqq_f64(nd, md)));
    put(o, "fcmge2d", i, UD(vcgeq_f64(nd, md)));
    put(o, "fcmgt2d", i, UD(vcgtq_f64(nd, md)));
    put(o, "facge2d", i, UD(vcageq_f64(nd, md)));
    put(o, "facgt2d", i, UD(vcagtq_f64(nd, md)));
    put(o, "fadd2d", i, D(PIN2("fadd %0.2d, %1.2d, %2.2d", float64x2_t, nd, md)));
    put(o, "fsub2d", i, D(vsubq_f64(nd, md)));
    put(o, "fmul2d", i, D(PIN2("fmul %0.2d, %1.2d, %2.2d", float64x2_t, nd, md)));
    put(o, "fdiv2d", i, D(vdivq_f64(nd, md)));
    put(o, "fabd2d", i, D(vabdq_f64(nd, md)));
    put(o, "faddp2d", i, D(vpaddq_f64(nd, md)));
    { float64x2_t r = dd; __asm__("fmla %0.2d, %1.2d, %2.2d" : "+w"(r) : "w"(nd), "w"(md)); put(o, "fmla2d", i, D(r)); }
    { float64x2_t r = dd; __asm__("fmls %0.2d, %1.2d, %2.2d" : "+w"(r) : "w"(nd), "w"(md)); put(o, "fmls2d", i, D(r)); }
    put(o, "fmax2d", i, D(vmaxq_f64(nd, md)));
    put(o, "fmin2d", i, D(vminq_f64(nd, md)));
    put(o, "fmaxnm2d", i, D(vmaxnmq_f64(nd, md)));
    put(o, "fminnm2d", i, D(vminnmq_f64(nd, md)));
    put(o, "fcmeq0_2d", i, UD(vceqzq_f64(nd)));
    put(o, "fcmge0_2d", i, UD(vcgezq_f64(nd)));
    put(o, "fcmgt0_2d", i, UD(vcgtzq_f64(nd)));
    put(o, "fcmle0_2d", i, UD(vclezq_f64(nd)));
    put(o, "fcmlt0_2d", i, UD(vcltzq_f64(nd)));
    put(o, "fabs2d", i, D(vabsq_f64(nd)));
    put(o, "fneg2d", i, D(vnegq_f64(nd)));
    put(o, "fsqrt2d", i, D(vsqrtq_f64(nd)));
    put(o, "fcvtzs2d", i, SD(vcvtq_s64_f64(nd)));
    put(o, "fcvtzu2d", i, UD(vcvtq_u64_f64(nd)));
    /* double <-> single */
    put(o, "fcvtn", i, Z(vcvt_f32_f64(nd)));
    put(o, "fcvtn2", i, F(vcvt_high_f32_f64(vget_low_f32(d), nd)));
    put(o, "fcvtl", i, D(vcvt_f64_f32(nl)));
    put(o, "fcvtl2", i, D(vcvt_high_f64_f32(n)));
  }
  return 0;
}
