// libnx threads fixture for ounce-core. Built with devkitA64 + libnx (see Makefile).
// Two worker threads add to a shared total under a libnx Mutex, yielding while they hold it so the other
// worker has to block in svcArbitrateLock. The main thread waits on a CondVar until both report done,
// then joins them with threadWaitForExit. Each thread also checks its own __thread variable.
// A third thread busy-waits on a flag without making SVCs; only time-slice preemption lets the main
// thread run and set it.
// Reports through svcOutputDebugString: "ounce-threads total=0x........" and
// "ounce-threads result=0x00000EEE" when every check passed, else 0xBAD0000 | failed checks.
#include <switch.h>

void __appInit(void) {}
void __appExit(void) {}

#define WORKERS 2
#define ITERS 300

static Mutex g_mutex;
static CondVar g_cv;
static u64 g_total;
static int g_done;
static u32 g_tls_fails;
static __thread u64 t_local = 5;
static volatile u32 g_flag;
static volatile u32 g_spun;

static void spinner(void *arg)
{
    while (!g_flag)
        ; // no SVC: the kernel has to preempt this thread
    g_spun = 1;
}

static void report(const char *tag, u32 value)
{
    char msg[48] = "ounce-threads ";
    int n = 14;
    while (*tag)
        msg[n++] = *tag++;
    msg[n++] = '=';
    msg[n++] = '0';
    msg[n++] = 'x';
    for (int i = 7; i >= 0; i--)
        msg[n++] = "0123456789ABCDEF"[(value >> (4 * i)) & 0xF];
    msg[n++] = '\n';
    svcOutputDebugString(msg, n);
}

static void worker(void *arg)
{
    u64 id = (u64)arg;
    for (int i = 0; i < ITERS; i++) {
        mutexLock(&g_mutex);
        u64 t = g_total;
        if (i % 10 == 0)
            svcSleepThread(0); // yield while holding the lock: the other worker must block on it
        g_total = t + id;
        mutexUnlock(&g_mutex);
        t_local += id;
    }
    mutexLock(&g_mutex);
    if (t_local != 5 + ITERS * id)
        g_tls_fails++;
    g_done++;
    condvarWakeAll(&g_cv);
    mutexUnlock(&g_mutex);
}

int main(int argc, char **argv)
{
    u32 fails = 0;
    Thread threads[WORKERS];
    mutexInit(&g_mutex);
    condvarInit(&g_cv);

    for (int i = 0; i < WORKERS; i++) {
        if (R_FAILED(threadCreate(&threads[i], worker, (void *)(u64)(i + 1), NULL, 0x10000, 0x2C, -2)))
            fails |= 1 << 0;
    }
    if (!fails) {
        for (int i = 0; i < WORKERS; i++)
            if (R_FAILED(threadStart(&threads[i])))
                fails |= 1 << 1;
    }
    Thread spin;
    if (!fails) {
        if (R_FAILED(threadCreate(&spin, spinner, NULL, NULL, 0x4000, 0x2C, -2)) || R_FAILED(threadStart(&spin)))
            fails |= 1 << 5;
        g_flag = 1; // reached only if the spinner was preempted
        if (!(fails & (1 << 5))) {
            threadWaitForExit(&spin);
            threadClose(&spin);
            if (!g_spun)
                fails |= 1 << 5;
        }
    }
    if (!fails) {
        mutexLock(&g_mutex);
        while (g_done < WORKERS)
            condvarWait(&g_cv, &g_mutex);
        mutexUnlock(&g_mutex);
        for (int i = 0; i < WORKERS; i++) {
            if (R_FAILED(threadWaitForExit(&threads[i])))
                fails |= 1 << 2;
            threadClose(&threads[i]);
        }
        if (g_total != (u64)ITERS * (1 + 2))
            fails |= 1 << 3;
        if (g_tls_fails || t_local != 5)
            fails |= 1 << 4;
    }

    report("total", (u32)g_total);
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
