/* Freestanding Linux AArch64 syscall helpers: no libc, no SIMD. */
typedef unsigned long u64;
typedef long i64;

static inline i64 sys6(u64 nr, u64 a, u64 b, u64 c, u64 d, u64 e, u64 f) {
    register u64 x8 __asm__("x8") = nr;
    register u64 x0 __asm__("x0") = a;
    register u64 x1 __asm__("x1") = b;
    register u64 x2 __asm__("x2") = c;
    register u64 x3 __asm__("x3") = d;
    register u64 x4 __asm__("x4") = e;
    register u64 x5 __asm__("x5") = f;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x8), "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x5) : "memory");
    return (i64)x0;
}
#define sys3(n, a, b, c) sys6(n, (u64)(a), (u64)(b), (u64)(c), 0, 0, 0)

#define SYS_write 64
#define SYS_writev 66
#define SYS_exit 93
#define SYS_exit_group 94
#define SYS_brk 214
#define SYS_munmap 215
#define SYS_mmap 222

static u64 slen(const char *s) { u64 n = 0; while (s[n]) n++; return n; }
static void puts_fd(int fd, const char *s) { sys3(SYS_write, fd, s, slen(s)); }
static void put(const char *s) { puts_fd(1, s); }
static void putdec(u64 v) {
    char b[24]; int i = 23; b[i] = 0;
    do { b[--i] = '0' + v % 10; v /= 10; } while (v);
    put(b + i);
}
static void quit(int code) { sys3(SYS_exit_group, code, 0, 0); for (;;) {} }
