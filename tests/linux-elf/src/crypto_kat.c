/* FEAT_AES/SHA1/SHA2/SHA512 known-answer program: prints each record of crypto_kat_core.h as hex bytes. */
#include "common.h"
#include "crypto_kat_core.h"
static void emit(const u8 *p, int n) {
    char b[130]; int k = 0;
    for (int i = 0; i < n; i++) { b[k++] = "0123456789abcdef"[p[i] >> 4]; b[k++] = "0123456789abcdef"[p[i] & 15]; }
    b[k++] = '\n'; b[k] = 0; put(b);
}
void _start(void) {
    run_crypto_kat(emit);
    quit(0);
}
