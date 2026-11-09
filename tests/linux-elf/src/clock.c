#include "common.h"
struct timespec { u64 sec, nsec; };
static void show(u64 clock, const char *name) {
    struct timespec t;
    if (sys3(113, clock, &t, 0) != 0) quit(1);
    put(name); putdec(t.sec); put(":"); putdec(t.nsec); put("\n");
}
void _start(void) {
    show(0, "realtime=");
    show(1, "monotonic=");
    show(4, "raw=");
    show(7, "boottime=");
    quit(0);
}
