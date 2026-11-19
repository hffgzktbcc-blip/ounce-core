/* Pinned floating-point edge-case workload for #359.
 *
 * One source, two builds: FREESTANDING=1 is a static AArch64 Linux ELF for `ounce-core run-elf` (no libc),
 * the default is a native program for an ARM64 host (macOS or Linux). Both print one line per case:
 *   <op> <fpcr> <operands...> <result> <fpsr&0x9f>
 * with every value as raw hex bits, so the outputs can be compared byte for byte. FPSR is cleared before each case.
 * Operands are fixed bit patterns (table below); nothing is random and nothing depends on the clock.
 */
#include <stdint.h>

typedef uint64_t u64;
typedef uint32_t u32;

#ifdef FREESTANDING
static long sys3(long nr, long a, long b, long c) {
    register long x8 __asm__("x8") = nr, x0 __asm__("x0") = a, x1 __asm__("x1") = b, x2 __asm__("x2") = c;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x8), "r"(x1), "r"(x2) : "memory");
    return x0;
}
static char obuf[8192];
static unsigned olen;
static void flush(void) { if (olen) sys3(64, 1, (long)obuf, olen); olen = 0; }
static void putc_(char c) { if (olen == sizeof obuf) flush(); obuf[olen++] = c; }
#else
#include <stdio.h>
static void putc_(char c) { putchar(c); }
static void flush(void) { fflush(stdout); }
#endif

static int strcmp_(const char *a, const char *b) { while (*a && *a == *b) a++, b++; return *a != *b; }
static void puts_(const char *s) { while (*s) putc_(*s++); }
static void hex(u64 v, int digits) {
    for (int i = digits - 1; i >= 0; i--) putc_("0123456789abcdef"[(v >> (4 * i)) & 15]);
}

/* ---- operand tables (raw IEEE-754 bit patterns) ---- */
static const u64 D[] = {
    0x0000000000000000ull, /* +0 */
    0x8000000000000000ull, /* -0 */
    0x3ff0000000000000ull, /* 1 */
    0xbff0000000000000ull, /* -1 */
    0x3ff8000000000000ull, /* 1.5 */
    0x3fb999999999999aull, /* 0.1 */
    0x0000000000000001ull, /* smallest subnormal */
    0x000fffffffffffffull, /* largest subnormal */
    0x8000000000000003ull, /* -subnormal */
    0x0010000000000000ull, /* smallest normal */
    0x7fefffffffffffffull, /* max finite */
    0x7ff0000000000000ull, /* +inf */
    0xfff0000000000000ull, /* -inf */
    0x7ff8000000000000ull, /* quiet NaN */
    0x7ff0000000000001ull, /* signalling NaN */
    0xfff8000000000123ull, /* negative quiet NaN with payload */
    0x43e0000000000000ull, /* 2^63 */
    0x4340000000000000ull, /* 2^53 */
};
static const u32 S[] = {
    0x00000000u, 0x80000000u, 0x3f800000u, 0xbf800000u, 0x3fc00000u, 0x3dcccccdu,
    0x00000001u, 0x007fffffu, 0x80000003u, 0x00800000u, 0x7f7fffffu, 0x7f800000u,
    0xff800000u, 0x7fc00000u, 0x7f800001u, 0xffc00123u, 0x5f000000u, 0x4b800000u,
};
#define ND ((int)(sizeof D / sizeof D[0]))
#define NS ((int)(sizeof S / sizeof S[0]))

/* FPCR: default, FZ (bit 24), DN (bit 25), FZ+DN, RMode RP/RM/RZ (bits 23:22 = 1/2/3), FZ+DN+RZ. */
static const u64 FPCRS[] = { 0, 1u << 24, 1u << 25, (1u << 24) | (1u << 25), 1u << 22, 2u << 22, 3u << 22,
                             (1u << 24) | (1u << 25) | (3u << 22) };
#define NF ((int)(sizeof FPCRS / sizeof FPCRS[0]))

#define PRE(fpcr) __asm__ volatile("msr fpcr, %0\n msr fpsr, xzr" ::"r"(fpcr) : "memory")
#define POST(fpsr) __asm__ volatile("mrs %0, fpsr\n msr fpcr, xzr" : "=r"(fpsr))

/* d,d -> d */
#define D2(NAME, INSN) static u64 NAME(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) { \
    u64 r; (void)c; PRE(fpcr); \
    __asm__ volatile("fmov d0, %1\n fmov d1, %2\n " INSN " d2, d0, d1\n fmov %0, d2" : "=r"(r) : "r"(a), "r"(b) : "v0", "v1", "v2"); \
    POST(*fpsr); return r; }
D2(fadd_d, "fadd") D2(fsub_d, "fsub") D2(fmul_d, "fmul") D2(fdiv_d, "fdiv")
D2(fmax_d, "fmax") D2(fmin_d, "fmin") D2(fmaxnm_d, "fmaxnm") D2(fminnm_d, "fminnm") D2(fnmul_d, "fnmul")
D2(fabd_d, "fabd") D2(frecps_d, "frecps") D2(frsqrts_d, "frsqrts")
/* d -> d */
#define D1(NAME, INSN) static u64 NAME(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) { \
    u64 r; (void)b; (void)c; PRE(fpcr); \
    __asm__ volatile("fmov d0, %1\n " INSN " d2, d0\n fmov %0, d2" : "=r"(r) : "r"(a) : "v0", "v2"); \
    POST(*fpsr); return r; }
D1(fsqrt_d, "fsqrt") D1(frintx_d, "frintx") D1(frinta_d, "frinta") D1(frintz_d, "frintz") D1(frinti_d, "frinti")
D1(frintm_d, "frintm") D1(frecpe_d, "frecpe") D1(frsqrte_d, "frsqrte") D1(fneg_d, "fneg") D1(fabs_d, "fabs")
D1(frint32z_d, "frint32z") D1(frint64x_d, "frint64x")
/* d,d,d -> d */
static u64 fmadd_d(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) {
    u64 r; PRE(fpcr);
    __asm__ volatile("fmov d0, %1\n fmov d1, %2\n fmov d3, %3\n fmadd d2, d0, d1, d3\n fmov %0, d2" : "=r"(r) : "r"(a), "r"(b), "r"(c) : "v0", "v1", "v2", "v3");
    POST(*fpsr); return r;
}
/* d -> integer / d -> s / compare */
#define DI(NAME, INSN) static u64 NAME(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) { \
    u64 r; (void)b; (void)c; PRE(fpcr); \
    __asm__ volatile("fmov d0, %1\n " INSN " %0, d0" : "=r"(r) : "r"(a) : "v0"); \
    POST(*fpsr); return r; }
DI(fcvtzs_dx, "fcvtzs") DI(fcvtzu_dx, "fcvtzu") DI(fcvtas_dx, "fcvtas") DI(fcvtms_dx, "fcvtms") DI(fcvtpu_dx, "fcvtpu")
static u64 fcvtzs_dw(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) {
    u64 r; (void)b; (void)c; PRE(fpcr);
    __asm__ volatile("fmov d0, %1\n fcvtzs %w0, d0" : "=r"(r) : "r"(a) : "v0");
    POST(*fpsr); return r & 0xffffffffull;
}
static u64 fcvt_ds(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) {
    u64 r; (void)b; (void)c; PRE(fpcr);
    __asm__ volatile("fmov d0, %1\n fcvt s2, d0\n fmov %w0, s2" : "=r"(r) : "r"(a) : "v0", "v2");
    POST(*fpsr); return r & 0xffffffffull;
}
static u64 scvtf_xd(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) {
    u64 r; (void)b; (void)c; PRE(fpcr);
    __asm__ volatile("scvtf d2, %1\n fmov %0, d2" : "=r"(r) : "r"(a) : "v2");
    POST(*fpsr); return r;
}
static u64 ucvtf_xd(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) {
    u64 r; (void)b; (void)c; PRE(fpcr);
    __asm__ volatile("ucvtf d2, %1\n fmov %0, d2" : "=r"(r) : "r"(a) : "v2");
    POST(*fpsr); return r;
}
static u64 fcmp_d(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) {
    u64 r; (void)c; PRE(fpcr);
    __asm__ volatile("fmov d0, %1\n fmov d1, %2\n fcmpe d0, d1\n mrs %0, nzcv" : "=r"(r) : "r"(a), "r"(b) : "v0", "v1", "cc");
    POST(*fpsr); return (r >> 28) & 15;
}
/* s,s -> s */
#define S2(NAME, INSN) static u64 NAME(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) { \
    u64 r; (void)c; PRE(fpcr); \
    __asm__ volatile("fmov s0, %w1\n fmov s1, %w2\n " INSN " s2, s0, s1\n fmov %w0, s2" : "=r"(r) : "r"(a), "r"(b) : "v0", "v1", "v2"); \
    POST(*fpsr); return r & 0xffffffffull; }
S2(fadd_s, "fadd") S2(fsub_s, "fsub") S2(fmul_s, "fmul") S2(fdiv_s, "fdiv") S2(fmax_s, "fmax") S2(fmin_s, "fmin")
#define S1(NAME, INSN) static u64 NAME(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) { \
    u64 r; (void)b; (void)c; PRE(fpcr); \
    __asm__ volatile("fmov s0, %w1\n " INSN " s2, s0\n fmov %w0, s2" : "=r"(r) : "r"(a) : "v0", "v2"); \
    POST(*fpsr); return r & 0xffffffffull; }
S1(fsqrt_s, "fsqrt") S1(frintx_s, "frintx") S1(frinta_s, "frinta") S1(frecpe_s, "frecpe") S1(frsqrte_s, "frsqrte")
static u64 fcvt_sd(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) {
    u64 r; (void)b; (void)c; PRE(fpcr);
    __asm__ volatile("fmov s0, %w1\n fcvt d2, s0\n fmov %0, d2" : "=r"(r) : "r"(a) : "v0", "v2");
    POST(*fpsr); return r;
}
static u64 fcvtzs_sw(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) {
    u64 r; (void)b; (void)c; PRE(fpcr);
    __asm__ volatile("fmov s0, %w1\n fcvtzs %w0, s0" : "=r"(r) : "r"(a) : "v0");
    POST(*fpsr); return r & 0xffffffffull;
}

/* ---- scalar FCVT* variants added for #488 (the original set above is unchanged) ---- */
/* d -> w, s -> x, s -> w integer converts; the d -> x forms are DI() above. */
#define FDW(NAME, INSN) static u64 NAME(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) { \
    u64 r; (void)b; (void)c; PRE(fpcr); \
    __asm__ volatile("fmov d0, %1\n " INSN " %w0, d0" : "=r"(r) : "r"(a) : "v0"); \
    POST(*fpsr); return r & 0xffffffffull; }
#define FSX(NAME, INSN) static u64 NAME(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) { \
    u64 r; (void)b; (void)c; PRE(fpcr); \
    __asm__ volatile("fmov s0, %w1\n " INSN " %0, s0" : "=r"(r) : "r"(a) : "v0"); \
    POST(*fpsr); return r; }
#define FSW(NAME, INSN) static u64 NAME(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) { \
    u64 r; (void)b; (void)c; PRE(fpcr); \
    __asm__ volatile("fmov s0, %w1\n " INSN " %w0, s0" : "=r"(r) : "r"(a) : "v0"); \
    POST(*fpsr); return r & 0xffffffffull; }
/* all four forms of one mnemonic (names: <insn>.dx .dw .sx .sw) */
#define FCVT4(M) DI(M##_dx, #M) FDW(M##_dw, #M) FSX(M##_sx, #M) FSW(M##_sw, #M)
FCVT4(fcvtns) FCVT4(fcvtnu) FCVT4(fcvtau) FCVT4(fcvtmu) FCVT4(fcvtps)
/* remaining forms of mnemonics whose first form was already listed */
FSX(fcvtzs_sx, "fcvtzs")
FDW(fcvtzu_dw, "fcvtzu") FSX(fcvtzu_sx, "fcvtzu") FSW(fcvtzu_sw, "fcvtzu")
FDW(fcvtas_dw, "fcvtas") FSX(fcvtas_sx, "fcvtas") FSW(fcvtas_sw, "fcvtas")
FDW(fcvtms_dw, "fcvtms") FSX(fcvtms_sx, "fcvtms") FSW(fcvtms_sw, "fcvtms")
FDW(fcvtpu_dw, "fcvtpu") FSX(fcvtpu_sx, "fcvtpu") FSW(fcvtpu_sw, "fcvtpu")
/* fixed-point (#fbits) forms; REG is the integer register modifier, SRC the FP register, FMOVA the fmov operand modifier */
#define FX(NAME, REG, SRC, FMOVA, INSN, FB, MASK) static u64 NAME(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) { \
    u64 r; (void)b; (void)c; PRE(fpcr); \
    __asm__ volatile("fmov " SRC "0, %" FMOVA "1\n " INSN " %" REG "0, " SRC "0, #" FB : "=r"(r) : "r"(a) : "v0"); \
    POST(*fpsr); return r & (MASK); }
FX(fcvtzs_fx_dx, "", "d", "", "fcvtzs", "16", ~0ull)
FX(fcvtzu_fx_dw, "w", "d", "", "fcvtzu", "5", 0xffffffffull)
FX(fcvtzs_fx_sx, "", "s", "w", "fcvtzs", "33", ~0ull)
FX(fcvtzu_fx_sw, "w", "s", "w", "fcvtzu", "7", 0xffffffffull)
FDW(fjcvtzs_dw, "fjcvtzs")
static u64 fcvtxn_ds(u64 a, u64 b, u64 c, u64 fpcr, u64 *fpsr) {
    u64 r; (void)b; (void)c; PRE(fpcr);
    __asm__ volatile("fmov d0, %1\n fcvtxn s2, d0\n fmov %w0, s2" : "=r"(r) : "r"(a) : "v0", "v2");
    POST(*fpsr); return r & 0xffffffffull;
}

/* ---- vector (SIMD) ops, #488 ----
 * Operands are full 128-bit vectors. Lane k of operand a for table index i is table[(i + k) % n]; of b for index j it is
 * table[(j + k) % n]; of the accumulator c (FMLA/FMLS) it is table[(i + j + k) % n]. So lane 0 walks the same 18 x 18
 * grid as the scalar ops while the other lanes carry different patterns, which catches a lane that is dropped,
 * swapped or inherits another lane's flags. FPSR is the OR over all lanes. Printed as 32 hex digits per vector. */
typedef void (*vopfn)(const u64 *, const u64 *, const u64 *, u64, u64 *, u64 *);
#define VHEAD(NAME) static void NAME(const u64 *a, const u64 *b, const u64 *c, u64 fpcr, u64 *fpsr, u64 *r)
#define V1(NAME, INSN) VHEAD(NAME) { (void)b; (void)c; PRE(fpcr); \
    __asm__ volatile("ldr q0, [%0]\n " INSN "\n str q2, [%1]" :: "r"(a), "r"(r) : "memory", "v0", "v2"); POST(*fpsr); }
#define V2(NAME, INSN) VHEAD(NAME) { (void)c; PRE(fpcr); \
    __asm__ volatile("ldr q0, [%0]\n ldr q1, [%1]\n " INSN "\n str q2, [%2]" :: "r"(a), "r"(b), "r"(r) : "memory", "v0", "v1", "v2"); POST(*fpsr); }
#define V3(NAME, INSN) VHEAD(NAME) { PRE(fpcr); \
    __asm__ volatile("ldr q0, [%0]\n ldr q1, [%1]\n ldr q2, [%2]\n " INSN "\n str q2, [%3]" :: "r"(a), "r"(b), "r"(c), "r"(r) : "memory", "v0", "v1", "v2"); POST(*fpsr); }
/* one mnemonic on both arrangements: names <insn>.4s and <insn>.2d */
#define VB(M) V2(M##_4s, #M " v2.4s, v0.4s, v1.4s") V2(M##_2d, #M " v2.2d, v0.2d, v1.2d")
#define VU(M) V1(M##_4s, #M " v2.4s, v0.4s") V1(M##_2d, #M " v2.2d, v0.2d")
VB(fadd) VB(fsub) VB(fmul) VB(fdiv) VB(fmulx) VB(fmax) VB(fmin) VB(fmaxnm) VB(fminnm) VB(fabd)
VB(frecps) VB(frsqrts) VB(faddp) VB(fmaxp) VB(fminp)
VB(fcmeq) VB(fcmge) VB(fcmgt) VB(facge) VB(facgt)
V3(fmla_4s, "fmla v2.4s, v0.4s, v1.4s") V3(fmla_2d, "fmla v2.2d, v0.2d, v1.2d")
V3(fmls_4s, "fmls v2.4s, v0.4s, v1.4s") V3(fmls_2d, "fmls v2.2d, v0.2d, v1.2d")
VU(fsqrt) VU(frecpe) VU(frsqrte) VU(fabs) VU(fneg)
VU(frintx) VU(frinta) VU(frintz) VU(frinti) VU(frintm) VU(frintn) VU(frintp) VU(frint32z) VU(frint64x)
VU(fcvtzs) VU(fcvtzu) VU(fcvtns) VU(fcvtnu) VU(fcvtas) VU(fcvtau) VU(fcvtms) VU(fcvtmu) VU(fcvtps) VU(fcvtpu)
VU(scvtf) VU(ucvtf)
V1(fcmeq0_4s, "fcmeq v2.4s, v0.4s, #0.0") V1(fcmeq0_2d, "fcmeq v2.2d, v0.2d, #0.0")
V1(fcvtzs_fx_4s, "fcvtzs v2.4s, v0.4s, #7") V1(fcvtzs_fx_2d, "fcvtzs v2.2d, v0.2d, #20")
V1(scvtf_fx_4s, "scvtf v2.4s, v0.4s, #7") V1(scvtf_fx_2d, "scvtf v2.2d, v0.2d, #20")
V1(ucvtf_fx_4s, "ucvtf v2.4s, v0.4s, #7") V1(ucvtf_fx_2d, "ucvtf v2.2d, v0.2d, #20")
V1(fcvtzu_fx_4s, "fcvtzu v2.4s, v0.4s, #7") V1(fcvtzu_fx_2d, "fcvtzu v2.2d, v0.2d, #20")
/* scalar-SIMD fixed-point converts (#469): one lane of v0, the rest of v2 zeroed */
V1(scvtf_fx_ss, "scvtf s2, s0, #7") V1(scvtf_fx_sd, "scvtf d2, d0, #20") V1(ucvtf_fx_ss, "ucvtf s2, s0, #7")
V1(ucvtf_fx_sd, "ucvtf d2, d0, #20") V1(fcvtzs_fx_ss, "fcvtzs s2, s0, #7") V1(fcvtzs_fx_sd, "fcvtzs d2, d0, #20")
V1(fcvtzu_fx_ss, "fcvtzu s2, s0, #7") V1(fcvtzu_fx_sd, "fcvtzu d2, d0, #20")
/* narrowing / widening converts: fcvtn/fcvtxn read two doubles, fcvtl two singles (low half; fcvtl2 the high half) */
V1(fcvtn_2d, "fcvtn v2.2s, v0.2d") V1(fcvtn2_2d, "movi v2.2d, #0\n fcvtn2 v2.4s, v0.2d")
V1(fcvtxn_2d, "fcvtxn v2.2s, v0.2d") V1(fcvtxn2_2d, "movi v2.2d, #0\n fcvtxn2 v2.4s, v0.2d")
V1(fcvtl_2s, "fcvtl v2.2d, v0.2s") V1(fcvtl2_4s, "fcvtl2 v2.2d, v0.4s")
/* across-lanes reductions (result in lane 0, rest zero) */
V1(fmaxv_4s, "fmaxv s2, v0.4s") V1(fminv_4s, "fminv s2, v0.4s") V1(fmaxnmv_4s, "fmaxnmv s2, v0.4s") V1(fminnmv_4s, "fminnmv s2, v0.4s")

typedef u64 (*opfn)(u64, u64, u64, u64, u64 *);
struct op { const char *name; opfn fn; int arity; int is_s; vopfn vfn; };
/* arity: 2 = two operands, 1 = one, 3 = fmadd/fmla (c rotated from the table). is_s: operands are 32-bit.
 * vfn: non-null for a vector op (fn is then null); is_s then means the lanes come from the 32-bit table. */
static const struct op OPS[] = {
    {"fadd.d", fadd_d, 2, 0}, {"fsub.d", fsub_d, 2, 0}, {"fmul.d", fmul_d, 2, 0}, {"fdiv.d", fdiv_d, 2, 0},
    {"fmax.d", fmax_d, 2, 0}, {"fmin.d", fmin_d, 2, 0}, {"fmaxnm.d", fmaxnm_d, 2, 0}, {"fminnm.d", fminnm_d, 2, 0},
    {"fnmul.d", fnmul_d, 2, 0}, {"fabd.d", fabd_d, 2, 0}, {"frecps.d", frecps_d, 2, 0}, {"frsqrts.d", frsqrts_d, 2, 0},
    {"fcmpe.d", fcmp_d, 2, 0}, {"fmadd.d", fmadd_d, 3, 0},
    {"fsqrt.d", fsqrt_d, 1, 0}, {"frintx.d", frintx_d, 1, 0}, {"frinta.d", frinta_d, 1, 0}, {"frintz.d", frintz_d, 1, 0},
    {"frinti.d", frinti_d, 1, 0}, {"frintm.d", frintm_d, 1, 0}, {"frecpe.d", frecpe_d, 1, 0}, {"frsqrte.d", frsqrte_d, 1, 0},
    {"fneg.d", fneg_d, 1, 0}, {"fabs.d", fabs_d, 1, 0}, {"frint32z.d", frint32z_d, 1, 0}, {"frint64x.d", frint64x_d, 1, 0},
    {"fcvtzs.dx", fcvtzs_dx, 1, 0}, {"fcvtzu.dx", fcvtzu_dx, 1, 0}, {"fcvtas.dx", fcvtas_dx, 1, 0},
    {"fcvtms.dx", fcvtms_dx, 1, 0}, {"fcvtpu.dx", fcvtpu_dx, 1, 0}, {"fcvtzs.dw", fcvtzs_dw, 1, 0},
    {"fcvt.ds", fcvt_ds, 1, 0}, {"scvtf.xd", scvtf_xd, 1, 0}, {"ucvtf.xd", ucvtf_xd, 1, 0},
    {"fadd.s", fadd_s, 2, 1}, {"fsub.s", fsub_s, 2, 1}, {"fmul.s", fmul_s, 2, 1}, {"fdiv.s", fdiv_s, 2, 1},
    {"fmax.s", fmax_s, 2, 1}, {"fmin.s", fmin_s, 2, 1},
    {"fsqrt.s", fsqrt_s, 1, 1}, {"frintx.s", frintx_s, 1, 1}, {"frinta.s", frinta_s, 1, 1},
    {"frecpe.s", frecpe_s, 1, 1}, {"frsqrte.s", frsqrte_s, 1, 1}, {"fcvt.sd", fcvt_sd, 1, 1}, {"fcvtzs.sw", fcvtzs_sw, 1, 1},
    /* #488: remaining scalar FCVT* forms */
    {"fcvtns.dx", fcvtns_dx, 1, 0}, {"fcvtns.dw", fcvtns_dw, 1, 0}, {"fcvtns.sx", fcvtns_sx, 1, 1},
    {"fcvtns.sw", fcvtns_sw, 1, 1}, {"fcvtnu.dx", fcvtnu_dx, 1, 0}, {"fcvtnu.dw", fcvtnu_dw, 1, 0},
    {"fcvtnu.sx", fcvtnu_sx, 1, 1}, {"fcvtnu.sw", fcvtnu_sw, 1, 1}, {"fcvtau.dx", fcvtau_dx, 1, 0},
    {"fcvtau.dw", fcvtau_dw, 1, 0}, {"fcvtau.sx", fcvtau_sx, 1, 1}, {"fcvtau.sw", fcvtau_sw, 1, 1},
    {"fcvtmu.dx", fcvtmu_dx, 1, 0}, {"fcvtmu.dw", fcvtmu_dw, 1, 0}, {"fcvtmu.sx", fcvtmu_sx, 1, 1},
    {"fcvtmu.sw", fcvtmu_sw, 1, 1}, {"fcvtps.dx", fcvtps_dx, 1, 0}, {"fcvtps.dw", fcvtps_dw, 1, 0},
    {"fcvtps.sx", fcvtps_sx, 1, 1}, {"fcvtps.sw", fcvtps_sw, 1, 1}, {"fcvtzs.sx", fcvtzs_sx, 1, 1},
    {"fcvtzu.dw", fcvtzu_dw, 1, 0}, {"fcvtzu.sx", fcvtzu_sx, 1, 1}, {"fcvtzu.sw", fcvtzu_sw, 1, 1},
    {"fcvtas.dw", fcvtas_dw, 1, 0}, {"fcvtas.sx", fcvtas_sx, 1, 1}, {"fcvtas.sw", fcvtas_sw, 1, 1},
    {"fcvtms.dw", fcvtms_dw, 1, 0}, {"fcvtms.sx", fcvtms_sx, 1, 1}, {"fcvtms.sw", fcvtms_sw, 1, 1},
    {"fcvtpu.dw", fcvtpu_dw, 1, 0}, {"fcvtpu.sx", fcvtpu_sx, 1, 1}, {"fcvtpu.sw", fcvtpu_sw, 1, 1},
    {"fcvtzs_fx.dx", fcvtzs_fx_dx, 1, 0}, {"fcvtzu_fx.dw", fcvtzu_fx_dw, 1, 0}, {"fcvtzs_fx.sx", fcvtzs_fx_sx, 1, 1},
    {"fcvtzu_fx.sw", fcvtzu_fx_sw, 1, 1}, {"fjcvtzs.dw", fjcvtzs_dw, 1, 0}, {"fcvtxn.ds", fcvtxn_ds, 1, 0},
    /* #488: vector ops (arity/is_s as above; lanes described at the vector section) */
    {"fadd.4s", 0, 2, 1, fadd_4s}, {"fadd.2d", 0, 2, 0, fadd_2d}, {"fsub.4s", 0, 2, 1, fsub_4s},
    {"fsub.2d", 0, 2, 0, fsub_2d}, {"fmul.4s", 0, 2, 1, fmul_4s}, {"fmul.2d", 0, 2, 0, fmul_2d},
    {"fdiv.4s", 0, 2, 1, fdiv_4s}, {"fdiv.2d", 0, 2, 0, fdiv_2d}, {"fmulx.4s", 0, 2, 1, fmulx_4s},
    {"fmulx.2d", 0, 2, 0, fmulx_2d}, {"fmax.4s", 0, 2, 1, fmax_4s}, {"fmax.2d", 0, 2, 0, fmax_2d},
    {"fmin.4s", 0, 2, 1, fmin_4s}, {"fmin.2d", 0, 2, 0, fmin_2d}, {"fmaxnm.4s", 0, 2, 1, fmaxnm_4s},
    {"fmaxnm.2d", 0, 2, 0, fmaxnm_2d}, {"fminnm.4s", 0, 2, 1, fminnm_4s}, {"fminnm.2d", 0, 2, 0, fminnm_2d},
    {"fabd.4s", 0, 2, 1, fabd_4s}, {"fabd.2d", 0, 2, 0, fabd_2d}, {"frecps.4s", 0, 2, 1, frecps_4s},
    {"frecps.2d", 0, 2, 0, frecps_2d}, {"frsqrts.4s", 0, 2, 1, frsqrts_4s}, {"frsqrts.2d", 0, 2, 0, frsqrts_2d},
    {"faddp.4s", 0, 2, 1, faddp_4s}, {"faddp.2d", 0, 2, 0, faddp_2d}, {"fmaxp.4s", 0, 2, 1, fmaxp_4s},
    {"fmaxp.2d", 0, 2, 0, fmaxp_2d}, {"fminp.4s", 0, 2, 1, fminp_4s}, {"fminp.2d", 0, 2, 0, fminp_2d},
    {"fcmeq.4s", 0, 2, 1, fcmeq_4s}, {"fcmeq.2d", 0, 2, 0, fcmeq_2d}, {"fcmge.4s", 0, 2, 1, fcmge_4s},
    {"fcmge.2d", 0, 2, 0, fcmge_2d}, {"fcmgt.4s", 0, 2, 1, fcmgt_4s}, {"fcmgt.2d", 0, 2, 0, fcmgt_2d},
    {"facge.4s", 0, 2, 1, facge_4s}, {"facge.2d", 0, 2, 0, facge_2d}, {"facgt.4s", 0, 2, 1, facgt_4s},
    {"facgt.2d", 0, 2, 0, facgt_2d}, {"fmla.4s", 0, 3, 1, fmla_4s}, {"fmla.2d", 0, 3, 0, fmla_2d},
    {"fmls.4s", 0, 3, 1, fmls_4s}, {"fmls.2d", 0, 3, 0, fmls_2d}, {"fsqrt.4s", 0, 1, 1, fsqrt_4s},
    {"fsqrt.2d", 0, 1, 0, fsqrt_2d}, {"frecpe.4s", 0, 1, 1, frecpe_4s}, {"frecpe.2d", 0, 1, 0, frecpe_2d},
    {"frsqrte.4s", 0, 1, 1, frsqrte_4s}, {"frsqrte.2d", 0, 1, 0, frsqrte_2d}, {"fabs.4s", 0, 1, 1, fabs_4s},
    {"fabs.2d", 0, 1, 0, fabs_2d}, {"fneg.4s", 0, 1, 1, fneg_4s}, {"fneg.2d", 0, 1, 0, fneg_2d},
    {"frintx.4s", 0, 1, 1, frintx_4s}, {"frintx.2d", 0, 1, 0, frintx_2d}, {"frinta.4s", 0, 1, 1, frinta_4s},
    {"frinta.2d", 0, 1, 0, frinta_2d}, {"frintz.4s", 0, 1, 1, frintz_4s}, {"frintz.2d", 0, 1, 0, frintz_2d},
    {"frinti.4s", 0, 1, 1, frinti_4s}, {"frinti.2d", 0, 1, 0, frinti_2d}, {"frintm.4s", 0, 1, 1, frintm_4s},
    {"frintm.2d", 0, 1, 0, frintm_2d}, {"frintn.4s", 0, 1, 1, frintn_4s}, {"frintn.2d", 0, 1, 0, frintn_2d},
    {"frintp.4s", 0, 1, 1, frintp_4s}, {"frintp.2d", 0, 1, 0, frintp_2d}, {"frint32z.4s", 0, 1, 1, frint32z_4s},
    {"frint32z.2d", 0, 1, 0, frint32z_2d}, {"frint64x.4s", 0, 1, 1, frint64x_4s},
    {"frint64x.2d", 0, 1, 0, frint64x_2d}, {"fcvtzs.4s", 0, 1, 1, fcvtzs_4s}, {"fcvtzs.2d", 0, 1, 0, fcvtzs_2d},
    {"fcvtzu.4s", 0, 1, 1, fcvtzu_4s}, {"fcvtzu.2d", 0, 1, 0, fcvtzu_2d}, {"fcvtns.4s", 0, 1, 1, fcvtns_4s},
    {"fcvtns.2d", 0, 1, 0, fcvtns_2d}, {"fcvtnu.4s", 0, 1, 1, fcvtnu_4s}, {"fcvtnu.2d", 0, 1, 0, fcvtnu_2d},
    {"fcvtas.4s", 0, 1, 1, fcvtas_4s}, {"fcvtas.2d", 0, 1, 0, fcvtas_2d}, {"fcvtau.4s", 0, 1, 1, fcvtau_4s},
    {"fcvtau.2d", 0, 1, 0, fcvtau_2d}, {"fcvtms.4s", 0, 1, 1, fcvtms_4s}, {"fcvtms.2d", 0, 1, 0, fcvtms_2d},
    {"fcvtmu.4s", 0, 1, 1, fcvtmu_4s}, {"fcvtmu.2d", 0, 1, 0, fcvtmu_2d}, {"fcvtps.4s", 0, 1, 1, fcvtps_4s},
    {"fcvtps.2d", 0, 1, 0, fcvtps_2d}, {"fcvtpu.4s", 0, 1, 1, fcvtpu_4s}, {"fcvtpu.2d", 0, 1, 0, fcvtpu_2d},
    {"scvtf.4s", 0, 1, 1, scvtf_4s}, {"scvtf.2d", 0, 1, 0, scvtf_2d}, {"ucvtf.4s", 0, 1, 1, ucvtf_4s},
    {"ucvtf.2d", 0, 1, 0, ucvtf_2d}, {"fcmeq0.4s", 0, 1, 1, fcmeq0_4s}, {"fcmeq0.2d", 0, 1, 0, fcmeq0_2d},
    {"fcvtzs_fx.4s", 0, 1, 1, fcvtzs_fx_4s}, {"fcvtzs_fx.2d", 0, 1, 0, fcvtzs_fx_2d},
    {"scvtf_fx.4s", 0, 1, 1, scvtf_fx_4s}, {"scvtf_fx.2d", 0, 1, 0, scvtf_fx_2d}, {"fcvtn.2d", 0, 1, 0, fcvtn_2d},
    {"ucvtf_fx.4s", 0, 1, 1, ucvtf_fx_4s}, {"ucvtf_fx.2d", 0, 1, 0, ucvtf_fx_2d}, {"fcvtzu_fx.4s", 0, 1, 1, fcvtzu_fx_4s},
    {"fcvtzu_fx.2d", 0, 1, 0, fcvtzu_fx_2d}, {"scvtf_fx.ss", 0, 1, 1, scvtf_fx_ss}, {"scvtf_fx.sd", 0, 1, 0, scvtf_fx_sd},
    {"ucvtf_fx.ss", 0, 1, 1, ucvtf_fx_ss}, {"ucvtf_fx.sd", 0, 1, 0, ucvtf_fx_sd}, {"fcvtzs_fx.ss", 0, 1, 1, fcvtzs_fx_ss},
    {"fcvtzs_fx.sd", 0, 1, 0, fcvtzs_fx_sd}, {"fcvtzu_fx.ss", 0, 1, 1, fcvtzu_fx_ss}, {"fcvtzu_fx.sd", 0, 1, 0, fcvtzu_fx_sd},
    {"fcvtn2.2d", 0, 1, 0, fcvtn2_2d}, {"fcvtxn.2d", 0, 1, 0, fcvtxn_2d}, {"fcvtxn2.2d", 0, 1, 0, fcvtxn2_2d},
    {"fcvtl.2s", 0, 1, 1, fcvtl_2s}, {"fcvtl2.4s", 0, 1, 1, fcvtl2_4s}, {"fmaxv.4s", 0, 1, 1, fmaxv_4s},
    {"fminv.4s", 0, 1, 1, fminv_4s}, {"fmaxnmv.4s", 0, 1, 1, fmaxnmv_4s}, {"fminnmv.4s", 0, 1, 1, fminnmv_4s},
};
#define NOPS ((int)(sizeof OPS / sizeof OPS[0]))

/* Hex digits of the result: compare -> NZCV nibble, 32-bit results (.dw .ds .sw and single-precision ops), else 64-bit. */
static int result_width(const struct op *o) {
    const char *n = o->name;
    while (*n && *n != '.') n++;
    if (n[0] == '.' && n[1] == 'd' && n[2] == 0) return o->name[1] == 'c' && o->name[2] == 'm' ? 1 : 16;
    if (!strcmp_(n, ".dx") || !strcmp_(n, ".sx") || !strcmp_(n, ".sd") || !strcmp_(n, ".xd")) return 16;
    return 8; /* .s .dw .ds .sw (and the _fx variants of those) */
}

static void emit(const struct op *o, u64 fpcr, u64 a, u64 b, u64 c) {
    u64 fpsr = 0;
    u64 r = o->fn(a, b, c, fpcr, &fpsr);
    int w = o->is_s ? 8 : 16;
    puts_(o->name); putc_(' '); hex(fpcr >> 22, 2); putc_(' ');
    hex(a, w);
    if (o->arity >= 2) { putc_(' '); hex(b, w); }
    if (o->arity == 3) { putc_(' '); hex(c, w); }
    putc_(' '); putc_('='); putc_(' ');
    hex(r, result_width(o));
    putc_(' '); hex(fpsr & 0x9f, 2); putc_('\n');
}

static void hex128(const u64 *v) { hex(v[1], 16); hex(v[0], 16); }

/* Vector op: lanes are built from the table as described at the vector section; same line layout, 32-digit vectors. */
static void emit_v(const struct op *o, u64 fpcr, const u64 *a, const u64 *b, const u64 *c) {
    u64 fpsr = 0, r[2] = {0, 0};
    o->vfn(a, b, c, fpcr, &fpsr, r);
    puts_(o->name); putc_(' '); hex(fpcr >> 22, 2); putc_(' ');
    hex128(a);
    if (o->arity >= 2) { putc_(' '); hex128(b); }
    if (o->arity == 3) { putc_(' '); hex128(c); }
    putc_(' '); putc_('='); putc_(' ');
    hex128(r);
    putc_(' '); hex(fpsr & 0x9f, 2); putc_('\n');
}
static void lanes(const struct op *o, int base, u64 *v) {
    int n = o->is_s ? NS : ND;
    v[0] = v[1] = 0;
    if (o->is_s) { for (int k = 0; k < 4; k++) v[k >> 1] |= (u64)S[(base + k) % n] << (32 * (k & 1)); }
    else { for (int k = 0; k < 2; k++) v[k] = D[(base + k) % n]; }
}

/* argv[1] selects one op by name; "--list" prints the op names; no argument runs everything. */
static int run(int argc, char **argv) {
    const char *only = argc > 1 ? argv[1] : 0;
    if (only && !strcmp_(only, "--list")) {
        for (int oi = 0; oi < NOPS; oi++) { puts_(OPS[oi].name); putc_('\n'); }
        return 0;
    }
    for (int oi = 0; oi < NOPS; oi++) {
        const struct op *o = &OPS[oi];
        if (only && strcmp_(only, o->name)) continue;
        int n = o->is_s ? NS : ND;
        if (o->vfn) {
            for (int f = 0; f < NF; f++)
                for (int i = 0; i < n; i++) {
                    u64 va[2], vb[2] = {0, 0}, vc[2] = {0, 0};
                    lanes(o, i, va);
                    if (o->arity == 1) { emit_v(o, FPCRS[f], va, vb, vc); continue; }
                    for (int j = 0; j < n; j++) {
                        lanes(o, j, vb);
                        if (o->arity == 3) lanes(o, i + j, vc);
                        emit_v(o, FPCRS[f], va, vb, vc);
                    }
                }
            continue;
        }
        for (int f = 0; f < NF; f++) {
            for (int i = 0; i < n; i++) {
                u64 a = o->is_s ? S[i] : D[i];
                if (o->arity == 1) { emit(o, FPCRS[f], a, 0, 0); continue; }
                for (int j = 0; j < n; j++) {
                    u64 b = o->is_s ? S[j] : D[j];
                    if (o->arity == 3) {
                        /* 3-operand fmadd: c rotates through the table, keeping the run bounded */
                        emit(o, FPCRS[f], a, b, D[(i + j) % n]);
                    } else {
                        emit(o, FPCRS[f], a, b, 0);
                    }
                }
            }
        }
    }
    return 0;
}

#ifdef FREESTANDING
/* Linux entry: sp -> argc, argv[0..], NULL. */
__asm__(".globl _start\n_start:\n mov x0, sp\n bl start_c\n");
void start_c(u64 *sp) {
    run((int)sp[0], (char **)(sp + 1));
    flush();
    sys3(94, 0, 0, 0);
    for (;;) {}
}
#else
int main(int argc, char **argv) { run(argc, argv); flush(); return 0; }
#endif
