/* Known-answer test for LDXP/STXP pairs and FEAT_LSE atomics. Prints results as hex words. */
#include "common.h"
#include "lse_kat_core.h"
static void hex(u64 v) {
    char b[18]; for (int i = 0; i < 16; i++) b[i] = "0123456789abcdef"[(v >> (60 - 4 * i)) & 15];
    b[16] = '\n'; b[17] = 0; put(b);
}
void _start(void) {
    u64 out[LSE_NWORDS];
    u64 n = run_lse_kat(out);
    for (u64 i = 0; i < n; i++) hex(out[i]);
    quit(0);
}
