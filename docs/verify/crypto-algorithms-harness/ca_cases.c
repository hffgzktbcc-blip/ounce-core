/* B-Con crypto-algorithms differential cases (public domain): SHA-1, SHA-256, MD5, MD2, AES
 * (ECB/CBC/CTR/CCM, 128/192/256), Blowfish, DES/3DES, RC4, Base64, ROT13. Digests and
 * ciphertexts are printed for generated inputs of many lengths; well-known vectors
 * (SHA-1/SHA-256/MD5 of "abc", FIPS-197 AES-128, RFC 6229-style RC4 "Key"/"Plaintext",
 * Base64 "foobar") are checked against constants as an independent oracle. */
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include "sha1.h"
#include "sha256.h"
#include "md5.h"
#include "md2.h"
#include "aes.h"
#include "blowfish.h"
#include "des.h"
#include "arcfour.h"
#include "base64.h"
#include "rot-13.h"

static void hex(FILE *o, const char *label, const BYTE *p, size_t n) {
    fprintf(o, "%s=", label);
    for (size_t i = 0; i < n; i++) fprintf(o, "%02x", p[i]);
    fputc('\n', o);
}
static int expect(FILE *o, const char *what, const BYTE *p, size_t n, const char *want) {
    char got[256];
    for (size_t i = 0; i < n; i++) sprintf(got + 2 * i, "%02x", p[i]);
    int ok = strcmp(got, want) == 0;
    fprintf(o, "known-answer %s: %s\n", what, ok ? "PASS" : "FAIL");
    return !ok;
}

int ca_run_cases(FILE *o) {
    static BYTE data[5000];
    int fails = 0;
    for (size_t i = 0; i < sizeof data; i++) data[i] = (BYTE)((i * 131 + (i >> 3)) ^ 0x5a);
    static const size_t lens[] = {0, 1, 3, 55, 56, 57, 63, 64, 65, 119, 120, 128, 1000, 4999};

    for (size_t k = 0; k < sizeof lens / sizeof *lens; k++) {
        size_t n = lens[k];
        char label[32];
        BYTE h[32];
        SHA1_CTX s1; sha1_init(&s1); sha1_update(&s1, data, n); sha1_final(&s1, h);
        snprintf(label, sizeof label, "sha1[%zu]", n); hex(o, label, h, SHA1_BLOCK_SIZE);
        SHA256_CTX s2; sha256_init(&s2); sha256_update(&s2, data, n / 2); sha256_update(&s2, data + n / 2, n - n / 2); sha256_final(&s2, h);
        snprintf(label, sizeof label, "sha256[%zu]", n); hex(o, label, h, SHA256_BLOCK_SIZE);
        MD5_CTX m5; md5_init(&m5); md5_update(&m5, data, n); md5_final(&m5, h);
        snprintf(label, sizeof label, "md5[%zu]", n); hex(o, label, h, MD5_BLOCK_SIZE);
        MD2_CTX m2; md2_init(&m2); md2_update(&m2, data, n); md2_final(&m2, h);
        snprintf(label, sizeof label, "md2[%zu]", n); hex(o, label, h, MD2_BLOCK_SIZE);
    }
    {
        BYTE h[32];
        SHA1_CTX s1; sha1_init(&s1); sha1_update(&s1, (const BYTE *)"abc", 3); sha1_final(&s1, h);
        fails += expect(o, "sha1(abc)", h, 20, "a9993e364706816aba3e25717850c26c9cd0d89d");
        SHA256_CTX s2; sha256_init(&s2); sha256_update(&s2, (const BYTE *)"abc", 3); sha256_final(&s2, h);
        fails += expect(o, "sha256(abc)", h, 32, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        MD5_CTX m5; md5_init(&m5); md5_update(&m5, (const BYTE *)"abc", 3); md5_final(&m5, h);
        fails += expect(o, "md5(abc)", h, 16, "900150983cd24fb0d6963f7d28e17f72");
    }

    /* AES: FIPS-197 vector, then all key sizes with ECB/CBC/CTR/CCM over generated data */
    {
        BYTE key[32], pt[16], ct[16], back[16];
        WORD ks[60];
        for (int i = 0; i < 32; i++) key[i] = (BYTE)i;
        for (int i = 0; i < 16; i++) pt[i] = (BYTE)(i * 0x11);
        aes_key_setup(key, ks, 128);
        aes_encrypt(pt, ct, ks, 128);
        fails += expect(o, "aes128-fips197", ct, 16, "69c4e0d86a7b0430d8cdb78070b4c55a");
        for (int bits = 128; bits <= 256; bits += 64) {
            char label[32];
            BYTE iv[16], buf[256], out[256], dec[256];
            aes_key_setup(key, ks, bits);
            aes_encrypt(pt, ct, ks, bits); aes_decrypt(ct, back, ks, bits);
            snprintf(label, sizeof label, "aes%d-ecb", bits); hex(o, label, ct, 16);
            int ok = memcmp(back, pt, 16) == 0; fails += !ok;
            for (int i = 0; i < 16; i++) iv[i] = (BYTE)(0xA0 + i);
            memcpy(buf, data, 256);
            aes_encrypt_cbc(buf, 256, out, ks, bits, iv);
            snprintf(label, sizeof label, "aes%d-cbc-hash", bits);
            { SHA256_CTX c; BYTE h[32]; sha256_init(&c); sha256_update(&c, out, 256); sha256_final(&c, h); hex(o, label, h, 32); }
            aes_encrypt_cbc_mac(buf, 256, out, ks, bits, iv);
            snprintf(label, sizeof label, "aes%d-cbcmac", bits); hex(o, label, out, 16);
            for (size_t n = 1; n <= 200; n += 49) {
                aes_encrypt_ctr(buf, n, out, ks, bits, iv);
                aes_decrypt_ctr(out, n, dec, ks, bits, iv);
                snprintf(label, sizeof label, "aes%d-ctr[%zu]", bits, n);
                hex(o, label, out, n < 24 ? n : 24);
                ok = memcmp(dec, buf, n) == 0; fails += !ok;
            }
            {
                BYTE nonce[12], ad[20], ctext[300], ptext[300];
                WORD clen = 0; int auth = 0;
                for (int i = 0; i < 12; i++) nonce[i] = (BYTE)(i + 1);
                for (int i = 0; i < 20; i++) ad[i] = (BYTE)(0xC0 + i);
                int r = aes_encrypt_ccm(buf, 100, ad, 20, nonce, 12, ctext, &clen, 8, key, bits);
                snprintf(label, sizeof label, "aes%d-ccm", bits);
                fprintf(o, "%s rc=%d clen=%u ", label, r, clen); hex(o, "tail", ctext + clen - 12, 12);
                WORD plen = 0;
                r = aes_decrypt_ccm(ctext, clen, ad, 20, nonce, 12, ptext, &plen, 8, &auth, key, bits);
                /* observation only: this library's CCM round trip is not asserted */
                fprintf(o, "%s decrypt rc=%d auth=%d plen=%u match=%d\n", label, r, auth, plen, plen >= 100 && memcmp(ptext, buf, 100) == 0);
                ctext[3] ^= 1;
                r = aes_decrypt_ccm(ctext, clen, ad, 20, nonce, 12, ptext, &plen, 8, &auth, key, bits);
                fprintf(o, "%s tampered: rc=%d auth=%d\n", label, r, auth);
                fails += auth != 0;
            }
        }
    }

    {
        BYTE key[16], in[8], enc[8], dec[8];
        BLOWFISH_KEY bk;
        for (int i = 0; i < 16; i++) key[i] = (BYTE)(0x10 + 3 * i);
        for (size_t klen = 4; klen <= 16; klen += 6) {
            blowfish_key_setup(key, &bk, klen);
            for (int blk = 0; blk < 3; blk++) {
                memcpy(in, data + blk * 8, 8);
                blowfish_encrypt(in, enc, &bk); blowfish_decrypt(enc, dec, &bk);
                char label[32]; snprintf(label, sizeof label, "blowfish[k%zu,b%d]", klen, blk); hex(o, label, enc, 8);
                fails += memcmp(dec, in, 8) != 0;
            }
        }
    }
    {
        BYTE key[24], in[8], enc[8], dec[8];
        BYTE sched[16][6], sched3[3][16][6], sched3d[3][16][6];
        for (int i = 0; i < 24; i++) key[i] = (BYTE)(0x31 + 7 * i);
        for (int blk = 0; blk < 4; blk++) {
            memcpy(in, data + 100 + blk * 8, 8);
            des_key_setup(key, sched, DES_ENCRYPT); des_crypt(in, enc, sched);
            des_key_setup(key, sched, DES_DECRYPT); des_crypt(enc, dec, sched);
            char label[32]; snprintf(label, sizeof label, "des[%d]", blk); hex(o, label, enc, 8);
            fails += memcmp(dec, in, 8) != 0;
            three_des_key_setup(key, sched3, DES_ENCRYPT); three_des_crypt(in, enc, sched3);
            three_des_key_setup(key, sched3d, DES_DECRYPT); three_des_crypt(enc, dec, sched3d);
            snprintf(label, sizeof label, "3des[%d]", blk); hex(o, label, enc, 8);
            fails += memcmp(dec, in, 8) != 0;
        }
    }
    {
        BYTE st[256], out[64];
        arcfour_key_setup(st, (const BYTE *)"Key", 3);
        arcfour_generate_stream(st, out, 9);
        BYTE ct[9]; const char *pt = "Plaintext";
        for (int i = 0; i < 9; i++) ct[i] = (BYTE)pt[i] ^ out[i];
        fails += expect(o, "rc4(Key,Plaintext)", ct, 9, "bbf316e8d940af0ad3");
        arcfour_key_setup(st, data, 16);
        arcfour_generate_stream(st, out, 64);
        hex(o, "rc4-stream", out, 64);
    }
    {
        BYTE enc[8000], dec[8000];
        size_t e = base64_encode((const BYTE *)"foobar", enc, 6, 0); enc[e] = 0;
        int ok = e == 8 && !strcmp((char *)enc, "Zm9vYmFy"); fails += !ok;
        fprintf(o, "known-answer base64(foobar)=%s: %s\n", enc, ok ? "PASS" : "FAIL");
        for (size_t k = 0; k < sizeof lens / sizeof *lens; k++) {
            size_t n = lens[k];
            e = base64_encode(data, enc, n, k & 1);
            enc[e] = 0; /* base64_decode scans to the end of its input; keep it deterministic */
            memset(dec, 0, sizeof dec);
            size_t d = e ? base64_decode(enc, dec, e) : 0; /* base64_decode(len 0) reads in[-1]: not called */
            SHA256_CTX c; BYTE h[32]; sha256_init(&c); sha256_update(&c, enc, e); sha256_final(&c, h);
            char label[32]; snprintf(label, sizeof label, "base64[%zu]", n);
            fprintf(o, "%s enc=%zu dec=%zu ", label, e, d); hex(o, "sha256", h, 32);
            /* Round trips are asserted only without newlines; the newline variants are observations. */
            if (!(k & 1)) fails += d != n || memcmp(dec, data, n) != 0;
        }
        char text[] = "Hello, World! 123";
        rot13(text);
        fprintf(o, "rot13=%s\n", text);
    }
    fprintf(o, "TOTAL FAILS=%d\n", fails);
    return fails;
}
