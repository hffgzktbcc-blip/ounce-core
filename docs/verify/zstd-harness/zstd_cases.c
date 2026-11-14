/* zstd (BSD-3-Clause) differential cases: one-shot compression across levels,
 * explicit CCtx parameters (checksum, windowLog, strategy), streaming flush/end,
 * dictionary compression (raw + CDict/DDict), frame inspection, and corrupt inputs. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "zstd.h"

static uint64_t fnv1a(const unsigned char *p, size_t n) {
    uint64_t h = 0xcbf29ce484222325ull;
    for (size_t i = 0; i < n; i++) {
        h ^= p[i];
        h *= 0x100000001b3ull;
    }
    return h;
}

static uint32_t rng_state = 0x243f6a88u;
static unsigned next_u8(void) {
    rng_state = rng_state * 1664525u + 1013904223u;
    return (rng_state >> 16) & 0xffu;
}

static void fill_pattern(unsigned char *buf, size_t n, int kind) {
    static const char phrase[] = "{\"id\":1234,\"tag\":\"zstd-gauntlet\",\"ok\":true,\"vals\":[1,2,3,5,8,13]};\n";
    size_t plen = sizeof(phrase) - 1;
    for (size_t i = 0; i < n; i++) {
        if (kind == 0) {
            buf[i] = (unsigned char)'Z';
        } else if (kind == 1) {
            buf[i] = (unsigned char)phrase[i % plen];
        } else if (kind == 2) {
            buf[i] = (unsigned char)(next_u8() & 0x0f);
        } else {
            buf[i] = (unsigned char)((i % 9 == 0) ? next_u8() : (unsigned)(i * 7 + 13));
        }
    }
}

int zstd_run_cases(FILE *o) {
    int fails = 0;
    fprintf(o, "zstd %s levels=[%d..%d]\n", ZSTD_versionString(), ZSTD_minCLevel(), ZSTD_maxCLevel());

    const size_t max_in = 32768;
    size_t cap = ZSTD_compressBound(max_in);
    unsigned char *in = malloc(max_in);
    unsigned char *comp = malloc(cap);
    unsigned char *decomp = malloc(max_in + 16);
    if (!in || !comp || !decomp) {
        free(in); free(comp); free(decomp);
        return 1;
    }

    /* 1. One-shot compress/decompress across sizes, patterns, and levels */
    static const struct { size_t len; int kind; } shapes[] = {
        {0, 0},
        {1, 0},
        {512, 0},
        {8192, 1},
        {16384, 2},
        {32768, 3},
    };
    static const int levels[] = {1, 3, 7, 12, 19};

    for (unsigned si = 0; si < sizeof shapes / sizeof *shapes; si++) {
        size_t n = shapes[si].len;
        rng_state = 0x13572468u + si * 97u;
        fill_pattern(in, n, shapes[si].kind);
        uint64_t in_hash = fnv1a(in, n);

        for (unsigned li = 0; li < sizeof levels / sizeof *levels; li++) {
            int lvl = levels[li];
            if (n > 16384 && lvl > 7) continue; /* keep budget bounded at high levels */
            size_t csz = ZSTD_compress(comp, cap, in, n, lvl);
            if (ZSTD_isError(csz)) {
                fprintf(o, "oneshot s%u L%d COMP_ERR=%s\n", si, lvl, ZSTD_getErrorName(csz));
                fails++;
                continue;
            }
            unsigned long long fcs = ZSTD_getFrameContentSize(comp, csz);
            size_t fcsz = ZSTD_findFrameCompressedSize(comp, csz);
            size_t dsz = ZSTD_decompress(decomp, max_in, comp, csz);
            int ok = !ZSTD_isError(dsz) && dsz == n && fcs == (unsigned long long)n &&
                     fcsz == csz && (n == 0 || memcmp(in, decomp, n) == 0);
            if (!ok) fails++;
            fprintf(o, "oneshot s%u(%zu) L%d in=%016llx c=%zu(%016llx) d=%zu ok=%d\n",
                    si, n, lvl, (unsigned long long)in_hash, csz, (unsigned long long)fnv1a(comp, csz), dsz, ok);
        }
    }

    /* 2. Explicit CCtx/DCtx with checksum and strategy parameters */
    {
        ZSTD_CCtx *cctx = ZSTD_createCCtx();
        ZSTD_DCtx *dctx = ZSTD_createDCtx();
        rng_state = 0xdeadbeefu;
        fill_pattern(in, 16384, 1);
        for (int strat = 1; strat <= 7; strat += 2) {
            ZSTD_CCtx_reset(cctx, ZSTD_reset_session_and_parameters);
            ZSTD_CCtx_setParameter(cctx, ZSTD_c_compressionLevel, 5);
            ZSTD_CCtx_setParameter(cctx, ZSTD_c_checksumFlag, 1);
            ZSTD_CCtx_setParameter(cctx, ZSTD_c_windowLog, 14);
            ZSTD_CCtx_setParameter(cctx, ZSTD_c_strategy, strat);
            size_t csz = ZSTD_compress2(cctx, comp, cap, in, 16384);
            size_t dsz = ZSTD_decompressDCtx(dctx, decomp, max_in, comp, csz);
            int ok = !ZSTD_isError(csz) && !ZSTD_isError(dsz) && dsz == 16384 && memcmp(in, decomp, 16384) == 0;
            if (!ok) fails++;
            fprintf(o, "cctx strat=%d c=%zu(%016llx) ok=%d\n",
                    strat, csz, (unsigned long long)fnv1a(comp, csz), ok);
        }

        /* Corrupt checksum trailer detection */
        ZSTD_CCtx_reset(cctx, ZSTD_reset_session_and_parameters);
        ZSTD_CCtx_setParameter(cctx, ZSTD_c_checksumFlag, 1);
        size_t csz = ZSTD_compress2(cctx, comp, cap, in, 4096);
        comp[csz - 1] ^= 0x40;
        size_t bad_csum = ZSTD_decompressDCtx(dctx, decomp, max_in, comp, csz);
        fprintf(o, "corrupt_checksum is_err=%d name=%s\n",
                ZSTD_isError(bad_csum), ZSTD_getErrorName(bad_csum));
        if (!ZSTD_isError(bad_csum)) fails++;

        /* Corrupt magic & truncated frame */
        comp[csz - 1] ^= 0x40;
        size_t trunc = ZSTD_decompressDCtx(dctx, decomp, max_in, comp, csz / 2);
        fprintf(o, "truncated is_err=%d name=%s\n", ZSTD_isError(trunc), ZSTD_getErrorName(trunc));
        if (!ZSTD_isError(trunc)) fails++;
        comp[0] ^= 0x01;
        size_t bad_hdr = ZSTD_decompressDCtx(dctx, decomp, max_in, comp, csz);
        fprintf(o, "bad_magic is_err=%d name=%s\n", ZSTD_isError(bad_hdr), ZSTD_getErrorName(bad_hdr));
        if (!ZSTD_isError(bad_hdr)) fails++;

        ZSTD_freeCCtx(cctx);
        ZSTD_freeDCtx(dctx);
    }

    /* 3. Streaming compression & decompression with small irregular chunks */
    {
        ZSTD_CStream *cs = ZSTD_createCStream();
        ZSTD_DStream *ds = ZSTD_createDStream();
        rng_state = 0xfeedfaceu;
        fill_pattern(in, 12000, 3);
        ZSTD_CCtx_reset(cs, ZSTD_reset_session_and_parameters);
        ZSTD_CCtx_setParameter(cs, ZSTD_c_compressionLevel, 4);
        ZSTD_CCtx_setParameter(cs, ZSTD_c_checksumFlag, 1);

        size_t in_pos = 0, out_pos = 0;
        size_t step_idx = 0;
        static const size_t chunks[] = {1, 17, 131, 1024, 509};
        while (in_pos < 12000) {
            size_t take = chunks[step_idx % 5];
            if (in_pos + take > 12000) take = 12000 - in_pos;
            ZSTD_inBuffer zin = {in + in_pos, take, 0};
            ZSTD_EndDirective mode = (in_pos + take == 12000) ? ZSTD_e_end : ((step_idx % 3 == 2) ? ZSTD_e_flush : ZSTD_e_continue);
            int done = 0;
            while (!done) {
                ZSTD_outBuffer zout = {comp + out_pos, cap - out_pos, 0};
                size_t rem = ZSTD_compressStream2(cs, &zout, &zin, mode);
                if (ZSTD_isError(rem)) { fails++; break; }
                out_pos += zout.pos;
                done = (mode == ZSTD_e_continue) ? (zin.pos == zin.size) : (rem == 0);
            }
            in_pos += take;
            step_idx++;
        }

        size_t d_in = 0, d_out = 0, ret = 1;
        while (d_in < out_pos || ret > 0) {
            size_t take = (out_pos - d_in > 37) ? 37 : (out_pos - d_in);
            ZSTD_inBuffer zin = {comp + d_in, take, 0};
            ZSTD_outBuffer zout = {decomp + d_out, (max_in - d_out > 53) ? 53 : (max_in - d_out), 0};
            ret = ZSTD_decompressStream(ds, &zout, &zin);
            if (ZSTD_isError(ret)) { fails++; break; }
            d_in += zin.pos;
            d_out += zout.pos;
            if (ret == 0) break;
        }
        int ok = d_out == 12000 && memcmp(in, decomp, 12000) == 0;
        if (!ok) fails++;
        fprintf(o, "stream c=%zu(%016llx) d=%zu ok=%d\n", out_pos, (unsigned long long)fnv1a(comp, out_pos), d_out, ok);
        ZSTD_freeCStream(cs);
        ZSTD_freeDStream(ds);
    }

    /* 4. Dictionary compression (raw + CDict/DDict) */
    {
        unsigned char dict[1024];
        fill_pattern(dict, sizeof dict, 1);
        fill_pattern(in, 4096, 1);
        in[123] = 'X';
        in[2048] = 'Y';

        ZSTD_CCtx *cctx = ZSTD_createCCtx();
        ZSTD_DCtx *dctx = ZSTD_createDCtx();
        size_t c1 = ZSTD_compress_usingDict(cctx, comp, cap, in, 4096, dict, sizeof dict, 5);
        size_t d1 = ZSTD_decompress_usingDict(dctx, decomp, max_in, comp, c1, dict, sizeof dict);
        int ok1 = !ZSTD_isError(c1) && !ZSTD_isError(d1) && d1 == 4096 && memcmp(in, decomp, 4096) == 0;
        if (!ok1) fails++;

        ZSTD_CDict *cd = ZSTD_createCDict(dict, sizeof dict, 5);
        ZSTD_DDict *dd = ZSTD_createDDict(dict, sizeof dict);
        size_t c2 = ZSTD_compress_usingCDict(cctx, comp, cap, in, 4096, cd);
        size_t d2 = ZSTD_decompress_usingDDict(dctx, decomp, max_in, comp, c2, dd);
        int ok2 = !ZSTD_isError(c2) && !ZSTD_isError(d2) && d2 == 4096 && memcmp(in, decomp, 4096) == 0;
        if (!ok2) fails++;

        fprintf(o, "dict raw_c=%zu ok1=%d cdict_c=%zu(%016llx) ok2=%d\n",
                c1, ok1, c2, (unsigned long long)fnv1a(comp, c2), ok2);
        ZSTD_freeCDict(cd);
        ZSTD_freeDDict(dd);
        ZSTD_freeCCtx(cctx);
        ZSTD_freeDCtx(dctx);
    }

    free(in);
    free(comp);
    free(decomp);
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
