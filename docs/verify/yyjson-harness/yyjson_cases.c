/* yyjson (MIT) differential cases: parsing, Dragonbox float serialization,
 * mutable documents, JSON Pointer, RFC 6902 Patch, RFC 7386 Merge Patch, and errors. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "yyjson.h"

static uint64_t fnv1a(const char *s, size_t n) {
    uint64_t h = 0xcbf29ce484222325ull;
    for (size_t i = 0; i < n; i++) {
        h ^= (unsigned char)s[i];
        h *= 0x100000001b3ull;
    }
    return h;
}

static void check_read_write(FILE *o, int *fails, const char *label, const char *json,
                             yyjson_read_flag rflg, yyjson_write_flag wflg) {
    yyjson_read_err rerr;
    yyjson_doc *doc = yyjson_read_opts((char *)json, strlen(json), rflg & ~YYJSON_READ_INSITU, NULL, &rerr);
    if (!doc) {
        fprintf(o, "[%s] READ_ERR code=%u pos=%zu\n", label, (unsigned)rerr.code, rerr.pos);
        return;
    }
    size_t len = 0;
    yyjson_write_err werr;
    char *out = yyjson_write_opts(doc, wflg, NULL, &len, &werr);
    if (!out) {
        fprintf(o, "[%s] WRITE_ERR code=%u\n", label, (unsigned)werr.code);
        yyjson_doc_free(doc);
        (*fails)++;
        return;
    }
    fprintf(o, "[%s] vals=%zu len=%zu hash=%016llx\n%s\n",
            label, yyjson_doc_get_val_count(doc), len,
            (unsigned long long)fnv1a(out, len), out);

    /* Round-trip check for strict JSON writes */
    if (!(wflg & (YYJSON_WRITE_ALLOW_INF_AND_NAN | YYJSON_WRITE_TRAILING_COMMAS))) {
        yyjson_doc *doc2 = yyjson_read(out, len, 0);
        if (!doc2) {
            fprintf(o, "[%s] ROUNDTRIP_READ_FAIL\n", label);
            (*fails)++;
        } else {
            bool eq = yyjson_equals(yyjson_doc_get_root(doc), yyjson_doc_get_root(doc2));
            fprintf(o, "[%s] roundtrip_eq=%d\n", label, (int)eq);
            if (!eq && !(rflg & YYJSON_READ_NUMBER_AS_RAW)) (*fails)++;
            yyjson_doc_free(doc2);
        }
    }
    free(out);
    yyjson_doc_free(doc);
}

int yyjson_run_cases(FILE *o) {
    int fails = 0;

    static const struct {
        const char *label;
        const char *json;
        yyjson_read_flag rflg;
        yyjson_write_flag wflg;
    } cases[] = {
        {"empty-obj", "{}", 0, 0},
        {"empty-arr", "[]", 0, 0},
        {"scalars", "[true,false,null,0,-0,42,-99,\"hello\"]", 0, 0},
        {"ints64", "[0,-1,2147483647,-2147483648,9223372036854775807,-9223372036854775807,18446744073709551615]", 0, 0},
        {"dragonbox", "[0.1,0.2,0.3,0.5,-0.25,3.141592653589793,1e10,1e-10,1.2345678901234567e100,1.7976931348623157e308,2.2250738585072014e-308,5e-324]", 0, 0},
        {"escapes", "{\"esc\":\"a\\nb\\tc\\\"d\\\\e\\/f\\b\\f\\r\\u0041\\u00e9\\u20ac\\ud83d\\ude00\"}", 0, YYJSON_WRITE_ESCAPE_UNICODE | YYJSON_WRITE_ESCAPE_SLASHES},
        {"utf8-long", "{\"k\":\"0123456789abcdef0123456789abcdef_\\u00e9_long_swar_scan_string_test_end\"}", 0, YYJSON_WRITE_PRETTY_TWO_SPACES},
        {"json5-ext", "{ /* c1 */ key: 'single', arr: [1, 2, 3, ], inf: Infinity, nan: NaN, // c2\n hex: 0x10 }",
         YYJSON_READ_JSON5, YYJSON_WRITE_ALLOW_INF_AND_NAN | YYJSON_WRITE_PRETTY_TWO_SPACES},
        {"num-raw", "{\"big\":1234567890123456789012345678901234567890,\"fp\":0.1000}",
         YYJSON_READ_NUMBER_AS_RAW, 0},
    };
    for (unsigned i = 0; i < sizeof cases / sizeof *cases; i++) {
        check_read_write(o, &fails, cases[i].label, cases[i].json, cases[i].rflg, cases[i].wflg);
    }

    /* Malformed inputs */
    static const char *bad[] = {
        "", "{", "[", "}", "{\"a\"}", "{\"a\":}", "[1,]", "[1 2]", "\"unterminated",
        "\"bad\\u12G4\"", "\"lone\\ud800\"", "[01]", "[1.]", "[+1]", "{\"a\":1} trailing"
    };
    for (unsigned i = 0; i < sizeof bad / sizeof *bad; i++) {
        char label[32];
        snprintf(label, sizeof label, "bad-%02u", i);
        check_read_write(o, &fails, label, bad[i], 0, 0);
    }

    /* JSON Pointer lookup with ~0 and ~1 escapes */
    {
        const char *ptr_doc = "{\"a/b\":{\"c~d\":[10,20,{\"x\":true,\"y\":null,\"z\":\"ok\"}]},\"\":\"empty-key\"}";
        yyjson_doc *doc = yyjson_read(ptr_doc, strlen(ptr_doc), 0);
        yyjson_val *root = yyjson_doc_get_root(doc);
        yyjson_val *v1 = yyjson_ptr_get(root, "/a~1b/c~0d/2/z");
        yyjson_val *v2 = yyjson_ptr_get(root, "/a~1b/c~0d/1");
        yyjson_val *v3 = yyjson_ptr_get(root, "/");
        yyjson_val *v4 = yyjson_ptr_get(root, "/missing/path");
        fprintf(o, "ptr v1=%s v2=%lld v3=%s v4_null=%d\n",
                v1 ? yyjson_get_str(v1) : "NULL",
                v2 ? (long long)yyjson_get_sint(v2) : -1LL,
                v3 ? yyjson_get_str(v3) : "NULL",
                v4 == NULL ? 1 : 0);
        if (!v1 || !v2 || !v3 || v4) fails++;
        yyjson_doc_free(doc);
    }

    /* Mutable doc building, rotation, key removal, pointer set */
    {
        yyjson_mut_doc *mdoc = yyjson_mut_doc_new(NULL);
        yyjson_mut_val *root = yyjson_mut_obj(mdoc);
        yyjson_mut_doc_set_root(mdoc, root);
        yyjson_mut_obj_add_str(mdoc, root, "project", "ounce-core");
        yyjson_mut_obj_add_int(mdoc, root, "stars", 12345);
        yyjson_mut_obj_add_real(mdoc, root, "ratio", 0.625);
        yyjson_mut_obj_add_bool(mdoc, root, "active", true);
        yyjson_mut_obj_add_null(mdoc, root, "extra");
        yyjson_mut_val *arr = yyjson_mut_arr(mdoc);
        yyjson_mut_obj_add_val(mdoc, root, "items", arr);
        for (int i = 0; i < 6; i++) yyjson_mut_arr_add_int(mdoc, arr, i * 11);
        yyjson_mut_arr_rotate(arr, 2);
        yyjson_mut_obj_remove_key(root, "extra");
        yyjson_mut_ptr_set(root, "/items/1", yyjson_mut_str(mdoc, "rotated"), mdoc);

        size_t len = 0;
        char *out = yyjson_mut_write(mdoc, YYJSON_WRITE_PRETTY_TWO_SPACES, &len);
        fprintf(o, "[mut-doc] len=%zu hash=%016llx\n%s\n", len, (unsigned long long)fnv1a(out, len), out);
        free(out);
        yyjson_mut_doc_free(mdoc);
    }

    /* RFC 6902 JSON Patch & RFC 7386 JSON Merge Patch */
    {
        const char *orig_s = "{\"a\":\"foo\",\"b\":[1,2,3],\"nested\":{\"keep\":true,\"drop\":9}}";
        const char *patch_s = "["
            "{\"op\":\"test\",\"path\":\"/a\",\"value\":\"foo\"},"
            "{\"op\":\"replace\",\"path\":\"/a\",\"value\":\"bar\"},"
            "{\"op\":\"add\",\"path\":\"/b/1\",\"value\":99},"
            "{\"op\":\"remove\",\"path\":\"/nested/drop\"},"
            "{\"op\":\"copy\",\"from\":\"/nested/keep\",\"path\":\"/copied\"},"
            "{\"op\":\"move\",\"from\":\"/b/3\",\"path\":\"/moved\"}"
        "]";
        const char *merge_s = "{\"a\":\"merged\",\"nested\":{\"keep\":null,\"added\":42}}";
        yyjson_doc *orig = yyjson_read(orig_s, strlen(orig_s), 0);
        yyjson_doc *patch = yyjson_read(patch_s, strlen(patch_s), 0);
        yyjson_doc *merge = yyjson_read(merge_s, strlen(merge_s), 0);
        yyjson_mut_doc *work = yyjson_mut_doc_new(NULL);
        yyjson_patch_err perr;
        yyjson_mut_val *patched = yyjson_patch(work, yyjson_doc_get_root(orig), yyjson_doc_get_root(patch), &perr);
        if (!patched) {
            fprintf(o, "patch ERR code=%u\n", (unsigned)perr.code);
            fails++;
        } else {
            size_t len = 0;
            char *out = yyjson_mut_val_write_opts(patched, 0, NULL, &len, NULL);
            fprintf(o, "[rfc6902] len=%zu hash=%016llx\n%s\n", len, (unsigned long long)fnv1a(out, len), out);
            free(out);
        }
        yyjson_mut_val *merged = yyjson_merge_patch(work, yyjson_doc_get_root(orig), yyjson_doc_get_root(merge));
        if (!merged) {
            fprintf(o, "merge ERR\n");
            fails++;
        } else {
            size_t len = 0;
            char *out = yyjson_mut_val_write_opts(merged, 0, NULL, &len, NULL);
            fprintf(o, "[rfc7386] len=%zu hash=%016llx\n%s\n", len, (unsigned long long)fnv1a(out, len), out);
            free(out);
        }
        yyjson_mut_doc_free(work);
        yyjson_doc_free(merge);
        yyjson_doc_free(patch);
        yyjson_doc_free(orig);
    }

    /* Large generated array (1000 doubles & strings) to stress Dragonbox & SWAR */
    {
        yyjson_mut_doc *mdoc = yyjson_mut_doc_new(NULL);
        yyjson_mut_val *arr = yyjson_mut_arr(mdoc);
        yyjson_mut_doc_set_root(mdoc, arr);
        for (int i = 0; i < 1000; i++) {
            double v = (double)(i - 500) * 0.137125;
            yyjson_mut_arr_add_real(mdoc, arr, v);
        }
        size_t len = 0;
        char *out = yyjson_mut_write(mdoc, 0, &len);
        yyjson_doc *reparsed = yyjson_read(out, len, 0);
        if (!reparsed) fails++;
        fprintf(o, "[big-arr] count=%zu len=%zu hash=%016llx reparse=%d\n",
                reparsed ? yyjson_arr_size(yyjson_doc_get_root(reparsed)) : 0,
                len, (unsigned long long)fnv1a(out, len), reparsed != NULL);
        yyjson_doc_free(reparsed);
        free(out);
        yyjson_mut_doc_free(mdoc);
    }

    fprintf(o, "TOTAL FAILS=%d\n", fails);
    return fails;
}
