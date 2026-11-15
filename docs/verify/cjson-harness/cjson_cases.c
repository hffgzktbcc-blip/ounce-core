/* cJSON differential cases. Deterministic text only. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "cJSON.h"

static FILE *O;

static void show(const char *label, cJSON *j) {
  char *f = cJSON_Print(j), *u = cJSON_PrintUnformatted(j);
  fprintf(O, "[%s]\n%s\n%s\n", label, f ? f : "(null)", u ? u : "(null)");
  cJSON_free(f); cJSON_free(u);
}

static void parse_case(const char *label, const char *txt) {
  const char *end = NULL;
  cJSON *j = cJSON_ParseWithOpts(txt, &end, 1);
  if (!j) {
    const char *ep = cJSON_GetErrorPtr();
    fprintf(O, "[%s] ERROR at offset %ld\n", label, ep ? (long)(ep - txt) : -1L);
    return;
  }
  show(label, j);
  /* round trip */
  char *u = cJSON_PrintUnformatted(j);
  cJSON *j2 = cJSON_Parse(u);
  char *u2 = j2 ? cJSON_PrintUnformatted(j2) : NULL;
  fprintf(O, "roundtrip %s compare=%d\n", (u2 && !strcmp(u, u2)) ? "same" : "DIFF",
          j2 ? (int)cJSON_Compare(j, j2, 1) : -1);
  cJSON_free(u); cJSON_free(u2); cJSON_Delete(j2); cJSON_Delete(j);
}

static void walk(cJSON *j, int depth) {
  for (cJSON *c = j; c; c = c->next) {
    fprintf(O, "%*s%s type=%d", depth * 2, "", c->string ? c->string : "-", c->type & 0xff);
    if (cJSON_IsNumber(c)) fprintf(O, " int=%d dbl=%.17g", c->valueint, c->valuedouble);
    if (cJSON_IsString(c)) fprintf(O, " str=\"%s\" len=%d", c->valuestring, (int)strlen(c->valuestring));
    fprintf(O, "\n");
    if (c->child) walk(c->child, depth + 1);
  }
}

int cjson_run_cases(FILE *o) {
  O = o;
  fprintf(O, "cJSON %s\n", cJSON_Version());

  parse_case("empty-obj", "{}");
  parse_case("empty-arr", "[]");
  parse_case("scalars", "[true,false,null,0,-1,42,\"x\"]");
  parse_case("nested", "{\"a\":{\"b\":{\"c\":[1,2,{\"d\":[[],[[]],{}]}]}},\"e\":\"f\"}");
  parse_case("whitespace", " \t\r\n{ \"k\" :\n [ 1 ,\t2 ] } \n");
  parse_case("unicode-esc", "[\"\\u0041\\u00e9\\u20ac\\ud83d\\ude00\\n\\t\\\"\\\\\\/\\b\\f\\r\"]");
  parse_case("utf8-raw", "[\"h\xc3\xa9llo \xe2\x82\xac \xf0\x9f\x98\x80\"]");
  parse_case("ctrl-print", "[\"\\u0001\\u001f\\u007f\"]");
  parse_case("numbers", "[0,-0,1,-1,123456789,2147483647,2147483648,-2147483648,-2147483649,"
                        "9007199254740993,18446744073709551615,12345678901234567890123]");
  parse_case("doubles", "[0.1,0.5,-0.25,3.141592653589793,1e10,1E-10,1.5e+3,2.5e-3,123.456e2,"
                        "1e308,-1e308,1.7976931348623157e308,2.2250738585072014e-308,5e-324,1e-400]");
  parse_case("neg-zero", "[-0,-0.0,0.0,-0e0]");
  parse_case("big-exp", "[1e309,-1e309]");
  parse_case("dup-keys", "{\"a\":1,\"a\":2,\"b\":3,\"a\":4}");
  parse_case("long-string", "[\"0123456789012345678901234567890123456789012345678901234567890123456789"
                            "01234567890123456789012345678901234567890123456789\"]");

  /* malformed */
  const char *bad[] = {
    "", "{", "[", "}", "]", "{\"a\"}", "{\"a\":}", "{\"a\":1,}", "[1,]", "[1 2]", "{a:1}",
    "'x'", "\"unterminated", "[\"bad\\x\"]", "[\"\\u12\"]", "[\"\\ud800\"]", "[\"\\udc00\"]",
    "tru", "nul", "fals", "NaN", "[NaN]", "[Infinity]", "[-Infinity]", "[+1]", "[.5]", "[01]",
    "[1.]", "[1e]", "[--1]", "{\"a\":1} x", "[1]]", "{\"a\" 1}", "[\"a\"\"b\"]", "\x01", "[\"\x01\"]"
  };
  for (unsigned i = 0; i < sizeof bad / sizeof *bad; i++) {
    char lab[32]; snprintf(lab, sizeof lab, "bad-%02u", i);
    parse_case(lab, bad[i]);
  }

  /* walk a parsed document */
  {
    cJSON *j = cJSON_Parse("{\"i\":7,\"d\":-2.5,\"big\":1e20,\"neg\":-1e20,\"s\":\"hi\",\"arr\":[1,[2,[3]]],\"o\":{\"x\":null}}");
    walk(j, 0);
    cJSON_Delete(j);
  }

  /* deep nesting near the limit (CJSON_NESTING_LIMIT 1000) */
  {
    int depths[] = {1, 100, 500, 999, 1000, 1001, 2000};
    for (unsigned k = 0; k < sizeof depths / sizeof *depths; k++) {
      int d = depths[k];
      char *s = malloc(2 * (size_t)d + 8);
      memset(s, '[', d); memset(s + d, ']', d); s[2 * d] = 0;
      cJSON *j = cJSON_Parse(s);
      if (!j) fprintf(O, "deep-arr %d ERROR at offset %ld\n", d, (long)(cJSON_GetErrorPtr() - s));
      else {
        char *u = cJSON_PrintUnformatted(j);
        fprintf(O, "deep-arr %d ok printed=%d\n", d, u ? (int)strlen(u) : -1);
        cJSON_free(u); cJSON_Delete(j);
      }
      free(s);
    }
    /* deep objects */
    int d = 300;
    size_t n = (size_t)d * 6 + 8; char *s = malloc(n), *p = s;
    for (int i = 0; i < d; i++) { memcpy(p, "{\"k\":", 5); p += 5; }
    *p++ = '1';
    for (int i = 0; i < d; i++) *p++ = '}';
    *p = 0;
    cJSON *j = cJSON_Parse(s);
    char *u = j ? cJSON_PrintUnformatted(j) : NULL;
    fprintf(O, "deep-obj %d %s len=%d same=%d\n", d, j ? "ok" : "ERROR", u ? (int)strlen(u) : -1, u ? !strcmp(u, s) : 0);
    cJSON_free(u); cJSON_Delete(j); free(s);
  }

  /* create via API */
  {
    cJSON *r = cJSON_CreateObject();
    cJSON_AddStringToObject(r, "name", "ounce");
    cJSON_AddNumberToObject(r, "int", 42);
    cJSON_AddNumberToObject(r, "neg", -7);
    cJSON_AddNumberToObject(r, "pi", 3.14159265358979323846);
    cJSON_AddNumberToObject(r, "big", 1e308);
    cJSON_AddNumberToObject(r, "tiny", 5e-324);
    cJSON_AddNumberToObject(r, "negzero", -0.0);
    cJSON_AddNumberToObject(r, "third", 1.0 / 3.0);
    cJSON_AddNumberToObject(r, "hundredth", 0.01);
    cJSON_AddNumberToObject(r, "i64", 9007199254740992.0);
    cJSON_AddNumberToObject(r, "nan", __builtin_nan(""));
    cJSON_AddNumberToObject(r, "inf", __builtin_inf());
    cJSON_AddNumberToObject(r, "ninf", -__builtin_inf());
    cJSON_AddTrueToObject(r, "t");
    cJSON_AddFalseToObject(r, "f");
    cJSON_AddNullToObject(r, "n");
    cJSON_AddBoolToObject(r, "b", 1);
    cJSON *a = cJSON_AddArrayToObject(r, "arr");
    for (int i = 0; i < 5; i++) cJSON_AddItemToArray(a, cJSON_CreateNumber(i * i * 1.5));
    cJSON_AddItemToArray(a, cJSON_CreateString("tail\n\"q\""));
    int ia[] = {3, 1, 4, 1, 5, 9};
    cJSON_AddItemToObject(r, "ints", cJSON_CreateIntArray(ia, 6));
    double da[] = {0.1, 0.2, 0.30000000000000004};
    cJSON_AddItemToObject(r, "dbls", cJSON_CreateDoubleArray(da, 3));
    const char *sa[] = {"a", "bb", "ccc"};
    cJSON_AddItemToObject(r, "strs", cJSON_CreateStringArray(sa, 3));
    cJSON *o = cJSON_AddObjectToObject(r, "obj");
    cJSON_AddStringToObject(o, "k", "v");
    cJSON_AddRawToObject(r, "raw", "{\"already\":[1,2,3]}");
    show("created", r);
    fprintf(O, "array-size=%d obj-has-name=%d has-zzz=%d\n", cJSON_GetArraySize(a),
            cJSON_HasObjectItem(r, "name"), cJSON_HasObjectItem(r, "zzz"));

    /* modify */
    cJSON_ReplaceItemInObject(r, "name", cJSON_CreateString("replaced"));
    cJSON_ReplaceItemInObjectCaseSensitive(r, "int", cJSON_CreateNumber(1000000));
    cJSON_SetNumberHelper(cJSON_GetObjectItem(r, "neg"), 99.5);
    cJSON_SetValuestring(cJSON_GetObjectItem(r, "name"), "replaced-again-with-a-longer-string");
    cJSON_ReplaceItemInArray(a, 1, cJSON_CreateString("one"));
    cJSON_InsertItemInArray(a, 0, cJSON_CreateString("first"));
    cJSON_DeleteItemFromObject(r, "t");
    cJSON_DeleteItemFromObjectCaseSensitive(r, "f");
    cJSON_DeleteItemFromArray(a, 2);
    cJSON_Delete(cJSON_DetachItemFromObject(r, "nan"));
    cJSON_Delete(cJSON_DetachItemFromArray(a, 0));
    cJSON_AddItemReferenceToObject(r, "ref", o);
    show("modified", r);

    cJSON *dup = cJSON_Duplicate(r, 1);
    fprintf(O, "dup compare=%d\n", (int)cJSON_Compare(r, dup, 1));
    cJSON_DeleteItemFromObject(dup, "i64");
    fprintf(O, "dup after delete compare=%d\n", (int)cJSON_Compare(r, dup, 1));
    cJSON_Delete(dup);
    cJSON_DeleteItemFromObject(r, "ref");
    show("deleted-ref", r);
    cJSON_Delete(r);
  }

  /* duplicate keys through API */
  {
    cJSON *r = cJSON_CreateObject();
    cJSON_AddNumberToObject(r, "k", 1);
    cJSON_AddNumberToObject(r, "k", 2);
    cJSON_AddNumberToObject(r, "K", 3);
    fprintf(O, "dup-key get=%.17g case-sens=%.17g ci-K=%.17g\n",
            cJSON_GetObjectItem(r, "k")->valuedouble,
            cJSON_GetObjectItemCaseSensitive(r, "K")->valuedouble,
            cJSON_GetObjectItem(r, "K")->valuedouble);
    cJSON_DeleteItemFromObject(r, "k");
    show("dup-key-after-delete", r);
    cJSON_Delete(r);
  }

  /* number setters / saturation */
  {
    double v[] = {2147483647.0, 2147483648.0, -2147483648.0, -2147483649.0, 1e20, -1e20, 0.9999999, -0.9999999};
    for (unsigned i = 0; i < sizeof v / sizeof *v; i++) {
      cJSON *n = cJSON_CreateNumber(v[i]);
      char *u = cJSON_PrintUnformatted(n);
      fprintf(O, "num %.17g -> int=%d print=%s\n", v[i], n->valueint, u);
      cJSON_free(u); cJSON_Delete(n);
    }
  }

  /* formatted print with preallocated buffer */
  {
    cJSON *j = cJSON_Parse("{\"a\":[1,2,{\"b\":null}],\"c\":\"d\"}");
    char buf[256];
    fprintf(O, "prealloc fmt=%d\n", cJSON_PrintPreallocated(j, buf, sizeof buf, 1));
    fprintf(O, "%s\n", buf);
    fprintf(O, "prealloc tiny=%d\n", cJSON_PrintPreallocated(j, buf, 10, 0));
    char *b = cJSON_PrintBuffered(j, 4, 1);
    fprintf(O, "buffered:\n%s\n", b); cJSON_free(b);
    cJSON_Delete(j);
  }

  /* Minify */
  {
    char m[] = " { \"a\" : [ 1 , 2 ] , // c\n \"b\" : /* x */ \"s p\" } ";
    cJSON_Minify(m);
    fprintf(O, "minify: %s\n", m);
  }

  /* big array */
  {
    cJSON *a = cJSON_CreateArray();
    for (int i = 0; i < 2000; i++) cJSON_AddItemToArray(a, cJSON_CreateNumber((double)i * 0.37));
    char *u = cJSON_PrintUnformatted(a);
    unsigned h = 2166136261u;
    for (char *p = u; *p; p++) { h ^= (unsigned char)*p; h *= 16777619u; }
    fprintf(O, "big-array len=%d fnv=%08x\n", (int)strlen(u), h);
    cJSON *b = cJSON_Parse(u);
    fprintf(O, "big-array reparse compare=%d size=%d\n", (int)cJSON_Compare(a, b, 1), cJSON_GetArraySize(b));
    cJSON_free(u); cJSON_Delete(a); cJSON_Delete(b);
  }
  fprintf(O, "done\n");
  return 0;
}
