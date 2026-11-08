// PMULL / PMULL2 differential check: inline-asm pmull over varied operands.
// Host:  clang -O1 -march=armv8-a+crypto pmull.c -o pmull-host && ./pmull-host > expected-host.txt
// Guest: built as an NRO with devkitA64 (-march=armv8-a+crypto), writes sdmc:/results.txt
#include <stdio.h>
#include <stdint.h>
#include <arm_neon.h>
#ifdef __SWITCH__
#include <switch.h>
#endif

static uint64_t s = 0x9E3779B97F4A7C15ull;
static uint64_t rnd(void) { s ^= s << 13; s ^= s >> 7; s ^= s << 17; return s; }

static void emit(FILE *o, const char *tag, uint64x2_t r) {
    fprintf(o, "%s %016llx%016llx\n", tag, (unsigned long long)vgetq_lane_u64(r, 1), (unsigned long long)vgetq_lane_u64(r, 0));
}

static void one(FILE *o, uint64_t a0, uint64_t a1, uint64_t b0, uint64_t b1) {
    uint64x2_t n = vcombine_u64(vcreate_u64(a0), vcreate_u64(a1));
    uint64x2_t m = vcombine_u64(vcreate_u64(b0), vcreate_u64(b1));
    uint64x2_t r;
    fprintf(o, "in %016llx %016llx %016llx %016llx\n", (unsigned long long)a0, (unsigned long long)a1, (unsigned long long)b0, (unsigned long long)b1);
    // Destination preloaded with junk: PMULL must overwrite all 128 bits.
    r = vdupq_n_u64(0xDEADBEEFDEADBEEFull);
    __asm__ volatile("pmull %0.8h, %1.8b, %2.8b" : "+w"(r) : "w"(n), "w"(m));
    emit(o, "pmull_8h ", r);
    r = vdupq_n_u64(0xDEADBEEFDEADBEEFull);
    __asm__ volatile("pmull2 %0.8h, %1.16b, %2.16b" : "+w"(r) : "w"(n), "w"(m));
    emit(o, "pmull2_8h", r);
    r = vdupq_n_u64(0xDEADBEEFDEADBEEFull);
    __asm__ volatile("pmull %0.1q, %1.1d, %2.1d" : "+w"(r) : "w"(n), "w"(m));
    emit(o, "pmull_1q ", r);
    r = vdupq_n_u64(0xDEADBEEFDEADBEEFull);
    __asm__ volatile("pmull2 %0.1q, %1.2d, %2.2d" : "+w"(r) : "w"(n), "w"(m));
    emit(o, "pmull2_1q", r);
}

int pmull_run(FILE *o) {
    static const uint64_t e[] = {0, 1, 2, 3, 0x80, 0xFF, 0x8000000000000001ull, 0xFFFFFFFFFFFFFFFFull,
                                 0x8000000000000000ull, 0x0123456789ABCDEFull, 0x00FF00FF00FF00FFull, 0x0303030303030303ull};
    int ne = sizeof e / sizeof e[0];
    for (int i = 0; i < ne; i++)
        for (int j = 0; j < ne; j++)
            one(o, e[i], e[(i + 5) % ne], e[j], e[(j + 3) % ne]);
    for (int i = 0; i < 200; i++) one(o, rnd(), rnd(), rnd(), rnd());
    return 0;
}

#ifdef __SWITCH__
int main(void) {
    consoleInit(NULL);
    FILE *f = fopen("sdmc:/results.txt", "w");
    pmull_run(f ? f : stdout);
    if (f) fclose(f);
    printf("pmull harness done\n");
    consoleUpdate(NULL);
    return 0;
}
#else
int main(void) { return pmull_run(stdout); }
#endif
