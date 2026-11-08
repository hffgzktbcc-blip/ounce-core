/* brk grow/shrink, anonymous mmap/munmap, writev, and a checksum over the memory. */
#include "common.h"
#define PROT_RW 3
#define MAP_PRIVATE_ANON 0x22
struct iov { const void *base; u64 len; };
void _start(void) {
    u64 b0 = sys3(SYS_brk, 0, 0, 0);
    u64 b1 = sys3(SYS_brk, b0 + 3 * 4096 + 17, 0, 0);
    if (b1 != b0 + 3 * 4096 + 17) { put("brk grow failed\n"); quit(1); }
    unsigned char *h = (unsigned char *)b0;
    for (u64 i = 0; i < 3 * 4096; i++) h[i] = (unsigned char)(i * 7);
    u64 sum = 0;
    for (u64 i = 0; i < 3 * 4096; i++) sum += h[i];
    u64 b2 = sys3(SYS_brk, b0 + 4096, 0, 0);
    if (b2 != b0 + 4096) { put("brk shrink failed\n"); quit(2); }

    unsigned char *m = (unsigned char *)sys6(SYS_mmap, 0, 65536, PROT_RW, MAP_PRIVATE_ANON, (u64)-1, 0);
    if ((i64)m < 0 && (i64)m > -4096) { put("mmap failed\n"); quit(3); }
    for (u64 i = 0; i < 65536; i++) m[i] = (unsigned char)(i ^ 0x55);
    u64 msum = 0;
    for (u64 i = 0; i < 65536; i++) msum += m[i];
    if (sys3(SYS_munmap, m, 65536, 0) != 0) { put("munmap failed\n"); quit(4); }

    struct iov v[3] = { { "heap_sum=", 9 }, { 0, 0 }, { "\n", 1 } };
    put("heap_sum="); putdec(sum); put("\n");
    put("mmap_sum="); putdec(msum); put("\n");
    sys3(SYS_writev, 1, v, 3);
    quit(0);
}
