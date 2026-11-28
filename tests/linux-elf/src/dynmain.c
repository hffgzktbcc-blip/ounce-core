/* A main program with PT_INTERP /lib/fake-ld.so (see fakeld.c): the loader starts the interpreter, which jumps
 * here. Prints and exits 0. */
#include "common.h"
void __attribute__((naked)) _start(void) { __asm__ volatile("bl cmain"); }
void cmain(void) { put("main: reached through the interpreter\n"); quit(0); }
