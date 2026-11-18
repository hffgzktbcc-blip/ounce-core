/* elf-workload wordfreq: tokenise an embedded text, count word frequencies in an open-addressing
 * hash table, sort by (count desc, word asc) and print the top entries, plus line, word and
 * character statistics and a Caesar/ROT13 round trip.
 * Provenance: original work written for ounce-core (MIT); the embedded text is original prose. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

static const char text[] =
    "The lab runs small programs and watches what they do. A program that prints the same bytes\n"
    "twice, on two engines, is a program the lab can trust a little more. The lab does not trust\n"
    "a program because it is popular; it trusts a program because the bytes match, the exit code\n"
    "matches, and the instruction count matches on every engine the lab has. When the bytes do not\n"
    "match, the lab stops, writes down the first difference, and fixes the engine, not the program.\n"
    "\n"
    "Small programs are best. A small program has few places to hide. A sorting routine, a hash, a\n"
    "formatter, a parser: each one is a question the engine must answer the same way every time.\n";

typedef struct { char word[24]; unsigned count; } entry;
#define TAB 512

static unsigned hash_word(const char *w) {
    unsigned h = 2166136261u;
    while (*w) h = (h ^ (unsigned char)*w++) * 16777619u;
    return h;
}

static int by_count_then_word(const void *a, const void *b) {
    const entry *x = a, *y = b;
    if (x->count != y->count) return x->count > y->count ? -1 : 1;
    return strcmp(x->word, y->word);
}

static void rot(char *s, int k) {
    for (; *s; s++) {
        if (isupper((unsigned char)*s)) *s = (char)('A' + (*s - 'A' + k + 26) % 26);
        else if (islower((unsigned char)*s)) *s = (char)('a' + (*s - 'a' + k + 26) % 26);
    }
}

int workload_run(FILE *o) {
    static entry tab[TAB];
    int fails = 0;
    unsigned words = 0, lines = 0, chars = 0, letters = 0, longest = 0;
    char cur[24];
    size_t len = 0;
    for (const char *p = text;; p++) {
        chars++;
        if (*p == '\n') lines++;
        if (isalpha((unsigned char)*p)) {
            letters++;
            if (len < sizeof cur - 1) cur[len++] = (char)tolower((unsigned char)*p);
        } else {
            if (len) {
                cur[len] = 0;
                unsigned i = hash_word(cur) % TAB;
                while (tab[i].count && strcmp(tab[i].word, cur)) i = (i + 1) % TAB;
                if (!tab[i].count) strcpy(tab[i].word, cur);
                tab[i].count++;
                words++;
                if (len > longest) longest = (unsigned)len;
                len = 0;
            }
            if (!*p) break;
        }
    }
    entry packed[TAB];
    unsigned distinct = 0;
    for (int i = 0; i < TAB; i++) if (tab[i].count) packed[distinct++] = tab[i];
    qsort(packed, distinct, sizeof *packed, by_count_then_word);
    fprintf(o, "chars=%u lines=%u words=%u letters=%u distinct=%u longest=%u\n", chars - 1, lines, words, letters,
            distinct, longest);
    unsigned total = 0;
    for (unsigned i = 0; i < distinct; i++) total += packed[i].count;
    if (total != words) fails++;
    for (unsigned i = 0; i < distinct && i < 12; i++) fprintf(o, "%2u. %-8s %3u\n", i + 1, packed[i].word, packed[i].count);
    unsigned hapax = 0;
    for (unsigned i = 0; i < distinct; i++) hapax += packed[i].count == 1;
    fprintf(o, "hapax legomena: %u\n", hapax);

    /* histogram of word lengths */
    unsigned hist[16] = {0};
    for (unsigned i = 0; i < distinct; i++) hist[strlen(packed[i].word) < 15 ? strlen(packed[i].word) : 15] += packed[i].count;
    for (int l = 1; l < 16; l++) if (hist[l]) fprintf(o, "len %2d: %3u %.*s\n", l, hist[l], (int)(hist[l] / 4), "##########################################");

    char s[64] = "Hello, World! The quick brown fox.";
    char orig[64];
    strcpy(orig, s);
    rot(s, 13);
    fprintf(o, "rot13: %s\n", s);
    rot(s, 13);
    fprintf(o, "twice: %s same=%d\n", s, !strcmp(s, orig));
    if (strcmp(s, orig)) fails++;
    rot(s, 3);
    rot(s, -3);
    if (strcmp(s, orig)) fails++;

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
