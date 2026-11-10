/* inih (BSD-3) differential cases: INI text with sections, comments, continuation lines, quotes,
 * duplicates, handler rejection and malformed lines, through ini_parse_string. Output lists every
 * callback and the returned line number of the first error. */
#include <stdio.h>
#include <string.h>
#include "ini.h"

struct ctx { FILE *o; int n; int stop_at; };

static int handler(void *user, const char *section, const char *name, const char *value) {
    struct ctx *c = user;
    fprintf(c->o, "  [%s] %s=<%s>\n", section, name, value ? value : "(null)");
    return ++c->n == c->stop_at ? 0 : 1;
}

int inih_run_cases(FILE *o) {
    static const char *texts[] = {
        "",
        "key=value\n",
        "; comment\n# another\n[sec]\na = 1\nb: two\nc =   spaced value   \n",
        "[one]\nx=1\n[two]\nx=2\n[one]\ny=3\n",
        "[s]\nlong = first part\n  continued here\n  and more\nnext=1\n",
        "[s]\nq = \"quoted ; not comment\"\ninline = value ; trailing comment\nhash = v # not a comment in default config\n",
        "[unterminated\nk=v\n",
        "no equals sign here\nk=v\n",
        "[s]\n=novalue\nk=\n=\n",
        "\xef\xbb\xbf[bom]\nk=v\n",
        "[a]\nk=1\r\nj=2\r\n[b]\r\nm=3\r\n",
        "[s]\nk=v\nk=w\nk=x\n",
        "[verylongsectionnamethatshouldbetruncatedbythelibraryifthelimitissmallenoughtomattersomewhereinthefile]\nk=v\n",
    };
    char buf[2048];
    for (unsigned i = 0; i < sizeof texts / sizeof *texts; i++) {
        struct ctx c = {o, 0, 0};
        fprintf(o, "text%u:\n", i);
        int r = ini_parse_string(texts[i], handler, &c);
        fprintf(o, "  -> %d\n", r);
    }
    {
        struct ctx c = {o, 0, 2};
        fprintf(o, "reject-second:\n");
        int r = ini_parse_string(texts[2], handler, &c);
        fprintf(o, "  -> %d\n", r);
    }
    {
        size_t at = 0;
        for (int i = 0; i < 60 && at < sizeof buf - 40; i++) at += (size_t)snprintf(buf + at, sizeof buf - at, "[sec%d]\nkey%d = value number %d\nflag%d=on\n", i / 6, i, i * 7, i);
        struct ctx c = {o, 0, 0};
        fprintf(o, "generated(%zu bytes):\n", at);
        int r = ini_parse_string(buf, handler, &c);
        fprintf(o, "  -> %d, %d callbacks\n", r, c.n);
    }
    fprintf(o, "TOTAL FAILS=0\n");
    return 0;
}
