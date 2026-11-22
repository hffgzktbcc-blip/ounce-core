// The Internet checksum (16-bit one's complement sum) from RFC 1071, R. Braden, D. Borman and C. Partridge, "Computing the Internet
// Checksum" (1988), and its incremental update from RFC 1624, A. Rijsinghani, "Computation of the Internet Checksum via Incremental
// Update" (1994). Written from the RFCs' definitions. Checks, all against numbers printed in the RFCs:
//   RFC 1071 section 3 (8 bytes 00 01 f2 03 f4 f5 f6 f7): the byte-by-byte sums (2dc, 1f0) and their carry-folded bytes (dd, f2);
//   the 16-bit word sum in normal order (2ddf0 -> ddf2) and in swapped order (1f2dc -> f2dd, byte-swapped to ddf2);
//   the 32-bit sums in the three orders (f4f7e8fa, f6f4fbe8, fbe8f6f4), their folded halves (1ddf1, 1f2dc, 1f2dc) and results
//   (ddf2, f2dd, f2dd -> ddf2); and the two-group sum starting on an odd byte (f201, f0eb, 1ddf1, ddf2);
//   RFC 1624 section 4 (m = 0x5555 -> m' = 0x3285, other octets summing to 0xCD7A): HC = 0xDD2F, recomputed HC' = 0x0000,
//   RFC 1141's equation (Eqn. 2) giving the wrong 0xFFFF, and Eqn. 3 and Eqn. 4 giving 0x0000, plus the two verification sums
//   (0xCD7A + 0x3285 + 0xFFFF and + 0x0000 both 0xFFFF).
// The numbers are data copied from the RFCs; no code was copied. Exit 0 only if all 30 checks match.
#include "ounce_paper.h"

#ifndef IC_CARRY       // negative control hook: 1 = end-around carry as in one's complement arithmetic; 0 = drop the carry
#define IC_CARRY 1
#endif

// one's complement addition on 16 bits
static u32 oc16(u32 a, u32 b) { u32 s = a + b; return IC_CARRY ? (s & 0xffff) + (s >> 16) : (s & 0xffff); }
static u32 fold16(u32 s) { while (s >> 16) s = (s & 0xffff) + (IC_CARRY ? (s >> 16) : 0); return s & 0xffff; }
static u32 swap16(u32 x) { return ((x & 0xff) << 8) | (x >> 8); }
// subtraction "with borrow" (end-around borrow), the form RFC 1624 says removes the two extra instructions of Eqn. 3
static u32 sub16(u32 x, u32 y) { u32 d = (x - y) & 0xffff; return (x < y && IC_CARRY) ? (d - 1) & 0xffff : d; }
static u32 neg(u32 x) { return ~x & 0xffff; }

static int ck(const char *name, u64 got, u64 want, int digits) { return pcheck_u64(name, got, want, digits); }

int paper_main(void) {
    static const u8 B[8] = {0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7};
    int ok = 1;
    // --- RFC 1071, byte by byte: even-position bytes are the high half, odd-position the low half
    u32 hi = 0, lo = 0;
    for (int i = 0; i < 8; i += 2) { hi += B[i]; lo += B[i + 1]; }
    ok &= ck("RFC1071 byte-by-byte Sum1, high column", hi, 0x2dc, 3);
    ok &= ck("RFC1071 byte-by-byte Sum1, low column", lo, 0x1f0, 3);
    // each column's carry goes into the other column (end-around carry on the 16-bit total)
    u32 hi2 = (hi & 0xff) + (IC_CARRY ? (lo >> 8) : 0), lo2 = (lo & 0xff) + (IC_CARRY ? (hi >> 8) : 0);
    ok &= ck("RFC1071 byte-by-byte Sum2, high byte", hi2, 0xdd, 2);
    ok &= ck("RFC1071 byte-by-byte Sum2, low byte", lo2, 0xf2, 2);
    // --- 16-bit words, normal and swapped order
    u32 wn = 0, ws = 0;
    for (int i = 0; i < 8; i += 2) { wn += ((u32)B[i] << 8) | B[i + 1]; ws += ((u32)B[i + 1] << 8) | B[i]; }
    ok &= ck("RFC1071 16-bit words, normal order Sum1", wn, 0x2ddf0, 5);
    ok &= ck("RFC1071 16-bit words, normal order Sum2", fold16(wn), 0xddf2, 4);
    ok &= ck("RFC1071 16-bit words, swapped order Sum1", ws, 0x1f2dc, 5);
    ok &= ck("RFC1071 16-bit words, swapped order Sum2", fold16(ws), 0xf2dd, 4);
    ok &= ck("RFC1071 16-bit words, swapped order Final Swap", swap16(fold16(ws)), 0xddf2, 4);
    // --- 32-bit sums in three orders: normal, 16-bit swapped (bytes within words), fully byte reversed
    u32 a0 = ((u32)B[0] << 24) | (B[1] << 16) | (B[2] << 8) | B[3], a1 = ((u32)B[4] << 24) | (B[5] << 16) | (B[6] << 8) | B[7];
    u32 b0 = ((u32)B[1] << 24) | (B[0] << 16) | (B[3] << 8) | B[2], b1 = ((u32)B[5] << 24) | (B[4] << 16) | (B[7] << 8) | B[6];
    u32 c0 = ((u32)B[3] << 24) | (B[2] << 16) | (B[1] << 8) | B[0], c1 = ((u32)B[7] << 24) | (B[6] << 16) | (B[5] << 8) | B[4];
    static const u32 W1[3] = {0xf4f7e8fa, 0xf6f4fbe8, 0xfbe8f6f4}, W2[3] = {0x1ddf1, 0x1f2dc, 0x1f2dc}, W3[3] = {0xddf2, 0xf2dd, 0xf2dd};
    static const char *const N1[3] = {"RFC1071 32-bit normal Sum1", "RFC1071 32-bit swapped Sum1", "RFC1071 32-bit reversed Sum1"};
    static const char *const N2[3] = {"RFC1071 32-bit normal Sum2 (halves added)", "RFC1071 32-bit swapped Sum2 (halves added)",
                                      "RFC1071 32-bit reversed Sum2 (halves added)"};
    static const char *const N3[3] = {"RFC1071 32-bit normal Sum3", "RFC1071 32-bit swapped Sum3", "RFC1071 32-bit reversed Sum3"};
    u32 x0[3] = {a0, b0, c0}, x1[3] = {a1, b1, c1};
    for (int k = 0; k < 3; k++) {
        u32 s = x0[k] + x1[k];                      // 32-bit sum; the RFC shows no carry out in any of the three orders
        ok &= ck(N1[k], s, W1[k], 8);
        u32 h2 = (s >> 16) + (s & 0xffff);
        ok &= ck(N2[k], h2, W2[k], 5);
        ok &= ck(N3[k], fold16(h2), W3[k], 4);
    }
    // --- two groups, the second starting on an odd byte
    u32 g1 = ((u32)B[0] << 8) | B[1];
    g1 += (u32)B[2] << 8;                           // byte 2 padded with 00 -> f200
    ok &= ck("RFC1071 odd boundary Sum1", g1, 0xf201, 4);
    u32 g2 = (((u32)B[3] << 8) | B[4]) + (((u32)B[5] << 8) | B[6]) + ((u32)B[7] << 8);
    ok &= ck("RFC1071 odd boundary Sum2", g2, 0x1f0ea, 5);
    u32 g3 = fold16(g2);
    ok &= ck("RFC1071 odd boundary Sum3", g3, 0xf0eb, 4);
    u32 g4 = g1 + swap16(g3);
    ok &= ck("RFC1071 odd boundary Sum4", g4, 0x1ddf1, 5);
    ok &= ck("RFC1071 odd boundary Sum5", fold16(g4), 0xddf2, 4);
    // --- RFC 1624 section 4
    u32 rest = 0xCD7A, m = 0x5555, mp = 0x3285;
    u32 HC = neg(oc16(rest, m));
    ok &= ck("RFC1624 HC = ~(0xCD7A + 0x5555)", HC, 0xDD2F, 4);
    u32 HCrecomputed = neg(oc16(rest, mp));
    ok &= ck("RFC1624 HC' by recomputation", HCrecomputed, 0x0000, 4);
    u32 eqn2 = oc16(oc16(HC, m), neg(mp));
    ok &= ck("RFC1624 Eqn.2 (RFC 1141) result, the wrong value", eqn2, 0xFFFF, 4);
    u32 eqn3 = neg(oc16(oc16(neg(HC), neg(m)), mp));
    ok &= ck("RFC1624 Eqn.3 HC' = ~(~HC + ~m + m')", eqn3, 0x0000, 4);
    u32 eqn4 = sub16(sub16(HC, neg(m)), mp);        // HC - ~m - m' with end-around borrow
    ok &= ck("RFC1624 Eqn.4 HC' = HC - ~m - m'", eqn4, 0x0000, 4);
    ok &= ck("RFC1624 verification 0xCD7A + 0x3285 + 0xFFFF", oc16(oc16(rest, mp), 0xFFFF), 0xFFFF, 4);
    ok &= ck("RFC1624 verification 0xCD7A + 0x3285 + 0x0000", oc16(oc16(rest, mp), 0x0000), 0xFFFF, 4);
    pstr(ok ? "PASS\n" : "FAIL\n");
    return ok ? 0 : 1;
}
