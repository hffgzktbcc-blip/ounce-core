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

typedef u64 (*opfn)(u64, u64, u64, u64, u64 *);
struct op { const char *name; opfn fn; int arity; int is_s; };
/* arity: 2 = two operands, 1 = one, 3 = fmadd (c rotated from the table). is_s: operands are 32-bit. */
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
};
#define NOPS ((int)(sizeof OPS / sizeof OPS[0]))

/* Hex digits of the result: compare -> NZCV nibble, 32-bit results (.dw .ds .sw and single-precision ops), else 64-bit. */
static int result_width(const struct op *o) {
    const char *n = o->name;
    while (*n && *n != '.') n++;
    if (n[0] == '.' && n[1] == 'd' && n[2] == 0) return o->name[1] == 'c' && o->name[2] == 'm' ? 1 : 16;
    if (!strcmp_(n, ".dx") || !strcmp_(n, ".sd") || !strcmp_(n, ".xd")) return 16;
    return 8; /* .s .dw .ds .sw */
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
