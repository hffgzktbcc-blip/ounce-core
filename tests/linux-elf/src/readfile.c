/* Prints the file named by argv[1] (opened read-only through openat) and exits 0; on failure prints the
 * negative errno and exits 1. Used for `run-elf --root=DIR`. */
#include "common.h"
#define SYS_openat 56
#define SYS_read 63
void __attribute__((naked)) _start(void) { __asm__ volatile("mov x0, sp\n bl cmain"); }
void cmain(u64 *sp) {
    char **argv = (char **)(sp + 1);
    i64 fd = sys6(SYS_openat, (u64)-100, (u64)argv[1], 0, 0, 0, 0);
    if (fd < 0) { put("open failed: -"); putdec((u64)-fd); put("\n"); quit(1); }
    char buf[256];
    for (;;) {
        i64 n = sys6(SYS_read, (u64)fd, (u64)buf, sizeof buf, 0, 0, 0);
        if (n < 0) { put("read failed\n"); quit(1); }
        if (n == 0) break;
        sys3(SYS_write, 1, buf, n);
    }
    quit(0);
}
