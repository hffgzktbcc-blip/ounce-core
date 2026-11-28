/* File-backed mmap of scratch files: MAP_PRIVATE contents and copy-on-write (the file keeps its bytes), a page
 * offset, zeros past the end of the file, a PROT_NONE mapping made readable with mprotect, a read-only MAP_SHARED
 * mapping, code run from an executable mapping and replaced in place by a MAP_FIXED mapping of another file
 * (stale translated code must not run), and the errors Linux returns for bad requests, in the order it checks
 * them. Prints one line per check, then exits 0; the output matches the same ELF under qemu-aarch64 on Linux. */
#include "common.h"

#define SYS_openat 56
#define SYS_close 57
#define SYS_pread64 67
#define SYS_mprotect 226
#define AT_FDCWD -100
#define O_RDONLY 0
#define O_WRONLY 1
#define O_RDWR 2
#define O_CREAT 0100
#define O_TRUNC 01000
#define PROT_NONE 0
#define PROT_READ 1
#define PROT_WRITE 2
#define PROT_EXEC 4
#define MAP_SHARED 1
#define MAP_PRIVATE 2
#define MAP_FIXED 0x10
#define EBADF 9
#define EACCES 13
#define ENODEV 19
#define EINVAL 22

static i64 mmap6(u64 addr, u64 len, u64 prot, u64 flags, i64 fd, u64 off) {
    return sys6(SYS_mmap, addr, len, prot, flags, (u64)fd, off);
}
static i64 open_rw(const char *path) { return sys6(SYS_openat, (u64)AT_FDCWD, (u64)path, O_RDWR | O_CREAT | O_TRUNC, 0644, 0, 0); }
static void check(int ok, const char *what) { put(what); put(ok ? ": ok\n" : ": WRONG\n"); }

static unsigned char page[8192];

/* mov w0, #imm; ret */
static void code(unsigned imm, unsigned char *out) {
    unsigned insns[2] = {0x52800000u | (imm << 5), 0xd65f03c0u};
    for (int i = 0; i < 8; i++) out[i] = (unsigned char)(insns[i / 4] >> (8 * (i % 4)));
}

void __attribute__((naked)) _start(void) { __asm__ volatile("bl cmain"); }

void cmain(void) {
    /* data.bin: page 0 is 'a'..'z' repeating, page 1 is all 'B', then 100 bytes of 'C' (8292 bytes in all) */
    for (int i = 0; i < 4096; i++) page[i] = (unsigned char)('a' + i % 26);
    for (int i = 4096; i < 8192; i++) page[i] = 'B';
    i64 fd = open_rw("/tmp/data.bin");
    sys3(SYS_write, fd, page, 8192);
    for (int i = 0; i < 100; i++) page[i] = 'C';
    sys3(SYS_write, fd, page, 100);

    volatile unsigned char *m = (volatile unsigned char *)mmap6(0, 3 * 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE, fd, 0);
    check((i64)m > 0 && m[0] == 'a' && m[27] == 'b' && m[4096] == 'B' && m[8191] == 'B', "private mapping has the file bytes");
    check(m[8192] == 'C' && m[8291] == 'C' && m[8292] == 0 && m[12287] == 0, "tail page is zero past the end of the file");
    m[0] = 'Z';
    unsigned char first = 0;
    sys6(SYS_pread64, (u64)fd, (u64)&first, 1, 0, 0, 0);
    check(m[0] == 'Z' && first == 'a', "a store to a private mapping leaves the file alone");
    sys3(SYS_munmap, m, 3 * 4096, 0);

    volatile unsigned char *o = (volatile unsigned char *)mmap6(0, 4096, PROT_READ, MAP_PRIVATE, fd, 4096);
    check((i64)o > 0 && o[0] == 'B' && o[4095] == 'B', "offset 4096 maps the second page");

    volatile unsigned char *n = (volatile unsigned char *)mmap6(0, 4096, PROT_NONE, MAP_PRIVATE, fd, 0);
    i64 mp = sys3(SYS_mprotect, n, 4096, PROT_READ);
    check((i64)n > 0 && mp == 0 && n[1] == 'b', "PROT_NONE mapping keeps the bytes for mprotect");

    i64 ro = sys6(SYS_openat, (u64)AT_FDCWD, (u64)"/tmp/data.bin", O_RDONLY, 0, 0, 0);
    volatile unsigned char *s = (volatile unsigned char *)mmap6(0, 4096, PROT_READ, MAP_SHARED, ro, 0);
    check((i64)s > 0 && s[2] == 'c', "read-only shared mapping");
    check(mmap6(0, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, ro, 0) == -EACCES, "writable shared mapping of a read-only fd: EACCES");
    i64 wo = sys6(SYS_openat, (u64)AT_FDCWD, (u64)"/tmp/data.bin", O_WRONLY, 0, 0, 0);
    check(mmap6(0, 4096, PROT_READ, MAP_PRIVATE, wo, 0) == -EACCES, "mapping a write-only fd: EACCES");
    check(mmap6(0, 4096, PROT_READ, MAP_PRIVATE, 1, 0) == -EACCES, "mapping write-only stdout: EACCES");
    i64 dir = sys6(SYS_openat, (u64)AT_FDCWD, (u64)"/tmp", O_RDONLY, 0, 0, 0);
    check(dir >= 0 && mmap6(0, 4096, PROT_READ, MAP_PRIVATE, dir, 0) == -ENODEV, "mapping a directory: ENODEV");
    check(mmap6(0, 4096, PROT_READ, MAP_PRIVATE, 77, 0) == -EBADF, "mapping a closed fd: EBADF");
    check(mmap6(0, 4096, PROT_READ, MAP_PRIVATE, fd, 100) == -EINVAL, "unaligned offset: EINVAL");
    check(mmap6(0, 4096, PROT_READ, 0, fd, 0) == -EINVAL, "neither MAP_SHARED nor MAP_PRIVATE: EINVAL");

    /* code: two files holding a function, mapped executable at one address in turn */
    i64 c1 = open_rw("/tmp/one.bin"), c2 = open_rw("/tmp/two.bin");
    unsigned char buf[8];
    code(42, buf); sys3(SYS_write, c1, buf, 8);
    code(7, buf); sys3(SYS_write, c2, buf, 8);
    i64 at = mmap6(0, 4096, PROT_READ | PROT_EXEC, MAP_PRIVATE, c1, 0);
    sys3(SYS_close, c1, 0, 0);
    int (*fn)(void) = (int (*)(void))at;
    /* Call enough times to cross Tier-3 (4) and Tier-4 (64) hot thresholds before remapping. */
    int ok = 1;
    for (int i = 0; i < 100; i++) {
        if (fn() != 42) ok = 0;
    }
    i64 again = mmap6((u64)at, 4096, PROT_READ | PROT_EXEC, MAP_PRIVATE | MAP_FIXED, c2, 0);
    int c = fn();
    check(at > 0 && ok, "code runs from an executable file mapping after its fd is closed");
    check(again == at && c == 7, "a MAP_FIXED mapping of another file replaces the code");
    quit(0);
}
