/* Two guest threads made with a raw clone (the flags musl's pthread_create uses, plus CLONE_CHILD_SETTID).
 * They wait on a futex until the main thread wakes them, then race 100000 atomic increments each on one
 * counter (an LDXR/STXR loop: exact only if a thread switch clears the exclusive monitor). Each checks its
 * TLS register and thread id; the main thread joins both through CLONE_CHILD_CLEARTID and a futex wait,
 * the way glibc's pthread_join does. Prints what it saw and exits 0. */
#include "common.h"

#define SYS_futex 98
#define SYS_gettid 178
#define SYS_clone 220
#define FUTEX_WAIT 0
#define FUTEX_WAIT_PRIVATE 128
#define FUTEX_WAKE_PRIVATE 129
#define EAGAIN 11
/* VM FS FILES SIGHAND THREAD SYSVSEM SETTLS PARENT_SETTID CHILD_CLEARTID CHILD_SETTID */
#define THREAD_FLAGS (0x100 | 0x200 | 0x400 | 0x800 | 0x10000 | 0x40000 | 0x80000 | 0x100000 | 0x200000 | 0x1000000)

struct worker {
    volatile int ptid, ctid;
    u64 tls_seen;
    i64 tid_seen;
    u64 id;
};
static struct worker w[2];
static char stacks[2][16384] __attribute__((aligned(16)));
static volatile int counter, go_flag;

static i64 futex(volatile int *addr, u64 op, int val) { return sys6(SYS_futex, (u64)addr, op, (u64)val, 0, 0, 0); }

static void worker_main(struct worker *me) {
    u64 tp;
    __asm__ volatile("mrs %0, tpidr_el0" : "=r"(tp));
    me->tls_seen = tp;
    me->tid_seen = sys6(SYS_gettid, 0, 0, 0, 0, 0, 0);
    while (!go_flag) futex(&go_flag, FUTEX_WAIT_PRIVATE, 0);
    for (int i = 0; i < 100000; i++) __atomic_fetch_add(&counter, 1, __ATOMIC_RELAXED);
}

/* clone(flags, stack, ptid, tls, ctid); the child calls worker_main(me) on its own stack, then exit(0) */
static i64 spawn(struct worker *me) {
    register u64 x8 __asm__("x8") = SYS_clone;
    register u64 x0 __asm__("x0") = THREAD_FLAGS;
    register u64 x1 __asm__("x1") = (u64)(stacks[me->id] + sizeof stacks[0]);
    register u64 x2 __asm__("x2") = (u64)&me->ptid;
    register u64 x3 __asm__("x3") = 0x1000 + me->id;
    register u64 x4 __asm__("x4") = (u64)&me->ctid;
    register u64 x9 __asm__("x9") = (u64)me;
    register u64 x10 __asm__("x10") = (u64)worker_main;
    __asm__ volatile("svc #0\n"
                     "cbnz x0, 1f\n"
                     "mov x0, x9\n"
                     "blr x10\n"
                     "mov x8, #93\n"
                     "mov x0, #0\n"
                     "svc #0\n"
                     "1:\n"
                     : "+r"(x0)
                     : "r"(x8), "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x9), "r"(x10)
                     : "memory", "x30");
    return (i64)x0;
}

void __attribute__((naked)) _start(void) { __asm__ volatile("bl cmain"); }

void cmain(void) {
    i64 me = sys6(SYS_gettid, 0, 0, 0, 0, 0, 0);
    w[1].id = 1;
    i64 t0 = spawn(&w[0]), t1 = spawn(&w[1]);
    put(t0 > 0 && t1 > 0 && t0 != t1 && t0 != me && t1 != me ? "distinct tids\n" : "tid clash\n");
    put(w[0].ptid == t0 && w[1].ptid == t1 ? "parent settid ok\n" : "parent settid wrong\n");
    put(futex(&go_flag, FUTEX_WAIT_PRIVATE, 1) == -EAGAIN ? "wait on a changed word: EAGAIN\n" : "wait: no EAGAIN\n");
    go_flag = 1;
    futex(&go_flag, FUTEX_WAKE_PRIVATE, 2);
    for (int i = 0; i < 2; i++) {
        int t;
        /* not FUTEX_PRIVATE: the kernel's CLONE_CHILD_CLEARTID wake is a shared-futex wake, as glibc's join assumes */
        while ((t = w[i].ctid) != 0) futex(&w[i].ctid, FUTEX_WAIT, t);
    }
    put("joined\n");
    put("counter="); putdec((u64)counter); put("\n");
    put(w[0].tls_seen == 0x1000 && w[1].tls_seen == 0x1001 ? "tls ok\n" : "tls wrong\n");
    put(w[0].tid_seen == t0 && w[1].tid_seen == t1 ? "gettid ok\n" : "gettid wrong\n");
    put(futex(&counter, FUTEX_WAKE_PRIVATE, 1) == 0 ? "wake with no waiters: 0\n" : "wake: nonzero\n");
    quit(0);
}
