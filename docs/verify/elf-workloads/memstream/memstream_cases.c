/* elf-workload memstream: bounded "file" I/O with no host files. fmemopen and open_memstream give
 * real FILE streams over memory, exercising stdio buffering, fgets, fscanf, fread/fwrite, fseek,
 * ftell, ungetc and EOF/error flags. run-elf has no read() or open() support, so these streams
 * are the file-bounded surface that is deterministic.
 * Provenance: original work written for ounce-core (MIT); no third-party source. */
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int fails;
#define CHECK(c) do { if (!(c)) { fails++; fprintf(o, "FAIL line %d: %s\n", __LINE__, #c); } } while (0)

int workload_run(FILE *o) {
    /* read a text document line by line */
    char doc[] = "line one\nsecond line here\n\nfourth\nlast without newline";
    FILE *in = fmemopen(doc, strlen(doc), "r");
    CHECK(in != NULL);
    if (!in) return 1;
    char line[64];
    int n = 0;
    while (fgets(line, sizeof line, in)) {
        size_t len = strlen(line);
        int nl = len && line[len - 1] == '\n';
        if (nl) line[len - 1] = 0;
        fprintf(o, "%d: len=%zu nl=%d \"%s\"\n", ++n, len, nl, line);
    }
    CHECK(feof(in) && n == 5);
    fclose(in);

    /* parse numbers with fscanf */
    char nums[] = "10 -20 0x1f 3.5 word 42";
    in = fmemopen(nums, strlen(nums), "r");
    int a, b, c, d = 0;
    double f;
    char w[16];
    int got = fscanf(in, "%d %d %i %lf %15s %d", &a, &b, &c, &f, w, &d);
    fprintf(o, "fscanf got=%d a=%d b=%d c=%d f=%.1f w=%s d=%d\n", got, a, b, c, f, w, d);
    CHECK(got == 6 && a + b + c + d == 10 - 20 + 31 + 42);
    fclose(in);

    /* write into a growing memory stream, then inspect it */
    char *out = NULL;
    size_t outlen = 0;
    FILE *ms = open_memstream(&out, &outlen);
    CHECK(ms != NULL);
    if (!ms) return 1;
    for (int i = 0; i < 200; i++) fprintf(ms, "record %03d: %s\n", i, i % 3 ? "odd-ish" : "triple");
    fflush(ms);
    fprintf(o, "memstream size after flush=%zu ftell=%ld\n", outlen, ftell(ms));
    CHECK(outlen > 0 && outlen == ftell(ms));
    fprintf(ms, "<end>");               /* no seeks on the growing stream: glibc and musl differ there */
    fclose(ms);
    fprintf(o, "memstream final size=%zu head=%.18s tail=%s\n", outlen, out, out + outlen - 5);
    CHECK(!strncmp(out, "record 000: triple", 18) && !strcmp(out + outlen - 5, "<end>"));
    unsigned sum = 0;
    for (size_t i = 0; i < outlen; i++) sum = sum * 31 + (unsigned char)out[i];
    fprintf(o, "memstream checksum=%08x\n", sum);

    /* read the produced text back through a second stream */
    in = fmemopen(out, outlen, "r");
    int lines = 0, triples = 0;
    while (fgets(line, sizeof line, in)) { lines++; triples += strstr(line, "triple") != NULL; }
    fprintf(o, "reread lines=%d triples=%d\n", lines, triples);
    CHECK(lines == 201 && triples == 67); /* 200 records plus the unterminated "<end>" */
    fclose(in);
    free(out);

    /* binary fwrite/fread round trip, fseek and ftell, ungetc, EOF */
    unsigned char bin[256];
    FILE *rw = fmemopen(bin, sizeof bin, "w+");
    unsigned char pat[64];
    for (int i = 0; i < 64; i++) pat[i] = (unsigned char)(i * 7 + 3);
    size_t wrote = fwrite(pat, 1, 64, rw);
    fprintf(o, "fwrite=%zu ftell=%ld\n", wrote, ftell(rw));
    rewind(rw);
    unsigned char back[64];
    size_t rd = fread(back, 1, 64, rw);
    CHECK(rd == 64 && !memcmp(pat, back, 64));
    fseek(rw, -4, SEEK_END);
    fprintf(o, "fseek(-4,END) ftell=%ld\n", ftell(rw));
    fseek(rw, 10, SEEK_SET);
    int ch = fgetc(rw);
    fprintf(o, "byte[10]=%d ", ch);
    ungetc(ch, rw);
    int again = fgetc(rw);
    fprintf(o, "after ungetc=%d ftell=%ld\n", again, ftell(rw));
    CHECK(ch == again && ch == pat[10]);
    fclose(rw);

    /* fputs/fputc/putc to stdout stream and fwrite of binary-safe text */
    fputs("fputs line\n", o);
    fputc('c', o); putc('\n', o);
    fwrite("fwrite 10\n", 1, 10, o);

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
