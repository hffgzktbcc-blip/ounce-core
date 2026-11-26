#define _XOPEN_SOURCE 600
#define _DARWIN_C_SOURCE
// Apple M3 (macOS) instruction-level evidence for SIGILL/SIGBUS and exclusive-monitor behaviour.
#include <signal.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <ucontext.h>
typedef uint64_t u64;
static volatile u64 sig_n, sig_code, sig_addr, sig_pc, cnt;
static void h(int sig, siginfo_t *si, void *ucv) {
    ucontext_t *uc = ucv;
    sig_n = sig; sig_code = si->si_code; sig_addr = (u64)si->si_addr;
    sig_pc = uc->uc_mcontext->__ss.__pc;
    uc->uc_mcontext->__ss.__pc += 4;
    cnt++;
}
static _Alignas(64) unsigned char buf[256];
#define RESET() (cnt = 0, sig_n = sig_code = sig_addr = sig_pc = 0)
#define REPORT(name, expect_pc) printf("%-28s signals=%llu sig=%llu code=%llu addr_is_%s pc_is_%s\n", name, cnt, sig_n, sig_code, \
   (sig_addr == (expect_pc) ? "pc" : (sig_addr ? "other" : "zero")), (sig_pc == (expect_pc) ? "insn" : "other"))
int main(void) {
    struct sigaction sa; memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = h; sa.sa_flags = SA_SIGINFO;
    sigaction(SIGILL, &sa, 0); sigaction(SIGBUS, &sa, 0);
    u64 a;
    RESET(); __asm__ volatile("adr %0, 1f\n1: .inst 0x00000000" : "=r"(a)); REPORT("udf #0", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: .inst 0x00010000" : "=r"(a)); REPORT("reserved 0x00010000", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: msr sctlr_el1, %1" : "=&r"(a) : "r"(0UL)); REPORT("msr sctlr_el1", a);
    RESET(); { u64 v; __asm__ volatile("adr %0, 1f\n1: mrs %1, id_aa64isar0_el1" : "=&r"(a), "=r"(v)); } REPORT("mrs id_aa64isar0_el1", a);
    u64 *p = (u64 *)buf, v = 0; u64 st;
    RESET(); __asm__ volatile("adr %0, 1f\n1: ldxr %1, [%2]" : "=&r"(a), "=r"(v) : "r"(buf + 1) : "memory"); REPORT("ldxr x misaligned by 1", a); printf("   fault addr == buf+1: %d\n", sig_addr == (u64)(buf + 1));
    RESET(); __asm__ volatile("adr %0, 1f\n1: ldxr %w1, [%2]" : "=&r"(a), "=r"(v) : "r"(buf + 2) : "memory"); REPORT("ldxr w misaligned by 2", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: ldxrh %w1, [%2]" : "=&r"(a), "=r"(v) : "r"(buf + 1) : "memory"); REPORT("ldxrh misaligned by 1", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: stxr %w1, %2, [%3]" : "=&r"(a), "=&r"(st) : "r"(v), "r"(buf + 4) : "memory"); REPORT("stxr x misaligned by 4", a);
    RESET(); { u64 x, y; __asm__ volatile("adr %0, 1f\n1: ldxp %1, %2, [%3]" : "=&r"(a), "=&r"(x), "=&r"(y) : "r"(buf + 8) : "memory"); } REPORT("ldxp x misaligned by 8", a);
    RESET(); { u64 x, y; __asm__ volatile("adr %0, 1f\n1: ldxp %w1, %w2, [%3]" : "=&r"(a), "=&r"(x), "=&r"(y) : "r"(buf + 4) : "memory"); } REPORT("ldxp w misaligned by 4", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: ldar %1, [%2]" : "=&r"(a), "=r"(v) : "r"(buf + 1) : "memory"); REPORT("ldar x misaligned by 1", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: ldar %w1, [%2]" : "=&r"(a), "=r"(v) : "r"(buf + 2) : "memory"); REPORT("ldar w misaligned by 2", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: stlr %1, [%2]" : "=&r"(a) : "r"(v), "r"(buf + 1) : "memory"); REPORT("stlr x misaligned by 1", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: ldxr %1, [%2]" : "=&r"(a), "=r"(v) : "r"(buf + 12) : "memory"); REPORT("ldxr x crossing 16B (+12)", a); printf("   fault addr == buf+12: %d\n", sig_addr == (u64)(buf + 12));
    RESET(); __asm__ volatile("adr %0, 1f\n1: ldxr %w1, [%2]" : "=&r"(a), "=r"(v) : "r"(buf + 14) : "memory"); REPORT("ldxr w crossing 16B (+14)", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: ldxrh %w1, [%2]" : "=&r"(a), "=r"(v) : "r"(buf + 15) : "memory"); REPORT("ldxrh crossing 16B (+15)", a);
    RESET(); __asm__ volatile("adr %0, 1f\n1: stxr %w1, %2, [%3]" : "=&r"(a), "=&r"(st) : "r"(v), "r"(buf + 12) : "memory"); REPORT("stxr x crossing 16B (+12)", a);
    RESET(); { u64 x, y; __asm__ volatile("adr %0, 1f\n1: ldaxp %1, %2, [%3]" : "=&r"(a), "=&r"(x), "=&r"(y) : "r"(buf + 1) : "memory"); } REPORT("ldaxp x misaligned by 1", a);
    // Monitor behaviour (aligned, no signals expected).
    RESET();
    __asm__ volatile("ldxr %0, [%1]\n stxr %w2, %0, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory"); printf("ldxr;stxr same addr/size       status=%llu\n", st);
    __asm__ volatile("ldxr %0, [%1]\n clrex\n stxr %w2, %0, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory"); printf("ldxr;clrex;stxr                status=%llu\n", st);
    __asm__ volatile("clrex\n stxr %w1, %2, [%0]" : "+r"(p), "=&r"(st) : "r"(0UL) : "memory"); printf("clrex;stxr (no ldxr)           status=%llu\n", st);
    __asm__ volatile("ldxr %0, [%1]\n stxr %w2, %0, [%1]\n stxr %w2, %0, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory"); printf("ldxr;stxr;stxr (second)        status=%llu\n", st);
    __asm__ volatile("ldxr %0, [%1]\n str %0, [%1]\n stxr %w2, %0, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory"); printf("ldxr;str same addr;stxr        status=%llu\n", st);
    __asm__ volatile("ldxr %0, [%1]\n str %0, [%1, #64]\n stxr %w2, %0, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory"); printf("ldxr;str other line;stxr       status=%llu\n", st);
    __asm__ volatile("ldxr %0, [%1]\n str %0, [%1, #8]\n stxr %w2, %0, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory"); printf("ldxr;str same line +8;stxr     status=%llu\n", st);
    __asm__ volatile("ldxr %0, [%1]\n add x16, %1, #8\n stxr %w2, %0, [x16]" : "=&r"(v), "+r"(p), "=&r"(st) :: "memory", "x16"); printf("ldxr [p];stxr [p+8]            status=%llu\n", st);
    { u64 z = 0; __asm__ volatile("ldxr %w0, [%1]\n stxr %w2, %3, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) : "r"(z) : "memory"); printf("ldxr w;stxr x same addr        status=%llu\n", st); }
    { u64 z = 0; __asm__ volatile("ldxr %0, [%1]\n stxr %w2, %w3, [%1]" : "=&r"(v), "+r"(p), "=&r"(st) : "r"(z) : "memory"); printf("ldxr x;stxr w same addr        status=%llu\n", st); }
    return 0;
}
