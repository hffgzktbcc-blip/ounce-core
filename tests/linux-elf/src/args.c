/* Prints argc and each argv string, the auxv AT_PAGESZ, and exits with argc. */
#include "common.h"
void __attribute__((naked)) _start(void) {
    __asm__ volatile("mov x0, sp\n bl cmain");
}
void cmain(u64 *sp) {
    u64 argc = sp[0];
    char **argv = (char **)(sp + 1);
    put("argc="); putdec(argc); put("\n");
    for (u64 i = 0; i < argc; i++) { put("argv: "); put(argv[i]); put("\n"); }
    u64 *p = (u64 *)(argv + argc + 1);   /* envp */
    while (*p) p++;
    p++;                                  /* auxv */
    for (; p[0]; p += 2) if (p[0] == 6) { put("pagesz="); putdec(p[1]); put("\n"); }
    quit((int)argc);
}
