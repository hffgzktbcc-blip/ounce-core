// Host side of the single-instruction fuzzer: runs random A64 data-processing (scalar FP + Advanced
// SIMD) words with random register inputs on real AArch64 hardware (Apple Silicon macOS or Linux; see
// README.md) and prints a text trace that
// `ounce-core fuzz-replay` replays in the interpreter. See README.md.
//
//   cc -O1 -o fuzz_host fuzz_host.c && ./fuzz_host --seed 1 --count 150000 | ounce-core fuzz-replay -
//
// Trace lines (hex, space separated):
//   U <word>                                   host raised SIGILL (UNDEFINED encoding)
//   T <word> <nzcv_in> <31 x> <32 v128> => <nzcv_out> <fpsr_out> <31 x> <32 v128>
#include <setjmp.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#ifdef __APPLE__
#include <pthread.h>
#include <libkern/OSCacheControl.h>
#define SYM "_"          /* Mach-O prefixes C symbols with an underscore */
#define JIT_WRITE(on) pthread_jit_write_protect_np(!(on))
#define JIT_FLAGS MAP_JIT
#define ICACHE(p, n) sys_icache_invalidate((p), (n))
#else  /* Linux aarch64: an anonymous RWX mapping is enough, no MAP_JIT or write-protect toggle */
#define SYM ""
#define JIT_WRITE(on) ((void)(on))
#define JIT_FLAGS 0
#define ICACHE(p, n) __builtin___clear_cache((char *)(p), (char *)(p) + (n))
#endif

// Context layout (bytes): in.x @0, in.nzcv @248, in.v @256, out.x @768, out.nzcv @1016, out.fpsr @1024,
// out.v @1040.
typedef struct {
    uint64_t x[31]; uint64_t nzcv; uint64_t v[64];
    uint64_t ox[31]; uint64_t onzcv; uint64_t ofpsr; uint64_t fpcr; uint64_t ov[64];
} __attribute__((aligned(16))) ctx_t;
_Static_assert(__builtin_offsetof(ctx_t, nzcv) == 248, "nzcv");
_Static_assert(__builtin_offsetof(ctx_t, v) == 256, "v");
_Static_assert(__builtin_offsetof(ctx_t, ox) == 768, "ox");
_Static_assert(__builtin_offsetof(ctx_t, onzcv) == 1016, "onzcv");
_Static_assert(__builtin_offsetof(ctx_t, fpcr) == 1032, "fpcr");
_Static_assert(__builtin_offsetof(ctx_t, ov) == 1040, "ov");

// Position-independent template copied into the JIT page: loads every register from the context
// (x0), runs the test word at tmpl_word, stores every register back. FPCR is left at the host default (0).
void tmpl_start(void);
void tmpl_word(void);
void tmpl_end(void);
__asm__(
    ".text\n.p2align 4\n"
    ".globl " SYM "tmpl_start\n" SYM "tmpl_start:\n"
    "sub sp, sp, #176\n"
    "stp x19, x20, [sp, #0]\nstp x21, x22, [sp, #16]\nstp x23, x24, [sp, #32]\nstp x25, x26, [sp, #48]\n"
    "stp x27, x28, [sp, #64]\nstp x29, x30, [sp, #80]\n"
    "stp d8, d9, [sp, #96]\nstp d10, d11, [sp, #112]\nstp d12, d13, [sp, #128]\nstp d14, d15, [sp, #144]\n"
    "str x0, [sp, #160]\n"
    "mov x16, x0\n"
    "ldp q0, q1, [x16, #256]\nldp q2, q3, [x16, #288]\nldp q4, q5, [x16, #320]\nldp q6, q7, [x16, #352]\n"
    "ldp q8, q9, [x16, #384]\nldp q10, q11, [x16, #416]\nldp q12, q13, [x16, #448]\nldp q14, q15, [x16, #480]\n"
    "ldp q16, q17, [x16, #512]\nldp q18, q19, [x16, #544]\nldp q20, q21, [x16, #576]\nldp q22, q23, [x16, #608]\n"
    "ldp q24, q25, [x16, #640]\nldp q26, q27, [x16, #672]\nldp q28, q29, [x16, #704]\nldp q30, q31, [x16, #736]\n"
    "ldr x17, [x16, #1032]\nmsr fpcr, x17\nldr x17, [x16, #248]\nmsr nzcv, x17\nmsr fpsr, xzr\n"
    "ldp x0, x1, [x16, #0]\nldp x2, x3, [x16, #16]\nldp x4, x5, [x16, #32]\nldp x6, x7, [x16, #48]\n"
    "ldp x8, x9, [x16, #64]\nldp x10, x11, [x16, #80]\nldp x12, x13, [x16, #96]\nldp x14, x15, [x16, #112]\n"
    "ldp x18, x19, [x16, #144]\nldp x20, x21, [x16, #160]\nldp x22, x23, [x16, #176]\nldp x24, x25, [x16, #192]\n"
    "ldp x26, x27, [x16, #208]\nldp x28, x29, [x16, #224]\nldr x30, [x16, #240]\n"
    "ldr x17, [x16, #136]\nldr x16, [x16, #128]\n"
    ".globl " SYM "tmpl_word\n" SYM "tmpl_word:\nnop\n"
    "stp x0, x1, [sp, #-16]!\n"
    "ldr x0, [sp, #176]\n"
    "add x0, x0, #768\n"
    "stp x2, x3, [x0, #16]\nstp x4, x5, [x0, #32]\nstp x6, x7, [x0, #48]\nstp x8, x9, [x0, #64]\n"
    "stp x10, x11, [x0, #80]\nstp x12, x13, [x0, #96]\nstp x14, x15, [x0, #112]\nstp x16, x17, [x0, #128]\n"
    "stp x18, x19, [x0, #144]\nstp x20, x21, [x0, #160]\nstp x22, x23, [x0, #176]\nstp x24, x25, [x0, #192]\n"
    "stp x26, x27, [x0, #208]\nstp x28, x29, [x0, #224]\nstr x30, [x0, #240]\n"
    "mrs x2, nzcv\nstr x2, [x0, #248]\nmrs x2, fpsr\nstr x2, [x0, #256]\nmsr fpcr, xzr\n"
    "stp q0, q1, [x0, #272]\nstp q2, q3, [x0, #304]\nstp q4, q5, [x0, #336]\nstp q6, q7, [x0, #368]\n"
    "stp q8, q9, [x0, #400]\nstp q10, q11, [x0, #432]\nstp q12, q13, [x0, #464]\nstp q14, q15, [x0, #496]\n"
    "stp q16, q17, [x0, #528]\nstp q18, q19, [x0, #560]\nstp q20, q21, [x0, #592]\nstp q22, q23, [x0, #624]\n"
    "stp q24, q25, [x0, #656]\nstp q26, q27, [x0, #688]\nstp q28, q29, [x0, #720]\nstp q30, q31, [x0, #752]\n"
    "ldp x2, x3, [sp], #16\nstp x2, x3, [x0, #0]\n"
    "ldp x19, x20, [sp, #0]\nldp x21, x22, [sp, #16]\nldp x23, x24, [sp, #32]\nldp x25, x26, [sp, #48]\n"
    "ldp x27, x28, [sp, #64]\nldp x29, x30, [sp, #80]\n"
    "ldp d8, d9, [sp, #96]\nldp d10, d11, [sp, #112]\nldp d12, d13, [sp, #128]\nldp d14, d15, [sp, #144]\n"
    "add sp, sp, #176\nret\n"
    ".globl " SYM "tmpl_end\n" SYM "tmpl_end:\nnop\n");

static sigjmp_buf jb;
static void on_sigill(int s) { (void)s; __asm__ volatile("msr fpcr, xzr"); siglongjmp(jb, 1); }

// xoshiro256** seeded by splitmix64
static uint64_t st[4];
static uint64_t rotl(uint64_t x, int k) { return (x << k) | (x >> (64 - k)); }
static uint64_t rnd(void) {
    uint64_t r = rotl(st[1] * 5, 7) * 9, t = st[1] << 17;
    st[2] ^= st[0]; st[3] ^= st[1]; st[1] ^= st[2]; st[0] ^= st[3]; st[2] ^= t; st[3] = rotl(st[3], 45);
    return r;
}
static void seed(uint64_t s) {
    for (int i = 0; i < 4; i++) {
        uint64_t z = (s += 0x9E3779B97F4A7C15ull);
        z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull; z = (z ^ (z >> 27)) * 0x94D049BB133111EBull;
        st[i] = z ^ (z >> 31);
    }
}

static const uint32_t F32[] = {0, 0x80000000, 0x3f800000, 0xbf800000, 0x40000000, 0x3f000000, 0x7f800000, 0xff800000,
    0x7fc00000, 0xffc00000, 0x7f800001, 0xffa00000, 0x00000001, 0x007fffff, 0x00800000, 0x80000001, 0x807fffff,
    0x7f7fffff, 0xff7fffff, 0x4b000000, 0x4b800000, 0xcf000000, 0x4f000000, 0x4effffff, 0x3effffff, 0x3f7fffff,
    0x3fc00000, 0x40490fdb, 0x33800000, 0x5f000000};
static const uint64_t F64[] = {0, 0x8000000000000000ull, 0x3ff0000000000000ull, 0xbff0000000000000ull,
    0x4000000000000000ull, 0x3fe0000000000000ull, 0x7ff0000000000000ull, 0xfff0000000000000ull,
    0x7ff8000000000000ull, 0xfff8000000000000ull, 0x7ff0000000000001ull, 0xfff4000000000000ull, 1,
    0x000fffffffffffffull, 0x0010000000000000ull, 0x8000000000000001ull, 0x7fefffffffffffffull,
    0xffefffffffffffffull, 0x4330000000000000ull, 0x43e0000000000000ull, 0xc3e0000000000000ull,
    0x41dfffffffc00000ull, 0x3ff8000000000000ull, 0x3fefffffffffffffull, 0x43dfffffffffffffull};
static const uint16_t F16[] = {0, 0x8000, 0x3c00, 0xbc00, 0x4000, 0x3800, 0x7c00, 0xfc00, 0x7e00, 0xfe00, 0x7c01,
    0xfd00, 1, 0x03ff, 0x0400, 0x8001, 0x7bff, 0xfbff, 0x6400, 0x7800};
#define N(a) (sizeof(a) / sizeof((a)[0]))

static uint64_t rand_v64(int mode) {
    switch (mode) {
    case 0: return rnd();
    case 1: return (uint64_t)F32[rnd() % N(F32)] | (uint64_t)F32[rnd() % N(F32)] << 32;
    case 2: return F64[rnd() % N(F64)];
    case 3: { uint64_t r = 0; for (int i = 0; i < 4; i++) r |= (uint64_t)F16[rnd() % N(F16)] << (16 * i); return r; }
    case 4: { // small / edge integers per lane of random width
        int w = 8 << (rnd() & 3); uint64_t r = 0;
        for (int i = 0; i < 64; i += w) {
            uint64_t m = w == 64 ? ~0ull : (1ull << w) - 1, v;
            switch (rnd() % 6) { case 0: v = 0; break; case 1: v = m; break; case 2: v = 1; break;
                case 3: v = m >> 1; break; case 4: v = (m >> 1) + 1; break; default: v = rnd() & 7; }
            r |= (v & m) << i;
        }
        return r;
    }
    default: { // random with some FP-ish exponent structure in each 32-bit lane
        uint64_t r = rnd();
        return (r & 0x807fffff807fffffull) | ((uint64_t)(0x60 + rnd() % 64) << 23) | ((uint64_t)(0x60 + rnd() % 64) << 55);
    }
    }
}
static uint64_t rand_x(void) {
    switch (rnd() % 4) {
    case 0: return rnd();
    case 1: return rnd() & 0xff;
    case 2: { uint64_t e[] = {0, ~0ull, 1, 0x7fffffffull, 0x80000000ull, 0xffffffffull, 0x7fffffffffffffffull, 0x8000000000000000ull}; return e[rnd() & 7]; }
    default: return (uint64_t)(int64_t)(int32_t)rnd();
    }
}

// Words: AdvSIMD / scalar FP data processing is bits[27:25]==111 (loads/stores have bit25==0). Branches,
// system, loads/stores and integer data processing are all outside that mask. Purely random words in
// that space are mostly UNDEFINED, so each case picks a family and fixes its top-level encoding bits.
static uint32_t fixed(uint32_t w, uint32_t mask, uint32_t val) { return (w & ~mask) | val; }
static uint32_t gen_word(void) {
    uint32_t w = (uint32_t)rnd();
    switch (rnd() % 16) {
    case 0: case 1: case 2: return fixed(w, 0x7F000000, 0x1E000000 | ((w & 1) << 24)); // scalar FP (incl. fused, int<->fp)
    case 3: case 4: case 5: return fixed(w, 0x9F200400, 0x0E200400);   // three-same
    case 6: case 7: return fixed(w, 0x9F3E0C00, 0x0E200800);          // two-reg misc
    case 8: return fixed(w, 0x9F3E0C00, 0x0E300800);                  // across lanes
    case 9: return fixed(w, 0x9F200C00, 0x0E200000);                  // three different
    case 10: return fixed(w, 0x9F000400, 0x0F000000);                 // vector by element
    case 11: return fixed(w, 0x9F800400, 0x0F000400);                 // vector shift immediate
    case 12: return fixed(w, 0x9F000000, 0x5E000000 | (w & 0x20000000)); // scalar three-same/misc/pairwise
    case 13: return fixed(w, 0x9F000000, 0x5F000000 | (w & 0x20000000)); // scalar by element / shift imm
    case 14: return fixed(w, 0x9F200000, 0x0E000000);                 // permute / extract / copy / tbl
    default: return fixed(w, 0x9FF80400, 0x0F000400);                 // modified immediate
    }
}

// ---- shared execution of one case ------------------------------------------------------------------
static uint8_t *page; static size_t woff; static void (*fn)(ctx_t *);
static ctx_t c;
static uint64_t nu, nt;

// Runs word `w` under FPCR `fpcr` with the registers in `c`, prints one trace line (T, TF when fpcr != 0, or U).
static void run_case(uint32_t w, uint64_t fpcr) {
    JIT_WRITE(1);
    memcpy(page + woff, &w, 4);
    JIT_WRITE(0);
    ICACHE(page + woff, 4);
    c.fpcr = fpcr;
    if (sigsetjmp(jb, 1)) { printf("U %08x\n", w); nu++; return; }
    fn(&c);
    nt++;
    if (fpcr) printf("TF %llx %08x %llx", (unsigned long long)fpcr, w, (unsigned long long)(c.nzcv >> 28));
    else printf("T %08x %llx", w, (unsigned long long)(c.nzcv >> 28));
    for (int i = 0; i < 31; i++) printf(" %llx", (unsigned long long)c.x[i]);
    for (int i = 0; i < 32; i++) printf(" %016llx%016llx", (unsigned long long)c.v[2 * i + 1], (unsigned long long)c.v[2 * i]);
    printf(" => %llx %llx", (unsigned long long)(c.onzcv >> 28), (unsigned long long)c.ofpsr);
    for (int i = 0; i < 31; i++) printf(" %llx", (unsigned long long)c.ox[i]);
    for (int i = 0; i < 32; i++) printf(" %016llx%016llx", (unsigned long long)c.ov[2 * i + 1], (unsigned long long)c.ov[2 * i]);
    printf("\n");
}

// ---- deterministic floating-point edge-case corpus (--corpus edge) -----------------------------------
// One fixed enumeration (no PRNG): every word below x every FPCR mode x every operand combination from the
// value tables. Operand bit patterns are listed explicitly so the corpus is identical on every host and
// every run; `--list-words` prints the word table. Scope and exclusions: docs/verify/fp-edge/README.md.
//
// Logical value ids (same meaning in f16/f32/f64): 0 +0, 1 -0, 2 +1, 3 -1, 4 +inf, 5 -inf, 6 qNaN, 7 -qNaN with
// payload, 8 sNaN, 9 -sNaN, 10 min denormal, 11 max denormal, 12 min normal, 13 -min denormal, 14 max normal,
// 15 half an ulp of 1.0 (a round-to-even tie against 1.0), 16 3.0 (inexact quotients), 17 -max denormal.
// 18..27 ("wide", used only by one-operand words) are rounding/conversion probes.
#define NCORE 18
#define NWIDE 28
static const uint16_t E16[NWIDE] = {0, 0x8000, 0x3c00, 0xbc00, 0x7c00, 0xfc00, 0x7e00, 0xfe01, 0x7d00, 0xfd01, 1, 0x03ff,
    0x0400, 0x8001, 0x7bff, 0x1000, 0x4200, 0x83ff, 0x4100, 0xc100, 0x3800, 0xb800, 0x6800, 0xe800, 0xfbff, 0x3e00, 0x5648, 0x5bff};
static const uint32_t E32[NWIDE] = {0, 0x80000000, 0x3f800000, 0xbf800000, 0x7f800000, 0xff800000, 0x7fc00000, 0xffc00001,
    0x7f800001, 0xffa00000, 1, 0x007fffff, 0x00800000, 0x80000001, 0x7f7fffff, 0x33800000, 0x40400000, 0x807fffff,
    0x40200000, 0xc0200000, 0x3f000000, 0xbf000000, 0x4f000000, 0xcf000000, 0x4f800000, 0x5f000000, 0xdf000000, 0x3fc00000};
static const uint64_t E64[NWIDE] = {0, 0x8000000000000000ull, 0x3ff0000000000000ull, 0xbff0000000000000ull,
    0x7ff0000000000000ull, 0xfff0000000000000ull, 0x7ff8000000000000ull, 0xfff8000000000001ull, 0x7ff0000000000001ull,
    0xfff4000000000000ull, 1, 0x000fffffffffffffull, 0x0010000000000000ull, 0x8000000000000001ull, 0x7fefffffffffffffull,
    0x3ca0000000000000ull, 0x4008000000000000ull, 0x800fffffffffffffull, 0x4004000000000000ull, 0xc004000000000000ull,
    0x3fe0000000000000ull, 0xbfe0000000000000ull, 0x41e0000000000000ull, 0xc1e0000000000000ull, 0x41f0000000000000ull,
    0x43e0000000000000ull, 0xc3e0000000000000ull, 0x3ff8000000000000ull};
static const int FMAI[9] = {0, 1, 2, 3, 4, 6, 8, 10, 14};
static const uint64_t EINT[12] = {0, 1, ~0ull, 0x7fffffffull, 0x80000000ull, 0xffffffffull, 0x7fffffffffffffffull,
    0x8000000000000000ull, 0x1000001ull, 0x1000003ull, 0x20000000000001ull, 0x8000008000000001ull};
// FPCR: 0, FZ, DN, FZ|DN, RMode RP/RM/RZ, FZ16, and everything at once (FZ|DN|FZ16|RZ).
static const uint64_t EFPCR[9] = {0, 1u << 24, 1u << 25, 3u << 24, 1u << 22, 2u << 22, 3u << 22, 1u << 19,
    (1u << 24) | (1u << 25) | (1u << 19) | (3u << 22)};

static uint64_t ev(int esz, int i) { return esz == 16 ? E16[i] : esz == 32 ? E32[i] : E64[i]; }

typedef struct {
    const char *name; uint32_t word;
    int vec;      // 0 scalar (lane 0, upper bits zero), 1 = 64-bit vector, 2 = 128-bit vector
    int esz;      // source lane size in bits
    int nin;      // operand count looped over: 1 (V0), 2 (V0,V1), 3 (V0,V1,+accumulator)
    int accr;     // register receiving the third operand (2 = Ra of FMADD, 3 = destination accumulator of FMLA)
    int intsrc;   // 1: V0 lanes / X0 come from the integer table (SCVTF/UCVTF)
} Op;

static Op ops[512]; static int nops;
static void add(const char *name, uint32_t word, int vec, int esz, int nin, int accr, int intsrc) {
    ops[nops++] = (Op){name, word, vec, esz, nin, accr, intsrc};
}
#define RN 0
#define RM1 (1u << 16)
#define RA2 (2u << 10)
#define RD3 3u
static void build_ops(void) {
    static char names[1024][32]; int nn = 0;
    const char *tn[4] = {"S", "D", "?", "H"}; int tesz[4] = {32, 64, 0, 16};
    #define NM(...) (snprintf(names[nn], 32, __VA_ARGS__), names[nn++])
    // scalar two-source: FMUL FDIV FADD FSUB FMAX FMIN FMAXNM FMINNM FNMUL
    const char *two[9] = {"FMUL", "FDIV", "FADD", "FSUB", "FMAX", "FMIN", "FMAXNM", "FMINNM", "FNMUL"};
    for (int t = 0; t < 4; t++) { if (t == 2) continue;
        for (int o = 0; o < 9; o++)
            add(NM("%s %s", two[o], tn[t]), 0x1E200800u | (uint32_t)t << 22 | (uint32_t)o << 12 | RM1 | RN << 5 | RD3, 0, tesz[t], 2, 0, 0);
        // scalar FMADD FMSUB FNMADD FNMSUB
        const char *f3[4] = {"FMADD", "FMSUB", "FNMADD", "FNMSUB"};
        for (int o = 0; o < 4; o++)
            add(NM("%s %s", f3[o], tn[t]), 0x1F000000u | (uint32_t)t << 22 | (uint32_t)(o >> 1) << 21 | (uint32_t)(o & 1) << 15 | RM1 | RA2 | RN << 5 | RD3, 0, tesz[t], 3, 2, 0);
        // scalar one-source: FABS FNEG FSQRT FRINT{N,P,M,Z,A,X,I}
        const char *one[] = {"FABS", "FNEG", "FSQRT", "FRINTN", "FRINTP", "FRINTM", "FRINTZ", "FRINTA", "FRINTX", "FRINTI"};
        const int oc[] = {1, 2, 3, 8, 9, 10, 11, 12, 14, 15};
        for (int o = 0; o < 10; o++)
            add(NM("%s %s", one[o], tn[t]), 0x1E204000u | (uint32_t)t << 22 | (uint32_t)oc[o] << 15 | RN << 5 | RD3, 0, tesz[t], 1, 0, 0);
        // FCMP / FCMPE register and zero forms (result is NZCV)
        for (int o = 0; o < 4; o++)
            add(NM("%s%s %s", o & 2 ? "FCMPE" : "FCMP", o & 1 ? "0" : "", tn[t]), 0x1E202000u | (uint32_t)t << 22 | (o & 1 ? 0 : RM1) | RN << 5 | (uint32_t)o << 3, 0, tesz[t], o & 1 ? 1 : 2, 0, 0);
        // FP <-> integer: FCVT{N,P,M,Z,A}{S,U} (W,X dest) and SCVTF/UCVTF (W,X source)
        const char *cv[] = {"FCVTNS", "FCVTNU", "FCVTAS", "FCVTAU", "FCVTPS", "FCVTPU", "FCVTMS", "FCVTMU", "FCVTZS", "FCVTZU"};
        const int cr[] = {0, 0, 0, 0, 1, 1, 2, 2, 3, 3}, co[] = {0, 1, 4, 5, 0, 1, 0, 1, 0, 1};
        for (int sf = 0; sf < 2; sf++) {
            for (int o = 0; o < 10; o++)
                add(NM("%s %s>%c", cv[o], tn[t], sf ? 'X' : 'W'), 0x1E200000u | (uint32_t)sf << 31 | (uint32_t)t << 22 | (uint32_t)cr[o] << 19 | (uint32_t)co[o] << 16 | RN << 5 | RD3, 0, tesz[t], 1, 0, 0);
            for (int u = 0; u < 2; u++)
                add(NM("%s %c>%s", u ? "UCVTF" : "SCVTF", sf ? 'X' : 'W', tn[t]), 0x1E200000u | (uint32_t)sf << 31 | (uint32_t)t << 22 | (uint32_t)(2 + u) << 16 | RN << 5 | RD3, 0, 64, 1, 0, 1);
        }
    }
    // FCVT between precisions (S<->D, H<->S, H<->D)
    add("FCVT D>S", 0x1E624000u | RD3, 0, 64, 1, 0, 0);
    add("FCVT S>D", 0x1E22C000u | RD3, 0, 32, 1, 0, 0);
    add("FCVT S>H", 0x1E23C000u | RD3, 0, 32, 1, 0, 0);
    add("FCVT H>S", 0x1EE24000u | RD3, 0, 16, 1, 0, 0);
    add("FCVT D>H", 0x1E63C000u | RD3, 0, 64, 1, 0, 0);
    add("FCVT H>D", 0x1EE2C000u | RD3, 0, 16, 1, 0, 0);
    // scalar estimate/step/extreme words (S and D)
    for (int d = 0; d < 2; d++) { uint32_t sz = (uint32_t)d << 22; int e = d ? 64 : 32; const char *x = d ? "D" : "S";
        add(NM("FRECPE %s", x), 0x5EA1D800u | sz | RD3, 0, e, 1, 0, 0);
        add(NM("FRSQRTE %s", x), 0x7EA1D800u | sz | RD3, 0, e, 1, 0, 0);
        add(NM("FRECPX %s", x), 0x5EA1F800u | sz | RD3, 0, e, 1, 0, 0);
        add(NM("FRECPS %s", x), 0x5E20FC00u | sz | RM1 | RD3, 0, e, 2, 0, 0);
        add(NM("FRSQRTS %s", x), 0x5EA0FC00u | sz | RM1 | RD3, 0, e, 2, 0, 0);
        add(NM("FMULX %s", x), 0x5E20DC00u | sz | RM1 | RD3, 0, e, 2, 0, 0);
    }
    // vector three-same FP: .4S (Q=1,sz=0) and .2D (Q=1,sz=1)
    struct { const char *n; uint32_t w; int acc; } v3[] = {
        {"FMAXNM", 0x0E20C400, 0}, {"FMLA", 0x0E20CC00, 1}, {"FADD", 0x0E20D400, 0}, {"FMULX", 0x0E20DC00, 0},
        {"FCMEQ", 0x0E20E400, 0}, {"FMAX", 0x0E20F400, 0}, {"FRECPS", 0x0E20FC00, 0}, {"FMINNM", 0x0EA0C400, 0},
        {"FMLS", 0x0EA0CC00, 1}, {"FSUB", 0x0EA0D400, 0}, {"FMIN", 0x0EA0F400, 0}, {"FRSQRTS", 0x0EA0FC00, 0},
        {"FMAXNMP", 0x2E20C400, 0}, {"FADDP", 0x2E20D400, 0}, {"FMUL", 0x2E20DC00, 0}, {"FCMGE", 0x2E20E400, 0},
        {"FACGE", 0x2E20EC00, 0}, {"FMAXP", 0x2E20F400, 0}, {"FDIV", 0x2E20FC00, 0}, {"FMINNMP", 0x2EA0C400, 0},
        {"FABD", 0x2EA0D400, 0}, {"FCMGT", 0x2EA0E400, 0}, {"FACGT", 0x2EA0EC00, 0}, {"FMINP", 0x2EA0F400, 0}};
    for (int d = 0; d < 2; d++) for (unsigned k = 0; k < sizeof v3 / sizeof v3[0]; k++)
        add(NM("%s V.%s", v3[k].n, d ? "2D" : "4S"), v3[k].w | 0x40000000u | (uint32_t)d << 22 | RM1 | RN << 5 | RD3,
            2, d ? 64 : 32, v3[k].acc ? 3 : 2, v3[k].acc ? 3 : 0, 0);
    // vector two-register misc FP: .4S and .2D
    struct { const char *n; uint32_t w; int is; } v2[] = {
        {"FABS", 0x0EA0F800, 0}, {"FNEG", 0x2EA0F800, 0}, {"FSQRT", 0x2EA1F800, 0}, {"FRECPE", 0x0EA1D800, 0},
        {"FRSQRTE", 0x2EA1D800, 0}, {"FRINTN", 0x0E218800, 0}, {"FRINTM", 0x0E219800, 0}, {"FRINTP", 0x0EA18800, 0},
        {"FRINTZ", 0x0EA19800, 0}, {"FRINTA", 0x2E218800, 0}, {"FRINTX", 0x2E219800, 0}, {"FRINTI", 0x2EA19800, 0},
        {"FCVTNS", 0x0E21A800, 0}, {"FCVTMS", 0x0E21B800, 0}, {"FCVTAS", 0x0E21C800, 0}, {"FCVTPS", 0x0EA1A800, 0},
        {"FCVTZS", 0x0EA1B800, 0}, {"FCVTNU", 0x2E21A800, 0}, {"FCVTMU", 0x2E21B800, 0}, {"FCVTAU", 0x2E21C800, 0},
        {"FCVTPU", 0x2EA1A800, 0}, {"FCVTZU", 0x2EA1B800, 0}, {"SCVTF", 0x0E21D800, 1}, {"UCVTF", 0x2E21D800, 1}};
    for (int d = 0; d < 2; d++) for (unsigned k = 0; k < sizeof v2 / sizeof v2[0]; k++)
        add(NM("%s V.%s", v2[k].n, d ? "2D" : "4S"), v2[k].w | 0x40000000u | (uint32_t)d << 22 | RN << 5 | RD3,
            2, d ? 64 : 32, 1, 0, v2[k].is);
    // across lanes (.4S) and scalar pairwise
    add("FMAXV S.4S", 0x6E30F800u | RD3, 2, 32, 1, 0, 0); add("FMINV S.4S", 0x6EB0F800u | RD3, 2, 32, 1, 0, 0);
    add("FMAXNMV S.4S", 0x6E30C800u | RD3, 2, 32, 1, 0, 0); add("FMINNMV S.4S", 0x6EB0C800u | RD3, 2, 32, 1, 0, 0);
    add("FADDP S.2S", 0x7E30D800u | RD3, 1, 32, 1, 0, 0); add("FADDP D.2D", 0x7E70D800u | RD3, 2, 64, 1, 0, 0);
    add("FMAXP D.2D", 0x7E70F800u | RD3, 2, 64, 1, 0, 0); add("FMINP D.2D", 0x7EF0F800u | RD3, 2, 64, 1, 0, 0);
    add("FMAXNMP D.2D", 0x7E70C800u | RD3, 2, 64, 1, 0, 0); add("FMINNMP D.2D", 0x7EF0C800u | RD3, 2, 64, 1, 0, 0);
}

static void set_lane(int reg, int k, int esz, uint64_t bits) {
    uint8_t *b = (uint8_t *)&c.v[2 * reg];
    memcpy(b + k * (esz / 8), &bits, esz / 8);
}
// Fills register `reg` for operand index `idx`: scalar words use lane 0 only; vectors rotate the value table by
// lane so every lane sees a different value while lane 0 enumerates the full table.
static void fill(const Op *o, int reg, int idx, int n, int intsrc) {
    int lanes = o->vec == 0 ? 1 : (o->vec == 2 ? 128 : 64) / o->esz;
    for (int k = 0; k < lanes; k++) {
        int j = (idx + k) % n;
        uint64_t m = o->esz == 64 ? ~0ull : (1ull << o->esz) - 1;
        set_lane(reg, k, o->esz, intsrc ? EINT[j % 12] & m : ev(o->esz, j));
    }
}
static void edge_corpus(void) {
    for (int oi = 0; oi < nops; oi++) {
        const Op *o = &ops[oi];
        int na = o->nin == 1 ? NWIDE : o->nin == 2 ? NCORE : 9;
        int nb = o->nin >= 2 ? na : 1, nc = o->nin == 3 ? 9 : 1;
        if (o->intsrc) na = 12;
        for (int f = 0; f < 9; f++)
            for (int a = 0; a < na; a++) for (int b = 0; b < nb; b++) for (int cc = 0; cc < nc; cc++) {
                memset(&c, 0, sizeof c);
                int ia = o->nin == 3 ? FMAI[a] : a, ib = o->nin == 3 ? FMAI[b] : b, ic = o->nin == 3 ? FMAI[cc] : 0;
                if (o->intsrc) { c.x[0] = EINT[a]; fill(o, 0, a, 12, 1); }
                else fill(o, 0, ia, 28, 0);
                if (o->nin >= 2) fill(o, 1, ib, 28, 0);
                // The destination starts as an unrelated value so "result not written" shows up.
                fill(o, 3, (ia + 2 * ib + 5) % NCORE, 28, 0);
                if (o->nin == 3) fill(o, o->accr, ic, 28, 0);
                run_case(o->word, EFPCR[f]);
            }
    }
}

int main(int argc, char **argv) {
    uint64_t sd = 1, count = 1000;
    int default_nan_case = 0, edge = 0, list_words = 0;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--list-words")) { list_words = 1; continue; }
        if (i + 1 >= argc) { fprintf(stderr, "options require values\n"); return 2; }
        if (!strcmp(argv[i], "--seed")) sd = strtoull(argv[++i], 0, 0);
        else if (!strcmp(argv[i], "--count")) count = strtoull(argv[++i], 0, 0);
        else if (!strcmp(argv[i], "--case") && !strcmp(argv[i + 1], "default-nan-mul2d")) { default_nan_case = 1; i++; }
        else if (!strcmp(argv[i], "--corpus") && !strcmp(argv[i + 1], "edge")) { edge = 1; i++; }
        else { fprintf(stderr, "usage: fuzz_host [--seed N] [--count N] [--case default-nan-mul2d] | --corpus edge [--list-words]\n"); return 2; }
    }
    if (list_words) {
        build_ops();
        for (int i = 0; i < nops; i++) printf("%08x %s\n", ops[i].word, ops[i].name);
        return 0;
    }
    if (default_nan_case) count = 1;
    seed(sd);
    size_t tlen = (uintptr_t)tmpl_end - (uintptr_t)tmpl_start;
    page = mmap(0, 16384, PROT_READ | PROT_WRITE | PROT_EXEC, MAP_PRIVATE | MAP_ANON | JIT_FLAGS, -1, 0);
    if (page == MAP_FAILED) { perror("mmap"); return 1; }
    woff = (uintptr_t)tmpl_word - (uintptr_t)tmpl_start;
    struct sigaction sa; memset(&sa, 0, sizeof sa); sa.sa_handler = on_sigill; sigemptyset(&sa.sa_mask);
    sa.sa_flags = SA_NODEFER;
    sigaction(SIGILL, &sa, 0);
    JIT_WRITE(1);
    memcpy(page, (void *)tmpl_start, tlen);
    JIT_WRITE(0);
    ICACHE(page, tlen);
    fn = (void (*)(ctx_t *))page;
    if (edge) {
        build_ops();
        edge_corpus();
        fprintf(stderr, "fuzz_host corpus=edge words=%d executed=%llu undefined(SIGILL)=%llu\n", nops,
                (unsigned long long)nt, (unsigned long long)nu);
        return 0;
    }
    for (uint64_t n = 0; n < count; n++) {
        uint32_t w;
        if (default_nan_case) {
            // FMUL V0.2D,V1.2D,V2.2D: zero times positive infinity in each lane.
            // Capture the actual hardware result; never substitute an expected NaN.
            memset(&c, 0, sizeof c);
            w = 0x6e62dc20;
            c.v[4] = c.v[5] = UINT64_C(0x7ff0000000000000);
        } else {
            w = gen_word();
            for (int i = 0; i < 31; i++) c.x[i] = rand_x();
            c.x[18] = 0; // macOS reserves x18 (the kernel clears it on a context switch); replay does not compare it
            c.nzcv = (uint64_t)(rnd() & 0xF) << 28;
            for (int i = 0; i < 32; i++) {
                int m = rnd() % 6;
                c.v[2 * i] = rand_v64(m);
                c.v[2 * i + 1] = (rnd() & 1) ? rand_v64(m) : rand_v64(rnd() % 6);
                if (rnd() % 8 == 0) c.v[2 * i + 1] = 0; // scalar-style values with zeroed upper half
            }
            if (rnd() % 4 == 0) { // make operand registers alias in a few cases (same-register forms)
                int a = rnd() % 32, b = rnd() % 32; c.v[2 * a] = c.v[2 * b]; c.v[2 * a + 1] = c.v[2 * b + 1];
            }
        }
        run_case(w, 0);
    }
    fprintf(stderr, "fuzz_host seed=%llu words=%llu executed=%llu undefined(SIGILL)=%llu\n",
            (unsigned long long)sd, (unsigned long long)count, (unsigned long long)nt, (unsigned long long)nu);
    return 0;
}
