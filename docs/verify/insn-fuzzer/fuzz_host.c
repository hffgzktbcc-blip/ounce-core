// Host side of the single-instruction fuzzer: runs random A64 data-processing (scalar FP + Advanced
// SIMD) words with random register inputs on real Apple Silicon and prints a text trace that
// `ounce-core fuzz-replay` replays in the interpreter. See README.md.
//
//   cc -O1 -o fuzz_host fuzz_host.c && ./fuzz_host --seed 1 --count 150000 | ounce-core fuzz-replay -
//
// Trace lines (hex, space separated):
//   U <word>                                   host raised SIGILL (UNDEFINED encoding)
//   T <word> <nzcv_in> <31 x> <32 v128> => <nzcv_out> <fpsr_out> <31 x> <32 v128>
#include <pthread.h>
#include <setjmp.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <libkern/OSCacheControl.h>

// Context layout (bytes): in.x @0, in.nzcv @248, in.v @256, out.x @768, out.nzcv @1016, out.fpsr @1024,
// out.v @1040.
typedef struct {
    uint64_t x[31]; uint64_t nzcv; uint64_t v[64];
    uint64_t ox[31]; uint64_t onzcv; uint64_t ofpsr; uint64_t pad1; uint64_t ov[64];
} __attribute__((aligned(16))) ctx_t;
_Static_assert(__builtin_offsetof(ctx_t, nzcv) == 248, "nzcv");
_Static_assert(__builtin_offsetof(ctx_t, v) == 256, "v");
_Static_assert(__builtin_offsetof(ctx_t, ox) == 768, "ox");
_Static_assert(__builtin_offsetof(ctx_t, onzcv) == 1016, "onzcv");
_Static_assert(__builtin_offsetof(ctx_t, ov) == 1040, "ov");

// Position-independent template copied into the JIT page: loads every register from the context
// (x0), runs the test word at tmpl_word, stores every register back. FPCR is left at the host default (0).
void tmpl_start(void);
void tmpl_word(void);
void tmpl_end(void);
__asm__(
    ".text\n.p2align 4\n"
    ".globl _tmpl_start\n_tmpl_start:\n"
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
    "ldr x17, [x16, #248]\nmsr nzcv, x17\nmsr fpsr, xzr\n"
    "ldp x0, x1, [x16, #0]\nldp x2, x3, [x16, #16]\nldp x4, x5, [x16, #32]\nldp x6, x7, [x16, #48]\n"
    "ldp x8, x9, [x16, #64]\nldp x10, x11, [x16, #80]\nldp x12, x13, [x16, #96]\nldp x14, x15, [x16, #112]\n"
    "ldp x18, x19, [x16, #144]\nldp x20, x21, [x16, #160]\nldp x22, x23, [x16, #176]\nldp x24, x25, [x16, #192]\n"
    "ldp x26, x27, [x16, #208]\nldp x28, x29, [x16, #224]\nldr x30, [x16, #240]\n"
    "ldr x17, [x16, #136]\nldr x16, [x16, #128]\n"
    ".globl _tmpl_word\n_tmpl_word:\nnop\n"
    "stp x0, x1, [sp, #-16]!\n"
    "ldr x0, [sp, #176]\n"
    "add x0, x0, #768\n"
    "stp x2, x3, [x0, #16]\nstp x4, x5, [x0, #32]\nstp x6, x7, [x0, #48]\nstp x8, x9, [x0, #64]\n"
    "stp x10, x11, [x0, #80]\nstp x12, x13, [x0, #96]\nstp x14, x15, [x0, #112]\nstp x16, x17, [x0, #128]\n"
    "stp x18, x19, [x0, #144]\nstp x20, x21, [x0, #160]\nstp x22, x23, [x0, #176]\nstp x24, x25, [x0, #192]\n"
    "stp x26, x27, [x0, #208]\nstp x28, x29, [x0, #224]\nstr x30, [x0, #240]\n"
    "mrs x2, nzcv\nstr x2, [x0, #248]\nmrs x2, fpsr\nstr x2, [x0, #256]\n"
    "stp q0, q1, [x0, #272]\nstp q2, q3, [x0, #304]\nstp q4, q5, [x0, #336]\nstp q6, q7, [x0, #368]\n"
    "stp q8, q9, [x0, #400]\nstp q10, q11, [x0, #432]\nstp q12, q13, [x0, #464]\nstp q14, q15, [x0, #496]\n"
    "stp q16, q17, [x0, #528]\nstp q18, q19, [x0, #560]\nstp q20, q21, [x0, #592]\nstp q22, q23, [x0, #624]\n"
    "stp q24, q25, [x0, #656]\nstp q26, q27, [x0, #688]\nstp q28, q29, [x0, #720]\nstp q30, q31, [x0, #752]\n"
    "ldp x2, x3, [sp], #16\nstp x2, x3, [x0, #0]\n"
    "ldp x19, x20, [sp, #0]\nldp x21, x22, [sp, #16]\nldp x23, x24, [sp, #32]\nldp x25, x26, [sp, #48]\n"
    "ldp x27, x28, [sp, #64]\nldp x29, x30, [sp, #80]\n"
    "ldp d8, d9, [sp, #96]\nldp d10, d11, [sp, #112]\nldp d12, d13, [sp, #128]\nldp d14, d15, [sp, #144]\n"
    "add sp, sp, #176\nret\n"
    ".globl _tmpl_end\n_tmpl_end:\nnop\n");

static sigjmp_buf jb;
static void on_sigill(int s) { (void)s; siglongjmp(jb, 1); }

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

int main(int argc, char **argv) {
    uint64_t sd = 1, count = 1000;
    int default_nan_case = 0;
    if ((argc - 1) % 2) { fprintf(stderr, "options require values\n"); return 2; }
    for (int i = 1; i + 1 < argc; i += 2) {
        if (!strcmp(argv[i], "--seed")) sd = strtoull(argv[i + 1], 0, 0);
        else if (!strcmp(argv[i], "--count")) count = strtoull(argv[i + 1], 0, 0);
        else if (!strcmp(argv[i], "--case") && !strcmp(argv[i + 1], "default-nan-mul2d")) default_nan_case = 1;
        else { fprintf(stderr, "usage: fuzz_host [--seed N] [--count N] [--case default-nan-mul2d]\n"); return 2; }
    }
    if (default_nan_case) count = 1;
    seed(sd);
    size_t tlen = (uintptr_t)tmpl_end - (uintptr_t)tmpl_start;
    uint8_t *page = mmap(0, 16384, PROT_READ | PROT_WRITE | PROT_EXEC, MAP_PRIVATE | MAP_ANON | MAP_JIT, -1, 0);
    if (page == MAP_FAILED) { perror("mmap MAP_JIT"); return 1; }
    size_t woff = (uintptr_t)tmpl_word - (uintptr_t)tmpl_start;
    struct sigaction sa; memset(&sa, 0, sizeof sa); sa.sa_handler = on_sigill; sigemptyset(&sa.sa_mask);
    sa.sa_flags = SA_NODEFER;
    sigaction(SIGILL, &sa, 0);
    static ctx_t c;
    pthread_jit_write_protect_np(0);
    memcpy(page, (void *)tmpl_start, tlen);
    pthread_jit_write_protect_np(1);
    sys_icache_invalidate(page, tlen);
    void (*fn)(ctx_t *) = (void (*)(ctx_t *))page;
    uint64_t nu = 0, nt = 0;
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
        pthread_jit_write_protect_np(0);
        memcpy(page + woff, &w, 4);
        pthread_jit_write_protect_np(1);
        sys_icache_invalidate(page + woff, 4);
        if (sigsetjmp(jb, 1)) { printf("U %08x\n", w); nu++; continue; }
        fn(&c);
        nt++;
        printf("T %08x %llx", w, (unsigned long long)(c.nzcv >> 28));
        for (int i = 0; i < 31; i++) printf(" %llx", (unsigned long long)c.x[i]);
        for (int i = 0; i < 32; i++) printf(" %016llx%016llx", (unsigned long long)c.v[2 * i + 1], (unsigned long long)c.v[2 * i]);
        printf(" => %llx %llx", (unsigned long long)(c.onzcv >> 28), (unsigned long long)c.ofpsr);
        for (int i = 0; i < 31; i++) printf(" %llx", (unsigned long long)c.ox[i]);
        for (int i = 0; i < 32; i++) printf(" %016llx%016llx", (unsigned long long)c.ov[2 * i + 1], (unsigned long long)c.ov[2 * i]);
        printf("\n");
    }
    fprintf(stderr, "fuzz_host seed=%llu words=%llu executed=%llu undefined(SIGILL)=%llu\n",
            (unsigned long long)sd, (unsigned long long)count, (unsigned long long)nt, (unsigned long long)nu);
    return 0;
}
