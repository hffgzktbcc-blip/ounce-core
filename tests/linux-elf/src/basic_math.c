#include "common.h"

void _start(void) {
    u64 a = 15;
    u64 b = 27;
    u64 sum = a + b;
    put("calculating 15 + 27 = ");
    putdec(sum);
    put("\n");
    quit((int)sum);
}
