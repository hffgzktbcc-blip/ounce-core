// Records of the 3x+1 (Collatz) problem: for each n the number of steps of the map x -> x/2 (x even), x -> 3x+1 (x odd) needed to
// reach 1 is computed, and the "delay records" (the n whose step count exceeds that of every smaller n) are compared with the
// published list of record holders and their step counts (the table of 3x+1 delay records at ericr.nl/wondrous/delrecs.html, as listed in
// OEIS A006877 and A006878). Written from the definition of
// the map; step counts use a table of the counts for smaller numbers (a trajectory stops as soon as it drops below its start).
// All n below 10^7 are examined: the published list has 54 records there (the last is 8400511 with 685 steps; the next, 11200681,
// is beyond 10^7). Exit 0 only if the computed records equal the published ones exactly.
#include "ounce_paper.h"

#define LIMIT 10000000UL
#ifndef COLLATZ_ODD_COST
#define COLLATZ_ODD_COST 2        // an odd step 3x+1 followed by the forced halving is two steps
#endif

static u16 steps[LIMIT];

// {record holder, steps} from OEIS A006877 / A006878, all terms below 10^7
static const struct { u32 n, s; } PUB[54] = {
    {1, 0},
    {2, 1},
    {3, 7},
    {6, 8},
    {7, 16},
    {9, 19},
    {18, 20},
    {25, 23},
    {27, 111},
    {54, 112},
    {73, 115},
    {97, 118},
    {129, 121},
    {171, 124},
    {231, 127},
    {313, 130},
    {327, 143},
    {649, 144},
    {703, 170},
    {871, 178},
    {1161, 181},
    {2223, 182},
    {2463, 208},
    {2919, 216},
    {3711, 237},
    {6171, 261},
    {10971, 267},
    {13255, 275},
    {17647, 278},
    {23529, 281},
    {26623, 307},
    {34239, 310},
    {35655, 323},
    {52527, 339},
    {77031, 350},
    {106239, 353},
    {142587, 374},
    {156159, 382},
    {216367, 385},
    {230631, 442},
    {410011, 448},
    {511935, 469},
    {626331, 508},
    {837799, 524},
    {1117065, 527},
    {1501353, 530},
    {1723519, 556},
    {2298025, 559},
    {3064033, 562},
    {3542887, 583},
    {3732423, 596},
    {5649499, 612},
    {6649279, 664},
    {8400511, 685}
};

int paper_main(void) {
    steps[1] = 0;
    u32 best = 0; int nrec = 0, match = 0, extra = 0;
    pstr("records found (n: steps):");
    for (u64 n = 1; n < LIMIT; n++) {
        u64 x = n; u32 c = 0;
        if (n > 1) {
            while (x >= n) {
                if (x & 1) { x = (3 * x + 1) / 2; c += COLLATZ_ODD_COST; } else { x >>= 1; c += 1; }
            }
            steps[n] = (u16)(c + steps[x]);
        }
        if (n == 1 || steps[n] > best) {
            best = steps[n];
            if (nrec < 54 && PUB[nrec].n == n && PUB[nrec].s == steps[n]) match++; else extra++;
            nrec++;
            if (nrec <= 5 || nrec > 50) { pstr(" "); pdec((i64)n); pstr(":"); pdec(steps[n]); if (nrec == 5) pstr(" ..."); }
        }
    }
    pstr("\n");
    pstr("records found: "); pdec(nrec); pstr(", matching the published list in position: "); pdec(match); pstr(" of 54\n");
    pstr("largest record below 10^7: "); pdec(PUB[53].n); pstr(" with "); pdec(best); pstr(" steps\n");
    int ok = nrec == 54 && match == 54 && extra == 0;
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
