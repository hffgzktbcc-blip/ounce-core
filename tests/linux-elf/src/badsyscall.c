/* Calls getrandom (278), which run-elf does not implement: expect a diagnosis naming it. */
#include "common.h"
void _start(void) {
    put("before\n");
    char buf[8];
    sys3(278, buf, sizeof buf, 0);
    put("after\n");
    quit(0);
}
