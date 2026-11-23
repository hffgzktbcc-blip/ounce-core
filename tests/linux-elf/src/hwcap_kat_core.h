/* Known-answer ops for every instruction behind the HWCAP_PMULL / HWCAP_CRC32 / HWCAP_SHA3 bits.
 * Shared by hwcap_kat.c (guest) and a native probe so expected values come from real hardware. */
typedef unsigned long u64;
typedef unsigned int u32;
#define NSETS 3
static const u64 IN[NSETS][6] = {
    {0x0123456789abcdefUL, 0xfedcba9876543210UL, 0xdeadbeefcafebabeUL, 0x0f1e2d3c4b5a6978UL, 0xa5a5a5a55a5a5a5aUL, 0xffffffff00000000UL},
    {0xffffffffffffffffUL, 0x8000000000000001UL, 0x0000000000000003UL, 0x8000000000000000UL, 0x123456789abcdef0UL, 0x00ff00ff00ff00ffUL},
    {0x0000000000000000UL, 0x0000000000000001UL, 0xc0ffee00badf00d5UL, 0x1b4e5e8a3c9d7f21UL, 0x7777777788888888UL, 0x9e3779b97f4a7c15UL},
};
#define NOPS 17
#define VEC(name, asmtxt) \
    static void name(const u64 *p, u64 *o) { \
        __asm__ volatile(".arch_extension aes\n.arch_extension sha3\n" \
            "ldr q1, [%1]\nldr q2, [%1, #16]\nldr q3, [%1, #32]\n" asmtxt "\nstr q0, [%0]\n" \
            : : "r"(o), "r"(p) : "memory", "v0", "v1", "v2", "v3"); }
VEC(op_pmull8b, "pmull v0.8h, v1.8b, v2.8b")
VEC(op_pmull2_16b, "pmull2 v0.8h, v1.16b, v2.16b")
VEC(op_pmull1d, "pmull v0.1q, v1.1d, v2.1d")
VEC(op_pmull2_2d, "pmull2 v0.1q, v1.2d, v2.2d")
VEC(op_eor3, "eor3 v0.16b, v1.16b, v2.16b, v3.16b")
VEC(op_bcax, "bcax v0.16b, v1.16b, v2.16b, v3.16b")
VEC(op_rax1, "rax1 v0.2d, v1.2d, v2.2d")
VEC(op_xar, "xar v0.2d, v1.2d, v2.2d, #19")
#define CRC(name, mn, T, R) \
    static u64 name(u64 acc, u64 d) { u32 r; \
        __asm__ volatile(".arch_extension crc\n" mn " %w0, %w1, %" R "2" : "=r"(r) : "r"((u32)acc), "r"((T)d)); return r; }
CRC(op_crc32b, "crc32b", u32, "w")
CRC(op_crc32h, "crc32h", u32, "w")
CRC(op_crc32w, "crc32w", u32, "w")
CRC(op_crc32x, "crc32x", u64, "x")
CRC(op_crc32cb, "crc32cb", u32, "w")
CRC(op_crc32ch, "crc32ch", u32, "w")
CRC(op_crc32cw, "crc32cw", u32, "w")
CRC(op_crc32cx, "crc32cx", u64, "x")
/* out: NSETS * (8 vector ops * 2 words + 8 crc words) u64 */
static void run_all(u64 *out) {
    for (int s = 0; s < NSETS; s++) {
        void (*v[8])(const u64 *, u64 *) = {op_pmull8b, op_pmull2_16b, op_pmull1d, op_pmull2_2d, op_eor3, op_bcax, op_rax1, op_xar};
        for (int i = 0; i < 8; i++) v[i](IN[s], out + i * 2);
        out += 16;
        u64 a = IN[s][3], d = IN[s][2];
        *out++ = op_crc32b(a, d); *out++ = op_crc32h(a, d); *out++ = op_crc32w(a, d); *out++ = op_crc32x(a, d);
        *out++ = op_crc32cb(a, d); *out++ = op_crc32ch(a, d); *out++ = op_crc32cw(a, d); *out++ = op_crc32cx(a, d);
    }
}
