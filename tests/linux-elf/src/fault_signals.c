/* MIT: SIGILL and SIGBUS reach a guest handler with the Linux arm64 siginfo and sigcontext, and the exclusive
 * monitor obeys CLREX. Everything here is architectural, so it also runs unchanged on Linux AArch64 hardware:
 * the misaligned exclusives cross a 16-byte boundary, which faults with and without FEAT_LSE2.
 * Sources: arch/arm64/kernel/traps.c (do_el0_undef: SIGILL, ILL_ILLOPC, si_addr = pc) and
 * arch/arm64/mm/fault.c (alignment fault: SIGBUS, BUS_ADRALN, si_addr = faulting address). */
#include "common.h"
#define ILL_ILLOPC 1
#define BUS_ADRALN 1
static volatile u64 count, last_sig, last_code, last_addr, last_pc, last_magic, last_esr;
static unsigned char buf[64] __attribute__((aligned(64)));
static void handler(u64 sig, u64 *info, unsigned char *uc) {
    u64 *pc = (u64 *)(uc + 176 + 264);
    last_sig = sig;
    last_code = ((unsigned int *)info)[2];
    last_addr = info[2];
    last_pc = *pc;
    /* The record after fpsimd_context (528 bytes) in __reserved: esr_context for SIGBUS, else the terminator. */
    u64 *rec = (u64 *)(uc + 176 + 288 + 528);
    last_magic = ((unsigned int *)rec)[0];
    last_esr = rec[1];
    *pc += 4; /* skip the faulting instruction in the Linux sigcontext */
    count++;
}
/* ESR_MAGIC record for SIGBUS: EC 0x24, IL, DFSC 0x21 (alignment fault), WnR (bit 6) for stores: 0x92000021 / 0x92000061.
 * SIGILL carries no esr_context (arm64_notify_die zeroes fault_code), so the record is the terminator. */
#define ESR_LOAD 0x92000021UL
#define ESR_STORE 0x92000061UL
#define EXPECT(n, sig, code, addr) \
    do { if (count != (n) || last_sig != (sig) || last_code != (code) || last_addr != (addr) || last_pc != insn || \
            ((sig) == 4 ? (last_magic != 0 || last_esr != 0) : (last_magic != 0x45535201 || last_esr != esr_want))) \
            quit(n * 10 + (sig)); } while (0)

void _start(void) {
    u64 action[4] = {(u64)handler, 4, 0, 0};
    if (sys6(134, 4, (u64)action, 0, 8, 0, 0) || sys6(134, 7, (u64)action, 0, 8, 0, 0)) quit(1);
    u64 insn, v = 0, st = 9, x, y;
    unsigned char *b = buf;
    u64 esr_want = ESR_LOAD;

    __asm__ volatile("adr %0, 1f\n1: .inst 0x00000000" : "=r"(insn)); /* UDF #0 */
    EXPECT(1, 4, ILL_ILLOPC, insn);
    __asm__ volatile("adr %0, 1f\n1: .inst 0x00010000" : "=r"(insn)); /* reserved class */
    EXPECT(2, 4, ILL_ILLOPC, insn);
    __asm__ volatile("adr %0, 1f\n1: msr sctlr_el1, %1" : "=&r"(insn) : "r"(0UL)); /* EL1 register from EL0 */
    EXPECT(3, 4, ILL_ILLOPC, insn);
    __asm__ volatile("adr %0, 1f\n1: mrs %1, sctlr_el1" : "=&r"(insn), "=r"(v)); /* never emulated for EL0 */
    EXPECT(4, 4, ILL_ILLOPC, insn);

    __asm__ volatile("adr %0, 1f\n1: ldxr %1, [%2]" : "=&r"(insn), "=r"(v) : "r"(b + 12) : "memory");
    EXPECT(5, 7, BUS_ADRALN, (u64)(b + 12));
    __asm__ volatile("adr %0, 1f\n1: ldaxr %w1, [%2]" : "=&r"(insn), "=r"(v) : "r"(b + 14) : "memory");
    EXPECT(6, 7, BUS_ADRALN, (u64)(b + 14));
    __asm__ volatile("adr %0, 1f\n1: ldxrh %w1, [%2]" : "=&r"(insn), "=r"(v) : "r"(b + 15) : "memory");
    EXPECT(7, 7, BUS_ADRALN, (u64)(b + 15));
    *(volatile u64 *)(b + 8) = 0x1111111111111111UL;
    *(volatile u64 *)(b + 16) = 0x2222222222222222UL;
    v = 0xdeadbeefdeadbeefUL;
    esr_want = ESR_STORE;
    __asm__ volatile("adr %0, 1f\n1: stxr %w1, %2, [%3]" : "=&r"(insn), "+&r"(st) : "r"(v), "r"(b + 12) : "memory");
    EXPECT(8, 7, BUS_ADRALN, (u64)(b + 12));
    esr_want = ESR_LOAD;
    if (st != 9 || *(volatile u64 *)(b + 8) != 0x1111111111111111UL || *(volatile u64 *)(b + 16) != 0x2222222222222222UL)
        quit(81); /* a misaligned STXR must not store or write its status register */
    __asm__ volatile("adr %0, 1f\n1: ldxp %1, %2, [%3]" : "=&r"(insn), "=&r"(x), "=&r"(y) : "r"(b + 8) : "memory");
    EXPECT(9, 7, BUS_ADRALN, (u64)(b + 8));
    __asm__ volatile("adr %0, 1f\n1: ldaxp %1, %2, [%3]" : "=&r"(insn), "=&r"(x), "=&r"(y) : "r"(b + 24) : "memory");
    EXPECT(10, 7, BUS_ADRALN, (u64)(b + 24));
    esr_want = ESR_STORE;
    __asm__ volatile("adr %0, 1f\n1: stxp %w1, %2, %3, [%4]" : "=&r"(insn), "=&r"(st) : "r"(v), "r"(v), "r"(b + 8) : "memory");
    EXPECT(11, 7, BUS_ADRALN, (u64)(b + 8));

    /* The handler returned each time, and a fault on an aligned, valid access is not reported. */
    u64 *p = (u64 *)(b + 32);
    *p = 5;
    __asm__ volatile("ldxr %0, [%1]\n stxr %w2, %0, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory");
    if (st != 0 || count != 11) quit(90);
    __asm__ volatile("ldxr %0, [%1]\n clrex\n stxr %w2, %0, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory");
    if (st != 1) quit(91); /* CLREX clears the monitor */
    __asm__ volatile("clrex\n stxr %w1, %2, [%0]" : "+r"(p), "=&r"(st) : "r"(v) : "memory");
    if (st != 1) quit(92); /* STXR with no LDXR fails */
    __asm__ volatile("ldxr %0, [%1]\n stxr %w2, %0, [%1]\n stxr %w2, %0, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory");
    if (st != 1) quit(93); /* a store-exclusive consumes the monitor */
    if (count != 11) quit(94);
    put("SIGILL and SIGBUS delivery, siginfo and CLREX: ok\n");
    quit(0);
}
