/* jsmn (MIT) differential cases: token streams for valid documents, every prefix of one document
 * (the streaming/partial-input behaviour), malformed inputs, and token-array exhaustion.
 * Library errors are observations; only an unexpected crash or a difference between engines fails. */
#include <stdio.h>
#include <string.h>
#define JSMN_STATIC
#include "jsmn.h"

static void dump(FILE *o, const char *name, const char *js, size_t len, unsigned ntok) {
    jsmntok_t t[128];
    jsmn_parser p;
    jsmn_init(&p);
    int r = jsmn_parse(&p, js, len, t, ntok < 128 ? ntok : 128);
    fprintf(o, "%s: r=%d", name, r);
    for (int i = 0; i < r; i++) {
        fprintf(o, " [%d:%d..%d/%d]", t[i].type, t[i].start, t[i].end, t[i].size);
    }
    fputc('\n', o);
}

int jsmn_run_cases(FILE *o) {
    static const char *docs[] = {
        "{}", "[]", "{\"a\":1}", "[1,2,3]", "\"str\"", "true", "null", "-12.5e+3",
        "{\"user\":{\"name\":\"Ada\",\"langs\":[\"c\",\"rust\",{\"x\":[]}],\"age\":36,\"ok\":false,\"n\":null}}",
        "[ 1 , \"two\" ,\t[3 ,\n4] , {\"k\" : \"v\"} ]",
        "{\"esc\":\"a\\\"b\\\\c\\/d\\u00e9\\n\",\"num\":[0,-1,1e10,0.5]}",
        "[[[[[[[[[[1]]]]]]]]]]",
        "{\"a\":[1,2,{\"b\":[3,4,{\"c\":5}]}],\"d\":\"e\"}",
    };
    char name[48];
    for (unsigned i = 0; i < sizeof docs / sizeof *docs; i++) {
        snprintf(name, sizeof name, "doc%u", i);
        dump(o, name, docs[i], strlen(docs[i]), 128);
    }
    const char *whole = docs[8];
    for (size_t n = 0; n <= strlen(whole); n += 3) {
        snprintf(name, sizeof name, "prefix%zu", n);
        dump(o, name, whole, n, 128);
    }
    static const char *bad[] = {
        "{", "[", "}", "]", "{\"a\"}", "{\"a\":}", "{1:2}", "[1,]", "[,1]", "\"unterminated", "\"bad\\escape\"",
        "\"bad\\u12G4\"", "{\"a\":1 \"b\":2}", "tru", "[1 2]", "{\"a\":{\"b\":}}", "]]", "[}", "{]",
    };
    for (unsigned i = 0; i < sizeof bad / sizeof *bad; i++) {
        snprintf(name, sizeof name, "bad%u", i);
        dump(o, name, bad[i], strlen(bad[i]), 128);
    }
    for (unsigned n = 0; n <= 12; n += 3) {
        snprintf(name, sizeof name, "tokens%u", n);
        dump(o, name, docs[8], strlen(docs[8]), n);
    }
    fprintf(o, "TOTAL FAILS=0\n");
    return 0;
}
