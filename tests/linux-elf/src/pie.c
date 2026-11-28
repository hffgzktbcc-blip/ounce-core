/* A static position-independent executable (ET_DYN, no PT_INTERP, no relocations: only PC-relative code).
 * The loader must place it at a base address; it prints where its code runs and exits 0. */
#include "common.h"
void __attribute__((naked)) _start(void) { __asm__ volatile("bl cmain"); }
void cmain(void) {
    u64 pc;
    __asm__ volatile("adr %0, ." : "=r"(pc));
    put(pc > 0x10000000 ? "pie runs at its load base\n" : "pie runs unrelocated\n");
    quit(0);
}
