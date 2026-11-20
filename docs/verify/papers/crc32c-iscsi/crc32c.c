// CRC32C (Castagnoli polynomial 0x11edc6f41) as specified for iSCSI digests in RFC 3720 section 12.1 and Appendix B.4, written
// from the RFC's description of the process: complement the first 32 message bits, multiply the message polynomial by x^32, divide
// by G(x), complement the 32-bit remainder. Two independent implementations are run and required to agree: a bit-by-bit
// polynomial division that follows the RFC's wording, and a byte-wise table-driven reflected CRC (table computed at run time).
// Checks against numbers printed in the RFC: the five example CRCs of Appendix B.4 (32 zero bytes, 32 bytes 0xff, 00..1f,
// 1f..00 and a 48-byte SCSI Read(10) command PDU) and the residue 0x1c2d19ed that section 12.1 says is the final remainder when the
// CRC is computed over a segment extended with its own CRC (once per example, 5 checks). The numbers are data copied from the RFC;
// none of its code was copied. Exit 0 only if all 10 published values match and both implementations agree on all five inputs.
#include "ounce_paper.h"

#ifndef CRC32C_POLY   // negative control hook: the generator polynomial (RFC 3720: 0x11edc6f41)
#define CRC32C_POLY 0x11edc6f41ULL
#endif

// Bit order: message bytes are consumed least significant bit first and the CRC bytes use the same order. This is the order in
// which the Appendix B.4 example bytes come out (and in which iSCSI implementations compute CRC32C). Taking the RFC's prose literally
// (bit 7 of the lowest numbered byte first, x^31 in bit 7 of the first digest byte) was tried and gives 55 6c 89 51 for the
// zeroes example instead of the published aa 36 91 8a, so that reading does not reproduce the examples (see paper.json).
// Remainder of M(x) * x^32 mod G(x) with the first 32 bits of M complemented (no final complement).
static u32 remainder_bitwise(const u8 *m, int n) {
    u64 r = 0;
    for (int i = 0; i < n * 8 + 32; i++) {
        int bit = 0;
        if (i < n * 8) { bit = (m[i / 8] >> (i % 8)) & 1; if (i < 32) bit ^= 1; }
        r = (r << 1) | (u64)bit;
        if (r >> 32) r ^= CRC32C_POLY;
    }
    return (u32)r;
}
static void digest_from_remainder(u32 rem, u8 out[4]) {   // complement; x^31 goes to bit 0 of byte 0
    u32 c = ~rem;
    for (int j = 0; j < 4; j++) { u8 b = 0; for (int k = 0; k < 8; k++) b |= (u8)(((c >> (31 - (8 * j + k))) & 1) << k); out[j] = b; }
}

static u32 T[256];
static void make_table(void) {
    u32 poly = 0;                                           // bit-reverse the low 32 bits of the generator
    for (int i = 0; i < 32; i++) if ((CRC32C_POLY >> i) & 1) poly |= 1u << (31 - i);
    // note: bit 32 of the generator (x^32) is implicit; bit i of CRC32C_POLY is x^i, so the reflected constant has x^31 at bit 0
    for (u32 b = 0; b < 256; b++) { u32 c = b; for (int k = 0; k < 8; k++) c = (c & 1) ? (c >> 1) ^ poly : c >> 1; T[b] = c; }
}
static void crc32c_table(const u8 *m, int n, u8 out[4]) {
    u32 c = 0xffffffffu;
    for (int i = 0; i < n; i++) c = T[(c ^ m[i]) & 0xff] ^ (c >> 8);
    c = ~c;
    for (int j = 0; j < 4; j++) out[j] = (u8)(c >> (8 * j));  // reflected value, least significant byte first
}

static u8 Z[32], F[32], I[32], D[32];
static const u8 PDU[48] = {
    0x01, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x18,
    0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00};
static const u8 W_ZERO[4] = {0xaa, 0x36, 0x91, 0x8a}, W_ONES[4] = {0x43, 0xab, 0xa8, 0x62}, W_INC[4] = {0x4e, 0x79, 0xdd, 0x46},
                W_DEC[4] = {0x5c, 0xdb, 0x3f, 0x11}, W_PDU[4] = {0x56, 0x3a, 0x96, 0xd9};

static int one(const char *name, const u8 *m, int n, const u8 want[4]) {
    int ok = 1;
    u8 d1[4], d2[4];
    digest_from_remainder(remainder_bitwise(m, n), d1);
    crc32c_table(m, n, d2);
    ok &= pcheck_bytes(name, d1, want, 4);
    int agree = 1; for (int i = 0; i < 4; i++) if (d1[i] != d2[i]) agree = 0;
    if (!agree) { pstr("FAIL table-driven CRC differs from bitwise division: "); phex_bytes(d2, 4); pstr("\n"); }
    ok &= agree;
    u8 ext[64]; for (int i = 0; i < n; i++) ext[i] = m[i];
    for (int i = 0; i < 4; i++) ext[n + i] = d1[i];
    ok &= pcheck_u64("  residue over segment + its CRC", remainder_bitwise(ext, n + 4), 0x1c2d19edu, 8);
    return ok;
}

int paper_main(void) {
    make_table();
    for (int i = 0; i < 32; i++) { Z[i] = 0; F[i] = 0xff; I[i] = (u8)i; D[i] = (u8)(31 - i); }
    int ok = 1;
    ok &= one("RFC 3720 B.4, 32 bytes of zeroes   ", Z, 32, W_ZERO);
    ok &= one("RFC 3720 B.4, 32 bytes of ones     ", F, 32, W_ONES);
    ok &= one("RFC 3720 B.4, 32 bytes 00..1f      ", I, 32, W_INC);
    ok &= one("RFC 3720 B.4, 32 bytes 1f..00      ", D, 32, W_DEC);
    ok &= one("RFC 3720 B.4, SCSI Read(10) PDU    ", PDU, 48, W_PDU);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
