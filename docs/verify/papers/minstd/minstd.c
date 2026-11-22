// The Lehmer "minimal standard" generator x[n+1] = a x[n] mod (2^31 - 1) (S. K. Park and K. W. Miller, "Random number generators:
// good ones are hard to find", Communications of the ACM 31(10), 1988), written from the recurrence. Two multipliers are run:
// a = 16807 (the 1988 paper's) and a = 48271 (the revised multiplier later proposed for the same generator). With seed 1 the 10000th
// value is the check the minimal standard is specified by; the C++ standard prints it for both (minstd_rand0, minstd_rand).
// Two published numbers; the expected numbers are data copied as numbers, no third-party code was copied.
// Exit 0 only if both match.
#include "ounce_paper.h"

#ifndef MINSTD_A0
#define MINSTD_A0 16807ULL
#endif
#define MOD 2147483647ULL
static u64 run(u64 a, u64 seed, int n) { u64 x = seed; for (int i = 0; i < n; i++) x = a * x % MOD; return x; }

int paper_main(void) {
    int ok = 1;
    ok &= pcheck_u64("a = 16807, seed 1, 10000th value", run(MINSTD_A0, 1, 10000), 1043618065ULL, 8);
    ok &= pcheck_u64("a = 48271, seed 1, 10000th value", run(48271ULL, 1, 10000), 399268537ULL, 8);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
