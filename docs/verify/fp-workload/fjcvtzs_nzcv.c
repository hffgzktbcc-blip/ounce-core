// Informational cross-check: FJCVTZS result, NZCV and FPSR for a list of doubles, at FPCR 0 and FZ. Freestanding aarch64
// Linux, one line per case: "<input bits> <fz> <w result> <NZCV nibble> <FPSR>". fp_edge does not print NZCV.
// Build like fp_edge.c's freestanding ELF (clang --target=aarch64-linux-gnu -ffreestanding -march=armv8.5-a, ld.lld -static
// -nostdlib -e _start), then compare `qemu-aarch64 x.elf`, `ounce-core run-elf x.elf --cpu=tier3` and, on ARM64 hardware, x.elf
// itself. Not part of run.sh or CI.
typedef unsigned long u64;
typedef unsigned int u32;

static void out(const char *s, long n) {
    register long x0 __asm__("x0") = 1;
    register const char *x1 __asm__("x1") = s;
    register long x2 __asm__("x2") = n;
    register long x8 __asm__("x8") = 64;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x8) : "memory");
}

static void hex(u64 v, int digits) {
    char b[17];
    for (int i = 0; i < digits; i++) b[i] = "0123456789abcdef"[(v >> (4 * (digits - 1 - i))) & 15];
    out(b, digits);
}

static const u64 in[] = {
    0x3ff0000000000000, 0xbff0000000000000, 0, 0x8000000000000000, 0x3ff8000000000000, 0xbff8000000000000,
    0x3fe0000000000000, 0xbfe0000000000000, 0x41e0000000000000, 0xc1e0000000000000, 0x41f0000000000000,
    0x41f0000000100000, 0x41f7fffffff00000, 0xc1f7fffffff00000, 0xc1e0000000200000, 0xc1e0000000100000,
    0x41dfffffffe00000, 0x41dfffffffc00000, 0x43e0000000000000, 0x43e0000000000001, 0x4278000000000000,
    0x7fefffffffffffff, 0x7ff0000000000000, 0xfff0000000000000, 0x7ff8000000000000, 0x7ff0000000000001,
    0x0000000000000001, 0x8000000000000001, 0x000fffffffffffff, 0x3fefffffffffffff, 0xbfefffffffffffff,
};

void _start(void) {
    for (unsigned i = 0; i < sizeof in / sizeof in[0]; i++) {
        for (int fz = 0; fz < 2; fz++) {
            u64 fpcr = fz ? (1ul << 24) : 0, nzcv, fpsr;
            u32 res;
            double d;
            __builtin_memcpy(&d, &in[i], sizeof d); /* copy the bits; a cast or aliasing read would not be defined */
            __asm__ volatile(
                "msr fpcr, %[fpcr]\n msr fpsr, xzr\n msr nzcv, %[ones]\n"
                "fjcvtzs %w[res], %d[d]\n"
                "mrs %[nzcv], nzcv\n mrs %[fpsr], fpsr\n msr fpcr, xzr\n"
                : [res] "=&r"(res), [nzcv] "=&r"(nzcv), [fpsr] "=&r"(fpsr)
                : [d] "w"(d), [fpcr] "r"(fpcr), [ones] "r"(0xf0000000ul)
                : "cc");
            hex(in[i], 16); out(" ", 1); hex(fz, 1); out(" ", 1); hex(res, 8); out(" ", 1);
            hex(nzcv >> 28, 1); out(" ", 1); hex(fpsr, 2); out("\n", 1);
        }
    }
    register long x0 __asm__("x0") = 0;
    register long x8 __asm__("x8") = 93;
    __asm__ volatile("svc #0" : : "r"(x0), "r"(x8));
    for (;;) {}
}
