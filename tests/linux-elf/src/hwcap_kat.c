/* Known-answer test for PMULL/PMULL2, CRC32*, CRC32C*, EOR3/BCAX/RAX1/XAR. Prints results as hex. */
#include "common.h"
#include "hwcap_kat_core.h"
static void hex(u64 v) {
    char b[18]; for (int i = 0; i < 16; i++) b[i] = "0123456789abcdef"[(v >> (60 - 4 * i)) & 15];
    b[16] = '\n'; b[17] = 0; put(b);
}
void _start(void) {
    u64 out[NSETS * 24];
    run_all(out);
    for (int i = 0; i < NSETS * 24; i++) hex(out[i]);
    quit(0);
}
