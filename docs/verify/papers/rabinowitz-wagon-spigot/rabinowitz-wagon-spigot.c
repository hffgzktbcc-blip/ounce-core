// Spigot algorithm for the digits of pi (S. Rabinowitz and S. Wagon, "A spigot algorithm for the digits of pi",
// American Mathematical Monthly 102 (1995) 195-203), written from the algorithm: pi = 2 + 1/3 (2 + 2/5 (2 + 3/7 (2 + ...)))
// is held as an array of len = floor(10n/3) + 1 mixed-radix digits all equal to 2; each round multiplies by 10, propagates carries
// from the right (position i is reduced modulo 2i - 1, with position 1 reduced modulo 10) and emits one decimal digit, with
// the paper's "predigit" mechanism to hold back runs of 9s. Only small-integer arithmetic is used.
// The program produces n = 2000 digits of pi and compares them with the digits listed in OEIS A000796.
// Exit 0 only if all 2000 digits are equal.
#include "ounce_paper.h"

#define NDIG 2000
#define LEN (10 * NDIG / 3 + 1)
#ifndef RW_MOD_OFFSET
#define RW_MOD_OFFSET 1      // divisor of position i is 2*i - RW_MOD_OFFSET (= 2i - 1)
#endif

// First 2000 digits of pi (3, then 1999 decimals) from OEIS A000796.
static const char PI_DIGITS[] =
    "3141592653589793238462643383279502884197169399375105820974944592307816406286208998628034825342117067"
    "9821480865132823066470938446095505822317253594081284811174502841027019385211055596446229489549303819"
    "6442881097566593344612847564823378678316527120190914564856692346034861045432664821339360726024914127"
    "3724587006606315588174881520920962829254091715364367892590360011330530548820466521384146951941511609"
    "4330572703657595919530921861173819326117931051185480744623799627495673518857527248912279381830119491"
    "2983367336244065664308602139494639522473719070217986094370277053921717629317675238467481846766940513"
    "2000568127145263560827785771342757789609173637178721468440901224953430146549585371050792279689258923"
    "5420199561121290219608640344181598136297747713099605187072113499999983729780499510597317328160963185"
    "9502445945534690830264252230825334468503526193118817101000313783875288658753320838142061717766914730"
    "3598253490428755468731159562863882353787593751957781857780532171226806613001927876611195909216420198"
    "9380952572010654858632788659361533818279682303019520353018529689957736225994138912497217752834791315"
    "1557485724245415069595082953311686172785588907509838175463746493931925506040092770167113900984882401"
    "2858361603563707660104710181942955596198946767837449448255379774726847104047534646208046684259069491"
    "2933136770289891521047521620569660240580381501935112533824300355876402474964732639141992726042699227"
    "9678235478163600934172164121992458631503028618297455570674983850549458858692699569092721079750930295"
    "5321165344987202755960236480665499119881834797753566369807426542527862551818417574672890977772793800"
    "0816470600161452491921732172147723501414419735685481613611573525521334757418494684385233239073941433"
    "3454776241686251898356948556209921922218427255025425688767179049460165346680498862723279178608578438"
    "3827967976681454100953883786360950680064225125205117392984896084128488626945604241965285022210661186"
    "3067442786220391949450471237137869609563643719172874677646575739624138908658326459958133904780275900"
;

static u32 A[LEN + 1];

int paper_main(void) {
    static char out[NDIG + 8];
    int nout = 0;
    for (int i = 1; i <= LEN; i++) A[i] = 2;
    int nines = 0, predigit = 0;
    int have_pre = 0;
    for (int j = 1; j <= NDIG; j++) {
        u32 q = 0;
        for (int i = LEN; i >= 1; i--) {
            u32 x = 10 * A[i] + q * (u32)i;
            if (i == 1) { A[1] = x % 10; q = x / 10; }
            else { A[i] = x % (u32)(2 * i - RW_MOD_OFFSET); q = x / (u32)(2 * i - RW_MOD_OFFSET); }
        }
        // q is now a digit 0..10 (10 means carry into the previous digit)
        if (q == 9) { nines++; }
        else if (q == 10) {
            if (have_pre) out[nout++] = (char)('0' + predigit + 1);
            for (int k = 0; k < nines; k++) out[nout++] = '0';
            predigit = 0; nines = 0; have_pre = 1;
        } else {
            if (have_pre) out[nout++] = (char)('0' + predigit);
            for (int k = 0; k < nines; k++) out[nout++] = '9';
            predigit = (int)q; nines = 0; have_pre = 1;
        }
    }
    // flush the held digit and the held run of 9s
    if (have_pre) out[nout++] = (char)('0' + predigit);
    for (int k = 0; k < nines; k++) out[nout++] = '9';
    // the first round's predigit is the leading 3; compare as many digits as were produced up to NDIG
    int first = -1;
    for (int i = 0; i < NDIG; i++) if (i >= nout || out[i] != PI_DIGITS[i]) { first = i; break; }
    pstr("digits produced: "); pdec(nout); pstr("\n");
    pstr("first 50 digits: "); paper_write(out, 50); pstr("\n");
    pstr("last 10 digits compared (1991-2000): "); paper_write(out + 1990, 10); pstr("\n");
    if (first < 0) pstr("PASS spigot output equals the 2000 digits of pi in OEIS A000796\n");
    else { pstr("FAIL first difference at digit "); pdec(first + 1); pstr("\n"); }
    return first < 0 ? 0 : 1;
}
