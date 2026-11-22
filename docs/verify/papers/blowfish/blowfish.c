// Blowfish from Schneier, "Description of a New Variable-Length Key, 64-Bit Block Cipher (Blowfish)" (FSE 1993), written from the
// paper's description. The initial P-array and S-boxes are the hexadecimal digits of pi, as the paper specifies; they are computed
// here (Machin's formula, big-integer fixed point) rather than copied. Checks the 34 ECB, 24 variable-key-length, and CBC/CFB64/OFB64
// known-answer values of "Test vectors by Eric Young" as published on the author's site. Exit 0 only if everything matches.
#include "ounce_paper.h"

// Vectors: data copied as strings from https://www.schneier.com/wp-content/uploads/2015/12/vectors-2.txt (fetched 2026-11-19).
static const char *ECB[][3] = {
    {"0000000000000000", "0000000000000000", "4EF997456198DD78"},
    {"FFFFFFFFFFFFFFFF", "FFFFFFFFFFFFFFFF", "51866FD5B85ECB8A"},
    {"3000000000000000", "1000000000000001", "7D856F9A613063F2"},
    {"1111111111111111", "1111111111111111", "2466DD878B963C9D"},
    {"0123456789ABCDEF", "1111111111111111", "61F9C3802281B096"},
    {"1111111111111111", "0123456789ABCDEF", "7D0CC630AFDA1EC7"},
    {"0000000000000000", "0000000000000000", "4EF997456198DD78"},
    {"FEDCBA9876543210", "0123456789ABCDEF", "0ACEAB0FC6A0A28D"},
    {"7CA110454A1A6E57", "01A1D6D039776742", "59C68245EB05282B"},
    {"0131D9619DC1376E", "5CD54CA83DEF57DA", "B1B8CC0B250F09A0"},
    {"07A1133E4A0B2686", "0248D43806F67172", "1730E5778BEA1DA4"},
    {"3849674C2602319E", "51454B582DDF440A", "A25E7856CF2651EB"},
    {"04B915BA43FEB5B6", "42FD443059577FA2", "353882B109CE8F1A"},
    {"0113B970FD34F2CE", "059B5E0851CF143A", "48F4D0884C379918"},
    {"0170F175468FB5E6", "0756D8E0774761D2", "432193B78951FC98"},
    {"43297FAD38E373FE", "762514B829BF486A", "13F04154D69D1AE5"},
    {"07A7137045DA2A16", "3BDD119049372802", "2EEDDA93FFD39C79"},
    {"04689104C2FD3B2F", "26955F6835AF609A", "D887E0393C2DA6E3"},
    {"37D06BB516CB7546", "164D5E404F275232", "5F99D04F5B163969"},
    {"1F08260D1AC2465E", "6B056E18759F5CCA", "4A057A3B24D3977B"},
    {"584023641ABA6176", "004BD6EF09176062", "452031C1E4FADA8E"},
    {"025816164629B007", "480D39006EE762F2", "7555AE39F59B87BD"},
    {"49793EBC79B3258F", "437540C8698F3CFA", "53C55F9CB49FC019"},
    {"4FB05E1515AB73A7", "072D43A077075292", "7A8E7BFA937E89A3"},
    {"49E95D6D4CA229BF", "02FE55778117F12A", "CF9C5D7A4986ADB5"},
    {"018310DC409B26D6", "1D9D5C5018F728C2", "D1ABB290658BC778"},
    {"1C587F1C13924FEF", "305532286D6F295A", "55CB3774D13EF201"},
    {"0101010101010101", "0123456789ABCDEF", "FA34EC4847B268B2"},
    {"1F1F1F1F0E0E0E0E", "0123456789ABCDEF", "A790795108EA3CAE"},
    {"E0FEE0FEF1FEF1FE", "0123456789ABCDEF", "C39E072D9FAC631D"},
    {"0000000000000000", "FFFFFFFFFFFFFFFF", "014933E0CDAFF6E4"},
    {"FFFFFFFFFFFFFFFF", "0000000000000000", "F21E9A77B71C49BC"},
    {"0123456789ABCDEF", "0000000000000000", "245946885754369A"},
    {"FEDCBA9876543210", "FFFFFFFFFFFFFFFF", "6B5C5A9C5D9E0A5A"},
};
static const char *SETKEY[][2] = {  /* key prefix of FEDCBA9876543210-... table, ciphertext of FEDCBA9876543210 */
    {"F0", "F9AD597C49DB005E"},
    {"F0E1", "E91D21C1D961A6D6"},
    {"F0E1D2", "E9C2B70A1BC65CF3"},
    {"F0E1D2C3", "BE1E639408640F05"},
    {"F0E1D2C3B4", "B39E44481BDB1E6E"},
    {"F0E1D2C3B4A5", "9457AA83B1928C0D"},
    {"F0E1D2C3B4A596", "8BB77032F960629D"},
    {"F0E1D2C3B4A59687", "E87A244E2CC85E82"},
    {"F0E1D2C3B4A5968778", "15750E7A4F4EC577"},
    {"F0E1D2C3B4A596877869", "122BA70B3AB64AE0"},
    {"F0E1D2C3B4A5968778695A", "3A833C9AFFC537F6"},
    {"F0E1D2C3B4A5968778695A4B", "9409DA87A90F6BF2"},
    {"F0E1D2C3B4A5968778695A4B3C", "884F80625060B8B4"},
    {"F0E1D2C3B4A5968778695A4B3C2D", "1F85031C19E11968"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E", "79D9373A714CA34F"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E0F", "93142887EE3BE15C"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E0F00", "03429E838CE2D14B"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E0F0011", "A4299E27469FF67B"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E0F001122", "AFD5AED1C1BC96A8"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E0F00112233", "10851C0E3858DA9F"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E0F0011223344", "E6F51ED79B9DB21F"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E0F001122334455", "64A6E14AFD36B46F"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E0F00112233445566", "80C7D7D45A5479AD"},
    {"F0E1D2C3B4A5968778695A4B3C2D1E0F0011223344556677", "05044B62FA52D080"},
};

// Hex helper: parse a hex string into bytes, return byte count.
static int hexbytes(const char *s, u8 *out) {
    int n = 0;
    for (; s[0] && s[1]; s += 2) {
        int v = 0;
        for (int i = 0; i < 2; i++) { char c = s[i]; v = v * 16 + (c <= '9' ? c - '0' : c - 'A' + 10); }
        out[n++] = (u8)v;
    }
    return n;
}

// ---- Pi, as the paper specifies the initial P-array and S-boxes: the fractional hex digits of pi, in order. ----
// 18 + 4*256 = 1042 words. Computed here with Machin's formula pi = 16 atan(1/5) - 4 atan(1/239) in fixed point:
// limb 0 is the integer part, limbs 1..NL the fraction (32 bits each, most significant first). Guard limbs absorb rounding.
#define NL 1048
#define NW 1042
static u32 pi_fx[NL + 1], term[NL + 1], tmp[NL + 1];

static void bn_div(u32 *a, u32 d) {            // a /= d (small d), most significant limb first
    u64 r = 0;
    for (int i = 0; i <= NL; i++) { u64 cur = (r << 32) | a[i]; a[i] = (u32)(cur / d); r = cur % d; }
}
static void bn_add(u32 *a, const u32 *b) { u64 c = 0; for (int i = NL; i >= 0; i--) { c += (u64)a[i] + b[i]; a[i] = (u32)c; c >>= 32; } }
static void bn_sub(u32 *a, const u32 *b) { i64 br = 0; for (int i = NL; i >= 0; i--) { i64 v = (i64)a[i] - b[i] - br; br = v < 0; a[i] = (u32)v; } }
static void bn_mul_small(u32 *a, u32 m) { u64 c = 0; for (int i = NL; i >= 0; i--) { c += (u64)a[i] * m; a[i] = (u32)c; c >>= 32; } }
// acc += sign * mult * atan(1/x)
static void atan_inv(u32 *acc, u32 x, u32 mult, int sign) {
    u32 sum[NL + 1];
    for (int i = 0; i <= NL; i++) { sum[i] = 0; term[i] = 0; }
    term[0] = 1; bn_div(term, x);                      // 1/x
    u32 x2 = x * x;
    for (u32 k = 0;; k++) {
        int zero = 1; for (int i = 0; i <= NL; i++) if (term[i]) { zero = 0; break; }
        if (zero) break;
        for (int i = 0; i <= NL; i++) tmp[i] = term[i];
        bn_div(tmp, 2 * k + 1);
        if (k & 1) bn_sub(sum, tmp); else bn_add(sum, tmp);
        bn_div(term, x2);
    }
    bn_mul_small(sum, mult);
    if (sign > 0) bn_add(acc, sum); else bn_sub(acc, sum);
}

static u32 P[18], S[4][256];
static const u32 *PIW;                                  // fractional words of pi, set by init_pi
static void init_pi(void) {
    for (int i = 0; i <= NL; i++) pi_fx[i] = 0;
    atan_inv(pi_fx, 5, 16, +1);
    atan_inv(pi_fx, 239, 4, -1);
    PIW = pi_fx + 1;                                    // pi_fx[0] must be 3
}
static void load_pi(void) {
    int n = 0;
    for (int i = 0; i < 18; i++) P[i] = PIW[n++];
    for (int b = 0; b < 4; b++) for (int i = 0; i < 256; i++) S[b][i] = PIW[n++];
}

// ---- Blowfish (Schneier 1993): 16-round Feistel, F(x) = ((S0[a] + S1[b]) ^ S2[c]) + S3[d] ----
static u32 F(u32 x) { return ((S[0][x >> 24] + S[1][(x >> 16) & 255]) ^ S[2][(x >> 8) & 255]) + S[3][x & 255]; }
#ifndef BF_ROUNDS
#define BF_ROUNDS 16
#endif
static void bf_enc(u32 *l, u32 *r) {
    u32 L = *l, R = *r;
    for (int i = 0; i < BF_ROUNDS; i++) { L ^= P[i]; R ^= F(L); u32 t = L; L = R; R = t; }
    u32 t = L; L = R; R = t;
    R ^= P[16]; L ^= P[17];
    *l = L; *r = R;
}
static void bf_key(const u8 *key, int klen) {
    load_pi();
    int j = 0;
    for (int i = 0; i < 18; i++) {
        u32 d = 0;
        for (int k = 0; k < 4; k++) { d = (d << 8) | key[j]; j = (j + 1) % klen; }
        P[i] ^= d;
    }
    u32 l = 0, r = 0;
    for (int i = 0; i < 18; i += 2) { bf_enc(&l, &r); P[i] = l; P[i + 1] = r; }
    for (int b = 0; b < 4; b++) for (int i = 0; i < 256; i += 2) { bf_enc(&l, &r); S[b][i] = l; S[b][i + 1] = r; }
}
static u32 be32(const u8 *p) { return ((u32)p[0] << 24) | ((u32)p[1] << 16) | ((u32)p[2] << 8) | p[3]; }
static void put32(u8 *p, u32 v) { p[0] = (u8)(v >> 24); p[1] = (u8)(v >> 16); p[2] = (u8)(v >> 8); p[3] = (u8)v; }
static void enc_block(const u8 *in, u8 *out) { u32 l = be32(in), r = be32(in + 4); bf_enc(&l, &r); put32(out, l); put32(out + 4, r); }

int paper_main(void) {
    int bad = 0, total = 0;
    init_pi();
    pcheck_u64("pi integer part", pi_fx[0], 3, 1); if (pi_fx[0] != 3) bad++;
    u8 key[24], pt[8], want[8], got[8];

    int ok = 0, n = (int)(sizeof ECB / sizeof ECB[0]);
    for (int i = 0; i < n; i++) {
        int kl = hexbytes(ECB[i][0], key); hexbytes(ECB[i][1], pt); hexbytes(ECB[i][2], want);
        bf_key(key, kl); enc_block(pt, got);
        int m = 1; for (int j = 0; j < 8; j++) if (got[j] != want[j]) m = 0;
        if (!m) { pcheck_bytes("ecb vector", got, want, 8); bad++; } else ok++;
        total++;
    }
    pstr("Eric Young ECB vectors: "); pdec(ok); pstr(" of "); pdec(n); pstr(" match\n");

    // set_key test: key = first k bytes of F0E1D2C3B4A5968778695A4B3C2D1E0F0011223344556677, plaintext FEDCBA9876543210
    static const u8 KT[24] = {0xF0,0xE1,0xD2,0xC3,0xB4,0xA5,0x96,0x87,0x78,0x69,0x5A,0x4B,0x3C,0x2D,0x1E,0x0F,0x00,0x11,0x22,0x33,0x44,0x55,0x66,0x77};
    ok = 0; n = (int)(sizeof SETKEY / sizeof SETKEY[0]);
    hexbytes("FEDCBA9876543210", pt);
    for (int i = 0; i < n; i++) {
        int kl = hexbytes(SETKEY[i][0], key);
        int same = 1; for (int j = 0; j < kl; j++) if (key[j] != KT[j]) same = 0;
        if (!same || kl != i + 1) { pstr("FAIL set_key table key mismatch\n"); bad++; }
        hexbytes(SETKEY[i][1], want);
        bf_key(key, kl); enc_block(pt, got);
        int m = 1; for (int j = 0; j < 8; j++) if (got[j] != want[j]) m = 0;
        if (!m) { pcheck_bytes("set_key vector", got, want, 8); bad++; } else ok++;
    }
    pstr("set_key vectors (key lengths 1..24): "); pdec(ok); pstr(" of "); pdec(n); pstr(" match\n");

    // chaining modes: key 0123456789ABCDEFF0E1D2C3B4A59687, iv FEDCBA9876543210, 29 data bytes
    u8 k16[16], iv[8], data[32], ct[32], wantct[32];
    hexbytes("0123456789ABCDEFF0E1D2C3B4A59687", k16); bf_key(k16, 16);
    int dl = hexbytes("37363534333231204E6F77206973207468652074696D6520666F722000", data);
    for (int i = dl; i < 32; i++) data[i] = 0;
    // CBC over 32 bytes (zero padded)
    hexbytes("FEDCBA9876543210", iv);
    u8 prev[8]; for (int i = 0; i < 8; i++) prev[i] = iv[i];
    for (int b = 0; b < 4; b++) {
        u8 x[8]; for (int i = 0; i < 8; i++) x[i] = data[b * 8 + i] ^ prev[i];
        enc_block(x, prev); for (int i = 0; i < 8; i++) ct[b * 8 + i] = prev[i];
    }
    hexbytes("6B77B4D63006DEE605B156E27403979358DEB9E7154616D959F1652BD5FF92CC", wantct);
    int c_cbc = pcheck_bytes("cbc 32 bytes", ct, wantct, 32);
    // CFB64 over 29 bytes
    u8 reg[8], ks[8]; for (int i = 0; i < 8; i++) reg[i] = iv[i];
    for (int i = 0, pos = 0; i < dl; i++) {
        if (pos == 0) enc_block(reg, ks);
        ct[i] = data[i] ^ ks[pos]; reg[pos] = ct[i]; pos = (pos + 1) & 7;
    }
    hexbytes("E73214A2822139CAF26ECF6D2EB9E76E3DA3DE04D1517200519D57A6C3", wantct);
    int c_cfb = pcheck_bytes("cfb64 29 bytes", ct, wantct, 29);
    // OFB64 over 29 bytes
    for (int i = 0; i < 8; i++) reg[i] = iv[i];
    for (int i = 0, pos = 0; i < dl; i++) {
        if (pos == 0) { enc_block(reg, ks); for (int j = 0; j < 8; j++) reg[j] = ks[j]; }
        ct[i] = data[i] ^ ks[pos]; pos = (pos + 1) & 7;
    }
    hexbytes("E73214A2822139CA62B343CC5B65587310DD908D0C241B2263C2CF80DA", wantct);
    int c_ofb = pcheck_bytes("ofb64 29 bytes", ct, wantct, 29);
    if (!c_cbc) bad++;
    if (!c_cfb) bad++;
    if (!c_ofb) bad++;
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
