/* Known-answer ops for LDXP/LDAXP/STXP/STLXP and FEAT_LSE (LD<op>, ST<op>, SWP, CAS, CASP, LDAPR).
 * Shared by lse_kat.c (guest) and a native probe so the expected values come from real hardware.
 * Every sequence starts from fixed memory and records registers and memory as u64 words. */
typedef unsigned long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
#define LSE_NWORDS 400
static u64 buf[4] __attribute__((aligned(16)));
static u64 *outp;
static void rec(u64 v) { *outp++ = v; }
static void reset(void) { buf[0] = 0xfedcba9876543210UL; buf[1] = 0x0123456789abcdefUL; buf[2] = ~0UL; buf[3] = 0x8000000000000001UL; }

#define AT(x) ".arch_extension lse\n" x
/* ---- exclusive pairs ---- */
static void ldxp_x(void) { u64 a, b; reset(); __asm__ volatile("ldxp %0, %1, [%2]" : "=&r"(a), "=&r"(b) : "r"(buf) : "memory"); rec(a); rec(b); }
static void ldaxp_x(void) { u64 a, b; reset(); __asm__ volatile("ldaxp %0, %1, [%2]" : "=&r"(a), "=&r"(b) : "r"(buf) : "memory"); rec(a); rec(b); }
static void ldxp_w(void) { u32 a, b; reset(); __asm__ volatile("ldxp %w0, %w1, [%2]" : "=&r"(a), "=&r"(b) : "r"(buf) : "memory"); rec(a); rec(b); }
static void ldaxp_w(void) { u32 a, b; reset(); __asm__ volatile("ldaxp %w0, %w1, [%2]" : "=&r"(a), "=&r"(b) : "r"(buf + 2) : "memory"); rec(a); rec(b); }
static void ldxp_zr(void) { u64 a, b; reset();
    __asm__ volatile("ldxp xzr, %0, [%2]\nldxp %1, xzr, [%2]" : "=&r"(a), "=&r"(b) : "r"(buf) : "memory"); rec(a); rec(b); }
/* load then store: success, then a second store with the monitor consumed must fail */
#define STP(name, ld, st, W, T) \
    static void name(void) { T a, b; u32 s1, s2; reset(); \
        __asm__ volatile(ld " %" W "0, %" W "1, [%4]\n" st " %w2, %" W "5, %" W "6, [%4]\n" st " %w3, %" W "5, %" W "6, [%4]" \
            : "=&r"(a), "=&r"(b), "=&r"(s1), "=&r"(s2) : "r"(buf), "r"((T)0x1111222233334444UL), "r"((T)0x5555666677778888UL) : "memory"); \
        rec(a); rec(b); rec(s1); rec(s2); rec(buf[0]); rec(buf[1]); }
STP(stxp_x, "ldxp", "stxp", "x", u64)
STP(stlxp_x, "ldaxp", "stlxp", "x", u64)
STP(stxp_w, "ldxp", "stxp", "w", u32)
STP(stlxp_w, "ldaxp", "stlxp", "w", u32)
/* stxp of XZR,XZR */
static void stxp_zr(void) { u32 s; u64 a, b; reset();
    __asm__ volatile("ldxp %1, %2, [%3]\nstxp %w0, xzr, xzr, [%3]" : "=&r"(s), "=&r"(a), "=&r"(b) : "r"(buf) : "memory"); rec(s); rec(buf[0]); rec(buf[1]); }
/* stxp with no load-exclusive since the last store-exclusive */
static void stxp_nomon(void) { u32 s1, s2; u64 a, b; reset();
    __asm__ volatile("ldxp %2, %3, [%4]\nstxp %w0, %2, %3, [%4]\nstxp %w1, xzr, xzr, [%4]" : "=&r"(s1), "=&r"(s2), "=&r"(a), "=&r"(b) : "r"(buf) : "memory");
    rec(s1); rec(s2); rec(buf[0]); rec(buf[1]); }
/* 128-bit increment loop (what __int128 atomics compile to): carries across the halves */
static void inc128(u64 lo, u64 hi, int n) {
    buf[0] = lo; buf[1] = hi;
    for (int i = 0; i < n; i++) {
        u64 a, b, nl, nh; u32 s;
        do { __asm__ volatile("ldaxp %0, %1, [%2]" : "=&r"(a), "=&r"(b) : "r"(buf) : "memory");
             nl = a + 1; nh = b + (nl == 0);
             __asm__ volatile("stlxp %w0, %1, %2, [%3]" : "=&r"(s) : "r"(nl), "r"(nh), "r"(buf) : "memory"); } while (s);
    }
    rec(buf[0]); rec(buf[1]);
}
/* 128-bit compare-and-swap loop: success then failure, written as LL/SC */
static void cas128_llsc(u64 elo, u64 ehi, u64 nlo, u64 nhi) {
    u64 a, b; u32 s; int ok = 0;
    do { __asm__ volatile("ldaxp %0, %1, [%2]" : "=&r"(a), "=&r"(b) : "r"(buf) : "memory");
         if (a != elo || b != ehi) { __asm__ volatile("clrex"); break; }
         __asm__ volatile("stlxp %w0, %1, %2, [%3]" : "=&r"(s) : "r"(nlo), "r"(nhi), "r"(buf) : "memory");
         if (!s) ok = 1; } while (!ok && !(a != elo || b != ehi));
    rec(a); rec(b); rec(ok); rec(buf[0]); rec(buf[1]);
}
/* ---- FEAT_LSE ---- */
static void casp_x(u64 elo, u64 ehi, u64 nlo, u64 nhi) {
    register u64 x0 __asm__("x0") = elo, x1 __asm__("x1") = ehi, x2 __asm__("x2") = nlo, x3 __asm__("x3") = nhi;
    __asm__ volatile(AT("caspal x0, x1, x2, x3, [%4]") : "+r"(x0), "+r"(x1) : "r"(x2), "r"(x3), "r"(buf) : "memory");
    rec(x0); rec(x1); rec(buf[0]); rec(buf[1]);
}
static void casp_w(u32 elo, u32 ehi, u32 nlo, u32 nhi) {
    register u32 x0 __asm__("w0") = elo, x1 __asm__("w1") = ehi, x2 __asm__("w2") = nlo, x3 __asm__("w3") = nhi;
    __asm__ volatile(AT("casp w0, w1, w2, w3, [%4]") : "+r"(x0), "+r"(x1) : "r"(x2), "r"(x3), "r"(buf) : "memory");
    rec(x0); rec(x1); rec(buf[0]); rec(buf[1]);
}
#define CASOP(name, mn, W, T) static void name(T e, T n) { T r = e; \
    __asm__ volatile(AT(mn " %" W "0, %" W "1, [%2]") : "+r"(r) : "r"(n), "r"(buf) : "memory"); rec(r); rec(buf[0]); }
CASOP(cas_b, "casalb", "w", u32) CASOP(cas_h, "casah", "w", u32) CASOP(cas_w, "casl", "w", u32) CASOP(cas_x, "casa", "x", u64)
#define LDOP(name, mn, W, T) static void name(T v) { T r; \
    __asm__ volatile(AT(mn " %" W "1, %" W "0, [%2]") : "=&r"(r) : "r"(v), "r"(buf) : "memory"); rec(r); rec(buf[0]); }
#define LDALL(sfx, mn, T) \
    LDOP(ldadd_##sfx, "ldadd" mn, "w", T) LDOP(ldclr_##sfx, "ldclr" mn, "w", T) LDOP(ldeor_##sfx, "ldeor" mn, "w", T) \
    LDOP(ldset_##sfx, "ldset" mn, "w", T) LDOP(ldsmax_##sfx, "ldsmax" mn, "w", T) LDOP(ldsmin_##sfx, "ldsmin" mn, "w", T) \
    LDOP(ldumax_##sfx, "ldumax" mn, "w", T) LDOP(ldumin_##sfx, "ldumin" mn, "w", T) LDOP(swp_##sfx, "swp" mn, "w", T)
LDALL(b, "alb", u32) LDALL(h, "alh", u32) LDALL(w, "al", u32)
#define LDOPX(name, mn) static void name(u64 v) { u64 r; \
    __asm__ volatile(AT(mn " %1, %0, [%2]") : "=&r"(r) : "r"(v), "r"(buf) : "memory"); rec(r); rec(buf[0]); }
LDOPX(ldadd_x, "ldaddal") LDOPX(ldclr_x, "ldclral") LDOPX(ldeor_x, "ldeoral") LDOPX(ldset_x, "ldsetal")
LDOPX(ldsmax_x, "ldsmaxal") LDOPX(ldsmin_x, "ldsminal") LDOPX(ldumax_x, "ldumaxal") LDOPX(ldumin_x, "lduminal") LDOPX(swp_x, "swpal")
static void stadd_x(u64 v) { __asm__ volatile(AT("stadd %0, [%1]") : : "r"(v), "r"(buf) : "memory"); rec(buf[0]); }
static void stset_w(u32 v) { __asm__ volatile(AT("stset %w0, [%1]") : : "r"(v), "r"(buf) : "memory"); rec(buf[0]); }
static void ldapr_all(void) { u64 x; u32 w, h, b; reset();
    __asm__ volatile(".arch_extension rcpc\nldapr %0, [%4]\nldapr %w1, [%4]\nldaprh %w2, [%4]\nldaprb %w3, [%4]" : "=&r"(x), "=&r"(w), "=&r"(h), "=&r"(b) : "r"(buf) : "memory");
    rec(x); rec(w); rec(h); rec(b); }

static u64 run_lse_kat(u64 *out) {
    outp = out;
    ldxp_x(); ldaxp_x(); ldxp_w(); ldaxp_w(); ldxp_zr();
    stxp_x(); stlxp_x(); stxp_w(); stlxp_w(); stxp_zr(); stxp_nomon();
    inc128(0xfffffffffffffffeUL, 0x00000000ffffffffUL, 4);
    inc128(0xffffffffffffffffUL, 0xffffffffffffffffUL, 1);
    reset(); cas128_llsc(buf[0], buf[1], 0x1111111111111111UL, 0x2222222222222222UL);
    reset(); cas128_llsc(1, 2, 0x1111111111111111UL, 0x2222222222222222UL);
    reset(); casp_x(buf[0], buf[1], 0xaaaaaaaaaaaaaaaaUL, 0xbbbbbbbbbbbbbbbbUL);
    reset(); casp_x(buf[0], buf[1] ^ 1, 0xaaaaaaaaaaaaaaaaUL, 0xbbbbbbbbbbbbbbbbUL);
    reset(); casp_w((u32)buf[0], (u32)(buf[0] >> 32), 0x01020304, 0x05060708);
    reset(); casp_w(7, (u32)(buf[0] >> 32), 0x01020304, 0x05060708);
    reset(); cas_b(0x10, 0x99); reset(); cas_b(0x11, 0x99);
    reset(); cas_h(0x3210, 0x9999); reset(); cas_h(0x3211, 0x9999);
    reset(); cas_w(0x76543210, 0x99999999); reset(); cas_w(0x76543211, 0x99999999);
    reset(); cas_x(buf[0], 0x9999999999999999UL); reset(); cas_x(5, 0x9999999999999999UL);
    static const u64 V[] = {0x00000000000000f0UL, 0x80000000000000ffUL, 0xfffffffffffffffeUL};
    for (int i = 0; i < 3; i++) {
        u64 v = V[i];
#define DO(f, a) reset(); f(a);
        DO(ldadd_b, (u32)v) DO(ldclr_b, (u32)v) DO(ldeor_b, (u32)v) DO(ldset_b, (u32)v) DO(ldsmax_b, (u32)v) DO(ldsmin_b, (u32)v) DO(ldumax_b, (u32)v) DO(ldumin_b, (u32)v) DO(swp_b, (u32)v)
        DO(ldadd_h, (u32)v) DO(ldclr_h, (u32)v) DO(ldeor_h, (u32)v) DO(ldset_h, (u32)v) DO(ldsmax_h, (u32)v) DO(ldsmin_h, (u32)v) DO(ldumax_h, (u32)v) DO(ldumin_h, (u32)v) DO(swp_h, (u32)v)
        DO(ldadd_w, (u32)v) DO(ldclr_w, (u32)v) DO(ldeor_w, (u32)v) DO(ldset_w, (u32)v) DO(ldsmax_w, (u32)v) DO(ldsmin_w, (u32)v) DO(ldumax_w, (u32)v) DO(ldumin_w, (u32)v) DO(swp_w, (u32)v)
        DO(ldadd_x, v) DO(ldclr_x, v) DO(ldeor_x, v) DO(ldset_x, v) DO(ldsmax_x, v) DO(ldsmin_x, v) DO(ldumax_x, v) DO(ldumin_x, v) DO(swp_x, v)
    }
    reset(); stadd_x(0x10); reset(); stset_w(0xf0f0f0f0);
    ldapr_all();
    return (u64)(outp - out);
}
