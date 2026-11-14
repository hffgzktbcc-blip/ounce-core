/* MIT: exercise the Linux ABI used by guarded Wasm memories, without libc. */
#include "common.h"
static volatile u64 faults;
static unsigned char alternate[16384] __attribute__((aligned(16)));
static void handler(u64 sig, u64 *info, unsigned char *uc) {
    u64 local;
    if (sig != 11 || ((unsigned int *)info)[2] != 2 || info[2] == 0 || (u64)&local < (u64)alternate ||
        (u64)&local >= (u64)alternate + sizeof alternate) quit(10);
    /* Skip the single faulting LDR in the Linux sigcontext. */
    u64 *pc = (u64 *)(uc + 176 + 264);
    *pc += 4;
    faults++;
}
void _start(void) {
    u64 n = 0x10000100000UL;
    i64 base = sys6(SYS_mmap, 0, n, 0, 0x4022, -1UL, 0);
    if (base < 0) quit(1);
    u64 p = (u64)base + 4096;
    if (sys3(226, p, 4096, 3)) quit(2);
    *(volatile u64 *)p = 42;
    if (sys3(226, p, 4096, 0)) quit(3);
    if (sys3(226, p, 4096, 3) || *(volatile u64 *)p != 42) quit(4);
    u64 action[4] = {(u64)handler, 4 | 0x08000000, 0, 0}, old[4] = {0};
    u64 alt[3] = {(u64)alternate, 0, sizeof alternate};
    if (sys3(132, alt, 0, 0) || sys6(134, 11, (u64)action, 0, 8, 0, 0)) quit(5);
    if (sys6(134, 11, 0, (u64)old, 8, 0, 0) || old[0] != action[0]) quit(6);
    /* Repeated faults test rt_sigreturn and restoration of the blocked signal mask. */
    __asm__ volatile("ldr x9, [%0]" :: "r"(p + 4096) : "x9", "memory");
    __asm__ volatile("ldr x9, [%0]" :: "r"(p + 8192) : "x9", "memory");
    *(volatile u64 *)(p + 4088) = -1UL;
    __asm__ volatile("str xzr, [%0]" :: "r"(p + 4092) : "memory");
    if (faults != 3 || *(volatile u64 *)(p + 4088) != -1UL) quit(7);
    if (sys6(SYS_mmap, p, 4096, 0, 0x4032, -1UL, 0) != (i64)p || sys3(226, p, 4096, 3)) quit(8);
    if (*(volatile u64 *)p != 0 || sys3(SYS_munmap, base, n, 0)) quit(9);
    put("guarded reservation, commit, fault, return, decommit: ok\n");
    quit(0);
}
