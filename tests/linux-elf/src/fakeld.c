/* A stand-in program interpreter (ET_DYN, loaded at its own base). It checks the auxv the loader passes,
 * AT_BASE (its own base), AT_ENTRY (the main program's entry) and AT_PHDR, then jumps to AT_ENTRY with the
 * original stack, as ld.so does after relocating. */
#include "common.h"
void __attribute__((naked)) _start(void) { __asm__ volatile("mov x0, sp\n bl ldmain"); }
static void start_main(u64 *sp, u64 entry) {
    __asm__ volatile("mov sp, %0\n br %1" ::"r"(sp), "r"(entry));
}
void ldmain(u64 *sp) {
    u64 argc = sp[0];
    u64 *p = sp + 1 + argc + 1;
    while (*p) p++;
    p++;
    u64 base = 0, entry = 0, phdr = 0, self;
    for (; p[0]; p += 2) {
        if (p[0] == 7) base = p[1];
        if (p[0] == 9) entry = p[1];
        if (p[0] == 3) phdr = p[1];
    }
    __asm__ volatile("adr %0, ." : "=r"(self));
    put(base && self > base && self - base < 0x100000 ? "ld: running at AT_BASE\n" : "ld: AT_BASE wrong\n");
    put(entry && phdr ? "ld: AT_ENTRY and AT_PHDR set\n" : "ld: AT_ENTRY or AT_PHDR missing\n");
    start_main(sp, entry);
}
