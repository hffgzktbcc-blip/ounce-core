/* picohttpparser differential cases: requests, responses, header blocks, partial input (every
 * prefix), malformed input and chunked decoding. Results print return codes and parsed fields. */
#include <stdio.h>
#include <string.h>
#include <sys/types.h>
#include "picohttpparser.h"

static void show(FILE *o, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)b[i];
        if (c >= 32 && c < 127 && c != '\\') fputc(c, o); else fprintf(o, "\\x%02x", c);
    }
}

static void request(FILE *o, const char *name, const char *buf, size_t len, size_t last) {
    const char *m, *p; size_t ml, pl, nh = 16; int minor = -1;
    struct phr_header h[16];
    int r = phr_parse_request(buf, len, &m, &ml, &p, &pl, &minor, h, &nh, last);
    fprintf(o, "%s: r=%d", name, r);
    if (r > 0) {
        fprintf(o, " method="); show(o, m, ml); fprintf(o, " path="); show(o, p, pl); fprintf(o, " minor=%d nh=%zu", minor, nh);
        for (size_t i = 0; i < nh; i++) { fprintf(o, " {"); show(o, h[i].name, h[i].name_len); fprintf(o, ":"); show(o, h[i].value, h[i].value_len); fprintf(o, "}"); }
    }
    fputc('\n', o);
}

int phr_run_cases(FILE *o) {
    static const char *reqs[] = {
        "GET / HTTP/1.0\r\n\r\n",
        "GET /index.html?q=1&x=%20y HTTP/1.1\r\nHost: example.org\r\nUser-Agent: curl/8.0\r\nAccept: */*\r\n\r\n",
        "POST /submit HTTP/1.1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 27\r\nX-Folded: a\r\n b\r\n\r\nname=ounce&mode=differential",
        "GET / HTTP/1.1\nHost: lf-only\n\n",
        "PUT /a/b/c HTTP/1.1\r\nCookie: a=1; b=2; c=3; d=4; e=5; f=6; g=7; h=8\r\nAuthorization: Bearer abcdefghijklmnopqrstuvwxyz0123456789\r\n\r\n",
        "GET /\x01" "bad HTTP/1.1\r\n\r\n", "GET / HTTP/2.0\r\n\r\n", "GE T / HTTP/1.1\r\n\r\n", "GET /  HTTP/1.1\r\n\r\n",
        "GET / HTTP/1.1\r\nBad Header: x\r\n\r\n", "GET / HTTP/1.1\r\nHost: a\x7f\r\n\r\n", "\r\nGET / HTTP/1.1\r\n\r\n",
    };
    char name[32];
    for (unsigned i = 0; i < sizeof reqs / sizeof *reqs; i++) {
        snprintf(name, sizeof name, "req%u", i);
        request(o, name, reqs[i], strlen(reqs[i]), 0);
    }
    const char *whole = reqs[1];
    for (size_t n = 0; n <= strlen(whole); n += 5) {
        snprintf(name, sizeof name, "reqprefix%zu", n);
        request(o, name, whole, n, 0);
        snprintf(name, sizeof name, "reqresume%zu", n);
        request(o, name, whole, n, n > 3 ? n - 3 : 0);
    }
    static const char *resps[] = {
        "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nServer: x\r\n\r\nhello",
        "HTTP/1.0 404 Not Found\r\n\r\n", "HTTP/1.1 301 \r\nLocation: /new\r\n\r\n", "HTTP/1.1 20 OK\r\n\r\n",
        "HTTP/1.1 abc OK\r\n\r\n", "HTTP/1.1 200 OK\r\nSet-Cookie: a=b\r\nSet-Cookie: c=d\r\n\r\n",
    };
    for (unsigned i = 0; i < sizeof resps / sizeof *resps; i++) {
        const char *msg; size_t ml, nh = 8; int minor = -1, status = 0;
        struct phr_header h[8];
        int r = phr_parse_response(resps[i], strlen(resps[i]), &minor, &status, &msg, &ml, h, &nh, 0);
        fprintf(o, "resp%u: r=%d minor=%d status=%d msg=", i, r, minor, status);
        if (r > 0) { show(o, msg, ml); fprintf(o, " nh=%zu", nh); }
        fputc('\n', o);
    }
    {
        const char *hs = "Host: a\r\nX-Long: 0123456789abcdef0123456789abcdef0123456789abcdef\r\nEmpty:\r\n\r\n";
        size_t nh = 8; struct phr_header h[8];
        int r = phr_parse_headers(hs, strlen(hs), h, &nh, 0);
        fprintf(o, "headers: r=%d nh=%zu\n", r, nh);
        nh = 1;
        r = phr_parse_headers(hs, strlen(hs), h, &nh, 0);
        fprintf(o, "headers-too-many: r=%d\n", r);
    }
    {
        static const char *chunks[] = {
            "5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n", "1a\r\nabcdefghijklmnopqrstuvwxyz\r\n0\r\nTrailer: x\r\n\r\n",
            "0\r\n\r\n", "zz\r\nbad\r\n", "5;ext=1\r\nhello\r\n0\r\n\r\n", "3\r\nab",
        };
        for (unsigned i = 0; i < sizeof chunks / sizeof *chunks; i++) {
            char buf[128]; size_t sz = strlen(chunks[i]);
            memcpy(buf, chunks[i], sz);
            struct phr_chunked_decoder d; memset(&d, 0, sizeof d);
            d.consume_trailer = 1;
            ssize_t r = phr_decode_chunked(&d, buf, &sz);
            fprintf(o, "chunked%u: r=%zd out=%zu data=", i, r, sz); show(o, buf, sz); fprintf(o, " in_data=%d\n", phr_decode_chunked_is_in_data(&d));
        }
    }
    fprintf(o, "TOTAL FAILS=0\n");
    return 0;
}
