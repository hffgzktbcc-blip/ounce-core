// Twofish from Schneier, Kelsey, Whiting, Wagner, Hall, Ferguson, "Twofish: A 128-Bit Block Cipher" (AES submission, June 15, 1998),
// written from the paper's section 4 (h, g, q0/q1, MDS and RS matrices, key schedule, round function). The 4-bit permutations of q0/q1
// (data) are copied by script from the paper's text; the test data is the authors' own file (intermediate-value test for the three key
// sizes, and the listed ecb_tbl entries). Exit 0 only if everything matches.
#include "ounce_paper.h"

// Twofish paper section 4.3.5: the 4-bit permutations t0..t3 of q0 (index 0) and q1 (index 1)
static const u8 QT[2][4][16] = {
  {
    {0x8,0x1,0x7,0xD,0x6,0xF,0x3,0x2,0x0,0xB,0x5,0x9,0xE,0xC,0xA,0x4},
    {0xE,0xC,0xB,0x8,0x1,0x2,0x3,0x5,0xF,0x4,0xA,0x6,0x7,0x0,0x9,0xD},
    {0xB,0xA,0x5,0xE,0x6,0xD,0x9,0x0,0xC,0x8,0xF,0x3,0x2,0x4,0x7,0x1},
    {0xD,0x7,0xF,0x4,0x1,0x2,0x6,0xE,0x9,0xB,0x3,0x0,0x8,0x5,0xC,0xA},
  },
  {
    {0x2,0x8,0xB,0xD,0xF,0x7,0x6,0xE,0x3,0x1,0x9,0x4,0x0,0xA,0xC,0x5},
    {0x1,0xE,0x2,0xB,0x4,0xC,0x3,0x7,0x6,0xD,0xA,0x5,0xF,0x9,0x0,0x8},
    {0x4,0xC,0x7,0x5,0x1,0x6,0x9,0xA,0x0,0xE,0xD,0x8,0x2,0xB,0x3,0xF},
    {0xB,0x9,0x5,0x1,0xC,0x3,0xD,0xE,0x6,0x4,0x7,0xF,0x2,0x0,0x8,0xA},
  },
};
// Authors' intermediate-value test (ecb_ival): for key sizes 128/192/256 with a zero plaintext.
struct ival { int bits; const char *key; const char *S[4]; const char *K[40]; const char *pt, *ct; const char *R[19][4]; const char *t[19][2]; };
static const struct ival IVAL[3] = {
{ 128, "00000000000000000000000000000000",
 { "00000000","00000000","","" },
 { "52C54DDE","11F0626D","7CAC9D4A","4D1B4AAA","B7B83A10","1E7D0BEB","EE9C341F","CFE14BE4","F98FFEF9","9C5B3C17","15A48310","342A4D81","424D89FE","C14724A7","311B834C","FDE87320","3302778F","26CD67B4","7A6C6362","C2BAF60E","3411B994","D972C87F","84ADB1EA","A7DEE434","54D2960F","A2F7CAA8","A6B8FF8C","8014C425","6A748D1C","EDBAF720","928EF78C","0338EE13","9949D6BE","C8314176","07C07D68","ECAE7EA7","1FE71844","85C05C89","F298311E","696EA672" },
 "00000000000000000000000000000000", "9F589F5CF6122C32B6BFEC2F2AE8C35A",
 { {"00000000","00000000","00000000","00000000"},
   {"52C54DDE","11F0626D","7CAC9D4A","4D1B4AAA"},
   {"52C54DDE","11F0626D","C38DCAA4","7A0A91B6"},
   {"55A538DE","5C5A4DB6","C38DCAA4","7A0A91B6"},
   {"55A538DE","5C5A4DB6","899063BD","893E49A9"},
   {"2AE61A96","84BC42D3","899063BD","893E49A9"},
   {"2AE61A96","84BC42D3","F14F2618","821B5F36"},
   {"0FFE0AD1","D6B87B70","F14F2618","821B5F36"},
   {"0FFE0AD1","D6B87B70","CD0D38A1","C069BD9B"},
   {"A85CE579","DE2661CE","CD0D38A1","C069BD9B"},
   {"A85CE579","DE2661CE","7A39754C","973ABD2A"},
   {"013077D7","B3528BA1","7A39754C","973ABD2A"},
   {"013077D7","B3528BA1","D57933FD","F8EA8B1B"},
   {"64F0EAA1","DA27090C","D57933FD","F8EA8B1B"},
   {"64F0EAA1","DA27090C","F64F1005","99149A52"},
   {"B0681C46","606D0273","F64F1005","99149A52"},
   {"B0681C46","606D0273","EB27628F","2C51191D"},
   {"C1708BA9","9522A3CE","EB27628F","2C51191D"},
   {"5C9F589F","322C12F6","2FECBFB6","5AC3E82A"} },
 { {"",""},
   {"",""},
   {"C06D4949","41B9BFC1"},
   {"7C4536B9","67A58299"},
   {"60DAC1A4","2D84C23D"},
   {"607AAEAD","6ED2DBF9"},
   {"067D0B49","318EACB4"},
   {"58554EDB","62585CF7"},
   {"839B0017","B3A89DB0"},
   {"E9BC6975","F0DDA4C3"},
   {"54687CDF","9044BF4B"},
   {"77FC927F","8B8678CC"},
   {"E3C81108","828E7493"},
   {"B33C25D6","83068533"},
   {"A0AA2F81","FFF30DB7"},
   {"114C17C5","EB143CFF"},
   {"677DA87D","989D1459"},
   {"9357B338","AC9926BF"},
   {"",""} } },
{ 192, "0123456789ABCDEFFEDCBA98765432100011223344556677",
 { "B89FF6F2","B255BC4B","45661061","" },
 { "38394A24","C36D1175","E802528F","219BFEB4","B9141AB4","BD3E70CD","AF609383","FD36908A","03EFB931","1D2EE7EC","A7489D55","6E44B6E8","714AD667","653AD51F","B6315B66","B27C05AF","A06C8140","9853D419","4016E346","8D1C0DD4","F05480BE","B6AF816F","2D7DC789","45B7BD3A","57F8A163","2BEFDA69","26AE7271","C2900D79","ED323794","3D3FFD80","5DE68E49","9C3D2478","DF326FE3","5911F70D","C229F13B","B1364772","4235364D","0CEC363A","57C8DD1F","6A1AD61E" },
 "00000000000000000000000000000000", "CFD1D2E5A9BE9CDF501F13B892BD2248",
 { {"00000000","00000000","00000000","00000000"},
   {"38394A24","C36D1175","E802528F","219BFEB4"},
   {"38394A24","C36D1175","9C263D67","5E68BE8F"},
   {"C8F5099F","0C4B8F53","9C263D67","5E68BE8F"},
   {"C8F5099F","0C4B8F53","69948F5E","E67C030F"},
   {"07633866","59421079","69948F5E","E67C030F"},
   {"07633866","59421079","C015BE79","149B9CEC"},
   {"A042B99D","709EF54B","C015BE79","149B9CEC"},
   {"A042B99D","709EF54B","0CD39FA6","B250BEDA"},
   {"F7B097FA","9E5C4FF7","0CD39FA6","B250BEDA"},
   {"F7B097FA","9E5C4FF7","77FC8B29","CC2B3F88"},
   {"A279C718","421A8D38","77FC8B29","CC2B3F88"},
   {"A279C718","421A8D38","5B1A0904","12FEBF99"},
   {"E4409C22","702548A2","5B1A0904","12FEBF99"},
   {"E4409C22","702548A2","5DDAA2A1","EFB2F051"},
   {"8561A604","825D2480","5DDAA2A1","EFB2F051"},
   {"8561A604","825D2480","5CC6CB7B","62A2CE64"},
   {"17738CD3","B5142D18","5CC6CB7B","62A2CE64"},
   {"E5D2D1CF","DF9CBEA9","B8131F50","4822BD92"} },
 { {"",""},
   {"",""},
   {"988C8223","33D1ECEC"},
   {"E8C880BC","19C23B0A"},
   {"C615F1F6","17AE5B7E"},
   {"90AB32AA","7F56EB43"},
   {"52971E00","F6BC546D"},
   {"DAA00849","2D2F5FCE"},
   {"EE03FB5B","FB5A051C"},
   {"09A1B597","18041948"},
   {"99C9694E","F1687F43"},
   {"5D174956","2F7D5E04"},
   {"5BC40012","78D2617B"},
   {"C251B3CE","4AC0BD46"},
   {"91BC2070","6FC0BBF3"},
   {"A7D24F8E","84878F62"},
   {"93690387","0EB8FA83"},
   {"5FE8370B","F3D5AB78"},
   {"",""} } },
{ 256, "0123456789ABCDEFFEDCBA987654321000112233445566778899AABBCCDDEEFF",
 { "B89FF6F2","B255BC4B","45661061","8E4447F7" },
 { "5EC769BF","44D13C60","76CD39B1","16750474","349C294B","EC21F6D6","4FBD10B4","578DA0ED","C3479695","9B6958FB","6A7FBC4E","0BF1830B","61B5E0FB","D78D9730","7C6CF0C4","2F9109C8","E69EA8D1","ED99BDFF","35DC0BBD","A03E5018","FB18EA0B","38BD43D3","76191781","37A9A0D3","72427BEA","911CC0B8","F1689449","71009CA9","B6363E89","494D9855","590BBC63","F95A28B5","FB72B4E1","2A43505C","BFD34176","5C133D12","3A9247F7","9A3331DD","EE7515E6","F0D54DCD" },
 "00000000000000000000000000000000", "37527BE0052334B89F0CFCCAE87CFA20",
 { {"00000000","00000000","00000000","00000000"},
   {"5EC769BF","44D13C60","76CD39B1","16750474"},
   {"5EC769BF","44D13C60","D38B6C9F","A23B7169"},
   {"99424DFF","FBC14BFC","D38B6C9F","A23B7169"},
   {"99424DFF","FBC14BFC","698BE047","6A997290"},
   {"2C125DD7","5A526278","698BE047","6A997290"},
   {"2C125DD7","5A526278","E35CD910","7CB57D06"},
   {"D5178F25","00D35CC5","E35CD910","7CB57D06"},
   {"D5178F25","00D35CC5","D8447F91","65C2BD96"},
   {"FF92E109","DF621C97","D8447F91","65C2BD96"},
   {"FF92E109","DF621C97","28BFEFF5","D45666FB"},
   {"BB79AD2E","AA410F41","28BFEFF5","D45666FB"},
   {"BB79AD2E","AA410F41","6576A3ED","BFF8215E"},
   {"4A6BBAFF","439F4766","6576A3ED","BFF8215E"},
   {"4A6BBAFF","439F4766","F7186836","04CA5304"},
   {"CBD3C29D","BC31FEBE","F7186836","04CA5304"},
   {"CBD3C29D","BC31FEBE","D4E77B7C","5415D5D3"},
   {"85411C2B","7777DC05","D4E77B7C","5415D5D3"},
   {"E07B5237","B8342305","CAFC0C9F","20FA7CE8"} },
 { {"",""},
   {"",""},
   {"29C0736C","E4D3D68D"},
   {"9D16BBB3","64AD7A3F"},
   {"E66B9D19","B87B2DFD"},
   {"0BB41F61","3945E62C"},
   {"D5397903","F35A3092"},
   {"8C8927A1","C3D8103E"},
   {"4D8B7489","0B2FC79F"},
   {"C1176720","F301CE95"},
   {"9F3BEC03","77BD388E"},
   {"8C6DB451","0B8B72BA"},
   {"8A317EF8","A1EAEAAE"},
   {"8F8307AA","472014C3"},
   {"CEB0BBE1","C12302BE"},
   {"5CF5C93C","C1033512"},
   {"853A6BB2","9F09EB26"},
   {"877AF61D","4B61EEC7"},
   {"",""} } },
};
// Authors' ecb_tbl chain: entry I has key = the ciphertext of earlier entries, plaintext = previous ciphertext. {key, plaintext, ciphertext}
struct trec { int i; const char *k, *p, *c; };
static const struct trec TBL128[12] = {
{1,"00000000000000000000000000000000","00000000000000000000000000000000","9F589F5CF6122C32B6BFEC2F2AE8C35A"},
{2,"00000000000000000000000000000000","9F589F5CF6122C32B6BFEC2F2AE8C35A","D491DB16E7B1C39E86CB086B789F5419"},
{3,"9F589F5CF6122C32B6BFEC2F2AE8C35A","D491DB16E7B1C39E86CB086B789F5419","019F9809DE1711858FAAC3A3BA20FBC3"},
{4,"D491DB16E7B1C39E86CB086B789F5419","019F9809DE1711858FAAC3A3BA20FBC3","6363977DE839486297E661C6C9D668EB"},
{5,"019F9809DE1711858FAAC3A3BA20FBC3","6363977DE839486297E661C6C9D668EB","816D5BD0FAE35342BF2A7412C246F752"},
{6,"6363977DE839486297E661C6C9D668EB","816D5BD0FAE35342BF2A7412C246F752","5449ECA008FF5921155F598AF4CED4D0"},
{7,"816D5BD0FAE35342BF2A7412C246F752","5449ECA008FF5921155F598AF4CED4D0","6600522E97AEB3094ED5F92AFCBCDD10"},
{8,"5449ECA008FF5921155F598AF4CED4D0","6600522E97AEB3094ED5F92AFCBCDD10","34C8A5FB2D3D08A170D120AC6D26DBFA"},
{9,"6600522E97AEB3094ED5F92AFCBCDD10","34C8A5FB2D3D08A170D120AC6D26DBFA","28530B358C1B42EF277DE6D4407FC591"},
{10,"34C8A5FB2D3D08A170D120AC6D26DBFA","28530B358C1B42EF277DE6D4407FC591","8A8AB983310ED78C8C0ECDE030B8DCA4"},
{48,"137A24CA47CD12BE818DF4D2F4355960","BCA724A54533C6987E14AA827952F921","6B459286F3FFD28D49F15B1581B08E42"},
{49,"BCA724A54533C6987E14AA827952F921","6B459286F3FFD28D49F15B1581B08E42","5D9D4EEFFA9151575524F115815A12E0"},
};
static const struct trec TBL192[12] = {
{1,"000000000000000000000000000000000000000000000000","00000000000000000000000000000000","EFA71F788965BD4453F860178FC19101"},
{2,"000000000000000000000000000000000000000000000000","EFA71F788965BD4453F860178FC19101","88B2B2706B105E36B446BB6D731A1E88"},
{3,"EFA71F788965BD4453F860178FC191010000000000000000","88B2B2706B105E36B446BB6D731A1E88","39DA69D6BA4997D585B6DC073CA341B2"},
{4,"88B2B2706B105E36B446BB6D731A1E88EFA71F788965BD44","39DA69D6BA4997D585B6DC073CA341B2","182B02D81497EA45F9DAACDC29193A65"},
{5,"39DA69D6BA4997D585B6DC073CA341B288B2B2706B105E36","182B02D81497EA45F9DAACDC29193A65","7AFF7A70CA2FF28AC31DD8AE5DAAAB63"},
{6,"182B02D81497EA45F9DAACDC29193A6539DA69D6BA4997D5","7AFF7A70CA2FF28AC31DD8AE5DAAAB63","D1079B789F666649B6BD7D1629F1F77E"},
{7,"7AFF7A70CA2FF28AC31DD8AE5DAAAB63182B02D81497EA45","D1079B789F666649B6BD7D1629F1F77E","3AF6F7CE5BD35EF18BEC6FA787AB506B"},
{8,"D1079B789F666649B6BD7D1629F1F77E7AFF7A70CA2FF28A","3AF6F7CE5BD35EF18BEC6FA787AB506B","AE8109BFDA85C1F2C5038B34ED691BFF"},
{9,"3AF6F7CE5BD35EF18BEC6FA787AB506BD1079B789F666649","AE8109BFDA85C1F2C5038B34ED691BFF","893FD67B98C550073571BD631263FC78"},
{10,"AE8109BFDA85C1F2C5038B34ED691BFF3AF6F7CE5BD35EF1","893FD67B98C550073571BD631263FC78","16434FC9C8841A63D58700B5578E8F67"},
{48,"DEA4F3DA75EC7A8EAC3861A9912402CD5DBE44032769DF54","FB66522C332FCC4C042ABE32FA9E902F","F0AB73301125FA21EF70BE5385FB76B6"},
{49,"FB66522C332FCC4C042ABE32FA9E902FDEA4F3DA75EC7A8E","F0AB73301125FA21EF70BE5385FB76B6","E75449212BEEF9F4A390BD860A640941"},
};
static const struct trec TBL256[12] = {
{1,"0000000000000000000000000000000000000000000000000000000000000000","00000000000000000000000000000000","57FF739D4DC92C1BD7FC01700CC8216F"},
{2,"0000000000000000000000000000000000000000000000000000000000000000","57FF739D4DC92C1BD7FC01700CC8216F","D43BB7556EA32E46F2A282B7D45B4E0D"},
{3,"57FF739D4DC92C1BD7FC01700CC8216F00000000000000000000000000000000","D43BB7556EA32E46F2A282B7D45B4E0D","90AFE91BB288544F2C32DC239B2635E6"},
{4,"D43BB7556EA32E46F2A282B7D45B4E0D57FF739D4DC92C1BD7FC01700CC8216F","90AFE91BB288544F2C32DC239B2635E6","6CB4561C40BF0A9705931CB6D408E7FA"},
{5,"90AFE91BB288544F2C32DC239B2635E6D43BB7556EA32E46F2A282B7D45B4E0D","6CB4561C40BF0A9705931CB6D408E7FA","3059D6D61753B958D92F4781C8640E58"},
{6,"6CB4561C40BF0A9705931CB6D408E7FA90AFE91BB288544F2C32DC239B2635E6","3059D6D61753B958D92F4781C8640E58","E69465770505D7F80EF68CA38AB3A3D6"},
{7,"3059D6D61753B958D92F4781C8640E586CB4561C40BF0A9705931CB6D408E7FA","E69465770505D7F80EF68CA38AB3A3D6","5AB67A5F8539A4A5FD9F0373BA463466"},
{8,"E69465770505D7F80EF68CA38AB3A3D63059D6D61753B958D92F4781C8640E58","5AB67A5F8539A4A5FD9F0373BA463466","DC096BCD99FC72F79936D4C748E75AF7"},
{9,"5AB67A5F8539A4A5FD9F0373BA463466E69465770505D7F80EF68CA38AB3A3D6","DC096BCD99FC72F79936D4C748E75AF7","C5A3E7CEE0F1B7260528A68FB4EA05F2"},
{10,"DC096BCD99FC72F79936D4C748E75AF75AB67A5F8539A4A5FD9F0373BA463466","C5A3E7CEE0F1B7260528A68FB4EA05F2","43D5CEC327B24AB90AD34A79D0469151"},
{48,"2E2158BC3E5FC714C1EEECA0EA696D48D2DED73E59319A8138E0331F0EA149EA","248A7F3528B168ACFDD1386E3F51E30C","431058F4DBC7F734DA4F02F04CC4F459"},
{49,"248A7F3528B168ACFDD1386E3F51E30C2E2158BC3E5FC714C1EEECA0EA696D48","431058F4DBC7F734DA4F02F04CC4F459","37FE26FF1CF66175F5DDF4C33B97A205"},
};

static int hexbytes(const char *s, u8 *out) {
    int n = 0;
    for (; s[0] && s[1]; s += 2) {
        int v = 0;
        for (int i = 0; i < 2; i++) { char c = s[i]; v = v * 16 + (c <= '9' ? c - '0' : c >= 'a' ? c - 'a' + 10 : c - 'A' + 10); }
        out[n++] = (u8)v;
    }
    return n;
}
static u32 hex32(const char *s) { u8 b[4]; hexbytes(s, b); return ((u32)b[0] << 24) | ((u32)b[1] << 16) | ((u32)b[2] << 8) | b[3]; }

// ---- Twofish (Schneier, Kelsey, Whiting, Wagner, Hall, Ferguson 1998), section 4 ----
#ifndef TF_KROT
#define TF_KROT 9              // rotation of the odd expanded-key words
#endif
static u8 q[2][256];
static u8 ror4(u8 b) { return (u8)((b >> 1) | ((b & 1) << 3)); }
static void make_q(void) {
    for (int w = 0; w < 2; w++) for (int x = 0; x < 256; x++) {
        const u8 (*t)[16] = QT[w];
        u8 a0 = (u8)(x >> 4), b0 = (u8)(x & 15);
        u8 a1 = a0 ^ b0, b1 = (u8)(a0 ^ ror4(b0) ^ ((8 * a0) & 15));
        u8 a2 = t[0][a1], b2 = t[1][b1];
        u8 a3 = a2 ^ b2, b3 = (u8)(a2 ^ ror4(b2) ^ ((8 * a2) & 15));
        u8 a4 = t[2][a3], b4 = t[3][b3];
        q[w][x] = (u8)(16 * b4 + a4);
    }
}
static u8 gmul(u8 a, u8 b, u32 poly) {          // multiplication in GF(2)[x]/poly (poly has the x^8 term)
    u32 r = 0, x = a;
    for (int i = 0; i < 8; i++) { if (b & (1 << i)) r ^= x << 0; x <<= 1; if (x & 0x100) x ^= poly; r &= 0xff; }
    return (u8)r;
}
#define VPOLY 0x169            // v(x) = x^8 + x^6 + x^5 + x^3 + 1, for the MDS matrix
#define WPOLY 0x14D            // w(x) = x^8 + x^6 + x^3 + x^2 + 1, for the RS matrix
static const u8 MDS[4][4] = {{0x01, 0xEF, 0x5B, 0x5B}, {0x5B, 0xEF, 0xEF, 0x01}, {0xEF, 0x5B, 0x01, 0xEF}, {0xEF, 0x01, 0xEF, 0x5B}};
static const u8 RS[4][8] = {{0x01, 0xA4, 0x55, 0x87, 0x5A, 0x58, 0xDB, 0x9E}, {0xA4, 0x56, 0x82, 0xF3, 0x1E, 0xC6, 0x68, 0xE5},
                            {0x02, 0xA1, 0xFC, 0xC1, 0x47, 0xAE, 0x3D, 0x19}, {0xA4, 0x55, 0x87, 0x5A, 0x58, 0xDB, 0x9E, 0x03}};
static u32 rol(u32 x, int n) { return (x << n) | (x >> (32 - n)); }
static u32 ror(u32 x, int n) { return (x >> n) | (x << (32 - n)); }
// h(X, L) with a list of k words (section 4.3.2)
static u32 h(u32 X, const u32 *L, int k) {
    u8 y[4], l[4][4];
    for (int i = 0; i < k; i++) for (int j = 0; j < 4; j++) l[i][j] = (u8)(L[i] >> (8 * j));
    for (int j = 0; j < 4; j++) y[j] = (u8)(X >> (8 * j));
    if (k == 4) { y[0] = q[1][y[0]] ^ l[3][0]; y[1] = q[0][y[1]] ^ l[3][1]; y[2] = q[0][y[2]] ^ l[3][2]; y[3] = q[1][y[3]] ^ l[3][3]; }
    if (k >= 3) { y[0] = q[1][y[0]] ^ l[2][0]; y[1] = q[1][y[1]] ^ l[2][1]; y[2] = q[0][y[2]] ^ l[2][2]; y[3] = q[0][y[3]] ^ l[2][3]; }
    u8 z[4];
    z[0] = q[1][q[0][q[0][y[0]] ^ l[1][0]] ^ l[0][0]];
    z[1] = q[0][q[0][q[1][y[1]] ^ l[1][1]] ^ l[0][1]];
    z[2] = q[1][q[1][q[0][y[2]] ^ l[1][2]] ^ l[0][2]];
    z[3] = q[0][q[1][q[1][y[3]] ^ l[1][3]] ^ l[0][3]];
    u32 Z = 0;
    for (int r = 0; r < 4; r++) { u8 v = 0; for (int c = 0; c < 4; c++) v ^= gmul(MDS[r][c], z[c], VPOLY); Z |= (u32)v << (8 * r); }
    return Z;
}
static u32 Kw[40], Sw[4], SL[4];       // SL is S in the paper's reversed order, used as h's list in g
static int KK;
static void tf_key(const u8 *key, int klen) {
    int k = klen / 8; KK = k;
    u32 M[8], Me[4], Mo[4];
    for (int i = 0; i < 2 * k; i++) M[i] = (u32)key[4 * i] | ((u32)key[4 * i + 1] << 8) | ((u32)key[4 * i + 2] << 16) | ((u32)key[4 * i + 3] << 24);
    for (int i = 0; i < k; i++) { Me[i] = M[2 * i]; Mo[i] = M[2 * i + 1]; }
    for (int i = 0; i < k; i++) {
        u32 s = 0;
        for (int r = 0; r < 4; r++) { u8 v = 0; for (int c = 0; c < 8; c++) v ^= gmul(RS[r][c], key[8 * i + c], WPOLY); s |= (u32)v << (8 * r); }
        Sw[i] = s;
    }
    for (int i = 0; i < k; i++) SL[i] = Sw[k - 1 - i];
    for (int i = 0; i < 20; i++) {
        u32 A = h(2 * i * 0x01010101u, Me, k), B = rol(h((2 * i + 1) * 0x01010101u, Mo, k), 8);
        Kw[2 * i] = A + B; Kw[2 * i + 1] = rol(A + 2 * B, TF_KROT);
    }
}
static u32 g(u32 X) { return h(X, SL, KK); }
static u32 ld(const u8 *p) { return (u32)p[0] | ((u32)p[1] << 8) | ((u32)p[2] << 16) | ((u32)p[3] << 24); }
static void st(u8 *p, u32 v) { for (int i = 0; i < 4; i++) p[i] = (u8)(v >> (8 * i)); }
// Encrypts one block; if trace is non-null it receives the 19 states R[-1..17] as 4 words each and the t0/t1 of rounds 1..16.
static void tf_enc(const u8 *in, u8 *out, u32 (*tr)[4], u32 (*tt)[2]) {
    u32 x[4];
    for (int i = 0; i < 4; i++) x[i] = ld(in + 4 * i);
    if (tr) for (int i = 0; i < 4; i++) tr[0][i] = x[i];
    for (int i = 0; i < 4; i++) x[i] ^= Kw[i];
    if (tr) for (int i = 0; i < 4; i++) tr[1][i] = x[i];
    for (int r = 0; r < 16; r++) {
        int a = (r & 1) * 2, b = 2 - a;                   // F reads words a, a+1 and updates words b, b+1 (in place, no swap)
        u32 t0 = g(x[a]), t1 = g(rol(x[a + 1], 8));
        u32 f0 = t0 + t1 + Kw[2 * r + 8], f1 = t0 + 2 * t1 + Kw[2 * r + 9];
        x[b] = ror(x[b] ^ f0, 1); x[b + 1] = rol(x[b + 1], 1) ^ f1;
        if (tr) { for (int i = 0; i < 4; i++) tr[r + 2][i] = x[i]; tt[r + 2][0] = t0; tt[r + 2][1] = t1; }
    }
    u32 c[4] = {x[2] ^ Kw[4], x[3] ^ Kw[5], x[0] ^ Kw[6], x[1] ^ Kw[7]};
    if (tr) for (int i = 0; i < 4; i++) tr[18][i] = c[i];
    for (int i = 0; i < 4; i++) st(out + 4 * i, c[i]);
}

static int total, bad;
static void cmp(u32 got, u32 want, const char *what, int a, int b) {
    total++;
    if (got != want) { bad++; if (bad <= 6) { pstr("FAIL "); pstr(what); pstr(" "); pdec(a); pstr("/"); pdec(b); pstr(": "); phex(got, 8); pstr(" expected "); phex(want, 8); pstr("\n"); } }
}
static int eq16(const u8 *a, const u8 *b) { for (int i = 0; i < 16; i++) if (a[i] != b[i]) return 0; return 1; }

int paper_main(void) {
    make_q();
    // 1. intermediate-value test for the three key sizes
    for (int v = 0; v < 3; v++) {
        const struct ival *E = &IVAL[v];
        u8 key[32], pt[16], ct[16], want[16]; u32 tr[19][4], tt[19][2];
        int klen = hexbytes(E->key, key); hexbytes(E->pt, pt); hexbytes(E->ct, want);
        int t0 = total, b0 = bad;
        tf_key(key, klen); tf_enc(pt, ct, tr, tt);
        for (int i = 0; i < KK; i++) cmp(Sw[i], hex32(E->S[i]), "S-box key word", E->bits, i);
        for (int i = 0; i < 40; i++) cmp(Kw[i], hex32(E->K[i]), "subkey", E->bits, i);
        for (int i = 0; i < 19; i++) for (int j = 0; j < 4; j++) cmp(tr[i][j], hex32(E->R[i][j]), "round state", E->bits, i * 4 + j);
        for (int i = 2; i < 18; i++) { cmp(tt[i][0], hex32(E->t[i][0]), "t0", E->bits, i); cmp(tt[i][1], hex32(E->t[i][1]), "t1", E->bits, i); }
        total++; if (!eq16(ct, want)) { bad++; pcheck_bytes("ciphertext", ct, want, 16); }
        pstr("Twofish-"); pdec(E->bits); pstr(" intermediate-value test: ciphertext "); phex_bytes(ct, 16); pstr(", "); pdec(total - t0); pstr(" published values compared, ");
        pdec(bad - b0); pstr(" mismatches\n");
    }
    // 2. the published ecb_tbl chain entries (I = 1..10, 48, 49 for each key size)
    static const int BITS[3] = {128, 192, 256};
    for (int s = 0; s < 3; s++) {
        const struct trec *T = s == 0 ? TBL128 : s == 1 ? TBL192 : TBL256;
        int n = s == 0 ? (int)(sizeof TBL128 / sizeof TBL128[0]) : s == 1 ? (int)(sizeof TBL192 / sizeof TBL192[0]) : (int)(sizeof TBL256 / sizeof TBL256[0]);
        int ok = 0;
        for (int i = 0; i < n; i++) {
            u8 key[32], pt[16], want[16], got[16];
            int kl = hexbytes(T[i].k, key); hexbytes(T[i].p, pt); hexbytes(T[i].c, want);
            tf_key(key, kl); tf_enc(pt, got, 0, 0);
            if (eq16(got, want)) ok++; else { bad++; pcheck_bytes("ecb_tbl entry", got, want, 16); }
        }
        total += n;
        pstr("Twofish-"); pdec(BITS[s]); pstr(" ecb_tbl entries: "); pdec(ok); pstr(" of "); pdec(n); pstr(" match\n");
    }
    pstr("published values compared: "); pdec(total); pstr(", mismatches "); pdec(bad); pstr("\n");
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
