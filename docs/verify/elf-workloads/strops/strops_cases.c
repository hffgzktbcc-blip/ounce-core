/* elf-workload strops: libc string and number-conversion routines on fixed inputs.
 * Provenance: original work written for ounce-core (MIT); no third-party source. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

static int fails;
#define CHECK(c) do { if (!(c)) { fails++; fprintf(o, "FAIL line %d: %s\n", __LINE__, #c); } } while (0)

static int cmp_str(const void *a, const void *b) {
    return strcmp(*(char *const *)a, *(char *const *)b);
}

int workload_run(FILE *o) {
    const char *s = "the quick brown fox jumps over the lazy dog";
    char buf[128];
    size_t i;
    fprintf(o, "strlen=%zu\n", strlen(s));
    CHECK(strlen(s) == 43);
    fprintf(o, "strchr=%ld strrchr=%ld\n", (long)(strchr(s, 'q') - s), (long)(strrchr(s, 'o') - s));
    const char *p = strstr(s, "lazy");
    CHECK(p && p - s == 35);
    CHECK(strstr(s, "cat") == NULL);
    fprintf(o, "strspn=%zu strcspn=%zu\n", strspn(s, "ehtu "), strcspn(s, "xyz"));

    /* strtok_r over a delimiter-rich string */
    strcpy(buf, "alpha, beta;;gamma  delta,epsilon");
    char *save = NULL;
    int n = 0;
    for (char *t = strtok_r(buf, ",; ", &save); t; t = strtok_r(NULL, ",; ", &save)) {
        fprintf(o, "tok%d=%s\n", n++, t);
    }
    CHECK(n == 5);

    /* copy / concat / compare */
    strncpy(buf, "abcdefgh", 5);
    buf[5] = 0;
    strcat(buf, "XYZ");
    fprintf(o, "buf=%s cmp=%d %d %d\n", buf,
            strcmp("abc", "abd") < 0, strcmp("b", "a") > 0, strncmp("abcx", "abcy", 3));
    CHECK(memcmp("abc\0d", "abc\0e", 5) < 0);
    memset(buf, 'z', 8);
    buf[8] = 0;
    memmove(buf + 2, buf, 4);
    fprintf(o, "memset/memmove=%s\n", buf);
    char ov[16] = "0123456789";
    memmove(ov + 3, ov, 7);
    fprintf(o, "overlap=%s\n", ov);

    /* case mapping and ctype classes (C locale) */
    char up[32];
    for (i = 0; s[i] && i < 20; i++) up[i] = (char)toupper((unsigned char)s[i]);
    up[i] = 0;
    fprintf(o, "upper=%s\n", up);
    int cls[5] = {0, 0, 0, 0, 0};
    for (i = 0; i < 128; i++) {
        cls[0] += isalpha((int)i) != 0;
        cls[1] += isdigit((int)i) != 0;
        cls[2] += isspace((int)i) != 0;
        cls[3] += ispunct((int)i) != 0;
        cls[4] += isxdigit((int)i) != 0;
    }
    fprintf(o, "ctype alpha=%d digit=%d space=%d punct=%d xdigit=%d\n", cls[0], cls[1], cls[2], cls[3], cls[4]);
    CHECK(cls[0] == 52 && cls[1] == 10 && cls[2] == 6 && cls[3] == 32 && cls[4] == 22);

    /* integer conversions: bases, signs, end pointers, overflow clamp */
    const char *nums[] = {"42", "-17", "  +99xyz", "0x1F", "0755", "9223372036854775807",
                          "123456789012", "zz"};
    for (i = 0; i < sizeof nums / sizeof *nums; i++) {
        char *end;
        long long v = strtoll(nums[i], &end, 0);
        fprintf(o, "strtoll(\"%s\")=%lld rest=\"%s\"\n", nums[i], v, end);
    }
    fprintf(o, "base36 zz=%ld base2=%ld\n", strtol("zz", NULL, 36), strtol("101101", NULL, 2));
    fprintf(o, "atoi=%d abs=%d labs=%ld\n", atoi("  -123abc"), abs(-7), labs(-70000000000L));
    unsigned long long u = strtoull("18446744073709551615", NULL, 10);
    fprintf(o, "strtoull=%llu\n", u);
    CHECK(u == 0xFFFFFFFFFFFFFFFFull);

    /* snprintf truncation reports the full length */
    int need = snprintf(buf, 8, "%s-%d", "truncated", 12345);
    fprintf(o, "snprintf need=%d got=\"%s\"\n", need, buf);
    CHECK(need == 15);

    /* qsort of strings, then bsearch */
    char *sorted[] = {"pear", "apple", "fig", "banana", "cherry", "date", "elderberry", "grape"};
    qsort(sorted, 8, sizeof *sorted, cmp_str);
    for (i = 0; i < 8; i++) fprintf(o, "%s%c", sorted[i], i == 7 ? '\n' : ' ');
    char *key = "date";
    char **hit = bsearch(&key, sorted, 8, sizeof *sorted, cmp_str);
    CHECK(hit && hit - sorted == 3);
    fprintf(o, "bsearch date idx=%ld\n", hit ? (long)(hit - sorted) : -1L);

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
